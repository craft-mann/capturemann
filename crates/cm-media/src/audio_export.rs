//! Audio export (VOC-2): the recording's audio tracks with the edit list applied (trim, cuts,
//! speed, pauses), mixed or isolated, loudness-normalised and written as M4A or WAV. No video is
//! read or encoded, so the same code serves a `kind: "audio"` bundle and "Export audio only" of a
//! screen recording.
//!
//! * [`plan_audio`] reads the manifest and the bundle folder (which audio files exist decides what
//!   there is to export; it does not look at the capture kind) and builds an [`AudioPlan`]: the
//!   FFmpeg inputs, the cut/stretch/mix graph from [`crate::edit_plan::audio_graph`] (the very
//!   code the video export uses, so both cut identically) and the output length.
//! * [`measure_args`] and [`encode_args`] turn a plan into FFmpeg argument lists. They are pure and
//!   unit tested without FFmpeg.
//! * [`export_audio`] runs them: **two-pass `loudnorm`** (pass 1 measures the edited mix, pass 2
//!   applies the measured values in linear mode, so speech keeps its dynamics and the result lands
//!   within a fraction of a LU of the target), then writes through a `.part` file and renames.
//!   When the measurement cannot be read (silence, an FFmpeg without JSON output) the second
//!   pass falls back to the single-pass dynamic `loudnorm`.
//!
//! Mixing is `amix` with `normalize=0` (a plain sum, followed by `alimiter` at 0.97 so a loud sum
//! cannot clip; the loudness stage then sets the level), or the FFmpeg default when `normalize`
//! is unavailable. M4A is AAC-LC 48 kHz, 128 kbps mono or 192 kbps stereo; WAV is 48 kHz 24-bit.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cm_core::{CmError, EditContext, EditList, MediaTime, PauseGap};
use serde_json::{json, Value};

use crate::edit_plan::{audio_graph, secs_of, AudioKind, AudioProcessor, AudioSource};
use crate::graph::{secs, AUDIO_BPS_MONO, AUDIO_BPS_STEREO, AUDIO_RATE};
use crate::quick::{file_stem, io_err, sweep_stale, Cleanup, ExportError, EXPORTS_DIR};
use crate::run::{failure_error, run_ffmpeg, RunError};
use crate::timeline::gaps_from_json;
use crate::tools::{probe_caps, Tools};

/// The default loudness target (LUFS) of an audio export.
pub const DEFAULT_LOUDNESS_LUFS: f64 = -16.0;
/// True-peak ceiling (dBTP) and loudness range handed to `loudnorm`.
const TRUE_PEAK_DBTP: f64 = -1.5;
const LOUDNESS_RANGE: f64 = 11.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioFormat {
    #[default]
    M4a,
    Wav,
}

impl AudioFormat {
    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::M4a => "m4a",
            AudioFormat::Wav => "wav",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.extension()
    }

    pub fn parse(s: &str) -> Option<AudioFormat> {
        match s {
            "m4a" => Some(AudioFormat::M4a),
            "wav" => Some(AudioFormat::Wav),
            _ => None,
        }
    }

    /// The encoder FFmpeg needs.
    pub fn encoder(self) -> &'static str {
        match self {
            AudioFormat::M4a => "aac",
            AudioFormat::Wav => "pcm_s24le",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackChoice {
    #[default]
    Mix,
    Mic,
    System,
}

impl TrackChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            TrackChoice::Mix => "mix",
            TrackChoice::Mic => "mic",
            TrackChoice::System => "system",
        }
    }

    pub fn parse(s: &str) -> Option<TrackChoice> {
        match s {
            "mix" => Some(TrackChoice::Mix),
            "mic" => Some(TrackChoice::Mic),
            "system" => Some(TrackChoice::System),
            _ => None,
        }
    }
}

/// What to export. `loudness_lufs: None` leaves the level alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioOptions {
    pub format: AudioFormat,
    pub tracks: TrackChoice,
    pub loudness_lufs: Option<f64>,
}

impl Default for AudioOptions {
    fn default() -> Self {
        AudioOptions {
            format: AudioFormat::M4a,
            tracks: TrackChoice::Mix,
            loudness_lufs: Some(DEFAULT_LOUDNESS_LUFS),
        }
    }
}

impl AudioOptions {
    /// The default loudness for a format: on for M4A, off for WAV (an archive copy), unless asked.
    pub fn default_loudness(format: AudioFormat) -> Option<f64> {
        match format {
            AudioFormat::M4a => Some(DEFAULT_LOUDNESS_LUFS),
            AudioFormat::Wav => None,
        }
    }
}

