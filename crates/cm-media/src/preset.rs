//! Export presets (EXP-2 platform presets, EXP-3 GIF and WebM): the table, and how a preset plus
//! the recording turns into one concrete [`Target`] (frame, fps, bitrate, loudness, limits).
//!
//! Pure arithmetic and parsing, tested without FFmpeg or a GPU. The table is data
//! (`data/presets.toml`, embedded); [`Presets::load_default`] lets a file named by
//! `CAPTUREMANN_PLATFORM_RULES` replace it without rebuilding the app (EXP-2.AC4).
//!
//! # Reframe hook (EDT-5)
//!
//! A preset frame that has another aspect than the recording is filled by *fitting* the screen by
//! width (or height) and centering it on the background ([`Reframe::Fit`]). EDT-5 will add a crop
//! mode ([`Reframe::Crop`]) that takes priority when the edit list sets one; the exporter reads
//! `Target::reframe` and nothing else about it.

use std::path::{Path, PathBuf};

use cm_core::CmError;

use crate::layout::{bubble_geom, BubbleGeom, CameraLayout};

/// The shipped table.
pub const BUILTIN: &str = include_str!("../data/presets.toml");
/// The shape version this build reads.
pub const TABLE_VERSION: i64 = 1;
/// Environment variable naming a replacement table.
pub const RULES_ENV: &str = "CAPTUREMANN_PLATFORM_RULES";
/// The largest side of an H.264 frame (level 5.2), also the cap for `frame = "master"`.
const MAX_SIDE: u32 = 4096;
/// Pixels kept between the bubble and the frame edge when it is clamped into the frame.
const SAFE_BUBBLE_GAP_PX: u32 = 0;

/// The output container and codec family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// H.264 High + AAC in MP4.
    Mp4,
    /// A looping GIF (no audio).
    Gif,
    /// VP9 + Opus in WebM.
    WebM,
}

impl Kind {
    /// The file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Mp4 => "mp4",
            Kind::Gif => "gif",
            Kind::WebM => "webm",
        }
    }
}

/// How the output frame is chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameDef {
    /// The recording's size (capped for H.264).
    Master,
    /// A fixed frame.
    Fixed(u32, u32),
}

/// One row of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetDef {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    pub frame: FrameDef,
    /// `None` = the recording's fps.
    pub fps: Option<f64>,
    pub mbps_30: Option<f64>,
    pub mbps_60: Option<f64>,
    pub loudness_lufs: Option<f64>,
    pub refuse_upscale: bool,
    pub safe_bottom_pct: f64,
    pub safe_right_pct: f64,
    /// GIF and WebM: default width.
    pub width: Option<u32>,
    /// GIF: the allowed widths (empty = any even width).
    pub widths: Vec<u32>,
    /// GIF: the allowed frame rates (empty = 1 to 60).
    pub fps_choices: Vec<f64>,
    pub max_seconds: Option<f64>,
    pub warn_seconds: Option<f64>,
}

/// The table.
#[derive(Debug, Clone, PartialEq)]
pub struct Presets {
    pub version: i64,
    pub defs: Vec<PresetDef>,
}

/// A table and where it came from.
#[derive(Debug, Clone)]
pub struct LoadedPresets {
    pub presets: Presets,
    /// The replacement file in use, `None` for the shipped table.
    pub source: Option<PathBuf>,
    /// Why a replacement file was ignored, for a log or a note.
    pub warning: Option<String>,
}

fn num(v: &toml::Value) -> Option<f64> {
    v.as_float().or_else(|| v.as_integer().map(|i| i as f64))
}

fn bad(id: &str, what: &str) -> CmError {
    CmError::invalid_input(format!("preset table: `{id}`: {what}"))
}

fn u32_of(v: &toml::Value) -> Option<u32> {
    v.as_integer().and_then(|i| u32::try_from(i).ok())
}

