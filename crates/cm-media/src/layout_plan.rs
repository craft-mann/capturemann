//! CAM-3 in the export: which compositor layout applies at a session time.
//!
//! The recorder stores the bubble it was recording with (`edits[0].layout.camera`, a
//! [`CameraLayout`]); CAM-3 adds layout ranges (`layout.segments`, `cm_core::camera_layout`) that
//! override it. [`LayoutPlanner::at`] turns the layout in force at one instant into the
//! compositor's [`Layout`], and during a smooth change also the layout it moves away from and how
//! far along it is, for `cm_render::compose::plan_blend` (CAM-3.AC3).

use cm_core::camera_layout::{layout_at, CameraLayoutSpec, CameraMode, LayoutSegment};
use cm_core::{EditList, MediaTime};
use cm_render::compose::{Bubble, BubbleShape, Layout, Ring};

use crate::layout::{bubble_geom, BubbleGeom, CameraLayout, Shape};
use crate::preset::{place_bubble, SafeArea};

/// The `rec` ring of the camera bubble, `#FF4F3A`.
pub(crate) const RING_RGBA: [u8; 4] = [0xFF, 0x4F, 0x3A, 0xFF];

/// Where a bubble's `rect` is measured: on the whole frame, or on the recording's picture inside
/// a preset frame (EXP-2), kept out of the platform's UI zones.
#[derive(Debug, Clone, Copy)]
pub enum Placement {
    Whole,
    OnPicture {
        pic: [u32; 4],
        safe: Option<SafeArea>,
    },
}

/// The compositor bubble for a pixel geometry. A ring width of 0 in the layout means no ring.
pub(crate) fn bubble_of(g: &BubbleGeom, l: &CameraLayout, out: (u32, u32)) -> Bubble {
    let (ow, oh) = (f64::from(out.0), f64::from(out.1));
    let short = ow.min(oh);
    Bubble {
        shape: match l.shape {
            Shape::Circle => BubbleShape::Circle,
            Shape::Rounded => BubbleShape::Rounded,
        },
        size: (f64::from(g.w) / short) as f32,
        center: (
            ((f64::from(g.x) + f64::from(g.w) / 2.0) / ow) as f32,
            ((f64::from(g.y) + f64::from(g.h) / 2.0) / oh) as f32,
        ),
        ring: (l.ring > 0.0).then_some(Ring {
            width_px: g.ring_px as f32,
            color: RING_RGBA,
        }),
    }
}

/// The geometry of the bubble `l` on a frame of size `out` (the same arithmetic the plain export
/// uses).
pub(crate) fn geom_of(l: &CameraLayout, out: (u32, u32), placement: Placement) -> BubbleGeom {
    match placement {
        Placement::Whole => bubble_geom(l, out),
        Placement::OnPicture { pic, safe } => place_bubble(l, out, pic, safe),
    }
}

/// Applies a layout range's bubble settings over the recorder's layout.
fn with_overrides(base: &CameraLayout, spec: &CameraLayoutSpec) -> CameraLayout {
    let mut l = base.clone();
    if let Some(s) = spec.shape {
        l.shape = match s {
            cm_core::BubbleKind::Circle => Shape::Circle,
            cm_core::BubbleKind::Rounded => Shape::Rounded,
        };
    }
    if let Some(r) = spec.rect {
        l.rect = r;
    }
    if let Some(g) = spec.ring {
        l.ring = g;
    }
    if let Some(m) = spec.mirror {
        l.mirror = Some(m);
    }
    l
}

/// Per-frame layout lookup for one export.
#[derive(Debug, Clone)]
pub struct LayoutPlanner {
    segments: Vec<LayoutSegment>,
    /// The recorder's bubble; `None` when there is no camera to draw.
    base: Option<CameraLayout>,
    out: (u32, u32),
    placement: Placement,
}

impl LayoutPlanner {
    pub fn new(
        list: &EditList,
        base: Option<CameraLayout>,
        out: (u32, u32),
        placement: Placement,
    ) -> Self {
        LayoutPlanner {
            segments: list.camera_segments(),
            base,
            out,
            placement,
        }
    }

    /// Layout ranges change what the camera feed must provide (a full-frame or side-by-side
    /// picture, a bubble of another size), so the feed is decoded uncropped and wide.
    pub fn is_flexible(&self) -> bool {
        self.base.is_some() && !self.segments.is_empty()
    }

    /// The compositor layout for one layout setting.
    pub fn layout_of(&self, spec: &CameraLayoutSpec) -> Layout {
        let Some(base) = &self.base else {
            return Layout::ScreenOnly;
        };
        match spec.mode {
            CameraMode::Screen => Layout::ScreenOnly,
            CameraMode::Camera => Layout::CameraFull,
            CameraMode::SideBySide => Layout::SideBySide,
            CameraMode::Bubble => {
                let l = with_overrides(base, spec);
                let g = geom_of(&l, self.out, self.placement);
                Layout::ScreenBubble(bubble_of(&g, &l, self.out))
            }
        }
    }

    /// Whether the camera picture is mirrored under `spec`.
    pub fn mirror_of(&self, spec: &CameraLayoutSpec) -> bool {
        spec.mirror
            .or_else(|| self.base.as_ref().and_then(|b| b.mirror))
            .unwrap_or(false)
    }

    /// The layout at session time `t`: the one in force, its mirror flag, and while a smooth
    /// change is under way the layout it comes from and the progress `0..1`.
    pub fn at(&self, t: MediaTime) -> FrameLayout {
        let at = layout_at(&self.segments, t);
        FrameLayout {
            layout: self.layout_of(&at.current),
            mirror: self.mirror_of(&at.current),
            from: at.from.map(|(p, f)| (self.layout_of(&p), f)),
        }
    }
}