/// Everything decided before FFmpeg runs.
#[derive(Debug, Clone)]
pub struct AudioPlan {
    pub inputs: Vec<PathBuf>,
    /// The `-filter_complex` string ending in `[outa]` (without loudness).
    pub graph: String,
    pub channels: u32,
    /// Output length in seconds.
    pub duration_s: f64,
    pub title: String,
    /// `YYYY-MM-DD` of the recording, for the date tag.
    pub date: Option<String>,
    /// File names (in the bundle) that feed the export.
    pub files: Vec<String>,
}

struct Found {
    kind: AudioKind,
    file: String,
    start: i64,
    channels: u32,
}

fn track_entry(m: &Value, name: &str) -> Option<Value> {
    let t = m.get("tracks")?.get(name)?;
    if t.get("missing").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(t.clone())
}

/// The audio file of a track: the manifest's, adjusted by the `denoise` choice of the edit list,
/// and only when it exists in the bundle. Without a manifest entry the conventional names are
/// tried (`mic.denoised.wav`, `mic.wav`, `system.wav`), so a bundle is recognised by its files.
fn find_track(bundle: &Path, m: &Value, kind: AudioKind, denoise: Option<bool>) -> Option<Found> {
    let (name, default_channels) = match kind {
        AudioKind::Mic => ("mic", 1u32),
        AudioKind::System => ("system", 2u32),
    };
    let entry = track_entry(m, name);
    let mut candidates: Vec<String> = Vec::new();
    if let Some(e) = &entry {
        let file = e.get("file").and_then(Value::as_str).map(str::to_owned);
        let raw = e.get("raw_file").and_then(Value::as_str).map(str::to_owned);
        match (kind, denoise) {
            (AudioKind::Mic, Some(false)) => candidates.extend(raw.clone()),
            (AudioKind::Mic, Some(true)) => candidates.push("mic.denoised.wav".into()),
            _ => {}
        }
        candidates.extend(file);
        candidates.extend(raw);
    }
    match kind {
        AudioKind::Mic => {
            candidates.push("mic.denoised.wav".into());
            candidates.push("mic.wav".into());
        }
        AudioKind::System => candidates.push("system.wav".into()),
    }
    let file = candidates.into_iter().find(|f| {
        !f.contains('/') && !f.contains('\\') && !f.contains("..") && bundle.join(f).is_file()
    })?;
    let t = entry.unwrap_or(Value::Null);
    Some(Found {
        kind,
        file,
        start: t.get("start").and_then(Value::as_i64).unwrap_or(0),
        channels: t
            .get("channels")
            .and_then(Value::as_u64)
            .and_then(|c| u32::try_from(c).ok())
            .filter(|c| (1..=2).contains(c))
            .unwrap_or(default_channels),
    })
}

/// Pause gaps in session time, from the manifest (top level, else a track's own list).
fn session_gaps(m: &Value) -> Vec<PauseGap> {
    let mut g = gaps_from_json(m.get("gaps"));
    if g.is_empty() {
        let tracks = m.get("tracks");
        'outer: for (track, keys) in [
            ("screen", ["pauses", "gaps"]),
            ("cursor", ["pauses", "pause_gaps"]),
            ("mic", ["pauses", "gaps"]),
        ] {
            for key in keys {
                let found =
                    gaps_from_json(tracks.and_then(|t| t.get(track)).and_then(|t| t.get(key)));
                if !found.is_empty() {
                    g = found;
                    break 'outer;
                }
            }
        }
    }
    g.into_iter()
        .map(|(a, b)| PauseGap {
            from: MediaTime::from_ticks(a),
            to: MediaTime::from_ticks(b),
        })
        .collect()
}

/// Gains and mutes of `edits[0].audio` as filters on each track, and `atempo` stretching.
struct MixProcessor {
    mic: Vec<String>,
    system: Vec<String>,
}

impl MixProcessor {
    fn new(a: &cm_core::edit::AudioMix) -> MixProcessor {
        let chain = |gain: f64, mute: bool| {
            if mute {
                vec!["volume=0".to_owned()]
            } else if gain.abs() > 1e-9 {
                vec![format!("volume={}dB", secs(gain))]
            } else {
                Vec::new()
            }
        };
        MixProcessor {
            mic: chain(a.mic_gain_db, a.mic_mute),
            system: chain(a.system_gain_db, a.system_mute),
        }
    }
}

