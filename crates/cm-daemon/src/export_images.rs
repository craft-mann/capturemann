//! Boards and Bulk export of pictures (BRD-1, BLK-1).
//!
//! * `create_board { capture_ids, layout, options?, title? }` makes a **new** screenshot capture
//!   from the rendered pictures of 2 to 12 screenshots (annotations and redactions applied, so a
//!   blurred area stays blurred and the board holds no unblurred pixels, BRD-1.AC2). The sources
//!   are copied into the picture: deleting one later changes nothing (BRD-1.AC3). Free tier: it
//!   creates a capture and changes no other.
//! * `export_images { capture_ids, dest, size?, format?, quality?, watermark?, blueprint?,
//!   name_pattern? }` queues one background `export` job (`kind: "images"`) that writes new files
//!   into `dest`: size rule, PNG / JPEG / lossless WebP, a watermark drawn on the file, a name
//!   pattern, never overwriting (BLK-1). The originals are never changed. A cancel keeps the files
//!   already written and leaves no partial file (BLK-1.AC2). The result lists what was exported
//!   and what was skipped, with a reason each (`Retry skipped` is a new call with those ids).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::Engine;
use cm_core::{Actor, CmError};
use cm_jobs::{JobContext, NewJob, JOB_EXPORT};
use cm_library::import::{install_as, new_staging_dir, remove_staging};
use cm_library::manifest::CaptureKind;
use cm_library::Library;
use cm_render::compose_board::{
    self, BoardLayout, BoardOptions, LayoutKind, MAX_SOURCES, MIN_SOURCES,
};
use cm_render::compose_bulk::{self, Mark, Position, SizeRule, Watermark};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;
use crate::events;
use crate::export::ExportHandler;

/// The `kind` of an `export` job that writes pictures.
pub(crate) const KIND: &str = "images";
/// Most captures one `export_images` call takes.
pub const MAX_CAPTURES: usize = 500;
/// A board's picture is returned inline up to this size (larger: no `image`, the capture is there).
const MAX_INLINE_BYTES: usize = 8_000_000;

// ---- shared: the rendered picture of a screenshot -------------------------------------------

/// The rendered picture of a screenshot capture (annotations and redactions applied) and its
/// record. A recording is `invalid_input`.
fn rendered(
    library: &Arc<Mutex<Library>>,
    id: &str,
) -> Result<(cm_library::index::CaptureRecord, RgbaImage), CmError> {
    let (rec, dir) = {
        let lib = lock_library(library);
        let rec = lib.get(id)?;
        let dir = lib.bundle_path(&rec);
        (rec, dir)
    };
    if rec.kind != CaptureKind::Screenshot {
        return Err(CmError::invalid_field(
            "capture_ids",
            format!("`{id}` is not a screenshot; boards and picture export take screenshots"),
        ));
    }
    let bundle = cm_library::ShotBundle::open(&dir)?;
    let base = image::open(bundle.original_path())
        .map_err(|e| CmError::internal(format!("cannot read the picture of `{id}`: {e}")))?
        .to_rgba8();
    let out = if bundle.ops().is_empty() {
        base
    } else {
        cm_render::render_annotations(&base, bundle.ops())?
    };
    Ok((rec, out))
}

// ---- create_board ---------------------------------------------------------------------------

/// `create_board` input.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateBoardParams {
    pub capture_ids: Vec<String>,
    pub layout: BoardLayout,
    pub options: BoardOptions,
    /// The new capture's title (default: the board title, else `Board`).
    pub title: Option<String>,
    pub request_id: Option<String>,
}

fn from_value<T: serde::de::DeserializeOwned>(v: &Value, field: &str) -> Result<T, CmError> {
    serde_json::from_value(v.clone()).map_err(|e| CmError::invalid_field(field, e.to_string()))
}

