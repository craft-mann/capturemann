//! S2 spike: frozen-screen region picker.
//!
//! Captures a display first, then shows that still image (dimmed) in a window
//! covering the whole display and lets you drag a region. On mouse release the
//! region is cropped from the ORIGINAL capture and saved.
//!
//! Window pixels map 1:1 to capture pixels at the window's real offset on the
//! display (never scaled), so the crop is exactly what you dragged. The spike
//! checks that the window covers the whole display, including the menu bar and
//! the notch strip on MacBooks.
//!
//! macOS: no native fullscreen (it keeps windows below the notch and animates).
//! Instead a borderless window is raised above the menu bar and given the
//! screen's full frame, as screenshot tools do.
//!
//! Usage: spike-overlay [--display N] [--out DIR]
//! Keys:  drag = select, release = save, Esc = cancel

use std::{num::NonZeroU32, path::PathBuf, rc::Rc, time::Instant};

use anyhow::{Context, Result};
use softbuffer::{Context as SbContext, Surface};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    monitor::MonitorHandle,
    window::{CursorIcon, Window, WindowId, WindowLevel},
};
use xcap::{image::RgbaImage, Monitor};

const DIM: u32 = 45; // percent brightness outside the selection
const ACCENT: u32 = 0x00E5_484D; // selection border, 0RGB
const BUDGET_MS: f64 = 150.0;

struct Shot {
    image: RgbaImage,
    xcap_pos: (i32, i32),
    scale: f32,
}

struct Timing {
    start: Instant,
    capture_ms: f64,
    window_start: Option<Instant>,
    window_ms: f64,
}

struct App {
    timing: Timing,
    shot: Shot,
    out_dir: PathBuf,
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    /// Physical position of the monitor the window should cover.
    mon_pos: (i32, i32),
    /// Window content origin relative to the monitor, in physical px.
    offset: (i32, i32),
    /// Pre-rendered frames at window size, 0RGB.
    bright: Vec<u32>,
    dimmed: Vec<u32>,
    win_size: (u32, u32),
    cursor: (f64, f64),
    drag_start: Option<(f64, f64)>,
    first_present: bool,
    result: Option<Result<()>>,
}

fn main() -> Result<()> {
    let start = Instant::now(); // stands in for "hotkey pressed"
    let mut args = std::env::args().skip(1);
    let mut display = 0usize;
    let mut out_dir = PathBuf::from("spike-out/overlay");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--display" => display = args.next().context("--display N")?.parse()?,
            "--out" => out_dir = PathBuf::from(args.next().context("--out DIR")?),
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    std::fs::create_dir_all(&out_dir)?;

    let monitors = Monitor::all().context("Monitor::all (screen-recording permission?)")?;
    let m = monitors
        .get(display)
        .context("no such display index; run spike-shot to list them")?;
    let t = Instant::now();
    let image = m.capture_image()?;
    let capture_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "captured display {display}: {}x{} px in {capture_ms:.1} ms (xcap pos {},{} size {}x{} scale {:.2})",
        image.width(),
        image.height(),
        m.x()?,
        m.y()?,
        m.width()?,
        m.height()?,
        m.scale_factor().unwrap_or(1.0)
    );
    let shot = Shot {
        image,
        xcap_pos: (m.x()?, m.y()?),
        scale: m.scale_factor().unwrap_or(1.0),
    };

    let event_loop = EventLoop::new()?;
    let mut app = App {
        timing: Timing {
            start,
            capture_ms,
            window_start: None,
            window_ms: 0.0,
        },
        shot,
        out_dir,
        window: None,
        surface: None,
        mon_pos: (0, 0),
        offset: (0, 0),
        bright: Vec::new(),
        dimmed: Vec::new(),
        win_size: (0, 0),
        cursor: (0.0, 0.0),
        drag_start: None,
        first_present: true,
        result: None,
    };
    event_loop.run_app(&mut app)?;
    match app.result {
        Some(r) => r,
        None => {
            println!("cancelled");
            Ok(())
        }
    }
}