impl AudioProcessor for MixProcessor {
    fn stretch(&self, factor: f64) -> Vec<String> {
        crate::edit_plan::FfmpegAudio.stretch(factor)
    }

    fn track(&self, kind: AudioKind) -> Vec<String> {
        match kind {
            AudioKind::Mic => self.mic.clone(),
            AudioKind::System => self.system.clone(),
        }
    }
}

/// Reads the manifest and the bundle folder and builds the audio graph. `amix_normalize` is
/// `Caps::amix_normalize` of the FFmpeg that will run it (`true` on FFmpeg 4.4+, where the sum is
/// requested with `normalize=0`).
pub fn plan_audio(
    bundle: &Path,
    manifest: &Value,
    opts: &AudioOptions,
    amix_normalize: bool,
) -> Result<AudioPlan, CmError> {
    let gaps = session_gaps(manifest);
    // The end of the recording is the last sample of any track, picture tracks included.
    let end = manifest
        .get("tracks")
        .and_then(Value::as_object)
        .map(|o| {
            o.values()
                .filter_map(|t| t.get("end").and_then(Value::as_i64))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        .max(
            manifest
                .get("duration")
                .and_then(Value::as_i64)
                .unwrap_or(0),
        );
    let cx = EditContext {
        end: MediaTime::from_ticks(end),
        live_at: manifest
            .get("live_at")
            .and_then(Value::as_i64)
            .map(MediaTime::from_ticks),
        gaps: &gaps,
    };
    let entry = manifest.get("edits").and_then(|e| e.get(0));
    let full = EditList::full(&cx);
    let list = entry.map_or_else(|| full.clone(), |e| EditList::from_json(e, &cx));
    let list = list.without_pauses(&gaps);

    let mut found: Vec<Found> = Vec::new();
    let want_mic = matches!(opts.tracks, TrackChoice::Mix | TrackChoice::Mic);
    let want_system = matches!(opts.tracks, TrackChoice::Mix | TrackChoice::System);
    if want_mic {
        found.extend(find_track(
            bundle,
            manifest,
            AudioKind::Mic,
            list.audio.denoise,
        ));
    }
    if want_system {
        found.extend(find_track(bundle, manifest, AudioKind::System, None));
    }
    if found.is_empty() {
        return Err(CmError::invalid_input(match opts.tracks {
            TrackChoice::Mix => "This capture has no audio track to export.",
            TrackChoice::Mic => "This capture has no microphone track to export.",
            TrackChoice::System => "This capture has no system audio track to export.",
        }));
    }

    let tl = list.timeline();
    if tl.duration() < cm_core::MIN_OUTPUT {
        return Err(CmError::invalid_input("Nothing left to export"));
    }
    let channels = found.iter().map(|f| f.channels).max().unwrap_or(1);
    let layout = if channels >= 2 { "stereo" } else { "mono" };
    let mut inputs = Vec::new();
    let mut sources = Vec::new();
    for f in &found {
        inputs.push(bundle.join(&f.file));
        sources.push(AudioSource {
            kind: f.kind,
            input: inputs.len() - 1,
            start: MediaTime::from_ticks(f.start),
        });
    }
    // Isolating a track is not muting the other one: a muted mic still counts when asked for.
    let proc = MixProcessor::new(&list.audio);
    let graph = audio_graph(&tl, &gaps, &sources, layout, amix_normalize, &proc)
        .ok_or_else(|| CmError::invalid_input("This capture has no audio track to export."))?;
    let date = manifest
        .get("created")
        .and_then(Value::as_str)
        .and_then(|c| c.get(..10))
        .filter(|d| d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-')
        .map(str::to_owned);
    Ok(AudioPlan {
        inputs,
        graph,
        channels,
        duration_s: secs_of(tl.duration()),
        title: manifest
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Recording")
            .to_owned(),
        date,
        files: found.into_iter().map(|f| f.file).collect(),
    })
}

/// What pass 1 of `loudnorm` measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub input_i: f64,
    pub input_tp: f64,
    pub input_lra: f64,
    pub input_thresh: f64,
    pub target_offset: f64,
}

/// Reads the JSON block `loudnorm=print_format=json` prints at the end of stderr. `None` when it
/// is missing or any value is not finite (digital silence measures as `-inf`).
pub fn parse_measured(stderr: &str) -> Option<Measured> {
    let end = stderr.rfind('}')?;
    let start = stderr[..end].rfind('{')?;
    let v: Value = serde_json::from_str(&stderr[start..=end]).ok()?;
    let num = |k: &str| -> Option<f64> {
        let x = v.get(k)?;
        let n = x
            .as_f64()
            .or_else(|| x.as_str().and_then(|s| s.trim().parse::<f64>().ok()))?;
        n.is_finite().then_some(n)
    };
    Some(Measured {
        input_i: num("input_i")?,
        input_tp: num("input_tp")?,
        input_lra: num("input_lra")?,
        input_thresh: num("input_thresh")?,
        target_offset: num("target_offset")?,
    })
}

/// The `loudnorm` filter for a target, with pass-1 values when available (linear mode).
pub fn loudnorm_filter(target: f64, measured: Option<&Measured>, json_print: bool) -> String {
    let mut f = format!(
        "loudnorm=I={}:TP={}:LRA={}",
        secs(target),
        secs(TRUE_PEAK_DBTP),
        secs(LOUDNESS_RANGE)
    );
    if let Some(m) = measured {
        f.push_str(&format!(
            ":measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}:linear=true",
            secs(m.input_i),
            secs(m.input_tp),
            secs(m.input_lra),
            secs(m.input_thresh),
            secs(m.target_offset)
        ));
    }
    if json_print {
        f.push_str(":print_format=json");
    }
    f
}

fn input_args(plan: &AudioPlan) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-nostats"]
        .map(String::from)
        .to_vec();
    for p in &plan.inputs {
        args.push("-i".into());
        args.push(p.to_string_lossy().into_owned());
    }
    args
}

/// Pass 1: decode, cut, mix and measure; nothing is written.
pub fn measure_args(plan: &AudioPlan, target: f64) -> Vec<String> {
    let mut args = input_args(plan);
    args.extend(["-loglevel".into(), "info".into()]);
    args.extend([
        "-filter_complex".into(),
        format!(
            "{};[outa]{}[m]",
            plan.graph,
            loudnorm_filter(target, None, true)
        ),
    ]);
    args.extend(["-map", "[m]", "-f", "null", "-"].map(String::from));
    args
}

/// The encoding pass. `loudness` is the target with the pass-1 values, or `None` for no
/// normalisation; `out` is written as `format` whatever its extension.
pub fn encode_args(
    plan: &AudioPlan,
    opts: &AudioOptions,
    measured: Option<&Measured>,
    out: &Path,
) -> Vec<String> {
    let mut args = input_args(plan);
    args.extend(["-loglevel", "warning", "-progress", "pipe:1"].map(String::from));
    let tail = match opts.loudness_lufs {
        // loudnorm resamples to 192 kHz internally: bring it back to 48 kHz after it.
        Some(t) => format!(
            ";[outa]{},aresample={AUDIO_RATE}[fin]",
            loudnorm_filter(t, measured, false)
        ),
        None => String::new(),
    };
    args.extend(["-filter_complex".into(), format!("{}{tail}", plan.graph)]);
    args.extend([
        "-map".into(),
        if opts.loudness_lufs.is_some() {
            "[fin]"
        } else {
            "[outa]"
        }
        .into(),
    ]);
    args.extend(["-map_metadata", "-1"].map(String::from));
    args.extend(["-metadata".into(), format!("title={}", plan.title)]);
    if let Some(d) = &plan.date {
        args.extend(["-metadata".into(), format!("date={d}")]);
    }
    args.extend(
        [
            "-c:a",
            opts.format.encoder(),
            "-ar",
            &AUDIO_RATE.to_string(),
            "-ac",
            &plan.channels.to_string(),
        ]
        .map(String::from),
    );
    match opts.format {
        AudioFormat::M4a => {
            let bps = if plan.channels >= 2 {
                AUDIO_BPS_STEREO
            } else {
                AUDIO_BPS_MONO
            };
            args.extend(["-b:a".into(), bps.to_string()]);
            args.extend(["-movflags", "+faststart", "-f", "mp4"].map(String::from));
        }
        AudioFormat::Wav => args.extend(["-f", "wav"].map(String::from)),
    }
    args.push(out.to_string_lossy().into_owned());
    args
}

/// The result of an audio export.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioOutcome {
    /// Path in the bundle, `exports/<name>` (as the video export records it).
    pub file: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub duration_s: f64,
    pub format: AudioFormat,
    pub tracks: TrackChoice,
    pub loudness_lufs: Option<f64>,
    /// `two_pass`, `single_pass` or `off`.
    pub loudness_mode: &'static str,
}