fn parse_def(t: &toml::Table) -> Result<PresetDef, CmError> {
    let id = t
        .get("id")
        .and_then(toml::Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .ok_or_else(|| CmError::invalid_input("preset table: every preset needs a lowercase `id`"))?
        .to_owned();
    let label = t
        .get("label")
        .and_then(toml::Value::as_str)
        .unwrap_or(&id)
        .to_owned();
    let kind = match t.get("kind").and_then(toml::Value::as_str) {
        Some("mp4") | None => Kind::Mp4,
        Some("gif") => Kind::Gif,
        Some("webm") => Kind::WebM,
        Some(other) => return Err(bad(&id, &format!("unknown kind `{other}`"))),
    };
    let frame = match t.get("frame") {
        None => FrameDef::Master,
        Some(toml::Value::String(s)) if s == "master" => FrameDef::Master,
        Some(toml::Value::Array(a)) if a.len() == 2 => {
            let (w, h) = (u32_of(&a[0]), u32_of(&a[1]));
            match (w, h) {
                (Some(w), Some(h))
                    if (16..=MAX_SIDE.max(7680)).contains(&w)
                        && (16..=MAX_SIDE.max(7680)).contains(&h)
                        && w.is_multiple_of(2)
                        && h.is_multiple_of(2) =>
                {
                    FrameDef::Fixed(w, h)
                }
                _ => return Err(bad(&id, "`frame` must be two even sizes from 16 to 7680")),
            }
        }
        Some(_) => return Err(bad(&id, "`frame` must be [w, h] or \"master\"")),
    };
    let fps = match t.get("fps") {
        None => None,
        Some(toml::Value::String(s)) if s == "source" => None,
        Some(v) => Some(
            num(v)
                .filter(|f| (1.0..=240.0).contains(f))
                .ok_or_else(|| bad(&id, "`fps` must be \"source\" or a number from 1 to 240"))?,
        ),
    };
    let opt_num = |key: &str, lo: f64, hi: f64| -> Result<Option<f64>, CmError> {
        match t.get(key) {
            None => Ok(None),
            Some(v) => num(v)
                .filter(|n| n.is_finite() && (lo..=hi).contains(n))
                .map(Some)
                .ok_or_else(|| bad(&id, &format!("`{key}` must be a number from {lo} to {hi}"))),
        }
    };
    let list_u32 = |key: &str| -> Result<Vec<u32>, CmError> {
        match t.get(key) {
            None => Ok(Vec::new()),
            Some(toml::Value::Array(a)) => a
                .iter()
                .map(|v| u32_of(v).filter(|n| *n >= 16 && n.is_multiple_of(2)))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| bad(&id, &format!("`{key}` must list even sizes"))),
            Some(_) => Err(bad(&id, &format!("`{key}` must be a list"))),
        }
    };
    let fps_choices = match t.get("fps_choices") {
        None => Vec::new(),
        Some(toml::Value::Array(a)) => a
            .iter()
            .map(|v| num(v).filter(|f| (1.0..=60.0).contains(f)))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| bad(&id, "`fps_choices` must list numbers from 1 to 60"))?,
        Some(_) => return Err(bad(&id, "`fps_choices` must be a list")),
    };
    let width = match t.get("width") {
        None => None,
        Some(v) => Some(
            u32_of(v)
                .filter(|n| (16..=7680).contains(n) && n.is_multiple_of(2))
                .ok_or_else(|| bad(&id, "`width` must be an even size from 16 to 7680"))?,
        ),
    };
    let def = PresetDef {
        id: id.clone(),
        label,
        kind,
        frame,
        fps,
        mbps_30: opt_num("mbps_30", 0.1, 500.0)?,
        mbps_60: opt_num("mbps_60", 0.1, 500.0)?,
        loudness_lufs: opt_num("loudness_lufs", -40.0, -5.0)?,
        refuse_upscale: t
            .get("refuse_upscale")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        safe_bottom_pct: opt_num("safe_bottom_pct", 0.0, 50.0)?.unwrap_or(0.0),
        safe_right_pct: opt_num("safe_right_pct", 0.0, 50.0)?.unwrap_or(0.0),
        width,
        widths: list_u32("widths")?,
        fps_choices,
        max_seconds: opt_num("max_seconds", 1.0, 36_000.0)?,
        warn_seconds: opt_num("warn_seconds", 1.0, 36_000.0)?,
    };
    if def.kind == Kind::Mp4 && def.mbps_30.is_none() && def.mbps_60.is_some() {
        return Err(bad(&id, "`mbps_60` needs `mbps_30`"));
    }
    Ok(def)
}

impl Presets {
    /// The table shipped in the binary.
    pub fn builtin() -> Presets {
        // The embedded table is checked by a unit test; failing here is a build defect.
        Self::from_toml(BUILTIN).unwrap_or(Presets {
            version: TABLE_VERSION,
            defs: Vec::new(),
        })
    }

