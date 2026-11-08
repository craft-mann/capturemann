//! Everything that differs by operating system, in one place (`scripts/check_target_os.py` allows a
//! `sys/` folder). The rest of the suite calls these functions and asks [`PLATFORM`] when a case only
//! makes sense on one system; on the others the calls answer "not supported" and the case reports SKIP.

use std::process::Command;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

// The Linux driver always compiles (std only, its parsers are unit-tested on any machine) and is the
// active one on Linux; `cases/linux.rs` reaches its probes as `sys::linux::...`.
pub mod linux;
#[cfg(target_os = "linux")]
use linux as imp;

// macOS driver. Compiled in test builds everywhere so its fake-runner unit tests run on any machine.
#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod other;
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
use other as imp;

/// `"windows"`, `"macos"` or `"linux"`.
pub const PLATFORM: &str = imp::PLATFORM;

/// A top-level window, as the Windows probes see it. Rectangles are physical pixels (the suite makes
/// itself per-monitor DPI aware first), `(left, top, right, bottom)`.
#[derive(Debug, Clone)]
pub struct WinInfo {
    pub hwnd: i64,
    pub pid: u32,
    pub class: String,
    pub title: String,
    pub rect: [i32; 4],
    pub ex_style: u32,
    pub visible: bool,
}

impl WinInfo {
    pub fn width(&self) -> i32 {
        self.rect[2] - self.rect[0]
    }
    pub fn height(&self) -> i32 {
        self.rect[3] - self.rect[1]
    }
    pub fn brief(&self) -> String {
        format!(
            "hwnd {:#x} pid {} class '{}' title '{}' rect {:?} ex {:#x}{}",
            self.hwnd,
            self.pid,
            self.class,
            crate::util::cut(&self.title, 40),
            self.rect,
            self.ex_style,
            if self.visible { "" } else { " hidden" }
        )
    }
}

/// `WS_EX_TRANSPARENT`: mouse clicks go to the window below.
pub const WS_EX_TRANSPARENT: u32 = 0x20;
/// `WS_EX_TOPMOST`.
pub const WS_EX_TOPMOST: u32 = 0x8;

/// A key for [`send_chord`].
#[derive(Debug, Clone, Copy)]
pub enum Key {
    Ctrl,
    Alt,
    Shift,
    Esc,
    #[cfg_attr(not(windows), allow(dead_code))]
    Char(char),
}

/// The executable file name for a program on this system (`capturemann` -> `capturemann.exe`).
pub fn exe_name(base: &str) -> String {
    if PLATFORM == "windows" {
        format!("{base}.exe")
    } else {
        base.to_owned()
    }
}

/// Keeps a console program from opening a visible window when the suite itself has none.
pub fn hide_console(cmd: &mut Command) {
    imp::hide_console(cmd);
}

/// Kills a process and everything it started.
pub fn kill_tree(pid: u32) {
    imp::kill_tree(pid);
}

/// All running processes: (pid, image name).
pub fn list_processes() -> Vec<(u32, String)> {
    imp::list_processes()
}

/// A fresh daemon endpoint inside the sandbox: a named pipe on Windows, a socket path elsewhere.
pub fn new_endpoint(dir: &std::path::Path, n: u32) -> String {
    imp::new_endpoint(dir, n)
}

/// Whether this process runs in a logged-in desktop session (Windows: not session 0).
pub fn has_desktop() -> bool {
    imp::has_desktop()
}

/// Per-monitor DPI awareness, so rectangles are physical pixels.
pub fn make_dpi_aware() {
    imp::make_dpi_aware();
}

/// Windows only: every top-level window.
pub fn enum_windows() -> Vec<WinInfo> {
    imp::enum_windows()
}

/// Windows only: the top-level window under a screen point.
pub fn window_from_point(x: i32, y: i32) -> Option<WinInfo> {
    imp::window_from_point(x, y)
}

/// Presses the keys together in order, then releases them in reverse order (injected input).
pub fn send_chord(keys: &[Key]) -> Result<(), String> {
    imp::send_chord(keys)
}

/// Holds a file open with no sharing until the guard is dropped.
pub fn hold_file_open(path: &std::path::Path) -> Result<Box<dyn std::any::Any>, String> {
    imp::hold_file_open(path)
}

/// The command that opens a small text window on a file (Windows: Notepad), for the window cases.
pub fn text_window_command(path: &std::path::Path) -> Option<Command> {
    imp::text_window_command(path)
}

/// Linux only: why a case that needs to find and capture another program's window cannot run (a Wayland
/// session has no window list, X tools missing, no desktop). `None` elsewhere and on a usable X11 session.
pub fn window_cases_unavailable() -> Option<String> {
    if PLATFORM == "linux" {
        linux::window_probe_unavailable(
            &|k| std::env::var(k).ok().filter(|v| !v.is_empty()),
            &linux::SystemRunner::default(),
        )
    } else {
        None
    }
}

/// True on a Linux Wayland session: windows have no ids there and the pointer is drawn into the picture (no
/// cursor track), both stated gaps of the Linux release. `false` everywhere else.
pub fn is_linux_wayland() -> bool {
    PLATFORM == "linux"
        && linux::session_kind(&|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            == linux::Session::Wayland
}

/// Linux only: why the recording cases cannot run (the build keeps recording hidden unless
/// `CM_LINUX_RECORDING=1`). `None` on the other systems and when the switch is on.
pub fn recording_unavailable() -> Option<String> {
    if PLATFORM == "linux" {
        linux::recording_unavailable()
    } else {
        None
    }
}

/// The path of the install folder a per-user installer uses by default.
pub fn default_install_dir() -> Option<std::path::PathBuf> {
    imp::default_install_dir()
}

/// macOS only: how many menu-bar icons the process shows; `None` when it cannot be probed (always `None`
/// elsewhere).
pub fn status_item_count(pid: u32) -> Option<usize> {
    #[cfg(target_os = "macos")]
    {
        imp::status_item_count(pid)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        None
    }
}

/// macOS crash reports of our programs in a DiagnosticReports folder, written since a time.
pub fn crash_reports_since(dir: &std::path::Path, since: std::time::SystemTime) -> Vec<String> {
    #[cfg(any(target_os = "macos", test))]
    {
        macos::crash_reports_since(dir, since)
    }
    #[cfg(not(any(target_os = "macos", test)))]
    {
        let _ = (dir, since);
        Vec::new()
    }
}

/// macOS only: mounts a DMG (or takes an `.app`), copies the app into `work`, checks signature, programs
/// and version, removes the copy; one note per step. Never touches `/Applications`.
pub fn check_app_install(
    source: &std::path::Path,
    work: &std::path::Path,
) -> Result<Vec<String>, String> {
    #[cfg(target_os = "macos")]
    {
        macos::check_app_install(&macos::RealRunner, source, work)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (source, work);
        Err("the app bundle and DMG check is only implemented on macOS".to_owned())
    }
}