impl App {
    /// Pick the winit monitor that matches the captured xcap display.
    fn find_monitor(&self, el: &ActiveEventLoop) -> Option<MonitorHandle> {
        let (img_w, img_h) = (self.shot.image.width(), self.shot.image.height());
        // xcap gives points on macOS, physical px on Windows; winit gives physical.
        let phys_pos = if cfg!(target_os = "macos") {
            (
                (self.shot.xcap_pos.0 as f32 * self.shot.scale).round() as i32,
                (self.shot.xcap_pos.1 as f32 * self.shot.scale).round() as i32,
            )
        } else {
            self.shot.xcap_pos
        };
        el.available_monitors()
            .min_by_key(|mh| {
                let p = mh.position();
                let s = mh.size();
                let size_penalty = if (s.width, s.height) == (img_w, img_h) {
                    0
                } else {
                    1_000_000
                };
                size_penalty + (p.x - phys_pos.0).abs() + (p.y - phys_pos.1).abs()
            })
            .or_else(|| el.primary_monitor())
    }

    /// Build bright and dimmed 0RGB frames at window size. Window pixel (x, y)
    /// shows capture pixel (x + offset.x, y + offset.y), 1:1; pixels outside the
    /// capture are black.
    fn prepare_frames(&mut self, w: u32, h: u32) {
        let img = &self.shot.image;
        let (iw, ih) = (img.width() as i64, img.height() as i64);
        let raw = img.as_raw();
        let (ox, oy) = (self.offset.0 as i64, self.offset.1 as i64);
        let n = (w as usize) * (h as usize);
        self.bright = vec![0; n];
        self.dimmed = vec![0; n];
        for y in 0..h as i64 {
            let sy = y + oy;
            if sy < 0 || sy >= ih {
                continue;
            }
            for x in 0..w as i64 {
                let sx = x + ox;
                if sx < 0 || sx >= iw {
                    continue;
                }
                let i = ((sy * iw + sx) * 4) as usize;
                let (r, g, b) = (raw[i] as u32, raw[i + 1] as u32, raw[i + 2] as u32);
                let o = (y * w as i64 + x) as usize;
                self.bright[o] = (r << 16) | (g << 8) | b;
                self.dimmed[o] = ((r * DIM / 100) << 16) | ((g * DIM / 100) << 8) | (b * DIM / 100);
            }
        }
        self.win_size = (w, h);
    }

    fn selection(&self) -> Option<(u32, u32, u32, u32)> {
        let (sx, sy) = self.drag_start?;
        let (cx, cy) = self.cursor;
        let (w, h) = self.win_size;
        let clamp = |v: f64, max: u32| v.round().clamp(0.0, max as f64) as u32;
        let (x0, x1) = (clamp(sx.min(cx), w), clamp(sx.max(cx), w));
        let (y0, y1) = (clamp(sy.min(cy), h), clamp(sy.max(cy), h));
        Some((x0, y0, x1 - x0, y1 - y0))
    }

    fn current_offset(&self) -> (i32, i32) {
        match self.window.as_ref().map(|w| w.inner_position()) {
            Some(Ok(p)) => (p.x - self.mon_pos.0, p.y - self.mon_pos.1),
            _ => (0, 0),
        }
    }

