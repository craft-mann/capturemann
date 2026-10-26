//! `export_video` and the `export` job (EXP-1, EXP-4, REC-1 auto-export).
//!
//! # The interim exporter
//!
//! After a recording stops, CaptureMann renders one combined MP4 (screen + round camera bubble with
//! the 4 px rec ring + microphone and system audio) with the FFmpeg sidecar (`cm-media`) into
//! `<bundle>/exports/<title> - quick.mp4`. The masters are never touched. In M4 the GPU compositor
//! replaces the renderer; the pieces below stay:
//!
//! * the action `export_video { capture_id, preset?, request_id? }` -> `{ job_id }` (Free,
//!   mutating, idempotent by `request_id`). Only the preset `quick` exists; every other preset
//!   (`youtube-1080p`, ...) and the custom `size`/`fps`/`codec` form are refused with
//!   `invalid_input` ("not available yet");
//! * the job type `export` with inputs `{ capture_id, preset, by, auto? }`, one at a time, held
//!   while a recording runs (EXP-4.AC2) and restarted from the beginning after a restart;
//! * the manifest entry appended on success (`exports[]`: `file`, `preset`, `created`,
//!   `duration_ms`, `size_bytes`, `encoder`, `settings`) and the events `export.queued`,
//!   `export.done` and `capture.revised`.
//!
//! # Auto-export
//!
//! The setting `export.auto_on_stop` (default **true**) makes the recording engine enqueue the job
//! when a recording stops normally (never for a discard, a cancelled countdown or the 60 fps
//! self-test). It runs in the daemon, so recordings started from the CLI or an agent get the same
//! export. The enqueue happens after the job holds are released, so the job starts at once; if a
//! new recording starts, the existing hold pauses it. The job is only enqueued when an `export`
//! handler is registered ([`Daemon::register_export_handler`]).
//!
//! # Layout at stop
//!
//! The bubble is a window of the desktop app, drawn again at export. The engine does not record
//! moves *during* the recording (known gap); at stop it persists the layout then in
//! `edits[0].layout.camera` ([`layout_at_stop`]). A change made during the recording through the
//! `camera.*` settings (the desktop writes them when the bubble is dragged or its look changes) is
//! picked up; hiding the bubble is not recorded.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cm_core::{Actor, CmError};
use cm_jobs::{
    Job, JobContext, JobFilter, JobHandler, JobQueue, JobState, NewJob, ProgressReporter,
    JOB_EXPORT,
};
use cm_library::index::CaptureRecord;
use cm_library::manifest::CaptureKind;
use cm_library::Library;
use cm_media::tools::{locate, Tools};
use cm_media::{
    export_quick, ExportError, ExportOptions, PresetRequest, Presets, QuickRequest, PRESET_QUICK,
};
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;

use crate::action::ExportVideoParams;
use crate::daemon::{blocking, lock_library, Daemon, Inner};
use crate::error::DaemonError;
use crate::events::{EventBus, CAPTURE_REVISED, EXPORT_DONE, EXPORT_QUEUED};

/// Setting: render the combined MP4 after every recording (default on).
pub const SETTING_AUTO_ON_STOP: &str = "export.auto_on_stop";

/// Whether the settings ask for an automatic export (default true).
pub fn auto_export_enabled(settings: &Map<String, Value>) -> bool {
    settings
        .get(SETTING_AUTO_ON_STOP)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// The preset table in force: the shipped one, or the replacement file named by
/// `CAPTUREMANN_PLATFORM_RULES`. A file that cannot be used is reported on stderr and ignored.
fn presets_in_force() -> Presets {
    let loaded = Presets::load_default();
    if let Some(w) = &loaded.warning {
        log::warn!("{w}");
    }
    loaded.presets
}

/// What an export does with the captions (AI-1): burn them into the picture, or leave them out.
/// Not asking at all (`None` in [`ExportOpts::captions`]) follows the recording's
/// `set_caption_options` (`burn_in`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptionChoice {
    Burn,
    None,
}

