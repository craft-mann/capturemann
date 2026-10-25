//! The pure math behind the editable cursor (EDT-4) and the camera of zooms and reframes
//! (EDT-4, EDT-5): where the cursor is at an output frame, which click effects are showing, and
//! which part of the screen frame the viewer sees. No GPU and no files except
//! [`CursorSprites::load`]; the editor preview and the exporter both call these functions, so a
//! frame looks the same in both.
//!
//! # Cursor
//!
//! [`CursorTrack`] turns `cursor.jsonl` into a uniform 120 Hz path. Between two telemetry events
//! the path glides linearly; after a long pause (the pointer was still) it holds, then glides in
//! the last 16 ms. Smoothing is a *zero-phase* low-pass (an exponential filter run forward, then
//! backward), so a smoothed cursor reaches a button at the same time as the real one and a click
//! effect never lands before the pointer arrives.
//!
//! # Camera
//!
//! [`Camera::crop_at`] returns the visible rectangle of the screen frame as fractions
//! `[x, y, w, h]` for a session time. It combines
//!
//! * the **reframe window** (EDT-5): the largest window of the output aspect, fixed per segment
//!   or following the cursor,
//! * **zoom regions** (EDT-4): the window shrinks by `scale` over 400 ms, holds, and grows back,
//!   its center moving from the reframe center to the zoom focus (or the cursor).
//!
//! "Follow cursor" keeps the cursor in the middle 80% of the window (a dead zone of 10% on each
//! side): the center only moves when the cursor leaves it, with a short smoothing, and never lets
//! the cursor out of the window. The follow paths are simulated once at 60 Hz in [`Camera::new`],
//! so `crop_at` is a pure lookup that can be called in any order (scrubbing, parallel export).

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use cm_core::{
    ClickEffect, CmError, CursorStyle, MediaTime, Reframe, ReframeMode, Smoothing, ZoomRegion,
    IDLE_HIDE, ZOOM_EASE,
};

use crate::image::{CursorImage, ImageIndex};
use crate::reader::CursorLog;
use crate::schema::{CursorEvent, CURSORS_DIR};

const TPS: f64 = cm_core::TICKS_PER_SECOND as f64;
/// Resampling step of the cursor path: 120 Hz.
const PATH_STEP: i64 = 1_000;
/// Camera simulation step: 60 Hz.
const CAMERA_STEP: i64 = 2_000;
/// A telemetry gap longer than this means the pointer was still.
const HOLD_GAP: i64 = 9_600;
/// How long before the next event a held pointer starts to glide.
const GLIDE_LEAD: i64 = 1_920;
/// Fade-out time when the idle cursor disappears.
const FADE_OUT: i64 = 24_000;
/// How long a click ring and a ripple last.
const RING_TICKS: i64 = 54_000;
const RIPPLE_TICKS: i64 = 84_000;
/// Most click effects drawn at once.
pub const MAX_EFFECTS: usize = 3;
/// Share of the window the cursor may roam in before a following camera moves (EDT-4.AC3).
const DEAD_ZONE: f32 = 0.8;
/// Smoothing time constant of a following camera, seconds.
const FOLLOW_TAU: f32 = 0.15;

#[cfg(test)]
fn ticks(secs: f64) -> i64 {
    (secs * TPS).round() as i64
}

// ---- a uniformly sampled 2-D path ---------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Path2 {
    t0: i64,
    step: i64,
    pts: Vec<[f32; 2]>,
}

impl Path2 {
    fn at(&self, t: i64) -> Option<[f32; 2]> {
        let first = *self.pts.first()?;
        if t <= self.t0 {
            return Some(first);
        }
        let f = (t - self.t0) as f64 / self.step as f64;
        let i = f.floor() as usize;
        if i + 1 >= self.pts.len() {
            return self.pts.last().copied();
        }
        let k = (f - i as f64) as f32;
        let (a, b) = (self.pts[i], self.pts[i + 1]);
        Some([a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k])
    }
}

// ---- the cursor track ------------------------------------------------------------------------------

/// A cursor image in use: which PNG, where its hotspot is, and its density.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef {
    pub id: String,
    pub hotspot: [i32; 2],
    /// PNG px per logical point.
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pos {
    t: i64,
    x: f32,
    y: f32,
}

/// The telemetry of one recording, ready to be asked "where is the cursor at `t`?".
#[derive(Debug)]
pub struct CursorTrack {
    /// Size of the recorded area in px (the screen track's coordinate space).
    pub size: (f32, f32),
    /// Physical px per logical point of the recorded display (2.0 on Retina).
    pub target_scale: f64,
    samples: Vec<Pos>,
    clicks: Vec<Pos>,
    activity: Vec<i64>,
    images: Vec<(i64, ImageRef)>,
    paths: [OnceLock<Path2>; 3],
}