impl AudioOutcome {
    /// The manifest `exports[]` entry.
    pub fn to_entry(&self, created: &str) -> Value {
        json!({
            "file": self.file,
            "preset": format!("audio-{}", self.tracks.as_str()),
            "kind": "audio",
            "created": created,
            "duration_ms": (self.duration_s * 1000.0).round() as i64,
            "size_bytes": self.size_bytes,
            "settings": {
                "format": self.format.as_str(),
                "tracks": self.tracks.as_str(),
                "loudness_lufs": self.loudness_lufs,
                "loudness_mode": self.loudness_mode,
                "sample_rate": AUDIO_RATE,
            },
        })
    }
}

/// `<title> - audio.m4a` (Mix) or `<title> - audio-mic.m4a`.
pub fn audio_output_name(title: &str, tracks: TrackChoice, ext: &str) -> String {
    let tag = match tracks {
        TrackChoice::Mix => "audio".to_owned(),
        t => format!("audio-{}", t.as_str()),
    };
    format!("{} - {tag}.{ext}", file_stem(title))
}

/// A missing FFmpeg component is a failed job (`job_failed`), not an internal error: the reason
/// names the component. The job id is filled in by the daemon, which knows it.
fn missing_component(reason: impl Into<String>) -> CmError {
    let reason = reason.into();
    CmError::JobFailed {
        message: reason.clone(),
        job_id: String::new(),
        reason,
    }
}