impl CaptionChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            CaptionChoice::Burn => "burn",
            CaptionChoice::None => "none",
        }
    }

    pub fn parse(s: &str) -> Option<CaptionChoice> {
        match s {
            "burn" => Some(CaptionChoice::Burn),
            "none" => Some(CaptionChoice::None),
            _ => None,
        }
    }
}

/// The GIF and WebM choices of an `export_video` (EXP-3) and the caption choice (AI-1), as they sit
/// in a job's inputs. The caption choice applies to every preset, `quick` included.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ExportOpts {
    pub width: Option<u32>,
    pub fps: Option<f64>,
    pub max_mb: Option<f64>,
    pub captions: Option<CaptionChoice>,
}

impl ExportOpts {
    pub fn from_params(p: &ExportVideoParams) -> ExportOpts {
        ExportOpts {
            width: p.width,
            fps: p.fps,
            max_mb: p.max_mb,
            captions: p.captions,
        }
    }

    pub fn from_inputs(inputs: &Value) -> ExportOpts {
        ExportOpts {
            width: inputs
                .get("width")
                .and_then(Value::as_u64)
                .and_then(|w| u32::try_from(w).ok()),
            fps: inputs.get("fps").and_then(Value::as_f64),
            max_mb: inputs.get("max_mb").and_then(Value::as_f64),
            captions: inputs
                .get("captions")
                .and_then(Value::as_str)
                .and_then(CaptionChoice::parse),
        }
    }

    /// The GIF and WebM options alone (the caption choice taken out), for the `quick` check.
    fn media_only(self) -> ExportOpts {
        ExportOpts {
            captions: None,
            ..self
        }
    }

    fn to_media(self) -> ExportOptions {
        ExportOptions {
            width: self.width,
            fps: self.fps,
            max_mb: self.max_mb,
        }
    }
}

/// The preset check shared by the action and the job: `quick` takes no options, any other id must
/// be in the preset table in force (the shipped one, or the file named by
/// `CAPTUREMANN_PLATFORM_RULES`, EXP-2.AC4).
pub fn check_preset(preset: &str, opts: &ExportOpts) -> Result<(), CmError> {
    if preset == PRESET_QUICK {
        if opts.media_only() != ExportOpts::default() {
            let key = if opts.width.is_some() {
                "width"
            } else if opts.fps.is_some() {
                "fps"
            } else {
                "max_mb"
            };
            return Err(CmError::invalid_field(
                key,
                "applies to the `gif` and `webm` presets only",
            ));
        }
        return Ok(());
    }
    let table = presets_in_force();
    if table.get(preset).is_some() {
        Ok(())
    } else {
        Err(CmError::invalid_field(
            "preset",
            format!(
                "unknown preset `{preset}`; use one of: {PRESET_QUICK}, {}",
                table.ids().join(", ")
            ),
        ))
    }
}

/// The part of the preset check that needs the recording: a preset it cannot use (EXP-2.AC3, the
/// GIF length) fails now, with the reason, instead of in the background. `publish_video` with an
/// `export_preset` runs the same check before its dialog.
pub(crate) fn check_preset_for_recording(
    library: &Arc<Mutex<Library>>,
    capture_id: &str,
    preset: &str,
    opts: ExportOpts,
) -> Result<(), CmError> {
    if preset == PRESET_QUICK {
        return Ok(());
    }
    let (_, _, manifest) = load_recording(library, capture_id)?;
    let table = presets_in_force();
    cm_media::check_request(
        &manifest,
        &PresetRequest {
            presets: &table,
            id: preset,
            options: opts.to_media(),
        },
    )?;
    Ok(())
}

