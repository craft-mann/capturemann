//! The only place that reaches below GPUI to the native window: hiding a window from screen
//! capture, making it click-through and moving it (REC-5, REC-6, CAM-1).
//!
//! * `exclude_from_capture`: `NSWindow.sharingType = none` on macOS, the belt-and-braces twin of the
//!   engine's own exclusion of this process's windows (`EngineOptions::own_pid`): the HUD, the
//!   countdown, the area indicator and the webcam bubble must never be in `screen.mp4`
//!   (REC-5.AC9, CAM-1.AC2). The library window is the one exception, captured on purpose
//!   (`capture_window_id`, REC-6.AC8). Windows would use `WDA_EXCLUDEFROMCAPTURE`; that path is not built yet.
//! * `click_through`: `ignoresMouseEvents`, for the area indicator that covers a display.
//! * `move_to` / `frame`: GPUI 0.3.9 can open a window at a position but cannot move one, and the
//!   HUD and the bubble snap after a drag.
//!
//! Coordinates are GPUI's: global points, origin top-left of the primary display, y down.

use gpui::{px, size, App, Bounds, TitlebarOptions, Window, WindowBounds, WindowOptions};

/// The application id of every window: the Wayland `app_id` and the X11 `WM_CLASS`. It must be
/// the name of the installed `.desktop` file (`dev.capturemann.app.desktop`) so GNOME and KDE
/// match a window to its launcher entry and icon. Ignored by macOS and Windows.
pub const APP_ID: &str = "dev.capturemann.app";

/// The ONE place a normal (decorated, movable, resizable) application window gets its options:
/// the Studio window. Sizes are logical px. On X11 and Wayland the window carries [`APP_ID`] and
/// the app icon; whether the title bar is drawn by the compositor or by the app is decided at
/// render time (`ui::csd`), because only the compositor's answer to the first configure says
/// (GNOME has no server-side decorations, KDE and most X11 window managers do).
pub fn normal_window_options(
    title: &str,
    size_px: (f32, f32),
    min_px: (f32, f32),
    cx: &App,
) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(size_px.0), px(size_px.1)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.to_owned().into()),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        window_min_size: Some(size(px(min_px.0), px(min_px.1))),
        app_id: Some(APP_ID.into()),
        icon: imp::window_icon(),
        ..Default::default()
    }
}

/// The base of every pop-up window's options (`..popup_base()` in place of
/// `..Default::default()`): the same [`APP_ID`], so GNOME groups the pop-ups under the app's
/// launcher entry instead of listing unknown windows.
pub fn popup_base() -> WindowOptions {
    WindowOptions {
        app_id: Some(APP_ID.into()),
        icon: imp::window_icon(),
        ..Default::default()
    }
}

/// Whether the desktop cannot show a separate toast window without taking focus: a Wayland
/// session. The compositor places a new top-level window where it likes and there is no
/// always-on-top or "do not focus" request, so toasts become banners inside the library
/// window while it is the active window (`ui::app::toasts`).
pub fn toasts_belong_in_window() -> bool {
    imp::toasts_belong_in_window()
}

/// Whether this platform has no way to exclude a window from capture, so the app hides its
/// own pop-ups around a capture instead (`capture_cloak`): Linux.
pub fn needs_capture_cloak() -> bool {
    imp::needs_capture_cloak()
}

/// Whether the session is Wayland, from the environment GPUI itself reads: `XDG_SESSION_TYPE`
/// says so, or `WAYLAND_DISPLAY` is set and there is no X11 `DISPLAY` to prefer.
pub fn is_wayland_session(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> bool {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    match xdg_session_type {
        Some("wayland") => set(wayland_display),
        Some("x11") => false,
        _ => set(wayland_display) && !set(display),
    }
}

/// What was applied (for logging and the manual test).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Applied {
    pub excluded_from_capture: bool,
    pub click_through: bool,
}

/// What to change on a window.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tweaks {
    pub exclude_from_capture: bool,
    pub click_through: bool,
    /// Raise the window to this NSWindow level (macOS). Above the menu bar (level 24 and up)
    /// AppKit stops clamping the window below it, which the full-display indicator needs.
    pub level: Option<isize>,
    /// Remove the window server's shadow and make the backing fully clear: a transparent GPUI
    /// window otherwise keeps a faint rectangular halo around its content.
    pub no_shadow: bool,
}

