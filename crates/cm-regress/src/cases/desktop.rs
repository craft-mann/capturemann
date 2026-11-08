//! The desktop app and the overlay in a real logged-in session: start-up, the second launch, the
//! global hotkey, the recording HUD and its click-through area window, and console windows that must
//! never appear. Windows probes come from `sys` (EnumWindows, WindowFromPoint, injected keys).

use super::common::{need_ffmpeg, write_clips};
use crate::check;
use crate::ctx::{displays, Ctx};
use crate::model::{fail, skip, Case, Need, Stop, Tier, R, WIN, WIN_MAC};
use crate::proc::exit_text;
use crate::sys::{self, Key, WinInfo};
use serde_json::Value;
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
            platforms: WIN_MAC,
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
            platforms: WIN_MAC,
            timeout_s: 120,
            run: second_launch,
        },
        Case {
            id: "desktop.hotkey_overlay",
            title: "the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it",
            area: "desktop",
            specs: &["CAP-1 overlay", "CAP-3", "DPI-WIN"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: WIN_MAC,
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
            id: "desktop.settings_applied",
            title: "settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long",
            area: "desktop",
            specs: &["bug: settings set not applied to the running app", "AGT-1"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            // A second launch forwards `--capture` on Windows and Linux only; on macOS it just shows the running app.
            platforms: WIN,
            timeout_s: 120,
            run: settings_applied,
        },
        Case {
            id: "desktop.recording_placement",
            title: "countdown, area window and HUD of a recording of the external monitor appear on that monitor",
            area: "desktop",
            specs: &["REC-5", "DPI-WIN", "bug: countdown and toasts on the other monitor"],
            tier: Tier::Full,
            needs: &[Need::DesktopSession, Need::Screen, Need::SecondMonitor],
            platforms: WIN,
            timeout_s: 180,
            run: placement,
        },
        Case {
            id: "desktop.tray_icon",
            title: "the tray icon appears in the notification area while the app runs (UI Automation probe)",
            area: "desktop",
            specs: &["REC-5 tray"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: WIN_MAC,
            timeout_s: 30,
            run: tray_icon,
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
    tracker: crate::proc::Tracker,
}

impl Drop for App {
    fn drop(&mut self) {
        sys::kill_tree(self.pid);
        if let Ok(mut t) = self.tracker.lock() {
            t.retain(|p| *p != self.pid);
        }
    }
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
        Some(true) => Ok(App {
            pid,
            tracker: ctx.tracker.clone(),
        }),
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
    // A macOS app may live in the menu bar only: a window or a menu-bar icon counts as "shows something".
    let icons = if sys::PLATFORM == "macos" {
        sys::status_item_count(app.pid).unwrap_or(0)
    } else {
        0
    };
    check!(
        !wins.is_empty() || icons > 0,
        "the app shows no window and no menu-bar icon after start-up"
    );
    Ok(())
}

fn second_launch(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    let baseline = crate::hygiene::product_pids();
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
        .filter(|(p, n)| {
            *p != app.pid && !baseline.contains(p) && n.to_lowercase().starts_with("cm-desktop")
        })
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
    // Another CaptureMann (the maintainer's own, or one left by an earlier run) holds the same global
    // hotkey and opens its own overlay too, so the case would count two overlays for one monitor.
    let others: Vec<u32> = sys::list_processes()
        .iter()
        .filter(|(_, n)| n.to_lowercase().starts_with("cm-desktop"))
        .map(|(p, _)| *p)
        .collect();
    if !others.is_empty() {
        return skip(format!(
            "another CaptureMann is running (pid {others:?}) and answers the same hotkey: quit it, then run this case"
        ));
    }
    let _app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(4));
    let t = ctx.cli_ok(&["targets", "--displays-only"])?;
    let ds = displays(&t);
    check!(!ds.is_empty(), "no displays");
    let chord: &[Key] = if sys::PLATFORM == "macos" {
        &[Key::Alt, Key::Shift, Key::Char('1')]
    } else {
        &[Key::Ctrl, Key::Alt, Key::Shift, Key::Char('1')]
    };
    if let Err(e) = sys::send_chord(chord) {
        return if sys::PLATFORM == "macos" {
            skip(format!("keys cannot be injected: {e}"))
        } else {
            fail(e)
        };
    }
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
        // macOS lists window bounds in points; the target list holds physical pixels.
        let k = if sys::PLATFORM == "macos" && d.scale > 0.0 {
            d.scale
        } else {
            1.0
        };
        let px = |v: i64| (v as f64 / k).round() as i32;
        let want = [px(d.x), px(d.y), px(d.x + d.w), px(d.y + d.h)];
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
    // The automatic export after the stop starts an ffmpeg; let it finish so it is not left behind.
    super::record::wait_idle_exports(ctx);
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

/// Counts the notification-area buttons named exactly "CaptureMann" with UI Automation: the Windows 11
/// tray keeps the icon in the "Show Hidden Icons" overflow, so the probe opens that flyout, reads it and
/// closes it again (Esc). The taskbar's own task button is named "CaptureMann - 1 running window" and
/// does not count. Prints `TRAY <n> <names>`.
const TRAY_PROBE: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class CmKey { [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint fl, UIntPtr e); }'
$A = [System.Windows.Automation.AutomationElement]; $TS = [System.Windows.Automation.TreeScope]
$all = [System.Windows.Automation.Condition]::TrueCondition
$root = $A::RootElement
$names = New-Object System.Collections.Generic.List[string]
function Scan($el) { foreach ($e in $el.FindAll($TS::Descendants, $all)) { if ($e.Current.Name -eq 'CaptureMann' -and $e.Current.ControlType.ProgrammaticName -eq 'ControlType.Button') { $names.Add('button') } } }
$tb = $root.FindFirst($TS::Children, (New-Object System.Windows.Automation.PropertyCondition($A::ClassNameProperty, 'Shell_TrayWnd')))
if (-not $tb) { 'TRAY -1 no taskbar'; exit 0 }
Scan $tb
$more = $tb.FindAll($TS::Descendants, $all) | Where-Object { $_.Current.Name -eq 'Show Hidden Icons' } | Select-Object -First 1
if ($more) {
  $ip = $null
  if ($more.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$ip)) {
    $ip.Invoke(); Start-Sleep -Milliseconds 1500
    foreach ($c in $root.FindAll($TS::Children, $all)) { if ($c.Current.ClassName -match 'Overflow') { Scan $c } }
    [CmKey]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80; [CmKey]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
  }
}
"TRAY $($names.Count) $($names -join ',')"
"#;

fn tray_count(ctx: &mut Ctx) -> R<i64> {
    let script = ctx.root.join("tray-probe.ps1");
    std::fs::write(&script, TRAY_PROBE)
        .map_err(|e| Stop::Fail(format!("could not write the probe script: {e}")))?;
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script);
    let out = ctx.run(c, 60)?;
    let text = out.text();
    let line = text
        .lines()
        .find(|l| l.starts_with("TRAY "))
        .unwrap_or_default();
    ctx.note(format!(
        "tray probe: {} {}",
        exit_text(out.code),
        crate::util::cut(&crate::util::one_line(&text), 160)
    ));
    line.split_whitespace()
        .nth(1)
        .and_then(|n| n.parse::<i64>().ok())
        .ok_or_else(|| Stop::Fail(format!("the tray probe printed no result: {text}")))
}

fn menu_bar_icon(ctx: &mut Ctx) -> R {
    let app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(6));
    let Some(n) = sys::status_item_count(app.pid) else {
        return skip("the menu-bar probe needs the Accessibility permission for the program that runs the suite");
    };
    ctx.note(format!("menu-bar items of the app: {n}"));
    check!(
        alive(app.pid),
        "cm-desktop (pid {}) is gone after the probe",
        app.pid
    );
    check!(n > 0, "the app shows no icon in the menu bar");
    Ok(())
}

