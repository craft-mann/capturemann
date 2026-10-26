//! `export_audio` (VOC-2): the audio of a capture, with its edits applied, as M4A or WAV.
//!
//! The action `export_audio { capture_id, format?, tracks?, loudness_lufs?, out_dir?, request_id? }`
//! -> `{ job_id }` (Free, mutating, idempotent by `request_id`) enqueues an `export` job whose
//! inputs carry `kind: "audio"`; [`crate::export::ExportHandler`] hands such a job to
//! [`run_job`]. It is the same job type as the video export, so it runs one at a time, is held
//! while a recording runs (EXP-4.AC2) and reports `job.progress`, `export.queued` and
//! `export.done`. The result is `{ path, size_bytes, duration_s, ... }`.
//!
//! Which audio there is comes from the files in the bundle (`mic.denoised.wav` or `mic.wav`,
//! `system.wav`), not from the capture kind, so a screen recording ("Export audio only") and an
//! audio recording take the same path. The edit list (trim, cuts, speed, pauses, gains) is applied
//! to every audio track together by `cm_media::audio_export`.
//!
//! The file lands in the bundle's `exports/` and is recorded in `manifest.exports` with
//! `kind: "audio"`, so the video-only readers (`latest_export` for upload, the player) skip it.
//! With `out_dir` (an existing absolute folder) a copy is made there under a name that does not
//! overwrite anything.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cm_core::{Actor, CmError};
use cm_jobs::{Job, JobContext, JobFilter, JobQueue, JobState, NewJob, JOB_EXPORT};
use cm_library::manifest::CaptureKind;
use cm_media::audio_export::{
    export_audio, plan_audio, AudioFormat, AudioOptions, AudioOutcome, TrackChoice,
};
use serde::{Serialize, Serializer};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;

use crate::action::{Action, Fields, LOUDNESS_LUFS_RANGE};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;
use crate::events::{CAPTURE_REVISED, EXPORT_DONE, EXPORT_QUEUED};
use crate::export::ExportHandler;

/// The `kind` value in the inputs of an audio export job.
pub const AUDIO_KIND: &str = "audio";

fn ser_format<S: Serializer>(f: &AudioFormat, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(f.as_str())
}

fn ser_tracks<S: Serializer>(t: &TrackChoice, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(t.as_str())
}

/// `export_audio` input. `loudness_lufs` is already resolved: the key left out means -16 for M4A
/// and off for WAV; `null` means off; a number is the target.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportAudioParams {
    pub capture_id: String,
    #[serde(serialize_with = "ser_format")]
    pub format: AudioFormat,
    #[serde(serialize_with = "ser_tracks")]
    pub tracks: TrackChoice,
    pub loudness_lufs: Option<f64>,
    pub out_dir: Option<String>,
    pub request_id: Option<String>,
}

impl ExportAudioParams {
    fn options(&self) -> AudioOptions {
        AudioOptions {
            format: self.format,
            tracks: self.tracks,
            loudness_lufs: self.loudness_lufs,
        }
    }
}

/// Parses the input of `export_audio`.
pub(crate) fn parse(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&[
        "capture_id",
        "format",
        "tracks",
        "loudness_lufs",
        "out_dir",
        "request_id",
    ])?;
    let format = match f.str_opt("format")?.as_deref() {
        None => AudioFormat::M4a,
        Some(s) => AudioFormat::parse(s)
            .ok_or_else(|| CmError::invalid_field("format", "must be `m4a` or `wav`"))?,
    };
    let tracks = match f.str_opt("tracks")?.as_deref() {
        None => TrackChoice::Mix,
        Some(s) => TrackChoice::parse(s)
            .ok_or_else(|| CmError::invalid_field("tracks", "must be `mix`, `mic` or `system`"))?,
    };
    let loudness_lufs = match f.obj.get("loudness_lufs") {
        None => AudioOptions::default_loudness(format),
        Some(Value::Null) => None,
        Some(v) => {
            let n = v.as_f64().filter(|n| {
                n.is_finite() && (LOUDNESS_LUFS_RANGE.0..=LOUDNESS_LUFS_RANGE.1).contains(n)
            });
            Some(n.ok_or_else(|| {
                CmError::invalid_field(
                    "loudness_lufs",
                    format!(
                        "must be a number from {} to {} (LUFS), or null for no normalisation",
                        LOUDNESS_LUFS_RANGE.0, LOUDNESS_LUFS_RANGE.1
                    ),
                )
            })?)
        }
    };
    let out_dir = f.str_opt("out_dir")?;
    if let Some(d) = &out_dir {
        if d.trim().is_empty() {
            return Err(CmError::invalid_field("out_dir", "must not be empty"));
        }
    }
    Ok(Action::ExportAudio(ExportAudioParams {
        capture_id: f.str_req("capture_id")?,
        format,
        tracks,
        loudness_lufs,
        out_dir,
        request_id: f.request_id()?,
    }))
}

