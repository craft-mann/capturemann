//! `xcap`-based backend (macOS and Windows). Reference: spikes/spike-shot, spikes/spike-overlay.
//!
//! Coordinate spaces: on macOS xcap reports displays and windows in points (logical), so this
//! backend multiplies by the display scale to report physical px. Display origins are scaled by
//! their own display's scale; a window's origin is its display's physical origin plus its
//! offset inside that display scaled by that display's scale (R1-2), so on mixed-DPI desktops
//! `window.bounds - display.bounds.origin` is exact for `window.display_id`, while rects of
//! different displays may overlap in the global space (clients must match on `display_id`). On Windows xcap already
//! reports physical px. Captured images are physical on both.
//!
//! Windows paths compile but have not been run (no Windows machine available to the author).

use cm_core::{CmError, PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};
use xcap::{Monitor, Window, XCapError};

use crate::{CaptureBackend, DisplayInfo, Frame, WindowInfo};

/// Real-screen backend. Stateless; every call asks the OS.
#[derive(Debug, Default, Clone, Copy)]
pub struct XcapBackend;

impl XcapBackend {
    pub fn new() -> Self {
        XcapBackend
    }
}

/// One pass over the Window Server's on-screen window list.
///
/// xcap answers every property of a window (`x`, `title`, `pid`, `current_monitor`, ...) with a
/// fresh `CGWindowListCopyWindowInfo` and a linear scan, about ten calls per window: with 25
/// windows that was 250 window-list copies (hundreds of ms). The list already holds all of it,
/// so it is read once here. Window Server layers (`kCGWindowLayer`): layer 0 is the normal
/// application window level; the menu bar, Dock, Control Center, status items and notification
/// banners sit above it.
#[cfg(target_os = "macos")]
mod listing {
    use std::ffi::c_void;

    use objc2_core_foundation::{
        CFBoolean, CFDictionary, CFNumber, CFNumberType, CFString, CGRect,
    };
    use objc2_core_graphics::{
        kCGWindowBounds, kCGWindowIsOnscreen, kCGWindowLayer, kCGWindowName, kCGWindowNumber,
        kCGWindowOwnerName, kCGWindowOwnerPID, kCGWindowSharingState,
        CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption,
    };

    /// A window as the Window Server lists it; geometry in points, front to back order.
    #[derive(Debug, Clone)]
    pub struct RawWindow {
        pub id: u32,
        pub layer: i32,
        pub title: String,
        pub app: String,
        pub pid: u32,
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
        pub on_screen: bool,
    }

    fn value(dict: &CFDictionary, key: &CFString) -> Option<*const c_void> {
        // SAFETY: `key` is a live CFString; `value` returns null or a pointer borrowed from
        // `dict`, which outlives every use of it below.
        let v = unsafe { dict.value((key as *const CFString).cast()) };
        (!v.is_null()).then_some(v)
    }

    fn int(dict: &CFDictionary, key: &CFString) -> Option<i32> {
        let v = value(dict, key)?;
        let mut out: i32 = 0;
        // SAFETY: the value under these keys is a CFNumber owned by `dict`; `out` is a valid
        // i32 for `IntType`.
        unsafe {
            (*v.cast::<CFNumber>()).value(
                CFNumberType::IntType,
                (&mut out as *mut i32).cast::<c_void>(),
            )
        }
        .then_some(out)
    }

    fn string(dict: &CFDictionary, key: &CFString) -> Option<String> {
        let v = value(dict, key)?;
        // SAFETY: the value under these keys is a CFString owned by `dict`.
        Some(unsafe { (*v.cast::<CFString>()).to_string() })
    }

    fn boolean(dict: &CFDictionary, key: &CFString) -> Option<bool> {
        let v = value(dict, key)?;
        // SAFETY: the value under this key is a CFBoolean owned by `dict`.
        Some(unsafe { (*v.cast::<CFBoolean>()).value() })
    }

