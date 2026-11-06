//! macOS: processes through `ps`/`pgrep`/`kill`, windows and displays through JavaScript for Automation
//! (`osascript -l JavaScript`, which reads CoreGraphics without any permission for bounds, owner and layer),
//! injected keys and the menu-bar probe through System Events (needs Accessibility; without it the probes
//! answer "not available" and the cases report SKIP), and the app-bundle / DMG install check through
//! `hdiutil`, `ditto`, `codesign` and `plutil`.
//!
//! Every function takes a [`Runner`], so the unit tests drive them with a scripted fake and never run a real
//! command. The functions the rest of the suite calls (`pub fn` without a runner argument at the bottom) bind
//! the real runner. Nothing here touches `/Applications`: installs go to the folder the case gives.

use super::{Key, WinInfo, WS_EX_TOPMOST};
use std::path::Path;
use std::process::Command;

pub const PLATFORM: &str = "macos";

/// What a command printed and how it ended.
#[derive(Debug, Clone, Default)]
pub struct CmdOut {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOut {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs a program and returns what it printed; `Err` when it could not be started at all.
pub trait Runner {
    fn run(&self, prog: &str, args: &[&str]) -> Result<CmdOut, String>;
}

/// The real thing.
pub struct RealRunner;

impl Runner for RealRunner {
    fn run(&self, prog: &str, args: &[&str]) -> Result<CmdOut, String> {
        let out = Command::new(prog)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("{prog} could not start: {e}"))?;
        Ok(CmdOut {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

pub fn hide_console(_cmd: &mut Command) {}

// ---- processes ----

pub fn parse_ps(text: &str) -> Vec<(u32, String)> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let (pid, name) = l.split_once(' ')?;
            let name = name.trim().rsplit('/').next()?.to_owned();
            Some((pid.parse().ok()?, name))
        })
        .collect()
}

pub fn list_processes_with(r: &dyn Runner) -> Vec<(u32, String)> {
    match r.run("ps", &["-A", "-o", "pid=,comm="]) {
        Ok(o) => parse_ps(&o.stdout),
        Err(_) => Vec::new(),
    }
}

/// Children first, then the process itself.
pub fn kill_tree_with(r: &dyn Runner, pid: u32) {
    if let Ok(o) = r.run("pgrep", &["-P", &pid.to_string()]) {
        for child in o.stdout.split_whitespace() {
            if let Ok(c) = child.parse::<u32>() {
                kill_tree_with(r, c);
            }
        }
    }
    let _ = r.run("kill", &["-9", &pid.to_string()]);
}

// ---- windows and displays (CoreGraphics through JXA) ----

/// Prints every window as JSON: number, owner pid and name, title, bounds in points, layer, alpha, on screen.
/// Window titles need the Screen Recording permission; everything else does not. The list is ordered front to
/// back, which `window_from_point_in` relies on.
const WINDOWS_JXA: &str = r#"ObjC.import('CoreGraphics');
var a = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(0, 0)));
JSON.stringify(a.map(function (w) { var b = w.kCGWindowBounds || {};
  return {id: w.kCGWindowNumber, pid: w.kCGWindowOwnerPID, owner: w.kCGWindowOwnerName || '', name: w.kCGWindowName || '',
          x: b.X, y: b.Y, w: b.Width, h: b.Height, layer: w.kCGWindowLayer, alpha: w.kCGWindowAlpha, on: !!w.kCGWindowIsOnscreen}; }))"#;

/// Prints every display's frame in the top-left-origin coordinates the windows use.
const DISPLAYS_JXA: &str = r#"ObjC.import('AppKit');
var s = $.NSScreen.screens; var n = parseInt(s.count); var out = [];
var f0 = s.objectAtIndex(0).frame; var top = f0.origin.y + f0.size.height;
for (var i = 0; i < n; i++) { var f = s.objectAtIndex(i).frame;
  out.push({x: f.origin.x, y: top - f.origin.y - f.size.height, w: f.size.width, h: f.size.height}); }