/// Does `ffmpeg` have what this format needs? A `job_failed` error naming the missing component,
/// so a build without the `wav` muxer or `pcm_s24le` says so instead of failing in the middle of
/// the run.
pub fn check_components(tools: &Tools, opts: &AudioOptions) -> Result<(), CmError> {
    let run = |args: &[&str]| -> String {
        Command::new(&tools.ffmpeg)
            .args(["-hide_banner"])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let has = |out: &str, name: &str| {
        out.lines()
            .any(|l| l.split_whitespace().nth(1).is_some_and(|w| w == name))
    };
    let encoders = run(&["-encoders"]);
    if !has(&encoders, opts.format.encoder()) {
        return Err(missing_component(format!(
            "This FFmpeg has no `{}` encoder, needed for {} audio.",
            opts.format.encoder(),
            opts.format.as_str().to_uppercase()
        )));
    }
    if opts.format == AudioFormat::Wav && !has(&run(&["-muxers"]), "wav") {
        return Err(missing_component(
            "This FFmpeg has no `wav` muxer, needed for WAV audio.",
        ));
    }
    if opts.loudness_lufs.is_some() && !has(&run(&["-filters"]), "loudnorm") {
        return Err(missing_component(
            "This FFmpeg has no `loudnorm` filter, needed for loudness normalisation.",
        ));
    }
    Ok(())
}

/// Runs FFmpeg for pass 1 and returns its stderr; polls `cancel` every 50 ms.
fn run_capture(
    ffmpeg: &Path,
    args: &[String],
    cancel: &dyn Fn() -> bool,
) -> Result<String, ExportError> {
    let mut child = Command::new(ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ExportError::Failed(crate::tools::spawn_error(ffmpeg, &e)))?;
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let mut buf = Vec::new();
            let _ = e.read_to_end(&mut buf);
            s = String::from_utf8_lossy(&buf).into_owned();
        }
        s
    });
    loop {
        if cancel() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(ExportError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = reader.join().unwrap_or_default();
                if !status.success() {
                    let tail: Vec<String> = text
                        .lines()
                        .rev()
                        .take(8)
                        .map(crate::run::cap_line)
                        .collect();
                    let tail: Vec<String> = tail.into_iter().rev().collect();
                    return Err(ExportError::Failed(failure_error(
                        &status.to_string(),
                        &tail,
                    )));
                }
                return Ok(text);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                return Err(ExportError::Failed(CmError::internal(format!(
                    "waiting for FFmpeg failed: {e}"
                ))))
            }
        }
    }
}