/// One click effect at an instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickFx {
    /// Where the click was, as fractions of the screen frame.
    pub pos: (f32, f32),
    /// 0 at the click, 1 when the effect has ended.
    pub progress: f32,
}

/// What to draw for the cursor at one instant.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorFrame {
    /// The hotspot, as fractions of the screen frame.
    pub pos: (f32, f32),
    pub image: Option<ImageRef>,
    /// 0 to 1: fades out when the cursor has been idle ("hide when idle for 2 s").
    pub opacity: f32,
    pub effects: Vec<ClickFx>,
}

impl CursorTrack {
    /// An empty track for a recorded area of `size` px.
    pub fn new(size: (f32, f32), target_scale: f64) -> CursorTrack {
        CursorTrack {
            size,
            target_scale: if target_scale > 0.0 {
                target_scale
            } else {
                1.0
            },
            samples: Vec::new(),
            clicks: Vec::new(),
            activity: Vec::new(),
            images: Vec::new(),
            paths: Default::default(),
        }
    }

    /// Adds a pointer position (events must be pushed in time order).
    pub fn push_move(&mut self, t: i64, x: f32, y: f32) {
        if self
            .samples
            .last()
            .is_none_or(|p| (p.x - x).abs() > f32::EPSILON || (p.y - y).abs() > f32::EPSILON)
        {
            self.activity.push(t);
        }
        self.samples.push(Pos { t, x, y });
    }

    pub fn push_click(&mut self, t: i64, x: f32, y: f32) {
        self.activity.push(t);
        self.clicks.push(Pos { t, x, y });
    }

    pub fn push_activity(&mut self, t: i64) {
        self.activity.push(t);
    }

    pub fn push_image(&mut self, t: i64, image: ImageRef) {
        self.images.push((t, image));
    }

    /// Builds the track from a parsed `cursor.jsonl`.
    pub fn from_log(log: &CursorLog) -> CursorTrack {
        let size = log.header.target.size;
        let mut tr = CursorTrack::new(
            (size.width as f32, size.height as f32),
            log.header.target.scale,
        );
        for e in &log.events {
            match e {
                CursorEvent::Move { t, x, y, .. } => tr.push_move(*t, *x as f32, *y as f32),
                CursorEvent::Down {
                    t, x, y, outside, ..
                } => {
                    tr.push_move(*t, *x as f32, *y as f32);
                    if !outside {
                        tr.push_click(*t, *x as f32, *y as f32);
                    }
                }
                CursorEvent::Up { t, x, y, .. } => tr.push_move(*t, *x as f32, *y as f32),
                CursorEvent::Scroll { t, .. } => tr.push_activity(*t),
                CursorEvent::Cursor {
                    t,
                    id,
                    hotspot,
                    scale,
                } => tr.push_image(
                    *t,
                    ImageRef {
                        id: id.clone(),
                        hotspot: *hotspot,
                        scale: *scale,
                    },
                ),
                CursorEvent::Unknown => {}
            }
        }
        tr
    }

