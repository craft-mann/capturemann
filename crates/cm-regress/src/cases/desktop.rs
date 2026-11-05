//! The desktop app and the overlay in a real logged-in session: start-up, the second launch, the
//! global hotkey, the recording HUD and its click-through area window, and console windows that must
//! never appear. Windows probes come from `sys` (EnumWindows, WindowFromPoint, injected keys).

use super::common::{need_ffmpeg, write_clips};
use crate::check;
use crate::ctx::{displays, Ctx};
use crate::model::{fail, skip, Case, Need, Stop, Tier, R, WIN};
use crate::proc::exit_text;
use crate::sys::{self, Key, WinInfo};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "desktop.stays_up",
            title: "cm-desktop starts, stays up for 10 s and leaves no crash event",
            area: "desktop",
            specs: &["bug: cm-desktop panic at start (tokio pipe outside a runtime)", "bug class: illegal instruction"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: WIN,
            timeout_s: 90,
            run: stays_up,
        },
        Case {
            id: "desktop.second_launch",
            title: "a second launch with a file forwards it to the running app and exits",
            area: "desktop",
            specs: &["IMP-4: second launch opens Import", "named pipe single instance"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: WIN,
            timeout_s: 120,
            run: second_launch,
        },
        Case {
            id: "desktop.hotkey_overlay",
            title: "Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it",
            area: "desktop",
            specs: &["CAP-1 overlay", "CAP-3", "DPI-WIN"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: WIN,
            timeout_s: 120,
            run: hotkey_overlay,
        },
        Case {
            id: "desktop.hud_click_through",
            title: "during a recording the HUD answers clicks and the area window is click-through",
            area: "desktop",
            specs: &["REC-5", "bug: area window swallowed clicks meant for the HUD"],
            tier: Tier::Full,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: WIN,
            timeout_s: 180,
            run: hud,
        },
        Case {
            id: "desktop.no_console_flash",
            title: "no console window appears during import, export and a screenshot",
            area: "desktop",
            specs: &["bug: console window flashes (child programs without CREATE_NO_WINDOW)"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: WIN,
            timeout_s: 240,
            run: no_console,
        },
        Case {
            id: "desktop.tray_icon",
            title: "the tray icon exists (UI Automation probe)",
            area: "desktop",
            specs: &["REC-5 tray"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: WIN,
            timeout_s: 30,
            run: tray_stub,
        },
    ]
}

/// Collects console windows that appear while it runs (polls every 20 ms).
pub struct ConsoleWatch {
    stop: Arc<AtomicBool>,
    seen: Arc<Mutex<Vec<String>>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn is_console_class(c: &str) -> bool {
    matches!(
        c,
        "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS" | "PseudoConsoleWindow"
    )
}

impl ConsoleWatch {
    pub fn start() -> ConsoleWatch {
        let stop = Arc::new(AtomicBool::new(false));
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let handle = if sys::PLATFORM == "windows" {
            let before: std::collections::BTreeSet<i64> = sys::enum_windows()
                .iter()
                .filter(|w| is_console_class(&w.class))
                .map(|w| w.hwnd)
                .collect();
            let (s, out) = (stop.clone(), seen.clone());
            Some(std::thread::spawn(move || {
                let mut known = before;
                while !s.load(Ordering::Relaxed) {
                    for w in sys::enum_windows()
                        .into_iter()
                        .filter(|w| is_console_class(&w.class))
                    {
                        if known.insert(w.hwnd) {
                            let name = sys::list_processes()
                                .into_iter()
                                .find(|(p, _)| *p == w.pid)
                                .map(|(_, n)| n)
                                .unwrap_or_default();
                            if let Ok(mut o) = out.lock() {
                                o.push(format!("{} by {name}", w.brief()));
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }))
        } else {
            None
        };
        ConsoleWatch { stop, seen, handle }
    }

    /// Stops watching; the console windows that appeared.
    pub fn finish(mut self) -> Vec<String> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.seen.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// The running app of a case.
struct App {
    pid: u32,
}

/// Starts `cm-desktop` on the sandbox endpoint and waits for its embedded daemon to answer.
fn start_app(ctx: &mut Ctx, args: &[&str]) -> R<App> {
    let mut c = ctx.product_cmd("cm-desktop");
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = c
        .spawn()
        .map_err(|e| Stop::Fail(format!("cm-desktop did not start: {e}")))?;
    let pid = child.id();
    if let Ok(mut t) = ctx.tracker.lock() {
        t.push(pid);
    }
    // The Child handle is dropped on purpose: the tracker kills the process after the case.
    drop(child);
    let ok = crate::proc::wait_for(ctx.secs(60), Duration::from_millis(500), || {
        if !alive(pid) {
            return Some(false);
        }
        ctx.daemon_answers().then_some(true)
    });
    match ok {
        Some(true) => Ok(App { pid }),
        Some(false) => fail(format!(
            "cm-desktop (pid {pid}) exited during start-up; log: {}",
            log_tail(ctx)
        )),
        None => fail(format!(
            "the app's daemon never answered on {}; log: {}",
            ctx.endpoint,
            log_tail(ctx)
        )),
    }
}

fn alive(pid: u32) -> bool {
    sys::list_processes().iter().any(|(p, _)| *p == pid)
}

fn log_tail(ctx: &Ctx) -> String {
    let mut s = String::new();
    if let Ok(rd) = std::fs::read_dir(ctx.config.join("logs")) {
        for e in rd.flatten() {
            if let Ok(t) = std::fs::read_to_string(e.path()) {
                let lines: Vec<&str> = t.lines().collect();
                s.push_str(&lines[lines.len().saturating_sub(5)..].join(" | "));
            }
        }
    }
    crate::util::cut(&s, 500)
}

fn stays_up(ctx: &mut Ctx) -> R {
    let app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(10));
    check!(
        alive(app.pid),
        "cm-desktop (pid {}) is gone after 10 s; log: {}",
        app.pid,
        log_tail(ctx)
    );
    let wins: Vec<WinInfo> = sys::enum_windows()
        .into_iter()
        .filter(|w| w.pid == app.pid && w.visible && w.width() > 50)
        .collect();
    ctx.note(format!(
        "{} visible window(s) of the app: {}",
        wins.len(),
        wins.iter()
            .map(|w| format!(
                "'{}' {}x{}",
                crate::util::cut(&w.title, 24),
                w.width(),
                w.height()
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    check!(!wins.is_empty(), "the app shows no window after start-up");
    Ok(())
}

fn second_launch(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    let app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(3));
    let before = sys::enum_windows()
        .iter()
        .filter(|w| w.pid == app.pid && w.visible)
        .count();
    let mut c = ctx.product_cmd("cm-desktop");
    c.arg(clips.join("clip-h264.mp4"));
    let out = ctx.run(c, 40)?;
    ctx.note(format!(
        "second launch: {} after {} ms",
        exit_text(out.code),
        out.elapsed_ms
    ));
    check!(
        !out.timed_out,
        "the second launch did not exit within 40 s (it should hand over and quit)"
    );
    check!(
        out.code == Some(0),
        "the second launch ended with {}",
        exit_text(out.code)
    );
    std::thread::sleep(Duration::from_secs(3));
    check!(
        alive(app.pid),
        "the first instance died when the second one forwarded a file"
    );
    let after = sys::enum_windows()
        .iter()
        .filter(|w| w.pid == app.pid && w.visible)
        .count();
    ctx.note(format!(
        "visible windows of the first instance: {before} before, {after} after"
    ));
    let others = sys::list_processes()
        .iter()
        .filter(|(p, n)| *p != app.pid && n.to_lowercase().starts_with("cm-desktop"))
        .count();
    check!(others == 0, "{others} other cm-desktop process(es) are still running: the second launch did not hand over");
    Ok(())
}

fn overlay_windows() -> Vec<WinInfo> {
    let procs = sys::list_processes();
    let overlay_pids: Vec<u32> = procs
        .iter()
        .filter(|(_, n)| n.to_lowercase().starts_with("cm-overlay"))
        .map(|(p, _)| *p)
        .collect();
    sys::enum_windows()
        .into_iter()
        .filter(|w| {
            overlay_pids.contains(&w.pid) && w.visible && w.width() > 100 && w.height() > 100
        })
        .collect()
}

fn hotkey_overlay(ctx: &mut Ctx) -> R {
    let _app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(4));
    let t = ctx.cli_ok(&["targets", "--displays-only"])?;
    let ds = displays(&t);
    check!(!ds.is_empty(), "no displays");
    sys::send_chord(&[Key::Ctrl, Key::Alt, Key::Shift, Key::Char('1')]).map_err(Stop::Fail)?;
    let found = crate::proc::wait_for(ctx.secs(10), Duration::from_millis(250), || {
        let w = overlay_windows();
        (w.len() >= ds.len()).then_some(w)
    });
    let wins = found.unwrap_or_else(overlay_windows);
    for w in &wins {
        ctx.note(w.brief());
    }
    let mut problems = Vec::new();
    if wins.len() != ds.len() {
        problems.push(format!(
            "{} overlay window(s) for {} monitor(s)",
            wins.len(),
            ds.len()
        ));
    }
    for d in &ds {
        let want = [
            d.x as i32,
            d.y as i32,
            (d.x + d.w) as i32,
            (d.y + d.h) as i32,
        ];
        let hit = wins.iter().any(|w| {
            w.rect
                .iter()
                .zip(want.iter())
                .all(|(a, b)| (a - b).abs() <= 2)
        });
        if !hit {
            problems.push(format!(
                "no overlay window on display {} rectangle {:?}",
                d.id, want
            ));
        }
    }
    sys::send_chord(&[Key::Esc]).map_err(Stop::Fail)?;
    let gone = crate::proc::wait_for(ctx.secs(10), Duration::from_millis(250), || {
        overlay_windows().is_empty().then_some(())
    });
    if gone.is_none() {
        problems.push("the overlay is still open 10 s after Esc".to_owned());
    }
    let left = sys::list_processes()
        .iter()
        .filter(|(_, n)| n.to_lowercase().starts_with("cm-overlay"))
        .count();
    if left > 0 {
        std::thread::sleep(Duration::from_secs(3));
        let still = sys::list_processes()
            .iter()
            .filter(|(_, n)| n.to_lowercase().starts_with("cm-overlay"))
            .count();
        if still > 0 {
            problems.push(format!(
                "{still} cm-overlay process(es) still run after Esc"
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}

fn hud(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    let app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(3));
    let d = ctx.primary_display()?;
    let id = d.id.to_string();
    ctx.cli_ok(&[
        "record",
        "start",
        "--display",
        &id,
        "--countdown",
        "0",
        "--size",
        "720p",
    ])?;
    std::thread::sleep(Duration::from_secs(4));
    let all: Vec<WinInfo> = sys::enum_windows()
        .into_iter()
        .filter(|w| w.pid == app.pid && w.visible && w.width() > 0 && w.height() > 0)
        .collect();
    for w in &all {
        ctx.note(w.brief());
    }
    let (area, rest): (Vec<&WinInfo>, Vec<&WinInfo>) = all
        .iter()
        .partition(|w| i64::from(w.width()) >= d.w - 60 && i64::from(w.height()) >= d.h - 60);
    let hud = rest
        .iter()
        .filter(|w| w.ex_style & sys::WS_EX_TOPMOST != 0 && w.width() <= 900 && w.height() <= 300)
        .min_by_key(|w| w.width() * w.height())
        .copied();
    let mut problems = Vec::new();
    let consoles = ConsoleWatch::start();
    match hud {
        None => problems
            .push("no HUD window (small topmost window of the app) while recording".to_owned()),
        Some(h) => {
            let (cx, cy) = ((h.rect[0] + h.rect[2]) / 2, (h.rect[1] + h.rect[3]) / 2);
            match sys::window_from_point(cx, cy) {
                Some(w) if w.hwnd == h.hwnd => {
                    ctx.note(format!("WindowFromPoint({cx},{cy}) is the HUD"))
                }
                Some(w) => problems.push(format!(
                    "a click at the HUD centre ({cx},{cy}) lands on another window: {}",
                    w.brief()
                )),
                None => problems.push("WindowFromPoint found nothing at the HUD centre".to_owned()),
            }
        }
    }
    if area.is_empty() {
        problems.push(format!(
            "no area window covering the recorded display {}x{}",
            d.w, d.h
        ));
    }
    for a in &area {
        if a.ex_style & sys::WS_EX_TRANSPARENT == 0 {
            problems.push(format!(
                "the area window is not click-through (WS_EX_TRANSPARENT missing): {}",
                a.brief()
            ));
        }
    }
    let stop = ctx.cli_long(&["record", "stop"], 90)?;
    let seen = consoles.finish();
    if !stop.ok() {
        problems.push(format!("record stop failed: {}", exit_text(stop.code())));
    }
    if !seen.is_empty() {
        problems.push(format!(
            "console windows appeared while stopping: {}",
            seen.join("; ")
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}

fn no_console(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let watch = ConsoleWatch::start();
    let r = (|| -> R {
        let (id, _) = super::common::import_file(ctx, &clips.join("clip-h264.mp4"))?;
        let r = ctx.cli_long(
            &[
                "export",
                &id,
                "--preset",
                "quick",
                "--wait",
                "--timeout",
                "120",
            ],
            150,
        )?;
        check!(
            r.ok(),
            "export failed: {}",
            crate::util::cut(&r.out.text(), 200)
        );
        let d = ctx.primary_display()?;
        let did = d.id.to_string();
        let out = ctx.root.join("flash.png").to_string_lossy().into_owned();
        ctx.cli_ok(&["capture", "screenshot", "--display", &did, "--out", &out])?;
        let _ = ctx.cli_text(&["diagnostics"])?;
        Ok(())
    })();
    let seen = watch.finish();
    r?;
    check!(
        seen.is_empty(),
        "console window(s) appeared: {}",
        seen.join("; ")
    );
    Ok(())
}

fn tray_stub(_ctx: &mut Ctx) -> R {
    skip("not implemented: the tray icon lives in the hidden-icons overflow and needs a UI Automation probe; see tests/manual/REC-5.md")
}