    /// The on-screen windows, front to back. Windows xcap would not list (no name or owner
    /// key, sharing state 0, the "StatusIndicator" of the Window Server) are left out.
    pub fn windows() -> Vec<RawWindow> {
        let mut out = Vec::new();
        // SAFETY: plain CoreGraphics queries; the array and the dictionaries it holds are
        // owned by the retained array for the whole loop, and the key statics are immutable
        // `'static` CFStrings.
        unsafe {
            let Some(list) = CGWindowListCopyWindowInfo(
                CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
                0,
            ) else {
                return out;
            };
            for i in 0..list.count() {
                let ptr = list.value_at_index(i).cast::<CFDictionary>();
                if ptr.is_null() {
                    continue;
                }
                let dict = &*ptr;
                let (Some(title), Some(app), Some(sharing), Some(id)) = (
                    string(dict, kCGWindowName),
                    string(dict, kCGWindowOwnerName),
                    int(dict, kCGWindowSharingState),
                    int(dict, kCGWindowNumber),
                ) else {
                    continue;
                };
                if sharing == 0 || (title == "StatusIndicator" && app == "Window Server") {
                    continue;
                }
                let Ok(id) = u32::try_from(id) else {
                    continue;
                };
                let Some(bounds) = value(dict, kCGWindowBounds) else {
                    continue;
                };
                let mut rect = CGRect::default();
                if !CGRectMakeWithDictionaryRepresentation(
                    Some(&*bounds.cast::<CFDictionary>()),
                    &mut rect,
                ) {
                    continue;
                }
                out.push(RawWindow {
                    id,
                    layer: int(dict, kCGWindowLayer).unwrap_or(0),
                    title,
                    app,
                    pid: int(dict, kCGWindowOwnerPID).map_or(0, |p| p as u32),
                    x: rect.origin.x,
                    y: rect.origin.y,
                    width: rect.size.width,
                    height: rect.size.height,
                    on_screen: boolean(dict, kCGWindowIsOnscreen).unwrap_or(false),
                });
            }
        }
        out
    }
}

fn map_err(e: XCapError) -> CmError {
    let msg = e.to_string();
    if msg.to_lowercase().contains("not found") {
        CmError::not_found(msg)
    } else {
        CmError::internal(msg)
    }
}

fn frame_from(img: xcap::image::RgbaImage) -> Result<Frame, CmError> {
    Frame::new(img.width(), img.height(), img.into_raw())
}

/// xcap geometry to physical px.
fn to_physical(x: i32, y: i32, w: u32, h: u32, scale: f64) -> PhysicalRect {
    if cfg!(target_os = "macos") {
        let s = ScaleFactor::new(scale).unwrap_or(ScaleFactor::ONE);
        let left = (f64::from(x) * scale).round();
        let top = (f64::from(y) * scale).round();
        PhysicalRect {
            origin: PhysicalPoint {
                x: left as i32,
                y: top as i32,
            },
            size: s.size_to_physical(cm_core::Size::new(f64::from(w), f64::from(h))),
        }
    } else {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }
}

fn monitor_scale(m: &Monitor) -> ScaleFactor {
    m.scale_factor()
        .ok()
        .and_then(|s| ScaleFactor::new(f64::from(s)).ok())
        .unwrap_or(ScaleFactor::ONE)
}

/// Whether the display is the laptop's own panel. xcap's monitor id is the `CGDirectDisplayID`.
#[cfg(target_os = "macos")]
fn display_is_builtin(id: u32) -> bool {
    objc2_core_graphics::CGDisplayIsBuiltin(id)
}

/// Windows: not determined yet (the display filter shows every display as external).
#[cfg(not(target_os = "macos"))]
fn display_is_builtin(_id: u32) -> bool {
    false
}

fn display_info(m: &Monitor) -> Result<DisplayInfo, XCapError> {
    let scale = monitor_scale(m);
    let name = m
        .friendly_name()
        .or_else(|_| m.name())
        .unwrap_or_else(|_| String::new());
    Ok(DisplayInfo {
        id: m.id()?,
        name,
        bounds: to_physical(m.x()?, m.y()?, m.width()?, m.height()?, scale.get()),
        scale,
        is_primary: m.is_primary().unwrap_or(false),
        is_builtin: display_is_builtin(m.id()?),
    })
}