/// Applies `tweaks` to the native window behind `window`. A failure leaves the window as it was
/// (the engine still excludes this process's windows by pid) and says why.
pub fn apply(window: &Window, tweaks: Tweaks) -> Result<Applied, String> {
    imp::apply(window, tweaks)
}

/// Lets mouse events through the whole window (`true`) or not. The bubble toggles this by hit
/// test so its transparent corners never swallow clicks meant for other apps.
pub fn set_ignores_mouse(window: &Window, ignore: bool) {
    imp::set_ignores_mouse(window, ignore);
}

/// The pointer in global points, top-left origin of the primary display.
pub fn mouse_location() -> Option<(f64, f64)> {
    imp::mouse_location()
}

/// Whether the OS asks for reduced motion (macOS: Accessibility -> Display -> Reduce motion).
/// `false` where the platform cannot say.
pub fn reduce_motion() -> bool {
    imp::reduce_motion()
}

/// Whether the primary mouse button is held down right now (a drag or click in progress).
pub fn left_button_down() -> bool {
    imp::left_button_down()
}

/// Hides `window` from screen capture (`sharingType = none`) and changes nothing else. For the
/// app's floating surfaces other than the recording windows (toasts, sheet, dialogs, launcher):
/// a screenshot taken while one is open must not contain it (R4-6). The debug demo
/// (`CM_DEMO_CAPTURABLE`) leaves windows capturable so they can be screenshotted.
pub fn exclude_from_capture(window: &Window) {
    if crate::recording::demo::capturable() {
        return;
    }
    if let Err(e) = apply(
        window,
        Tweaks {
            exclude_from_capture: true,
            ..Tweaks::default()
        },
    ) {
        log::warn!("could not hide a window from capture: {e}");
    }
}

/// The CGWindowID of `window` when it is on screen (visible and not minimized), `None`
/// otherwise and where the platform has no such id. Only for the library window, which
/// recordings and screenshots include (see `studio_window`).
pub fn capture_window_id(window: &Window) -> Option<u32> {
    imp::capture_window_id(window)
}

/// Moves the window so its top-left corner is at `(x, y)` (global points).
pub fn move_to(window: &Window, x: f64, y: f64) {
    imp::move_to(window, x, y);
}

/// Sets the window's frame (global points, top-left origin), bypassing the menu-bar clamp AppKit
/// applies when a window is first shown (the area indicator must cover the whole display).
pub fn set_frame(window: &Window, x: f64, y: f64, w: f64, h: f64) {
    imp::set_frame(window, x, y, w, h);
}

/// A handle on the native window that outlives the `&Window` borrow. Resizing must not happen
/// inside a GPUI window update: AppKit tells GPUI about the new size synchronously, GPUI's resize
/// callback then needs to update the same window, finds it already being updated and drops the
/// news, so the window is resized but its content is never redrawn at the new size. Take this
/// handle while building the window and call [`NativeWindow::set_frame`] from a spawned task that
/// holds no `App` borrow (not from `cx.defer` or an entity update, which keep the app borrowed).
#[derive(Clone)]
pub struct NativeWindow(imp::Native);

impl NativeWindow {
    /// The native window behind `window`, if the platform has one we can reach.
    pub fn of(window: &Window) -> Option<NativeWindow> {
        imp::native(window).map(NativeWindow)
    }

    /// Sets the frame (global points, top-left origin). Call outside a window update.
    pub fn set_frame(&self, x: f64, y: f64, w: f64, h: f64) {
        imp::native_set_frame(&self.0, x, y, w, h);
    }
}

/// The window's frame as `(x, y, w, h)` in global points, top-left origin.
pub fn frame(window: &Window) -> Option<(f64, f64, f64, f64)> {
    imp::frame(window)
}

/// Raises (or lowers) the window to this NSWindow level without touching its style (macOS).
/// The start sheet sits one above the other pop-ups (101) so a webcam bubble that overlaps it
/// can never cover Start.
pub fn set_level(window: &Window, level: isize) {
    imp::set_level(window, level);
}

/// The display's frame as `(x, y, w, h)` in global points, top-left origin of the primary display.
/// GPUI 0.3.9 reports every display's bounds with origin (0, 0), so the real position of a
/// secondary display (it can be left of or above the primary, with negative coordinates) comes
/// from here. `id` is GPUI's `DisplayId` (the platform display id).
pub fn display_frame(id: u32) -> Option<(f64, f64, f64, f64)> {
    imp::display_frame(id)
}

