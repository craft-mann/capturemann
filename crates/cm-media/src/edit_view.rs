//! What the viewer sees of the screen at one instant (EDT-4, EDT-5): the crop of zoom regions and
//! of a reframe window, and the telemetry cursor with its click effects. One [`EditView`] is built
//! from the edit list and used by **both** the export (`edited`) and the editor preview, so a
//! frame is drawn the same way in each (the M4 gate: the export matches the preview).
//!
//! The pure math is `cm_cursor::view` ([`Camera`], [`CursorTrack`]); this module only carries it
//! into a `cm_render::compose::Scene`.

use std::path::Path;
use std::sync::Arc;

use cm_core::{Aspect, ClickEffect, CursorStyle, EditList, MediaTime};
use cm_cursor::view::{
    fallback_arrow, load_cursor, Camera, CameraInput, CursorSprites, CursorTrack, Sprite,
};
use cm_render::compose::{ClickMark, ClickStyle, CursorLayer, CursorSprite, Frame, Scene};

/// Logical px of a click ring at its widest, before the cursor size is applied.
const EFFECT_RADIUS_PT: f64 = 28.0;
/// The click effect color (a clear blue, visible on light and dark screens).
const EFFECT_COLOR: [u8; 4] = [70, 140, 255, 235];

/// The recording's cursor track and images, loaded once and shared.
#[derive(Debug)]
pub struct CursorMedia {
    pub track: CursorTrack,
    pub sprites: CursorSprites,
}

impl CursorMedia {
    /// Reads `cursor.jsonl` and `cursors/` of a bundle. `None` when there is no cursor track or
    /// it cannot be read (the view then has no cursor and no cursor-following camera).
    pub fn load(bundle: &Path) -> Option<Arc<CursorMedia>> {
        match load_cursor(bundle) {
            Ok(Some((track, sprites))) if !track.is_empty() => {
                Some(Arc::new(CursorMedia { track, sprites }))
            }
            _ => None,
        }
    }
}

/// What an [`EditView`] is built from.
pub struct ViewParams<'a> {
    pub list: &'a EditList,
    pub media: Option<Arc<CursorMedia>>,
    /// The screen master as decoded, in px.
    pub screen_size: (u32, u32),
    /// The recorded area in physical px (the master may be scaled down from it).
    pub source_size: (u32, u32),
    /// Physical px per point of the recorded display (2.0 on Retina).
    pub display_scale: f64,
    /// The cursor is not in the screen track (EDT-4: "Record cursor separately"), so it is drawn
    /// from the telemetry. A baked cursor is left alone.
    pub cursor_separate: bool,
    /// The output frame the picture is drawn into (decides which reframe applies).
    pub out_size: (u32, u32),
}

/// The camera and the cursor of one edit list for one output size.
#[derive(Debug)]
pub struct EditView {
    camera: Camera,
    media: Option<Arc<CursorMedia>>,
    style: CursorStyle,
    draw_cursor: bool,
    /// Master px per physical source px.
    density: f64,
    display_scale: f64,
    fallback: Sprite,
}

impl EditView {
    pub fn new(p: &ViewParams<'_>) -> EditView {
        let list = p.list;
        let reframe = list
            .view
            .reframe_for_frame(p.out_size.0, p.out_size.1)
            .map(|(_, r)| r);
        let out_aspect = reframe.map(|_| f64::from(p.out_size.0) / f64::from(p.out_size.1.max(1)));
        let camera = Camera::new(&CameraInput {
            src_size: (f64::from(p.screen_size.0), f64::from(p.screen_size.1)),
            out_aspect,
            reframe,
            zooms: &list.view.zooms,
            track: p.media.as_deref().map(|m| &m.track),
            span: (list.start, list.end),
        });
        let density = f64::from(p.screen_size.0) / f64::from(p.source_size.0.max(1));
        EditView {
            camera,
            media: p.media.clone(),
            style: list.view.cursor,
            draw_cursor: p.cursor_separate && p.media.is_some(),
            density,
            display_scale: p.display_scale.max(1.0),
            fallback: fallback_arrow(p.display_scale.max(1.0)),
        }
    }