JSON.stringify(out)"#;

fn jxa(r: &dyn Runner, script: &str) -> Option<String> {
    let o = r
        .run("osascript", &["-l", "JavaScript", "-e", script])
        .ok()?;
    o.ok().then_some(o.stdout)
}

/// Reads the JSON of [`WINDOWS_JXA`]. Windows without bounds are dropped.
pub fn parse_windows(json: &str) -> Vec<WinInfo> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json.trim()) else {
        return Vec::new();
    };
    let Some(a) = v.as_array() else {
        return Vec::new();
    };
    a.iter()
        .filter_map(|w| {
            let num = |k: &str| w.get(k).and_then(serde_json::Value::as_f64);
            let (x, y, wd, ht) = (num("x")?, num("y")?, num("w")?, num("h")?);
            let layer = num("layer").unwrap_or(0.0);
            let alpha = num("alpha").unwrap_or(1.0);
            let on = w.get("on").and_then(serde_json::Value::as_bool) == Some(true);
            let text = |k: &str| {
                w.get(k)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            Some(WinInfo {
                hwnd: num("id")? as i64,
                pid: num("pid")? as u32,
                class: text("owner"),
                title: text("name"),
                rect: [
                    x.round() as i32,
                    y.round() as i32,
                    (x + wd).round() as i32,
                    (y + ht).round() as i32,
                ],
                // Normal windows are layer 0; floating panels, the menu bar and overlays are above it.
                ex_style: if layer > 0.0 { WS_EX_TOPMOST } else { 0 },
                visible: on && alpha > 0.0,
            })
        })
        .collect()
}

pub fn enum_windows_with(r: &dyn Runner) -> Vec<WinInfo> {
    jxa(r, WINDOWS_JXA)
        .map(|j| parse_windows(&j))
        .unwrap_or_default()
}

/// The frontmost visible window that contains the point (the list is front to back).
pub fn window_from_point_in(wins: &[WinInfo], x: i32, y: i32) -> Option<WinInfo> {
    wins.iter()
        .find(|w| {
            w.visible
                && w.width() > 0
                && w.height() > 0
                && x >= w.rect[0]
                && x < w.rect[2]
                && y >= w.rect[1]
                && y < w.rect[3]
        })
        .cloned()
}

/// Display frames `(left, top, right, bottom)` in points, `None` when they could not be read.
pub fn parse_displays(json: &str) -> Option<Vec<[i32; 4]>> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    let a = v.as_array()?;
    a.iter()
        .map(|d| {
            let n = |k: &str| d.get(k).and_then(serde_json::Value::as_f64);
            let (x, y, w, h) = (n("x")?, n("y")?, n("w")?, n("h")?);
            Some([
                x.round() as i32,
                y.round() as i32,
                (x + w).round() as i32,
                (y + h).round() as i32,
            ])
        })
        .collect()
}

pub fn list_displays_with(r: &dyn Runner) -> Option<Vec<[i32; 4]>> {
    parse_displays(&jxa(r, DISPLAYS_JXA)?)
}

/// Whether the screen is locked, from the text of `ioreg -r -k IOConsoleUsers -d 1`.
pub fn screen_locked(ioreg: &str) -> bool {
    ioreg.contains("\"CGSSessionScreenIsLocked\"=Yes")
}

/// A locked screen, or no display at all (a display that is off lists none), means no desktop session for
/// the cases. When the display list cannot be read the answer stays "yes": the case then fails or skips with
/// its own reason.
pub fn has_desktop_with(r: &dyn Runner) -> bool {
    if let Ok(o) = r.run("ioreg", &["-r", "-k", "IOConsoleUsers", "-d", "1"]) {
        if screen_locked(&o.stdout) {
            return false;
        }
    }
    !matches!(list_displays_with(r), Some(d) if d.is_empty())
}

// ---- injected keys (System Events, needs Accessibility) ----