/// Enqueues an export job unless an equal one (same capture and preset) is already queued,
/// running or paused, in which case that job is returned (`true` = reused). Publishes
/// `export.queued` (with the capture's title) for a new job.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enqueue_export(
    queue: &JobQueue,
    events: &EventBus,
    capture_id: &str,
    title: &str,
    preset: &str,
    loudness_lufs: Option<f64>,
    opts: ExportOpts,
    by: &str,
    auto: bool,
) -> Result<(Job, bool), CmError> {
    // R2-14: list-then-enqueue is one step per process, so two callers (an `export_video`
    // and the auto-export, or two agents) cannot both see "none" and create two jobs for the
    // same capture and preset.
    static ENQUEUE_LOCK: Mutex<()> = Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_EXPORT.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    if let Some(j) = active.into_iter().find(|j| {
        j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id)
            && j.inputs.get("preset").and_then(Value::as_str) == Some(preset)
            && j.inputs.get("loudness_lufs").and_then(Value::as_f64) == loudness_lufs
            && ExportOpts::from_inputs(&j.inputs) == opts
    }) {
        return Ok((j, true));
    }
    let job = queue.enqueue(NewJob::new(
        JOB_EXPORT,
        json!({ "capture_id": capture_id, "preset": preset, "loudness_lufs": loudness_lufs,
                "width": opts.width, "fps": opts.fps, "max_mb": opts.max_mb,
                "captions": opts.captions.map(CaptionChoice::as_str),
                "by": by, "auto": auto }),
    ))?;
    events.publish(
        EXPORT_QUEUED,
        json!({ "job_id": job.id, "capture_id": capture_id, "title": title, "preset": preset,
                "loudness_lufs": loudness_lufs, "auto": auto }),
    );
    Ok((job, false))
}

// ---- the layout at stop ------------------------------------------------------------------------

const CAMERA_KEYS: [(&str, &str); 4] = [
    ("camera.shape", "shape"),
    ("camera.size", "size"),
    ("camera.position", "position"),
    ("camera.mirror", "mirror"),
];

/// The `camera.*` settings, to compare at stop with what they were at start.
pub(crate) fn camera_settings(all: &Map<String, Value>) -> Map<String, Value> {
    CAMERA_KEYS
        .iter()
        .filter_map(|(k, _)| all.get(*k).map(|v| ((*k).to_owned(), v.clone())))
        .collect()
}

/// `layout.camera` for the manifest, from the recording's bubble facts (`indicator.camera` /
/// `indicator.area`) with any `camera.*` setting that **changed since the start** taking over (the
/// desktop's drag, size, shape and mirror controls write those). `None` without a camera.
pub fn layout_at_stop(
    indicator: &Value,
    at_start: &Map<String, Value>,
    now: &Map<String, Value>,
) -> Option<Value> {
    let cam = indicator.get("camera").filter(|c| c.is_object())?;
    let pick = |key: &str, field: &str| -> Option<Value> {
        let current = now.get(key);
        if current.is_some() && current != at_start.get(key) {
            current.cloned()
        } else {
            cam.get(field).cloned()
        }
    };
    let shape = pick("camera.shape", "shape")?;
    let size = pick("camera.size", "size")?;
    let position = pick("camera.position", "position")?;
    let mirror = pick("camera.mirror", "mirror").and_then(|v| v.as_bool());
    let area = indicator.get("area")?;
    let src = area.get("source_size")?.as_array()?;
    let scale = area
        .get("scale")
        .and_then(Value::as_f64)
        .filter(|s| *s >= 1.0)
        .unwrap_or(1.0);
    let (w, h) = (src.first()?.as_f64()?, src.get(1)?.as_f64()?);
    let layout = cm_media::layout::camera_layout(
        shape.as_str()?,
        size.as_str()?,
        &position,
        mirror,
        (w / scale, h / scale),
    );
    Some(json!({ "camera": layout.to_json() }))
}

// ---- the job handler -----------------------------------------------------------------------

/// How the handler finds FFmpeg (tests inject a fixed one).
pub type ToolsFn = Arc<dyn Fn() -> Result<Tools, CmError> + Send + Sync>;

/// Runs `export` jobs with `cm-media`.
pub struct ExportHandler {
    pub(crate) library: Arc<Mutex<Library>>,
    pub(crate) events: EventBus,
    pub(crate) tools: ToolsFn,
}

impl ExportHandler {
    pub(crate) fn new(library: Arc<Mutex<Library>>, events: EventBus, tools: ToolsFn) -> Self {
        ExportHandler {
            library,
            events,
            tools,
        }
    }
}

