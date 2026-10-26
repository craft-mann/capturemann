//! Windows backend: `WH_MOUSE_LL` low-level hook for moves, buttons and wheel, `GetCursorInfo`
//! for the cursor image.
//!
//! STATUS: compile-checked only (`cargo check --target x86_64-pc-windows-msvc`); never run on
//! Windows. Needs: the process to be per-monitor-v2 DPI aware (so hook coordinates are physical
//! px of the virtual screen, and the `CursorTarget` scale is 1.0).
//!
//! Timestamps: `MSLLHOOKSTRUCT.time` is a millisecond tick count with no QPC relation, so the hook
//! stamps with the host clock at hook entry (receipt time, typically well under 1 ms after the
//! event unless the hook thread is starved; Windows removes hooks that exceed
//! `LowLevelHooksTimeout`, so the callback only stamps and queues).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cm_core::{CmError, HostClock, SystemHostClock};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorInfo, GetCursorPos, GetIconInfo, GetMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, CURSORINFO, CURSOR_SHOWING, HHOOK, HICON, ICONINFO,
    MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::image::CursorImage;
use crate::recorder::CursorConfig;
use crate::schema::{Button, CursorStatus, Modifier, MoveSource};
use crate::source::{
    spawn_image_poller, spawn_position_poller, Activity, CursorSource, ImageProvider, RawEvent,
    RawKind, RawSender, POLL_HZ,
};

pub const CLOCK_NAME: &str = "session host clock (QPC) at hook entry";
pub const PLATFORM: &str = "windows";

/// No permission exists for a low-level mouse hook; the status is always full.
pub fn preflight() -> CursorStatus {
    CursorStatus::full(MoveSource::Hook)
}

pub fn default_source(_config: &CursorConfig) -> Box<dyn CursorSource> {
    Box::new(WinSource::default())
}

struct HookCtx {
    tx: RawSender,
    activity: Arc<Activity>,
}

/// The hook procedure has no user pointer, so the context is a process-wide pointer owned by the
/// hook thread (set before the hook is installed, cleared after it is removed).
static CTX: AtomicPtr<HookCtx> = AtomicPtr::new(std::ptr::null_mut());

fn mods_now() -> Vec<Modifier> {
    // Only the pressed/not-pressed state of the four modifier keys: no other key is queried.
    // SAFETY: GetAsyncKeyState takes a plain virtual-key code and touches no memory.
    let down = |vk: u16| (unsafe { GetAsyncKeyState(i32::from(vk)) } as u16) & 0x8000 != 0;
    let mut m = Vec::new();
    if down(VK_SHIFT.0) {
        m.push(Modifier::Shift);
    }
    if down(VK_CONTROL.0) {
        m.push(Modifier::Ctrl);
    }
    if down(VK_MENU.0) {
        m.push(Modifier::Alt);
    }
    if down(VK_LWIN.0) || down(VK_RWIN.0) {
        m.push(Modifier::Cmd);
    }
    m
}

/// # Safety
/// Only Windows may call this, as the `WH_MOUSE_LL` hook procedure: for `code >= 0`, `lparam` is
/// a pointer to a valid `MSLLHOOKSTRUCT`.
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Stamp first, before anything else can delay us.
    let host_ns = SystemHostClock.now_ns();
    if code >= 0 {
        let ctx = CTX.load(Ordering::Acquire);
        if !ctx.is_null() && lparam.0 != 0 {
            // SAFETY: `ctx` is cleared before the hook thread frees it; for `code >= 0` lparam
            // points to a valid MSLLHOOKSTRUCT for the duration of the call.
            let (ctx, info) = unsafe { (&*ctx, &*(lparam.0 as *const MSLLHOOKSTRUCT)) };
            let (gx, gy) = (f64::from(info.pt.x), f64::from(info.pt.y));
            let hi = (info.mouseData >> 16) as u16;
            let kind = match wparam.0 as u32 {
                WM_MOUSEMOVE => Some(RawKind::Move { gx, gy }),
                WM_LBUTTONDOWN | WM_LBUTTONUP => Some(button(
                    gx,
                    gy,
                    Button::Left,
                    wparam.0 as u32 == WM_LBUTTONDOWN,
                )),
                WM_RBUTTONDOWN | WM_RBUTTONUP => Some(button(
                    gx,
                    gy,
                    Button::Right,
                    wparam.0 as u32 == WM_RBUTTONDOWN,
                )),
                WM_MBUTTONDOWN | WM_MBUTTONUP => Some(button(
                    gx,
                    gy,
                    Button::Middle,
                    wparam.0 as u32 == WM_MBUTTONDOWN,
                )),
                WM_XBUTTONDOWN | WM_XBUTTONUP => {
                    let b = if hi == 1 {
                        Button::Back
                    } else {
                        Button::Forward
                    };
                    Some(button(gx, gy, b, wparam.0 as u32 == WM_XBUTTONDOWN))
                }
                WM_MOUSEWHEEL => Some(RawKind::Scroll {
                    dx: 0,
                    dy: i16::signum(hi as i16) as i8,
                }),
                WM_MOUSEHWHEEL => Some(RawKind::Scroll {
                    dx: i16::signum(hi as i16) as i8,
                    dy: 0,
                }),
                _ => None,
            };
            if let Some(kind) = kind {
                ctx.activity.touch(host_ns);
                ctx.tx.send(RawEvent { host_ns, kind });
            }
        }
    }
    // SAFETY: forwards the unchanged arguments to the next hook.
    unsafe { CallNextHookEx(None::<HHOOK>, code, wparam, lparam) }
}