/// What one output frame is drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameLayout {
    pub layout: Layout,
    pub mirror: bool,
    pub from: Option<(Layout, f32)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::camera_layout::{BubbleKind, LayoutTransition};
    use cm_core::{EditContext, Span};

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn list(segs: &[(f64, f64, CameraLayoutSpec)]) -> EditList {
        let mut l = EditList::full(&EditContext {
            end: t(60.0),
            live_at: None,
            gaps: &[],
        });
        for (a, b, s) in segs {
            l = l.set_camera_layout(Span::new(t(*a), t(*b)), s).unwrap();
        }
        l
    }

    fn base() -> CameraLayout {
        CameraLayout {
            visible: true,
            shape: Shape::Circle,
            rect: [0.78, 0.72, 0.16, 0.28],
            mirror: Some(true),
            ring: crate::layout::DEFAULT_RING,
            radius: crate::layout::DEFAULT_RADIUS,
            size: None,
        }
    }

    /// With no ranges the default bubble is drawn, exactly as the plain export draws it.
    #[test]
    fn no_ranges_is_the_recorders_bubble() {
        let p = LayoutPlanner::new(&list(&[]), Some(base()), (1280, 720), Placement::Whole);
        assert!(!p.is_flexible());
        let f = p.at(t(5.0));
        let g = bubble_geom(&base(), (1280, 720));
        assert_eq!(
            f.layout,
            Layout::ScreenBubble(bubble_of(&g, &base(), (1280, 720)))
        );
        assert!(f.mirror && f.from.is_none());
    }

    /// CAM-3.AC1/AC2: a dragged bubble and camera full resolve by time.
    #[test]
    fn cam3_ranges_choose_the_layout_by_time() {
        let mut bubble = CameraLayoutSpec::new(CameraMode::Bubble);
        bubble.rect = Some([0.02, 0.02, 0.2, 0.2]);
        bubble.shape = Some(BubbleKind::Rounded);
        bubble.ring = Some(0.0);
        let l = list(&[
            (0.0, 15.0, CameraLayoutSpec::new(CameraMode::Camera)),
            (20.0, 30.0, bubble),
        ]);
        let p = LayoutPlanner::new(&l, Some(base()), (1280, 720), Placement::Whole);
        assert!(p.is_flexible());
        assert_eq!(p.at(t(1.0)).layout, Layout::CameraFull);
        assert!(matches!(p.at(t(16.0)).layout, Layout::ScreenBubble(b)
            if b.center.0 > 0.5 && b.ring.is_some()));
        // Top left, rounded, no ring.
        let Layout::ScreenBubble(b) = p.at(t(25.0)).layout else {
            panic!("a bubble")
        };
        assert!(b.center.0 < 0.2 && b.center.1 < 0.3);
        assert_eq!(b.shape, BubbleShape::Rounded);
        assert!(b.ring.is_none());
    }

    /// CAM-3.AC3: during the first 300 ms of a smooth range the previous layout and the progress
    /// are reported; a hidden camera turns every layout into screen only.
    #[test]
    fn smooth_ranges_report_the_layout_they_come_from() {
        let mut cam = CameraLayoutSpec::new(CameraMode::Camera);
        cam.transition = LayoutTransition::Smooth;
        let l = list(&[
            (0.0, 10.0, CameraLayoutSpec::new(CameraMode::Screen)),
            (10.0, 20.0, cam),
        ]);
        let p = LayoutPlanner::new(&l, Some(base()), (1280, 720), Placement::Whole);
        let f = p.at(t(10.15));
        let (from, k) = f.from.unwrap();
        assert_eq!(from, Layout::ScreenOnly);
        assert!((k - 0.5).abs() < 0.01);
        assert_eq!(f.layout, Layout::CameraFull);
        let none = LayoutPlanner::new(&l, None, (1280, 720), Placement::Whole);
        assert_eq!(none.at(t(12.0)).layout, Layout::ScreenOnly);
    }

    /// CAM-3.AC4: on a 9:16 preset frame every bubble stays inside the frame and out of the safe
    /// zone.
    #[test]
    fn cam3_ac4_bubble_stays_inside_a_portrait_frame() {
        let mut spec = CameraLayoutSpec::new(CameraMode::Bubble);
        spec.rect = Some([0.95, 0.95, 0.3, 0.3]);
        let l = list(&[(0.0, 30.0, spec)]);
        let out = (1080, 1920);
        let pic = [0, 656, 1080, 608];
        let safe = Some(SafeArea {
            bottom: 0.2,
            right: 0.12,
        });
        let p = LayoutPlanner::new(&l, Some(base()), out, Placement::OnPicture { pic, safe });
        let Layout::ScreenBubble(b) = p.at(t(5.0)).layout else {
            panic!("a bubble")
        };
        let d = f64::from(b.size) * f64::from(out.0.min(out.1));
        let (cx, cy) = (
            f64::from(b.center.0) * 1080.0,
            f64::from(b.center.1) * 1920.0,
        );
        assert!(
            cx - d / 2.0 >= 0.0 && cx + d / 2.0 <= 1080.0 * 0.88 + 1.0,
            "x {cx} d {d}"
        );
        assert!(
            cy - d / 2.0 >= 0.0 && cy + d / 2.0 <= 1920.0 * 0.8 + 1.0,
            "y {cy} d {d}"
        );
    }
}