pub(crate) fn parse_board(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["capture_ids", "layout", "options", "title", "request_id"])?;
    let ids = capture_ids(f, MIN_SOURCES, MAX_SOURCES)?;
    let layout = match f.obj.get("layout") {
        Some(Value::String(s)) => {
            let kind: LayoutKind = from_value(&json!(s), "layout")?;
            BoardLayout::new(kind)
        }
        Some(v @ Value::Object(_)) => from_value(v, "layout")?,
        None | Some(Value::Null) => {
            return Err(CmError::invalid_field(
                "layout",
                "is required: pair, grid, strip or free, or an object with `kind`",
            ))
        }
        Some(_) => {
            return Err(CmError::invalid_field(
                "layout",
                "must be a name or an object",
            ))
        }
    };
    let options: BoardOptions = match f.obj.get("options") {
        None | Some(Value::Null) => BoardOptions::default(),
        Some(v) => from_value(v, "options")?,
    };
    options.validate()?;
    Ok(Action::CreateBoard(CreateBoardParams {
        capture_ids: ids,
        layout,
        options,
        title: f.str_opt("title")?,
        request_id: f.request_id()?,
    }))
}

fn capture_ids(f: &Fields<'_>, min: usize, max: usize) -> Result<Vec<String>, CmError> {
    let arr = f
        .obj
        .get("capture_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| CmError::invalid_field("capture_ids", "is required: an array of ids"))?;
    if arr.len() < min || arr.len() > max {
        return Err(CmError::invalid_field(
            "capture_ids",
            format!("must hold {min} to {max} capture ids"),
        ));
    }
    arr.iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    CmError::invalid_field(&format!("capture_ids[{i}]"), "must be an id string")
                })
        })
        .collect()
}

pub(crate) fn board_summary(p: &CreateBoardParams) -> String {
    format!(
        "Make a {} board of {} screenshots",
        p.layout.kind.as_str(),
        p.capture_ids.len()
    )
}

impl Inner {
    /// `create_board`.
    pub(crate) async fn create_board(
        &self,
        actor: &Actor,
        p: CreateBoardParams,
    ) -> Result<Value, DaemonError> {
        let (library, events, now) = (self.library.clone(), self.events.clone(), self.clock.now());
        let by = actor.clone();
        let out = blocking(move || {
            let mut pics = Vec::with_capacity(p.capture_ids.len());
            let mut sources = Vec::with_capacity(p.capture_ids.len());
            for id in &p.capture_ids {
                let (rec, img) = rendered(&library, id)?;
                sources.push(json!({
                    "capture_id": rec.id, "title": rec.title,
                    "width": img.width(), "height": img.height(),
                }));
                pics.push(img);
            }
            let board = compose_board::render_board(&pics, &p.layout, &p.options)?;
            drop(pics);
            let png = cm_render::encode_png(&board)?;
            let title = p
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .or_else(|| p.options.title.clone().filter(|t| !t.trim().is_empty()))
                .unwrap_or_else(|| "Board".to_owned());
            let id = cm_library::board::mint_id(now);
            let meta = json!({
                "layout": p.layout, "options": p.options, "sources": sources,
                "created_by": by,
            });
            let root = lock_library(&library).root().to_path_buf();
            let stage = new_staging_dir(&root, &format!("board-{id}"), 0)?;
            let built =
                cm_library::board::build_board_bundle(&stage, &id, &title, &png, &meta, now);
            let (dir, thumb_warning) = match built {
                Ok(b) => b,
                Err(e) => {
                    remove_staging(&stage);
                    return Err(e);
                }
            };
            let record = {
                let mut lib = lock_library(&library);
                install_as(&mut lib, &dir, &title, CaptureKind::Screenshot)
            };
            remove_staging(&stage);
            let record = record?;
            events.publish(
                events::CAPTURE_CREATED,
                json!({ "capture_id": id, "title": record.title, "by": by,
                        "kind": record.kind.as_str(), "board": p.layout.kind.as_str() }),
            );
            let path = lock_library(&library).bundle_path(&record);
            let mut out = json!({
                "capture_id": id, "title": record.title, "path": path,
                "kind": record.kind.as_str(), "layout": p.layout.kind.as_str(),
                "width": board.width(), "height": board.height(),
                "sources": sources,
            });
            if let Some(w) = thumb_warning {
                out["warnings"] = json!([format!("thumbnail not created: {w}")]);
            }
            if png.len() <= MAX_INLINE_BYTES {
                out["image"] = json!({
                    "mime_type": "image/png",
                    "data": base64::engine::general_purpose::STANDARD.encode(&png),
                });
            }
            Ok(out)
        })
        .await?;
        Ok(out)
    }
}

// ---- export_images --------------------------------------------------------------------------

/// The picture format of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    /// Lossless only (the encoder in the `image` crate writes nothing else).
    Webp,
}

impl ImageFormat {
    fn ext(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
        }
    }
}

