//! The preview renderer: a worker thread that owns the [`FrameSource`] and the GPU compositor,
//! takes the newest request (a session time and a background) and leaves the newest finished
//! picture for the view. Requests are latest-wins, so dragging the playhead never queues work, and
//! a slow decode never blocks the UI thread.
//!
//! The same `cm_render::Compositor` draws the export, so the background, padding, corner radius
//! and camera bubble in the preview are the ones in the file (EXP-1). Without a GPU adapter the
//! screen picture is shown plain, resized to the preview size, and `Rendered::plain` says so.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use cm_core::{BackgroundKind, EditList, MediaTime};
use cm_media::edit_view::{CursorMedia, EditView, ViewParams};
use cm_media::layout_plan::FrameLayout;
use cm_media::CaptionTrack;
use cm_render::compose::{
    plan_blend, Background, CameraLayer, CaptionLayer, Compositor, Frame, Scene, ScreenLayer,
};
use image::RgbaImage;

use super::inspector::BgSpec;
use super::source::{FrameSource, SourceFrames};
use super::viewedit::PreviewAspect;

/// Preview pictures are at most this wide.
pub const PREVIEW_WIDTH: u32 = 1280;

/// What the renderer knows about the recording beyond its pictures: where the cursor track is and
/// how the recorded area maps to the picture (EDT-4, EDT-5).
#[derive(Debug, Clone)]
pub struct ViewContext {
    pub bundle: PathBuf,
    /// The recorded area in physical px.
    pub source_size: (u32, u32),
    /// Physical px per point of the recorded display.
    pub display_scale: f64,
    /// The cursor is not in the picture, so it is drawn from the telemetry.
    pub cursor_separate: bool,
}

/// The edit list and the output shape the pictures are drawn for. A new `id` means the zooms, the
/// cursor style, the reframe or the previewed aspect changed.
#[derive(Debug, Clone)]
pub struct ViewSpec {
    pub id: u64,
    pub list: EditList,
    pub aspect: PreviewAspect,
}

/// What to draw.
#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub seq: u64,
    pub session: MediaTime,
    pub background: BgSpec,
    /// The camera layout at `session` (CAM-3), from `LayoutPlanner::at` on the preview's size.
    pub layout: FrameLayout,
    /// Zooms, cursor and reframe; `None` draws the plain screen (tests, no view context).
    pub view: Option<Arc<ViewSpec>>,
    /// The captions to show (AI-1): the recording's caption lines on its output timeline, drawn
    /// by the same code as the export's burn-in. `None` shows none (the switch is off, or the
    /// recording has no transcript).
    pub captions: Option<Arc<CaptionTrack>>,
}

/// A finished preview picture.
pub struct Rendered {
    pub seq: u64,
    pub image: RgbaImage,
    /// The GPU compositor was not available: the picture has no background or bubble.
    pub plain: bool,
    /// Why the source could not give a picture for this time (the previous one stays).
    pub error: Option<String>,
}

#[derive(Default)]
struct Shared {
    request: Mutex<Option<RenderRequest>>,
    wake: Condvar,
    out: Mutex<Option<Rendered>>,
    stop: AtomicBool,
}

pub struct PreviewRenderer {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    label: String,
    size: (u32, u32),
    source_size: (u32, u32),
}

/// The preview size for a screen of `source` pixels: its aspect, at most [`PREVIEW_WIDTH`] wide,
/// both sides even.
pub fn preview_size(source: (u32, u32)) -> (u32, u32) {
    let (w, h) = (source.0.max(2), source.1.max(2));
    let pw = w.min(PREVIEW_WIDTH) & !1;
    let ph = ((u64::from(pw) * u64::from(h) / u64::from(w)) as u32 & !1).max(2);
    (pw.max(2), ph)
}

impl PreviewRenderer {
    pub fn spawn(source: Box<dyn FrameSource>) -> PreviewRenderer {
        Self::spawn_with(source, None)
    }