pub(crate) fn summary(p: &ExportAudioParams) -> String {
    format!(
        "Export the audio of {} as {} ({})",
        p.capture_id,
        p.format.as_str().to_uppercase(),
        p.tracks.as_str()
    )
}

/// The index row's bundle folder and manifest as JSON; refuses a screenshot.
fn load_bundle(
    library: &Arc<Mutex<cm_library::Library>>,
    capture_id: &str,
) -> Result<(cm_library::index::CaptureRecord, PathBuf, Value), CmError> {
    let (record, dir) = {
        let lib = lock_library(library);
        let r = lib.get(capture_id)?;
        let d = lib.bundle_path(&r);
        (r, d)
    };
    if record.kind == CaptureKind::Screenshot {
        return Err(CmError::invalid_input(format!(
            "`{capture_id}` is a screenshot; it has no audio to export"
        )));
    }
    if !dir.is_dir() {
        return Err(CmError::not_found(format!(
            "The capture folder `{}` was moved or deleted",
            dir.display()
        )));
    }
    let manifest = cm_library::read_manifest(&dir)?;
    let value =
        serde_json::to_value(&manifest).map_err(|e| CmError::internal(format!("manifest: {e}")))?;
    Ok((record, dir, value))
}

fn preset_of(tracks: TrackChoice) -> String {
    format!("audio-{}", tracks.as_str())
}

/// Enqueues an audio export unless an equal one is already queued, running or paused.
fn enqueue(
    queue: &JobQueue,
    events: &crate::events::EventBus,
    p: &ExportAudioParams,
    title: &str,
    by: &str,
) -> Result<(Job, bool), CmError> {
    static ENQUEUE_LOCK: Mutex<()> = Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_EXPORT.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    let inputs = |j: &Job| {
        let i = &j.inputs;
        i.get("kind").and_then(Value::as_str) == Some(AUDIO_KIND)
            && i.get("capture_id").and_then(Value::as_str) == Some(p.capture_id.as_str())
            && i.get("format").and_then(Value::as_str) == Some(p.format.as_str())
            && i.get("tracks").and_then(Value::as_str) == Some(p.tracks.as_str())
            && i.get("loudness_lufs").and_then(Value::as_f64) == p.loudness_lufs
            && i.get("out_dir").and_then(Value::as_str) == p.out_dir.as_deref()
    };
    if let Some(j) = active.into_iter().find(inputs) {
        return Ok((j, true));
    }
    let preset = preset_of(p.tracks);
    let job = queue.enqueue(NewJob::new(
        JOB_EXPORT,
        json!({ "kind": AUDIO_KIND, "capture_id": p.capture_id, "preset": preset,
                "format": p.format.as_str(), "tracks": p.tracks.as_str(),
                "loudness_lufs": p.loudness_lufs, "out_dir": p.out_dir,
                "by": by, "auto": false }),
    ))?;
    events.publish(
        EXPORT_QUEUED,
        json!({ "job_id": job.id, "capture_id": p.capture_id, "title": title,
                "preset": preset, "kind": AUDIO_KIND, "loudness_lufs": p.loudness_lufs,
                "auto": false }),
    );
    Ok((job, false))
}

