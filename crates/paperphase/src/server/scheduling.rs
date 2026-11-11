//! Clock/freshness deadlines and the simulated refresh gate. No provider polling.
use std::time::Duration;

use crate::display::{Controller, Operation};
use paperphase_core::{Layout, RefreshSettings, SnapshotSet, TextSource};
use paperphase_render::Frame;
use tokio::time::Instant;

use super::{ApiResult, Engine, STANDARD, Saved, Shared, Workspace, internal, invalid, render};

impl Workspace {
    pub(super) fn new(
        saved: Saved,
        png: Vec<u8>,
        frame: Frame,
        snapshots: &SnapshotSet,
        at: u64,
    ) -> anyhow::Result<Self> {
        let quiet = saved.refresh_settings.is_quiet(at)?;
        let next_due = next_due(&saved.layout, snapshots, at)?;
        Ok(Self {
            displayed_revision: saved.revision,
            completed_layout: saved.layout.clone(),
            saved,
            png,
            completed_frame: frame,
            controller: Controller::new(),
            desired: None,
            worker_running: false,
            refresh_wake: std::sync::Arc::new(tokio::sync::Notify::new()),
            frame_serial: 0,
            refreshing: false,
            quiet,
            pending: false,
            last_tick: at,
            next_due,
            last_error: None,
        })
    }

    pub(super) fn phase(&self) -> &'static str {
        if self.refreshing {
            "refreshing"
        } else if self.quiet {
            "paused"
        } else if self.last_error.is_some() {
            "error"
        } else if self.pending {
            "pending"
        } else {
            "idle"
        }
    }
}