    /// Parses a table. Unknown keys are ignored; a newer `version` is refused.
    pub fn from_toml(text: &str) -> Result<Presets, CmError> {
        let table: toml::Table = text
            .parse()
            .map_err(|e| CmError::invalid_input(format!("preset table: {e}")))?;
        let version = table
            .get("version")
            .and_then(toml::Value::as_integer)
            .unwrap_or(TABLE_VERSION);
        if version != TABLE_VERSION {
            return Err(CmError::invalid_input(format!(
                "preset table: version {version} is not supported (this build reads version {TABLE_VERSION})"
            )));
        }
        let rows = table
            .get("preset")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| CmError::invalid_input("preset table: no [[preset]] rows"))?;
        let mut defs: Vec<PresetDef> = Vec::new();
        for row in rows {
            let t = row
                .as_table()
                .ok_or_else(|| CmError::invalid_input("preset table: [[preset]] must be tables"))?;
            let def = parse_def(t)?;
            if defs.iter().any(|d| d.id == def.id) {
                return Err(bad(&def.id, "the id appears twice"));
            }
            defs.push(def);
        }
        Ok(Presets { version, defs })
    }

    /// The shipped table, or the file named by `CAPTUREMANN_PLATFORM_RULES` when it parses
    /// (EXP-2.AC4). A broken file is ignored with a warning, never an error: an export must not
    /// fail because of a bad rules file.
    pub fn load_default() -> LoadedPresets {
        match std::env::var_os(RULES_ENV) {
            Some(p) if !p.is_empty() => Self::load_file(Path::new(&p)),
            _ => LoadedPresets {
                presets: Self::builtin(),
                source: None,
                warning: None,
            },
        }
    }

    /// Reads a replacement table from `path`.
    pub fn load_file(path: &Path) -> LoadedPresets {
        let parsed = std::fs::read_to_string(path)
            .map_err(|e| CmError::invalid_input(format!("{}: {e}", path.display())))
            .and_then(|t| Self::from_toml(&t));
        match parsed {
            Ok(presets) => LoadedPresets {
                presets,
                source: Some(path.to_owned()),
                warning: None,
            },
            Err(e) => LoadedPresets {
                presets: Self::builtin(),
                source: None,
                warning: Some(format!(
                    "ignored the preset table `{}`: {e}",
                    path.display()
                )),
            },
        }
    }

    pub fn get(&self, id: &str) -> Option<&PresetDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// The preset ids, in table order.
    pub fn ids(&self) -> Vec<&str> {
        self.defs.iter().map(|d| d.id.as_str()).collect()
    }
}

/// The request's own choices for GIF and WebM (EXP-3). All `None` for MP4 presets.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ExportOptions {
    /// Output width in px.
    pub width: Option<u32>,
    pub fps: Option<f64>,
    /// "Fit under N MB".
    pub max_mb: Option<f64>,
}

/// How the recording fills a frame of another aspect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reframe {
    /// Fit by width (or height) and center on the background.
    Fit,
    /// EDT-5: show this part of the recording (`[x, y, w, h]`, fractions) and fill the frame.
    /// Not produced yet.
    Crop([f64; 4]),
}

/// Platform UI zones the bubble stays out of, as fractions of the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafeArea {
    pub bottom: f64,
    pub right: f64,
}

/// A preset made concrete for one recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    /// The output frame; `None` = the recording's size (capped for H.264).
    pub frame: Option<(u32, u32)>,
    /// The output fps; `None` = the recording's.
    pub fps: Option<f64>,
    /// The video bitrate in bit/s; `None` = the encoder default for the size.
    pub video_bps: Option<u64>,
    pub loudness_lufs: Option<f64>,
    pub safe: Option<SafeArea>,
    pub reframe: Reframe,
    /// GIF and WebM "Fit under N MB", in bytes.
    pub max_bytes: Option<u64>,
    pub max_seconds: Option<f64>,
    pub warn_seconds: Option<f64>,
    /// GIF: the allowed widths, used to step down when the size limit needs it.
    pub width_ladder: Vec<u32>,
    pub notes: Vec<String>,
}

impl Target {
    /// Frames of an output of `duration_s` at this target's fps given the recording's `fps`.
    pub fn fps_for(&self, source_fps: f64) -> f64 {
        self.fps.unwrap_or(source_fps)
    }
}

fn even_up(x: f64) -> u32 {
    let n = x.round().max(2.0) as u32;
    n + n % 2
}

