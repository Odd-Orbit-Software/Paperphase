//! Pure refresh decisions. The profile below describes only the software simulator.
use std::time::Duration;

use anyhow::Result;
use paperphase_core::{Layout, Rect, RefreshHint, RefreshPolicy};
use paperphase_render::Frame;
use serde::Serialize;
use tokio::time::Instant;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Profile {
    pub id: &'static str,
    pub partial: bool,
    pub align_x: u32,
    pub align_y: u32,
    pub min_interval_ms: u64,
    pub max_partials: u32,
    pub max_full_age_secs: u64,
    pub max_partial_area_percent: u32,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: "software-simulator",
            partial: true,
            align_x: 8,
            align_y: 1,
            min_interval_ms: 250,
            max_partials: 20,
            max_full_age_secs: 7200,
            max_partial_area_percent: 60,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Partial,
    Full,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reason {
    ChangedArea,
    PartialUnsupported,
    UnknownBaseline,
    ComponentRequest,
    PartialLimit,
    MaintenanceDue,
    LargeArea,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Plan {
    pub mode: Mode,
    pub reason: Reason,
    pub region: Rect,
    pub dirty: Option<Rect>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Operation {
    #[serde(flatten)]
    pub plan: Plan,
    pub revision: u64,
}

pub(crate) struct Controller {
    pub profile: Profile,
    pub baseline_known: bool,
    pub partials: u32,
    pub last_full: Instant,
    pub last_completed: Option<Instant>,
    pub retry_at: Option<Instant>,
    pub failures: u32,
    pub active: Option<Operation>,
    pub completed: Option<Operation>,
    // Deterministic driver fault injection, reachable only from Rust tests.
    #[cfg(test)]
    pub fail_next: bool,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            profile: Profile::default(),
            baseline_known: true,
            partials: 0,
            last_full: Instant::now(),
            last_completed: None,
            retry_at: None,
            failures: 0,
            active: None,
            completed: None,
            #[cfg(test)]
            fail_next: false,
        }
    }

    pub fn ready_at(&self, policy: &RefreshPolicy, now: Instant) -> Instant {
        let spacing =
            Duration::from_millis(policy.min_interval_ms.max(self.profile.min_interval_ms));
        self.last_completed
            .map_or(now, |last| last + spacing)
            .max(self.retry_at.unwrap_or(now))
    }

    pub fn plan(
        &self,
        previous: &Frame,
        desired: &Frame,
        old: &Layout,
        new: &Layout,
        policy: &RefreshPolicy,
        now: Instant,
    ) -> Result<Option<Plan>> {
        let dirty = desired.difference_bounds(previous)?;
        if dirty.is_none() && self.baseline_known {
            return Ok(None);
        }
        let full = Rect {
            x: 0,
            y: 0,
            width: desired.width(),
            height: desired.height(),
        };
        let dirty_region = dirty.unwrap_or(full);
        let aligned = Rect {
            x: dirty_region.x / self.profile.align_x * self.profile.align_x,
            y: dirty_region.y / self.profile.align_y * self.profile.align_y,
            width: 0,
            height: 0,
        };
        let aligned = Rect {
            width: ((dirty_region.right() as u32).div_ceil(self.profile.align_x)
                * self.profile.align_x)
                .min(full.width)
                - aligned.x,
            height: ((dirty_region.bottom() as u32).div_ceil(self.profile.align_y)
                * self.profile.align_y)
                .min(full.height)
                - aligned.y,
            ..aligned
        };
        let reason = if !self.baseline_known {
            Some(Reason::UnknownBaseline)
        } else if !self.profile.partial {
            Some(Reason::PartialUnsupported)
        } else if new
            .components
            .iter()
            .any(|c| c.refresh_hint == RefreshHint::Full && c.rect.intersects(dirty_region))
            || old.components.iter().any(|c| {
                c.refresh_hint == RefreshHint::Full
                    && c.rect.intersects(dirty_region)
                    && !new
                        .components
                        .iter()
                        .any(|next| next.id == c.id && next.rect == c.rect)
            })
        {
            Some(Reason::ComponentRequest)
        } else if self.partials >= policy.full_after_partials.min(self.profile.max_partials) {
            Some(Reason::PartialLimit)
        } else if now.saturating_duration_since(self.last_full).as_secs()
            >= policy.full_after_secs.min(self.profile.max_full_age_secs)
        {
            Some(Reason::MaintenanceDue)
        } else if u64::from(aligned.width) * u64::from(aligned.height) * 100
            > u64::from(full.width)
                * u64::from(full.height)
                * u64::from(
                    policy
                        .max_partial_area_percent
                        .min(self.profile.max_partial_area_percent),
                )
        {
            Some(Reason::LargeArea)
        } else {
            None
        };
        Ok(Some(Plan {
            mode: if reason.is_some() {
                Mode::Full
            } else {
                Mode::Partial
            },
            reason: reason.unwrap_or(Reason::ChangedArea),
            region: if reason.is_some() { full } else { aligned },
            dirty,
        }))
    }

    pub fn complete(&mut self, operation: Operation, now: Instant) {
        if operation.plan.mode == Mode::Full {
            self.partials = 0;
            self.last_full = now;
        } else {
            self.partials = self.partials.saturating_add(1);
        }
        self.baseline_known = true;
        self.last_completed = Some(now);
        self.retry_at = None;
        self.failures = 0;
        self.active = None;
        self.completed = Some(operation);
    }

    pub fn failed(&mut self, now: Instant) {
        self.baseline_known = false;
        self.active = None;
        self.failures = self.failures.saturating_add(1);
        self.retry_at =
            Some(now + Duration::from_secs(1 << self.failures.saturating_sub(1).min(5)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paperphase_core::SnapshotSet;
    use paperphase_render::Renderer;
    use serde_json::json;

    fn scene(text: &str) -> Layout {
        serde_json::from_value(
            json!({"schema_version":1,"display":{"id":"test","width":256,"height":128},
            "components":[{"id":"a","rect":{"x":3,"y":3,"width":128,"height":64},"padding":0,
                "blocks":[{"id":"text","font_size":20,"source":{"kind":"literal","text":text}}]}]}),
        )
        .unwrap()
    }
    fn frame(layout: &Layout) -> Frame {
        Renderer::new()
            .unwrap()
            .render(
                layout,
                &SnapshotSet {
                    schema_version: 1,
                    snapshots: vec![],
                },
                100,
            )
            .unwrap()
            .frame
    }
    #[test]
    fn changed_pixels_are_aligned_outwards_and_unchanged_frames_need_no_operation() {
        let controller = Controller::new();
        let old = scene("A");
        let new = scene("B");
        let before = frame(&old);
        let after = frame(&new);
        assert!(
            controller
                .plan(
                    &before,
                    &before,
                    &old,
                    &old,
                    &RefreshPolicy::default(),
                    Instant::now()
                )
                .unwrap()
                .is_none()
        );
        let plan = controller
            .plan(
                &before,
                &after,
                &old,
                &new,
                &RefreshPolicy::default(),
                Instant::now(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(plan.mode, Mode::Partial);
        assert_eq!(plan.region.x % 8, 0);
        assert_eq!(plan.region.width % 8, 0);
        let dirty = plan.dirty.unwrap();
        assert!(plan.region.x <= dirty.x && plan.region.right() >= dirty.right());
        assert!(plan.region.y <= dirty.y && plan.region.bottom() >= dirty.bottom());
        assert!(plan.region.right() <= 256 && plan.region.bottom() <= 128);
        let mut edge = scene("");
        edge.display.width = 255;
        edge.display.height = 127;
        edge.components[0].rect = Rect {
            x: 223,
            y: 95,
            width: 32,
            height: 32,
        };
        edge.components[0].border = true;
        let mut blank = edge.clone();
        blank.components.clear();
        let plan = controller
            .plan(
                &frame(&blank),
                &frame(&edge),
                &blank,
                &edge,
                &RefreshPolicy::default(),
                Instant::now(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(plan.mode, Mode::Partial);
        assert_eq!(plan.region.right(), 255);
        assert_eq!(plan.region.bottom(), 127);
    }
    #[test]
    fn moved_and_removed_content_invalidates_the_old_pixels() {
        let controller = Controller::new();
        let old = scene("A");
        let mut new = old.clone();
        new.components[0].rect.x = 128;
        let plan = controller
            .plan(
                &frame(&old),
                &frame(&new),
                &old,
                &new,
                &RefreshPolicy::default(),
                Instant::now(),
            )
            .unwrap()
            .unwrap();
        assert!(plan.region.x < 32 && plan.region.right() > 128);
        new.components.clear();
        let removed = controller
            .plan(
                &frame(&old),
                &frame(&new),
                &old,
                &new,
                &RefreshPolicy::default(),
                Instant::now(),
            )
            .unwrap()
            .unwrap();
        assert!(removed.dirty.is_some());
    }
    #[test]
    fn host_rules_override_partial_requests_and_full_requests_are_honoured() {
        let mut controller = Controller::new();
        let old = scene("A");
        let mut new = scene("B");
        new.components[0].refresh_hint = RefreshHint::Partial;
        let before = frame(&old);
        let after = frame(&new);
        let policy = RefreshPolicy::default();
        let mut previously_full = old.clone();
        previously_full.components[0].refresh_hint = RefreshHint::Full;
        assert_eq!(
            controller
                .plan(
                    &before,
                    &after,
                    &previously_full,
                    &new,
                    &policy,
                    Instant::now()
                )
                .unwrap()
                .unwrap()
                .mode,
            Mode::Partial
        );
        controller.profile.partial = false;
        assert_eq!(
            controller
                .plan(&before, &after, &old, &new, &policy, Instant::now())
                .unwrap()
                .unwrap()
                .reason,
            Reason::PartialUnsupported
        );
        controller.profile.partial = true;
        controller.baseline_known = false;
        let recovery = controller
            .plan(&before, &before, &old, &old, &policy, Instant::now())
            .unwrap()
            .unwrap();
        assert_eq!(recovery.reason, Reason::UnknownBaseline);
        assert_eq!(recovery.region.width, 256);
        controller.baseline_known = true;
        new.components[0].refresh_hint = RefreshHint::Full;
        assert_eq!(
            controller
                .plan(&before, &after, &old, &new, &policy, Instant::now())
                .unwrap()
                .unwrap()
                .reason,
            Reason::ComponentRequest
        );
    }
    #[test]
    fn count_age_area_and_profile_caps_promote_partial_updates() {
        let mut controller = Controller::new();
        let old = scene("A");
        let new = scene("B");
        let before = frame(&old);
        let after = frame(&new);
        let now = Instant::now();
        let mut policy = RefreshPolicy {
            full_after_partials: 2,
            ..RefreshPolicy::default()
        };
        for count in 0..=2 {
            let plan = controller
                .plan(&before, &after, &old, &new, &policy, now)
                .unwrap()
                .unwrap();
            assert_eq!(
                plan.mode,
                if count == 2 {
                    Mode::Full
                } else {
                    Mode::Partial
                }
            );
            controller.complete(
                Operation {
                    plan,
                    revision: count as u64,
                },
                now,
            );
        }
        assert_eq!(controller.partials, 0);
        policy.full_after_secs = 1;
        assert_eq!(
            controller
                .plan(
                    &before,
                    &after,
                    &old,
                    &new,
                    &policy,
                    now + Duration::from_secs(2)
                )
                .unwrap()
                .unwrap()
                .reason,
            Reason::MaintenanceDue
        );
        policy.full_after_secs = 604800;
        policy.full_after_partials = 1000;
        controller.partials = controller.profile.max_partials;
        assert_eq!(
            controller
                .plan(&before, &after, &old, &new, &policy, now)
                .unwrap()
                .unwrap()
                .reason,
            Reason::PartialLimit
        );
        controller.partials = 0;
        policy.max_partial_area_percent = 1;
        let wide = scene("A larger area");
        assert_eq!(
            controller
                .plan(&before, &frame(&wide), &old, &wide, &policy, now)
                .unwrap()
                .unwrap()
                .reason,
            Reason::LargeArea
        );
        policy.max_partial_area_percent = 100;
        controller.profile.max_partial_area_percent = 1;
        assert_eq!(
            controller
                .plan(&before, &frame(&wide), &old, &wide, &policy, now)
                .unwrap()
                .unwrap()
                .reason,
            Reason::LargeArea
        );
    }
    #[test]
    fn spacing_and_failure_backoff_use_monotonic_time_and_success_resets_failures() {
        let mut controller = Controller::new();
        let now = Instant::now();
        let policy = RefreshPolicy {
            min_interval_ms: 0,
            ..RefreshPolicy::default()
        };
        controller.last_completed = Some(now);
        assert_eq!(
            controller.ready_at(&policy, now),
            now + Duration::from_millis(250)
        );
        for failure in 0..8 {
            controller.failed(now);
            assert!(!controller.baseline_known);
            assert_eq!(
                controller.ready_at(&policy, now),
                now + Duration::from_secs(1 << failure.min(5))
            );
        }
        let scene = scene("A");
        let plan = controller
            .plan(&frame(&scene), &frame(&scene), &scene, &scene, &policy, now)
            .unwrap()
            .unwrap();
        controller.complete(Operation { plan, revision: 1 }, now);
        assert!(controller.baseline_known);
        assert_eq!(controller.failures, 0);
        assert!(controller.retry_at.is_none());
    }
}