pub(super) fn next_due(
    layout: &Layout,
    data: &SnapshotSet,
    now: u64,
) -> paperphase_core::Result<Option<u64>> {
    let mut next = None;
    let mut offer = |at: u64| {
        if at > now {
            next = Some(next.map_or(at, |old: u64| old.min(at)));
        }
    };
    for component in &layout.components {
        for block in &component.blocks {
            match &block.source {
                TextSource::Clock { clock } => offer(clock.next_change_after(now)?),
                source if source.binding().is_some() => {
                    let binding = source.binding().expect("checked");
                    if let Some(snapshot) = data.snapshots.iter().find(|s| {
                        s.connection_id == binding.connection_id
                            && s.dataset.id == binding.dataset_id
                    }) && snapshot.last_error.is_none()
                    {
                        if !snapshot.is_stale(now) {
                            let observed = snapshot
                                .observed_at
                                .unwrap_or(snapshot.last_success_at)
                                .min(snapshot.last_success_at);
                            offer(observed.saturating_add(snapshot.stale_after_secs));
                        } else {
                            // A future timestamp is stale until both clocks have caught up.
                            offer(
                                snapshot
                                    .observed_at
                                    .unwrap_or(0)
                                    .max(snapshot.last_success_at),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(next)
}

pub(super) struct PreparedFrame {
    data_epoch: u64,
    revision: u64,
    layout: Layout,
    frame: Frame,
    png: Vec<u8>,
    next_due: Option<u64>,
    quiet: bool,
    at: u64,
}

pub(super) async fn prepare(
    app: Shared,
    layout: &Layout,
    settings: &RefreshSettings,
    at: u64,
) -> ApiResult<PreparedFrame> {
    let quiet = settings.is_quiet(at).map_err(invalid)?;
    let preview = render(app, layout.clone(), at).await?;
    Ok(PreparedFrame {
        png: STANDARD.decode(preview.png_base64).map_err(internal)?,
        revision: 0,
        layout: layout.clone(),
        frame: preview.frame,
        next_due: preview.next_due,
        data_epoch: preview.data_epoch,
        quiet,
        at,
    })
}

pub(super) fn queue(app: &Shared, ws: &mut Workspace, mut frame: PreparedFrame) {
    ws.last_tick = frame.at;
    ws.next_due = frame.next_due;
    ws.quiet = frame.quiet;
    ws.pending = true;
    if frame.data_epoch != app.data_epoch.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    // A metadata-only save can acknowledge the completed frame immediately.
    if !ws.worker_running
        && ws.controller.baseline_known
        && frame.frame == ws.completed_frame
        && !ws.quiet
    {
        ws.completed_layout = frame.layout;
        ws.displayed_revision = ws.saved.revision;
        ws.desired = None;
        ws.pending = false;
        ws.last_error = None;
        return;
    }
    frame.revision = ws.saved.revision;
    ws.desired = Some(frame);
    ws.refresh_wake.notify_one();
    if ws.quiet || ws.worker_running {
        return;
    }
    ws.worker_running = true;
    let app = std::sync::Arc::clone(app);
    tokio::spawn(async move {
        drive(app).await;
    });
}

fn current(app: &Shared, ws: &Workspace, frame: &PreparedFrame, now: u64) -> bool {
    frame.revision == ws.saved.revision
        && frame.data_epoch == app.data_epoch.load(std::sync::atomic::Ordering::SeqCst)
        && now >= frame.at
        && frame.next_due.is_none_or(|due| now < due)
}

async fn drive(app: Shared) {
    // Collect a burst of edits before starting the single simulated operation.
    tokio::time::sleep(Duration::from_millis(180)).await;
    loop {
        let (frame, operation) = {
            let mut ws = app.workspace.lock().await;
            ws.quiet = ws
                .saved
                .refresh_settings
                .is_quiet((app.clock)())
                .unwrap_or(true);
            if ws.quiet {
                ws.worker_running = false;
                return;
            }
            let Some(frame) = ws.desired.take() else {
                ws.worker_running = false;
                return;
            };
            if !current(&app, &ws, &frame, (app.clock)()) {
                ws.worker_running = false;
                ws.pending = true;
                return;
            }
            let now = Instant::now();
            let plan = ws.controller.plan(
                &ws.completed_frame,
                &frame.frame,
                &ws.completed_layout,
                &frame.layout,
                &ws.saved.refresh_settings.policy,
                now,
            );
            let plan = match plan {
                Ok(Some(plan)) => plan,
                Ok(None) => {
                    ws.displayed_revision = frame.revision;
                    ws.completed_layout = frame.layout;
                    ws.pending = false;
                    ws.last_error = None;
                    ws.worker_running = false;
                    return;
                }
                Err(error) => {
                    eprintln!("refresh planning failed: {error}");
                    ws.last_error = Some("Could not plan a display update.".into());
                    ws.worker_running = false;
                    return;
                }
            };
            let ready = ws
                .controller
                .ready_at(&ws.saved.refresh_settings.policy, now);
            if ready > now {
                ws.desired = Some(frame);
                let wake = std::sync::Arc::clone(&ws.refresh_wake);
                drop(ws);
                tokio::select! {
                    _ = tokio::time::sleep_until(ready) => {},
                    _ = wake.notified() => {},
                }
                continue;
            }
            let operation = Operation {
                plan,
                revision: frame.revision,
            };
            ws.controller.active = Some(operation.clone());
            ws.refreshing = true;
            (frame, operation)
        };
        // The simulated driver owns this frame until completion. Newer desired
        // frames cannot cancel it or start a second operation concurrently.
        tokio::time::sleep(Duration::from_millis(180)).await;
        let mut ws = app.workspace.lock().await;
        ws.refreshing = false;
        ws.quiet = ws
            .saved
            .refresh_settings
            .is_quiet((app.clock)())
            .unwrap_or(true);
        #[cfg(test)]
        let failed = std::mem::take(&mut ws.controller.fail_next);
        #[cfg(not(test))]
        let failed = false;
        if failed {
            ws.controller.failed(Instant::now());
            ws.last_error = Some("Display update failed; retrying with a full refresh. Last completed frame is shown.".into());
            ws.pending = true;
            if ws.desired.is_none() {
                ws.desired = Some(frame);
            }
        } else {
            let up_to_date = current(&app, &ws, &frame, (app.clock)());
            ws.controller.complete(operation, Instant::now());
            ws.png = frame.png;
            ws.completed_frame = frame.frame;
            ws.completed_layout = frame.layout;
            ws.displayed_revision = frame.revision;
            ws.frame_serial = ws.frame_serial.wrapping_add(1);
            ws.pending = !up_to_date || ws.desired.is_some();
            ws.last_error = None;
        }
        if ws.desired.is_none() || ws.quiet {
            ws.worker_running = false;
            return;
        }
    }
}

pub(super) async fn tick(app: Shared, force: bool) -> ApiResult<()> {
    let _tick = app.tick_lock.lock().await;
    let now = (app.clock)();
    let saved = {
        let mut ws = app.workspace.lock().await;
        let was_quiet = ws.quiet;
        let backwards = now < ws.last_tick;
        ws.last_tick = now;
        ws.quiet = ws.saved.refresh_settings.is_quiet(now).map_err(invalid)?;
        let due = ws.next_due.is_some_and(|due| now >= due);
        if ws.quiet {
            ws.pending |= force || due || backwards || ws.refreshing;
            ws.desired = None;
            ws.refresh_wake.notify_one();
            return Ok(());
        }
        if !force && !was_quiet && !backwards && (ws.worker_running || (!due && !ws.pending)) {
            return Ok(());
        }
        ws.saved.clone()
    };
    let result = prepare(
        std::sync::Arc::clone(&app),
        &saved.layout,
        &saved.refresh_settings,
        now,
    )
    .await;
    let mut ws = app.workspace.lock().await;
    if ws.saved.revision != saved.revision {
        return Ok(());
    }
    match result {
        Ok(frame) => queue(&app, &mut ws, frame),
        Err(error) => {
            ws.pending = true;
            ws.last_error =
                Some("Could not render the latest frame; keeping the completed display.".into());
            return Err(error);
        }
    }
    Ok(())
}

pub(super) fn start(app: Shared) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = tick(std::sync::Arc::clone(&app), false).await {
                eprintln!("display scheduler: {}", error.1);
            }
        }
    })
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::server::{App, SaveRequest, persist, save};
    use axum::{Json, extract::State};
    use paperphase_render::Renderer;
    use serde_json::json;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    };
    use tokio::sync::{Mutex as AsyncMutex, Semaphore};

    const START: u64 = 1_699_999_980; // 2023-11-14 22:13 UTC

    pub(in crate::server) fn scene() -> Layout {
        serde_json::from_value(json!({"schema_version":1,"display":{"id":"test","width":320,"height":160},"components":[{
            "id":"clock","rect":{"x":0,"y":0,"width":320,"height":160},"blocks":[
                {"id":"time","font_size":40,"source":{"kind":"clock","clock":{"timezone":"Europe/Dublin"}}},
                {"id":"caption","font_size":16,"source":{"kind":"literal","text":"Original"}}
            ]}]})).unwrap()
    }
    pub(in crate::server) fn quiet() -> RefreshSettings {
        serde_json::from_value(
            json!({"quiet_hours":{"timezone":"Europe/Dublin","start":"22:14","end":"22:16"}}),
        )
        .unwrap()
    }
    pub(in crate::server) fn app(
        layout: Layout,
        settings: RefreshSettings,
        snapshots: SnapshotSet,
        at: u64,
    ) -> (Shared, Arc<AtomicU64>, tempfile::TempDir) {
        let clock = Arc::new(AtomicU64::new(at));
        let time = Arc::clone(&clock);
        let saved = Saved {
            schema_version: 1,
            revision: 0,
            layout,
            refresh_settings: settings,
        };
        let mut renderer = Renderer::new().unwrap();
        let rendered = renderer.render(&saved.layout, &snapshots, at).unwrap();
        let mut png = Vec::new();
        rendered.frame.write_png(&mut png).unwrap();
        let workspace = Workspace::new(saved, png, rendered.frame, &snapshots, at).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        persist(&path, &workspace.saved).unwrap();
        let app = Arc::new(App {
            executors: None,
            fetch_slots: super::super::fetching::slots(),
            data_epoch: AtomicU64::new(0),
            connector_catalog: serde_json::from_str(include_str!(
                "../../../../examples/simulator/connectors.json"
            ))
            .unwrap(),
            connections: AsyncMutex::new(super::super::connections::ConnectionStore::default()),
            connections_file: dir.path().join("connections.json"),
            workspace: AsyncMutex::new(workspace),
            renderer: Mutex::new(renderer),
            snapshots,
            state_file: path,
            clock: Arc::new(move || time.load(Ordering::SeqCst)),
            clock_fixed: false,
            boot_id: "test".into(),
            tick_lock: AsyncMutex::new(()),
            authorities: vec![],
            addresses: vec![],
            render_slots: Arc::new(Semaphore::new(4)),
            _file_lock: tempfile::tempfile().unwrap(),
        });
        (app, clock, dir)
    }
    pub(in crate::server) fn empty() -> SnapshotSet {
        SnapshotSet {
            schema_version: 1,
            snapshots: vec![],
        }
    }
    pub(in crate::server) async fn settle(app: &Shared) {
        for _ in 0..250 {
            if !app.workspace.lock().await.worker_running {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("simulator did not complete");
    }
    async fn edit(app: &Shared, text: &str) -> serde_json::Value {
        let saved = app.workspace.lock().await.saved.clone();
        let mut layout = serde_json::to_value(saved.layout).unwrap();
        layout["components"][0]["blocks"][1]["source"]["text"] = json!(text);
        save(
            State(Arc::clone(app)),
            Json(SaveRequest {
                base_revision: saved.revision,
                layout: serde_json::from_value(layout).unwrap(),
                refresh_settings: None,
            }),
        )
        .await
        .unwrap()
        .0
    }
    pub(in crate::server) async fn assert_current_pixels(app: &Shared) {
        let (layout, settings, png) = {
            let ws = app.workspace.lock().await;
            (
                ws.saved.layout.clone(),
                ws.saved.refresh_settings.clone(),
                ws.png.clone(),
            )
        };
        let expected = prepare(Arc::clone(app), &layout, &settings, (app.clock)())
            .await
            .unwrap();
        assert_eq!(png, expected.png);
    }

    #[tokio::test]
    async fn minute_updates_missed_ticks_and_backwards_clock_do_not_change_layout_revisions() {
        let (app, clock, _dir) = app(scene(), RefreshSettings::default(), empty(), START);
        let original = app.workspace.lock().await.png.clone();
        clock.store(START + 59, Ordering::SeqCst);
        tick(Arc::clone(&app), false).await.unwrap();
        assert_eq!(app.workspace.lock().await.png, original);
        for (time, expected_serial) in [(START + 60, 1), (START + 240, 2), (START + 90, 3)] {
            clock.store(time, Ordering::SeqCst);
            tick(Arc::clone(&app), false).await.unwrap();
            settle(&app).await;
            let ws = app.workspace.lock().await;
            assert_eq!(ws.frame_serial, expected_serial);
            assert_eq!(ws.saved.revision, 0);
            assert_eq!(ws.displayed_revision, 0);
            drop(ws);
            assert_current_pixels(&app).await;
        }
        tick(Arc::clone(&app), true).await.unwrap();
        assert_eq!(app.workspace.lock().await.frame_serial, 3);
    }

    #[tokio::test]
    async fn quiet_hours_keep_the_frame_and_resume_only_the_newest_content() {
        let (app, clock, _dir) = app(scene(), quiet(), empty(), START);
        let original = app.workspace.lock().await.png.clone();
        clock.store(START + 60, Ordering::SeqCst);
        tick(Arc::clone(&app), false).await.unwrap();
        assert_eq!(edit(&app, "First saved update").await["phase"], "paused");
        assert_eq!(edit(&app, "Newest saved update").await["phase"], "paused");
        tick(Arc::clone(&app), true).await.unwrap();
        let ws = app.workspace.lock().await;
        assert_eq!(ws.phase(), "paused");
        assert!(ws.pending);
        assert_eq!(ws.png, original);
        assert_eq!(ws.displayed_revision, 0);
        assert_eq!(ws.saved.revision, 2);
        drop(ws);
        clock.store(START + 240, Ordering::SeqCst);
        tick(Arc::clone(&app), false).await.unwrap();
        settle(&app).await;
        let ws = app.workspace.lock().await;
        assert_eq!(ws.phase(), "idle");
        assert!(!ws.pending);
        assert_eq!(ws.displayed_revision, 2);
        assert_eq!(ws.frame_serial, 1);
        drop(ws);
        assert_current_pixels(&app).await;
    }

    #[tokio::test]
    async fn a_queued_frame_cannot_start_in_quiet_hours() {
        let (app, clock, _dir) = app(scene(), quiet(), empty(), START + 59);
        let original = app.workspace.lock().await.png.clone();
        assert_eq!(
            edit(&app, "Queued before quiet hours").await["phase"],
            "pending"
        );
        clock.store(START + 60, Ordering::SeqCst);
        settle(&app).await;
        let ws = app.workspace.lock().await;
        assert_eq!(ws.phase(), "paused");
        assert_eq!(ws.png, original);
        assert!(ws.pending);
        assert_eq!(ws.frame_serial, 0);
    }

    #[tokio::test]
    async fn newer_saves_supersede_pending_completions_and_unchanged_pixels_skip_refresh() {
        let (app, _clock, _dir) = app(scene(), RefreshSettings::default(), empty(), START);
        edit(&app, "Superseded").await;
        edit(&app, "Newest").await;
        settle(&app).await;
        assert_current_pixels(&app).await;
        assert_eq!(app.workspace.lock().await.frame_serial, 1);
        assert_eq!(edit(&app, "Newest").await["phase"], "idle");
        let ws = app.workspace.lock().await;
        assert_eq!(ws.frame_serial, 1);
        assert_eq!(ws.saved.revision, 3);
        assert_eq!(ws.displayed_revision, 3);
    }

    #[tokio::test]
    async fn bound_snapshot_freshness_has_its_own_deadline_without_a_clock_block() {
        let mut value = serde_json::to_value(scene()).unwrap();
        value["components"][0]["blocks"] = json!([{"id":"temperature","font_size":30,"source":{"kind":"field","binding":{"connection_id":"home","dataset_id":"weather","field_id":"temperature"}}}]);
        let data: SnapshotSet = serde_json::from_value(json!({"schema_version":1,"snapshots":[{"connection_id":"home","last_success_at":START,"stale_after_secs":60,"dataset":{"schema_version":1,"id":"weather","fields":[{"id":"temperature","kind":"number"}],"rows":[{"temperature":18.5}]}}]})).unwrap();
        let (app, clock, _dir) = app(
            serde_json::from_value(value).unwrap(),
            RefreshSettings::default(),
            data,
            START,
        );
        let original = app.workspace.lock().await.png.clone();
        clock.store(START + 60, Ordering::SeqCst);
        tick(Arc::clone(&app), false).await.unwrap();
        settle(&app).await;
        let ws = app.workspace.lock().await;
        assert_ne!(ws.png, original);
        assert_eq!(ws.frame_serial, 1);
        assert_eq!(ws.next_due, None);
    }
    async fn wait_for(app: &Shared, ready: impl Fn(&Workspace) -> bool) {
        for _ in 0..1000 {
            if ready(&*app.workspace.lock().await) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("refresh state did not arrive");
    }

    #[tokio::test]
    async fn running_operations_finish_serially_while_pending_saves_are_batched() {
        let (app, _clock, _dir) = app(scene(), RefreshSettings::default(), empty(), START);
        let original = app.workspace.lock().await.png.clone();
        edit(&app, "First running update").await;
        wait_for(&app, |ws| ws.refreshing).await;
        edit(&app, "Superseded pending update").await;
        edit(&app, "Newest pending update").await;
        {
            let ws = app.workspace.lock().await;
            assert_eq!(ws.controller.active.as_ref().unwrap().revision, 1);
            assert_eq!(ws.png, original);
            assert_eq!(ws.displayed_revision, 0);
        }
        wait_for(&app, |ws| ws.frame_serial == 1).await;
        {
            let ws = app.workspace.lock().await;
            assert_eq!(ws.displayed_revision, 1);
            assert_eq!(ws.saved.revision, 3);
            assert!(ws.pending);
            assert!(!ws.refreshing);
        }
        settle(&app).await;
        assert_current_pixels(&app).await;
        let ws = app.workspace.lock().await;
        assert_eq!(ws.frame_serial, 2);
        assert_eq!(ws.displayed_revision, 3);
    }

    #[tokio::test]
    async fn a_running_refresh_finishes_in_quiet_hours_but_no_next_operation_starts() {
        let (app, clock, _dir) = app(scene(), quiet(), empty(), START + 59);
        edit(&app, "Started before quiet hours").await;
        wait_for(&app, |ws| ws.refreshing).await;
        clock.store(START + 60, Ordering::SeqCst);
        tick(Arc::clone(&app), false).await.unwrap();
        assert_eq!(app.workspace.lock().await.phase(), "refreshing");
        edit(&app, "Saved during quiet hours").await;
        settle(&app).await;
        {
            let ws = app.workspace.lock().await;
            assert_eq!(ws.displayed_revision, 1);
            assert_eq!(ws.saved.revision, 2);
            assert_eq!(ws.phase(), "paused");
            assert!(ws.pending);
            assert_eq!(ws.frame_serial, 1);
        }
        clock.store(START + 240, Ordering::SeqCst);
        tick(Arc::clone(&app), false).await.unwrap();
        settle(&app).await;
        assert_current_pixels(&app).await;
    }

    #[tokio::test]
    async fn failed_updates_keep_completed_pixels_obey_backoff_and_recover_with_full_refresh() {
        let (app, _clock, _dir) = app(scene(), RefreshSettings::default(), empty(), START);
        let original = app.workspace.lock().await.png.clone();
        app.workspace.lock().await.controller.fail_next = true;
        edit(&app, "An update that fails once").await;
        wait_for(&app, |ws| ws.controller.failures == 1).await;
        {
            let ws = app.workspace.lock().await;
            assert_eq!(ws.png, original);
            assert_eq!(ws.displayed_revision, 0);
            assert_eq!(ws.frame_serial, 0);
            assert_eq!(ws.controller.partials, 0);
            assert!(!ws.controller.baseline_known);
            assert_eq!(ws.phase(), "error");
        }
        tick(Arc::clone(&app), true).await.unwrap();
        assert!(app.workspace.lock().await.controller.active.is_none());
        settle(&app).await;
        assert_current_pixels(&app).await;
        let ws = app.workspace.lock().await;
        assert!(ws.last_error.is_none());
        assert_eq!(
            ws.controller.completed.as_ref().unwrap().plan.reason,
            crate::display::Reason::UnknownBaseline
        );
        assert_eq!(ws.frame_serial, 1);
    }

    #[tokio::test]
    async fn reducing_a_saved_interval_wakes_the_pending_worker() {
        let (app, _clock, _dir) = app(scene(), RefreshSettings::default(), empty(), START);
        edit(&app, "First").await;
        settle(&app).await;
        let saved = app.workspace.lock().await.saved.clone();
        let mut slow = saved.refresh_settings.clone();
        slow.policy.min_interval_ms = 60000;
        let mut layout = serde_json::to_value(saved.layout).unwrap();
        layout["components"][0]["blocks"][1]["source"]["text"] =
            json!("Waiting for a long interval");
        let layout: Layout = serde_json::from_value(layout).unwrap();
        let _ = save(
            State(Arc::clone(&app)),
            Json(SaveRequest {
                base_revision: saved.revision,
                layout: layout.clone(),
                refresh_settings: Some(slow),
            }),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(220)).await;
        assert!(app.workspace.lock().await.controller.active.is_none());
        let _ = save(
            State(Arc::clone(&app)),
            Json(SaveRequest {
                base_revision: saved.revision + 1,
                layout,
                refresh_settings: Some(RefreshSettings::default()),
            }),
        )
        .await
        .unwrap();
        settle(&app).await;
        assert_current_pixels(&app).await;
        assert_eq!(
            app.workspace.lock().await.displayed_revision,
            saved.revision + 2
        );
    }
}
