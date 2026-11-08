//! The cursor track on Linux under X11 (REC-7; parity matrix task 3).
//!
//! - Moves, clicks and the wheel: XInput2 **raw** events (`RawMotion`, `RawButtonPress`,
//!   `RawButtonRelease`) selected on the root window for all master devices. They arrive without a
//!   grab and without disturbing other programs, and the mask holds no key events (NFR-9: no
//!   keystroke is ever seen). Raw motion carries deltas, so the absolute position is read with
//!   `QueryPointer` on the root window (once per batch of motion events); a click reads the
//!   position and the modifier flags the same way.
//! - The cursor image: XFixes `GetCursorImage`, polled by the shared image poller, taken only when
//!   the cursor serial changed.
//! - Time: the host clock when the event is read from the connection (the X11 event time is a
//!   server millisecond counter with no relation to the host clock). The reader sleeps 2 ms
//!   between empty polls, so an event is stamped at most about 2 ms late.
//! - Coordinates are root-window pixels (physical), so a `CursorTarget` for X11 has scale 1.0 and
//!   the origin of the recorded area in root coordinates.
//!
//! Wayland has no equivalent: an application cannot read the global pointer there (XWayland shows
//! the pointer only over X11 windows), so `start` answers `not_implemented` and the preflight
//! reports positions and clicks as unavailable. See `verification/` for the experiment.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cm_core::{CmError, ErrorCode, HostClock, SystemHostClock};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use super::x11_convert::{
    argb_premultiplied_to_rgba, classify_button, modifiers_from_state, XButton,
};
use crate::image::CursorImage;
use crate::schema::{Availability, CursorStatus, CursorWarning, MoveSource};
use crate::source::{
    spawn_image_poller, Activity, CursorSource, ImageProvider, RawEvent, RawKind, RawSender,
};

/// Is this a session where the X11 backend sees the whole pointer?
pub fn session_has_global_pointer() -> bool {
    std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty())
        && !super::x11_convert::is_wayland_session(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
        )
}

/// The status before recording: full on X11, positions and clicks unavailable elsewhere.
pub fn preflight() -> CursorStatus {
    if session_has_global_pointer() && x11rb::connect(None).is_ok() {
        CursorStatus::full(MoveSource::Xi2Raw)
    } else {
        unavailable_status()
    }
}

fn unavailable_status() -> CursorStatus {
    CursorStatus {
        clicks: Availability::Unavailable,
        scroll: Availability::Unavailable,
        move_source: MoveSource::Poll,
        warnings: vec![CursorWarning {
            code: ErrorCode::NotImplemented,
            permission: None,
            message: "This session (Wayland, or no X11 display) gives applications no global \
                      pointer position, so no cursor track is recorded."
                .into(),
        }],
    }
}

fn x(e: impl std::fmt::Display) -> CmError {
    CmError::internal(format!("X11 cursor: {e}"))
}

