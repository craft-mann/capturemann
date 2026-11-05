//! Checks every case gets for free: no new stray product process, no ERROR or panic in the logs of its
//! private config folder, no new crash in the Windows Application log.

use crate::sys;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

/// Image names of the programs of the package (and the helpers they start).
pub const PRODUCT_PROCESSES: &[&str] = &[
    "capturemann",
    "cm-daemon",
    "cm-mcp",
    "cm-desktop",
    "cm-overlay",
    "ffmpeg",
    "ffprobe",
    "llama-server",
];

fn is_product(name: &str) -> bool {
    let lower = name.to_lowercase();
    let base = lower.strip_suffix(".exe").unwrap_or(&lower);
    PRODUCT_PROCESSES.contains(&base)
}

/// Pids of product programs running now.
pub fn product_pids() -> BTreeSet<u32> {
    sys::list_processes()
        .into_iter()
        .filter(|(_, n)| is_product(n))
        .map(|(p, _)| p)
        .collect()
}

/// Product programs that were not running before and are now: (pid, name).
pub fn strays_since(before: &BTreeSet<u32>) -> Vec<(u32, String)> {
    sys::list_processes()
        .into_iter()
        .filter(|(p, n)| is_product(n) && !before.contains(p))
        .collect()
}

/// A known harmless ERROR of the GPUI Windows backend: its vsync thread calls `RedrawWindow` on a window
/// that was destroyed a message earlier; the window leaves the tracked list one message later
/// (`gpui-pre-windows-0.3.9/src/platform.rs:493`, read in the GPUI source on Nov 5, 2026). Windows answers
/// `ERROR_INVALID_WINDOW_HANDLE` (0x80070578). It appears in a burst once per recording when the recording
/// windows close. Matched by file position AND code, counted as a WARN, never failed.
const GPUI_REDRAW_RACE: (&str, &str) = ("gpui-pre-windows-0.3.9/src/platform.rs:493", "0x80070578");

/// Lines of the logs below `config/logs` that count as a problem: panics and ERROR lines, minus the
/// allowed substrings and [`GPUI_REDRAW_RACE`]. Also returns how many WARN lines there were, the redraw race
/// lines included (reported, not failed).
pub fn scan_logs(config: &Path, allow: &[String]) -> (Vec<String>, usize) {
    let mut bad = Vec::new();
    let mut warns = 0;
    let dir = config.join("logs");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return (bad, warns);
    };
    for e in rd.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let file = e.file_name().to_string_lossy().into_owned();
        for line in text.lines() {
            let is_err = line.contains(" ERROR ")
                || line.contains("panicked at")
                || line.contains("thread '") && line.contains("panicked");
            if line.contains(" WARN ") {
                warns += 1;
            }
            if is_err && line.contains(GPUI_REDRAW_RACE.0) && line.contains(GPUI_REDRAW_RACE.1) {
                warns += 1;
                continue;
            }
            if is_err && !allow.iter().any(|a| line.contains(a.as_str())) {
                bad.push(format!("{file}: {}", crate::util::cut(line, 240)));
            }
        }
    }
    (bad, warns)
}

/// Crash records of our programs in the Windows Application log since a time. Empty elsewhere.
pub fn app_crashes_since(since: SystemTime) -> Vec<String> {
    if sys::PLATFORM != "windows" {
        return Vec::new();
    }
    let iso = crate::util::utc_iso(since).replace('Z', ".000Z");
    let query = format!(
        "*[System[(EventID=1000 or EventID=1001 or EventID=1002) and TimeCreated[@SystemTime>='{iso}']]]"
    );
    let mut c = Command::new("wevtutil");
    c.args([
        "qe",
        "Application",
        &format!("/q:{query}"),
        "/f:text",
        "/c:100",
        "/rd:true",
    ]);
    sys::hide_console(&mut c);
    let Ok(out) = c.output() else {
        return Vec::new();
    };
    parse_crash_events(&String::from_utf8_lossy(&out.stdout))
}

/// Keeps the event blocks that name one of our programs; one summary line each.
pub fn parse_crash_events(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for block in text.split("Event[").skip(1) {
        let lower = block.to_lowercase();
        let ours = [
            "cm-daemon",
            "cm-desktop",
            "cm-overlay",
            "cm-mcp",
            "capturemann",
            "llama-server",
        ]
        .iter()
        .any(|n| lower.contains(n));
        if !ours {
            continue;
        }
        let pick = |key: &str| {
            block
                .lines()
                .find(|l| l.to_lowercase().contains(key))
                .map(|l| l.trim().to_owned())
        };
        let summary = [
            pick("faulting application name"),
            pick("exception code"),
            pick("faulting module name"),
            pick("date:"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ");
        out.push(if summary.is_empty() {
            crate::util::cut(&crate::util::one_line(block), 200)
        } else {
            summary
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_names_match_with_or_without_exe() {
        assert!(is_product("cm-daemon.exe"));
        assert!(is_product("CM-OVERLAY.EXE"));
        assert!(is_product("ffmpeg"));
        assert!(!is_product("notepad.exe"));
    }

    #[test]
    fn the_gpui_redraw_race_is_counted_not_failed() {
        let dir = std::env::temp_dir().join(format!("cmr-hyg-race-{}", std::process::id()));
        let logs = dir.join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join("desktop.log"),
            "2026 desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: Invalid window handle. (0x80070578)\n2026 desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: something else\n2026 desktop ERROR other.rs:1: (0x80070578)\n",
        )
        .unwrap();
        let (bad, warns) = scan_logs(&dir, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(bad.len(), 2, "{bad:?}");
        assert_eq!(warns, 1);
    }

    #[test]
    fn logs_scan_finds_errors_and_honours_the_allow_list() {
        let d = std::env::temp_dir().join(format!("cmr-hyg-{}", std::process::id()));
        let logs = d.join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join("daemon.log"),
            "2026 INFO a fine line\n2026 WARN something odd\n2026 ERROR cm_x: it broke\n2026 ERROR cm_y: expected thing\nthread 'main' panicked at src/x.rs:1\n",
        )
        .unwrap();
        let (bad, warns) = scan_logs(&d, &["expected thing".to_owned()]);
        assert_eq!(warns, 1);
        assert_eq!(bad.len(), 2, "{bad:?}");
        assert!(bad[0].starts_with("daemon.log: "));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(scan_logs(&d, &[]), (Vec::new(), 0));
    }

    #[test]
    fn crash_events_keep_only_our_programs() {
        let text = "Event[0]:\n  Date: 2026-11-05T10:00:00.000\n  Description:\nFaulting application name: cm-daemon.exe, version: 0.2.8\nFaulting module name: ntdll.dll\nException code: 0xc0000409\nEvent[1]:\n  Date: 2026-11-05T10:01:00.000\n  Description:\nFaulting application name: chrome.exe\n";
        let v = parse_crash_events(text);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("cm-daemon.exe") && v[0].contains("0xc0000409"));
    }
}