    /// With a [`ViewContext`] the pictures get the zooms, the telemetry cursor and the reframe of
    /// the request's [`ViewSpec`], drawn by the same code as the export.
    pub fn spawn_with(source: Box<dyn FrameSource>, ctx: Option<ViewContext>) -> PreviewRenderer {
        let shared = Arc::new(Shared::default());
        let label = source.label();
        let source_size = source.size();
        let size = preview_size(source_size);
        let thread = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("recedit-preview".into())
                .spawn(move || run(source, size, ctx, &shared))
                .ok()
        };
        PreviewRenderer {
            shared,
            thread,
            label,
            size,
            source_size,
        }
    }

    /// The size of the screen picture the preview is made from.
    pub fn source_size(&self) -> (u32, u32) {
        self.source_size
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Replaces any request still waiting.
    pub fn request(&self, r: RenderRequest) {
        *self
            .shared
            .request
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(r);
        self.shared.wake.notify_one();
    }

    /// The newest finished picture, once.
    pub fn take(&self) -> Option<Rendered> {
        self.shared
            .out
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for PreviewRenderer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The [`EditView`] of the newest request, kept while its edit list, aspect and frame size hold.
struct ViewCache {
    id: u64,
    frame: (u32, u32),
    view: EditView,
}

fn run(
    mut source: Box<dyn FrameSource>,
    size: (u32, u32),
    ctx: Option<ViewContext>,
    shared: &Shared,
) {
    // The GPU compositor lives on this thread only (it is not shared across threads).
    let mut gpu = Compositor::new().ok();
    let mut image_cache: Option<(String, RgbaImage)> = None;
    let mut last: Option<SourceFrames> = None;
    // The cursor track is read once, and only when a recording draws its cursor or a zoom
    // follows it.
    let media = ctx.as_ref().and_then(|c| CursorMedia::load(&c.bundle));
    let mut views: Option<ViewCache> = None;
    loop {
        let request = {
            let mut slot = shared
                .request
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(r) = slot.take() {
                    break r;
                }
                slot = shared
                    .wake
                    .wait_timeout(slot, Duration::from_millis(200))
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        };
        let mut error = None;
        match source.frame_at(request.session) {
            Ok(f) => last = Some(f),
            Err(e) => error = Some(e),
        }
        let Some(frames) = last.as_ref() else {
            // Nothing to draw yet: say why, with a blank picture of the right size.
            publish(
                shared,
                Rendered {
                    seq: request.seq,
                    image: RgbaImage::from_pixel(size.0, size.1, image::Rgba([0, 0, 0, 255])),
                    plain: true,
                    error,
                },
            );
            continue;
        };
        // Zooms, the cursor and the reframe: rebuilt when the edit list or the aspect changed.
        let frame_size = frames.screen.dimensions();
        let (view, out) = match (&request.view, &ctx) {
            (Some(spec), Some(c)) => {
                if views
                    .as_ref()
                    .is_none_or(|v| v.id != spec.id || v.frame != frame_size)
                {
                    let out_size = spec.aspect.out_size(c.source_size);
                    views = Some(ViewCache {
                        id: spec.id,
                        frame: frame_size,
                        view: EditView::new(&ViewParams {
                            list: &spec.list,
                            media: media.clone(),
                            screen_size: frame_size,
                            source_size: c.source_size,
                            display_scale: c.display_scale,
                            cursor_separate: c.cursor_separate,
                            out_size,
                        }),
                    });
                }
                (
                    views.as_ref().map(|v| &v.view),
                    spec.aspect.out_size(c.source_size),
                )
            }
            _ => (None, size),
        };
        let caption_layer = request
            .captions
            .as_ref()
            .and_then(|track| track.layer_at(request.session, out));
        let (image, plain) = compose_with(
            &mut gpu,
            frames,
            &request.background,
            &request.layout,
            out,
            view.map(|v| (v, request.session)),
            caption_layer,
            &mut image_cache,
        );
        publish(
            shared,
            Rendered {
                seq: request.seq,
                image,
                plain,
                error,
            },
        );
    }
}

fn publish(shared: &Shared, r: Rendered) {
    *shared.out.lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
}

/// `#rrggbb` (or `#rgb`) to bytes.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    let full: String = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        6 => h.to_owned(),
        _ => return None,
    };
    let n = u32::from_str_radix(&full, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// The compositor's background for `spec`. An image background is read from its path and kept in
/// `cache` (the `Frame` borrows from it).
pub fn background_of<'a>(
    spec: &BgSpec,
    cache: &'a mut Option<(String, RgbaImage)>,
) -> Background<'a> {
    match spec.kind {
        BackgroundKind::None => Background::None,
        BackgroundKind::Color => spec
            .value
            .as_str()
            .and_then(parse_hex)
            .map_or(Background::None, Background::Color),
        BackgroundKind::Gradient => {
            let pair = spec.value.as_array().and_then(|a| {
                Some((
                    parse_hex(a.first()?.as_str()?)?,
                    parse_hex(a.get(1)?.as_str()?)?,
                ))
            });
            pair.map_or(Background::None, |(from, to)| Background::Gradient {
                from,
                to,
                angle_deg: 135.0,
            })
        }
        BackgroundKind::Image => {
            let Some(path) = spec.value.as_str() else {
                return Background::None;
            };
            if cache.as_ref().map(|(p, _)| p.as_str()) != Some(path) {
                *cache = image::open(path)
                    .ok()
                    .map(|i| (path.to_owned(), i.to_rgba8()));
            }
            match cache {
                Some((_, img)) => {
                    Background::Image(Frame::rgba(img.width(), img.height(), img.as_raw()))
                }
                None => Background::None,
            }
        }
    }
}

