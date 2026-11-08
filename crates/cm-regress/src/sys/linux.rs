//! Linux (Ubuntu 24.04 on the lab laptop; X11 and GNOME Wayland): process helpers through `ps` and `kill`,
//! window and monitor probes through the X11 tools (`xprop`, `xwininfo`, `xrandr`), the VA-API report
//! through `vainfo`, the `.deb` state through `dpkg-query`, crashes through `coredumpctl`.
//!
//! This file compiles on every system (only std is used) so that its parsers and the fake-runner tests run
//! on the developer's Mac; it is the active driver only on Linux (`sys/mod.rs`). Everything that talks to
//! the machine goes through [`CommandRunner`] and an environment lookup, so tests feed canned text.
//!
//! Limits, stated where the cases use them: on a Wayland session `xprop` only sees XWayland windows, GNOME
//! has no supported way to list native Wayland windows from outside (`org.gnome.Shell.Introspect` answers
//! AccessDenied unless the shell runs in unsafe mode), and keystrokes are never injected.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use super::{Key, WinInfo};
use std::process::Command;

pub const PLATFORM: &str = "linux";

/// `_NET_WM_STATE_ABOVE` is reported as the Windows "topmost" bit so the shared probes read it the same way.
const EX_TOPMOST: u32 = 0x8;

// ---------------------------------------------------------------------------------------------------
// Running commands (fakeable)
// ---------------------------------------------------------------------------------------------------

/// What a finished command printed.
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

/// Runs a program to the end. `None` means it could not be started (not installed). Tests use a fake.
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Option<CmdOut>;
}

/// The real thing: every command runs under coreutils `timeout` so a stuck X server or D-Bus call cannot
/// hang the suite.
pub struct SystemRunner {
    pub secs: u64,
}

impl Default for SystemRunner {
    fn default() -> Self {
        SystemRunner { secs: 15 }
    }
}

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Option<CmdOut> {
        let out = Command::new("timeout")
            .args(["-k", "2", &self.secs.to_string(), program])
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .ok()?;
        // `timeout` answers 127 when it could not start the program.
        if out.status.code() == Some(127) {
            return None;
        }
        Some(CmdOut {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// An environment variable lookup (tests pass a closure over a map).
pub type EnvLookup<'a> = &'a dyn Fn(&str) -> Option<String>;

fn real_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------------------------------------------
// Session and recording gate
// ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    X11,
    Wayland,
    /// Neither `DISPLAY` nor `WAYLAND_DISPLAY`: an ssh login without the desktop's environment.
    Headless,
}

impl Session {
    pub fn name(self) -> &'static str {
        match self {
            Session::X11 => "x11",
            Session::Wayland => "wayland",
            Session::Headless => "headless",
        }
    }
}

/// The kind of desktop session the environment describes. `XDG_SESSION_TYPE` wins when it says `x11` or
/// `wayland`; otherwise `WAYLAND_DISPLAY`, then `DISPLAY` (XWayland sets `DISPLAY` on Wayland too, which is
/// why Wayland is checked first).
pub fn session_kind(env: EnvLookup) -> Session {
    match env("XDG_SESSION_TYPE").as_deref() {
        Some("wayland") => return Session::Wayland,
        Some("x11") => return Session::X11,
        _ => {}
    }
    if env("WAYLAND_DISPLAY").is_some() {
        Session::Wayland
    } else if env("DISPLAY").is_some() {
        Session::X11
    } else {
        Session::Headless
    }
}

/// `CM_LINUX_RECORDING=1` (the same switch the app reads, `cm_platform::capability`) says the build under
/// test records. Without it the recording cases report SKIP with this reason.
pub fn recording_unavailable_with(env: EnvLookup) -> Option<String> {
    match env("CM_LINUX_RECORDING").as_deref().map(str::trim) {
        Some("1") => None,
        _ => Some(
            "recording is hidden in the Linux build (set CM_LINUX_RECORDING=1 in the environment of the run to try it)"
                .to_owned(),
        ),
    }
}

pub fn recording_unavailable() -> Option<String> {
    recording_unavailable_with(&real_env)
}

// ---------------------------------------------------------------------------------------------------
// Monitors (xrandr; works on X11 and, through XWayland, on GNOME Wayland)
// ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub name: String,
    pub primary: bool,
    /// Physical pixels as the X server reports them.
    pub w: i32,
    pub h: i32,
    pub x: i32,
    pub y: i32,
}

