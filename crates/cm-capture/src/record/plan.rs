//! Turns a target and the user's settings into a concrete capture plan (REC-1, REC-2).

use cm_core::{CmError, PhysicalRect, PhysicalSize, ScaleFactor};
use cm_encode::{plan_video, VideoPlan, VideoSettings};
use serde::{Deserialize, Serialize};

use crate::{resolve_region, CaptureTarget, Targets};

/// A shareable source of window ids, queried when a plan is made and again by the capture
/// backend while recording (the kept-window set of [`Exclusions`]). Compares by `Arc` pointer.
#[derive(Clone)]
pub struct WindowProvider(pub std::sync::Arc<dyn Fn() -> Vec<u32> + Send + Sync>);

impl WindowProvider {
    pub fn new(f: impl Fn() -> Vec<u32> + Send + Sync + 'static) -> Self {
        WindowProvider(std::sync::Arc::new(f))
    }

    pub fn from_arc(f: std::sync::Arc<dyn Fn() -> Vec<u32> + Send + Sync>) -> Self {
        WindowProvider(f)
    }

    /// The window ids right now.
    pub fn ids(&self) -> Vec<u32> {
        (self.0)()
    }
}

impl std::fmt::Debug for WindowProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WindowProvider(..)")
    }
}

impl PartialEq for WindowProvider {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for WindowProvider {}

/// Windows and apps kept out of the screen track (REC-6).
///
/// CaptureMann's recorder windows are always excluded: pass the app's pid (every window of that
/// process, including ones opened later) plus the explicit ids of the camera bubble, HUD, scrim
/// and countdown windows (those also have `sharingType = none` set by their owners, see S6).
/// The library/editor window is the exception: its id goes in the keep set so it is captured
/// although its process is excluded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exclusions {
    pub window_ids: Vec<u32>,
    /// Every window of these processes is excluded, including windows opened later.
    pub pids: Vec<u32>,
    /// Same, by bundle id (the REC-6 default list: password managers).
    pub bundle_ids: Vec<String>,
    /// Windows of an excluded application that are captured anyway (the library window).
    /// Ignored when no listed window with such an id belongs to an excluded application, so an
    /// id that is not (yet) listed stays excluded: the fail-safe direction.
    #[serde(default)]
    pub keep_window_ids: Vec<u32>,
    /// Live source of more keep ids, queried by the backend while recording (a window opened
    /// mid-recording is picked up within one watch interval). Not serialized.
    #[serde(skip)]
    pub keep_provider: Option<WindowProvider>,
}

impl Exclusions {
    pub fn is_empty(&self) -> bool {
        self.window_ids.is_empty() && self.pids.is_empty() && self.bundle_ids.is_empty()
    }

