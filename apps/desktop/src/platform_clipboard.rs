#![cfg(not(target_os = "macos"))]
//! The clipboard backends that are not the macOS pasteboard (CAP-4): `arboard` on Windows and,
//! on Linux, `arboard` first and an installed helper program second. Every function returns
//! `Ok` only when the data really is on the clipboard, so the "Copied to clipboard" toast and
//! `--copy` never claim a copy that did not happen.
//!
//! Linux notes (checked in the QEMU VM, `verification/L1-clipboard-hotkeys-linux-2026-10-31-claude.md`):
//! * X11: the app owns the selection and serves paste requests from `arboard`'s background
//!   thread for as long as the process lives (the desktop app is long-lived; a one-shot process
//!   would lose the data on exit, which is why only the app copies).
//! * GNOME Wayland: Mutter has no `wlr-data-control`, so `arboard` falls back to X11 through
//!   XWayland, and Mutter hands that selection to Wayland apps. With no XWayland the error says
//!   so and `wl-copy` (wl-clipboard, GPL-3.0, run as a program, never linked) is tried.
//! * wlroots and KDE: `arboard`'s `wayland-data-control` feature talks the data-control protocol.

use cm_core::CmError;

/// PNG bytes onto the clipboard as an image.
pub fn set_image(png: &[u8]) -> Result<(), CmError> {
    let first = set_image_arboard(png);
    #[cfg(target_os = "linux")]
    if let Err(e) = &first {
        return linux::helper_image(png).map_err(|helper| linux::combine(e, helper));
    }
    first
}

/// Plain text onto the clipboard.
pub fn set_text(text: &str) -> Result<(), CmError> {
    let first = with_clipboard(|c| c.set_text(text.to_owned()));
    #[cfg(target_os = "linux")]
    if let Err(e) = &first {
        return linux::helper_text(text).map_err(|helper| linux::combine(e, helper));
    }
    first
}

fn set_image_arboard(png: &[u8]) -> Result<(), CmError> {
    let (width, height, bytes) = crate::clipboard::png_to_rgba(png)?;
    with_clipboard(|c| {
        c.set_image(arboard::ImageData {
            width,
            height,
            bytes: bytes.into(),
        })
    })
}