/// The size step.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SizeStep {
    #[serde(flatten)]
    pub rule: SizeRule,
    /// Larger than the original only when this is true.
    #[serde(default)]
    pub allow_larger: bool,
}

/// The watermark step: text or a PNG/JPEG file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatermarkStep {
    pub text: Option<String>,
    /// Absolute path of a PNG or JPEG.
    pub image_path: Option<String>,
    #[serde(default)]
    pub position: Position,
    /// 0.05 to 1.0 (default 0.5).
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Width of the mark relative to the picture, 0.05 to 1.0 (default 0.25).
    #[serde(default = "default_mark_size")]
    pub size: f32,
}

fn default_opacity() -> f32 {
    0.5
}
fn default_mark_size() -> f32 {
    0.25
}

/// `export_images` input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportImagesParams {
    pub capture_ids: Vec<String>,
    pub dest: String,
    pub size: Option<SizeStep>,
    pub format: ImageFormat,
    /// JPEG quality, 60 to 100.
    pub quality: u8,
    pub watermark: Option<WatermarkStep>,
    /// BLK-1 Blueprint step (ANN-11). Not available until the `blueprint` op lands (Track A).
    pub blueprint: bool,
    /// `{name}`, `{date}`, `{n}`, `{w}`, `{h}`.
    pub name_pattern: String,
    #[serde(skip)]
    pub request_id: Option<String>,
}

/// The default file name pattern.
pub const DEFAULT_PATTERN: &str = "{name}";

pub(crate) fn parse_export(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&[
        "capture_ids",
        "dest",
        "size",
        "format",
        "quality",
        "watermark",
        "blueprint",
        "name_pattern",
        "request_id",
    ])?;
    let capture_ids = capture_ids(f, 1, MAX_CAPTURES)?;
    let dest = f.str_req("dest")?;
    if !Path::new(&dest).is_absolute() {
        return Err(CmError::invalid_field(
            "dest",
            "must be an absolute folder path",
        ));
    }
    let size = match f.obj.get("size") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let s: SizeStep = from_value(v, "size")?;
            compose_bulk::validate_rule(&s.rule)?;
            Some(s)
        }
    };
    let format: ImageFormat = match f.obj.get("format") {
        None | Some(Value::Null) => ImageFormat::Png,
        Some(v) => from_value(v, "format").map_err(|_| {
            CmError::invalid_field("format", "must be \"png\", \"jpeg\" or \"webp\" (lossless)")
        })?,
    };
    let quality = match f.obj.get("quality") {
        None | Some(Value::Null) => 90,
        Some(v) => v
            .as_u64()
            .filter(|q| (60..=100).contains(q))
            .ok_or_else(|| CmError::invalid_field("quality", "must be 60 to 100"))?
            as u8,
    };
    let watermark = match f.obj.get("watermark") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let w: WatermarkStep = from_value(v, "watermark")?;
            match (&w.text, &w.image_path) {
                (Some(t), None) if !t.trim().is_empty() && t.chars().count() <= 200 => {}
                (None, Some(p)) if Path::new(p).is_absolute() => {}
                _ => {
                    return Err(CmError::invalid_field(
                        "watermark",
                        "needs exactly one of `text` (up to 200 characters) or an absolute \
                         `image_path`",
                    ))
                }
            }
            if !(0.05..=1.0).contains(&w.opacity) || !(0.05..=1.0).contains(&w.size) {
                return Err(CmError::invalid_field(
                    "watermark",
                    "opacity and size must be 0.05 to 1.0",
                ));
            }
            Some(w)
        }
    };
    let name_pattern = f
        .str_opt("name_pattern")?
        .unwrap_or_else(|| DEFAULT_PATTERN.to_owned());
    check_pattern(&name_pattern)?;
    Ok(Action::ExportImages(ExportImagesParams {
        capture_ids,
        dest,
        size,
        format,
        quality,
        watermark,
        blueprint: f.bool_or("blueprint", false)?,
        name_pattern,
        request_id: f.request_id()?,
    }))
}

pub(crate) fn export_summary(p: &ExportImagesParams) -> String {
    format!(
        "Export {} screenshot(s) as {} into {}",
        p.capture_ids.len(),
        p.format.ext(),
        p.dest
    )
}

/// The tokens a name pattern may hold.
const TOKENS: [&str; 5] = ["name", "date", "n", "w", "h"];