    /// True when the telemetry holds no position at all.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    fn path(&self, smoothing: Smoothing) -> &Path2 {
        let i = match smoothing {
            Smoothing::Off => 0,
            Smoothing::Light => 1,
            Smoothing::Strong => 2,
        };
        self.paths[i].get_or_init(|| self.build_path(smoothing))
    }

    fn build_path(&self, smoothing: Smoothing) -> Path2 {
        let Some(first) = self.samples.first() else {
            return Path2::default();
        };
        let last_t = self.samples.last().map_or(first.t, |p| p.t);
        let n = ((last_t - first.t) / PATH_STEP) as usize + 2;
        let mut pts: Vec<[f32; 2]> = Vec::with_capacity(n);
        let mut seg = 0usize;
        for k in 0..n {
            let u = first.t + k as i64 * PATH_STEP;
            while seg + 1 < self.samples.len() && self.samples[seg + 1].t <= u {
                seg += 1;
            }
            let a = self.samples[seg];
            let p = match self.samples.get(seg + 1) {
                None => [a.x, a.y],
                Some(b) => {
                    let gap = b.t - a.t;
                    let from = if gap > HOLD_GAP {
                        b.t - GLIDE_LEAD
                    } else {
                        a.t
                    };
                    if u <= from || b.t <= from {
                        [a.x, a.y]
                    } else {
                        let k = ((u - from) as f32 / (b.t - from) as f32).clamp(0.0, 1.0);
                        [a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k]
                    }
                }
            };
            pts.push(p);
        }
        let tau = match smoothing {
            Smoothing::Off => 0.0,
            Smoothing::Light => 0.03,
            Smoothing::Strong => 0.09,
        };
        if tau > 0.0 && pts.len() > 1 {
            let a = 1.0 - (-(PATH_STEP as f64 / TPS) / tau).exp() as f32;
            // Forward, then backward: a zero-phase low-pass.
            let lerp = |from: [f32; 2], to: [f32; 2]| {
                [
                    from[0] + a * (to[0] - from[0]),
                    from[1] + a * (to[1] - from[1]),
                ]
            };
            for i in 1..pts.len() {
                pts[i] = lerp(pts[i - 1], pts[i]);
            }
            for i in (0..pts.len() - 1).rev() {
                pts[i] = lerp(pts[i + 1], pts[i]);
            }
        }
        Path2 {
            t0: first.t,
            step: PATH_STEP,
            pts,
        }
    }

    /// The pointer in px of the recorded area at session time `t` with this smoothing.
    pub fn position_at(&self, t: MediaTime, smoothing: Smoothing) -> Option<(f32, f32)> {
        self.path(smoothing).at(t.ticks()).map(|p| (p[0], p[1]))
    }

    fn fraction(&self, p: [f32; 2]) -> (f32, f32) {
        (p[0] / self.size.0.max(1.0), p[1] / self.size.1.max(1.0))
    }

    fn opacity_at(&self, t: i64, hide_idle: bool) -> f32 {
        if !hide_idle {
            return 1.0;
        }
        let hi = self.activity.partition_point(|a| *a <= t);
        let Some(last) = hi.checked_sub(1).map(|i| self.activity[i]) else {
            // Nothing has moved yet: show the cursor where it starts.
            return 1.0;
        };
        let idle = t - last;
        if idle <= IDLE_HIDE.ticks() {
            1.0
        } else {
            (1.0 - (idle - IDLE_HIDE.ticks()) as f32 / FADE_OUT as f32).clamp(0.0, 1.0)
        }
    }

    fn image_at(&self, t: i64) -> Option<ImageRef> {
        let hi = self.images.partition_point(|(it, _)| *it <= t);
        self.images
            .get(hi.saturating_sub(1))
            .map(|(_, image)| image.clone())
    }

    /// What to draw for the cursor at session time `t`, or `None` when it is hidden: outside the
    /// recorded area, faded out, or the track has no positions.
    pub fn frame_at(&self, t: MediaTime, style: &CursorStyle) -> Option<CursorFrame> {
        let path = self.path(style.smoothing);
        let p = path.at(t.ticks())?;
        let (w, h) = self.size;
        if p[0] < -2.0 || p[1] < -2.0 || p[0] > w + 2.0 || p[1] > h + 2.0 {
            return None;
        }
        let opacity = self.opacity_at(t.ticks(), style.hide_idle);
        if opacity <= 0.0 {
            return None;
        }
        Some(CursorFrame {
            pos: self.fraction(p),
            image: self.image_at(t.ticks()),
            opacity,
            effects: self.effects_at(t.ticks(), style),
        })
    }

    fn effects_at(&self, t: i64, style: &CursorStyle) -> Vec<ClickFx> {
        let dur = match style.click_effect {
            ClickEffect::None => return Vec::new(),
            ClickEffect::Ring => RING_TICKS,
            ClickEffect::Ripple => RIPPLE_TICKS,
        };
        let path = self.path(style.smoothing);
        let hi = self.clicks.partition_point(|c| c.t <= t);
        self.clicks[..hi]
            .iter()
            .rev()
            .take_while(|c| t - c.t < dur)
            .take(MAX_EFFECTS)
            .map(|c| {
                // The smoothed pointer at the click, so the effect starts under the drawn cursor.
                let at = path.at(c.t).unwrap_or([c.x, c.y]);
                ClickFx {
                    pos: self.fraction(at),
                    progress: (t - c.t) as f32 / dur as f32,
                }
            })
            .collect()
    }
}

// ---- sprites ----------------------------------------------------------------------------------------

/// One cursor picture, straight RGBA8, top-left origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub hotspot: [i32; 2],
    /// PNG px per logical point.
    pub scale: f64,
}

/// The cursor images of a bundle (`cursors/`), by id.
#[derive(Debug, Default)]
pub struct CursorSprites {
    map: HashMap<String, Sprite>,
}

impl CursorSprites {
    /// Reads `<bundle>/cursors/*.png`. A missing or damaged folder gives an empty set (the
    /// drawn cursor falls back to [`fallback_arrow`]); one bad image is skipped.
    pub fn load(bundle: &Path) -> CursorSprites {
        let dir = bundle.join(CURSORS_DIR);
        let Ok(index) = ImageIndex::read(&dir) else {
            return CursorSprites::default();
        };
        let mut map = HashMap::new();
        for (id, e) in &index.images {
            let Ok(bytes) = std::fs::read(dir.join(&e.file)) else {
                continue;
            };
            if let Ok(img) = CursorImage::from_png(&bytes, e.hotspot, e.scale) {
                map.insert(
                    id.clone(),
                    Sprite {
                        width: img.width,
                        height: img.height,
                        rgba: img.rgba,
                        hotspot: img.hotspot,
                        scale: img.scale,
                    },
                );
            }
        }
        CursorSprites { map }
    }

