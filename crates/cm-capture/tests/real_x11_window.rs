//! Window capture on a real X11 server (CAP-1.AC4, CAP-3), self-contained: the test opens its own
//! windows through the X protocol, so it needs only an X server with a window manager that sets
//! `_NET_CLIENT_LIST_STACKING` (Xvfb plus openbox, as in CI and the Linux VM). Ignored: it needs
//! a display. Run with `DISPLAY=:99 cargo test -p cm-capture --test real_x11_window -- --ignored
//! --nocapture`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use cm_capture::{CaptureRequest, CaptureTarget, Capturer};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, PropMode, WindowClass};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::COPY_DEPTH_FROM_PARENT;

fn on_x11() -> bool {
    std::env::var("DISPLAY").is_ok_and(|d| !d.is_empty())
        && std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("wayland")
}

/// A mapped window of one flat colour. `wm_name` is set as `STRING` (what xclock and xterm do,
/// the case `xcap` reads as an empty title); `net_wm_name` as `UTF8_STRING` when given.
fn open_window(
    conn: &RustConnection,
    screen: usize,
    (x, y, w, h): (i16, i16, u16, u16),
    color: u32,
    wm_name: &[u8],
    net_wm_name: Option<&str>,
) -> u32 {
    let root = conn.setup().roots[screen].root;
    let id = conn.generate_id().unwrap();
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        id,
        root,
        x,
        y,
        w,
        h,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().background_pixel(color),
    )
    .unwrap();
    conn.change_property8(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        wm_name,
    )
    .unwrap();
    conn.change_property8(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        b"cmtest\0CmTest\0",
    )
    .unwrap();
    if let Some(name) = net_wm_name {
        let net = conn
            .intern_atom(false, b"_NET_WM_NAME")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let utf8 = conn
            .intern_atom(false, b"UTF8_STRING")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        conn.change_property8(PropMode::REPLACE, id, net, utf8, name.as_bytes())
            .unwrap();
    }
    conn.map_window(id).unwrap();
    conn.flush().unwrap();
    id
}

fn wait_for_window(c: &Capturer, needle: &str) -> cm_capture::WindowInfo {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(w) = c.find_window(needle) {
            return w;
        }
        assert!(
            Instant::now() < until,
            "no window titled {needle:?} appeared (is a window manager running?)"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn pixel(frame: &cm_capture::Frame, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * frame.width() + x) * 4) as usize;
    [frame.rgba()[i], frame.rgba()[i + 1], frame.rgba()[i + 2]]
}

/// A window found by the title a classic client sets (`WM_NAME` as `STRING`, Latin-1) is
/// captured at its bounds with its own pixels (CAP-1.AC4).
#[test]
#[ignore = "needs an X server and a window manager"]
fn a_latin_1_titled_window_is_listed_and_captured() {
    if !on_x11() {
        println!("SKIPPED: no X11 session");
        return;
    }
    let (conn, screen) = RustConnection::connect(None).unwrap();
    // "Café window" in Latin-1: the byte 0xE9.
    let id = open_window(
        &conn,
        screen,
        (80, 90, 320, 200),
        0x0033_99cc,
        b"Caf\xe9 window",
        None,
    );
    let c = Capturer::system().with_own_pid(None);
    let w = wait_for_window(&c, "café window");
    println!("{w:?}");
    assert_eq!(w.id, id);
    assert_eq!(w.title, "Café window");
    assert_eq!(w.app, "CmTest");
    let cap = c
        .capture(
            CaptureRequest::new(CaptureTarget::Window { window_id: w.id }),
            None,
        )
        .unwrap()
        .unwrap();
    assert!(cap.frame.width().abs_diff(320) <= 1 && cap.frame.height().abs_diff(200) <= 1);
    assert!(cap.frame.width().abs_diff(w.bounds.size.width) <= 1);
    // The window's own colour, away from the edges.
    assert_eq!(pixel(&cap.frame, 160, 100), [0x33, 0x99, 0xcc]);
    conn.destroy_window(id).unwrap();
    conn.flush().unwrap();
}

/// `_NET_WM_NAME` (UTF-8) wins over `WM_NAME`, and two windows are told apart.
#[test]
#[ignore = "needs an X server and a window manager"]
fn the_utf_8_title_is_used_when_there_is_one() {
    if !on_x11() {
        println!("SKIPPED: no X11 session");
        return;
    }
    let (conn, screen) = RustConnection::connect(None).unwrap();
    let a = open_window(
        &conn,
        screen,
        (500, 100, 200, 150),
        0x00cc_3333,
        b"old name",
        Some("Tiếng Việt cm-utf8"),
    );
    let b = open_window(
        &conn,
        screen,
        (500, 300, 200, 150),
        0x0033_cc33,
        b"cm-plain",
        None,
    );
    let c = Capturer::system().with_own_pid(None);
    let wa = wait_for_window(&c, "cm-utf8");
    let wb = wait_for_window(&c, "cm-plain");
    assert_eq!((wa.id, wb.id), (a, b));
    assert_eq!(wa.title, "Tiếng Việt cm-utf8");
    // A window just mapped may not be painted for a moment: look again until it shows its colour.
    let shows = |id: u32, want: [u8; 3]| {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let cap = c
                .capture(
                    CaptureRequest::new(CaptureTarget::Window { window_id: id }),
                    None,
                )
                .unwrap()
                .unwrap();
            let got = pixel(&cap.frame, 100, 75);
            if got == want || Instant::now() > until {
                return got;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    assert_eq!(shows(wa.id, [0xcc, 0x33, 0x33]), [0xcc, 0x33, 0x33]);
    assert_eq!(shows(wb.id, [0x33, 0xcc, 0x33]), [0x33, 0xcc, 0x33]);
    for id in [a, b] {
        conn.destroy_window(id).unwrap();
    }
    conn.flush().unwrap();
}
