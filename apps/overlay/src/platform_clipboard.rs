//! Putting text on the clipboard from the picker (`C` copies the colour under the pointer,
//! CAP-3.AC4). The result is `Ok` only when the text really is on the clipboard.
//!
//! The overlay is a short-lived process. On X11 the selection lives in the process that owns it,
//! so a copy made with `arboard` vanishes when the overlay exits (there is no clipboard manager
//! to take it over on a minimal desktop). On Linux the text therefore goes first to `wl-copy` /
//! `xclip`, which fork a server that outlives the overlay; `arboard` (one handle kept for the
//! life of the process, like `apps/desktop/src/platform_clipboard.rs`, whose helper-program idea
//! this repeats in a smaller form: the overlay must not depend on the desktop crate) is the
//! fallback, and the error names both reasons and what to install.

use std::io::{ErrorKind, Write};
use std::process::{Command, Stdio};

/// A helper program that can put text on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Helper {
    program: &'static str,
    args: &'static [&'static str],
}

const WL_COPY: Helper = Helper {
    program: "wl-copy",
    args: &["--type", "text/plain"],
};
const XCLIP: Helper = Helper {
    program: "xclip",
    args: &["-selection", "clipboard", "-i"],
};

/// `wl-copy` first in a Wayland session, `xclip` first otherwise.
fn helpers(wayland: bool) -> [Helper; 2] {
    if wayland {
        [WL_COPY, XCLIP]
    } else {
        [XCLIP, WL_COPY]
    }
}

/// Pipes `text` into the helper. Both helpers fork a background server and the parent exits 0
/// once it owns the selection, so a zero exit is the success signal. Output is not captured:
/// the forked server would hold the pipe open.
fn run(h: &Helper, text: &str) -> Result<(), String> {
    let mut child = Command::new(h.program)
        .args(h.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                format!("{} is not installed", h.program)
            } else {
                format!("cannot run {}: {e}", h.program)
            }
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("{} did not take the text: {e}", h.program))?;
    }
    let status = child.wait().map_err(|e| format!("{}: {e}", h.program))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} could not set the clipboard ({status})",
            h.program
        ))
    }
}

fn try_helpers(list: &[Helper], text: &str) -> Result<(), String> {
    let mut failures = Vec::new();
    for h in list {
        match run(h, text) {
            Ok(()) => return Ok(()),
            Err(e) => failures.push(e),
        }
    }
    Err(failures.join(", "))
}

/// `arboard` with one handle for the life of the process (dropping the last `Clipboard` gives
/// the X11 selection up). A handle that failed is rebuilt on the next copy.
fn with_arboard(text: &str) -> Result<(), String> {
    use std::sync::{Mutex, PoisonError};
    static KEPT: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    if kept.is_none() {
        *kept = Some(arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?);
    }
    let Some(cb) = kept.as_mut() else {
        return Err("clipboard unavailable".into());
    };
    let result = cb.set_text(text.to_owned());
    if result.is_err() {
        *kept = None;
    }
    result.map_err(|e| format!("cannot copy to the clipboard: {e}"))
}

/// Text onto the clipboard; the error says why it did not get there.
pub fn copy_text(text: &str) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return with_arboard(text);
    }
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    if try_helpers(&helpers(wayland), text).is_ok() {
        return Ok(());
    }
    with_arboard(text).map_err(|e| {
        format!(
            "cannot reach the clipboard: {e}; no clipboard helper worked either. \
             Install wl-clipboard (Wayland) or xclip (X11)"
        )
    })
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
            args: &[],
        };
        assert!(run(&h, "x").unwrap_err().contains("not installed"));
        let all = try_helpers(&[h], "x").unwrap_err();
        assert!(all.contains("cm-no-such-clipboard-helper"), "{all}");
    }

    #[test]
    fn a_failing_helper_is_an_error() {
        let h = Helper {
            program: "false",
            args: &[],
        };
        assert!(run(&h, "x").is_err());
    }
}