/// The preset to suggest for a `master`-sized recording that a preset would enlarge: the largest
/// landscape MP4 preset that does not enlarge it, else the one that keeps the recording's size.
fn largest_without_upscale(presets: &Presets, master: (u32, u32)) -> Option<&PresetDef> {
    presets
        .defs
        .iter()
        .filter(|d| d.kind == Kind::Mp4)
        .filter_map(|d| match d.frame {
            FrameDef::Fixed(w, h) if w > h && fit_scale((w, h), master) <= 1.0 + 1e-9 => {
                Some((u64::from(w) * u64::from(h), d))
            }
            _ => None,
        })
        .max_by_key(|(area, _)| *area)
        .map(|(_, d)| d)
        .or_else(|| presets.defs.iter().find(|d| d.frame == FrameDef::Master))
}

/// How much the recording is scaled when it is fitted into `frame` (above 1 = enlarged).
pub fn fit_scale(frame: (u32, u32), master: (u32, u32)) -> f64 {
    (f64::from(frame.0) / f64::from(master.0.max(1)))
        .min(f64::from(frame.1) / f64::from(master.1.max(1)))
}

/// Makes preset `id` concrete for a recording of `master` pixels at `master_fps`.
///
/// Refuses (`invalid_input`) an unknown id, options that do not belong to the preset, a width or
/// fps that is not allowed, and an upscale the preset forbids (EXP-2.AC3, with a hint naming the
/// largest preset that fits).
pub fn resolve(
    presets: &Presets,
    id: &str,
    master: (u32, u32),
    master_fps: f64,
    opts: &ExportOptions,
) -> Result<Target, CmError> {
    let def = presets.get(id).ok_or_else(|| {
        CmError::invalid_field(
            "preset",
            format!(
                "unknown preset `{id}`; use one of: quick, {}",
                presets.ids().join(", ")
            ),
        )
    })?;
    let mut notes = Vec::new();
    let has_opts = opts.width.is_some() || opts.fps.is_some() || opts.max_mb.is_some();
    if def.kind == Kind::Mp4 && has_opts {
        let key = if opts.width.is_some() {
            "width"
        } else if opts.fps.is_some() {
            "fps"
        } else {
            "max_mb"
        };
        return Err(CmError::invalid_field(
            key,
            format!(
                "applies to the `gif` and `webm` presets only; `{id}` has a fixed size, frame rate and bitrate"
            ),
        ));
    }
    let max_bytes = match opts.max_mb {
        None => None,
        Some(mb) if mb.is_finite() && (0.01..=1000.0).contains(&mb) => {
            Some((mb * 1_000_000.0).round() as u64)
        }
        Some(_) => {
            return Err(CmError::invalid_field(
                "max_mb",
                "must be a number from 0.01 to 1000 (MB = 10^6 bytes)",
            ))
        }
    };
    let master = (master.0.max(2), master.1.max(2));
    let mut t = Target {
        id: def.id.clone(),
        label: def.label.clone(),
        kind: def.kind,
        frame: None,
        fps: None,
        video_bps: None,
        loudness_lufs: def.loudness_lufs,
        safe: (def.safe_bottom_pct > 0.0 || def.safe_right_pct > 0.0).then_some(SafeArea {
            bottom: def.safe_bottom_pct / 100.0,
            right: def.safe_right_pct / 100.0,
        }),
        reframe: Reframe::Fit,
        max_bytes,
        max_seconds: def.max_seconds,
        warn_seconds: def.warn_seconds,
        width_ladder: Vec::new(),
        notes: Vec::new(),
    };
    match def.kind {
        Kind::Mp4 => {
            let fps = def.fps.unwrap_or(master_fps);
            match def.frame {
                FrameDef::Master => {}
                FrameDef::Fixed(w, h) => {
                    if def.refuse_upscale && fit_scale((w, h), master) > 1.0 + 1e-9 {
                        let hint = largest_without_upscale(presets, master)
                            .map(|d| format!("; use `{}` instead", d.id))
                            .unwrap_or_default();
                        return Err(CmError::invalid_field(
                            "preset",
                            format!(
                                "`{id}` would enlarge this {}x{} recording to {w}x{h}{hint}",
                                master.0, master.1
                            ),
                        ));
                    }
                    t.frame = Some((w, h));
                }
            }
            t.fps = Some(fps);
            let mbps = if fps > 31.0 {
                def.mbps_60.or(def.mbps_30)
            } else {
                def.mbps_30
            };
            t.video_bps = mbps.map(|m| (m * 1_000_000.0).round() as u64);
        }
        Kind::Gif => {
            let width = opts.width.or(def.width).unwrap_or(800);
            if !def.widths.is_empty() && !def.widths.contains(&width) {
                return Err(CmError::invalid_field(
                    "width",
                    format!(
                        "a GIF is {} px wide",
                        def.widths
                            .iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
            let fps = opts.fps.or(def.fps).unwrap_or(15.0);
            if !def.fps_choices.is_empty()
                && !def.fps_choices.iter().any(|f| (f - fps).abs() < 1e-9)
            {
                return Err(CmError::invalid_field(
                    "fps",
                    format!(
                        "a GIF runs at {} fps",
                        def.fps_choices
                            .iter()
                            .map(|f| format!("{f}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
            t.frame = Some(sized_by_width(width, master));
            t.fps = Some(fps);
            t.loudness_lufs = None;
            t.width_ladder = def
                .widths
                .iter()
                .copied()
                .filter(|w| *w <= width)
                .collect::<Vec<_>>();
            t.width_ladder.sort_unstable_by(|a, b| b.cmp(a));
        }
        Kind::WebM => {
            let width = opts.width.or(def.width).unwrap_or(1280).min(7680);
            if width < 16 || !width.is_multiple_of(2) {
                return Err(CmError::invalid_field(
                    "width",
                    "must be an even width of at least 16 px",
                ));
            }
            let fps = opts
                .fps
                .or(def.fps)
                .unwrap_or(30.0)
                .min(master_fps.max(1.0));
            if !(1.0..=60.0).contains(&fps) {
                return Err(CmError::invalid_field("fps", "must be from 1 to 60"));
            }
            // A recording narrower than the asked width is not enlarged.
            let width = width.min(even_up(f64::from(master.0)));
            t.frame = Some(sized_by_width(width, master));
            t.fps = Some(fps);
        }
    }
    t.notes.append(&mut notes);
    Ok(t)
}

/// A frame `width` wide with the recording's aspect (even sizes).
fn sized_by_width(width: u32, master: (u32, u32)) -> (u32, u32) {
    let h = f64::from(width) * f64::from(master.1) / f64::from(master.0.max(1));
    (width, even_up(h))
}

/// The recording's picture inside `frame` after `padding_pct` padding: the same arithmetic as the
/// compositor's `contain` (`cm_render::compose::plan`), so the camera bubble can be placed on the
/// picture instead of on the whole frame. `[x, y, w, h]` in whole pixels.
pub fn fit_rect(frame: (u32, u32), padding_pct: f64, src: (u32, u32)) -> [u32; 4] {
    let (wf, hf) = (f64::from(frame.0), f64::from(frame.1));
    let p = padding_pct.clamp(0.0, 20.0) / 100.0;
    let (pad_x, pad_y) = ((p * wf).round(), (p * hf).round());
    let cell = [
        pad_x,
        pad_y,
        (wf - 2.0 * pad_x).max(1.0),
        (hf - 2.0 * pad_y).max(1.0),
    ];
    let aspect = f64::from(src.0) / f64::from(src.1.max(1));
    let cell_aspect = cell[2] / cell[3];
    let (w, h) = if aspect > cell_aspect {
        (cell[2], cell[2] / aspect)
    } else {
        (cell[3] * aspect, cell[3])
    };
    let (w, h) = (w.round().max(1.0), h.round().max(1.0));
    [
        (cell[0] + (cell[2] - w) / 2.0).round() as u32,
        (cell[1] + (cell[3] - h) / 2.0).round() as u32,
        w as u32,
        h as u32,
    ]
}

/// The bubble on a preset frame: placed on the picture (`pic`, from [`fit_rect`]) the way the
/// layout places it on the recording, then moved out of the platform UI zones (`safe`) and kept
/// inside the frame.
pub fn place_bubble(
    layout: &CameraLayout,
    frame: (u32, u32),
    pic: [u32; 4],
    safe: Option<SafeArea>,
) -> BubbleGeom {
    let mut g = bubble_geom(layout, (pic[2].max(2), pic[3].max(2)));
    g.x += pic[0];
    g.y += pic[1];
    if let Some(s) = safe {
        let max_right = (f64::from(frame.0) * (1.0 - s.right)).floor() as u32;
        let max_bottom = (f64::from(frame.1) * (1.0 - s.bottom)).floor() as u32;
        if g.x + g.w > max_right {
            g.x = max_right.saturating_sub(g.w);
        }
        if g.y + g.h > max_bottom {
            g.y = max_bottom.saturating_sub(g.h);
        }
    }
    g.x = g.x.min(frame.0.saturating_sub(g.w + SAFE_BUBBLE_GAP_PX));
    g.y = g.y.min(frame.1.saturating_sub(g.h + SAFE_BUBBLE_GAP_PX));
    g
}

/// The video bitrate for a "fit under `max_bytes`" WebM of `duration_s`, leaving room for
/// `audio_bps` and 6% container overhead. At least 20 kbit/s.
pub fn fit_bitrate(max_bytes: u64, duration_s: f64, audio_bps: u64) -> u64 {
    let total = max_bytes as f64 * 8.0 * 0.94 / duration_s.max(0.1);
    ((total - audio_bps as f64).max(20_000.0)).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Shape;

    fn presets() -> Presets {
        Presets::builtin()
    }

    fn resolve_ok(id: &str, master: (u32, u32), fps: f64) -> Target {
        resolve(&presets(), id, master, fps, &ExportOptions::default()).unwrap()
    }

    /// The shipped table parses and lists every preset of the spec.
    #[test]
    fn the_embedded_table_has_the_spec_presets() {
        let p = presets();
        assert_eq!(p.version, TABLE_VERSION);
        assert_eq!(
            p.ids(),
            [
                "youtube-4k",
                "youtube-1440p",
                "youtube-1080p",
                "vertical-1080",
                "square-1080",
                "source",
                "gif",
                "webm"
            ]
        );
    }

    /// EXP-2: the frame, fps policy and bitrate of every platform preset.
    #[test]
    fn exp2_preset_parameters() {
        let m = (3840, 2160);
        let t = resolve_ok("youtube-4k", m, 30.0);
        assert_eq!(t.frame, Some((3840, 2160)));
        assert_eq!(t.video_bps, Some(40_000_000));
        assert_eq!(
            resolve_ok("youtube-4k", m, 60.0).video_bps,
            Some(60_000_000)
        );
        let t = resolve_ok("youtube-1440p", (3024, 1964), 30.0);
        assert_eq!(
            (t.frame, t.video_bps),
            (Some((2560, 1440)), Some(16_000_000))
        );
        assert_eq!(
            resolve_ok("youtube-1440p", (3024, 1964), 60.0).video_bps,
            Some(24_000_000)
        );
        let t = resolve_ok("youtube-1080p", (3024, 1964), 30.0);
        assert_eq!(
            (t.frame, t.video_bps),
            (Some((1920, 1080)), Some(8_000_000))
        );
        assert_eq!(
            resolve_ok("youtube-1080p", (3024, 1964), 60.0).video_bps,
            Some(12_000_000)
        );
        let v = resolve_ok("vertical-1080", (1920, 1080), 30.0);
        assert_eq!(
            (v.frame, v.video_bps),
            (Some((1080, 1920)), Some(12_000_000))
        );
        assert_eq!(
            v.safe,
            Some(SafeArea {
                bottom: 0.20,
                right: 0.12
            })
        );
        let s = resolve_ok("square-1080", (1920, 1080), 30.0);
        assert_eq!(
            (s.frame, s.video_bps),
            (Some((1080, 1080)), Some(8_000_000))
        );
        // Every platform preset normalises to -14 LUFS; `source` leaves the level alone.
        for id in [
            "youtube-1440p",
            "youtube-1080p",
            "vertical-1080",
            "square-1080",
        ] {
            assert_eq!(
                resolve_ok(id, (3840, 2160), 30.0).loudness_lufs,
                Some(-14.0)
            );
        }
        let src = resolve_ok("source", (3024, 1964), 60.0);
        assert_eq!((src.frame, src.loudness_lufs), (None, None));
        assert_eq!(src.fps, Some(60.0));
        assert_eq!(src.reframe, Reframe::Fit);
    }

    /// EXP-2.AC3: a 1080p master and `youtube-4k` is refused, with a hint to the right preset.
    #[test]
    fn exp2_ac3_upscaling_to_4k_is_refused_with_a_hint() {
        let e = resolve(
            &presets(),
            "youtube-4k",
            (1920, 1080),
            30.0,
            &ExportOptions::default(),
        )
        .unwrap_err();
        let m = e.to_string();
        assert!(
            m.contains("enlarge") && m.contains("`youtube-1080p`"),
            "{m}"
        );
        // A MacBook-sized master is below 2160p too.
        let e = resolve(
            &presets(),
            "youtube-4k",
            (3024, 1964),
            30.0,
            &ExportOptions::default(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("`youtube-1440p`"), "{e}");
        // A recording smaller than every landscape frame is pointed to `source`.
        let e = resolve(
            &presets(),
            "youtube-4k",
            (1280, 720),
            30.0,
            &ExportOptions::default(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("`source`"), "{e}");
        // A 4K master is fine, and so is a 1080p master for the 1080p preset.
        assert!(resolve(
            &presets(),
            "youtube-4k",
            (3840, 2160),
            30.0,
            &ExportOptions::default()
        )
        .is_ok());
        assert!(resolve(
            &presets(),
            "youtube-1080p",
            (1920, 1080),
            30.0,
            &ExportOptions::default()
        )
        .is_ok());
    }

    #[test]
    fn unknown_presets_and_misplaced_options_are_refused() {
        let o = ExportOptions::default();
        let e = resolve(&presets(), "tiktok", (1920, 1080), 30.0, &o).unwrap_err();
        assert!(e.to_string().contains("youtube-1080p"), "{e}");
        for (opts, key) in [
            (
                ExportOptions {
                    width: Some(800),
                    ..o
                },
                "width",
            ),
            (
                ExportOptions {
                    fps: Some(15.0),
                    ..o
                },
                "fps",
            ),
            (
                ExportOptions {
                    max_mb: Some(10.0),
                    ..o
                },
                "max_mb",
            ),
        ] {
            let e = resolve(&presets(), "youtube-1080p", (1920, 1080), 30.0, &opts).unwrap_err();
            assert!(e.to_string().contains(key), "{e}");
        }
    }

    /// EXP-3: GIF widths 480/640/800/1024 and fps 10/15/24; the frame keeps the recording's aspect.
    #[test]
    fn exp3_gif_parameters() {
        let t = resolve_ok("gif", (1920, 1080), 30.0);
        assert_eq!((t.frame, t.fps), (Some((800, 450)), Some(15.0)));
        assert_eq!(t.loudness_lufs, None);
        assert_eq!(t.width_ladder, [800, 640, 480]);
        let o = ExportOptions {
            width: Some(1024),
            fps: Some(24.0),
            max_mb: Some(10.0),
        };
        let t = resolve(&presets(), "gif", (3024, 1964), 60.0, &o).unwrap();
        assert_eq!(t.frame, Some((1024, 666)));
        assert_eq!(t.fps, Some(24.0));
        assert_eq!(t.max_bytes, Some(10_000_000));
        assert_eq!(t.width_ladder, [1024, 800, 640, 480]);
        for bad_opts in [
            ExportOptions {
                width: Some(700),
                ..ExportOptions::default()
            },
            ExportOptions {
                fps: Some(12.0),
                ..ExportOptions::default()
            },
            ExportOptions {
                max_mb: Some(0.0),
                ..ExportOptions::default()
            },
        ] {
            assert!(resolve(&presets(), "gif", (1920, 1080), 30.0, &bad_opts).is_err());
        }
        assert_eq!(t.max_seconds, Some(30.0));
        assert_eq!(t.warn_seconds, Some(15.0));
    }

    /// EXP-3: WebM takes any even width, never enlarges the recording, caps fps at the source.
    #[test]
    fn exp3_webm_parameters() {
        let t = resolve_ok("webm", (3024, 1964), 60.0);
        assert_eq!((t.frame, t.fps), (Some((1280, 831 + 831 % 2)), Some(30.0)));
        let small = resolve_ok("webm", (800, 600), 24.0);
        assert_eq!(small.frame, Some((800, 600)));
        assert_eq!(small.fps, Some(24.0));
        let o = ExportOptions {
            width: Some(641),
            ..ExportOptions::default()
        };
        assert!(resolve(&presets(), "webm", (1920, 1080), 30.0, &o).is_err());
    }

    /// EXP-2.AC4: a replacement table changes the limits without rebuilding.
    #[test]
    fn exp2_ac4_a_replacement_table_applies() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("platform_rules.toml");
        let edited = BUILTIN.replace("mbps_30 = 8\n", "mbps_30 = 5\n");
        assert_ne!(edited, BUILTIN);
        std::fs::write(&path, edited).unwrap();
        let loaded = Presets::load_file(&path);
        assert_eq!(loaded.source.as_deref(), Some(path.as_path()));
        assert!(loaded.warning.is_none());
        let t = resolve(
            &loaded.presets,
            "youtube-1080p",
            (1920, 1080),
            30.0,
            &ExportOptions::default(),
        )
        .unwrap();
        assert_eq!(t.video_bps, Some(5_000_000));
        // A broken file is ignored with a warning; the shipped table stays in force.
        std::fs::write(&path, "version = 1\n[[preset]]\nid = \"BAD ID\"\n").unwrap();
        let loaded = Presets::load_file(&path);
        assert_eq!(loaded.presets, Presets::builtin());
        assert!(loaded.warning.unwrap().contains("ignored"));
        // So is a newer shape version, and a missing file.
        std::fs::write(&path, "version = 2\n").unwrap();
        assert!(Presets::load_file(&path).warning.is_some());
        assert!(Presets::load_file(&d.path().join("none.toml"))
            .warning
            .is_some());
    }

    #[test]
    fn table_validation_rejects_bad_rows() {
        for text in [
            "version = 1\n[[preset]]\nid = \"a\"\nframe = [1921, 1080]\n",
            "version = 1\n[[preset]]\nid = \"a\"\nfps = 0\n",
            "version = 1\n[[preset]]\nid = \"a\"\nkind = \"avi\"\n",
            "version = 1\n[[preset]]\nid = \"a\"\n[[preset]]\nid = \"a\"\n",
            "version = 1\n[[preset]]\nid = \"a\"\nmbps_60 = 12\n",
            "version = 1\n",
        ] {
            assert!(Presets::from_toml(text).is_err(), "{text}");
        }
    }

    /// The fit arithmetic matches the compositor's tile for the screen.
    #[test]
    fn fit_rect_matches_the_compositor_plan() {
        use cm_render::compose::{plan, Frame, Scene};
        for (frame, src, pad) in [
            ((108u32, 192u32), (160u32, 90u32), 0.0f64),
            ((108, 108), (160, 100), 8.0),
            ((192, 108), (160, 100), 5.0),
            ((192, 108), (90, 160), 0.0),
        ] {
            let data = vec![0u8; (src.0 * src.1 * 4) as usize];
            let mut scene = Scene::new(frame, Frame::rgba(src.0, src.1, &data));
            scene.padding_pct = pad as f32;
            let tile = plan(&scene).unwrap().screen.unwrap();
            let want = tile.rect.map(|v| v as u32);
            assert_eq!(fit_rect(frame, pad, src), want, "{frame:?} {src:?} {pad}");
        }
    }

    fn layout() -> CameraLayout {
        CameraLayout {
            visible: true,
            shape: Shape::Circle,
            rect: [0.78, 0.74, 0.16, 0.16 * 16.0 / 9.0],
            mirror: None,
            ring: 4.0 / 240.0,
            radius: 0.0,
            size: None,
        }
    }

    /// EXP-2 (`vertical-1080`): the bubble sits on the picture and stays out of the bottom 20%
    /// and the right 12% of the frame.
    #[test]
    fn vertical_bubble_stays_in_the_safe_area() {
        let frame = (1080, 1920);
        let pic = fit_rect(frame, 0.0, (1920, 1080));
        assert_eq!(pic, [0, 656, 1080, 608]);
        let safe = Some(SafeArea {
            bottom: 0.20,
            right: 0.12,
        });
        let g = place_bubble(&layout(), frame, pic, safe);
        assert!(g.x + g.w <= 950, "{g:?}");
        assert!(g.y + g.h <= 1536, "{g:?}");
        // Without a safe area the bubble sits at its layout place on the picture.
        let free = place_bubble(&layout(), frame, pic, None);
        assert!(
            free.y >= pic[1] && free.y + free.h <= pic[1] + pic[3],
            "{free:?}"
        );
        assert!(free.x > g.x, "the safe area moved it left");
        // Inside the frame whatever the layout says.
        let mut far = layout();
        far.rect = [0.99, 0.99, 0.3, 0.3];
        let g = place_bubble(&far, frame, pic, safe);
        assert!(g.x + g.w <= frame.0 && g.y + g.h <= frame.1);
    }

    #[test]
    fn fit_bitrate_leaves_room_for_audio_and_overhead() {
        // 10 MB over 10 s = 8 Mbit/s raw; 6% overhead and 96 kbit/s of audio come off.
        let b = fit_bitrate(10_000_000, 10.0, 96_000);
        assert_eq!(b, ((8_000_000.0 * 0.94) - 96_000.0) as u64);
        assert_eq!(fit_bitrate(1_000, 600.0, 96_000), 20_000);
    }
}