/// The virtual key code of a letter or digit on a US layout (the physical key the shortcut names).
fn key_code(c: char) -> Option<u8> {
    const LETTERS: [u8; 26] = [
        0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16,
        6,
    ];
    const DIGITS: [u8; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
    let c = c.to_ascii_lowercase();
    match c {
        'a'..='z' => Some(LETTERS[(c as u8 - b'a') as usize]),
        '0'..='9' => Some(DIGITS[(c as u8 - b'0') as usize]),
        _ => None,
    }
}

/// The AppleScript for a chord: modifiers held while the last non-modifier key is pressed.
pub fn chord_script(keys: &[Key]) -> Result<String, String> {
    let mut mods: Vec<&str> = Vec::new();
    let mut base: Option<String> = None;
    for k in keys {
        match k {
            Key::Ctrl => mods.push("control down"),
            Key::Alt => mods.push("option down"),
            Key::Shift => mods.push("shift down"),
            Key::Esc => base = Some("key code 53".to_owned()),
            // A physical key, not the typed character: with Shift held `keystroke "1"` types "!" and
            // the app's global hotkey (Option+Shift+1) never fires.
            Key::Char(c) => match key_code(*c) {
                Some(code) => base = Some(format!("key code {code}")),
                None => return Err(format!("no key mapping for '{c}'")),
            },
        }
    }
    let base = base.ok_or("a chord needs one key besides the modifiers")?;
    let using = if mods.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", mods.join(", "))
    };
    Ok(format!(
        "tell application \"System Events\" to {base}{using}"
    ))
}

pub fn send_chord_with(r: &dyn Runner, keys: &[Key]) -> Result<(), String> {
    let script = chord_script(keys)?;
    let o = r.run("osascript", &["-e", &script])?;
    if o.ok() {
        Ok(())
    } else {
        Err(format!(
            "System Events refused the key press (grant Accessibility to the program that runs the suite): {}",
            o.stderr.trim()
        ))
    }
}

// ---- the menu-bar icon ----

/// How many status items (menu-bar icons) the process with this pid shows, read through System Events
/// (the app's own status items live in `menu bar 2`). `None` when the probe is not possible: no Accessibility
/// permission, or System Events does not list the process.
pub fn status_item_count_with(r: &dyn Runner, pid: u32) -> Option<usize> {
    let script = format!(
        "tell application \"System Events\" to tell (first process whose unix id is {pid}) to count of menu bar items of menu bar 2"
    );
    let o = r.run("osascript", &["-e", &script]).ok()?;
    if !o.ok() {
        return None;
    }
    o.stdout.trim().parse().ok()
}

// ---- crash reports ----

/// Names of crash reports of our programs among `(file name, modified seconds since the epoch)` entries that
/// were written at or after `since` (also seconds since the epoch).
pub fn filter_crash_reports(entries: &[(String, u64)], since: u64) -> Vec<String> {
    entries
        .iter()
        .filter(|(name, t)| {
            let lower = name.to_lowercase();
            let ours = ["cm-", "capturemann", "llama-server"]
                .iter()
                .any(|p| lower.starts_with(p));
            let report = lower.ends_with(".ips") || lower.ends_with(".crash");
            ours && report && *t >= since
        })
        .map(|(n, _)| n.clone())
        .collect()
}

/// Crash reports in `~/Library/Logs/DiagnosticReports` written since a time (also its `Retired` subfolder is
/// not read: macOS moves old reports there).
pub fn crash_reports_since(dir: &Path, since: std::time::SystemTime) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let secs = |t: std::time::SystemTime| {
        t.duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    };
    let entries: Vec<(String, u64)> = rd
        .flatten()
        .filter_map(|e| {
            let t = e.metadata().ok()?.modified().ok()?;
            Some((e.file_name().to_string_lossy().into_owned(), secs(t)))
        })
        .collect();
    filter_crash_reports(&entries, secs(since))
}

// ---- the app bundle and the DMG ----