    /// The picture is cropped to the output aspect (EDT-5): it fills the frame, so the padding
    /// and the rounded screen corners of the background do not apply.
    pub fn reframing(&self) -> bool {
        self.camera.reframes()
    }

    pub fn drawing_cursor(&self) -> bool {
        self.draw_cursor
    }

    /// Nothing changes the picture: no crop, no zoom, no drawn cursor.
    pub fn is_plain(&self) -> bool {
        self.camera.is_identity() && !self.draw_cursor
    }

    /// The crop and the cursor at session time `t`.
    pub fn at(&self, t: MediaTime) -> FrameView<'_> {
        let crop = (!self.camera.is_identity()).then(|| self.camera.crop_at(t));
        let cursor = self
            .draw_cursor
            .then_some(self.media.as_ref())
            .flatten()
            .and_then(|m| m.track.frame_at(t, &self.style))
            .map(|f| {
                let (sprite, hotspot, sprite_scale) = match f.image.as_ref() {
                    Some(r) => match m_sprite(self.media.as_deref(), &r.id) {
                        Some(s) => (s, s.hotspot, s.scale),
                        None => (&self.fallback, self.fallback.hotspot, self.fallback.scale),
                    },
                    None => (&self.fallback, self.fallback.hotspot, self.fallback.scale),
                };
                // Master px per sprite px: the display density against the image density,
                // times the master's downscale and the user's size.
                let scale = (self.style.size * self.density * self.display_scale
                    / sprite_scale.max(0.01)) as f32;
                let style = match self.style.click_effect {
                    ClickEffect::None => ClickStyle::None,
                    ClickEffect::Ring => ClickStyle::Ring,
                    ClickEffect::Ripple => ClickStyle::Ripple,
                };
                DrawnCursor {
                    sprite,
                    hotspot: (hotspot[0] as f32, hotspot[1] as f32),
                    pos: f.pos,
                    scale,
                    opacity: f.opacity,
                    style,
                    clicks: f
                        .effects
                        .iter()
                        .map(|e| ClickMark {
                            pos: e.pos,
                            progress: e.progress,
                        })
                        .collect(),
                    radius: (EFFECT_RADIUS_PT * self.display_scale * self.density * self.style.size)
                        as f32,
                }
            });
        FrameView { crop, cursor }
    }
}

fn m_sprite<'a>(media: Option<&'a CursorMedia>, id: &str) -> Option<&'a Sprite> {
    media?.sprites.get(id)
}

struct DrawnCursor<'a> {
    sprite: &'a Sprite,
    hotspot: (f32, f32),
    pos: (f32, f32),
    scale: f32,
    opacity: f32,
    style: ClickStyle,
    clicks: Vec<ClickMark>,
    radius: f32,
}

/// The crop and cursor of one frame, ready to be put into a scene.
pub struct FrameView<'a> {
    /// The visible part of the screen as fractions `[x, y, w, h]`; `None` shows all of it.
    pub crop: Option<[f32; 4]>,
    cursor: Option<DrawnCursor<'a>>,
}

impl<'a> FrameView<'a> {
    /// Sets the crop of the screen layer and the cursor layer of `scene`. The scene borrows the
    /// cursor picture from this view, so keep the view alive while rendering.
    pub fn apply<'s>(&'s self, scene: &mut Scene<'s>)
    where
        'a: 's,
    {
        if let (Some(c), Some(s)) = (self.crop, scene.screen.as_mut()) {
            s.crop = Some(c);
        }
        scene.cursor = self.cursor.as_ref().map(|d| CursorLayer {
            sprite: Some(CursorSprite {
                frame: Frame::rgba(d.sprite.width, d.sprite.height, &d.sprite.rgba),
                hotspot: d.hotspot,
            }),
            pos: d.pos,
            scale: d.scale,
            opacity: d.opacity,
            click_style: d.style,
            clicks: &d.clicks,
            effect_radius_px: d.radius,
            effect_color: EFFECT_COLOR,
        });
    }
}