/// Parses `xrandr --listmonitors`:
/// ```text
/// Monitors: 2
///  0: +*eDP-1 1920/309x1080/174+0+0  eDP-1
///  1: +HDMI-1 2560/597x1440/336+1920+0  HDMI-1
/// ```
pub fn parse_monitors(text: &str) -> Vec<Monitor> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((idx, rest)) = line.split_once(':') else {
            continue;
        };
        if idx.trim().parse::<u32>().is_err() {
            continue;
        }
        let mut it = rest.split_whitespace();
        let (Some(flags), Some(geo)) = (it.next(), it.next()) else {
            continue;
        };
        let name = it.next().unwrap_or(flags.trim_start_matches(['+', '*']));
        let primary = flags.contains('*');
        // 1920/309x1080/174+0+0
        let Some((size, pos)) = geo.split_once(['+', '-']) else {
            continue;
        };
        let sign_at = geo.len() - pos.len() - 1;
        let first_sign = &geo[sign_at..=sign_at];
        let (w, h) = match size.split_once('x') {
            Some((a, b)) => (mm_strip(a), mm_strip(b)),
            None => continue,
        };
        let mut coords = pos.split(['+', '-']);
        let x: i32 = coords.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let y: i32 = coords.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        // A negative x or y is rare (a monitor left of or above the primary has the primary at 0,0
        // in xrandr, so these are non-negative) but keep the first sign right.
        let x = if first_sign == "-" { -x } else { x };
        if let (Some(w), Some(h)) = (w, h) {
            out.push(Monitor {
                name: name.to_owned(),
                primary,
                w,
                h,
                x,
                y,
            });
        }
    }
    out
}

/// `1920/309` -> 1920.
fn mm_strip(s: &str) -> Option<i32> {
    s.split('/').next()?.parse().ok()
}

pub fn list_monitors(r: &dyn CommandRunner) -> Result<Vec<Monitor>, String> {
    let out = r
        .run("xrandr", &["--listmonitors"])
        .ok_or("xrandr is not installed (apt install x11-apps or x11-xserver-utils)")?;
    if !out.ok() {
        return Err(format!(
            "xrandr failed: {}",
            crate::util::cut(out.stderr.trim(), 160)
        ));
    }
    Ok(parse_monitors(&out.stdout))
}

// ---------------------------------------------------------------------------------------------------
// Windows (X11 / XWayland only)
// ---------------------------------------------------------------------------------------------------