fn tray_icon(ctx: &mut Ctx) -> R {
    if sys::PLATFORM == "macos" {
        return menu_bar_icon(ctx);
    }
    if sys::PLATFORM != "windows" {
        return skip("the tray probe uses Windows UI Automation");
    }
    // Icons of other CaptureMann programs already running (the maintainer's app) are the baseline.
    let before = tray_count(ctx)?;
    if before < 0 {
        return skip("the machine shows no taskbar (Shell_TrayWnd)");
    }
    let app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(6));
    let after = tray_count(ctx)?;
    ctx.note(format!(
        "tray buttons named CaptureMann: {before} before the app, {after} with it"
    ));
    check!(
        alive(app.pid),
        "cm-desktop (pid {}) is gone after the probe",
        app.pid
    );
    check!(
        after > before,
        "no new CaptureMann icon in the notification area ({before} before, {after} with the app running)"
    );
    Ok(())
}

fn settings_applied(ctx: &mut Ctx) -> R {
    let _app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(3));
    ctx.cli_ok(&["settings", "set", "capture.delay_s", "5"])?;
    let g = ctx.cli_ok(&["settings", "get", "capture.delay_s"])?;
    check!(
        g.pointer("/settings/capture.delay_s")
            .and_then(Value::as_i64)
            == Some(5),
        "settings get does not return 5: {g}"
    );
    // The app is told through settings.changed; a display capture asked for by a second launch must wait.
    let t0 = std::time::Instant::now();
    let mut c = ctx.product_cmd("cm-desktop");
    c.args(["--capture", "display"]);
    let out = ctx.run(c, 30)?;
    ctx.note(format!(
        "forwarded capture request: {}",
        exit_text(out.code)
    ));
    let appeared = crate::proc::wait_for(ctx.secs(30), Duration::from_millis(300), || {
        let v = ctx
            .cli(&["library", "list", "--kind", "screenshot"])
            .ok()?
            .json?;
        v.get("captures")?
            .as_array()
            .filter(|a| !a.is_empty())
            .map(|_| t0.elapsed())
    });
    let Some(waited) = appeared else {
        return fail("no screenshot appeared within 30 s of the forwarded display capture request");
    };
    ctx.note(format!(
        "the screenshot appeared after {:.1} s with capture.delay_s = 5",
        waited.as_secs_f64()
    ));
    check!(
        waited.as_secs_f64() >= 4.5,
        "the capture happened after {:.1} s although capture.delay_s was set to 5: the running app did not apply the setting",
        waited.as_secs_f64()
    );
    Ok(())
}

