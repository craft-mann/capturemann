//! `estimate_export` (EXP-5): the size an export would have, before it runs.
//!
//! Free and read-only: a formula over the length after cuts, the chosen frame and a per-codec
//! bit-rate table (`cm_media::estimate`) for the bound, plus the screen master's file size (one
//! `stat`, no decoding) for the typical size; no file of the recording is read besides its
//! manifest.
//! It takes the fields of `export_video` that choose the file (`format`, `size`, `width`,
//! `max_mb`) and refuses what `export_video` refuses, with the same text.

use cm_core::CmError;
use cm_media::estimate::{self, Choice, Codec, Estimated, Format, MasterRate, SizeChoice};
use serde_json::{json, Value};

use crate::action::{parse_export_options, parse_export_size, Action, Fields};
use crate::daemon::{blocking, Inner};
use crate::error::DaemonError;

pub(crate) const METHOD: &str = "estimate_export";

/// `estimate_export { capture_id, format?, size?, width?, max_mb? }`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EstimateExportParams {
    pub capture_id: String,
    /// `mp4` (the default), `webm` or `gif`.
    pub format: String,
    #[serde(serialize_with = "crate::action::ser_size")]
    pub size: Option<SizeChoice>,
    pub width: Option<u32>,
    pub max_mb: Option<f64>,
}

impl EstimateExportParams {
    pub fn choice(&self) -> Choice {
        Choice {
            format: Format::parse(&self.format).unwrap_or(Format::Mp4),
            size: self.size,
            width: self.width,
            max_mb: self.max_mb,
        }
    }
}

pub(crate) fn parse(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["capture_id", "format", "size", "width", "max_mb"])?;
    let format = f.str_opt("format")?.unwrap_or_else(|| "mp4".to_owned());
    if Format::parse(&format).is_none() {
        return Err(CmError::invalid_field(
            "format",
            "must be `mp4`, `webm` or `gif`",
        ));
    }
    let (width, _fps, max_mb) = parse_export_options(f)?;
    let size = parse_export_size(f)?;
    Ok(Action::EstimateExport(EstimateExportParams {
        capture_id: f.str_req("capture_id")?,
        format,
        size,
        width,
        max_mb,
    }))
}

/// The screen master's size and length for the typical estimate: one `stat` of the master file,
/// no decoding. `None` (the bound alone) for an audio or imported capture without a screen
/// master, a missing file or an unreadable manifest; never an error.
pub(crate) fn master_rate_of(dir: &std::path::Path, manifest: &Value) -> Option<MasterRate> {
    let spec = cm_media::QuickSpec::from_manifest(manifest).ok()?;
    estimate::master_rate(dir, manifest, &spec)
}

/// The codec of an MP4 here. FFmpeg is looked up only where the answer needs it (Linux).
pub(crate) fn mp4_codec_here() -> Result<Codec, CmError> {
    let tools = cm_media::tools::locate().ok();
    estimate::mp4_codec(tools.as_ref())
}

/// The sizes the chooser lists for a recording: the recording's size, the steps below it and the
/// GIF widths.
pub(crate) fn options_json(master: (u32, u32)) -> Value {
    let src = cm_media::graph::output_size(master);
    json!({
        "source": { "width": src.0, "height": src.1 },
        "sizes": estimate::size_options(master).iter().map(|o| json!({
            "size": o.choice.to_json(), "width": o.width, "height": o.height,
        })).collect::<Vec<_>>(),
        "gif_widths": estimate::gif_width_options(master),
    })
}

/// The reply for a manifest and a choice (pure but for the FFmpeg lookup of `codec`). `master`
/// is the screen master's rate ([`estimate::master_rate`] of the bundle folder) for the typical
/// size; `None` answers the bound alone.
pub(crate) fn estimate_json(
    manifest: &Value,
    choice: &Choice,
    mp4_codec: Codec,
    master: Option<&MasterRate>,
) -> Result<Value, CmError> {
    let spec = cm_media::QuickSpec::from_manifest(manifest)?;
    let mut v = match estimate::estimate_with_master(manifest, choice, mp4_codec, master)? {
        Estimated::Bound(e) => json!({
            "max_bytes": e.max_bytes, "label": e.label,
            "typical_bytes": e.typical_bytes, "typical_label": e.typical_label,
            "codec": e.codec.as_str(), "width": e.width, "height": e.height,
            "fps": e.fps, "duration_s": e.duration_s,
        }),
        Estimated::KnownAfterExport { duration_s } => {
            json!({ "known_after_export": true, "duration_s": duration_s })
        }
    };
    v["format"] = json!(choice.format.as_str());
    v["options"] = options_json(spec.screen.size);
    Ok(v)
}

impl Inner {
    /// `estimate_export`.
    pub(crate) async fn estimate_export(
        &self,
        p: EstimateExportParams,
    ) -> Result<Value, DaemonError> {
        // Option combinations that need no recording to refuse (the same as `export_video`).
        crate::action::check_size_options(&p.format, p.size, p.width)?;
        let library = self.library.clone();
        blocking(move || {
            let (_, dir, manifest) = crate::export::load_recording(&library, &p.capture_id)?;
            let choice = p.choice();
            let codec = if choice.format == Format::Mp4 {
                mp4_codec_here()?
            } else {
                Codec::H264
            };
            let master = master_rate_of(&dir, &manifest);
            estimate_json(&manifest, &choice, codec, master.as_ref())
        })
        .await
    }
}