/// A monitor in the OS's own units (points on macOS), for placing windows on displays.
#[cfg(target_os = "macos")]
struct MonitorPoints {
    id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: ScaleFactor,
    primary: bool,
}

#[cfg(target_os = "macos")]
impl MonitorPoints {
    fn of(m: &Monitor) -> Result<Self, XCapError> {
        Ok(MonitorPoints {
            id: m.id()?,
            x: f64::from(m.x()?),
            y: f64::from(m.y()?),
            width: f64::from(m.width()?),
            height: f64::from(m.height()?),
            scale: monitor_scale(m),
            primary: m.is_primary().unwrap_or(false),
        })
    }

    fn contains_center(&self, w: &listing::RawWindow) -> bool {
        let (cx, cy) = (w.x + w.width / 2.0, w.y + w.height / 2.0);
        cx >= self.x && cx < self.x + self.width && cy >= self.y && cy < self.y + self.height
    }

    fn overlaps(&self, w: &listing::RawWindow) -> bool {
        w.x < self.x + self.width
            && w.x + w.width > self.x
            && w.y < self.y + self.height
            && w.y + w.height > self.y
    }

    /// Where this monitor starts in the global physical space (what `display_info` reports).
    fn physical_origin(&self) -> PhysicalPoint {
        PhysicalPoint {
            x: (self.x * self.scale.get()).round() as i32,
            y: (self.y * self.scale.get()).round() as i32,
        }
    }
}

/// A window rect given in points, expressed in physical px of the display it is on.
///
/// The global point space is mixed-DPI: a 1x display at x = 1512 pt next to a 2x display that
/// spans 0..1512 pt. Scaling every origin by its own item's scale (the old behavior) put the
/// 1x display at physical x = 1512 while the 2x display spans 0..3024 (R1-2). Here the window
/// offset inside its display is scaled with that display's scale and added to the display's
/// physical origin, so `bounds - display.bounds.origin` is the window's position in the
/// display's own pixels, and `window.display_id` says which display that is.
#[cfg(any(target_os = "macos", test))]
fn rect_on_display(
    window_pt: (f64, f64, f64, f64),
    display_origin_pt: (f64, f64),
    display_origin_phys: PhysicalPoint,
    scale: f64,
) -> PhysicalRect {
    let (x, y, w, h) = window_pt;
    // Offsets are rounded, so a window flush with its display edge lands exactly on it.
    let dx = ((x - display_origin_pt.0) * scale).round() as i32;
    let dy = ((y - display_origin_pt.1) * scale).round() as i32;
    PhysicalRect {
        origin: PhysicalPoint {
            x: display_origin_phys.x.saturating_add(dx),
            y: display_origin_phys.y.saturating_add(dy),
        },
        size: PhysicalSize {
            width: (w * scale).round().max(0.0) as u32,
            height: (h * scale).round().max(0.0) as u32,
        },
    }
}

/// The monitor holding the window's CENTER (R1-19: the first merely overlapping monitor used
/// to win), else one it overlaps, else the primary one.
#[cfg(target_os = "macos")]
fn monitor_for<'a>(
    w: &listing::RawWindow,
    monitors: &'a [MonitorPoints],
) -> Option<&'a MonitorPoints> {
    monitors
        .iter()
        .find(|m| m.contains_center(w))
        .or_else(|| monitors.iter().find(|m| m.overlaps(w)))
        .or_else(|| monitors.iter().find(|m| m.primary))
}

