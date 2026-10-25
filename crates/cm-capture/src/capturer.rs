use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use cm_core::{CmError, PhysicalPoint, PhysicalRect, ScaleFactor};

use crate::{
    region::resolve_region, CaptureBackend, CaptureKind, CaptureTarget, DisplayInfo, Frame,
    Targets, WindowInfo,
};

/// Longest accepted delay (CAP-5: `delay_s: 0..30`).
pub const MAX_DELAY_SECS: f64 = 30.0;

/// Slice used to poll the cancel token while waiting.
const POLL: Duration = Duration::from_millis(10);

/// Validates `delay_s` (CAP-5: 0..=30, finite) and converts it to a [`Duration`].
pub fn delay_from_secs(secs: f64) -> Result<Duration, CmError> {
    if secs.is_finite() && (0.0..=MAX_DELAY_SECS).contains(&secs) {
        Ok(Duration::from_secs_f64(secs))
    } else {
        Err(CmError::invalid_field(
            "delay_s",
            format!("must be between 0 and {MAX_DELAY_SECS}"),
        ))
    }
}

/// Cooperative cancel flag for a pending delayed capture (CAP-5.AC2: Esc). Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A capture to take: what, and after how long.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureRequest {
    pub target: CaptureTarget,
    pub delay: Duration,
}

impl CaptureRequest {
    pub fn new(target: CaptureTarget) -> Self {
        CaptureRequest {
            target,
            delay: Duration::ZERO,
        }
    }

    /// Sets the delay from the MCP/CLI `delay_s` value; see [`delay_from_secs`].
    pub fn with_delay_secs(mut self, secs: f64) -> Result<Self, CmError> {
        self.delay = delay_from_secs(secs)?;
        Ok(self)
    }
}

/// A finished capture at physical size.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub frame: Frame,
    pub kind: CaptureKind,
    pub display_id: Option<u32>,
    pub window_id: Option<u32>,
    pub scale: ScaleFactor,
    /// Where the pixels came from, in physical px: display-relative for `Display`/`Region`
    /// (a display capture is `(0, 0, w, h)`), global desktop bounds for `Window`.
    pub source: PhysicalRect,
    /// Time actually spent waiting before the frame was taken (CAP-5.AC1).
    pub waited: Duration,
}

/// Entry point of the screenshot API; wraps a [`CaptureBackend`].
pub struct Capturer {
    backend: Box<dyn CaptureBackend>,
    own_pid: Option<u32>,
    hide_shell_windows: bool,
    exclude_own_windows: bool,
    kept_windows: Option<crate::record::WindowProvider>,
}

/// Owners of macOS desktop-shell windows: the Window Server's "Menubar", the Dock, Control
/// Center, `SystemUIServer` (menu extras) and Notification Center banners. They are never
/// capture targets (CAP-3).
pub const SHELL_APPS: &[&str] = &[
    "Window Server",
    "Dock",
    "Control Center",
    "SystemUIServer",
    "Notification Center",
];

/// True for a desktop-shell window (menu bar, Dock, Control Center, ...), recognized by its
/// owning app. The macOS backend additionally drops everything above window layer 0.
pub fn is_shell_window(w: &WindowInfo) -> bool {
    SHELL_APPS.contains(&w.app.as_str())
}

impl Capturer {
    /// By default CaptureMann's own windows (this process) are left out of `list_targets`.
    pub fn new(backend: impl CaptureBackend + 'static) -> Self {
        Capturer {
            backend: Box::new(backend),
            own_pid: Some(std::process::id()),
            hide_shell_windows: cfg!(target_os = "macos"),
            exclude_own_windows: true,
            kept_windows: None,
        }
    }

    /// Whether display and region screenshots leave this process's own windows (`own_pid`)
    /// out when the backend can filter by process (macOS 14+ ScreenCaptureKit; R4-6). On by
    /// default; it only has an effect while `own_pid` is `Some`. Window captures never contain
    /// other windows, and a backend that cannot filter falls back to the plain frame.
    pub fn with_exclude_own_windows(mut self, exclude: bool) -> Self {
        self.exclude_own_windows = exclude;
        self
    }

    /// Windows of the own process that stay in display and region screenshots although the
    /// process is excluded: the library/editor window (REC-6, CAP-1). The provider is asked on
    /// every capture; recorder UI such as the toast and confirmation dialogs is never in it.
    /// Without a provider every own window is left out.
    pub fn with_kept_windows(mut self, provider: crate::record::WindowProvider) -> Self {
        self.kept_windows = Some(provider);
        self
    }