impl Inner {
    /// `export_audio`: checks that there is audio to export, then enqueues (or reuses) the job.
    pub(crate) async fn export_audio(
        &self,
        actor: &Actor,
        p: ExportAudioParams,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_EXPORT) {
            return Err(DaemonError::NotImplemented(
                "export is not available: this daemon has no exporter registered".to_owned(),
            ));
        }
        if let Some(d) = &p.out_dir {
            if !Path::new(d).is_absolute() || !Path::new(d).is_dir() {
                return Err(DaemonError::from(CmError::invalid_field(
                    "out_dir",
                    format!("`{d}` must be the absolute path of an existing folder"),
                )));
            }
        }
        let (library, id, opts) = (self.library.clone(), p.capture_id.clone(), p.options());
        let title = blocking(move || {
            let (record, dir, manifest) = load_bundle(&library, &id)?;
            // Fails now, with the reason (no audio track, nothing left after the cuts), instead
            // of in the background.
            plan_audio(&dir, &manifest, &opts, true)?;
            Ok(record.title)
        })
        .await?;
        let (events, by) = (self.events.clone(), actor.to_string());
        let pp = p.clone();
        let (job, reused) = blocking(move || enqueue(&queue, &events, &pp, &title, &by)).await?;
        Ok(json!({
            "job_id": job.id,
            "capture_id": p.capture_id,
            "kind": AUDIO_KIND,
            "format": p.format.as_str(),
            "tracks": p.tracks.as_str(),
            "loudness_lufs": p.loudness_lufs,
            "out_dir": p.out_dir,
            "state": job.state.as_str(),
            "reused": reused,
        }))
    }
}

/// A free name in `dir`: `name`, else `stem (2).ext`, `stem (3).ext`, ... Never overwrites.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned());
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..10_000)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .unwrap_or(first)
}

/// Copies the export to `out_dir`; returns the copy's path.
fn copy_out(out: &AudioOutcome, out_dir: &str) -> Result<PathBuf, CmError> {
    let dir = Path::new(out_dir);
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(CmError::invalid_field(
            "out_dir",
            format!("`{out_dir}` is not an existing folder"),
        ));
    }
    let name = out
        .path
        .file_name()
        .map_or_else(|| out.file.clone(), |n| n.to_string_lossy().into_owned());
    let target = free_name(dir, &name);
    std::fs::copy(&out.path, &target).map_err(|e| {
        if e.raw_os_error() == Some(28) {
            CmError::disk_full("The disk is full, so the audio could not be copied.")
        } else {
            CmError::internal(format!("cannot copy to `{}`: {e}", target.display()))
        }
    })?;
    Ok(target)
}

fn report(reporter: &cm_jobs::ProgressReporter, p: f64, last: &mut f64) {
    if p >= 1.0 || p - *last >= 0.01 {
        *last = p;
        reporter.report(
            p,
            Some(format!("Exporting audio {}%", (p * 100.0).floor() as u32)),
        );
    }
}

/// Runs an audio export job (called by the `export` job handler).
pub(crate) async fn run_job(h: &ExportHandler, ctx: JobContext) -> Result<Value, CmError> {
    let inputs = ctx.job.inputs.clone();
    let capture_id = inputs
        .get("capture_id")
        .and_then(Value::as_str)
        .ok_or_else(|| CmError::invalid_field("capture_id", "is required"))?
        .to_owned();
    let opts = AudioOptions {
        format: inputs
            .get("format")
            .and_then(Value::as_str)
            .and_then(AudioFormat::parse)
            .unwrap_or_default(),
        tracks: inputs
            .get("tracks")
            .and_then(Value::as_str)
            .and_then(TrackChoice::parse)
            .unwrap_or_default(),
        loudness_lufs: inputs.get("loudness_lufs").and_then(Value::as_f64),
    };
    let out_dir = inputs
        .get("out_dir")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let by: Actor = inputs
        .get("by")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .unwrap_or(Actor::User);

    let (record, dir, manifest) = {
        let (library, id) = (h.library.clone(), capture_id.clone());
        blocking(move || load_bundle(&library, &id))
            .await
            .map_err(daemon_to_cm)?
    };
    let tools = (h.tools)()?;

    // A pause (a recording started), cancel or shutdown stops FFmpeg; the queue decides the final
    // state from the interrupt kind.
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
    let joined = tokio::task::spawn_blocking(move || {
        let mut last = -1.0;
        export_audio(
            &tools,
            &dir,
            &manifest,
            &opts,
            &mut |p| report(&reporter, p, &mut last),
            &|| run_flag.load(Ordering::SeqCst),
        )
        .map(|o| (o, dir))
    })
    .await;
    watcher.abort();
    let (outcome, dir) = match joined {
        Ok(Ok(x)) => x,
        Ok(Err(cm_media::ExportError::Cancelled)) => {
            return Err(CmError::internal("The export was interrupted"))
        }
        Ok(Err(cm_media::ExportError::Failed(e))) => return Err(e),
        Err(e) => return Err(CmError::internal(format!("export worker failed: {e}"))),
    };

    let copy_path = match &out_dir {
        Some(d) => Some(copy_out(&outcome, d)?),
        None => None,
    };

    // Record the export in the manifest and the index, then tell clients.
    let created = h
        .events
        .now()
        .format(&Rfc3339)
        .map_err(|e| CmError::internal(e.to_string()))?;
    let mut entry = outcome.to_entry(&created);
    if let (Some(o), Some(c)) = (entry.as_object_mut(), &copy_path) {
        o.insert("copy".into(), json!(c));
    }
    let now = h.events.now();
    let (library, manifest_dir, who) = (h.library.clone(), dir, by.clone());
    let revision = blocking(move || {
        let m = cm_library::record_export(&manifest_dir, entry, who, now)?;
        lock_library(&library).index_dir(&manifest_dir)?;
        Ok(m.revisions.last().map_or(0, |r| r.rev))
    })
    .await
    .map_err(daemon_to_cm)?;

    let result = json!({
        "capture_id": capture_id,
        "kind": AUDIO_KIND,
        "preset": preset_of(outcome.tracks),
        "file": outcome.file,
        "path": outcome.path,
        "size_bytes": outcome.size_bytes,
        "duration_s": outcome.duration_s,
        "format": outcome.format.as_str(),
        "tracks": outcome.tracks.as_str(),
        "loudness_lufs": outcome.loudness_lufs,
        "loudness_mode": outcome.loudness_mode,
        "copy_path": copy_path,
    });
    let mut done = result.clone();
    if let Some(o) = done.as_object_mut() {
        o.insert("job_id".into(), json!(ctx.job.id));
        o.insert("title".into(), json!(record.title));
    }
    h.events.publish(EXPORT_DONE, done);
    h.events.publish(
        CAPTURE_REVISED,
        json!({ "capture_id": capture_id, "revision": revision, "by": by }),
    );
    Ok(result)
}