#[derive(Default)]
pub struct X11Source {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl CursorSource for X11Source {
    fn start(&mut self, tx: RawSender) -> Result<CursorStatus, CmError> {
        if !session_has_global_pointer() {
            return Err(CmError::not_implemented(
                "Cursor telemetry needs an X11 session; Wayland gives applications no global \
                 pointer position.",
            ));
        }
        let (conn, screen) = x11rb::connect(None).map_err(x)?;
        let root = conn.setup().roots[screen].root;
        let ver = conn
            .xinput_xi_query_version(2, 2)
            .map_err(x)?
            .reply()
            .map_err(x)?;
        if ver.major_version < 2 {
            return Err(CmError::not_implemented(
                "The X server has no XInput 2, which the cursor track needs.",
            ));
        }
        // Raw motion and buttons of every master device. No key events are selected.
        let mask = xinput::EventMask {
            deviceid: 1, // XIAllMasterDevices
            mask: vec![
                XIEventMask::RAW_MOTION
                    | XIEventMask::RAW_BUTTON_PRESS
                    | XIEventMask::RAW_BUTTON_RELEASE,
            ],
        };
        conn.xinput_xi_select_events(root, &[mask])
            .map_err(x)?
            .check()
            .map_err(x)?;

        self.stop.store(false, Ordering::Relaxed);
        let activity = Arc::new(Activity::default());
        let clock: Arc<dyn HostClock> = Arc::new(SystemHostClock);

        let (stop, act, ev_tx, ev_clock) = (
            self.stop.clone(),
            activity.clone(),
            tx.clone(),
            clock.clone(),
        );
        self.threads.push(
            thread::Builder::new()
                .name("cm-cursor-x11".into())
                .spawn(move || event_loop(&conn, root, &stop, &act, &ev_tx, ev_clock.as_ref()))
                .map_err(x)?,
        );
        // The image: its own connection, polled; absent XFixes only loses the pictures.
        if let Some(p) = XFixesImages::connect() {
            self.threads.push(spawn_image_poller(
                Box::new(p),
                clock,
                tx,
                self.stop.clone(),
                activity,
            ));
        }
        Ok(CursorStatus::full(MoveSource::Xi2Raw))
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for X11Source {
    fn drop(&mut self) {
        self.stop();
    }
}

fn event_loop(
    conn: &RustConnection,
    root: u32,
    stop: &AtomicBool,
    activity: &Activity,
    tx: &RawSender,
    clock: &dyn HostClock,
) {
    let pointer = || {
        conn.query_pointer(root)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| (f64::from(r.root_x), f64::from(r.root_y), u16::from(r.mask)))
    };
    let mut last: Option<(f64, f64)> = None;
    let mut last_poll = std::time::Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let mut moved: Option<u64> = None;
        let mut idle = true;
        loop {
            let ev = match conn.poll_for_event() {
                Ok(Some(ev)) => ev,
                Ok(None) => break,
                Err(_) => return, // the X connection is gone
            };
            idle = false;
            let host_ns = clock.now_ns();
            match ev {
                // Motion is batched: one position read after the queue is drained.
                Event::XinputRawMotion(_) => moved = Some(host_ns),
                Event::XinputRawButtonPress(b) => {
                    button_event(b.detail, true, host_ns, &pointer, activity, tx);
                }
                Event::XinputRawButtonRelease(b) => {
                    button_event(b.detail, false, host_ns, &pointer, activity, tx);
                }
                _ => {}
            }
        }
        // A warp (`XWarpPointer`: remote desktops, `xdotool mousemove`, some games) makes no raw
        // event, so the position is also read every 16 ms and sent when it changed.
        if moved.is_none() && last_poll.elapsed() >= Duration::from_millis(16) {
            moved = Some(clock.now_ns());
        }
        if let Some(host_ns) = moved {
            last_poll = std::time::Instant::now();
            if let Some((gx, gy, _)) = pointer().filter(|p| last != Some((p.0, p.1))) {
                last = Some((gx, gy));
                activity.touch(host_ns);
                if !tx.send(RawEvent {
                    host_ns,
                    kind: RawKind::Move { gx, gy },
                }) {
                    return;
                }
            }
        }
        if idle {
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn button_event(
    detail: u32,
    down: bool,
    host_ns: u64,
    pointer: &dyn Fn() -> Option<(f64, f64, u16)>,
    activity: &Activity,
    tx: &RawSender,
) {
    let Some((gx, gy, state)) = pointer() else {
        return;
    };
    activity.touch(host_ns);
    let kind = match classify_button(detail) {
        XButton::Button(button) => RawKind::Button {
            gx,
            gy,
            button,
            down,
            mods: modifiers_from_state(state),
        },
        // The wheel is a press and a release of a button; one event per notch.
        XButton::Scroll(dx, dy) => {
            if !down {
                return;
            }
            RawKind::Scroll { dx, dy }
        }
    };
    tx.send(RawEvent { host_ns, kind });
}

/// Cursor pictures through XFixes, given only when the cursor serial changes.
struct XFixesImages {
    conn: RustConnection,
    last_serial: Option<u32>,
}

impl XFixesImages {
    fn connect() -> Option<Self> {
        let (conn, _) = x11rb::connect(None).ok()?;
        conn.xfixes_query_version(4, 0).ok()?.reply().ok()?;
        Some(Self {
            conn,
            last_serial: None,
        })
    }
}

impl ImageProvider for XFixesImages {
    fn poll(&mut self) -> Option<CursorImage> {
        let r = self.conn.xfixes_get_cursor_image().ok()?.reply().ok()?;
        if self.last_serial == Some(r.cursor_serial) {
            return None;
        }
        self.last_serial = Some(r.cursor_serial);
        let (w, h) = (u32::from(r.width), u32::from(r.height));
        if w == 0 || h == 0 || r.cursor_image.len() < (w * h) as usize {
            return None;
        }
        Some(CursorImage {
            width: w,
            height: h,
            rgba: argb_premultiplied_to_rgba(&r.cursor_image[..(w * h) as usize]),
            hotspot: [i32::from(r.xhot), i32::from(r.yhot)],
            scale: 1.0,
        })
    }
}