/// The recording's index row, bundle folder and manifest as JSON.
fn load_recording(
    library: &Arc<Mutex<Library>>,
    capture_id: &str,
) -> Result<(CaptureRecord, std::path::PathBuf, Value), CmError> {
    let (record, dir) = {
        let lib = lock_library(library);
        let r = lib.get(capture_id)?;
        let d = lib.bundle_path(&r);
        (r, d)
    };
    if record.kind != CaptureKind::Recording {
        return Err(CmError::invalid_input(format!(
            "`{capture_id}` is a screenshot; only recordings can be exported as video"
        )));
    }
    if !dir.is_dir() {
        return Err(CmError::not_found(format!(
            "The recording folder `{}` was moved or deleted",
            dir.display()
        )));
    }
    let manifest = cm_library::read_manifest(&dir)?;
    let value =
        serde_json::to_value(&manifest).map_err(|e| CmError::internal(format!("manifest: {e}")))?;
    Ok((record, dir, value))
}

fn report(reporter: &ProgressReporter, p: f64, last: &mut f64) {
    if p >= 1.0 || p - *last >= 0.01 {
        *last = p;
        reporter.report(
            p,
            Some(format!("Exporting {}%", (p * 100.0).floor() as u32)),
        );
    }
}

#[async_trait]
impl JobHandler for ExportHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let inputs = ctx.job.inputs.clone();
        // VOC-2: an audio export is an `export` job with `kind: "audio"`.
        if inputs.get("kind").and_then(Value::as_str) == Some(crate::export_audio::AUDIO_KIND) {
            return crate::export_audio::run_job(self, ctx).await;
        }
        let capture_id = inputs
            .get("capture_id")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("capture_id", "is required"))?
            .to_owned();
        let preset = inputs
            .get("preset")
            .and_then(Value::as_str)
            .unwrap_or(PRESET_QUICK)
            .to_owned();
        let opts = ExportOpts::from_inputs(&inputs);
        check_preset(&preset, &opts)?;
        let loudness_lufs = inputs.get("loudness_lufs").and_then(Value::as_f64);
        let by: Actor = inputs
            .get("by")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
            .unwrap_or(Actor::User);

        let (record, dir, manifest) = {
            let (library, id) = (self.library.clone(), capture_id.clone());
            blocking(move || load_recording(&library, &id))
                .await
                .map_err(daemon_to_cm)?
        };
        // Captions (AI-1): the choice of the request, else the recording's `burn_in` switch. A
        // `burn` that asks for them by name fails without a transcript; the recording's default
        // alone never blocks an export.
        let wants_captions = crate::caption_options::wants_burn_in(opts.captions, &manifest);
        let caption_source = if wants_captions {
            crate::caption_options::burn_in_source(
                &capture_id,
                &dir,
                &manifest,
                opts.captions == Some(CaptionChoice::Burn),
            )?
        } else {
            None
        };
        let captions_result = match (wants_captions, caption_source.is_some()) {
            (false, _) => "off",
            (true, true) => "burned",
            (true, false) => "skipped_no_transcript",
        };
        // FFmpeg missing: `not_found` with the install hint. Not retryable, so the job fails once.
        let tools = (self.tools)()?;

        // A pause (a recording started), cancel or shutdown stops FFmpeg and deletes the partial
        // file; the queue decides the final state from the interrupt kind.
        let flag = Arc::new(AtomicBool::new(false));
        let watcher = {
            let mut token = ctx.cancel.clone();
            let flag = flag.clone();
            tokio::spawn(async move {
                token.interrupted().await;
                flag.store(true, Ordering::SeqCst);
            })
        };
        let reporter = ctx.reporter.clone();
        let run_flag = flag.clone();
        let preset_id = preset.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let mut last = -1.0;
            // The table is read when the export starts, so an updated rules file applies to it.
            let table = presets_in_force();
            let preset_request = (preset_id != PRESET_QUICK).then(|| PresetRequest {
                presets: &table,
                id: &preset_id,
                options: opts.to_media(),
            });
            export_quick(
                &tools,
                &QuickRequest {
                    bundle_dir: &dir,
                    manifest: &manifest,
                    prefer_encoder: None,
                    path: cm_media::ExportPath::Auto,
                    loudness_lufs,
                    preset: preset_request,
                    captions: caption_source,
                },
                &mut |p| report(&reporter, p, &mut last),
                &|| run_flag.load(Ordering::SeqCst),
            )
            .map(|o| (o, dir))
        })
        .await;
        watcher.abort();
        let (outcome, dir) = match joined {
            Ok(Ok(x)) => x,
            Ok(Err(ExportError::Cancelled)) => {
                return Err(CmError::internal("The export was interrupted"))
            }
            Ok(Err(ExportError::Failed(e))) => return Err(e),
            Err(e) => return Err(CmError::internal(format!("export worker failed: {e}"))),
        };

        // Record the export in the manifest and the index, then tell clients.
        let created = self
            .events
            .now()
            .format(&Rfc3339)
            .map_err(|e| CmError::internal(e.to_string()))?;
        let entry = outcome.to_entry(&created);
        let now = self.events.now();
        let (library, manifest_dir, who) = (self.library.clone(), dir, by.clone());
        let revision = blocking(move || {
            let m = cm_library::record_export(&manifest_dir, entry, who, now)?;
            // The bundle grew: refresh the index row (size).
            lock_library(&library).index_dir(&manifest_dir)?;
            Ok(m.revisions.last().map_or(0, |r| r.rev))
        })
        .await
        .map_err(daemon_to_cm)?;

        let duration_s = outcome.duration_ms as f64 / 1000.0;
        let result = json!({
            "capture_id": capture_id,
            "preset": preset,
            "file": outcome.file,
            "path": outcome.path,
            "size_bytes": outcome.size_bytes,
            "duration_s": duration_s,
            "duration_ms": outcome.duration_ms,
            "encoder": outcome.encoder,
            "captions": captions_result,
            "elapsed_ms": outcome.elapsed_ms,
        });
        let mut done = result.clone();
        if let Some(o) = done.as_object_mut() {
            o.insert("job_id".into(), json!(ctx.job.id));
            o.insert("title".into(), json!(record.title));
        }
        self.events.publish(EXPORT_DONE, done);
        self.events.publish(
            CAPTURE_REVISED,
            json!({ "capture_id": capture_id, "revision": revision, "by": by }),
        );
        Ok(result)
    }

    async fn on_cancel(&self, _job: &Job) {
        // `export_quick` already deleted the partial file and its work folder.
    }
}