/// The programs the app bundle must hold in `Contents/MacOS`.
pub const BUNDLE_PROGRAMS: &[&str] = &["capturemann", "cm-desktop", "cm-overlay", "cm-mcp"];

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The first `*.app` folder name in a `ls` of a mounted image.
pub fn find_app(ls: &str) -> Option<String> {
    ls.lines()
        .map(str::trim)
        .find(|l| l.ends_with(".app"))
        .map(str::to_owned)
}

/// Whether `hdiutil info` still lists the mount point.
pub fn still_mounted(hdiutil_info: &str, mountpoint: &str) -> bool {
    hdiutil_info.lines().any(|l| l.contains(mountpoint))
}

/// Mounts a DMG read-only under `work/mnt` (not in `/Volumes`, no Finder window), copies the app out of it
/// into `work/CaptureMann.app`, ejects, and checks the copy: the signature is intact (`codesign --verify
/// --deep --strict`), the bundle id and version are readable, the programs are in place and
/// `capturemann --version` works. Then removes the copy and checks nothing is left (no copy, no mount).
/// `source` may also be a `.app` folder (then there is no mount). Returns one note per step. Never touches
/// `/Applications`, never opens the app.
pub fn check_app_install(
    r: &dyn Runner,
    source: &Path,
    work: &Path,
) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let copy = work.join("CaptureMann.app");
    let mnt = work.join("mnt");
    let is_dmg = source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dmg"));
    let step = |what: &str, o: Result<CmdOut, String>| -> Result<CmdOut, String> {
        let o = o?;
        if o.ok() {
            Ok(o)
        } else {
            Err(format!(
                "{what} ended with {:?}: {}",
                o.code,
                crate::util::cut(o.stderr.trim(), 200)
            ))
        }
    };
    let result = (|| -> Result<(), String> {
        let app_src = if is_dmg {
            std::fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
            step(
                "hdiutil attach",
                r.run(
                    "hdiutil",
                    &[
                        "attach",
                        "-nobrowse",
                        "-readonly",
                        "-noautoopen",
                        "-mountpoint",
                        &s(&mnt),
                        &s(source),
                    ],
                ),
            )?;
            let ls = step("ls of the image", r.run("ls", &[&s(&mnt)]))?;
            let app = find_app(&ls.stdout).ok_or("the disk image holds no .app")?;
            notes.push(format!("mounted {} ({app})", source.display()));
            mnt.join(app)
        } else {
            source.to_path_buf()
        };
        step("ditto", r.run("ditto", &[&s(&app_src), &s(&copy)]))?;
        notes.push(format!("copied the app to {}", copy.display()));
        Ok(())
    })();
    // Always eject, also after a failed copy.
    if is_dmg {
        let d = r.run("hdiutil", &["detach", "-force", &s(&mnt)]);
        let detached = matches!(&d, Ok(o) if o.ok());
        notes.push(if detached {
            "ejected the image".to_owned()
        } else {
            "the image did not eject".to_owned()
        });
        if let Ok(info) = r.run("hdiutil", &["info"]) {
            if still_mounted(&info.stdout, &s(&mnt)) {
                let _ = r.run("rm", &["-rf", &s(&copy)]);
                return Err(format!(
                    "{} is still mounted after the detach",
                    mnt.display()
                ));
            }
        }
    }
    if let Err(e) = result {
        let _ = r.run("rm", &["-rf", &s(&copy)]);
        return Err(e);
    }
    let checked = (|| -> Result<(), String> {
        step(
            "codesign --verify --deep --strict",
            r.run("codesign", &["--verify", "--deep", "--strict", &s(&copy)]),
        )?;
        notes.push("the code signature verifies (deep, strict)".to_owned());
        let plist = copy.join("Contents/Info.plist");
        let read = |key: &str| -> Result<String, String> {
            let o = step(
                "plutil",
                r.run("plutil", &["-extract", key, "raw", "-o", "-", &s(&plist)]),
            )?;
            Ok(o.stdout.trim().to_owned())
        };
        let (id, ver) = (
            read("CFBundleIdentifier")?,
            read("CFBundleShortVersionString")?,
        );
        if id.is_empty() || ver.is_empty() {
            return Err(format!(
                "Info.plist has an empty id or version ('{id}', '{ver}')"
            ));
        }
        notes.push(format!("bundle id {id}, version {ver}"));
        for p in BUNDLE_PROGRAMS {
            let path = copy.join("Contents/MacOS").join(p);
            let o = r.run("test", &["-x", &s(&path)])?;
            if !o.ok() {
                return Err(format!("Contents/MacOS/{p} is missing or not executable"));
            }
        }
        let v = step(
            "capturemann --version",
            r.run(&s(&copy.join("Contents/MacOS/capturemann")), &["--version"]),
        )?;
        notes.push(format!("installed: {}", v.stdout.trim()));
        Ok(())
    })();
    // Uninstall: the copy goes; nothing of it may remain.
    let _ = r.run("rm", &["-rf", &s(&copy)]);
    let left = r
        .run("test", &["-e", &s(&copy)])
        .map(|o| o.ok())
        .unwrap_or(false);
    checked?;
    if left {
        return Err(format!(
            "{} is still there after the removal",
            copy.display()
        ));
    }
    notes.push("removed the copy; nothing left".to_owned());
    Ok(notes)
}