/// Runs `f` on an `arboard` clipboard. Windows opens one per call. On Linux the handle is kept
/// for the life of the process: with X11 the data lives in the process that owns the selection,
/// and `arboard` gives it up when its last `Clipboard` is dropped (without a clipboard manager
/// the copy would vanish at once, while the toast already said "Copied").
fn with_clipboard<T>(
    f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, CmError> {
    #[cfg(target_os = "linux")]
    {
        use std::sync::{Mutex, PoisonError};
        static KEPT: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);
        let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
        if kept.is_none() {
            *kept = Some(
                arboard::Clipboard::new()
                    .map_err(|e| CmError::internal(format!("clipboard unavailable: {e}")))?,
            );
        }
        let Some(cb) = kept.as_mut() else {
            return Err(CmError::internal("clipboard unavailable"));
        };
        let result = f(cb);
        if result.is_err() {
            // A connection that failed once is rebuilt on the next copy.
            *kept = None;
        }
        result.map_err(|e| CmError::internal(format!("cannot copy to the clipboard: {e}")))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut cb = arboard::Clipboard::new()
            .map_err(|e| CmError::internal(format!("clipboard unavailable: {e}")))?;
        f(&mut cb).map_err(|e| CmError::internal(format!("cannot copy to the clipboard: {e}")))
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::CmError;
    use std::io::{ErrorKind, Write};
    use std::process::{Command, Stdio};

    /// A helper program that can put data on the clipboard.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct Helper {
        pub program: &'static str,
        pub args_image: &'static [&'static str],
        pub args_text: &'static [&'static str],
    }

    const WL_COPY: Helper = Helper {
        program: "wl-copy",
        args_image: &["--type", "image/png"],
        args_text: &["--type", "text/plain"],
    };
    const XCLIP: Helper = Helper {
        program: "xclip",
        args_image: &["-selection", "clipboard", "-t", "image/png", "-i"],
        args_text: &["-selection", "clipboard", "-i"],
    };

    /// `wl-copy` first in a Wayland session, `xclip` first otherwise; the other one second.
    pub(super) fn helpers(wayland: bool) -> [Helper; 2] {
        if wayland {
            [WL_COPY, XCLIP]
        } else {
            [XCLIP, WL_COPY]
        }
    }

    fn wayland_session() -> bool {
        std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
    }

    /// Pipes `data` into the helper. Both helpers fork a background process that keeps serving
    /// the selection and the parent exits 0 once it owns it, so a zero exit is the success signal.
    /// The helper's output is not captured: the forked server would hold the pipe open.
    pub(super) fn run(h: &Helper, args: &[&str], data: &[u8]) -> Result<(), CmError> {
        let mut child = Command::new(h.program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == ErrorKind::NotFound {
                    CmError::internal(format!("{} is not installed", h.program))
                } else {
                    CmError::internal(format!("cannot run {}: {e}", h.program))
                }
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(data).map_err(|e| {
                CmError::internal(format!("{} did not take the data: {e}", h.program))
            })?;
        }
        let status = child
            .wait()
            .map_err(|e| CmError::internal(format!("{}: {e}", h.program)))?;
        if status.success() {
            Ok(())
        } else {
            Err(CmError::internal(format!(
                "{} could not set the clipboard ({status})",
                h.program
            )))
        }
    }

    fn try_helpers(image: bool, data: &[u8]) -> Result<(), CmError> {
        let mut failures = Vec::new();
        for h in helpers(wayland_session()) {
            let args = if image { h.args_image } else { h.args_text };
            match run(&h, args, data) {
                Ok(()) => return Ok(()),
                Err(e) => failures.push(e.to_string()),
            }
        }
        Err(CmError::internal(failures.join(", ")))
    }

    pub(super) fn helper_image(png: &[u8]) -> Result<(), CmError> {
        try_helpers(true, png)
    }

    pub(super) fn helper_text(text: &str) -> Result<(), CmError> {
        try_helpers(false, text.as_bytes())
    }

    /// The error when nothing worked: both reasons, and what to install.
    pub(super) fn combine(first: &CmError, helper: CmError) -> CmError {
        CmError::internal(format!(
            "cannot reach the clipboard: {first}; {helper}. Install wl-clipboard (Wayland) or xclip (X11), or run the app in a session with X11 or XWayland."
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn helper_order_follows_the_session() {
            assert_eq!(helpers(true)[0].program, "wl-copy");
            assert_eq!(helpers(false)[0].program, "xclip");
            assert_eq!(helpers(false)[1].program, "wl-copy");
        }

        #[test]
        fn a_missing_helper_is_reported_not_swallowed() {
            let h = Helper {
                program: "cm-no-such-clipboard-helper",
                args_image: &[],
                args_text: &[],
            };
            let e = run(&h, &[], b"x").unwrap_err().to_string();
            assert!(e.contains("not installed"), "{e}");
        }

        /// Puts a PNG on the real clipboard and reads it back with `CM_CLIP_READER`
        /// (`xclip` or `wl-paste`): CAP-4 on Linux. Replaces the user's clipboard, so by hand:
        /// `CM_CLIP_READER=xclip cargo test -p cm-desktop --lib real_clipboard -- --ignored`.
        #[test]
        #[ignore = "overwrites the user's clipboard"]
        fn real_clipboard_round_trip() {
            let mut png = Vec::new();
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                5,
                4,
                image::Rgba([200, 30, 60, 255]),
            ))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
            super::super::set_image(&png).unwrap();
            let out = match std::env::var("CM_CLIP_READER").as_deref() {
                Ok("wl-paste") => Command::new("wl-paste")
                    .args(["--type", "image/png"])
                    .output(),
                _ => Command::new("xclip")
                    .args(["-selection", "clipboard", "-t", "image/png", "-o"])
                    .output(),
            }
            .unwrap();
            assert!(
                out.status.success(),
                "reader failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let img = image::load_from_memory_with_format(&out.stdout, image::ImageFormat::Png)
                .unwrap()
                .to_rgba8();
            assert_eq!((img.width(), img.height()), (5, 4));
            assert_eq!(img.get_pixel(0, 0).0, [200, 30, 60, 255]);
            super::super::set_text("cm text").unwrap();
        }

        #[test]
        fn a_failing_helper_is_an_error() {
            let h = Helper {
                program: "false",
                args_image: &[],
                args_text: &[],
            };
            assert!(run(&h, &[], b"x").is_err());
        }
    }
}