    /// A display frame, without the own process's windows when enabled and supported.
    fn display_frame(&self, d: &DisplayInfo) -> Result<Frame, CmError> {
        if let (true, Some(pid)) = (self.exclude_own_windows, self.own_pid) {
            let keep = self
                .kept_windows
                .as_ref()
                .map(crate::record::WindowProvider::ids)
                .unwrap_or_default();
            // A failure of the filtered path must not fail a screenshot the plain path can take.
            let filtered = if keep.is_empty() {
                self.backend
                    .capture_display_excluding(d.id, d.bounds.size, pid)
            } else {
                self.backend
                    .capture_display_excluding_except(d.id, d.bounds.size, pid, &keep)
            };
            if let Ok(Some(f)) = filtered {
                if f.size() == d.bounds.size {
                    return Ok(f);
                }
            }
        }
        self.backend.capture_display(d.id)
    }

    /// Whether `list_targets` leaves out desktop-shell windows ([`is_shell_window`]). On by
    /// default on macOS, where the Window Server reports the menu bar and the Dock as
    /// windows; off elsewhere (the Windows backend already skips tool and cloaked windows).
    pub fn with_shell_windows_hidden(mut self, hidden: bool) -> Self {
        self.hide_shell_windows = hidden;
        self
    }

    /// Capturer on the real OS backend.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn system() -> Self {
        Self::new(crate::XcapBackend::new())
    }

    /// Overrides which process counts as "CaptureMann itself"; `None` keeps every window.
    pub fn with_own_pid(mut self, pid: Option<u32>) -> Self {
        self.own_pid = pid;
        self
    }

    /// Displays and windows (`list_targets`). Fails with `permission_missing(screen)` when the
    /// OS denies screen capture. Windows of this process and zero-area windows are omitted.
    pub fn list_targets(&self) -> Result<Targets, CmError> {
        self.list_targets_with(true)
    }

    /// Displays only, with an empty window list: the fast path for a client that needs display
    /// ids and geometry right away (the overlay freezes them before it asks for windows).
    pub fn list_displays(&self) -> Result<Targets, CmError> {
        self.list_targets_with(false)
    }

    /// [`Capturer::list_targets`] with the window enumeration optional (`windows: false`
    /// leaves `Targets::windows` empty and skips the Window Server entirely).
    pub fn list_targets_with(&self, windows: bool) -> Result<Targets, CmError> {
        self.backend.check_permission()?;
        let displays = self.backend.displays()?;
        if !windows {
            return Ok(Targets {
                displays,
                windows: Vec::new(),
            });
        }
        let windows = self
            .backend
            .windows()?
            .into_iter()
            .filter(|w| {
                w.bounds.size.width > 0
                    && w.bounds.size.height > 0
                    && self.own_pid.is_none_or(|p| p != w.pid)
                    && !(self.hide_shell_windows && is_shell_window(w))
            })
            .collect();
        Ok(Targets { displays, windows })
    }

    /// First window (front to back) whose title contains `needle`, case-insensitively
    /// (CLI `--window "<title substring>"`). `not_found` if none.
    pub fn find_window(&self, needle: &str) -> Result<WindowInfo, CmError> {
        let needle_lc = needle.to_lowercase();
        self.list_targets()?
            .windows
            .into_iter()
            .find(|w| w.title.to_lowercase().contains(&needle_lc))
            .ok_or_else(|| CmError::not_found(format!("No window title contains \"{needle}\"")))
    }

    /// Captures `request.target` after `request.delay`. `Ok(None)` means `cancel` fired during
    /// the delay and nothing was captured (CAP-5.AC2).
    ///
    /// The target is validated before waiting (fast failure) and resolved again after, so a
    /// region or window reflects the screen at capture time (CAP-5.AC3).
    pub fn capture(
        &self,
        request: CaptureRequest,
        cancel: Option<&CancelToken>,
    ) -> Result<Option<Capture>, CmError> {
        if request.delay > Duration::from_secs_f64(MAX_DELAY_SECS) {
            return Err(CmError::invalid_field(
                "delay_s",
                format!("must be between 0 and {MAX_DELAY_SECS}"),
            ));
        }
        self.backend.check_permission()?;
        self.resolve(request.target)?;

        let start = Instant::now();
        let deadline = start + request.delay;
        loop {
            if cancel.is_some_and(CancelToken::is_cancelled) {
                return Ok(None);
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            std::thread::sleep((deadline - now).min(POLL));
        }
        let waited = start.elapsed();

        let resolved = self.resolve(request.target)?;
        let mut cap = match resolved {
            Resolved::Display(d) => {
                let frame = self.display_frame(&d)?;
                Capture {
                    source: PhysicalRect {
                        origin: PhysicalPoint { x: 0, y: 0 },
                        size: frame.size(),
                    },
                    frame,
                    kind: CaptureKind::Display,
                    display_id: Some(d.id),
                    window_id: None,
                    scale: d.scale,
                    waited,
                }
            }
            Resolved::Window(w) => {
                let frame = self.backend.capture_window(w.id)?;
                Capture {
                    frame,
                    kind: CaptureKind::Window,
                    display_id: w.display_id,
                    window_id: Some(w.id),
                    scale: w.scale,
                    source: w.bounds,
                    waited,
                }
            }
            Resolved::Region(d, rect) => {
                let full = self.display_frame(&d)?;
                Capture {
                    frame: full.crop(rect)?,
                    kind: CaptureKind::Region,
                    display_id: Some(d.id),
                    window_id: None,
                    scale: d.scale,
                    source: rect,
                    waited,
                }
            }
        };
        cap.waited = waited;
        Ok(Some(cap))
    }

    fn resolve(&self, target: CaptureTarget) -> Result<Resolved, CmError> {
        match target {
            CaptureTarget::Display { display_id } => {
                Ok(Resolved::Display(self.display(display_id)?))
            }
            CaptureTarget::Region { display_id, rect } => {
                let d = self.display(display_id)?;
                let r = resolve_region(&d, rect)?;
                Ok(Resolved::Region(d, r))
            }
            CaptureTarget::Window { window_id } => {
                let w = self
                    .backend
                    .windows()?
                    .into_iter()
                    .find(|w| w.id == window_id)
                    .ok_or_else(|| CmError::not_found(format!("Window {window_id} not found")))?;
                if w.minimized {
                    return Err(CmError::invalid_input("Window is minimized"));
                }
                if w.bounds.size.width == 0 || w.bounds.size.height == 0 {
                    return Err(CmError::invalid_input("Window has no visible area"));
                }
                Ok(Resolved::Window(w))
            }
        }
    }

    fn display(&self, id: u32) -> Result<DisplayInfo, CmError> {
        self.backend
            .displays()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| CmError::not_found(format!("Display {id} not found")))
    }
}

