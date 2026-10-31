//! `cm-overlay`: the region, window and display picker (CAP-1) as a client of the daemon.
//!
//! The overlay never calls an engine crate. It asks the daemon for `list_targets`, freezes each
//! display with `freeze_display` (no bundle is made), shows one borderless always-on-top window per display
//! with the frozen frame dimmed, lets the user pick, then asks the daemon for the final
//! capture (a crop of the frozen frame) and prints its `capture_id` as JSON.
//!
//! # Layout
//!
//! Pure, display-free logic (unit tested, cites CAP-1 / CAP-3 acceptance criteria):
//! [`selection`] (geometry, handles, nudging, click vs drag), [`picker`] (per-display state
//! machine), [`targets`] (window hit testing, monitor matching), [`magnifier`], [`canvas`],
//! [`text`], [`paint`] (software rendering to a pixel buffer), [`args`], [`error`],
//! [`session`] (daemon calls and their params), [`gate`] (pointer input held until the window
//! list arrives), [`trace`] (phase timings).
//!
//! Windowing: [`app`] (winit + softbuffer) and [`platform`] (the only `unsafe`).
//!
//! # Coordinates
//!
//! Everything the picker handles is **physical px relative to its display's top-left corner**,
//! the same space as the daemon's `rect` and the frozen frame, so no scale conversion happens
//! on the way to a capture (CAP-1.AC2). Logical px appear only when sizing UI chrome
//! (`value * scale`, see [`paint`]).
//!
//! # Exit codes
//!
//! 0 and the capture JSON on stdout; failures print `{"error": ...}` on stderr and exit per
//! the shared table (`permission_missing` 4, `daemon_unavailable` 5, ...). Esc exits 3
//! (`user_declined`) with `{"cancelled": true}` on stdout: nothing was saved.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod app;
pub mod args;
pub mod canvas;
pub mod client;
pub mod error;
pub mod gate;
pub mod magnifier;
pub mod paint;
pub mod picker;
pub mod platform;
pub mod selection;
pub mod session;
pub mod targets;
pub mod text;
pub mod trace;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::app::{Finish, Inbox, Options};
use crate::args::Args;
use crate::error::OverlayError;
use crate::session::{Emit, Frozen, SessionEvent};
use crate::trace::Tracer;

/// How long to wait, after the overlay ends, for freezes still in flight to finish.
const SETTLE: Duration = Duration::from_millis(300);

/// How much longer to wait for a freeze still in flight after [`SETTLE`] (a 4K frame is a 33 MB
/// file the daemon is still writing). Normally nothing waits: this only matters when the user
/// cancelled within a few hundred milliseconds of the overlay opening, and then it keeps the
/// frame from sitting in the daemon's temp directory until its 120 s expiry (R4-20).
const LATE_FREEZE: Duration = Duration::from_secs(2);

/// Runs one pick: freeze, show, choose, capture. Returns the JSON to print.
///
/// The session (connect, displays, freezes, window list) runs on the async runtime while the
/// main thread builds the event loop and the windows, so the two overlap; see [`app`].
///
/// Must be called on the main thread (the window system requires it).
pub fn run(args: &Args, started: Instant) -> Result<Value, OverlayError> {
    let tracer = Tracer::new(started, std::env::var_os("CM_OVERLAY_TRACE").is_some());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .map_err(|e| OverlayError::internal(format!("cannot start the async runtime: {e}")))?;
    tracer.mark("runtime_ready", Value::Null);
    // Wayland window mode: no window list exists, the desktop's own picker chooses (CAP-1).
    if args.mode == picker::Mode::Window
        && !args.pick
        && !args.exit_after_first_frame
        && session::is_wayland_session(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        )
    {
        match runtime.block_on(session::pick_with_system_picker(&args.endpoint, args.mode)) {
            Ok(v) => return Ok(v),
            // A daemon whose desktop has no picker: fall through to the overlay (it lists no
            // windows there, but a display or the region mode still work).
            Err(e) if e.code == cm_core::ErrorCode::NotImplemented.as_str() => {}
            Err(e) => return Err(e),
        }
    }
    let inbox = Inbox::new();
    let emit: Emit = {
        let inbox = inbox.clone();
        Arc::new(move |ev: SessionEvent| inbox.push(ev))
    };
    let issued = session::IssuedFrames::default();
    let session = {
        let (endpoint, tracer, emit) = (args.endpoint.clone(), tracer.clone(), emit);
        let issued = issued.clone();
        runtime.spawn(async move {
            let r = session::stream_tracked(&endpoint, &tracer, &emit, &issued).await;
            if let Err(e) = &r {
                emit(SessionEvent::Failed(e.clone()));
            }
            r
        })
    };
    let opts = Options {
        mode: args.mode,
        magnifier: args.magnifier,
        hint: args.hint,
        confirm_on_release: args.confirm_on_release,
        tab_to_list: args.tab_to_list && args.mode == picker::Mode::Window,
        exit_after_first_frame: args.exit_after_first_frame,
    };
    // Windows are closed when `app::run` returns, before the final capture is taken.
    let finished = app::run(&inbox, opts, &tracer);
    // Whatever happened, give the daemon its frames back (they would expire anyway). Freezes
    // still in flight are given a moment to land so their frames can be released too.
    let mut session = session;
    let settled = runtime
        .block_on(async {
            match tokio::time::timeout(SETTLE, &mut session).await {
                Ok(done) => Some(done),
                // Still freezing: give it a little longer, then stop waiting for it.
                Err(_) => match tokio::time::timeout(LATE_FREEZE, &mut session).await {
                    Ok(done) => Some(done),
                    Err(_) => {
                        session.abort();
                        None
                    }
                },
            }
        })
        .and_then(Result::ok)
        .and_then(Result::ok);
    let frozen: Vec<Frozen> = match (settled, &finished) {
        (Some(all), _) => all,
        (None, Ok(f)) => f.frozen.clone(),
        (None, Err(_)) => Vec::new(),
    };
    let result = match finished {
        Ok(app::Finished {
            finish: Finish::Cancelled,
            ..
        }) => Err(OverlayError::cancelled()),
        Ok(app::Finished {
            finish: Finish::SwitchToList,
            ..
        }) => Ok(json!({ "switch_to": "list" })),
        Ok(app::Finished {
            finish: Finish::FirstFrame,
            ..
        }) => Ok(json!({ "first_frame_ms": tracer.summary() })),
        Ok(app::Finished {
            finish: Finish::Chosen(choice),
            ..
        }) if args.pick => Ok(session::pick_json(args.mode, choice)),
        Ok(app::Finished {
            finish: Finish::Chosen(choice),
            ..
        }) => runtime.block_on(session::commit(&args.endpoint, args.mode, choice, &frozen)),
        Err(e) => Err(e),
    };
    // Release everything the daemon issued, including frames of freezes that never finished
    // loading here.
    let issued = issued
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let ids = session::ids_to_release(&issued, &frozen);
    runtime.block_on(session::release_ids(&args.endpoint, &ids));
    result
}