/// `_NET_CLIENT_LIST(WINDOW): window id # 0x3a00007, 0x3c00003` -> the ids.
pub fn parse_window_ids(text: &str) -> Vec<i64> {
    let Some((_, ids)) = text.split_once('#') else {
        return Vec::new();
    };
    ids.split(',')
        .filter_map(|s| i64::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XProps {
    pub pid: Option<u32>,
    pub class: String,
    pub title: String,
    pub states: Vec<String>,
    pub types: Vec<String>,
}

fn after_eq(line: &str) -> Option<&str> {
    line.split_once('=').map(|(_, v)| v.trim())
}

fn quoted_items(v: &str) -> Vec<String> {
    v.split("\", \"")
        .map(|s| s.trim().trim_matches('"').to_owned())
        .collect()
}

fn atom_items(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parses the answer of `xprop -id <id> _NET_WM_PID WM_CLASS _NET_WM_NAME _NET_WM_STATE _NET_WM_WINDOW_TYPE`.
pub fn parse_xprop(text: &str) -> XProps {
    let mut p = XProps::default();
    for line in text.lines() {
        let Some(v) = after_eq(line) else { continue };
        if line.starts_with("_NET_WM_PID") {
            p.pid = v.parse().ok();
        } else if line.starts_with("WM_CLASS") {
            let items = quoted_items(v);
            // (instance, class): the class is the stable one.
            p.class = items.get(1).or(items.first()).cloned().unwrap_or_default();
        } else if line.starts_with("_NET_WM_NAME") {
            p.title = v.trim_matches('"').to_owned();
        } else if line.starts_with("_NET_WM_STATE") {
            p.states = atom_items(v);
        } else if line.starts_with("_NET_WM_WINDOW_TYPE") {
            p.types = atom_items(v);
        }
    }
    p
}

/// `(left, top, right, bottom, viewable)` from `xwininfo -id`.
pub fn parse_xwininfo(text: &str) -> Option<([i32; 4], bool)> {
    let num = |key: &str| -> Option<i32> {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key))
            .and_then(|v| v.trim().parse().ok())
    };
    let x = num("Absolute upper-left X:")?;
    let y = num("Absolute upper-left Y:")?;
    let w = num("Width:")?;
    let h = num("Height:")?;
    let viewable = text.contains("Map State: IsViewable");
    Some(([x, y, x + w, y + h], viewable))
}

/// Every client window of the X server, bottom to top (the window manager's stacking order). Wayland
/// sessions list the XWayland windows only.
pub fn windows_with(r: &dyn CommandRunner) -> Vec<WinInfo> {
    let ids = r
        .run("xprop", &["-root", "_NET_CLIENT_LIST_STACKING"])
        .filter(CmdOut::ok)
        .map(|o| parse_window_ids(&o.stdout))
        .unwrap_or_default();
    let mut out = Vec::new();
    for id in ids {
        let hex = format!("{id:#x}");
        let Some(props) = r
            .run(
                "xprop",
                &[
                    "-id",
                    &hex,
                    "_NET_WM_PID",
                    "WM_CLASS",
                    "_NET_WM_NAME",
                    "_NET_WM_STATE",
                    "_NET_WM_WINDOW_TYPE",
                ],
            )
            .filter(CmdOut::ok)
        else {
            continue;
        };
        let props = parse_xprop(&props.stdout);
        let Some((rect, viewable)) = r
            .run("xwininfo", &["-id", &hex])
            .filter(CmdOut::ok)
            .and_then(|o| parse_xwininfo(&o.stdout))
        else {
            continue;
        };
        let hidden = props.states.iter().any(|s| s == "_NET_WM_STATE_HIDDEN");
        let above = props.states.iter().any(|s| s == "_NET_WM_STATE_ABOVE");
        out.push(WinInfo {
            hwnd: id,
            pid: props.pid.unwrap_or(0),
            class: props.class,
            title: props.title,
            rect,
            ex_style: if above { EX_TOPMOST } else { 0 },
            visible: viewable && !hidden,
        });
    }
    out
}

/// The topmost visible window that contains the point (stacking order, top first).
pub fn window_at(wins: &[WinInfo], x: i32, y: i32) -> Option<WinInfo> {
    wins.iter()
        .rev()
        .find(|w| w.visible && x >= w.rect[0] && x < w.rect[2] && y >= w.rect[1] && y < w.rect[3])
        .cloned()
}

/// Why the window probes cannot see the app's windows, when that is the case (Wayland, no X tools).
pub fn window_probe_unavailable(env: EnvLookup, r: &dyn CommandRunner) -> Option<String> {
    match session_kind(env) {
        Session::Headless => Some("no desktop session in the environment (DISPLAY and WAYLAND_DISPLAY are unset; start the run from the logged-in session, see scripts/testlab/regress.sh linux)".to_owned()),
        Session::Wayland => Some("this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session".to_owned()),
        Session::X11 => {
            if r.run("xprop", &["-version"]).is_none() || r.run("xwininfo", &["-version"]).is_none() {
                Some("xprop and xwininfo are not installed (apt install x11-utils)".to_owned())
            } else {
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// VA-API
// ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VaapiReport {
    /// `vainfo` ran and listed at least one profile.
    pub works: bool,
    pub driver: String,
    pub h264_encode: bool,
    pub hevc_encode: bool,
    pub vp9_encode: bool,
    pub profiles: Vec<String>,
}

impl VaapiReport {
    /// One line for the report: which encoder a recording would use on this machine.
    pub fn describe(&self) -> String {
        if !self.works {
            return "VA-API unusable (vainfo failed or lists no profile): software VP9 encoder"
                .to_owned();
        }
        let drv = if self.driver.is_empty() {
            "unknown driver"
        } else {
            &self.driver
        };
        if self.h264_encode {
            format!("VA-API hardware H.264 encode available ({drv}): hardware encoder")
        } else {
            format!("VA-API present ({drv}) but no H.264 encode entry point: software VP9 encoder")
        }
    }
}

/// Parses `vainfo`:
/// ```text
/// vainfo: Driver version: Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()
/// vainfo: Supported profile and entrypoints
///       VAProfileH264Main               : VAEntrypointVLD
///       VAProfileH264Main               : VAEntrypointEncSlice
/// ```
pub fn parse_vainfo(text: &str) -> VaapiReport {
    let mut r = VaapiReport::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(d) = line.strip_prefix("vainfo: Driver version:") {
            r.driver = d.trim().to_owned();
        } else if let Some((profile, entry)) = line.split_once(':') {
            let (profile, entry) = (profile.trim(), entry.trim());
            if !profile.starts_with("VAProfile") || !entry.starts_with("VAEntrypoint") {
                continue;
            }
            r.works = true;
            let enc = entry.contains("Enc");
            if !r.profiles.contains(&profile.to_owned()) {
                r.profiles.push(profile.to_owned());
            }
            if enc && profile.starts_with("VAProfileH264") {
                r.h264_encode = true;
            }
            if enc && profile.starts_with("VAProfileHEVC") {
                r.hevc_encode = true;
            }
            if enc && profile.starts_with("VAProfileVP9") {
                r.vp9_encode = true;
            }
        }
    }
    r
}

pub fn vaapi_report(r: &dyn CommandRunner) -> Result<VaapiReport, String> {
    let out = r
        .run("vainfo", &[])
        .ok_or("vainfo is not installed (apt install vainfo)")?;
    // A failing vainfo is a finding (no usable VA-API), not an error of the probe.
    Ok(parse_vainfo(&format!("{}\n{}", out.stdout, out.stderr)))
}

// ---------------------------------------------------------------------------------------------------
// Tray (StatusNotifierItem over D-Bus; GNOME needs the AppIndicator extension, which Ubuntu ships on)
// ---------------------------------------------------------------------------------------------------

/// Parses the `gdbus call ... Properties.Get ... RegisteredStatusNotifierItems` answer, for example
/// `(<['org.kde.StatusNotifierItem-4242-1/StatusNotifierItem', ':1.55/org/ayatana/NotificationItem/x']>,)`,
/// into the registered item names.
pub fn parse_tray_items(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find('\'') {
        let after = &rest[a + 1..];
        let Some(b) = after.find('\'') else { break };
        out.push(after[..b].to_owned());
        rest = &after[b + 1..];
    }
    out
}

/// The tray items registered right now. `Err` when no StatusNotifierWatcher answers (no tray host).
pub fn tray_items(r: &dyn CommandRunner) -> Result<Vec<String>, String> {
    let out = r
        .run(
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                "org.kde.StatusNotifierWatcher",
                "--object-path",
                "/StatusNotifierWatcher",
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                "org.kde.StatusNotifierWatcher",
                "RegisteredStatusNotifierItems",
            ],
        )
        .ok_or("gdbus is not installed")?;
    if !out.ok() {
        return Err(format!(
            "no StatusNotifierWatcher answers (the desktop shows no tray): {}",
            crate::util::cut(out.stderr.trim(), 160)
        ));
    }
    Ok(parse_tray_items(&out.stdout))
}

// ---------------------------------------------------------------------------------------------------
// Packages and crashes
// ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebState {
    NotInstalled,
    /// Removed but its configuration files are left (`rc`).
    ConfigFilesOnly,
    Installed(String),
}

/// Parses `dpkg-query -W -f='${db:Status-Abbrev}|${Version}\n' <package>`.
pub fn parse_dpkg_state(text: &str) -> DebState {
    let Some(line) = text.lines().map(str::trim).find(|l| l.contains('|')) else {
        return DebState::NotInstalled;
    };
    let (status, version) = line.split_once('|').unwrap_or((line, ""));
    let status = status.trim();
    if status.starts_with("ii") {
        DebState::Installed(version.trim().to_owned())
    } else if status.starts_with("rc") {
        DebState::ConfigFilesOnly
    } else {
        DebState::NotInstalled
    }
}

pub fn deb_state(r: &dyn CommandRunner, package: &str) -> DebState {
    r.run(
        "dpkg-query",
        &["-W", "-f=${db:Status-Abbrev}|${Version}\\n", package],
    )
    .filter(CmdOut::ok)
    .map_or(DebState::NotInstalled, |o| parse_dpkg_state(&o.stdout))
}

/// Whether `sudo` works without a password (the lab laptop is set up that way, `linux-setup.sh`).
pub fn passwordless_sudo(r: &dyn CommandRunner) -> bool {
    r.run("sudo", &["-n", "true"]).is_some_and(|o| o.ok())
}

/// Crash records of our programs from `coredumpctl list --no-legend`: lines whose executable path ends in
/// one of the names. The time filter is `coredumpctl`'s own (`--since`).
pub fn parse_coredump_list(text: &str, names: &[&str]) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| {
            l.split_whitespace().any(|w| {
                w.rsplit('/')
                    .next()
                    .is_some_and(|base| names.contains(&base))
                    && w.contains('/')
            })
        })
        .map(str::to_owned)
        .collect()
}