fn button(gx: f64, gy: f64, button: Button, down: bool) -> RawKind {
    RawKind::Button {
        gx,
        gy,
        button,
        down,
        mods: mods_now(),
    }
}

fn hook_thread(tx: RawSender, activity: Arc<Activity>, ready: mpsc::Sender<Option<u32>>) {
    let ctx = Box::into_raw(Box::new(HookCtx { tx, activity }));
    CTX.store(ctx, Ordering::Release);
    // SAFETY: standard hook installation on this thread; the module handle is the process's.
    let hook = unsafe {
        let hmod = GetModuleHandleW(None)
            .ok()
            .map(|m| windows::Win32::Foundation::HINSTANCE(m.0));
        SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), hmod, 0)
    };
    match hook {
        Ok(h) => {
            // SAFETY: plain query of the calling thread id.
            let _ = ready.send(Some(unsafe { GetCurrentThreadId() }));
            let mut msg = MSG::default();
            // SAFETY: a standard message loop; WM_QUIT (posted by `stop`) ends it, GetMessageW
            // returns 0 then (and -1 on error).
            while unsafe { GetMessageW(&mut msg, None::<HWND>, 0, 0) }.0 > 0 {}
            // SAFETY: `h` was returned by SetWindowsHookExW on this thread.
            let _ = unsafe { UnhookWindowsHookEx(h) };
        }
        Err(_) => {
            let _ = ready.send(None);
        }
    }
    CTX.store(std::ptr::null_mut(), Ordering::Release);
    // SAFETY: allocated by Box::into_raw above; the hook is removed so no callback can use it.
    drop(unsafe { Box::from_raw(ctx) });
}

/// Polls `GetCursorInfo` and extracts the image when the `HCURSOR` handle changes.
struct WinImageProvider {
    last: Option<isize>,
}

impl ImageProvider for WinImageProvider {
    fn poll(&mut self) -> Option<CursorImage> {
        let mut ci = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `ci` is a correctly sized, writable CURSORINFO.
        unsafe { GetCursorInfo(&mut ci) }.ok()?;
        if ci.flags.0 & CURSOR_SHOWING.0 == 0 {
            return None;
        }
        let key = ci.hCursor.0 as isize;
        if self.last == Some(key) {
            return None;
        }
        self.last = Some(key);
        // SAFETY: the handle comes from GetCursorInfo just now.
        unsafe { extract(HICON(ci.hCursor.0)) }
    }
}

/// Best effort: 32-bit colour cursors (alpha or AND-mask) and monochrome cursors.
///
/// # Safety
/// `icon` must be a valid cursor handle.
unsafe fn extract(icon: HICON) -> Option<CursorImage> {
    let mut info = ICONINFO::default();
    // SAFETY: `info` is writable; on success the bitmaps are ours to delete.
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    // SAFETY: bitmaps from GetIconInfo; freed below on every path.
    let out = unsafe { read_bitmaps(&info) };
    // SAFETY: deleting the GDI objects GetIconInfo gave us (each at most once).
    unsafe {
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
    }
    let (w, h, rgba) = out?;
    // SAFETY: GetDpiForSystem takes no arguments and touches no memory.
    let scale = f64::from(unsafe { GetDpiForSystem() }) / 96.0;
    Some(CursorImage {
        width: w,
        height: h,
        rgba,
        hotspot: [info.xHotspot as i32, info.yHotspot as i32],
        scale,
    })
}

/// # Safety
/// `hdc` must be a valid device context and `bm` a valid bitmap not selected into any DC; the
/// bitmap must be `w` x `h` pixels.
unsafe fn dib(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    bm: HBITMAP,
    w: i32,
    h: i32,
) -> Option<Vec<u8>> {
    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    // SAFETY: `buf` holds w*h BGRA pixels, matching the header.
    let lines = unsafe {
        GetDIBits(
            hdc,
            bm,
            0,
            h as u32,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            &mut bi,
            DIB_RGB_COLORS,
        )
    };
    (lines != 0).then_some(buf)
}