/// One preview picture without captions (the tests' shorthand for [`compose_with`]). Returns it
/// and whether it is the plain fallback (no GPU or a failed draw).
#[cfg(test)]
fn compose(
    gpu: &mut Option<Compositor>,
    frames: &SourceFrames,
    bg: &BgSpec,
    layout: &FrameLayout,
    size: (u32, u32),
    view: Option<(&EditView, MediaTime)>,
    cache: &mut Option<(String, RgbaImage)>,
) -> (RgbaImage, bool) {
    compose_with(gpu, frames, bg, layout, size, view, None, cache)
}

/// One preview picture, with the caption layer to burn in over it (AI-1), if any.
#[allow(clippy::too_many_arguments)]
fn compose_with(
    gpu: &mut Option<Compositor>,
    frames: &SourceFrames,
    bg: &BgSpec,
    layout: &FrameLayout,
    size: (u32, u32),
    view: Option<(&EditView, MediaTime)>,
    captions: Option<CaptionLayer<'_>>,
    cache: &mut Option<(String, RgbaImage)>,
) -> (RgbaImage, bool) {
    if let Some(c) = gpu.as_mut() {
        let background = background_of(bg, cache);
        let screen = &frames.screen;
        // The crop and the cursor of this instant, declared before the scene that borrows them.
        let frame_view = view.map(|(v, t)| v.at(t));
        // A reframed picture fills the frame: no padding, rounded corners or shadow (EDT-5).
        let reframing = view.is_some_and(|(v, _)| v.reframing());
        let mut scene = Scene::new(
            size,
            Frame::rgba(screen.width(), screen.height(), screen.as_raw()),
        );
        scene.background = background;
        scene.padding_pct = if reframing {
            0.0
        } else {
            bg.padding_pct as f32
        };
        scene.radius_px = if reframing { 0.0 } else { bg.radius_px as f32 };
        scene.shadow = bg.shadow && !reframing;
        scene.screen = Some(ScreenLayer::new(Frame::rgba(
            screen.width(),
            screen.height(),
            screen.as_raw(),
        )));
        if let Some(cam) = &frames.camera {
            scene.camera = Some(CameraLayer {
                frame: Frame::rgba(cam.width(), cam.height(), cam.as_raw()),
                mirror: layout.mirror,
            });
            scene.layout = layout.layout;
        }
        if let Some(fv) = &frame_view {
            fv.apply(&mut scene);
        }
        scene.captions = captions;
        // CAM-3.AC3: part way through a smooth change the tiles are between the two layouts.
        let drawn = match (&frames.camera, layout.from) {
            (Some(_), Some((from, f))) => {
                plan_blend(&scene, from, f).and_then(|p| c.render_with_plan(&scene, p))
            }
            _ => c.render(&scene),
        };
        match drawn {
            Ok(img) => return (img, false),
            Err(e) => eprintln!("cm-desktop: preview compose failed: {e}"),
        }
    }
    let plain = image::imageops::resize(
        &frames.screen,
        size.0,
        size.1,
        image::imageops::FilterType::Triangle,
    );
    (plain, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recedit::inspector::Preset;
    use crate::recedit::source::SyntheticFrames;
    use cm_render::compose::Layout;
    use serde_json::json;
    use std::time::Instant;

    fn none() -> BgSpec {
        BgSpec::of(&cm_core::Background::default())
    }

    /// Screen only, no change under way.
    fn plain_layout() -> FrameLayout {
        FrameLayout {
            layout: Layout::ScreenOnly,
            mirror: false,
            from: None,
        }
    }

    #[test]
    fn hex_colours_parse() {
        assert_eq!(parse_hex("#e8e9ec"), Some([0xe8, 0xe9, 0xec]));
        assert_eq!(parse_hex("#fff"), Some([255, 255, 255]));
        assert_eq!(parse_hex("e8e9ec"), None);
        assert_eq!(parse_hex("#12"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    #[test]
    fn preview_size_keeps_the_aspect_and_is_even() {
        assert_eq!(preview_size((3024, 1964)), (1280, 830));
        assert_eq!(preview_size((1920, 1080)), (1280, 720));
        assert_eq!(preview_size((640, 360)), (640, 360));
        let (w, h) = preview_size((1001, 777));
        assert!(w % 2 == 0 && h % 2 == 0);
    }

    /// EDT-3: each kind of background maps to the compositor's.
    #[test]
    fn edt3_backgrounds_map_to_the_compositors() {
        let mut cache = None;
        assert!(matches!(
            background_of(&none(), &mut cache),
            Background::None
        ));
        let soft = BgSpec::preset(Preset::SoftGray, &none());
        assert!(matches!(
            background_of(&soft, &mut cache),
            Background::Color([0xe8, 0xe9, 0xec])
        ));
        let violet = BgSpec::preset(Preset::LensViolet, &none());
        assert!(matches!(
            background_of(&violet, &mut cache),
            Background::Gradient { .. }
        ));
        // A broken value falls back to no background instead of failing the preview.
        let bad = BgSpec {
            value: json!(7),
            ..soft
        };
        assert!(matches!(background_of(&bad, &mut cache), Background::None));
    }

    /// EDT-3.AC1/AC3: the preview draws the padding and the background, or none when off. Skipped
    /// without a GPU adapter.
    #[test]
    fn edt3_preview_draws_background_and_padding() {
        let Ok(c) = Compositor::new() else {
            eprintln!("SKIPPED: no GPU adapter");
            return;
        };
        let mut gpu = Some(c);
        let frames = SyntheticFrames::default()
            .frame_at(MediaTime::ZERO)
            .unwrap();
        let mut cache = None;
        let size = (640, 360);
        let soft = BgSpec::preset(Preset::SoftGray, &none());
        let (img, plain) = compose(
            &mut gpu,
            &frames,
            &soft,
            &plain_layout(),
            size,
            None,
            &mut cache,
        );
        assert!(!plain);
        assert_eq!(img.dimensions(), size);
        // The corner is background (8 % padding), the middle is the screen.
        let corner = img.get_pixel(2, 2).0;
        assert!(
            corner[0].abs_diff(0xe8) <= 4 && corner[1].abs_diff(0xe9) <= 4,
            "{corner:?}"
        );
        // Off: the screen goes edge to edge, so the corner is the picture's own.
        let (flat, _) = compose(
            &mut gpu,
            &frames,
            &none(),
            &plain_layout(),
            size,
            None,
            &mut cache,
        );
        assert_ne!(flat.get_pixel(2, 2).0, corner);
    }

    /// CAM-3: the preview draws the layout it is given: camera full replaces the screen, a smooth
    /// change part way draws the camera between its two places. Skipped without a GPU adapter.
    #[test]
    fn cam3_preview_draws_the_camera_layout() {
        let Ok(c) = Compositor::new() else {
            eprintln!("SKIPPED: no GPU adapter");
            return;
        };
        let mut gpu = Some(c);
        let mut frames = SyntheticFrames::default()
            .frame_at(MediaTime::ZERO)
            .unwrap();
        frames.camera = Some(RgbaImage::from_pixel(
            160,
            90,
            image::Rgba([220, 40, 40, 255]),
        ));
        let mut cache = None;
        let size = (640, 360);
        let full = FrameLayout {
            layout: Layout::CameraFull,
            mirror: false,
            from: None,
        };
        let (img, plain) = compose(&mut gpu, &frames, &none(), &full, size, None, &mut cache);
        assert!(!plain);
        for (x, y) in [(5, 5), (320, 180), (630, 350)] {
            let p = img.get_pixel(x, y).0;
            assert!(
                p[0].abs_diff(220) <= 6 && p[1].abs_diff(40) <= 6,
                "({x},{y}) {p:?}"
            );
        }
        // Half way from the screen to camera full: the camera does not cover the corner yet.
        let moving = FrameLayout {
            from: Some((Layout::ScreenOnly, 0.5)),
            ..full
        };
        let (mid, _) = compose(&mut gpu, &frames, &none(), &moving, size, None, &mut cache);
        let corner = mid.get_pixel(5, 5).0;
        assert!(
            !(corner[0].abs_diff(220) <= 6 && corner[1].abs_diff(40) <= 6),
            "{corner:?}"
        );
        let centre = mid.get_pixel(320, 180).0;
        assert!(
            centre[0].abs_diff(220) <= 6 && centre[1].abs_diff(40) <= 6,
            "{centre:?}"
        );
    }

    /// The worker answers the newest request, drops older ones and stops cleanly.
    #[test]
    fn worker_is_latest_wins_and_stops() {
        let mut r = PreviewRenderer::spawn(Box::new(SyntheticFrames::default()));
        assert_eq!(r.size(), (1280, 720));
        for seq in 1..=5 {
            r.request(RenderRequest {
                seq,
                session: MediaTime::from_secs(seq as i64),
                background: none(),
                layout: plain_layout(),
                view: None,
                captions: None,
            });
        }
        let started = Instant::now();
        let mut last = 0;
        while last < 5 && started.elapsed() < Duration::from_secs(20) {
            if let Some(done) = r.take() {
                assert!(done.seq > last, "pictures arrive in order");
                assert_eq!(done.image.dimensions(), (1280, 720));
                last = done.seq;
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        assert_eq!(last, 5, "the newest request is always answered");
        r.stop();
    }

    /// EDT-4/EDT-5 in the preview: a vertical preview is drawn at the portrait size with the
    /// reframe applied, and a zoom changes the picture. Skipped without a GPU adapter.
    #[test]
    fn edt5_the_preview_applies_the_zoom_and_the_vertical_reframe() {
        use cm_core::{Aspect, EditContext, ReframeMode, ReframeUpdate, ZoomRegion};
        if Compositor::new().is_err() {
            eprintln!("SKIPPED: no GPU adapter");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let ctx = ViewContext {
            bundle: dir.path().to_path_buf(),
            source_size: (1280, 720),
            display_scale: 1.0,
            cursor_separate: false,
        };
        let r = PreviewRenderer::spawn_with(Box::new(SyntheticFrames::default()), Some(ctx));
        let cx = EditContext {
            end: MediaTime::from_secs(60),
            live_at: None,
            gaps: &[],
        };
        let plain = EditList::full(&cx);
        let zoomed = plain
            .set_zooms(
                &[ZoomRegion {
                    at: MediaTime::from_secs(1),
                    length: MediaTime::from_secs(4),
                    focus: (0.25, 0.25),
                    scale: 2.0,
                    follow_cursor: false,
                }],
                false,
            )
            .unwrap();
        let vertical = plain
            .set_reframe(&ReframeUpdate {
                aspect: Aspect::Vertical,
                mode: Some(ReframeMode::Fixed),
                focus: Some((0.5, 0.5)),
                at: None,
            })
            .unwrap();
        let wait = |r: &PreviewRenderer, seq: u64| {
            let t = Instant::now();
            loop {
                if let Some(d) = r.take() {
                    if d.seq == seq {
                        return d;
                    }
                }
                assert!(t.elapsed() < Duration::from_secs(20), "timed out");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let ask = |seq: u64, list: &EditList, aspect: PreviewAspect, t: i64| {
            r.request(RenderRequest {
                seq,
                session: MediaTime::from_secs(t),
                background: none(),
                layout: plain_layout(),
                view: Some(Arc::new(ViewSpec {
                    id: seq,
                    list: list.clone(),
                    aspect,
                })),
                captions: None,
            });
            wait(&r, seq)
        };
        let a = ask(1, &plain, PreviewAspect::Wide, 3);
        let b = ask(2, &zoomed, PreviewAspect::Wide, 3);
        assert!(!a.plain && !b.plain);
        assert_eq!(a.image.dimensions(), (1280, 720));
        assert_eq!(b.image.dimensions(), (1280, 720));
        assert_ne!(
            a.image.as_raw(),
            b.image.as_raw(),
            "EDT-4.AC2: the zoom changes the picture"
        );
        // Outside the zoom the picture is the plain one again.
        let c = ask(3, &zoomed, PreviewAspect::Wide, 10);
        let d = ask(4, &plain, PreviewAspect::Wide, 10);
        assert_eq!(c.image.as_raw(), d.image.as_raw());
        // The vertical preview has its own size, and a Fit/none list is only letterboxed.
        let v = ask(5, &vertical, PreviewAspect::Vertical, 3);
        assert_eq!(v.image.dimensions(), (540, 960));
        let w = ask(6, &vertical, PreviewAspect::Wide, 3);
        assert_eq!(
            w.image.dimensions(),
            (1280, 720),
            "the 16:9 preview ignores a 9:16 reframe"
        );
        assert_eq!(
            w.image.as_raw(),
            a.image.as_raw(),
            "EDT-5.AC3: 16:9 is unchanged"
        );
    }

    /// A source that fails keeps the previous picture and reports the reason.
    #[test]
    fn source_errors_keep_the_last_picture() {
        struct Flaky(u32);
        impl FrameSource for Flaky {
            fn size(&self) -> (u32, u32) {
                (640, 360)
            }
            fn frame_at(&mut self, t: MediaTime) -> Result<SourceFrames, String> {
                self.0 += 1;
                if self.0 == 1 {
                    SyntheticFrames::default().frame_at(t)
                } else {
                    Err("no picture here".into())
                }
            }
            fn label(&self) -> String {
                "flaky".into()
            }
        }
        let r = PreviewRenderer::spawn(Box::new(Flaky(0)));
        let wait = |r: &PreviewRenderer, seq: u64| {
            let t = Instant::now();
            loop {
                if let Some(d) = r.take() {
                    if d.seq == seq {
                        return d;
                    }
                }
                assert!(t.elapsed() < Duration::from_secs(20), "timed out");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        r.request(RenderRequest {
            seq: 1,
            session: MediaTime::ZERO,
            background: none(),
            layout: plain_layout(),
            view: None,
            captions: None,
        });
        assert!(wait(&r, 1).error.is_none());
        r.request(RenderRequest {
            seq: 2,
            session: MediaTime::from_secs(1),
            background: none(),
            layout: plain_layout(),
            view: None,
            captions: None,
        });
        let second = wait(&r, 2);
        assert_eq!(second.error.as_deref(), Some("no picture here"));
        assert_eq!(second.image.dimensions(), (640, 360));
    }
}