/// The visible frame (the display minus the menu bar and the Dock) of the display that holds the
/// global point `(x, y)`, as `(x, y, w, h)` in global points, top-left origin. `None` where the
/// platform cannot say (callers fall back to the display bounds).
pub fn visible_frame_at(x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
    imp::visible_frame_at(x, y)
}

/// The frame `(x, y, w, h)` of a `w` by `h` window centered in `visible`, shrunk to fit and kept
/// inside it (a window taller or wider than the visible part gets the visible part's size).
pub fn centered_frame(visible: (f64, f64, f64, f64), w: f64, h: f64) -> (f64, f64, f64, f64) {
    let (vx, vy, vw, vh) = visible;
    let (w, h) = (w.min(vw).max(0.0), h.min(vh).max(0.0));
    (vx + (vw - w) / 2.0, vy + (vh - h) / 2.0, w, h)
}

/// Makes `window` the frontmost, key window of this app and activates the app: for dialogs that
/// must never end up behind the library window (permissions, confirmations). When `visible` is
/// given (the visible frame of the pointer's display, global points) the window is first centered
/// in it, keeping its size. The window level is not changed: it behaves like a normal dialog and
/// does not float over other apps once the user switches away. Call from the main thread, after
/// the window exists (a spawned task or `cx.defer`, not inside the window's own update).
pub fn bring_to_front(window: &Window, visible: Option<(f64, f64, f64, f64)>) {
    if let (Some(v), Some((_, _, w, h))) = (visible, frame(window)) {
        let (x, y, w, h) = centered_frame(v, w, h);
        set_frame(window, x, y, w, h);
    }
    imp::bring_to_front(window);
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Applied, Tweaks};
    use gpui::Window;
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSView, NSWindow, NSWindowSharingType, NSWindowStyleMask};
    use objc2_foundation::NSPoint;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    fn ns_window(window: &Window) -> Result<Retained<NSWindow>, String> {
        let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return Err("not an AppKit window".to_owned());
        };
        // SAFETY: `ns_view` is the live NSView GPUI owns for as long as `window` lives; it is only
        // borrowed for this call, on the main thread (GPUI calls us from its window update).
        let view: &NSView = unsafe { h.ns_view.cast::<NSView>().as_ref() };
        view.window()
            .ok_or_else(|| "the view has no NSWindow".to_owned())
    }

    /// macOS takes the icon from the bundle.
    pub fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
        None
    }

    pub fn toasts_belong_in_window() -> bool {
        false
    }

    /// `NSWindow.sharingType` hides the pop-ups; nothing to cloak.
    pub fn needs_capture_cloak() -> bool {
        false
    }

    fn primary_height() -> Option<f64> {
        let mtm = MainThreadMarker::new()?;
        NSScreen::screens(mtm)
            .firstObject()
            .map(|s| s.frame().size.height)
    }

    pub fn apply(window: &Window, tweaks: Tweaks) -> Result<Applied, String> {
        let w = ns_window(window)?;
        let mut done = Applied::default();
        if tweaks.exclude_from_capture {
            w.setSharingType(NSWindowSharingType::None);
            done.excluded_from_capture = w.sharingType() == NSWindowSharingType::None;
        }
        if let Some(l) = tweaks.level {
            w.setLevel(l);
            // A titled window is clamped below the menu bar; a borderless one is not.
            w.setStyleMask(NSWindowStyleMask::Borderless);
        }
        if tweaks.no_shadow {
            w.setHasShadow(false);
            w.setOpaque(false);
            w.setBackgroundColor(Some(&objc2_app_kit::NSColor::clearColor()));
            w.invalidateShadow();
        }
        if tweaks.click_through {
            w.setIgnoresMouseEvents(true);
            done.click_through = w.ignoresMouseEvents();
        }
        Ok(done)
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CgRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    extern "C" {
        fn CGDisplayBounds(display: u32) -> CgRect;
    }

    pub fn display_frame(id: u32) -> Option<(f64, f64, f64, f64)> {
        // SAFETY: `CGDisplayBounds` is a plain CoreGraphics query (linked by GPUI already) that
        // takes a display id by value and returns a struct of four doubles; an unknown id gives
        // a zero rectangle, which is rejected below.
        let r = unsafe { CGDisplayBounds(id) };
        (r.w > 0.0 && r.h > 0.0).then_some((r.x, r.y, r.w, r.h))
    }

    pub fn capture_window_id(window: &Window) -> Option<u32> {
        let w = ns_window(window).ok()?;
        // A window hidden from capture (`sharingType = none`) cannot be captured anyway.
        if w.sharingType() == NSWindowSharingType::None {
            return None;
        }
        crate::studio_window::capturable_id(w.windowNumber(), w.isVisible(), w.isMiniaturized())
    }

    pub fn set_ignores_mouse(window: &Window, ignore: bool) {
        if let Ok(w) = ns_window(window) {
            if w.ignoresMouseEvents() != ignore {
                w.setIgnoresMouseEvents(ignore);
            }
        }
    }

    pub fn reduce_motion() -> bool {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }

    pub fn left_button_down() -> bool {
        objc2_app_kit::NSEvent::pressedMouseButtons() & 1 != 0
    }

    pub fn mouse_location() -> Option<(f64, f64)> {
        let h = primary_height()?;
        let p = objc2_app_kit::NSEvent::mouseLocation();
        Some((p.x, h - p.y))
    }

    pub fn set_level(window: &Window, level: isize) {
        if let Ok(w) = ns_window(window) {
            w.setLevel(level);
        }
    }

    pub fn bring_to_front(window: &Window) {
        let (Ok(w), Some(mtm)) = (ns_window(window), MainThreadMarker::new()) else {
            return;
        };
        // Activate first (a login item or `open` leaves the app inactive), then order the window
        // in front of every other window of the app and of other apps' windows, and make it key.
        #[allow(deprecated)]
        objc2_app_kit::NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        w.orderFrontRegardless();
        w.makeKeyAndOrderFront(None);
    }

    pub fn visible_frame_at(x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
        let mtm = MainThreadMarker::new()?;
        let screens = NSScreen::screens(mtm);
        let ph = screens.firstObject()?.frame().size.height;
        screens.iter().find_map(|s| {
            let f = s.frame();
            let (fx, fy) = (f.origin.x, ph - (f.origin.y + f.size.height));
            if x < fx || x >= fx + f.size.width || y < fy || y >= fy + f.size.height {
                return None;
            }
            let v = s.visibleFrame();
            Some((
                v.origin.x,
                ph - (v.origin.y + v.size.height),
                v.size.width,
                v.size.height,
            ))
        })
    }

    pub fn move_to(window: &Window, x: f64, y: f64) {
        let (Ok(w), Some(h)) = (ns_window(window), primary_height()) else {
            return;
        };
        w.setFrameTopLeftPoint(NSPoint::new(x, h - y));
    }

    pub fn set_frame(window: &Window, x: f64, y: f64, w: f64, h: f64) {
        if let Ok(win) = ns_window(window) {
            native_set_frame(&win, x, y, w, h);
        }
    }

    pub type Native = Retained<NSWindow>;

    pub fn native(window: &Window) -> Option<Native> {
        ns_window(window).ok()
    }

    pub fn native_set_frame(win: &Native, x: f64, y: f64, w: f64, h: f64) {
        use objc2_foundation::{NSRect, NSSize};
        let Some(ph) = primary_height() else { return };
        win.setFrame_display(
            NSRect::new(NSPoint::new(x, ph - y - h), NSSize::new(w, h)),
            true,
        );
    }

    pub fn frame(window: &Window) -> Option<(f64, f64, f64, f64)> {
        let w = ns_window(window).ok()?;
        let h = primary_height()?;
        let f = w.frame();
        Some((
            f.origin.x,
            h - (f.origin.y + f.size.height),
            f.size.width,
            f.size.height,
        ))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{Applied, Tweaks};
    use gpui::Window;

    /// Linux has no per-window "exclude from capture" (X11 and Wayland offer none): the app
    /// hides its own pop-ups around a capture instead (`capture_cloak`), so the request counts
    /// as handled by that fallback and is not logged as a failure.
    #[cfg(target_os = "linux")]
    pub fn apply(_window: &Window, _tweaks: Tweaks) -> Result<Applied, String> {
        Ok(Applied::default())
    }

    /// Windows would use `WDA_EXCLUDEFROMCAPTURE`; that path is not built yet.
    #[cfg(not(target_os = "linux"))]
    pub fn apply(_window: &Window, _tweaks: Tweaks) -> Result<Applied, String> {
        Err("hiding windows from capture is only built for macOS so far".to_owned())
    }

    /// The 128 px app icon for the X11 `_NET_WM_ICON` (Wayland takes the icon from the
    /// `.desktop` file that the app id names).
    #[cfg(target_os = "linux")]
    pub fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
        static ICON: &[u8] = include_bytes!("../assets/brand/app-icon-128.png");
        image::load_from_memory_with_format(ICON, image::ImageFormat::Png)
            .ok()
            .map(|i| std::sync::Arc::new(i.to_rgba8()))
    }

    #[cfg(not(target_os = "linux"))]
    pub fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
        None
    }

    #[cfg(target_os = "linux")]
    pub fn toasts_belong_in_window() -> bool {
        super::is_wayland_session(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
            std::env::var("DISPLAY").ok().as_deref(),
        )
    }

    #[cfg(not(target_os = "linux"))]
    pub fn toasts_belong_in_window() -> bool {
        false
    }

    pub fn needs_capture_cloak() -> bool {
        cfg!(target_os = "linux")
    }

    pub fn display_frame(_id: u32) -> Option<(f64, f64, f64, f64)> {
        None
    }

    #[derive(Clone)]
    pub struct Native;

    pub fn native(_window: &Window) -> Option<Native> {
        None
    }

    pub fn native_set_frame(_win: &Native, _x: f64, _y: f64, _w: f64, _h: f64) {}

    pub fn set_level(_window: &Window, _level: isize) {}

    pub fn set_ignores_mouse(_window: &Window, _ignore: bool) {}

    pub fn mouse_location() -> Option<(f64, f64)> {
        None
    }

    pub fn reduce_motion() -> bool {
        false
    }

    pub fn left_button_down() -> bool {
        false
    }

    pub fn bring_to_front(_window: &Window) {}

    pub fn capture_window_id(_window: &Window) -> Option<u32> {
        None
    }

    pub fn visible_frame_at(_x: f64, _y: f64) -> Option<(f64, f64, f64, f64)> {
        None
    }

    pub fn move_to(_window: &Window, _x: f64, _y: f64) {}

    pub fn set_frame(_window: &Window, _x: f64, _y: f64, _w: f64, _h: f64) {}

    pub fn frame(_window: &Window) -> Option<(f64, f64, f64, f64)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{centered_frame, is_wayland_session};

    #[test]
    fn the_session_type_decides_wayland() {
        assert!(is_wayland_session(Some("wayland"), Some("wayland-0"), None));
        assert!(is_wayland_session(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0")
        ));
        assert!(!is_wayland_session(Some("wayland"), None, None));
        assert!(!is_wayland_session(
            Some("x11"),
            Some("wayland-0"),
            Some(":0")
        ));
        // No session type: a Wayland display without an X11 one.
        assert!(is_wayland_session(None, Some("wayland-0"), None));
        assert!(!is_wayland_session(None, Some("wayland-0"), Some(":0")));
        assert!(!is_wayland_session(None, None, Some(":0")));
        assert!(!is_wayland_session(None, Some(""), None));
    }

    #[test]
    fn centers_in_the_visible_frame() {
        let v = (0.0, 25.0, 1440.0, 800.0);
        assert_eq!(
            centered_frame(v, 640.0, 400.0),
            (400.0, 225.0, 640.0, 400.0)
        );
    }

    #[test]
    fn follows_a_display_with_a_negative_origin() {
        let v = (-1920.0, -200.0, 1920.0, 1080.0);
        assert_eq!(
            centered_frame(v, 640.0, 720.0),
            (-1280.0, -20.0, 640.0, 720.0)
        );
    }

    #[test]
    fn clamps_to_a_short_display() {
        let v = (0.0, 25.0, 1280.0, 600.0);
        assert_eq!(centered_frame(v, 640.0, 720.0), (320.0, 25.0, 640.0, 600.0));
        assert_eq!(
            centered_frame(v, 2000.0, 100.0),
            (0.0, 275.0, 1280.0, 100.0)
        );
    }
}