fn daemon_to_cm(e: DaemonError) -> CmError {
    match e {
        DaemonError::Cm(c) => c,
        other => CmError::internal(other.to_string()),
    }
}

impl Daemon {
    /// The `export` job handler on this daemon's library and event bus, finding FFmpeg with
    /// [`cm_media::tools::locate`].
    pub fn export_handler(&self) -> Arc<dyn JobHandler> {
        self.export_handler_with(Arc::new(locate))
    }

    /// [`Daemon::export_handler`] with a custom FFmpeg lookup (tests).
    pub fn export_handler_with(&self, tools: ToolsFn) -> Arc<dyn JobHandler> {
        Arc::new(ExportHandler::new(
            self.inner.library.clone(),
            self.inner.events.clone(),
            tools,
        ))
    }

    /// Registers the quick exporter for the job type `export`. Call it at startup, before
    /// [`Daemon::start_jobs`], in every process that owns a daemon (the desktop app and the
    /// standalone `cm-daemon`). Errors with `not_implemented` without a job queue.
    pub fn register_export_handler(&self) -> Result<(), DaemonError> {
        self.register_job_handler(JOB_EXPORT, self.export_handler())
    }
}

impl Inner {
    /// `export_video`: validates, then enqueues (or reuses) the export job.
    pub(crate) async fn export_video(
        &self,
        actor: &Actor,
        p: ExportVideoParams,
    ) -> Result<Value, DaemonError> {
        let opts = ExportOpts::from_params(&p);
        check_preset(&p.preset, &opts)?;
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_EXPORT) {
            return Err(DaemonError::NotImplemented(
                "export is not available: this daemon has no exporter registered".to_owned(),
            ));
        }
        let library = self.library.clone();
        let id = p.capture_id.clone();
        let preset_id = p.preset.clone();
        let burn_requested = p.captions == Some(CaptionChoice::Burn);
        let title = blocking(move || {
            let r = lock_library(&library).get(&id)?;
            if r.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; only recordings can be exported as video"
                )));
            }
            // A preset the recording cannot use (EXP-2.AC3, the GIF length) fails now, with the
            // reason, instead of in the background.
            check_preset_for_recording(&library, &id, &preset_id, opts)?;
            // `captions: "burn"` asked for them by name: no transcript fails now, with the way to
            // get one, instead of exporting a video without captions.
            if burn_requested {
                let (_, dir, manifest) = load_recording(&library, &id)?;
                crate::caption_options::burn_in_source(&id, &dir, &manifest, true)?;
            }
            Ok(r.title)
        })
        .await?;
        let (events, by) = (self.events.clone(), actor.to_string());
        let (capture_id, preset, loudness_lufs) = (p.capture_id, p.preset, p.loudness_lufs);
        let q = queue.clone();
        let store = self.destinations.clone();
        let (job, reused) = blocking(move || {
            let (job, reused) = enqueue_export(
                &q,
                &events,
                &capture_id,
                &title,
                &preset,
                loudness_lufs,
                opts,
                &by,
                false,
            )?;
            // STO-3: destinations set to "after export" get an upload that waits for this job.
            if let Some(store) = &store {
                if let Err(e) = crate::auto_upload::enqueue(
                    &q,
                    store,
                    &events,
                    &capture_id,
                    &title,
                    &by,
                    cm_storage::AutoUpload::OnExport,
                    &crate::auto_upload::ExportState::Job(&job),
                ) {
                    log::warn!("automatic upload not queued: {e}");
                }
            }
            Ok((job, reused))
        })
        .await?;
        Ok(json!({
            "job_id": job.id,
            "capture_id": job.inputs.get("capture_id"),
            "preset": job.inputs.get("preset"),
            "loudness_lufs": job.inputs.get("loudness_lufs"),
            "captions": job.inputs.get("captions"),
            "state": job.state.as_str(),
            "reused": reused,
        }))
    }
}

