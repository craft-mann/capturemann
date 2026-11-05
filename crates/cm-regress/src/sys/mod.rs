//! Everything that differs by operating system, in one place (`scripts/check_target_os.py` allows a
//! `sys/` folder). The rest of the suite calls these functions and asks [`PLATFORM`] when a case only
//! makes sense on one system; on the others the calls answer "not supported" and the case reports SKIP.

use std::process::Command;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(not(windows))]
mod other;
#[cfg(not(windows))]
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

/// The path of the install folder a per-user installer uses by default.
pub fn default_install_dir() -> Option<std::path::PathBuf> {
    imp::default_install_dir()
}