/// Crops a region (or, with `None`, the whole image) out of an already captured display
/// frame, without touching the screen again. This is what makes a region capture from the
/// picker's frozen frame pixel-identical to what the user saw (CAP-1.AC2).
///
/// `rect` is physical px relative to the display, like [`CaptureTarget::Region`]; it must lie
/// inside the frame (`invalid_input` otherwise).
pub fn crop_display_frame(
    frame: &Frame,
    display_id: u32,
    scale: ScaleFactor,
    rect: Option<PhysicalRect>,
) -> Result<Capture, CmError> {
    let (out, kind, source) = match rect {
        Some(r) => (frame.crop(r)?, CaptureKind::Region, r),
        None => (
            frame.clone(),
            CaptureKind::Display,
            PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 0 },
                size: frame.size(),
            },
        ),
    };
    Ok(Capture {
        frame: out,
        kind,
        display_id: Some(display_id),
        window_id: None,
        scale,
        source,
        waited: Duration::ZERO,
    })
}

enum Resolved {
    Display(DisplayInfo),
    Window(WindowInfo),
    Region(DisplayInfo, PhysicalRect),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{pattern_pixel, FakeBackend};
    use crate::RegionRect;
    use cm_core::{ErrorCode, PhysicalSize, Rect};

    const OWN_PID: u32 = 4242;

    /// 3024x1964 physical at 2x (CAP-1.AC1 display) plus a 1x external display.
    fn desktop() -> FakeBackend {
        FakeBackend::new()
            .with_display(1, 0, 0, 3024, 1964, 2.0)
            .with_display(2, 3024, 0, 1920, 1080, 1.0)
            .with_window(
                10,
                "Editor - main.rs",
                "Code",
                100,
                1,
                (200, 100, 800, 600),
                false,
            )
            .with_window(
                11,
                "CaptureMann",
                "CaptureMann",
                OWN_PID,
                1,
                (0, 0, 300, 300),
                false,
            )
            .with_window(12, "Hidden", "Code", 100, 1, (0, 0, 300, 300), true)
    }