    pub fn get(&self, id: &str) -> Option<&Sprite> {
        self.map.get(id)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// A plain arrow (white with a dark outline) for recordings whose images are missing.
pub fn fallback_arrow(scale: f64) -> Sprite {
    // Arrow outline in a 12 x 19 logical grid, tip at the origin.
    const POLY: [(f32, f32); 7] = [
        (0.0, 0.0),
        (0.0, 16.0),
        (4.0, 12.5),
        (7.0, 18.5),
        (9.5, 17.5),
        (6.5, 11.5),
        (11.5, 11.5),
    ];
    let s = scale.max(1.0) as f32;
    let (w, h) = ((14.0 * s).ceil() as u32 + 2, (21.0 * s).ceil() as u32 + 2);
    let inside = |px: f32, py: f32, grow: f32| -> bool {
        // Point-in-polygon on the grown outline: test the polygon scaled about its centroid.
        let (cx, cy) = (5.0f32, 9.0f32);
        let mut c = false;
        let n = POLY.len();
        for i in 0..n {
            let a = POLY[i];
            let b = POLY[(i + 1) % n];
            let ga = (cx + (a.0 - cx) * grow, cy + (a.1 - cy) * grow);
            let gb = (cx + (b.0 - cx) * grow, cy + (b.1 - cy) * grow);
            if (ga.1 > py) != (gb.1 > py) && px < (gb.0 - ga.0) * (py - ga.1) / (gb.1 - ga.1) + ga.0
            {
                c = !c;
            }
        }
        c
    };
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (mut fill, mut edge) = (0u32, 0u32);
            for sy in 0..3 {
                for sx in 0..3 {
                    let px = ((x as f32 + (sx as f32 + 0.5) / 3.0) / s) - 0.5;
                    let py = ((y as f32 + (sy as f32 + 0.5) / 3.0) / s) - 0.5;
                    if inside(px, py, 1.0) {
                        fill += 1;
                    } else if inside(px, py, 1.18) {
                        edge += 1;
                    }
                }
            }
            let i = ((y * w + x) * 4) as usize;
            let a_fill = fill as f32 / 9.0;
            let a_edge = edge as f32 / 9.0;
            let alpha = a_fill + a_edge;
            if alpha > 0.0 {
                let v = (255.0 * a_fill / alpha) as u8;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, (alpha * 255.0) as u8]);
            }
        }
    }
    Sprite {
        width: w,
        height: h,
        rgba,
        hotspot: [(s * 0.5) as i32, (s * 0.5) as i32],
        scale,
    }
}

// ---- the camera ---------------------------------------------------------------------------------------

/// Everything the camera needs to know.
#[derive(Debug, Clone, Copy)]
pub struct CameraInput<'a> {
    /// The screen frame in px; only its aspect matters.
    pub src_size: (f64, f64),
    /// The output aspect (width over height) when the export reframes; `None` shows the whole
    /// frame. Ignored unless `reframe` crops.
    pub out_aspect: Option<f64>,
    pub reframe: Option<&'a Reframe>,
    pub zooms: &'a [ZoomRegion],
    pub track: Option<&'a CursorTrack>,
    /// The kept part of the recording (session time).
    pub span: (MediaTime, MediaTime),
}