// ---- what `sys/mod.rs` calls ----

pub fn kill_tree(pid: u32) {
    kill_tree_with(&RealRunner, pid);
}

pub fn list_processes() -> Vec<(u32, String)> {
    list_processes_with(&RealRunner)
}

pub fn new_endpoint(dir: &Path, n: u32) -> String {
    // Unix socket paths are short (104 bytes on macOS): keep it directly under the sandbox.
    dir.join(format!("s{n}.sock"))
        .to_string_lossy()
        .into_owned()
}

pub fn has_desktop() -> bool {
    has_desktop_with(&RealRunner)
}

pub fn make_dpi_aware() {}

pub fn enum_windows() -> Vec<WinInfo> {
    enum_windows_with(&RealRunner)
}

pub fn window_from_point(x: i32, y: i32) -> Option<WinInfo> {
    window_from_point_in(&enum_windows(), x, y)
}

pub fn send_chord(keys: &[Key]) -> Result<(), String> {
    send_chord_with(&RealRunner, keys)
}

pub fn status_item_count(pid: u32) -> Option<usize> {
    status_item_count_with(&RealRunner, pid)
}

pub fn hold_file_open(path: &Path) -> Result<Box<dyn std::any::Any>, String> {
    // Unix has no mandatory locks; an open handle is the best stand-in.
    std::fs::File::open(path)
        .map(|f| Box::new(f) as Box<dyn std::any::Any>)
        .map_err(|e| e.to_string())
}

pub fn text_window_command(_path: &Path) -> Option<Command> {
    None
}