fn daemon_to_cm(e: DaemonError) -> CmError {
    match e {
        DaemonError::Cm(c) => c,
        other => CmError::internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn parse_json(v: Value) -> Result<Action, CmError> {
        let empty = Map::new();
        let f = Fields::new(&v, &empty)?;
        parse(&f)
    }

    fn params(v: Value) -> ExportAudioParams {
        match parse_json(v).unwrap() {
            Action::ExportAudio(p) => p,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn defaults_are_m4a_mix_at_minus_16() {
        let p = params(json!({"capture_id": "rec_1"}));
        assert_eq!(p.format, AudioFormat::M4a);
        assert_eq!(p.tracks, TrackChoice::Mix);
        assert_eq!(p.loudness_lufs, Some(-16.0));
        assert_eq!(p.out_dir, None);
    }

    #[test]
    fn wav_defaults_to_no_loudness_and_null_turns_it_off() {
        assert_eq!(
            params(json!({"capture_id": "r", "format": "wav"})).loudness_lufs,
            None
        );
        assert_eq!(
            params(json!({"capture_id": "r", "loudness_lufs": null})).loudness_lufs,
            None
        );
        assert_eq!(
            params(json!({"capture_id": "r", "format": "wav", "loudness_lufs": -18})).loudness_lufs,
            Some(-18.0)
        );
    }

    #[test]
    fn bad_input_names_the_field() {
        for (v, field) in [
            (json!({}), "capture_id"),
            (json!({"capture_id": "r", "format": "mp3"}), "format"),
            (json!({"capture_id": "r", "tracks": "both"}), "tracks"),
            (
                json!({"capture_id": "r", "loudness_lufs": -60}),
                "loudness_lufs",
            ),
            (
                json!({"capture_id": "r", "loudness_lufs": "x"}),
                "loudness_lufs",
            ),
            (json!({"capture_id": "r", "out_dir": ""}), "out_dir"),
            (json!({"capture_id": "r", "bogus": 1}), "bogus"),
        ] {
            let e = parse_json(v).unwrap_err();
            assert_eq!(e.code().as_str(), "invalid_input");
            assert!(e.to_string().contains(field), "{e}");
        }
    }

    #[test]
    fn copies_never_overwrite() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a - audio.m4a"), b"x").unwrap();
        assert_eq!(
            free_name(d.path(), "a - audio.m4a"),
            d.path().join("a - audio (2).m4a")
        );
        assert_eq!(free_name(d.path(), "b.wav"), d.path().join("b.wav"));
    }
}
