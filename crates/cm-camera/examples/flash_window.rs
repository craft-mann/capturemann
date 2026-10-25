//! A small window that flips between black and white once per second for the optical sync test
//! (tests/manual/CAM-2.md) and logs the host time of each change.
//!
//! ```text
//! cargo run -p cm-camera --example flash_window -- --out flashes.json [--secs 8]
//! ```
//!
//! Safety: 640x480 window (not fullscreen), one colour change about every second (0.5 Hz flicker, far below
//! the 3 Hz photosensitivity limit), at most 10 s, closes itself. It takes focus when it opens.
//! It never touches the camera; point the camera at it yourself (or at a mirror showing it).

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("flash_window is macOS only");
}

#[cfg(target_os = "macos")]
fn main() {
    imp::run();
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::RefCell;
    use std::time::Duration;

    use cm_camera::flash::FlashEvent;
    use cm_core::{HostClock, SystemHostClock};
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSWindow,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    thread_local! {
        static WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        let value = |n: &str| {
            args.iter()
                .position(|a| a == n)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let Some(out) = value("--out") else {
            eprintln!("usage: flash_window --out flashes.json [--secs 8]");
            std::process::exit(2);
        };
        let secs: u64 = value("--secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(8)
            .clamp(2, 10);
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("must run on the main thread");
            std::process::exit(1);
        };
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        // SAFETY: standard window creation on the main thread; the window is kept in a
        // thread-local for the life of the process.
        let window = unsafe {
            let w = NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(NSPoint::new(200.0, 200.0), NSSize::new(640.0, 480.0)),
                NSWindowStyleMask::Titled,
                NSBackingStoreType::Buffered,
                false,
            );
            w.setReleasedWhenClosed(false);
            w
        };
        window.setTitle(&NSString::from_str(
            "cm-camera flash test (closes by itself)",
        ));
        window.setBackgroundColor(Some(&NSColor::blackColor()));
        window.makeKeyAndOrderFront(None);
        WINDOW.with(|w| *w.borrow_mut() = Some(window));
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            let (tx, rx) = std::sync::mpsc::channel::<u64>();
            let mut log: Vec<FlashEvent> = Vec::new();
            let mut bright = false;
            for _ in 0..secs {
                bright = !bright;
                let tx = tx.clone();
                DispatchQueue::main().exec_async(move || {
                    WINDOW.with(|w| {
                        if let Some(w) = w.borrow().as_ref() {
                            let c = if bright {
                                NSColor::whiteColor()
                            } else {
                                NSColor::blackColor()
                            };
                            w.setBackgroundColor(Some(&c));
                            w.display();
                        }
                    });
                    let _ = tx.send(SystemHostClock.now_ns());
                });
                if let Ok(host_ns) = rx.recv_timeout(Duration::from_secs(1)) {
                    log.push(FlashEvent {
                        host_ns,
                        rising: bright,
                    });
                }
                // 1.013 s, not 1 s: the flash phase then walks across the camera's frame grid, so the
                // +-half-frame quantization of each edge averages out over the run.
                std::thread::sleep(Duration::from_millis(1013));
            }
            match serde_json::to_string_pretty(&log) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&out, json) {
                        eprintln!("cannot write {out}: {e}");
                    } else {
                        println!("logged {} flashes to {out}", log.len());
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
            std::process::exit(0);
        });
        app.run();
    }
}