/// The graphics adapters from `lspci -mm -nn`: the VGA, display and 3D controllers as
/// `<vendor> <device>`, joined with `; ` (empty when there are none). Used in the report header,
/// which said "GPU: n/a" on Linux.
pub fn parse_lspci_gpus(text: &str) -> String {
    let mut out = Vec::new();
    for l in text.lines() {
        let q: Vec<&str> = l.split('"').collect();
        // 00:02.0 "class [0300]" "vendor [8086]" "device [3ea0]" ...
        let (Some(class), Some(vendor), Some(device)) = (q.get(1), q.get(3), q.get(5)) else {
            continue;
        };
        let c = class.to_lowercase();
        if !(c.contains("vga") || c.contains("display controller") || c.contains("3d controller")) {
            continue;
        }
        let strip = |s: &str| match s.rfind(" [") {
            Some(i) if s.ends_with(']') => s[..i].to_owned(),
            _ => s.to_owned(),
        };
        out.push(format!("{} {}", strip(vendor), strip(device)));
    }
    out.join("; ")
}

/// Core dumps of the product's programs since `since` (a `coredumpctl --since` time such as `-30min` or
/// `2026-11-06 10:00:00`). Empty when `coredumpctl` is missing or journald keeps no dumps.
pub fn crashes_since(r: &dyn CommandRunner, since: &str) -> Vec<String> {
    let Some(out) = r.run(
        "coredumpctl",
        &["list", "--no-legend", "--no-pager", "--since", since],
    ) else {
        return Vec::new();
    };
    parse_coredump_list(
        &out.stdout,
        &[
            "capturemann",
            "cm-daemon",
            "cm-mcp",
            "cm-desktop",
            "cm-overlay",
            "ffmpeg",
            "llama-server",
        ],
    )
}