/// Same rules as xcap's `is_minimized` (off screen and smaller than the monitor).
#[cfg(target_os = "macos")]
fn window_from_raw(w: &listing::RawWindow, monitors: &[MonitorPoints]) -> WindowInfo {
    let monitor = monitor_for(w, monitors);
    let scale = monitor.map_or(ScaleFactor::ONE, |m| m.scale);
    let maximized = monitor
        .is_some_and(|m| w.width as u32 >= m.width as u32 && w.height as u32 >= m.height as u32);
    let bounds = match monitor {
        Some(m) => rect_on_display(
            (w.x, w.y, w.width, w.height),
            (m.x, m.y),
            m.physical_origin(),
            scale.get(),
        ),
        None => to_physical(
            w.x.round() as i32,
            w.y.round() as i32,
            w.width.round() as u32,
            w.height.round() as u32,
            1.0,
        ),
    };
    WindowInfo {
        id: w.id,
        title: w.title.clone(),
        app: w.app.clone(),
        pid: w.pid,
        bounds,
        scale,
        display_id: monitor.map(|m| m.id),
        minimized: !w.on_screen && !maximized,
    }
}

#[cfg(not(target_os = "macos"))]
fn window_info(w: &Window) -> Result<WindowInfo, XCapError> {
    let monitor = w.current_monitor().ok();
    let scale = monitor.as_ref().map_or(ScaleFactor::ONE, monitor_scale);
    Ok(WindowInfo {
        id: w.id()?,
        title: w.title().unwrap_or_default(),
        app: w.app_name().unwrap_or_default(),
        pid: w.pid().unwrap_or(0),
        bounds: to_physical(w.x()?, w.y()?, w.width()?, w.height()?, scale.get()),
        scale,
        display_id: monitor.and_then(|m| m.id().ok()),
        minimized: w.is_minimized().unwrap_or(false),
    })
}

impl CaptureBackend for XcapBackend {
    fn check_permission(&self) -> Result<(), CmError> {
        crate::sys::permission::check()
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
        self.check_permission()?;
        let monitors = Monitor::all().map_err(map_err)?;
        monitors
            .iter()
            .map(|m| display_info(m).map_err(map_err))
            .collect()
    }