    /// Make the softbuffer surface and the pre-rendered frames match the
    /// window's size and offset. Called before every redraw. Nothing here may
    /// panic: we run inside AppKit callbacks, where a panic aborts the app.
    fn sync_size(&mut self, size: PhysicalSize<u32>) -> bool {
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return false;
        };
        let offset = self.current_offset();
        if (size.width, size.height) == self.win_size
            && offset == self.offset
            && !self.dimmed.is_empty()
        {
            return true;
        }
        let Some(surface) = self.surface.as_mut() else {
            return false;
        };
        if let Err(e) = surface.resize(w, h) {
            eprintln!(
                "surface resize to {}x{} failed: {e}",
                size.width, size.height
            );
            return false;
        }
        self.offset = offset;
        let (iw, ih) = (self.shot.image.width(), self.shot.image.height());
        let covers = offset == (0, 0) && (size.width, size.height) == (iw, ih);
        println!(
            "window {}x{} px at offset {:?} vs capture {iw}x{ih} px -> {}",
            size.width,
            size.height,
            offset,
            if covers {
                "covers the whole display, 1:1"
            } else {
                "1:1 but DOES NOT cover the whole display (note in docs/SPIKES.md if this is the final line)"
            }
        );
        self.prepare_frames(size.width, size.height);
        true
    }

    fn redraw(&mut self) {
        let Some(size) = self.window.as_ref().map(|w| w.inner_size()) else {
            return;
        };
        if !self.sync_size(size) {
            return;
        }
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let (w, h) = self.win_size;
        let mut buf = match surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("buffer_mut: {e}");
                return;
            }
        };
        if buf.len() != self.dimmed.len() {
            eprintln!(
                "skipping frame: buffer {} px vs frame {} px",
                buf.len(),
                self.dimmed.len()
            );
            return;
        }
        buf.copy_from_slice(&self.dimmed);
        if let Some((x, y, sw, sh)) = self.drag_start.map(|(sx, sy)| {
            let (cx, cy) = self.cursor;
            let c = |v: f64, m: u32| v.round().clamp(0.0, m as f64) as u32;
            let (x0, x1) = (c(sx.min(cx), w), c(sx.max(cx), w));
            let (y0, y1) = (c(sy.min(cy), h), c(sy.max(cy), h));
            (x0, y0, x1 - x0, y1 - y0)
        }) {
            for row in y..y + sh {
                let a = (row * w + x) as usize;
                buf[a..a + sw as usize].copy_from_slice(&self.bright[a..a + sw as usize]);
            }
            // 1px border just outside the selection so it never covers selected pixels
            let (bx0, by0) = (x.saturating_sub(1), y.saturating_sub(1));
            let (bx1, by1) = ((x + sw).min(w - 1), (y + sh).min(h - 1));
            for xx in bx0..=bx1 {
                buf[(by0 * w + xx) as usize] = ACCENT;
                buf[(by1 * w + xx) as usize] = ACCENT;
            }
            for yy in by0..=by1 {
                buf[(yy * w + bx0) as usize] = ACCENT;
                buf[(yy * w + bx1) as usize] = ACCENT;
            }
        }
        if let Err(e) = buf.present() {
            eprintln!("present: {e}");
        }
        if self.first_present {
            self.first_present = false;
            let t = &self.timing;
            let total = t.start.elapsed().as_secs_f64() * 1e3;
            let first_frame = t
                .window_start
                .map(|s| s.elapsed().as_secs_f64() * 1e3 - t.window_ms)
                .unwrap_or(0.0);
            // In the product the overlay window is created once at startup and
            // kept hidden, so hotkey -> visible = capture + show + first frame.
            let hot_path = t.capture_ms + first_frame;
            println!(
                "timing: capture {:.0} ms, window creation {:.0} ms, first frame {:.0} ms, total {:.0} ms",
                t.capture_ms, t.window_ms, first_frame, total
            );
            println!(
                "hotkey path (capture + first frame, window pre-created as in the product): {hot_path:.0} ms (budget {BUDGET_MS:.0} ms) -> {}",
                if hot_path <= BUDGET_MS { "ok" } else { "TOO SLOW" }
            );
        }
    }

    fn save_selection(&mut self) -> Result<()> {
        let (x, y, w, h) = self.selection().context("no selection")?;
        anyhow::ensure!(w > 0 && h > 0, "empty selection");
        let (iw, ih) = (
            self.shot.image.width() as i64,
            self.shot.image.height() as i64,
        );
        // window px -> capture px: add the window's offset on the display (1:1, never scaled)
        let x0 = (x as i64 + self.offset.0 as i64).clamp(0, iw);
        let y0 = (y as i64 + self.offset.1 as i64).clamp(0, ih);
        let x1 = (x as i64 + w as i64 + self.offset.0 as i64).clamp(0, iw);
        let y1 = (y as i64 + h as i64 + self.offset.1 as i64).clamp(0, ih);
        anyhow::ensure!(
            x1 > x0 && y1 > y0,
            "selection is outside the captured display"
        );
        let (cx, cy, cw, ch) = (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32);
        let crop = xcap::image::imageops::crop_imm(&self.shot.image, cx, cy, cw, ch).to_image();
        let path = self.out_dir.join(format!("region-{cx}-{cy}-{cw}x{ch}.png"));
        crop.save(&path)?;
        println!("selection window px:  x={x} y={y} {w}x{h}");
        println!(
            "selection capture px: x={cx} y={cy} {cw}x{ch}  -> {}",
            path.display()
        );
        println!(
            "{}",
            if (cw, ch) == (w, h) {
                "crop size matches the drag exactly"
            } else {
                "CROP SIZE DIFFERS from the drag (clipped at the display edge?)"
            }
        );
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let t = Instant::now();
        self.timing.window_start = Some(t);
        let monitor = self.find_monitor(el);
        if let Some(mh) = &monitor {
            println!(
                "winit monitor: {:?} pos {:?} size {:?} scale {:.2}",
                mh.name(),
                mh.position(),
                mh.size(),
                mh.scale_factor()
            );
            self.mon_pos = (mh.position().x, mh.position().y);
        }

        let mut attrs = Window::default_attributes()
            .with_title("CaptureMann overlay spike")
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop);
        if let Some(mh) = &monitor {
            attrs = attrs
                .with_position(mh.position())
                .with_inner_size(mh.size());
        }
        #[cfg(not(target_os = "macos"))]
        {
            // Windows/Linux: borderless fullscreen covers the taskbar.
            attrs =
                attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(monitor.clone())));
        }

        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                self.result = Some(Err(e.into()));
                el.exit();
                return;
            }
        };

        #[cfg(target_os = "macos")]
        if let Err(e) = mac::cover_screen(&window) {
            eprintln!("could not raise the overlay above the menu bar: {e}");
        }

        window.set_cursor(CursorIcon::Crosshair);
        window.focus_window();
        let surface =
            SbContext::new(window.clone()).and_then(|ctx| Surface::new(&ctx, window.clone()));
        match surface {
            Ok(s) => self.surface = Some(s),
            Err(e) => {
                self.result = Some(Err(anyhow::anyhow!("softbuffer surface: {e}")));
                el.exit();
                return;
            }
        }
        self.window = Some(window.clone());
        self.timing.window_ms = t.elapsed().as_secs_f64() * 1e3;
        window.request_redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_)
            | WindowEvent::Moved(_)
            | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CursorMoved {
                position: PhysicalPosition { x, y },
                ..
            } => {
                self.cursor = (x, y);
                if self.drag_start.is_some() {
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.drag_start = Some(self.cursor),
                ElementState::Released => {
                    self.result = Some(self.save_selection());
                    el.exit();
                }
            },
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                el.exit();
            }
            _ => {}
        }
    }
}