// ---------------------------------------------------------------------------------------------------
// The shared `sys` interface
// ---------------------------------------------------------------------------------------------------

pub fn hide_console(_cmd: &mut Command) {}

pub fn kill_tree(pid: u32) {
    // Children first, then the process itself.
    if let Ok(out) = Command::new("pgrep")
        .args(["-P", &pid.to_string()])
        .output()
    {
        for child in String::from_utf8_lossy(&out.stdout).split_whitespace() {
            if let Ok(c) = child.parse::<u32>() {
                kill_tree(c);
            }
        }
    }
    let _ = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status();
}

/// Parses `ps -A -o pid=,stat=,comm=` and leaves out zombies (state `Z`): a process the case killed
/// but whose parent has not waited for yet is not running, and must not count as a program left
/// behind (0.2.9 report: `linux.desktop_stays_up`, `linux.second_launch`, `linux.tray_icon`).
pub fn parse_ps_stat(text: &str) -> Vec<(u32, String)> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?;
            let stat = it.next()?;
            if stat.starts_with('Z') {
                return None;
            }
            let name = it.collect::<Vec<_>>().join(" ");
            let name = name.rsplit('/').next()?.to_owned();
            Some((pid.parse().ok()?, name))
        })
        .collect()
}

pub fn list_processes() -> Vec<(u32, String)> {
    let Ok(out) = Command::new("ps")
        .args(["-A", "-o", "pid=,stat=,comm="])
        .output()
    else {
        return Vec::new();
    };
    parse_ps_stat(&String::from_utf8_lossy(&out.stdout))
}

pub fn new_endpoint(dir: &std::path::Path, n: u32) -> String {
    // Unix socket paths are limited to 107 bytes: keep it directly under the sandbox.
    dir.join(format!("s{n}.sock"))
        .to_string_lossy()
        .into_owned()
}

/// A desktop session is reachable: `DISPLAY` or `WAYLAND_DISPLAY` is set and the runtime directory exists.
pub fn has_desktop_with(env: EnvLookup, runtime_dir_exists: bool) -> bool {
    session_kind(env) != Session::Headless && runtime_dir_exists
}

pub fn has_desktop() -> bool {
    let dir_ok = real_env("XDG_RUNTIME_DIR").is_some_and(|d| std::path::Path::new(&d).is_dir());
    has_desktop_with(&real_env, dir_ok)
}

pub fn make_dpi_aware() {}

pub fn enum_windows() -> Vec<WinInfo> {
    if window_probe_unavailable(&real_env, &SystemRunner::default()).is_some() {
        return Vec::new();
    }
    windows_with(&SystemRunner::default())
}

pub fn window_from_point(x: i32, y: i32) -> Option<WinInfo> {
    window_at(&enum_windows(), x, y)
}

pub fn send_chord(_keys: &[Key]) -> Result<(), String> {
    Err("injected keys are not implemented on Linux (the suite never injects input into the lab laptop's session)".to_owned())
}

pub fn hold_file_open(path: &std::path::Path) -> Result<Box<dyn std::any::Any>, String> {
    // Unix has no mandatory locks; an open handle is the best stand-in.
    std::fs::File::open(path)
        .map(|f| Box::new(f) as Box<dyn std::any::Any>)
        .map_err(|e| e.to_string())
}

/// A small window that shows a text file and carries the file's stem in its title (the window cases look
/// the window up by that title): `zenity --text-info` (Ubuntu desktop ships it), else `xmessage`.
pub fn text_window_command(path: &std::path::Path) -> Option<Command> {
    text_window_with(path, &SystemRunner::default())
}

pub fn text_window_with(path: &std::path::Path, r: &dyn CommandRunner) -> Option<Command> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    if r.run("zenity", &["--version"]).is_some_and(|o| o.ok()) {
        let mut c = Command::new("zenity");
        c.arg("--text-info")
            .arg(format!("--title={stem}"))
            .arg(format!("--filename={}", path.display()))
            .args(["--width=720", "--height=320"]);
        return Some(c);
    }
    if r.run("xmessage", &["-version"]).is_some() {
        let mut c = Command::new("xmessage");
        c.args(["-title", &stem, "-file"]).arg(path);
        return Some(c);
    }
    None
}