fn centre(w: &WinInfo) -> (i32, i32) {
    ((w.rect[0] + w.rect[2]) / 2, (w.rect[1] + w.rect[3]) / 2)
}

fn inside(w: &WinInfo, d: &crate::ctx::Display) -> bool {
    let (cx, cy) = centre(w);
    i64::from(cx) >= d.x
        && i64::from(cx) < d.x + d.w
        && i64::from(cy) >= d.y
        && i64::from(cy) < d.y + d.h
}

fn placement(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    let app = start_app(ctx, &[])?;
    std::thread::sleep(Duration::from_secs(3));
    let t = ctx.cli_ok(&["targets", "--displays-only"])?;
    let ds = displays(&t);
    let Some(ext) = ds.iter().find(|d| !d.primary).cloned() else {
        return skip("only one display is connected");
    };
    for d in &ds {
        ctx.note(format!(
            "display {} '{}' ({},{}) {}x{} scale {}{}",
            d.id,
            d.name,
            d.x,
            d.y,
            d.w,
            d.h,
            d.scale,
            if d.primary { " primary" } else { "" }
        ));
    }
    let id = ext.id.to_string();
    ctx.cli_ok(&[
        "record",
        "start",
        "--display",
        &id,
        "--countdown",
        "3",
        "--size",
        "720p",
    ])?;
    // Sample the app's small windows every 100 ms through the countdown.
    let mut small: Vec<WinInfo> = Vec::new();
    let end = std::time::Instant::now() + Duration::from_millis(3200);
    while std::time::Instant::now() < end {
        for w in sys::enum_windows() {
            let big = w.width() > 1200 || w.height() > 700;
            if w.pid == app.pid
                && w.visible
                && !big
                && w.width() > 40
                && !small.iter().any(|s| s.hwnd == w.hwnd)
            {
                small.push(w);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for w in &small {
        ctx.note(format!(
            "countdown phase: {} -> {}",
            w.brief(),
            if inside(w, &ext) {
                "on the recorded monitor"
            } else {
                "ELSEWHERE"
            }
        ));
    }
    std::thread::sleep(Duration::from_secs(2));
    let all: Vec<WinInfo> = sys::enum_windows()
        .into_iter()
        .filter(|w| w.pid == app.pid && w.visible && w.width() > 0)
        .collect();
    let mut problems = Vec::new();
    if small.is_empty() {
        problems.push("no countdown window was seen".to_owned());
    }
    for w in small.iter().filter(|w| !inside(w, &ext)) {
        problems.push(format!(
            "a countdown-phase window is not on the recorded monitor: {}",
            w.brief()
        ));
    }
    let area: Vec<&WinInfo> = all
        .iter()
        .filter(|w| i64::from(w.width()) >= ext.w - 80 && i64::from(w.height()) >= ext.h - 80)
        .collect();
    for a in &all {
        ctx.note(format!("recording phase: {}", a.brief()));
    }
    match area.first() {
        None => problems.push(format!(
            "no area window covers the recorded monitor {}x{}",
            ext.w, ext.h
        )),
        Some(a) => {
            let off = [
                a.rect[0] - ext.x as i32,
                a.rect[1] - ext.y as i32,
                a.rect[2] - (ext.x + ext.w) as i32,
                a.rect[3] - (ext.y + ext.h) as i32,
            ];
            if off.iter().any(|o| o.abs() > 20) {
                problems.push(format!(
                    "the area window is not the display frame: offsets {off:?}"
                ));
            }
        }
    }
    let hud = all
        .iter()
        .filter(|w| w.ex_style & sys::WS_EX_TOPMOST != 0 && w.width() <= 900 && w.height() <= 300)
        .min_by_key(|w| w.width() * w.height());
    match hud {
        None => problems.push("no HUD window while recording".to_owned()),
        Some(h) => {
            let inside_frame = h.rect[0] >= ext.x as i32
                && h.rect[1] >= ext.y as i32
                && h.rect[2] <= (ext.x + ext.w) as i32
                && h.rect[3] <= (ext.y + ext.h) as i32;
            if !inside_frame {
                problems.push(format!(
                    "the HUD is not inside the recorded display: {}",
                    h.brief()
                ));
            }
        }
    }
    let stop = ctx.cli_long(&["record", "stop"], 90)?;
    super::record::wait_idle_exports(ctx);
    if !stop.ok() {
        problems.push(format!("record stop failed: {}", exit_text(stop.code())));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}