    #[cfg(target_os = "macos")]
    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        self.check_permission()?;
        // One window-list pass (see `listing`) and one monitor pass, instead of xcap's
        // window-list copy per property. Only layer 0 holds document windows (the menu bar,
        // Dock, Control Center and other shell windows live above it, CAP-3).
        let monitors: Vec<MonitorPoints> = Monitor::all()
            .map_err(map_err)?
            .iter()
            .filter_map(|m| MonitorPoints::of(m).ok())
            .collect();
        Ok(listing::windows()
            .into_iter()
            .filter(|w| w.layer == 0)
            .map(|w| window_from_raw(&w, &monitors))
            .collect())
    }

    #[cfg(not(target_os = "macos"))]
    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        self.check_permission()?;
        // A window can vanish between enumeration and inspection; skip those. On Windows xcap
        // already skips invisible, cloaked and tool windows (`WS_EX_TOOLWINDOW`).
        Ok(Window::all()
            .map_err(map_err)?
            .iter()
            .filter_map(|w| window_info(w).ok())
            .collect())
    }

    fn capture_display(&self, display_id: u32) -> Result<Frame, CmError> {
        self.check_permission()?;
        let monitor = Monitor::all()
            .map_err(map_err)?
            .into_iter()
            .find(|m| m.id().is_ok_and(|id| id == display_id))
            .ok_or_else(|| CmError::not_found(format!("Display {display_id} not found")))?;
        frame_from(monitor.capture_image().map_err(map_err)?)
    }

    #[cfg(target_os = "macos")]
    fn capture_display_excluding(
        &self,
        display_id: u32,
        size: PhysicalSize,
        exclude_pid: u32,
    ) -> Result<Option<Frame>, CmError> {
        self.check_permission()?;
        crate::sck_shot::capture_display_excluding(
            display_id,
            (size.width, size.height),
            &crate::sck_shot::Exclusions::pid(exclude_pid),
        )
    }

    #[cfg(target_os = "macos")]
    fn capture_display_excluding_except(
        &self,
        display_id: u32,
        size: PhysicalSize,
        exclude_pid: u32,
        keep_window_ids: &[u32],
    ) -> Result<Option<Frame>, CmError> {
        self.check_permission()?;
        crate::sck_shot::capture_display_excluding(
            display_id,
            (size.width, size.height),
            &crate::sck_shot::Exclusions::pid(exclude_pid).keeping(keep_window_ids.to_vec()),
        )
    }

    fn capture_window(&self, window_id: u32) -> Result<Frame, CmError> {
        self.check_permission()?;
        let window = Window::all()
            .map_err(map_err)?
            .into_iter()
            .find(|w| w.id().is_ok_and(|id| id == window_id))
            .ok_or_else(|| CmError::not_found(format!("Window {window_id} not found")))?;
        frame_from(window.capture_image().map_err(map_err)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R1-2: a 2x display at 0..1512 pt next to a 1x display at 1512 pt. The 1x display's
    /// physical origin is 1512 (its own scale), the 2x display's span is 0..3024: the global
    /// physical space overlaps, so a window must be placed from its own display's origin.
    #[test]
    fn r1_2_mixed_dpi_window_is_placed_relative_to_its_display() {
        let origin_1x = PhysicalPoint { x: 1512, y: 0 };
        // 100 pt into the 1x display: 100 px, not 200.
        let r = rect_on_display((1612.0, 50.0, 400.0, 300.0), (1512.0, 0.0), origin_1x, 1.0);
        assert_eq!(r.origin, PhysicalPoint { x: 1612, y: 50 });
        assert_eq!(r.size.width, 400);
        // 100 pt into the 2x display: 200 px.
        let r2 = rect_on_display(
            (100.0, 50.0, 400.0, 300.0),
            (0.0, 0.0),
            PhysicalPoint { x: 0, y: 0 },
            2.0,
        );
        assert_eq!(r2.origin, PhysicalPoint { x: 200, y: 100 });
        assert_eq!(r2.size.width, 800);
    }

    /// Half-point window origins are rounded, not truncated (R1-19).
    #[test]
    fn r1_19_origins_are_rounded() {
        let r = rect_on_display(
            (10.75, 20.5, 100.0, 100.0),
            (0.0, 0.0),
            PhysicalPoint { x: 0, y: 0 },
            1.0,
        );
        assert_eq!(r.origin, PhysicalPoint { x: 11, y: 21 });
    }

    #[cfg(target_os = "macos")]
    fn mon(id: u32, x: f64, w: f64, scale: f64, primary: bool) -> MonitorPoints {
        MonitorPoints {
            id,
            x,
            y: 0.0,
            width: w,
            height: 900.0,
            scale: ScaleFactor::new(scale).unwrap(),
            primary,
        }
    }

    #[cfg(target_os = "macos")]
    fn raw(x: f64, w: f64) -> listing::RawWindow {
        listing::RawWindow {
            id: 1,
            layer: 0,
            title: "t".into(),
            app: "a".into(),
            pid: 1,
            x,
            y: 100.0,
            width: w,
            height: 300.0,
            on_screen: true,
        }
    }

    /// R1-19 + R1-2: a window straddling two displays belongs to the one holding its center,
    /// and its bounds are relative to that display's physical origin.
    #[cfg(target_os = "macos")]
    #[test]
    fn r1_19_window_goes_to_the_display_holding_its_center() {
        let monitors = vec![
            mon(1, 0.0, 1512.0, 2.0, true),
            mon(2, 1512.0, 1920.0, 1.0, false),
        ];
        // Mostly on display 2 (center at 1512 + 300).
        let w = window_from_raw(&raw(1400.0, 800.0), &monitors);
        assert_eq!(w.display_id, Some(2));
        assert_eq!(w.scale.get(), 1.0);
        // Display 2's physical origin is 1512; the window starts 112 pt left of it.
        assert_eq!(w.bounds.origin.x, 1512 - 112);
        // Mostly on display 1.
        let w = window_from_raw(&raw(100.0, 1600.0), &monitors);
        assert_eq!(w.display_id, Some(1));
        assert_eq!(w.bounds.origin.x, 200);
    }
}
