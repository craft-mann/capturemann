//! Windows: process helpers through `taskkill` and `tasklist`, window probes through the Win32 API.

#![allow(clippy::unwrap_used)]

use super::{Key, WinInfo};
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetWindowLongW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, WindowFromPoint, GA_ROOT, GWL_EXSTYLE,
};

pub const PLATFORM: &str = "windows";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn hide_console(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

pub fn kill_tree(pid: u32) {
    let mut c = Command::new("taskkill");
    c.args(["/F", "/T", "/PID", &pid.to_string()]);
    hide_console(&mut c);
    let _ = c.output();
}

pub fn list_processes() -> Vec<(u32, String)> {
    let mut c = Command::new("tasklist");
    c.args(["/FO", "CSV", "/NH"]);
    hide_console(&mut c);
    let Ok(out) = c.output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            // "name.exe","1234","Console","1","12,345 K"
            let mut it = l.trim().trim_matches('"').split("\",\"");
            let name = it.next()?.to_owned();
            let pid = it.next()?.parse().ok()?;
            Some((pid, name))
        })
        .collect()
}

pub fn new_endpoint(_dir: &std::path::Path, n: u32) -> String {
    format!(r"\\.\pipe\cm-regress-{}-{n}", std::process::id())
}

pub fn has_desktop() -> bool {
    let mut session = 0u32;
    // SAFETY: plain query of the current process; `session` is a valid out pointer.
    let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
    ok.is_ok() && session != 0
}

pub fn make_dpi_aware() {
    // SAFETY: no pointers; fails harmlessly when the awareness is already set.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn info(hwnd: HWND) -> WinInfo {
    let mut rect = RECT::default();
    let mut class = [0u16; 256];
    let mut title = [0u16; 512];
    let mut pid = 0u32;
    // SAFETY: `hwnd` came from the system; all buffers outlive the calls and the lengths match.
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
        let nc = GetClassNameW(hwnd, &mut class).max(0) as usize;
        let nt = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        WinInfo {
            hwnd: hwnd.0 as i64,
            pid,
            class: String::from_utf16_lossy(&class[..nc]),
            title: String::from_utf16_lossy(&title[..nt]),
            rect: [rect.left, rect.top, rect.right, rect.bottom],
            ex_style: GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
            visible: IsWindowVisible(hwnd).as_bool(),
        }
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the address of the Vec passed by `enum_windows` below.
    let out = unsafe { &mut *(lparam.0 as *mut Vec<WinInfo>) };
    out.push(info(hwnd));
    BOOL(1)
}

pub fn enum_windows() -> Vec<WinInfo> {
    let mut out: Vec<WinInfo> = Vec::new();
    // SAFETY: the callback only runs during this call and `out` outlives it.
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut out as *mut Vec<WinInfo> as isize),
        );
    }
    out
}

pub fn window_from_point(x: i32, y: i32) -> Option<WinInfo> {
    // SAFETY: plain query.
    unsafe {
        let hit = WindowFromPoint(POINT { x, y });
        if hit.0.is_null() {
            return None;
        }
        // A child control answers; report the top-level window it belongs to.
        let root = GetAncestor(hit, GA_ROOT);
        Some(info(if root.0.is_null() { hit } else { root }))
    }
}

fn vk(k: Key) -> u8 {
    match k {
        Key::Ctrl => VK_CONTROL.0 as u8,
        Key::Alt => VK_MENU.0 as u8,
        Key::Shift => VK_SHIFT.0 as u8,
        Key::Esc => VK_ESCAPE.0 as u8,
        Key::Char(c) => c.to_ascii_uppercase() as u8,
    }
}

pub fn send_chord(keys: &[Key]) -> Result<(), String> {
    // SAFETY: plain input injection into the current desktop.
    unsafe {
        for k in keys {
            keybd_event(vk(*k), 0, KEYBD_EVENT_FLAGS(0), 0);
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
        for k in keys.iter().rev() {
            keybd_event(vk(*k), 0, KEYEVENTF_KEYUP, 0);
        }
    }
    Ok(())
}

pub fn hold_file_open(path: &std::path::Path) -> Result<Box<dyn std::any::Any>, String> {
    use std::os::windows::fs::OpenOptionsExt;
    // share_mode 0: nobody else may open the file while we hold it.
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .map(|f| Box::new(f) as Box<dyn std::any::Any>)
        .map_err(|e| e.to_string())
}

pub fn text_window_command(path: &std::path::Path) -> Option<Command> {
    let mut c = Command::new("notepad.exe");
    c.arg(path);
    Some(c)
}

pub fn default_install_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|d| std::path::PathBuf::from(d).join(r"Programs\CaptureMann"))
}
