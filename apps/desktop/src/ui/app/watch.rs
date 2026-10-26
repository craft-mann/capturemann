use super::*;

/// Polls the daemon's agent activity log every 2 s: refreshes the sidebar card and shows the
/// agent toast for each newly allowed agent action. The log is the only source of agent
/// identity the daemon exposes (events carry no actor), so this is a cheap poll, not a push.
pub fn start_agent_watch(app: &Entity<AppModel>, cx: &mut App) {
    let app2 = app.clone();
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(Duration::from_secs(2)).await;
        cx.update(|cx| {
            let (backend, seen) = {
                let m = app2.read(cx);
                (m.backend.clone(), m.agent_seen)
            };
            let Some(daemon) = backend.daemon.as_ref() else {
                return;
            };
            let activity = daemon.activity();
            let fresh = crate::agent_status::new_entries(seen, &activity);
            let total = activity.len();
            if fresh.is_empty() && total == seen {
                return;
            }
            let toasts: Vec<ToastSpec> = fresh
                .iter()
                .filter(|e| crate::agent_status::started_by_agent(e.decision))
                // The recording controller shows its own agent toast from the start event.
                .filter(|e| e.tool != "start_recording")
                .map(|e| ToastSpec::agent(e.tool.clone(), e.client.clone()))
                .collect();
            app2.update(cx, |m, cx| {
                m.agent_seen = total;
                cx.notify();
            });
            for spec in toasts {
                push_toast(&app2, spec, cx);
            }
        });
    })
    .detach();
}

/// Publishes the Studio window's CGWindowID twice a second (main thread), so recordings and
/// screenshots include the library window while it is on screen and drop it as soon as it is
/// closed or minimized (REC-6.AC8). The daemon's provider reads the published value from
/// capture threads; see `studio_window`.
pub fn start_studio_window_watch(app: &Entity<AppModel>, cx: &mut App) {
    let app2 = app.clone();
    cx.spawn(async move |cx| loop {
        cx.update(|cx| sync_studio_window(&app2, cx));
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
    })
    .detach();
}

/// One refresh of the published Studio window id.
pub fn sync_studio_window(app: &Entity<AppModel>, cx: &mut App) {
    let id = match app.read(cx).studio {
        Some(handle) => handle
            .update(cx, |_, window, _| {
                crate::platform_window::capture_window_id(window)
            })
            .ok()
            .flatten(),
        None => None,
    };
    crate::studio_window::publish(id);
}

/// How often the app looks at the bundle folders when it comes to the front (R1-6).
pub const RESCAN_EVERY: Duration = Duration::from_secs(10);

/// Whether a rescan is due: never while one runs, and at most every [`RESCAN_EVERY`] unless
/// `force`d.
pub fn rescan_due(last: Option<Instant>, busy: bool, now: Instant, force: bool) -> bool {
    !busy && (force || last.is_none_or(|t| now.saturating_duration_since(t) >= RESCAN_EVERY))
}

/// Whether a daemon error says the capture's bundle is gone (`no bundle at ...`, `no capture ...`):
/// the library row is a ghost and a rescan drops it (R1-6).
pub fn looks_missing(message: &str) -> bool {
    message.starts_with("no bundle at")
        || message.contains("no capture `")
        || message.contains("No such file or directory")
}

/// A capture could not be opened with `message`: when its folder is gone, rescan so the ghost row
/// disappears (R1-6).
pub fn note_open_failed(message: &str, cx: &mut App) {
    if !looks_missing(message) {
        return;
    }
    if let Some(app) = cx.try_global::<AppHandle>().map(|h| h.0.clone()) {
        request_rescan(&app, true, cx);
    }
}

/// Remembers that capture `id` changed and tells the views (the library reloads; only this
/// capture's thumbnail is dropped).
pub fn capture_changed(app: &Entity<AppModel>, id: Option<&str>, cx: &mut App) {
    app.update(cx, |m, cx| {
        if let Some(id) = id {
            if !m.changed_captures.iter().any(|c| c == id) && m.changed_captures.len() < 256 {
                m.changed_captures.push(id.to_owned());
            }
        }
        m.library_epoch += 1;
        cx.notify();
    });
}

/// R1-6: rebuilds the library index from the bundle folders on a worker thread (bundles copied in
/// appear, deleted ones stop being ghost rows). Called at startup, when the app comes to the front
/// (throttled to once per [`RESCAN_EVERY`]) and when opening a capture failed because its folder
/// is gone (`force`). The library reloads when something changed.
pub fn request_rescan(app: &Entity<AppModel>, force: bool, cx: &mut App) {
    let (daemon, due) = {
        let m = app.read(cx);
        (
            m.backend.daemon.clone(),
            rescan_due(m.last_rescan, m.rescan_busy, Instant::now(), force),
        )
    };
    let Some(daemon) = daemon.filter(|_| due) else {
        return;
    };
    app.update(cx, |m, _| {
        m.rescan_busy = true;
        m.last_rescan = Some(Instant::now());
    });
    let (rt, app2) = (app.read(cx).backend.rt.clone(), app.clone());
    cx.spawn(async move |cx| {
        let res = rt
            .spawn(async move { crate::backend::Backend::rescan_library(&daemon).await })
            .await;
        cx.update(|cx| {
            let summary = match res {
                Ok(Ok(s)) => Some(s),
                Ok(Err(e)) => {
                    log::warn!("library rescan failed: {e}");
                    None
                }
                Err(e) => {
                    log::warn!("library rescan failed: {e}");
                    None
                }
            };
            let changed = app2.update(cx, |m, _| {
                m.rescan_busy = false;
                let Some(s) = summary else { return false };
                let changed = s.removed > 0 || m.last_indexed != Some(s.indexed);
                m.last_indexed = Some(s.indexed);
                changed
            });
            if changed {
                capture_changed(&app2, None, cx);
            }
        });
    })
    .detach();
}
