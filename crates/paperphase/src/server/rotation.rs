//! Presentation deadlines belong to acknowledged images, never provider polling.
use std::{collections::BTreeMap, time::Duration};

use paperphase_core::{Layout, Rotation};
use serde::Serialize;
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct Runtime {
    shown: BTreeMap<String, Shown>,
    epoch: u64,
}

struct Shown {
    member: String,
    since: Instant,
    paused: bool,
}

#[derive(Serialize)]
pub(super) struct Status {
    component_id: String,
    shown_member: Option<String>,
    candidate_member: String,
    paused: bool,
    count: usize,
    next_in_ms: Option<u128>,
}

fn first(rotation: &Rotation) -> &str {
    rotation
        .paused_member
        .as_deref()
        .unwrap_or(&rotation.members[0].id)
}

impl Runtime {
    #[cfg(test)]
    pub fn age_for_test(&mut self, seconds: u64) {
        for shown in self.shown.values_mut() {
            shown.since -= Duration::from_secs(seconds);
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn selection(&self, layout: &Layout, now: Instant) -> BTreeMap<String, String> {
        layout
            .components
            .iter()
            .filter_map(|component| {
                let rotation = component.rotation.as_ref()?;
                let shown = self.shown.get(&component.id);
                let index = shown.and_then(|shown| {
                    rotation
                        .members
                        .iter()
                        .position(|member| member.id == shown.member)
                });
                let member = if rotation.paused {
                    rotation
                        .paused_member
                        .as_deref()
                        .or_else(|| index.map(|index| rotation.members[index].id.as_str()))
                        .unwrap_or_else(|| first(rotation))
                } else if let (Some(shown), Some(index)) = (shown, index) {
                    if !shown.paused
                        && rotation.members.len() > 1
                        && now
                            >= shown.since + Duration::from_secs(u64::from(rotation.interval_secs))
                    {
                        &rotation.members[(index + 1) % rotation.members.len()].id
                    } else {
                        &rotation.members[index].id
                    }
                } else {
                    first(rotation)
                };
                Some((component.id.clone(), member.to_owned()))
            })
            .collect()
    }

    pub fn due(&self, layout: &Layout, now: Instant) -> bool {
        self.selection(layout, now).iter().any(|(area, member)| {
            let paused = layout
                .components
                .iter()
                .find(|c| &c.id == area)
                .and_then(|c| c.rotation.as_ref())
                .is_some_and(|r| r.paused);
            self.shown
                .get(area)
                .is_none_or(|shown| &shown.member != member || shown.paused != paused)
        })
    }

    /// Also used for a logical transition whose pixels match the completed image.
    pub fn acknowledge(
        &mut self,
        layout: &Layout,
        selected: &BTreeMap<String, String>,
        now: Instant,
    ) {
        for component in &layout.components {
            let Some(rotation) = &component.rotation else {
                continue;
            };
            let Some(member) = selected.get(&component.id) else {
                continue;
            };
            match self.shown.get_mut(&component.id) {
                Some(shown) => {
                    let reset = shown.member != *member || (shown.paused && !rotation.paused);
                    let changed = reset || shown.paused != rotation.paused;
                    if reset {
                        shown.since = now;
                    }
                    shown.member = member.clone();
                    shown.paused = rotation.paused;
                    if changed {
                        self.epoch = self.epoch.wrapping_add(1);
                    }
                }
                None => {
                    self.shown.insert(
                        component.id.clone(),
                        Shown {
                            member: member.clone(),
                            since: now,
                            paused: rotation.paused,
                        },
                    );
                    self.epoch = self.epoch.wrapping_add(1);
                }
            }
        }
    }

    pub fn retain(&mut self, layout: &Layout) {
        let before = self.shown.len();
        self.shown.retain(|id, _| {
            layout
                .components
                .iter()
                .any(|c| &c.id == id && c.rotation.is_some())
        });
        if before != self.shown.len() {
            self.epoch = self.epoch.wrapping_add(1);
        }
    }

    pub fn status(&self, layout: &Layout, now: Instant) -> Vec<Status> {
        let selection = self.selection(layout, now);
        layout
            .components
            .iter()
            .filter_map(|component| {
                let rotation = component.rotation.as_ref()?;
                let shown = self.shown.get(&component.id);
                let next_in_ms = shown
                    .filter(|shown| {
                        !rotation.paused
                            && !shown.paused
                            && rotation.members.len() > 1
                            && rotation
                                .members
                                .iter()
                                .any(|member| member.id == shown.member)
                    })
                    .map(|shown| {
                        (shown.since + Duration::from_secs(u64::from(rotation.interval_secs)))
                            .saturating_duration_since(now)
                            .as_millis()
                    });
                Some(Status {
                    component_id: component.id.clone(),
                    shown_member: shown.map(|s| s.member.clone()),
                    candidate_member: selection[&component.id].clone(),
                    paused: rotation.paused,
                    count: rotation.members.len(),
                    next_in_ms,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Layout {
        serde_json::from_str(include_str!("../../../../examples/simulator/rotation.json")).unwrap()
    }
    fn choice(runtime: &Runtime, layout: &Layout, at: Instant) -> String {
        runtime.selection(layout, at)["news"].clone()
    }

    #[test]
    fn dwell_begins_on_ack_and_other_updates_never_restart_it() {
        let layout = scene();
        let now = Instant::now();
        let mut runtime = Runtime::default();
        let selected = runtime.selection(&layout, now);
        assert!(runtime.status(&layout, now)[0].shown_member.is_none());
        runtime.acknowledge(&layout, &selected, now);
        assert_eq!(
            choice(&runtime, &layout, now + Duration::from_secs(299)),
            "local"
        );
        let selected = runtime.selection(&layout, now + Duration::from_secs(300));
        assert_eq!(selected["news"], "world");
        assert_eq!(
            choice(&runtime, &layout, now + Duration::from_secs(3600)),
            "world"
        );
        runtime.acknowledge(&layout, &selected, now + Duration::from_secs(3600));
        runtime.acknowledge(&layout, &selected, now + Duration::from_secs(3700)); // data/maintenance of same item
        assert_eq!(
            choice(&runtime, &layout, now + Duration::from_secs(3899)),
            "world"
        );
        assert_eq!(
            choice(&runtime, &layout, now + Duration::from_secs(3900)),
            "culture"
        );
    }

    #[test]
    fn pause_resume_and_membership_changes_have_no_missed_transition_backlog() {
        let mut layout = scene();
        let now = Instant::now();
        let mut runtime = Runtime::default();
        runtime.acknowledge(&layout, &runtime.selection(&layout, now), now);
        let rotation = layout.components[0].rotation.as_mut().unwrap();
        rotation.paused = true;
        rotation.paused_member = Some("local".into());
        runtime.acknowledge(&layout, &runtime.selection(&layout, now), now);
        assert_eq!(
            choice(&runtime, &layout, now + Duration::from_secs(86400)),
            "local"
        );
        layout.components[0].rotation.as_mut().unwrap().paused = false;
        let resumed = now + Duration::from_secs(86400);
        runtime.acknowledge(&layout, &runtime.selection(&layout, resumed), resumed);
        assert_eq!(
            choice(&runtime, &layout, resumed + Duration::from_secs(299)),
            "local"
        );
        assert_eq!(
            choice(&runtime, &layout, resumed + Duration::from_secs(300)),
            "world"
        );
        layout.components[0]
            .rotation
            .as_mut()
            .unwrap()
            .members
            .remove(1);
        assert_eq!(
            choice(&runtime, &layout, resumed + Duration::from_secs(300)),
            "culture"
        );
    }
}