    fn capturer() -> Capturer {
        Capturer::new(desktop()).with_own_pid(Some(OWN_PID))
    }

    fn phys(x: i32, y: i32, w: u32, h: u32) -> RegionRect {
        RegionRect::Physical(PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        })
    }

    fn take(c: &Capturer, t: CaptureTarget) -> Capture {
        c.capture(CaptureRequest::new(t), None).unwrap().unwrap()
    }

    /// CAP-3: the menu bar, Dock, Control Center and friends are not window targets.
    #[test]
    fn cap3_list_targets_hides_desktop_shell_windows() {
        let backend = desktop()
            .with_window(
                20,
                "Menubar",
                "Window Server",
                1,
                1,
                (0, 0, 3024, 50),
                false,
            )
            .with_window(21, "", "Dock", 2, 1, (0, 1900, 3024, 64), false)
            .with_window(22, "", "Control Center", 3, 1, (2800, 0, 200, 40), false)
            .with_window(
                23,
                "Notification",
                "Notification Center",
                4,
                1,
                (2000, 60, 400, 100),
                false,
            )
            .with_window(
                24,
                "Dock settings",
                "System Settings",
                5,
                1,
                (10, 10, 500, 500),
                false,
            );
        let c = Capturer::new(backend)
            .with_own_pid(Some(OWN_PID))
            .with_shell_windows_hidden(true);
        let ids: Vec<u32> = c
            .list_targets()
            .unwrap()
            .windows
            .iter()
            .map(|w| w.id)
            .collect();
        // Real windows stay (including one whose *title* mentions the Dock).
        assert_eq!(ids, vec![10, 12, 24]);
        let c = Capturer::new(desktop().with_window(
            20,
            "Menubar",
            "Window Server",
            1,
            1,
            (0, 0, 9, 9),
            false,
        ))
        .with_own_pid(Some(OWN_PID))
        .with_shell_windows_hidden(false);
        assert!(c.list_targets().unwrap().windows.iter().any(|w| w.id == 20));
    }

    struct Shared(std::sync::Arc<FakeBackend>);
    impl CaptureBackend for Shared {
        fn check_permission(&self) -> Result<(), CmError> {
            self.0.check_permission()
        }
        fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
            self.0.displays()
        }
        fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
            self.0.windows()
        }
        fn capture_display(&self, id: u32) -> Result<Frame, CmError> {
            self.0.capture_display(id)
        }
        fn capture_window(&self, id: u32) -> Result<Frame, CmError> {
            self.0.capture_window(id)
        }
        fn capture_display_excluding(
            &self,
            id: u32,
            size: PhysicalSize,
            pid: u32,
        ) -> Result<Option<Frame>, CmError> {
            self.0.capture_display_excluding(id, size, pid)
        }
        fn capture_display_excluding_except(
            &self,
            id: u32,
            size: PhysicalSize,
            pid: u32,
            keep: &[u32],
        ) -> Result<Option<Frame>, CmError> {
            self.0.capture_display_excluding_except(id, size, pid, keep)
        }
    }

    /// The library window id from the provider reaches the backend with the own-process
    /// exclusion; the toast is not in it. An empty provider answer is the plain exclusion.
    #[test]
    fn screenshots_keep_the_provided_library_window_only() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let studio = std::sync::Arc::new(AtomicU32::new(100));
        let s2 = studio.clone();
        let backend = std::sync::Arc::new(desktop().with_process_exclusion());
        let c = Capturer::new(Shared(backend.clone()))
            .with_own_pid(Some(OWN_PID))
            .with_kept_windows(crate::WindowProvider::new(move || {
                match s2.load(Ordering::SeqCst) {
                    0 => vec![],
                    id => vec![id],
                }
            }));
        take(&c, CaptureTarget::Display { display_id: 1 });
        studio.store(0, Ordering::SeqCst);
        take(&c, CaptureTarget::Display { display_id: 1 });
        assert_eq!(
            backend.calls(),
            vec![
                format!("capture_display_excluding:1:{OWN_PID}:keep=100"),
                format!("capture_display_excluding:1:{OWN_PID}"),
            ]
        );
    }

    /// R4-6: display and region screenshots go through the own-process exclusion when the
    /// backend supports it; window captures and opted-out capturers use the plain path.
    #[test]
    fn r4_6_display_and_region_captures_exclude_own_windows() {
        let backend = std::sync::Arc::new(desktop().with_process_exclusion());
        let c = Capturer::new(Shared(backend.clone())).with_own_pid(Some(OWN_PID));
        take(&c, CaptureTarget::Display { display_id: 1 });
        take(
            &c,
            CaptureTarget::Region {
                display_id: 1,
                rect: phys(10, 10, 50, 50),
            },
        );
        take(&c, CaptureTarget::Window { window_id: 10 });
        assert_eq!(
            backend.calls(),
            vec![
                format!("capture_display_excluding:1:{OWN_PID}"),
                format!("capture_display_excluding:1:{OWN_PID}"),
                "capture_window:10".to_owned()
            ]
        );
        // Opt out, or no own pid: the plain xcap-style path.
        for c in [
            Capturer::new(Shared(backend.clone()))
                .with_own_pid(Some(OWN_PID))
                .with_exclude_own_windows(false),
            Capturer::new(Shared(backend.clone())).with_own_pid(None),
        ] {
            take(&c, CaptureTarget::Display { display_id: 1 });
        }
        let calls = backend.calls();
        assert_eq!(&calls[3..], ["capture_display:1", "capture_display:1"]);
    }

    /// R4-6: a backend without process filtering (macOS 13, Windows) falls back silently.
    #[test]
    fn r4_6_unsupported_exclusion_falls_back_to_the_plain_frame() {
        let backend = std::sync::Arc::new(desktop());
        let c = Capturer::new(Shared(backend.clone())).with_own_pid(Some(OWN_PID));
        let cap = take(&c, CaptureTarget::Display { display_id: 1 });
        assert_eq!(cap.frame.pixel(5, 5), Some(pattern_pixel(5, 5)));
        assert_eq!(backend.calls(), vec!["capture_display:1"]);
    }

    /// CAP-1.AC2: cropping a cached frame never touches the screen again and equals the
    /// golden pattern pixel for pixel.
    #[test]
    fn cap1_ac2_crop_of_a_frozen_frame_is_exact_and_does_not_recapture() {
        let backend = std::sync::Arc::new(desktop());
        let c = Capturer::new(Shared(backend.clone())).with_own_pid(None);
        let frozen = c
            .capture(
                CaptureRequest::new(CaptureTarget::Display { display_id: 1 }),
                None,
            )
            .unwrap()
            .unwrap();
        let rect = PhysicalRect {
            origin: PhysicalPoint { x: 2207, y: 669 },
            size: PhysicalSize {
                width: 420,
                height: 649,
            },
        };
        let cap = crop_display_frame(&frozen.frame, 1, frozen.scale, Some(rect)).unwrap();
        assert_eq!(
            backend.calls(),
            vec!["capture_display:1"],
            "no second capture"
        );
        assert_eq!((cap.frame.width(), cap.frame.height()), (420, 649));
        assert_eq!(cap.kind, CaptureKind::Region);
        assert_eq!(cap.source, rect);
        assert_eq!(cap.frame.pixel(0, 0), Some(pattern_pixel(2207, 669)));
        assert_eq!(cap.frame.pixel(419, 648), Some(pattern_pixel(2626, 1317)));
        let whole = crop_display_frame(&frozen.frame, 1, frozen.scale, None).unwrap();
        assert_eq!(whole.kind, CaptureKind::Display);
        assert_eq!(whole.frame, frozen.frame);
        let outside = PhysicalRect {
            origin: PhysicalPoint { x: 3000, y: 0 },
            size: PhysicalSize {
                width: 100,
                height: 10,
            },
        };
        assert!(crop_display_frame(&frozen.frame, 1, frozen.scale, Some(outside)).is_err());
    }

    #[test]
    fn cap1_ac1_display_is_physical_size() {
        let cap = take(&capturer(), CaptureTarget::Display { display_id: 1 });
        assert_eq!((cap.frame.width(), cap.frame.height()), (3024, 1964));
        assert_eq!(cap.kind, CaptureKind::Display);
        assert_eq!(cap.scale.get(), 2.0);
    }

    #[test]
    fn cap1_ac2_region_is_pixel_identical_to_frozen_frame() {
        let c = capturer();
        let full = take(&c, CaptureTarget::Display { display_id: 1 });
        let cap = take(
            &c,
            CaptureTarget::Region {
                display_id: 1,
                rect: phys(2207, 669, 420, 649),
            },
        );
        assert_eq!((cap.frame.width(), cap.frame.height()), (420, 649));
        for (x, y) in [(0, 0), (419, 0), (0, 648), (419, 648), (200, 300)] {
            let want = full.frame.pixel(2207 + x, 669 + y).unwrap();
            assert_eq!(cap.frame.pixel(x, y).unwrap(), want);
            assert_eq!(want, pattern_pixel(2207 + x, 669 + y));
        }
        assert_eq!(cap.frame, full.frame.crop(cap.source).unwrap());
    }

    #[test]
    fn cap1_ac2_logical_rect_matches_physical_on_2x() {
        let c = capturer();
        let a = take(
            &c,
            CaptureTarget::Region {
                display_id: 1,
                rect: RegionRect::Logical(Rect::new(1103.5, 334.5, 210.0, 324.5)),
            },
        );
        let b = take(
            &c,
            CaptureTarget::Region {
                display_id: 1,
                rect: phys(2207, 669, 420, 649),
            },
        );
        assert_eq!(a.frame, b.frame);
    }

    #[test]
    fn cap1_region_on_1x_display_is_unscaled() {
        let cap = take(
            &capturer(),
            CaptureTarget::Region {
                display_id: 2,
                rect: RegionRect::Logical(Rect::new(10.0, 20.0, 30.0, 40.0)),
            },
        );
        assert_eq!((cap.frame.width(), cap.frame.height()), (30, 40));
        assert_eq!(cap.frame.pixel(0, 0), Some(pattern_pixel(10, 20)));
    }

    #[test]
    fn cap1_region_outside_display_is_invalid_input() {
        let err = capturer()
            .capture(
                CaptureRequest::new(CaptureTarget::Region {
                    display_id: 2,
                    rect: phys(1900, 0, 100, 10),
                }),
                None,
            )
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn cap1_ac4_window_is_1to1_with_its_bounds() {
        let cap = take(&capturer(), CaptureTarget::Window { window_id: 10 });
        assert_eq!((cap.frame.width(), cap.frame.height()), (800, 600));
        assert_eq!(cap.frame.pixel(0, 0), Some(pattern_pixel(200, 100)));
        assert_eq!(cap.frame.pixel(799, 599), Some(pattern_pixel(999, 699)));
        assert_eq!(cap.window_id, Some(10));
    }

    #[test]
    fn cap1_minimized_window_is_invalid_input() {
        let err = capturer()
            .capture(
                CaptureRequest::new(CaptureTarget::Window { window_id: 12 }),
                None,
            )
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert_eq!(err.to_string(), "Window is minimized");
    }

    #[test]
    fn cap1_unknown_targets_are_not_found() {
        let c = capturer();
        for t in [
            CaptureTarget::Display { display_id: 99 },
            CaptureTarget::Window { window_id: 99 },
            CaptureTarget::Region {
                display_id: 99,
                rect: phys(0, 0, 1, 1),
            },
        ] {
            let err = c.capture(CaptureRequest::new(t), None).unwrap_err();
            assert_eq!(err.code(), ErrorCode::NotFound);
        }
    }

    #[test]
    fn cap1_list_targets_shape_and_excludes_own_windows() {
        let t = capturer().list_targets().unwrap();
        assert_eq!(t.displays.len(), 2);
        let ids: Vec<u32> = t.windows.iter().map(|w| w.id).collect();
        assert_eq!(ids, vec![10, 12]); // 11 belongs to CaptureMann itself
        let json = serde_json::to_value(&t).unwrap();
        let w = &json["windows"][0];
        for k in ["id", "title", "app", "bounds", "scale"] {
            assert!(w.get(k).is_some(), "missing {k}");
        }
        assert_eq!(json["displays"][0]["scale"], 2.0);
        let all = Capturer::new(desktop())
            .with_own_pid(None)
            .list_targets()
            .unwrap();
        assert_eq!(all.windows.len(), 3);
    }

    /// CAP-1.AC1: the overlay needs display ids and geometry before it knows any window, so
    /// the displays-only listing returns the same displays and no windows (and still checks
    /// the Screen Recording permission).
    #[test]
    fn cap1_ac1_list_displays_skips_windows() {
        let c = capturer();
        let full = c.list_targets().unwrap();
        let fast = c.list_displays().unwrap();
        assert_eq!(fast.displays, full.displays);
        assert!(fast.windows.is_empty() && !full.windows.is_empty());
        assert_eq!(c.list_targets_with(true).unwrap(), full);
        let denied = Capturer::new(desktop().without_permission());
        assert_eq!(
            denied.list_displays().unwrap_err().code(),
            ErrorCode::PermissionMissing
        );
    }

    #[test]
    fn cap1_find_window_by_title_substring() {
        let c = capturer();
        assert_eq!(c.find_window("main.RS").unwrap().id, 10);
        assert_eq!(
            c.find_window("nope").unwrap_err().code(),
            ErrorCode::NotFound
        );
    }

    #[test]
    fn cap1_missing_screen_permission_is_permission_missing() {
        let c = Capturer::new(desktop().without_permission());
        let e1 = c.list_targets().unwrap_err();
        let e2 = c
            .capture(
                CaptureRequest::new(CaptureTarget::Display { display_id: 1 }),
                None,
            )
            .unwrap_err();
        for e in [e1, e2] {
            assert_eq!(e.code(), ErrorCode::PermissionMissing);
            assert_eq!(e.data().unwrap()["permission"], serde_json::json!("screen"));
        }
    }

    #[test]
    fn cap5_delay_validation() {
        assert_eq!(delay_from_secs(5.0).unwrap(), Duration::from_secs(5));
        assert!(delay_from_secs(0.0).is_ok());
        assert!(delay_from_secs(30.0).is_ok());
        for bad in [-0.1, 30.1, f64::NAN, f64::INFINITY] {
            assert_eq!(
                delay_from_secs(bad).unwrap_err().code(),
                ErrorCode::InvalidInput
            );
        }
        let req = CaptureRequest {
            target: CaptureTarget::Display { display_id: 1 },
            delay: Duration::from_secs(31),
        };
        assert!(capturer().capture(req, None).is_err());
    }

    #[test]
    fn cap5_ac1_capture_happens_after_the_delay() {
        let c = capturer();
        let req = CaptureRequest::new(CaptureTarget::Display { display_id: 2 })
            .with_delay_secs(0.3)
            .unwrap();
        let t = Instant::now();
        let cap = c.capture(req, None).unwrap().unwrap();
        let e = t.elapsed();
        assert!(e >= Duration::from_millis(300), "{e:?}");
        // The lower bound proves the delay happens; the upper one only that it is not a longer wait
        // (a loaded CI runner took 426 ms once, so it is generous).
        assert!(e < Duration::from_millis(2000), "{e:?}");
        assert!(cap.waited >= Duration::from_millis(300) && cap.waited <= e);
    }

    #[test]
    fn cap5_ac2_cancel_during_delay_captures_nothing() {
        let c = Capturer::new(desktop());
        let token = CancelToken::new();
        let t2 = token.clone();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            t2.cancel();
        });
        let req = CaptureRequest::new(CaptureTarget::Display { display_id: 1 })
            .with_delay_secs(10.0)
            .unwrap();
        let t = Instant::now();
        let out = c.capture(req, Some(&token)).unwrap();
        h.join().unwrap();
        assert!(out.is_none());
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn cap5_ac3_screen_is_only_touched_after_the_delay() {
        struct Shared(std::sync::Arc<FakeBackend>);
        impl CaptureBackend for Shared {
            fn check_permission(&self) -> Result<(), CmError> {
                self.0.check_permission()
            }
            fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
                self.0.displays()
            }
            fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
                self.0.windows()
            }
            fn capture_display(&self, id: u32) -> Result<Frame, CmError> {
                self.0.capture_display(id)
            }
            fn capture_window(&self, id: u32) -> Result<Frame, CmError> {
                self.0.capture_window(id)
            }
        }
        let backend = std::sync::Arc::new(desktop());
        let c = Capturer::new(Shared(backend.clone()));
        let cancelled = CancelToken::new();
        cancelled.cancel();
        let req = CaptureRequest::new(CaptureTarget::Display { display_id: 2 })
            .with_delay_secs(0.1)
            .unwrap();
        assert!(c.capture(req, Some(&cancelled)).unwrap().is_none());
        assert!(backend.calls().is_empty());
        let _ = c.capture(req, None).unwrap();
        assert_eq!(backend.calls(), vec!["capture_display:2"]);
    }
}
