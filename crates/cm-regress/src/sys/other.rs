//! macOS and Linux: process helpers through `ps` and `kill`; the window probes say "not supported".

use super::{Key, WinInfo};
use std::process::Command;

#[cfg(target_os = "macos")]
pub const PLATFORM: &str = "macos";
#[cfg(not(target_os = "macos"))]
pub const PLATFORM: &str = "linux";

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

pub fn list_processes() -> Vec<(u32, String)> {
    let Ok(out) = Command::new("ps").args(["-A", "-o", "pid=,comm="]).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (pid, name) = l.split_once(' ')?;
            let name = name.trim().rsplit('/').next()?.to_owned();
            Some((pid.parse().ok()?, name))
        })
        .collect()
}

pub fn new_endpoint(dir: &std::path::Path, n: u32) -> String {
    // Unix socket paths are short (104 bytes on macOS): keep it directly under the sandbox.
    dir.join(format!("s{n}.sock"))
        .to_string_lossy()
        .into_owned()
}

pub fn has_desktop() -> bool {
    true
}

pub fn make_dpi_aware() {}

pub fn enum_windows() -> Vec<WinInfo> {
    Vec::new()
}

pub fn window_from_point(_x: i32, _y: i32) -> Option<WinInfo> {
    None
}

pub fn send_chord(_keys: &[Key]) -> Result<(), String> {
    Err("injected keys are only implemented on Windows".to_owned())
}

pub fn hold_file_open(path: &std::path::Path) -> Result<Box<dyn std::any::Any>, String> {
    // Unix has no mandatory locks; an open handle is the best stand-in.
    std::fs::File::open(path)
        .map(|f| Box::new(f) as Box<dyn std::any::Any>)
        .map_err(|e| e.to_string())
}

pub fn text_window_command(_path: &std::path::Path) -> Option<Command> {
    None
}

pub fn default_install_dir() -> Option<std::path::PathBuf> {
    None
}