#[cfg(test)]
mod layout_at_stop_tests {
    use super::*;

    fn indicator() -> Value {
        json!({ "area": { "source_size": [2000, 1200], "scale": 2.0 },
                "camera": { "shape": "circle", "size": "m", "position": "bottom-left",
                            "mirror": true } })
    }

    fn rect_of(l: &Value) -> Vec<f64> {
        l["camera"]["rect"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default()
    }

    /// CAM-1: the start position (bottom-left, 24 pt margin) is used when nothing was dragged; a
    /// changed `camera.position` (the drag) wins.
    #[test]
    fn dragged_position_wins_over_the_default() {
        let none = Map::new();
        let l = layout_at_stop(&indicator(), &none, &none).unwrap_or_default();
        let r = rect_of(&l);
        assert!((r[0] - 24.0 / 1000.0).abs() < 1e-3 && (r[1] - 336.0 / 600.0).abs() < 1e-3);

        let at_start: Map<String, Value> = [("camera.position".to_owned(), json!("bottom-right"))]
            .into_iter()
            .collect();
        // Unchanged stored value (an older drag): the start position stays.
        let same = layout_at_stop(&indicator(), &at_start, &at_start).unwrap_or_default();
        assert_eq!(rect_of(&same), r);
        // Dragged during the recording.
        let now: Map<String, Value> = [("camera.position".to_owned(), json!([0.5, 0.5]))]
            .into_iter()
            .collect();
        let l = layout_at_stop(&indicator(), &at_start, &now).unwrap_or_default();
        let d = rect_of(&l);
        assert!(
            (d[0] - 0.38).abs() < 1e-3 && (d[1] - 0.3).abs() < 1e-3,
            "{d:?}"
        );
    }
}