/// Where the tarball's `install.sh` puts the programs for one user (`~/.local/lib/capturemann`), or the
/// `.deb`'s folder (`/opt/capturemann/lib/capturemann`) when only that exists.
pub fn default_install_dir() -> Option<std::path::PathBuf> {
    let user = std::env::var_os("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".local/lib/capturemann"));
    let deb = std::path::PathBuf::from("/opt/capturemann/lib/capturemann");
    match user {
        Some(u) if u.join("capturemann").is_file() => Some(u),
        _ if deb.join("capturemann").is_file() => Some(deb),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A fake runner: `"program arg arg"` -> output. Unknown commands are "not installed".
    struct Fake(BTreeMap<String, CmdOut>);

    impl Fake {
        fn new(items: &[(&str, i32, &str)]) -> Fake {
            Fake(
                items
                    .iter()
                    .map(|(k, code, out)| {
                        (
                            (*k).to_owned(),
                            CmdOut {
                                code: Some(*code),
                                stdout: (*out).to_owned(),
                                stderr: String::new(),
                            },
                        )
                    })
                    .collect(),
            )
        }
    }

    impl CommandRunner for Fake {
        fn run(&self, program: &str, args: &[&str]) -> Option<CmdOut> {
            let key = std::iter::once(program)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            self.0.get(&key).cloned()
        }
    }

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn text_window_prefers_zenity_and_titles_it_with_the_file_stem() {
        let r = Fake::new(&[("zenity --version", 0, "4.0.1\n")]);
        let c = text_window_with(std::path::Path::new("/tmp/regress-window-7.txt"), &r).unwrap();
        assert_eq!(c.get_program(), "zenity");
        let args: Vec<_> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--title=regress-window-7".to_owned()));
        assert!(args.contains(&"--filename=/tmp/regress-window-7.txt".to_owned()));
        let none = Fake::new(&[]);
        assert!(text_window_with(std::path::Path::new("/tmp/a.txt"), &none).is_none());
    }

    #[test]
    fn session_kind_prefers_the_declared_type_then_wayland_display() {
        let k = |p: &[(&str, &str)]| session_kind(&env_of(p));
        assert_eq!(
            k(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")]),
            Session::X11
        );
        assert_eq!(
            k(&[("XDG_SESSION_TYPE", "wayland"), ("DISPLAY", ":0")]),
            Session::Wayland
        );
        // XWayland also sets DISPLAY; without a declared type WAYLAND_DISPLAY decides.
        assert_eq!(
            k(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]),
            Session::Wayland
        );
        assert_eq!(k(&[("DISPLAY", ":1")]), Session::X11);
        assert_eq!(k(&[("XDG_SESSION_TYPE", "tty")]), Session::Headless);
        assert!(!has_desktop_with(&env_of(&[]), true));
        assert!(!has_desktop_with(&env_of(&[("DISPLAY", ":0")]), false));
        assert!(has_desktop_with(&env_of(&[("DISPLAY", ":0")]), true));
    }

    #[test]
    fn recording_needs_the_opt_in() {
        assert!(recording_unavailable_with(&env_of(&[("CM_LINUX_RECORDING", "1")])).is_none());
        assert!(recording_unavailable_with(&env_of(&[("CM_LINUX_RECORDING", " 1 ")])).is_none());
        let why = recording_unavailable_with(&env_of(&[])).unwrap();
        assert!(why.contains("CM_LINUX_RECORDING=1"));
        assert!(recording_unavailable_with(&env_of(&[("CM_LINUX_RECORDING", "0")])).is_some());
    }

    #[test]
    fn xrandr_monitors_are_parsed() {
        let t = "Monitors: 2\n 0: +*eDP-1 1920/309x1080/174+0+0  eDP-1\n 1: +HDMI-1 2560/597x1440/336+1920+0  HDMI-1\n";
        let m = parse_monitors(t);
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[0],
            Monitor {
                name: "eDP-1".into(),
                primary: true,
                w: 1920,
                h: 1080,
                x: 0,
                y: 0
            }
        );
        assert_eq!(
            (m[1].w, m[1].h, m[1].x, m[1].y, m[1].primary),
            (2560, 1440, 1920, 0, false)
        );
        assert!(parse_monitors("Monitors: 0\n").is_empty());
        let r = Fake::new(&[("xrandr --listmonitors", 0, t)]);
        assert_eq!(list_monitors(&r).unwrap().len(), 2);
        assert!(list_monitors(&Fake::new(&[]))
            .unwrap_err()
            .contains("not installed"));
    }

    #[test]
    fn window_ids_and_properties() {
        assert_eq!(
            parse_window_ids(
                "_NET_CLIENT_LIST_STACKING(WINDOW): window id # 0x3a00007, 0x3c00003\n"
            ),
            vec![0x3a00007, 0x3c00003]
        );
        assert!(
            parse_window_ids("_NET_CLIENT_LIST_STACKING:  no such atom on any window.").is_empty()
        );
        let p = parse_xprop(
            "_NET_WM_PID(CARDINAL) = 4242\nWM_CLASS(STRING) = \"cm-desktop\", \"dev.capturemann.app\"\n_NET_WM_NAME(UTF8_STRING) = \"CaptureMann\"\n_NET_WM_STATE(ATOM) = _NET_WM_STATE_ABOVE, _NET_WM_STATE_FOCUSED\n_NET_WM_WINDOW_TYPE(ATOM) = _NET_WM_WINDOW_TYPE_NORMAL\n",
        );
        assert_eq!(p.pid, Some(4242));
        assert_eq!(p.class, "dev.capturemann.app");
        assert_eq!(p.title, "CaptureMann");
        assert_eq!(p.states.len(), 2);
        assert_eq!(p.types, vec!["_NET_WM_WINDOW_TYPE_NORMAL"]);
        let (rect, viewable) = parse_xwininfo(
            "  Absolute upper-left X:  100\n  Absolute upper-left Y:  50\n  Width: 800\n  Height: 600\n  Map State: IsViewable\n",
        )
        .unwrap();
        assert_eq!(rect, [100, 50, 900, 650]);
        assert!(viewable);
        assert!(parse_xwininfo("garbage").is_none());
    }

    #[test]
    fn windows_are_assembled_and_hit_tested_top_first() {
        let r = Fake::new(&[
            (
                "xprop -root _NET_CLIENT_LIST_STACKING",
                0,
                "_NET_CLIENT_LIST_STACKING(WINDOW): window id # 0x10, 0x20\n",
            ),
            (
                "xprop -id 0x10 _NET_WM_PID WM_CLASS _NET_WM_NAME _NET_WM_STATE _NET_WM_WINDOW_TYPE",
                0,
                "_NET_WM_PID(CARDINAL) = 1\nWM_CLASS(STRING) = \"a\", \"A\"\n_NET_WM_NAME(UTF8_STRING) = \"bottom\"\n",
            ),
            (
                "xprop -id 0x20 _NET_WM_PID WM_CLASS _NET_WM_NAME _NET_WM_STATE _NET_WM_WINDOW_TYPE",
                0,
                "_NET_WM_PID(CARDINAL) = 2\nWM_CLASS(STRING) = \"b\", \"B\"\n_NET_WM_NAME(UTF8_STRING) = \"top\"\n_NET_WM_STATE(ATOM) = _NET_WM_STATE_ABOVE\n",
            ),
            (
                "xwininfo -id 0x10",
                0,
                "Absolute upper-left X: 0\nAbsolute upper-left Y: 0\nWidth: 1000\nHeight: 1000\nMap State: IsViewable\n",
            ),
            (
                "xwininfo -id 0x20",
                0,
                "Absolute upper-left X: 10\nAbsolute upper-left Y: 10\nWidth: 100\nHeight: 100\nMap State: IsViewable\n",
            ),
        ]);
        let w = windows_with(&r);
        assert_eq!(w.len(), 2);
        assert_eq!(w[1].ex_style & EX_TOPMOST, EX_TOPMOST);
        assert_eq!(window_at(&w, 50, 50).unwrap().title, "top");
        assert_eq!(window_at(&w, 500, 500).unwrap().title, "bottom");
        assert!(window_at(&w, 2000, 2000).is_none());
    }

    #[test]
    fn window_probe_says_why_it_cannot_run() {
        let r = Fake::new(&[
            ("xprop -version", 0, "xprop 1.2"),
            ("xwininfo -version", 0, "x"),
        ]);
        assert!(window_probe_unavailable(&env_of(&[("XDG_SESSION_TYPE", "x11")]), &r).is_none());
        let wl = window_probe_unavailable(&env_of(&[("XDG_SESSION_TYPE", "wayland")]), &r).unwrap();
        assert!(wl.contains("Wayland"));
        assert!(window_probe_unavailable(&env_of(&[]), &r)
            .unwrap()
            .contains("no desktop session"));
        let none = Fake::new(&[]);
        assert!(
            window_probe_unavailable(&env_of(&[("DISPLAY", ":0")]), &none)
                .unwrap()
                .contains("x11-utils")
        );
    }

    const VAINFO_HW: &str = "libva info: VA-API version 1.20.0\nvainfo: Driver version: Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()\nvainfo: Supported profile and entrypoints\n      VAProfileH264Main               : VAEntrypointVLD\n      VAProfileH264Main               : VAEntrypointEncSlice\n      VAProfileVP9Profile0            : VAEntrypointVLD\n";
    const VAINFO_DECODE_ONLY: &str = "vainfo: Driver version: Mesa Gallium driver 24.0 for AMD\nvainfo: Supported profile and entrypoints\n      VAProfileH264Main               : VAEntrypointVLD\n";

    #[test]
    fn vaapi_report_tells_hardware_from_software() {
        let hw = parse_vainfo(VAINFO_HW);
        assert!(hw.works && hw.h264_encode && !hw.vp9_encode);
        assert!(hw.driver.contains("iHD"));
        assert!(hw.describe().contains("hardware encoder"));
        let dec = parse_vainfo(VAINFO_DECODE_ONLY);
        assert!(dec.works && !dec.h264_encode);
        assert!(dec.describe().contains("software VP9"));
        let none = parse_vainfo("error: failed to initialize display\n");
        assert!(!none.works);
        assert!(none.describe().contains("unusable"));
        let r = Fake::new(&[("vainfo", 0, VAINFO_HW)]);
        assert!(vaapi_report(&r).unwrap().h264_encode);
        assert!(vaapi_report(&Fake::new(&[])).is_err());
    }

    #[test]
    fn dpkg_state_is_parsed() {
        assert_eq!(
            parse_dpkg_state("ii |0.2.9\n"),
            DebState::Installed("0.2.9".into())
        );
        assert_eq!(parse_dpkg_state("rc |0.2.9\n"), DebState::ConfigFilesOnly);
        assert_eq!(parse_dpkg_state(""), DebState::NotInstalled);
        assert_eq!(parse_dpkg_state("un |\n"), DebState::NotInstalled);
        let r = Fake::new(&[(
            "dpkg-query -W -f=${db:Status-Abbrev}|${Version}\\n capturemann",
            0,
            "ii |0.2.9\n",
        )]);
        assert_eq!(
            deb_state(&r, "capturemann"),
            DebState::Installed("0.2.9".into())
        );
        // dpkg-query exits 1 for an unknown package.
        let r = Fake::new(&[(
            "dpkg-query -W -f=${db:Status-Abbrev}|${Version}\\n capturemann",
            1,
            "",
        )]);
        assert_eq!(deb_state(&r, "capturemann"), DebState::NotInstalled);
        assert!(passwordless_sudo(&Fake::new(&[("sudo -n true", 0, "")])));
        assert!(!passwordless_sudo(&Fake::new(&[("sudo -n true", 1, "")])));
    }

    #[test]
    fn coredumps_of_our_programs_are_picked_out() {
        let t = "Fri 2026-11-06 10:00:01 UTC 1234 1000 1000 SIGSEGV present /opt/capturemann/lib/capturemann/cm-desktop 12.3M\nFri 2026-11-06 10:05:00 UTC 99 1000 1000 SIGABRT present /usr/bin/gnome-shell 5M\n";
        let r = Fake::new(&[("coredumpctl list --no-legend --no-pager --since -1h", 1, t)]);
        let c = crashes_since(&r, "-1h");
        assert_eq!(c.len(), 1);
        assert!(c[0].contains("cm-desktop"));
        assert!(crashes_since(&Fake::new(&[]), "-1h").is_empty());
    }

    #[test]
    fn lspci_gpus_are_named_for_the_report_header() {
        let t = "00:02.0 \"VGA compatible controller [0300]\" \"Intel Corporation [8086]\" \"WhiskeyLake-U GT2 [UHD Graphics 620] [3ea0]\" -r02 -p00 \"Lenovo [17aa]\" \"x [2279]\"\n3d:00.0 \"Non-Volatile memory controller [0108]\" \"Samsung [144d]\" \"SSD [a808]\"\n";
        assert_eq!(
            parse_lspci_gpus(t),
            "Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]"
        );
        assert_eq!(parse_lspci_gpus(""), "");
    }

    #[test]
    fn tray_items_are_parsed() {
        let t = "(<['org.kde.StatusNotifierItem-4242-1/StatusNotifierItem', ':1.55/org/ayatana/NotificationItem/x']>,)\n";
        let v = parse_tray_items(t);
        assert_eq!(v.len(), 2);
        assert!(v[0].contains("-4242-"));
        assert!(parse_tray_items("(<@as []>,)").is_empty());
        let key = "gdbus call --session --dest org.kde.StatusNotifierWatcher --object-path /StatusNotifierWatcher --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems";
        let r = Fake::new(&[(key, 0, t)]);
        assert_eq!(tray_items(&r).unwrap().len(), 2);
        assert!(tray_items(&Fake::new(&[])).is_err());
    }

    /// A killed program that its parent has not reaped yet (state Z) is not "left running".
    #[test]
    fn zombies_are_not_running_programs() {
        let p = parse_ps_stat(
            "    1 Ss   systemd\n 4242 Z    cm-desktop\n 4300 Sl+  cm-desktop\n 77 R /usr/bin/x\n",
        );
        assert_eq!(
            p,
            vec![
                (1, "systemd".to_owned()),
                (4300, "cm-desktop".to_owned()),
                (77, "x".to_owned())
            ]
        );
    }
}