/// The visible part of the screen frame over time. Built once, queried per frame.
#[derive(Debug)]
pub struct Camera {
    /// The reframe window size as fractions of the frame (`1, 1` without a reframe).
    base: (f32, f32),
    reframe: Option<Reframe>,
    zooms: Vec<ZoomRegion>,
    follow_base: Option<Path2>,
    follow_zoom: Option<Path2>,
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// How far zoom `z` is in at session time `t`: `(0..1 ease weight, region)`.
fn zoom_state(zooms: &[ZoomRegion], t: i64) -> Option<(f32, &ZoomRegion)> {
    let z = zooms
        .iter()
        .find(|z| t >= z.at.ticks() && t < z.end().ticks())?;
    let len = z.length.ticks();
    let ease = ZOOM_EASE.ticks().min(len / 2).max(1);
    let u = t - z.at.ticks();
    let w = if u < ease {
        smooth(u as f32 / ease as f32)
    } else if len - u < ease {
        smooth((len - u) as f32 / ease as f32)
    } else {
        1.0
    };
    Some((w, z))
}

fn zoom_factor(zooms: &[ZoomRegion], t: i64) -> f32 {
    zoom_state(zooms, t).map_or(1.0, |(w, z)| 1.0 + (z.scale as f32 - 1.0) * w)
}

/// A camera center that follows `target`, moving only when the target leaves the central
/// `DEAD_ZONE` of the window, and never lets it out of the window.
fn simulate_follow(
    span: (i64, i64),
    target: impl Fn(i64) -> Option<(f32, f32)>,
    window: impl Fn(i64) -> (f32, f32),
) -> Path2 {
    let n = ((span.1 - span.0).max(0) / CAMERA_STEP) as usize + 2;
    let a = 1.0 - (-(CAMERA_STEP as f64 / TPS) as f32 / FOLLOW_TAU).exp();
    let mut pts = Vec::with_capacity(n);
    let mut c = target(span.0).map_or([0.5, 0.5], |p| [p.0, p.1]);
    for k in 0..n {
        let t = span.0 + k as i64 * CAMERA_STEP;
        let (ww, wh) = window(t);
        if let Some(p) = target(t) {
            let half = [ww * DEAD_ZONE / 2.0, wh * DEAD_ZONE / 2.0];
            let p = [p.0, p.1];
            let mut want = c;
            for i in 0..2 {
                if p[i] > c[i] + half[i] {
                    want[i] = p[i] - half[i];
                } else if p[i] < c[i] - half[i] {
                    want[i] = p[i] + half[i];
                }
                c[i] += (want[i] - c[i]) * a;
                // The smoothing may lag a fast pointer; it must not lose it.
                let reach = [ww, wh][i] * 0.48;
                c[i] = c[i].clamp(p[i] - reach, p[i] + reach);
            }
        }
        pts.push(c);
    }
    Path2 {
        t0: span.0,
        step: CAMERA_STEP,
        pts,
    }
}

impl Camera {
    pub fn new(input: &CameraInput<'_>) -> Camera {
        let reframe = input
            .reframe
            .filter(|r| r.mode != ReframeMode::Fit && input.out_aspect.is_some())
            .cloned();
        let base = match (&reframe, input.out_aspect) {
            (Some(_), Some(out)) => {
                let src = input.src_size.0 / input.src_size.1.max(1.0);
                if out < src {
                    ((out / src) as f32, 1.0)
                } else {
                    (1.0, (src / out) as f32)
                }
            }
            _ => (1.0, 1.0),
        };
        let span = (input.span.0.ticks(), input.span.1.ticks());
        let cursor = |t: i64| -> Option<(f32, f32)> {
            let tr = input.track?;
            let p = tr.path(Smoothing::Light).at(t)?;
            Some(tr.fraction(p))
        };
        let zooms: Vec<ZoomRegion> = input.zooms.to_vec();
        let follow_base = reframe
            .as_ref()
            .filter(|r| r.mode == ReframeMode::Follow && input.track.is_some_and(|t| !t.is_empty()))
            .map(|_| simulate_follow(span, cursor, |_| base));
        let follow_zoom = (zooms.iter().any(|z| z.follow_cursor)
            && input.track.is_some_and(|t| !t.is_empty()))
        .then(|| {
            simulate_follow(span, cursor, |t| {
                let z = zoom_factor(&zooms, t);
                (base.0 / z, base.1 / z)
            })
        });
        Camera {
            base,
            reframe,
            zooms,
            follow_base,
            follow_zoom,
        }
    }

    /// True when the picture is the whole frame all the time.
    pub fn is_identity(&self) -> bool {
        self.reframe.is_none() && self.zooms.is_empty()
    }

    /// True when the camera crops to the output aspect (EDT-5).
    pub fn reframes(&self) -> bool {
        self.reframe.is_some()
    }