/// # Safety
/// `info` must hold the valid bitmap handles (or null/invalid for an absent one) from GetIconInfo.
unsafe fn read_bitmaps(info: &ICONINFO) -> Option<(u32, u32, Vec<u8>)> {
    // SAFETY: GDI calls on bitmaps owned by the caller; the DC is released before returning.
    unsafe {
        let hdc = GetDC(None);
        let color = !info.hbmColor.is_invalid();
        let probe = if color { info.hbmColor } else { info.hbmMask };
        let mut bmp = BITMAP::default();
        let n = GetObjectW(
            HGDIOBJ(probe.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&mut bmp as *mut BITMAP).cast::<c_void>()),
        );
        let result = (|| {
            if n == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
                return None;
            }
            let w = bmp.bmWidth;
            if color {
                let h = bmp.bmHeight;
                let mut px = dib(hdc, info.hbmColor, w, h)?;
                let any_alpha = px.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
                if !any_alpha {
                    // Old-style colour cursor: transparency comes from the AND mask (1 = clear).
                    let mask = dib(hdc, info.hbmMask, w, h)?;
                    for (p, m) in px
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(mask.as_chunks::<4>().0)
                    {
                        p[3] = if m[0] != 0 { 0 } else { 255 };
                    }
                }
                for p in px.as_chunks_mut::<4>().0.iter_mut() {
                    p.swap(0, 2); // BGRA -> RGBA
                }
                Some((w as u32, h as u32, px))
            } else {
                // Monochrome: the mask bitmap is 2h tall, AND plane on top, XOR plane below.
                let h = bmp.bmHeight / 2;
                let both = dib(hdc, info.hbmMask, w, h * 2)?;
                let plane = (w * h * 4) as usize;
                let (and, xor) = both.split_at(plane);
                let mut out = Vec::with_capacity(plane);
                for (a, x) in and.as_chunks::<4>().0.iter().zip(xor.as_chunks::<4>().0) {
                    let (a, x) = (a[0] != 0, x[0] != 0);
                    // AND=1,XOR=0 transparent; 0,0 black; 0,1 white; 1,1 inverts the screen,
                    // approximated as white at half opacity.
                    out.extend_from_slice(&match (a, x) {
                        (true, false) => [0, 0, 0, 0],
                        (false, false) => [0, 0, 0, 255],
                        (false, true) => [255, 255, 255, 255],
                        (true, true) => [255, 255, 255, 128],
                    });
                }
                Some((w as u32, h as u32, out))
            }
        })();
        ReleaseDC(None, hdc);
        result
    }
}

fn pointer_location() -> Option<(f64, f64)> {
    let mut p = POINT::default();
    // SAFETY: `p` is writable.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((f64::from(p.x), f64::from(p.y)))
}

/// The real Windows source.
#[derive(Default)]
pub struct WinSource {
    stop: Option<Arc<AtomicBool>>,
    hook_thread_id: Option<u32>,
    threads: Vec<JoinHandle<()>>,
}

impl CursorSource for WinSource {
    fn start(&mut self, tx: RawSender) -> Result<CursorStatus, CmError> {
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = Some(stop.clone());
        let clock: Arc<dyn HostClock> = Arc::new(SystemHostClock);
        let activity = Arc::new(Activity::default());
        let (rtx, rrx) = mpsc::channel();
        let (t, a) = (tx.clone(), activity.clone());
        self.threads.push(
            thread::Builder::new()
                .name("cm-cursor-hook".into())
                .spawn(move || hook_thread(t, a, rtx))
                .map_err(|e| CmError::internal(format!("hook thread: {e}")))?,
        );
        let status = match rrx.recv_timeout(Duration::from_secs(2)) {
            Ok(Some(id)) => {
                self.hook_thread_id = Some(id);
                CursorStatus::full(MoveSource::Hook)
            }
            _ => {
                // The hook could not be installed: positions only, like the denied path.
                self.threads.push(spawn_position_poller(
                    pointer_location,
                    POLL_HZ,
                    clock.clone(),
                    tx.clone(),
                    stop.clone(),
                    activity.clone(),
                ));
                let mut s = CursorStatus::full(MoveSource::Poll);
                s.clicks = crate::schema::Availability::Unavailable;
                s.scroll = crate::schema::Availability::Unavailable;
                s.warnings.push(crate::schema::CursorWarning {
                    code: cm_core::ErrorCode::PlatformRejected,
                    permission: None,
                    message: "the mouse hook could not be installed: positions only".into(),
                });
                s
            }
        };
        if let Some((gx, gy)) = pointer_location() {
            let host_ns = clock.now_ns();
            tx.send(RawEvent {
                host_ns,
                kind: RawKind::Move { gx, gy },
            });
        }
        self.threads.push(spawn_image_poller(
            Box::new(WinImageProvider { last: None }),
            clock,
            tx,
            stop,
            activity,
        ));
        Ok(status)
    }

    fn stop(&mut self) {
        if let Some(s) = &self.stop {
            s.store(true, Ordering::Relaxed);
        }
        if let Some(id) = self.hook_thread_id.take() {
            // SAFETY: posts WM_QUIT to the hook thread's queue, ending its message loop.
            let _ = unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
        for h in self.threads.drain(..) {
            let _ = h.join();
        }
    }
}

impl Drop for WinSource {
    fn drop(&mut self) {
        self.stop();
    }
}