    /// The keep ids now: the fixed ones plus the provider's answer, sorted and unique.
    pub fn current_keep_ids(&self) -> Vec<u32> {
        let mut ids = self.keep_window_ids.clone();
        if let Some(p) = &self.keep_provider {
            ids.extend(p.ids());
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// What the caller asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenSettings {
    pub target: CaptureTarget,
    pub video: VideoSettings,
    /// Draw the cursor into the video. On by default: REC-7 bakes it into the screen track in
    /// v0.1; the "record cursor separately" option (P1) turns this off.
    pub show_cursor: bool,
    pub exclusions: Exclusions,
}

impl ScreenSettings {
    pub fn new(target: CaptureTarget) -> Self {
        ScreenSettings {
            target,
            video: VideoSettings::default(),
            show_cursor: true,
            exclusions: Exclusions::default(),
        }
    }
}

/// A resolved capture: what to grab, at what physical size, and how to encode it.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturePlan {
    pub target: CaptureTarget,
    /// The display being captured (for a window: the display holding it, if known).
    pub display_id: Option<u32>,
    /// Backing scale of that display or window.
    pub scale: ScaleFactor,
    /// Physical px of the captured area before output scaling.
    pub source_size: PhysicalSize,
    /// Region crop in physical px relative to the display's top-left (same convention as
    /// screenshots); `None` for display and window targets.
    pub crop: Option<PhysicalRect>,
    pub video: VideoPlan,
    pub show_cursor: bool,
    pub exclusions: Exclusions,
}

/// Resolves the target against the current displays and windows (REC-1, REC-2).
pub fn plan_capture(settings: &ScreenSettings, targets: &Targets) -> Result<CapturePlan, CmError> {
    let (display_id, scale, source_size, crop) = match settings.target {
        CaptureTarget::Display { display_id } => {
            let d = targets
                .displays
                .iter()
                .find(|d| d.id == display_id)
                .ok_or_else(|| CmError::not_found(format!("Display {display_id} not found")))?;
            (Some(d.id), d.scale, d.bounds.size, None)
        }
        CaptureTarget::Region { display_id, rect } => {
            let d = targets
                .displays
                .iter()
                .find(|d| d.id == display_id)
                .ok_or_else(|| CmError::not_found(format!("Display {display_id} not found")))?;
            let crop = resolve_region(d, rect)?;
            (Some(d.id), d.scale, crop.size, Some(crop))
        }
        CaptureTarget::Window { window_id } => {
            let w = targets
                .windows
                .iter()
                .find(|w| w.id == window_id)
                .ok_or_else(|| CmError::not_found(format!("Window {window_id} not found")))?;
            if w.minimized {
                return Err(CmError::invalid_input(
                    "The window is minimized; show it before recording",
                ));
            }
            (w.display_id, w.scale, w.bounds.size, None)
        }
    };
    let video = plan_video(source_size.width, source_size.height, &settings.video)?;
    Ok(CapturePlan {
        target: settings.target,
        display_id,
        scale,
        source_size,
        crop,
        video,
        show_cursor: settings.show_cursor,
        exclusions: settings.exclusions.clone(),
    })
}

impl CapturePlan {
    /// Re-plans the video for a source whose real size differs from the one planned for (the
    /// Linux portal picks the monitor itself). Keeps what the user asked for: a plan that was
    /// scaled down to a height keeps that height (width follows the aspect ratio), a native plan
    /// stays native. Frame rate, codec and quality are unchanged; the bit rate follows the size.
    pub fn adopt_source_size(&mut self, size: PhysicalSize) {
        if size == self.source_size || size.width < 2 || size.height < 2 {
            return;
        }
        let was_scaled = self.video.height < self.source_size.height & !1;
        let (sw, sh) = (size.width & !1, size.height & !1);
        let (w, h) = if was_scaled && self.video.height < sh {
            let h = self.video.height;
            let w =
                ((f64::from(sw) * f64::from(h) / f64::from(sh) / 2.0).round() as u32 * 2).max(2);
            (w, h)
        } else {
            (sw, sh)
        };
        self.source_size = size;
        self.video.width = w;
        self.video.height = h;
        self.video.bitrate_bps =
            cm_encode::bitrate_bps(self.video.codec, self.video.fps, self.video.quality, w, h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayInfo, RegionRect, WindowInfo};
    use cm_core::{PhysicalPoint, Rect};
    use cm_encode::{Codec, OutputSize};

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn targets() -> Targets {
        Targets {
            displays: vec![DisplayInfo {
                id: 1,
                name: "Built-in".into(),
                bounds: rect(0, 0, 3024, 1964),
                scale: ScaleFactor::new(2.0).unwrap(),
                is_primary: true,
                is_builtin: false,
            }],
            windows: vec![WindowInfo {
                id: 77,
                title: "Doc".into(),
                app: "Editor".into(),
                pid: 5,
                bounds: rect(200, 100, 1601, 901),
                scale: ScaleFactor::new(2.0).unwrap(),
                display_id: Some(1),
                minimized: false,
            }],
        }
    }

    #[test]
    fn rec1_ac1_display_source_is_native() {
        let s = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
        let p = plan_capture(&s, &targets()).unwrap();
        assert_eq!((p.video.width, p.video.height), (3024, 1964));
        assert_eq!(p.video.codec, Codec::Hevc); // REC-2.AC1
        assert!(p.crop.is_none() && p.show_cursor);
    }

    #[test]
    fn rec1_ac3_region_1280x720_records_1280x720_with_a_physical_crop() {
        let s = ScreenSettings::new(CaptureTarget::Region {
            display_id: 1,
            rect: RegionRect::Physical(rect(100, 80, 1280, 720)),
        });
        let p = plan_capture(&s, &targets()).unwrap();
        assert_eq!((p.video.width, p.video.height), (1280, 720));
        assert_eq!(p.crop, Some(rect(100, 80, 1280, 720)));
        assert_eq!(p.video.codec, Codec::H264);
    }

    #[test]
    fn logical_regions_use_the_screenshot_convention() {
        let s = ScreenSettings::new(CaptureTarget::Region {
            display_id: 1,
            rect: RegionRect::Logical(Rect::new(50.0, 40.0, 640.0, 360.0)),
        });
        let p = plan_capture(&s, &targets()).unwrap();
        assert_eq!(p.crop, Some(rect(100, 80, 1280, 720)));
    }

    #[test]
    fn region_outside_the_display_is_invalid_input() {
        let s = ScreenSettings::new(CaptureTarget::Region {
            display_id: 1,
            rect: RegionRect::Physical(rect(3000, 0, 100, 100)),
        });
        assert!(plan_capture(&s, &targets()).is_err());
    }

    #[test]
    fn window_size_is_fixed_at_start_and_even() {
        let s = ScreenSettings::new(CaptureTarget::Window { window_id: 77 });
        let p = plan_capture(&s, &targets()).unwrap();
        assert_eq!((p.video.width, p.video.height), (1600, 900));
        assert_eq!(p.source_size.width, 1601);
    }

    #[test]
    fn rec2_ac3_1080p_on_the_display_is_1662x1080() {
        let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
        s.video.size = OutputSize::P1080;
        let p = plan_capture(&s, &targets()).unwrap();
        assert_eq!((p.video.width, p.video.height), (1662, 1080));
    }

    #[test]
    fn unknown_targets_are_not_found() {
        let s = ScreenSettings::new(CaptureTarget::Display { display_id: 9 });
        assert!(matches!(
            plan_capture(&s, &targets()),
            Err(CmError::NotFound { .. })
        ));
        let s = ScreenSettings::new(CaptureTarget::Window { window_id: 9 });
        assert!(plan_capture(&s, &targets()).is_err());
    }

    #[test]
    fn adopting_a_new_source_size_keeps_the_scaling_the_user_chose() {
        let mut native = plan_capture(
            &ScreenSettings::new(CaptureTarget::Display { display_id: 1 }),
            &targets(),
        )
        .unwrap();
        native.adopt_source_size(PhysicalSize {
            width: 1281,
            height: 721,
        });
        assert_eq!((native.video.width, native.video.height), (1280, 720));
        assert_eq!(native.source_size.width, 1281);

        let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
        s.video.size = OutputSize::P1080;
        let mut scaled = plan_capture(&s, &targets()).unwrap();
        scaled.adopt_source_size(PhysicalSize {
            width: 3840,
            height: 2160,
        });
        assert_eq!((scaled.video.width, scaled.video.height), (1920, 1080));
        // Never upscaled: a source smaller than the asked-for height stays native.
        scaled.adopt_source_size(PhysicalSize {
            width: 1280,
            height: 720,
        });
        assert_eq!((scaled.video.width, scaled.video.height), (1280, 720));
    }
}