/// Checks the pattern: only the known tokens, balanced braces, not empty.
pub fn check_pattern(pattern: &str) -> Result<(), CmError> {
    expand(pattern, |_| Some(String::new())).map(|_| ())
}

fn expand(pattern: &str, mut value: impl FnMut(&str) -> Option<String>) -> Result<String, CmError> {
    let bad = |m: String| CmError::invalid_field("name_pattern", m);
    if pattern.trim().is_empty() || pattern.chars().count() > 200 {
        return Err(bad("must be 1 to 200 characters".into()));
    }
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        if rest.as_bytes()[i] == b'}' {
            return Err(bad("has a `}` without a `{`".into()));
        }
        let Some(j) = rest[i..].find('}') else {
            return Err(bad("has a `{` without a `}`".into()));
        };
        let token = &rest[i + 1..i + j];
        if !TOKENS.contains(&token) {
            return Err(bad(format!(
                "`{{{token}}}` is not known; use {{name}}, {{date}}, {{n}}, {{w}} and {{h}}"
            )));
        }
        out.push_str(&value(token).unwrap_or_default());
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Makes a string safe as one file name.
fn safe_stem(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let t = cleaned.trim().trim_matches('.').trim();
    let t: String = t.chars().take(150).collect();
    if t.is_empty() {
        "image".to_owned()
    } else {
        t
    }
}

/// The file stem for one picture. Pure.
pub fn file_stem(pattern: &str, name: &str, date: &str, n: usize, w: u32, h: u32) -> String {
    let s = expand(pattern, |t| {
        Some(match t {
            "name" => name.to_owned(),
            "date" => date.to_owned(),
            "n" => n.to_string(),
            "w" => w.to_string(),
            "h" => h.to_string(),
            _ => String::new(),
        })
    })
    .unwrap_or_else(|_| name.to_owned());
    safe_stem(&s)
}

/// Reserves a file that does not exist: `stem.ext`, else `stem (2).ext`, ... The empty file is
/// created with `create_new`, so a name that exists is never overwritten, not even by a race.
fn reserve(dest: &Path, stem: &str, ext: &str) -> std::io::Result<PathBuf> {
    for k in 1..100_000u32 {
        let name = if k == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} ({k}).{ext}")
        };
        let path = dest.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("too many files with the same name"))
}

/// Encodes a picture. JPEG has no alpha: it goes on white.
fn encode(img: &RgbaImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>, CmError> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    let bad = |e: image::ImageError| CmError::internal(format!("cannot encode the picture: {e}"));
    match format {
        ImageFormat::Png => return cm_render::encode_png(img),
        ImageFormat::Jpeg => {
            let flat = compose_board::flatten_on_white(img);
            let rgb = image::DynamicImage::ImageRgba8(flat).to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality)
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(bad)?;
        }
        ImageFormat::Webp => {
            image::codecs::webp::WebPEncoder::new_lossless(&mut buf)
                .write_image(
                    img.as_raw(),
                    img.width(),
                    img.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(bad)?;
        }
    }
    Ok(buf)
}

/// What the Blueprint step does (ANN-11).
///
/// TODO(Track A): call `cm_render`'s `blueprint` op here once Track A merges it. Until then the
/// step is a clearly refused hook, so a call that asks for it fails before anything is written.
fn apply_blueprint(_img: &mut RgbaImage) -> Result<(), CmError> {
    Err(CmError::not_implemented(
        "the Blueprint step of Bulk export is not available yet",
    ))
}

fn date_of(t: time::OffsetDateTime) -> String {
    format!("{:04}-{:02}-{:02}", t.year(), u8::from(t.month()), t.day())
}

/// What happened to one picture (a row of the summary).
fn skipped(id: &str, reason: &str, message: impl Into<String>) -> Value {
    json!({ "capture_id": id, "status": "skipped", "reason": reason, "message": message.into() })
}