    /// The visible rectangle `[x, y, w, h]` of the screen frame, as fractions, at session time `t`.
    pub fn crop_at(&self, t: MediaTime) -> [f32; 4] {
        let t = t.ticks();
        let (w0, h0) = self.base;
        let center =
            |c: [f32; 2], w: f32, h: f32, around: [f32; 2], aw: f32, ah: f32| -> [f32; 2] {
                // Keeps a w x h window centered at c inside the aw x ah window centered at `around`.
                let rx = (aw - w) / 2.0;
                let ry = (ah - h) / 2.0;
                [
                    c[0].clamp(around[0] - rx, around[0] + rx),
                    c[1].clamp(around[1] - ry, around[1] + ry),
                ]
            };
        let c0 = match &self.reframe {
            None => [0.5, 0.5],
            Some(r) => {
                let f = self
                    .follow_base
                    .as_ref()
                    .and_then(|p| p.at(t))
                    .unwrap_or_else(|| {
                        let f = r.focus_at(MediaTime::from_ticks(t));
                        [f.0 as f32, f.1 as f32]
                    });
                center(f, w0, h0, [0.5, 0.5], 1.0, 1.0)
            }
        };
        let Some((w, z)) = zoom_state(&self.zooms, t) else {
            return [c0[0] - w0 / 2.0, c0[1] - h0 / 2.0, w0, h0];
        };
        let factor = 1.0 + (z.scale as f32 - 1.0) * w;
        let (wz, hz) = (w0 / factor, h0 / factor);
        let target = if z.follow_cursor {
            self.follow_zoom
                .as_ref()
                .and_then(|p| p.at(t))
                .unwrap_or([z.focus.0 as f32, z.focus.1 as f32])
        } else {
            [z.focus.0 as f32, z.focus.1 as f32]
        };
        let c = [
            c0[0] + (target[0] - c0[0]) * w,
            c0[1] + (target[1] - c0[1]) * w,
        ];
        let c = center(c, wz, hz, c0, w0, h0);
        [c[0] - wz / 2.0, c[1] - hz / 2.0, wz, hz]
    }
}

/// Loads the cursor track and images of a bundle. `Ok(None)` when it has no cursor file.
pub fn load_cursor(bundle: &Path) -> Result<Option<(CursorTrack, CursorSprites)>, CmError> {
    let file = bundle.join(crate::schema::CURSOR_FILE);
    if !file.is_file() {
        return Ok(None);
    }
    let log = CursorLog::read(&file)?;
    Ok(Some((
        CursorTrack::from_log(&log),
        CursorSprites::load(bundle),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{Reframe, ReframeWindow};

    fn t(secs: f64) -> MediaTime {
        MediaTime::from_ticks(ticks(secs))
    }

    fn track(points: &[(f64, f32, f32)]) -> CursorTrack {
        let mut tr = CursorTrack::new((1000.0, 500.0), 2.0);
        for &(s, x, y) in points {
            tr.push_move(ticks(s), x, y);
        }
        tr
    }

    fn zoom(at: f64, len: f64, follow: bool) -> ZoomRegion {
        ZoomRegion {
            at: t(at),
            length: t(len),
            focus: (0.25, 0.25),
            scale: 2.0,
            follow_cursor: follow,
        }
    }

    fn cam<'a>(
        zooms: &'a [ZoomRegion],
        reframe: Option<&'a Reframe>,
        out: Option<f64>,
        tr: Option<&'a CursorTrack>,
    ) -> Camera {
        Camera::new(&CameraInput {
            src_size: (1920.0, 1080.0),
            out_aspect: out,
            reframe,
            zooms,
            track: tr,
            span: (t(0.0), t(60.0)),
        })
    }

    #[test]
    fn edt4_the_cursor_glides_between_events_and_holds_when_still() {
        let tr = track(&[
            (0.0, 0.0, 0.0),
            (0.01, 10.0, 0.0),
            (2.0, 10.0, 0.0),
            (2.02, 110.0, 0.0),
        ]);
        let at = |s| tr.position_at(t(s), Smoothing::Off).unwrap().0;
        assert!((at(0.005) - 5.0).abs() < 0.2, "linear between close events");
        assert!((at(1.0) - 10.0).abs() < 1e-3, "held while still");
        assert!((at(2.03) - 110.0).abs() < 0.5, "arrived within a grid step");
        assert!((at(5.0) - 110.0).abs() < 1e-3, "holds after the last event");
    }

    #[test]
    fn edt4_smoothing_does_not_delay_the_pointer() {
        // A pointer that jumps at t = 1 s: zero-phase smoothing is centered on the jump.
        let tr = track(&[
            (0.0, 0.0, 0.0),
            (0.99, 0.0, 0.0),
            (1.0, 100.0, 0.0),
            (3.0, 100.0, 0.0),
        ]);
        for s in [Smoothing::Light, Smoothing::Strong] {
            let mid = tr.position_at(t(0.995), s).unwrap().0;
            assert!(
                (mid - 50.0).abs() < 8.0,
                "{s:?}: half way at the jump, got {mid}"
            );
            let before = tr.position_at(t(0.5), Smoothing::Off).unwrap().0;
            assert!(before.abs() < 1e-3);
        }
        // And it is smoother than the raw path: no longer a step.
        let raw = tr.position_at(t(0.985), Smoothing::Off).unwrap().0;
        let soft = tr.position_at(t(0.985), Smoothing::Strong).unwrap().0;
        assert!(
            soft > raw + 1.0,
            "the strong path starts moving before the raw one"
        );
    }

    #[test]
    fn edt4_ac1_the_cursor_frame_has_the_telemetry_position_as_fractions() {
        let tr = track(&[(0.0, 500.0, 250.0), (1.0, 500.0, 250.0)]);
        let style = CursorStyle::default();
        let f = tr.frame_at(t(0.5), &style).unwrap();
        assert_eq!(f.pos, (0.5, 0.5));
        assert_eq!(f.opacity, 1.0);
    }

    #[test]
    fn edt4_hide_when_idle_fades_after_two_seconds() {
        let mut tr = track(&[(0.0, 10.0, 10.0), (1.0, 20.0, 20.0), (8.0, 20.0, 20.0)]);
        tr.push_activity(ticks(8.0));
        let style = CursorStyle {
            hide_idle: true,
            ..Default::default()
        };
        let op = |s| tr.frame_at(t(s), &style).map_or(0.0, |f| f.opacity);
        assert_eq!(op(2.5), 1.0, "1.5 s after the last move");
        assert_eq!(op(3.0), 1.0, "exactly 2 s");
        assert!(op(3.1) < 1.0 && op(3.1) > 0.0, "fading");
        assert_eq!(op(4.0), 0.0, "hidden");
        assert_eq!(op(8.5), 1.0, "back after movement");
        let always = CursorStyle::default();
        assert!(tr.frame_at(t(4.0), &always).is_some());
    }

    #[test]
    fn edt4_a_pointer_outside_the_recorded_area_is_hidden() {
        let tr = track(&[(0.0, -300.0, 20.0), (1.0, -300.0, 20.0)]);
        assert!(tr.frame_at(t(0.5), &CursorStyle::default()).is_none());
    }

    #[test]
    fn edt4_click_effects_run_their_course_and_stop() {
        let mut tr = track(&[(0.0, 100.0, 100.0), (5.0, 100.0, 100.0)]);
        tr.push_click(ticks(1.0), 100.0, 100.0);
        tr.push_click(ticks(1.2), 100.0, 100.0);
        let fx = |style: &CursorStyle, s| tr.frame_at(t(s), style).unwrap().effects;
        let ring = CursorStyle {
            click_effect: ClickEffect::Ring,
            ..Default::default()
        };
        let ripple = CursorStyle {
            click_effect: ClickEffect::Ripple,
            ..Default::default()
        };
        assert!(fx(&CursorStyle::default(), 1.1).is_empty());
        assert_eq!(fx(&ring, 0.9).len(), 0);
        let e = fx(&ring, 1.25);
        assert_eq!(e.len(), 2, "two clicks overlap");
        assert!((e[0].progress - 0.05 / 0.45).abs() < 0.01, "newest first");
        assert_eq!(fx(&ring, 1.6).len(), 1, "the ring lasts 450 ms");
        assert_eq!(fx(&ripple, 1.6).len(), 2, "the ripple lasts 700 ms");
        assert_eq!(fx(&ripple, 2.0).len(), 0);
    }

    #[test]
    fn edt4_the_cursor_image_follows_the_cursor_events() {
        let mut tr = track(&[(0.0, 1.0, 1.0)]);
        let img = |id: &str| ImageRef {
            id: id.into(),
            hotspot: [1, 1],
            scale: 2.0,
        };
        tr.push_image(ticks(0.0), img("a"));
        tr.push_image(ticks(2.0), img("b"));
        let style = CursorStyle::default();
        assert_eq!(tr.frame_at(t(1.0), &style).unwrap().image.unwrap().id, "a");
        assert_eq!(tr.frame_at(t(3.0), &style).unwrap().image.unwrap().id, "b");
    }

    #[test]
    fn edt4_ac2_a_zoom_eases_in_for_400_ms_holds_and_eases_out() {
        let zs = [zoom(15.0, 3.0, false)];
        let c = cam(&zs, None, None, None);
        let w = |s| c.crop_at(t(s))[2];
        assert_eq!(w(14.9), 1.0);
        assert!(w(15.2) < 1.0 && w(15.2) > 0.5, "half way in the ease");
        for s in [15.4, 16.0, 17.0, 17.6] {
            assert!(
                (w(s) - 0.5).abs() < 1e-4,
                "{s}: fully zoomed at 2x, got {}",
                w(s)
            );
        }
        assert!(w(17.8) < 1.0 && w(17.8) > 0.5, "easing out");
        assert_eq!(w(18.0), 1.0);
        // The focus area is in the middle of the view at full zoom (clamped inside the frame).
        let r = c.crop_at(t(16.0));
        assert!(
            (r[0] - 0.0).abs() < 1e-4 && (r[1] - 0.0).abs() < 1e-4,
            "focus 0.25 with 2x: {r:?}"
        );
    }

    #[test]
    fn edt4_ac3_a_following_zoom_only_pans_when_the_cursor_leaves_the_middle_80_percent() {
        // The pointer sits still at x = 0.5, then walks right.
        let mut pts = vec![(0.0, 500.0, 250.0), (4.0, 500.0, 250.0)];
        for i in 1..=40 {
            pts.push((4.0 + i as f64 * 0.1, 500.0 + 10.0 * i as f32, 250.0));
        }
        let tr = track(&pts);
        let zs = [ZoomRegion {
            at: t(1.0),
            length: t(20.0),
            focus: (0.5, 0.5),
            scale: 2.0,
            follow_cursor: true,
        }];
        let c = cam(&zs, None, None, Some(&tr));
        let still = c.crop_at(t(2.0));
        let still2 = c.crop_at(t(3.5));
        assert!(
            (still[0] - still2[0]).abs() < 1e-3,
            "no pan while the cursor is still"
        );
        let mut outside = 0;
        let mut frames = 0;
        let mut s = 2.0;
        while s < 8.0 {
            let r = c.crop_at(t(s));
            let (px, _) = tr.position_at(t(s), Smoothing::Light).unwrap();
            let fx = px / 1000.0;
            frames += 1;
            if fx < r[0] || fx > r[0] + r[2] {
                outside += 1;
            }
            s += 0.02;
        }
        assert_eq!(
            outside, 0,
            "the cursor stays inside the visible frame ({frames} frames)"
        );
        let late = c.crop_at(t(8.0));
        assert!(
            late[0] > still[0] + 0.1,
            "the view panned once the cursor walked away"
        );
    }

    #[test]
    fn edt5_a_fixed_reframe_is_the_largest_window_of_the_output_aspect() {
        let r = Reframe {
            mode: ReframeMode::Fixed,
            windows: vec![
                ReframeWindow {
                    from: t(0.0),
                    focus: (0.0, 0.5),
                },
                ReframeWindow {
                    from: t(30.0),
                    focus: (1.0, 0.5),
                },
            ],
        };
        let c = cam(&[], Some(&r), Some(9.0 / 16.0), None);
        let a = c.crop_at(t(10.0));
        // 9:16 out of 16:9: full height, width = (9/16) / (16/9).
        assert!((a[3] - 1.0).abs() < 1e-6);
        assert!((a[2] - 0.31640625).abs() < 1e-4);
        assert!(a[0].abs() < 1e-6, "focus 0 is clamped to the left edge");
        let b = c.crop_at(t(40.0));
        assert!(
            (b[0] + b[2] - 1.0).abs() < 1e-6,
            "the second window sits at the right edge"
        );
        // The crop has the output aspect in pixels.
        let aspect = (a[2] as f64 * 1920.0) / (a[3] as f64 * 1080.0);
        assert!((aspect - 9.0 / 16.0).abs() < 1e-4);
        // 1:1 is cropped the same way.
        let sq = cam(&[], Some(&r), Some(1.0), None).crop_at(t(10.0));
        assert!(((sq[2] as f64 * 1920.0) / (sq[3] as f64 * 1080.0) - 1.0).abs() < 1e-4);
        // Fit and no aspect show everything.
        let fit = Reframe::default();
        assert!(cam(&[], Some(&fit), Some(9.0 / 16.0), None).is_identity());
        assert!(cam(&[], Some(&r), None, None).is_identity());
    }

    #[test]
    fn edt5_ac2_a_following_reframe_keeps_the_cursor_in_the_frame() {
        // The pointer wanders left and right across the whole screen at speed.
        let mut pts = Vec::new();
        for i in 0..600 {
            let s = i as f64 * 0.1;
            let x = 500.0 + 450.0 * ((s * 0.9).sin() as f32);
            let y = 250.0 + 200.0 * ((s * 0.4).cos() as f32);
            pts.push((s, x, y));
        }
        let tr = track(&pts);
        let r = Reframe {
            mode: ReframeMode::Follow,
            windows: vec![],
        };
        let c = cam(&[], Some(&r), Some(9.0 / 16.0), Some(&tr));
        let (mut inside, mut total) = (0, 0);
        let mut s = 0.5;
        while s < 59.0 {
            let rect = c.crop_at(t(s));
            let (px, _) = tr.position_at(t(s), Smoothing::Light).unwrap();
            let fx = px / 1000.0;
            total += 1;
            if fx >= rect[0] && fx <= rect[0] + rect[2] {
                inside += 1;
            }
            s += 1.0 / 30.0;
        }
        assert!(
            inside as f64 / total as f64 >= 0.99,
            "{inside} of {total} frames have the cursor in view"
        );
    }

    #[test]
    fn a_zoom_inside_a_reframe_stays_inside_the_reframe_window() {
        let r = Reframe {
            mode: ReframeMode::Fixed,
            windows: vec![ReframeWindow {
                from: t(0.0),
                focus: (0.5, 0.5),
            }],
        };
        let zs = [ZoomRegion {
            at: t(5.0),
            length: t(4.0),
            focus: (0.0, 0.0),
            scale: 2.0,
            follow_cursor: false,
        }];
        let c = cam(&zs, Some(&r), Some(9.0 / 16.0), None);
        let base = c.crop_at(t(1.0));
        let z = c.crop_at(t(7.0));
        assert!((z[2] - base[2] / 2.0).abs() < 1e-5);
        assert!(z[0] >= base[0] - 1e-6 && z[0] + z[2] <= base[0] + base[2] + 1e-6);
    }

    #[test]
    fn the_fallback_arrow_has_a_visible_tip_and_transparent_corners() {
        let s = fallback_arrow(2.0);
        let px = |x: u32, y: u32| s.rgba[((y * s.width + x) * 4 + 3) as usize];
        assert!(px(2, 3) > 200, "near the tip");
        assert_eq!(px(s.width - 1, 0), 0, "top right corner is empty");
        assert_eq!(s.rgba.len(), (s.width * s.height * 4) as usize);
    }
}