/// The reframe aspect an output frame belongs to, if it is one.
pub fn aspect_of(out: (u32, u32)) -> Option<Aspect> {
    Aspect::of_frame(out.0, out.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{EditContext, ReframeMode, ReframeUpdate, ZoomRegion, TICKS_PER_SECOND};

    fn t(secs: f64) -> MediaTime {
        MediaTime::from_ticks((secs * TICKS_PER_SECOND as f64) as i64)
    }

    fn list() -> EditList {
        let cx = EditContext {
            end: t(60.0),
            live_at: None,
            gaps: &[],
        };
        EditList::full(&cx)
    }

    fn media() -> Arc<CursorMedia> {
        let mut tr = CursorTrack::new((1920.0, 1080.0), 2.0);
        tr.push_move(0, 960.0, 540.0);
        tr.push_move(t(30.0).ticks(), 960.0, 540.0);
        Arc::new(CursorMedia {
            track: tr,
            sprites: CursorSprites::default(),
        })
    }

    fn params<'a>(l: &'a EditList, out: (u32, u32), separate: bool) -> ViewParams<'a> {
        ViewParams {
            list: l,
            media: Some(media()),
            screen_size: (1920, 1080),
            source_size: (3840, 2160),
            display_scale: 2.0,
            cursor_separate: separate,
            out_size: out,
        }
    }

    #[test]
    fn an_untouched_list_with_a_baked_cursor_is_plain() {
        let l = list();
        let v = EditView::new(&params(&l, (1920, 1080), false));
        assert!(v.is_plain());
        assert!(!v.reframing());
        let f = v.at(t(1.0));
        assert!(f.crop.is_none());
    }

    #[test]
    fn edt4_a_separate_cursor_is_drawn_with_the_fallback_arrow_at_the_telemetry_spot() {
        let l = list();
        let v = EditView::new(&params(&l, (1920, 1080), true));
        assert!(!v.is_plain());
        let f = v.at(t(1.0));
        let d = f.cursor.as_ref().unwrap();
        assert_eq!(d.pos, (0.5, 0.5));
        // 3840 px source -> 1920 px master (density 0.5); a 2x display and a 2x arrow: 1 px
        // of sprite is 0.5 master px at size 1.
        assert!((d.scale - 0.5).abs() < 1e-5, "{}", d.scale);
        let mut big = l.clone();
        big.view.cursor.size = 2.0;
        let v2 = EditView::new(&params(&big, (1920, 1080), true));
        assert!(
            (v2.at(t(1.0)).cursor.unwrap().scale - 1.0).abs() < 1e-5,
            "AC1: 2x is twice as big"
        );
    }

    #[test]
    fn edt4_zoom_regions_crop_the_frame_and_apply_puts_it_in_the_scene() {
        let l = list()
            .set_zooms(
                &[ZoomRegion {
                    at: t(10.0),
                    length: t(4.0),
                    focus: (0.25, 0.25),
                    scale: 2.0,
                    follow_cursor: false,
                }],
                false,
            )
            .unwrap();
        let v = EditView::new(&params(&l, (1920, 1080), false));
        assert!(v.at(t(5.0)).crop.is_some_and(|c| c == [0.0, 0.0, 1.0, 1.0]));
        let f = v.at(t(12.0));
        let c = f.crop.unwrap();
        assert!((c[2] - 0.5).abs() < 1e-4 && c[0].abs() < 1e-4, "{c:?}");
        let data = vec![0u8; 4 * 4 * 4];
        let mut scene = Scene::new((4, 4), Frame::rgba(4, 4, &data));
        f.apply(&mut scene);
        assert_eq!(scene.screen.unwrap().crop, Some(c));
        assert!(scene.cursor.is_none(), "baked cursor: nothing drawn");
    }

    #[test]
    fn edt5_ac3_a_reframe_applies_only_to_its_own_aspect() {
        let l = list()
            .set_reframe(&ReframeUpdate {
                aspect: Aspect::Vertical,
                mode: Some(ReframeMode::Fixed),
                focus: Some((0.5, 0.5)),
                at: None,
            })
            .unwrap();
        let portrait = EditView::new(&params(&l, (1080, 1920), false));
        assert!(portrait.reframing());
        let c = portrait.at(t(1.0)).crop.unwrap();
        assert!((c[3] - 1.0).abs() < 1e-6 && c[2] < 0.5);
        for out in [(1920, 1080), (1080, 1080)] {
            let v = EditView::new(&params(&l, out, false));
            assert!(!v.reframing(), "{out:?} is untouched");
            assert!(v.is_plain());
        }
    }
}