/// Exports one picture into `dest`. Blocking. The file is written under a temporary name and moved
/// onto the reserved name, so an interrupted run never leaves a half-written picture.
#[allow(clippy::too_many_arguments)]
fn export_one(
    library: &Arc<Mutex<Library>>,
    id: &str,
    index: usize,
    p: &ExportImagesParams,
    mark: Option<&Watermark>,
    dest: &Path,
    job_id: &str,
) -> Value {
    let (rec, mut img) = match rendered(library, id) {
        Ok(r) => r,
        Err(e) => {
            let reason = match e.code() {
                cm_core::ErrorCode::NotFound => "not_found",
                cm_core::ErrorCode::InvalidInput => "not_a_screenshot",
                _ => "unreadable",
            };
            return skipped(id, reason, e.to_string());
        }
    };
    if p.blueprint {
        if let Err(e) = apply_blueprint(&mut img) {
            return skipped(id, "blueprint_unavailable", e.to_string());
        }
    }
    if let Some(step) = &p.size {
        let (w, h) =
            compose_bulk::target_size(img.width(), img.height(), &step.rule, step.allow_larger);
        img = compose_bulk::resize_to(&img, w, h);
    }
    if let Some(m) = mark {
        if let Err(e) = compose_bulk::apply_watermark(&mut img, m) {
            return skipped(id, "watermark_failed", e.to_string());
        }
    }
    let (w, h) = img.dimensions();
    let bytes = match encode(&img, p.format, p.quality) {
        Ok(b) => b,
        Err(e) => return skipped(id, "encode_failed", e.to_string()),
    };
    drop(img);
    let stem = file_stem(
        &p.name_pattern,
        &rec.title,
        &date_of(rec.created),
        index + 1,
        w,
        h,
    );
    let io_reason = |e: &std::io::Error| {
        if e.kind() == std::io::ErrorKind::StorageFull {
            "no_space"
        } else {
            "write_failed"
        }
    };
    let target = match reserve(dest, &stem, p.format.ext()) {
        Ok(t) => t,
        Err(e) => return skipped(id, io_reason(&e), e.to_string()),
    };
    let tmp = dest.join(format!(".cm-{job_id}-{index}.tmp"));
    let written = std::fs::write(&tmp, &bytes).and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&target);
        return skipped(id, io_reason(&e), e.to_string());
    }
    json!({
        "capture_id": id, "status": "exported", "path": target,
        "width": w, "height": h, "bytes": bytes.len(),
    })
}

fn load_watermark(step: &WatermarkStep) -> Result<Watermark, CmError> {
    let mark = match (&step.text, &step.image_path) {
        (Some(t), _) => Mark::Text(t.clone()),
        (None, Some(path)) => Mark::Image(
            image::open(path)
                .map_err(|e| {
                    CmError::invalid_field(
                        "watermark.image_path",
                        format!("cannot read `{path}` as a PNG or JPEG: {e}"),
                    )
                })?
                .to_rgba8(),
        ),
        _ => return Err(CmError::invalid_field("watermark", "is empty")),
    };
    Ok(Watermark {
        mark,
        position: step.position,
        opacity: step.opacity,
        size: step.size,
    })
}

impl Inner {
    /// `export_images`: checks the call, then queues the job and returns at once.
    pub(crate) async fn export_images(
        &self,
        actor: &Actor,
        p: ExportImagesParams,
    ) -> Result<Value, DaemonError> {
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_EXPORT) {
            return Err(DaemonError::NotImplemented(
                "export is not available: this daemon has no exporter registered".to_owned(),
            ));
        }
        if p.blueprint {
            // Fails now while the step is not available (Track A), before anything is queued.
            apply_blueprint(&mut RgbaImage::new(1, 1))?;
        }
        let (dest, wm) = (p.dest.clone(), p.watermark.clone());
        blocking(move || {
            let d = Path::new(&dest);
            if d.exists() && !d.is_dir() {
                return Err(CmError::invalid_field("dest", "is not a folder"));
            }
            if let Some(w) = &wm {
                load_watermark(w)?;
            }
            Ok(())
        })
        .await?;
        let inputs = json!({
            "kind": KIND,
            "params": p,
            "request_id": p.request_id,
            "by": actor.to_string(),
            "total": p.capture_ids.len(),
        });
        let job = blocking(move || queue.enqueue(NewJob::new(JOB_EXPORT, inputs))).await?;
        Ok(json!({
            "job_id": job.id, "kind": KIND, "state": job.state.as_str(),
            "dest": p.dest, "count": p.capture_ids.len(),
        }))
    }
}