/// macOS: raise the window above the menu bar and give it the screen's full
/// frame, including the notch strip. Uses plain Objective-C messages on the
/// NSWindow behind winit's view.
#[cfg(target_os = "macos")]
mod mac {
    use objc2::{
        class,
        encode::{Encode, Encoding},
        msg_send,
        runtime::AnyObject,
    };
    use winit::{
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding =
            Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }

    /// NSScreenSaverWindowLevel: above the menu bar and the Dock.
    const SCREEN_SAVER_LEVEL: isize = 1000;
    // NSWindowCollectionBehavior: CanJoinAllSpaces | Stationary | IgnoresCycle | FullScreenAuxiliary
    const BEHAVIOR: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);

    pub fn cover_screen(window: &Window) -> Result<(), String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return Err("not an AppKit window".into());
        };
        // SAFETY: ns_view is a live NSView owned by winit for the lifetime of
        // `window`; all messages below are standard NSWindow/NSScreen API.
        unsafe {
            let view = h.ns_view.as_ptr() as *mut AnyObject;
            let ns_window: *mut AnyObject = msg_send![view, window];
            if ns_window.is_null() {
                return Err("view has no NSWindow".into());
            }
            let mut screen: *mut AnyObject = msg_send![ns_window, screen];
            if screen.is_null() {
                screen = msg_send![class!(NSScreen), mainScreen];
            }
            if screen.is_null() {
                return Err("no NSScreen".into());
            }
            let frame: CGRect = msg_send![screen, frame];
            let _: () = msg_send![ns_window, setLevel: SCREEN_SAVER_LEVEL];
            let _: () = msg_send![ns_window, setCollectionBehavior: BEHAVIOR];
            let _: () = msg_send![ns_window, setHasShadow: false];
            let _: () = msg_send![ns_window, setFrame: frame, display: true];
            println!(
                "macOS: overlay raised above the menu bar, frame {}x{} pt at ({}, {})",
                frame.size.width, frame.size.height, frame.origin.x, frame.origin.y
            );
        }
        Ok(())
    }
}