pub fn default_install_dir() -> Option<std::path::PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Answers by the first rule whose text is contained in "prog arg arg ..."; records every call.
    struct Fake {
        rules: Vec<(&'static str, CmdOut)>,
        calls: RefCell<Vec<String>>,
    }

    fn out(code: i32, stdout: &str) -> CmdOut {
        CmdOut {
            code: Some(code),
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }
    }

    impl Fake {
        fn new(rules: Vec<(&'static str, CmdOut)>) -> Fake {
            Fake {
                rules,
                calls: RefCell::default(),
            }
        }
        fn called(&self, needle: &str) -> bool {
            self.calls.borrow().iter().any(|c| c.contains(needle))
        }
    }

    impl Runner for Fake {
        fn run(&self, prog: &str, args: &[&str]) -> Result<CmdOut, String> {
            let line = format!("{prog} {}", args.join(" "));
            self.calls.borrow_mut().push(line.clone());
            for (needle, o) in &self.rules {
                if line.contains(needle) {
                    return Ok(o.clone());
                }
            }
            Ok(out(0, ""))
        }
    }

    #[test]
    fn ps_lines_become_pid_and_base_name() {
        let t = "  1 /sbin/launchd\n 4242 /Applications/CaptureMann.app/Contents/MacOS/cm-desktop\n 77 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\nbad line\n";
        let v = parse_ps(t);
        assert_eq!(v[1], (4242, "cm-desktop".to_owned()));
        assert_eq!(v[2], (77, "Google Chrome".to_owned()));
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn list_processes_runs_ps() {
        let f = Fake::new(vec![("ps -A", out(0, "10 /bin/zsh\n"))]);
        assert_eq!(list_processes_with(&f), vec![(10, "zsh".to_owned())]);
    }

    #[test]
    fn kill_tree_kills_children_before_the_parent() {
        let f = Fake::new(vec![
            ("pgrep -P 100", out(0, "101\n102\n")),
            ("pgrep -P 101", out(0, "103\n")),
            ("pgrep", out(1, "")),
        ]);
        kill_tree_with(&f, 100);
        let calls = f.calls.borrow();
        let pos = |s: &str| calls.iter().position(|c| c == s).unwrap();
        assert!(pos("kill -9 103") < pos("kill -9 101"));
        assert!(pos("kill -9 101") < pos("kill -9 100"));
        assert!(pos("kill -9 102") < pos("kill -9 100"));
    }

    const WINDOWS: &str = r#"[
      {"id":5,"pid":900,"owner":"cm-overlay","name":"","x":0,"y":0,"w":1440,"h":900,"layer":25,"alpha":1,"on":true},
      {"id":6,"pid":901,"owner":"cm-desktop","name":"CaptureMann","x":100.4,"y":50,"w":800,"h":600,"layer":0,"alpha":1,"on":true},
      {"id":7,"pid":901,"owner":"cm-desktop","name":"hidden","x":0,"y":0,"w":10,"h":10,"layer":0,"alpha":0,"on":false},
      {"id":8,"pid":1,"owner":"x"}
    ]"#;

    #[test]
    fn windows_parse_with_layer_as_topmost_and_visibility() {
        let w = parse_windows(WINDOWS);
        assert_eq!(w.len(), 3, "a window without bounds is dropped");
        assert_eq!(w[0].rect, [0, 0, 1440, 900]);
        assert_ne!(w[0].ex_style & WS_EX_TOPMOST, 0);
        assert!(w[0].visible);
        assert_eq!(w[1].rect, [100, 50, 900, 650]);
        assert_eq!(w[1].ex_style, 0);
        assert_eq!(w[1].title, "CaptureMann");
        assert_eq!(w[1].pid, 901);
        assert!(!w[2].visible);
        assert!(parse_windows("not json").is_empty());
        assert!(parse_windows("{}").is_empty());
    }

    #[test]
    fn enum_windows_uses_osascript_and_survives_a_failure() {
        let f = Fake::new(vec![("osascript -l JavaScript", out(0, WINDOWS))]);
        assert_eq!(enum_windows_with(&f).len(), 3);
        assert!(f.called("CGWindowListCopyWindowInfo"));
        let bad = Fake::new(vec![("osascript", out(1, ""))]);
        assert!(enum_windows_with(&bad).is_empty());
    }

    #[test]
    fn point_lookup_takes_the_frontmost_visible_window() {
        let w = parse_windows(WINDOWS);
        assert_eq!(window_from_point_in(&w, 500, 300).unwrap().hwnd, 5);
        let no_overlay = &w[1..];
        assert_eq!(window_from_point_in(no_overlay, 500, 300).unwrap().hwnd, 6);
        assert!(
            window_from_point_in(no_overlay, 5, 5).is_none(),
            "the hidden window does not count"
        );
        assert!(
            window_from_point_in(no_overlay, 900, 300).is_none(),
            "the right edge is outside"
        );
    }

    #[test]
    fn displays_parse_and_the_desktop_check_uses_them() {
        let two = r#"[{"x":0,"y":0,"w":1440,"h":900},{"x":1440,"y":-120,"w":1920,"h":1080}]"#;
        assert_eq!(
            parse_displays(two).unwrap(),
            vec![[0, 0, 1440, 900], [1440, -120, 3360, 960]]
        );
        assert!(parse_displays("oops").is_none());
        let f = Fake::new(vec![
            ("osascript", out(0, two)),
            ("ioreg", out(0, "\"CGSSessionScreenIsLocked\"=No")),
        ]);
        assert!(has_desktop_with(&f));
        let locked = Fake::new(vec![(
            "ioreg",
            out(0, "x \"CGSSessionScreenIsLocked\"=Yes"),
        )]);
        assert!(!has_desktop_with(&locked));
        let none = Fake::new(vec![("osascript", out(0, "[]")), ("ioreg", out(0, ""))]);
        assert!(!has_desktop_with(&none), "no display listed");
        let unknown = Fake::new(vec![("osascript", out(1, ""))]);
        assert!(
            has_desktop_with(&unknown),
            "an unreadable list does not hide the cases"
        );
    }

    #[test]
    fn chords_become_system_events_scripts() {
        assert_eq!(
            chord_script(&[Key::Alt, Key::Shift, Key::Char('1')]).unwrap(),
            "tell application \"System Events\" to key code 18 using {option down, shift down}"
        );
        assert_eq!(
            chord_script(&[Key::Esc]).unwrap(),
            "tell application \"System Events\" to key code 53"
        );
        assert!(chord_script(&[Key::Ctrl, Key::Shift]).is_err());
        assert!(chord_script(&[Key::Char('!')]).is_err());
    }

    #[test]
    fn a_refused_key_press_names_the_accessibility_permission() {
        let f = Fake::new(vec![("osascript", out(1, ""))]);
        let e = send_chord_with(&f, &[Key::Esc]).unwrap_err();
        assert!(e.contains("Accessibility"));
        let ok = Fake::new(vec![]);
        assert!(send_chord_with(&ok, &[Key::Esc]).is_ok());
        assert!(ok.called("key code 53"));
    }

    #[test]
    fn status_items_are_counted_or_unknown() {
        let f = Fake::new(vec![("osascript", out(0, "2\n"))]);
        assert_eq!(status_item_count_with(&f, 321), Some(2));
        assert!(f.called("unix id is 321"));
        let denied = Fake::new(vec![("osascript", out(1, ""))]);
        assert_eq!(status_item_count_with(&denied, 321), None);
        let junk = Fake::new(vec![("osascript", out(0, "missing value"))]);
        assert_eq!(status_item_count_with(&junk, 321), None);
    }

    #[test]
    fn crash_reports_keep_ours_and_recent() {
        let e = vec![
            ("cm-desktop-2026-11-06-101010.ips".to_owned(), 200),
            ("cm-desktop-old.ips".to_owned(), 50),
            ("Safari-2026.ips".to_owned(), 300),
            ("capturemann_x.crash".to_owned(), 100),
            ("cm-notes.txt".to_owned(), 300),
        ];
        assert_eq!(
            filter_crash_reports(&e, 100),
            vec!["cm-desktop-2026-11-06-101010.ips", "capturemann_x.crash"]
        );
        assert!(crash_reports_since(
            Path::new("/definitely/not/here"),
            std::time::SystemTime::now()
        )
        .is_empty());
    }

    #[test]
    fn app_and_mount_helpers() {
        assert_eq!(
            find_app("Applications\nCaptureMann.app\n").as_deref(),
            Some("CaptureMann.app")
        );
        assert!(find_app("readme\n").is_none());
        assert!(still_mounted(
            "image-path : /x\n /tmp/w/mnt\n",
            "/tmp/w/mnt"
        ));
        assert!(!still_mounted("nothing", "/tmp/w/mnt"));
    }

    fn work(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cm-regress-mac-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn install_rules() -> Vec<(&'static str, CmdOut)> {
        vec![
            ("ls ", out(0, "CaptureMann.app\n")),
            (
                "plutil -extract CFBundleIdentifier",
                out(0, "dev.capturemann.app\n"),
            ),
            (
                "plutil -extract CFBundleShortVersionString",
                out(0, "0.2.9\n"),
            ),
            ("--version", out(0, "capturemann 0.2.9\n")),
            ("test -e", out(1, "")),
        ]
    }

    // The fake install tree is built with Unix paths; the module only runs on macOS (CI Windows failure, Nov 6, 2026).
    #[cfg(unix)]
    #[test]
    fn dmg_install_checks_signature_programs_and_removal() {
        let w = work("dmg");
        let f = Fake::new(install_rules());
        let notes = check_app_install(&f, Path::new("/x/CaptureMann.dmg"), &w).unwrap();
        assert!(notes.iter().any(|n| n.contains("dev.capturemann.app")));
        assert!(notes.last().unwrap().contains("nothing left"));
        assert!(f.called("hdiutil attach -nobrowse -readonly -noautoopen -mountpoint"));
        assert!(f.called("hdiutil detach"));
        assert!(f.called("codesign --verify --deep --strict"));
        assert!(f.called("test -x") && f.called("Contents/MacOS/cm-overlay"));
        assert!(f.called("rm -rf"));
        assert!(
            !f.calls.borrow().iter().any(|c| c.contains("/Applications")),
            "the check never touches /Applications"
        );
        let _ = std::fs::remove_dir_all(w);
    }

    #[test]
    fn an_app_folder_is_checked_without_a_mount() {
        let w = work("app");
        let f = Fake::new(install_rules());
        check_app_install(&f, Path::new("/x/CaptureMann.app"), &w).unwrap();
        assert!(!f.called("hdiutil"));
        assert!(f.called("ditto /x/CaptureMann.app"));
        let _ = std::fs::remove_dir_all(w);
    }

    #[test]
    fn a_bad_signature_fails_but_still_ejects_and_removes() {
        let w = work("sig");
        let mut rules = vec![("codesign", out(1, ""))];
        rules.extend(install_rules());
        let f = Fake::new(rules);
        let e = check_app_install(&f, Path::new("/x/CaptureMann.dmg"), &w).unwrap_err();
        assert!(e.contains("codesign"), "{e}");
        assert!(f.called("hdiutil detach"));
        assert!(f.called("rm -rf"));
        let _ = std::fs::remove_dir_all(w);
    }

    // The fake install tree is built with Unix paths; the module only runs on macOS (CI Windows failure, Nov 6, 2026).
    #[cfg(unix)]
    #[test]
    fn a_missing_program_a_failed_copy_and_a_leftover_are_reported() {
        let w = work("miss");
        let mut rules = vec![("test -x", out(1, ""))];
        rules.extend(install_rules());
        let e = check_app_install(&Fake::new(rules), Path::new("/x/C.app"), &w).unwrap_err();
        assert!(e.contains("missing or not executable"), "{e}");

        let f = Fake::new(vec![("ditto", out(1, "")), ("ls ", out(0, "C.app\n"))]);
        let e = check_app_install(&f, Path::new("/x/C.dmg"), &w).unwrap_err();
        assert!(e.contains("ditto"), "{e}");
        assert!(f.called("hdiutil detach"), "ejects after a failed copy");

        let mut rules = vec![("test -e", out(0, ""))];
        rules.extend(install_rules());
        let e = check_app_install(&Fake::new(rules), Path::new("/x/C.app"), &w).unwrap_err();
        assert!(e.contains("still there"), "{e}");

        let f = Fake::new(vec![(
            "hdiutil info",
            out(0, &format!("{}/mnt\n", w.display())),
        )]);
        let e = check_app_install(&f, Path::new("/x/C.dmg"), &w).unwrap_err();
        assert!(e.contains("still mounted"), "{e}");
        let _ = std::fs::remove_dir_all(w);
    }

    #[test]
    fn an_image_without_an_app_fails() {
        let w = work("noapp");
        let f = Fake::new(vec![("ls ", out(0, "readme.txt\n"))]);
        let e = check_app_install(&f, Path::new("/x/C.dmg"), &w).unwrap_err();
        assert!(e.contains("no .app"), "{e}");
        let _ = std::fs::remove_dir_all(w);
    }
}