/// Runs an `export` job of `kind: "images"`.
pub(crate) async fn run_job(h: &ExportHandler, ctx: JobContext) -> Result<Value, CmError> {
    let p: ExportImagesParams = from_value(&ctx.job.inputs["params"], "params")?;
    let dest = PathBuf::from(&p.dest);
    let mark = match &p.watermark {
        Some(w) => {
            let w = w.clone();
            Some(
                tokio::task::spawn_blocking(move || load_watermark(&w))
                    .await
                    .map_err(|e| CmError::internal(format!("worker failed: {e}")))??,
            )
        }
        None => None,
    };
    {
        let d = dest.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&d).map_err(|e| {
                let msg = format!("cannot create the folder `{}`: {e}", d.display());
                if e.kind() == std::io::ErrorKind::StorageFull {
                    CmError::disk_full(msg)
                } else {
                    CmError::invalid_field("dest", msg)
                }
            })
        })
        .await
        .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
    }
    // Rows finished by an earlier attempt of this job (the checkpoint).
    let mut rows: Vec<Value> = ctx
        .job
        .checkpoint
        .as_ref()
        .and_then(|c| c["rows"].as_array().cloned())
        .unwrap_or_default();
    let total = p.capture_ids.len();
    for (i, id) in p.capture_ids.iter().enumerate() {
        if i < rows.len() {
            continue;
        }
        // Cancel, pause or shutdown: stop between files. The files written stay; no file is
        // half written because each one is moved into place whole.
        if ctx.cancel.is_interrupted() {
            return Err(CmError::internal("the export was interrupted"));
        }
        ctx.reporter.report(
            i as f64 / total.max(1) as f64,
            Some(format!("Exporting {} of {total}", i + 1)),
        );
        let (library, params, mark, dest, job_id, id) = (
            h.library.clone(),
            p.clone(),
            mark.clone(),
            dest.clone(),
            ctx.job.id.clone(),
            id.clone(),
        );
        let row = tokio::task::spawn_blocking(move || {
            export_one(&library, &id, i, &params, mark.as_ref(), &dest, &job_id)
        })
        .await
        .map_err(|e| CmError::internal(format!("export worker failed: {e}")))?;
        rows.push(row);
        ctx.reporter.save_checkpoint(&json!({ "rows": rows }))?;
    }
    let exported = rows.iter().filter(|r| r["status"] == "exported").count();
    let skipped_rows: Vec<&Value> = rows.iter().filter(|r| r["status"] == "skipped").collect();
    ctx.reporter.report(1.0, Some("Done".to_owned()));
    let mut summary = format!("{exported} exported");
    if let Some(first) = skipped_rows.first() {
        summary.push_str(&format!(
            ", {} skipped: {}",
            skipped_rows.len(),
            first["reason"].as_str().unwrap_or("")
        ));
    }
    Ok(json!({
        "dest": p.dest, "exported": exported, "skipped": skipped_rows.len(),
        "summary": summary, "files": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_are_checked_and_expanded() {
        assert!(check_pattern("{name}-{n}").is_ok());
        assert!(check_pattern("{name}_{w}x{h}").is_ok());
        for bad in ["", "{nam}", "{name", "name}", "{}"] {
            assert!(check_pattern(bad).is_err(), "{bad}");
        }
        assert_eq!(
            file_stem(
                "{date} {name} {w}x{h} #{n}",
                "Login",
                "2026-11-14",
                3,
                800,
                600
            ),
            "2026-11-14 Login 800x600 #3"
        );
        // Path characters never reach the file system.
        assert_eq!(file_stem("{name}", "a/b\\c:d", "d", 1, 1, 1), "a_b_c_d");
        assert_eq!(file_stem("{name}", "..", "d", 1, 1, 1), "image");
    }

    #[test]
    fn a_name_that_exists_is_numbered_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.png"), b"mine").unwrap();
        let p = reserve(tmp.path(), "a", "png").unwrap();
        assert_eq!(p.file_name().unwrap(), "a (2).png");
        let p = reserve(tmp.path(), "a", "png").unwrap();
        assert_eq!(p.file_name().unwrap(), "a (3).png");
        assert_eq!(std::fs::read(tmp.path().join("a.png")).unwrap(), b"mine");
    }

    #[test]
    fn jpeg_and_webp_encode_and_decode_back() {
        let img = RgbaImage::from_pixel(32, 24, image::Rgba([10, 120, 200, 255]));
        for f in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Webp] {
            let bytes = encode(&img, f, 90).unwrap();
            let back = image::load_from_memory(&bytes).unwrap();
            assert_eq!((back.width(), back.height()), (32, 24), "{f:?}");
        }
    }
}