/// Exports the audio of the bundle in `bundle` into its `exports/` folder. `progress` gets 0..1
/// (1.0 after the rename). The masters are only read.
pub fn export_audio(
    tools: &Tools,
    bundle: &Path,
    manifest: &Value,
    opts: &AudioOptions,
    progress: &mut dyn FnMut(f64),
    cancel: &dyn Fn() -> bool,
) -> Result<AudioOutcome, ExportError> {
    let caps = probe_caps(tools)?;
    check_components(tools, opts)?;
    let plan = plan_audio(bundle, manifest, opts, caps.amix_normalize())?;
    let exports = bundle.join(EXPORTS_DIR);
    fs::create_dir_all(&exports).map_err(|e| io_err("create", &exports, &e))?;
    sweep_stale(&exports);
    let name = audio_output_name(&plan.title, opts.tracks, opts.format.extension());
    let final_path = exports.join(&name);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let part = exports.join(format!("{name}.{}-{nonce}.part", std::process::id()));
    let mut cleanup = Cleanup {
        paths: vec![part.clone()],
        dirs: Vec::new(),
    };
    progress(0.0);

    // Pass 1 (only with a loudness target).
    let mut measured: Option<Measured> = None;
    let mut mode = "off";
    let mut opts = *opts;
    if let Some(target) = opts.loudness_lufs {
        mode = "single_pass";
        let text = run_capture(&tools.ffmpeg, &measure_args(&plan, target), cancel)?;
        measured = parse_measured(&text);
        if measured.is_some() {
            mode = "two_pass";
        } else if text.contains("-inf") {
            // Digital silence: there is no level to normalise.
            opts.loudness_lufs = None;
            mode = "off";
        }
        progress(0.4);
    }
    let base = if mode == "off" && opts.loudness_lufs.is_none() {
        0.0
    } else if measured.is_some() || mode == "single_pass" {
        0.4
    } else {
        0.0
    };
    if cancel() {
        return Err(ExportError::Cancelled);
    }

    let args = encode_args(&plan, &opts, measured.as_ref(), &part);
    let mut last = base;
    let res = run_ffmpeg(
        &tools.ffmpeg,
        &args,
        plan.duration_s,
        &mut |p| {
            let f = (base + (1.0 - base) * p.fraction).clamp(base, 0.99);
            if f > last {
                last = f;
                progress(f);
            }
        },
        cancel,
    );
    match res {
        Ok(()) => {}
        Err(RunError::Cancelled) => return Err(ExportError::Cancelled),
        Err(RunError::Failed(e)) => return Err(ExportError::Failed(e)),
    }
    let size_bytes = fs::metadata(&part)
        .map_err(|e| io_err("read", &part, &e))?
        .len();
    if size_bytes == 0 {
        return Err(CmError::internal("FFmpeg wrote an empty audio file").into());
    }
    fs::rename(&part, &final_path).map_err(|e| io_err("rename", &part, &e))?;
    cleanup.paths.clear();
    progress(1.0);
    Ok(AudioOutcome {
        file: format!("{EXPORTS_DIR}/{name}"),
        path: final_path,
        size_bytes,
        duration_s: plan.duration_s,
        format: opts.format,
        tracks: opts.tracks,
        loudness_lufs: opts.loudness_lufs,
        loudness_mode: mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TPS: i64 = 120_000;

    /// A missing FFmpeg component is `job_failed` with a reason naming it, never `internal`.
    #[cfg(unix)]
    #[test]
    fn missing_component_is_job_failed() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let fake = d.path().join("ffmpeg");
        // Lists no encoders, muxers or filters.
        fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        let tools = Tools {
            ffmpeg: fake,
            ffprobe: None,
        };
        for opts in [
            AudioOptions::default(),
            AudioOptions {
                format: AudioFormat::Wav,
                loudness_lufs: None,
                ..AudioOptions::default()
            },
        ] {
            let e = check_components(&tools, &opts).unwrap_err();
            assert_eq!(e.code().as_str(), "job_failed", "{e}");
            match e {
                CmError::JobFailed { reason, .. } => {
                    assert!(reason.contains("This FFmpeg has no"), "{reason}");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    fn bundle_with(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in files {
            fs::write(d.path().join(f), b"x").unwrap();
        }
        d
    }

    fn manifest() -> Value {
        json!({
            "title": "Weekly sync", "created": "2026-10-26T09:00:00Z",
            "tracks": {
                "mic": {"file": "mic.denoised.wav", "raw_file": "mic.wav", "channels": 1,
                        "start": 0, "end": 120 * TPS},
                "system": {"file": "system.wav", "channels": 2, "start": 0, "end": 120 * TPS}
            },
            "edits": [{"version": 2, "in": 0, "out": 120 * TPS,
                       "cuts": [[60 * TPS, 90 * TPS]]}]
        })
    }

    fn plan(m: &Value, files: &[&str], opts: &AudioOptions) -> Result<AudioPlan, CmError> {
        let d = bundle_with(files);
        plan_audio(d.path(), m, opts, true)
    }

    #[test]
    fn a_cut_shortens_every_track_together() {
        let p = plan(
            &manifest(),
            &["mic.denoised.wav", "system.wav"],
            &AudioOptions::default(),
        )
        .unwrap();
        assert!((p.duration_s - 90.0).abs() < 0.001, "{}", p.duration_s);
        assert_eq!(p.inputs.len(), 2);
        assert_eq!(p.channels, 2);
        // Both tracks are cut into the same two pieces and mixed as a sum.
        assert_eq!(p.graph.matches("concat=n=2").count(), 2);
        assert!(p.graph.contains("amix=inputs=2"));
        assert!(p.graph.contains("normalize=0"));
        assert!(p.graph.ends_with("[outa]"));
        assert_eq!(p.date.as_deref(), Some("2026-10-26"));
    }

    #[test]
    fn mic_only_reads_only_the_mic_file() {
        let o = AudioOptions {
            tracks: TrackChoice::Mic,
            ..AudioOptions::default()
        };
        let p = plan(&manifest(), &["mic.denoised.wav", "system.wav"], &o).unwrap();
        assert_eq!(p.files, vec!["mic.denoised.wav"]);
        assert_eq!(p.channels, 1);
        assert!(!p.graph.contains("amix"));
    }

    #[test]
    fn system_only_and_missing_files() {
        let o = AudioOptions {
            tracks: TrackChoice::System,
            ..AudioOptions::default()
        };
        let p = plan(&manifest(), &["system.wav"], &o).unwrap();
        assert_eq!(p.files, vec!["system.wav"]);
        // The mic file is not on disk: Mix falls back to what exists; nothing at all is an error.
        let p = plan(&manifest(), &["mic.wav"], &AudioOptions::default()).unwrap();
        assert_eq!(p.files, vec!["mic.wav"]);
        let e = plan(&manifest(), &[], &AudioOptions::default()).unwrap_err();
        assert!(e.to_string().contains("no audio track"), "{e}");
    }

    #[test]
    fn bundles_are_recognised_by_their_files_without_track_entries() {
        let m = json!({"title": "Note", "duration": 10 * TPS});
        let p = plan(&m, &["mic.wav"], &AudioOptions::default()).unwrap();
        assert!((p.duration_s - 10.0).abs() < 0.001);
    }

    #[test]
    fn denoise_choice_picks_the_raw_file() {
        let mut m = manifest();
        m["edits"][0]["audio"] = json!({"denoise": false});
        let o = AudioOptions {
            tracks: TrackChoice::Mic,
            ..AudioOptions::default()
        };
        let p = plan(&m, &["mic.denoised.wav", "mic.wav"], &o).unwrap();
        assert_eq!(p.files, vec!["mic.wav"]);
    }

    #[test]
    fn pauses_are_removed_and_speed_applies() {
        let mut m = manifest();
        m["gaps"] = json!([{"from": 10 * TPS, "to": 20 * TPS}]);
        let p = plan(
            &m,
            &["mic.denoised.wav", "system.wav"],
            &AudioOptions::default(),
        )
        .unwrap();
        assert!((p.duration_s - 80.0).abs() < 0.001, "{}", p.duration_s);
        let mut m = manifest();
        m["edits"][0]["speed"] = json!(2.0);
        let p = plan(
            &m,
            &["mic.denoised.wav", "system.wav"],
            &AudioOptions::default(),
        )
        .unwrap();
        assert!(p.graph.contains("atempo=2"), "{}", p.graph);
        assert!((p.duration_s - 45.0).abs() < 0.01, "{}", p.duration_s);
    }

    #[test]
    fn gains_and_mutes_become_volume_filters() {
        let mut m = manifest();
        m["edits"][0]["audio"] = json!({"mic_gain_db": 3.0, "system_mute": true});
        let p = plan(
            &m,
            &["mic.denoised.wav", "system.wav"],
            &AudioOptions::default(),
        )
        .unwrap();
        assert!(p.graph.contains("volume=3dB"), "{}", p.graph);
        assert!(p.graph.contains("volume=0"), "{}", p.graph);
    }

    fn sample_plan() -> AudioPlan {
        plan(
            &manifest(),
            &["mic.denoised.wav", "system.wav"],
            &AudioOptions::default(),
        )
        .unwrap()
    }

    #[test]
    fn m4a_args_are_aac_48k_stereo_192k_with_tags_and_loudnorm() {
        let m = Measured {
            input_i: -23.5,
            input_tp: -4.0,
            input_lra: 3.2,
            input_thresh: -34.0,
            target_offset: 0.4,
        };
        let a = encode_args(
            &sample_plan(),
            &AudioOptions::default(),
            Some(&m),
            Path::new("/x/out.part"),
        );
        let s = a.join(" ");
        assert!(s.contains("-c:a aac"), "{s}");
        assert!(s.contains("-ar 48000"), "{s}");
        assert!(s.contains("-ac 2"), "{s}");
        assert!(s.contains("-b:a 192000"), "{s}");
        assert!(s.contains("-f mp4"), "{s}");
        assert!(s.contains("title=Weekly sync"), "{s}");
        assert!(s.contains("date=2026-10-26"), "{s}");
        assert!(
            s.contains("loudnorm=I=-16:TP=-1.5:LRA=11:measured_I=-23.5"),
            "{s}"
        );
        assert!(s.contains("linear=true"), "{s}");
        assert!(s.contains("aresample=48000[fin]"), "{s}");
        assert!(s.contains("-map [fin]"), "{s}");
        assert!(a.last().unwrap().ends_with("out.part"));
    }

    #[test]
    fn mono_m4a_is_128k_and_wav_is_24_bit_without_loudness_by_choice() {
        let o = AudioOptions {
            tracks: TrackChoice::Mic,
            ..AudioOptions::default()
        };
        let p = plan(&manifest(), &["mic.denoised.wav", "system.wav"], &o).unwrap();
        let s = encode_args(&p, &o, None, Path::new("o.part")).join(" ");
        assert!(s.contains("-b:a 128000"), "{s}");
        assert!(s.contains("-ac 1"), "{s}");
        // No measurement: the single-pass dynamic filter.
        assert!(
            s.contains("loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000[fin]"),
            "{s}"
        );

        let w = AudioOptions {
            format: AudioFormat::Wav,
            loudness_lufs: None,
            ..AudioOptions::default()
        };
        let s = encode_args(&sample_plan(), &w, None, Path::new("o.part")).join(" ");
        assert!(s.contains("-c:a pcm_s24le"), "{s}");
        assert!(s.contains("-f wav"), "{s}");
        assert!(!s.contains("loudnorm"), "{s}");
        assert!(!s.contains("-b:a"), "{s}");
        assert!(s.contains("-map [outa]"), "{s}");
    }

    #[test]
    fn measure_pass_writes_nothing_and_prints_json() {
        let s = measure_args(&sample_plan(), -16.0).join(" ");
        assert!(s.contains("print_format=json"), "{s}");
        assert!(s.contains("-f null -"), "{s}");
        assert!(!s.contains("-progress"), "{s}");
    }

    #[test]
    fn measured_values_are_read_from_the_json_block() {
        let text = "[Parsed_loudnorm_0 @ 0x1] \n{\n\t\"input_i\" : \"-23.54\",\n\t\"input_tp\" : \"-5.10\",\n\t\"input_lra\" : \"2.10\",\n\t\"input_thresh\" : \"-33.9\",\n\t\"output_i\" : \"-16.2\",\n\t\"target_offset\" : \"0.20\"\n}\n";
        let m = parse_measured(text).unwrap();
        assert_eq!(m.input_i, -23.54);
        assert_eq!(m.target_offset, 0.2);
        let silent = text.replace("-23.54", "-inf");
        assert_eq!(parse_measured(&silent), None);
        assert_eq!(parse_measured("no json here"), None);
    }

    #[test]
    fn option_parsing_and_names() {
        assert_eq!(AudioFormat::parse("wav"), Some(AudioFormat::Wav));
        assert_eq!(AudioFormat::parse("mp3"), None);
        assert_eq!(TrackChoice::parse("system"), Some(TrackChoice::System));
        assert_eq!(
            audio_output_name("A/B", TrackChoice::Mic, "m4a"),
            "A_B - audio-mic.m4a".replace("A_B", &file_stem("A/B"))
        );
        assert_eq!(AudioOptions::default_loudness(AudioFormat::Wav), None);
        assert_eq!(
            AudioOptions::default_loudness(AudioFormat::M4a),
            Some(-16.0)
        );
    }
}
