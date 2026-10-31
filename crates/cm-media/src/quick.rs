//! The quick export (interim exporter): one combined MP4 per recording, written to
//! `<bundle>/exports/<title> - quick.mp4` through a temporary name and an atomic rename. The
//! masters are only read.
//!
//! Forward compatibility with M4 (EXP-1..4): the preset id (`quick`), the file naming
//! (`{title} - {preset}.mp4`), the manifest entry (see [`QuickOutcome::to_entry`]) and the job
//! contract stay; only the renderer behind them changes from FFmpeg to the GPU compositor.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_core::CmError;
use serde_json::{json, Value};

use crate::bubble::write_assets;
use crate::edited::{default_audio, export_edited, resolve_edits, resolve_untouched, ExportPath};
use crate::graph::{build, prepare, BubbleAssets, BuildInput, Plan};
use crate::preset::{self, ExportOptions, Kind, Presets, Target};
use crate::run::{run_ffmpeg, RunError};
use crate::spec::QuickSpec;
use crate::timeline::file_duration_seconds;
use crate::tools::{
    encoder_candidates_for, is_encoder_init_failure, probe_caps, probe_file, Encoder,
    EncoderPolicy, Tools,
};

/// The interim preset id.
pub const PRESET_QUICK: &str = "quick";
/// The folder inside the bundle.
pub const EXPORTS_DIR: &str = "exports";

/// What to export.
pub struct QuickRequest<'a> {
    /// The `.cmrec` folder.
    pub bundle_dir: &'a Path,
    /// `manifest.json` as JSON.
    pub manifest: &'a Value,
    /// Force an encoder (tests); `None` picks hardware first.
    pub prefer_encoder: Option<Encoder>,
    /// Which renderer; [`ExportPath::Auto`] uses the compositor when the edit list changes
    /// the picture.
    pub path: ExportPath,
    /// Integrated loudness target in LUFS (AUD-4); `None` leaves the level alone. Applied by the
    /// compositor path; the FFmpeg graph path ignores it.
    pub loudness_lufs: Option<f64>,
    /// An EXP-2 / EXP-3 preset; `None` is the interim `quick` preset.
    pub preset: Option<PresetRequest<'a>>,
    /// Burn captions into the picture (AI-1): the source of the cues for the output timeline.
    /// `Some` always uses the compositor path (the FFmpeg graph cannot draw them).
    pub captions: Option<crate::captions::CaptionSource>,
}

/// A preset asked for by id, resolved against the recording when the export starts.
#[derive(Debug, Clone, Copy)]
pub struct PresetRequest<'a> {
    pub presets: &'a Presets,
    pub id: &'a str,
    /// GIF and WebM choices (width, fps, "fit under N MB").
    pub options: ExportOptions,
}

/// A finished export.
#[derive(Debug, Clone)]
pub struct QuickOutcome {
    /// The preset id, `quick` for the interim export.
    pub preset: String,
    /// Path relative to the bundle, `exports/<title> - <preset>.<ext>`.
    pub file: String,
    pub path: PathBuf,
    pub duration_ms: i64,
    pub size_bytes: u64,
    /// The encoder used, e.g. `h264_videotoolbox`, `h264_mf` or `libx264`.
    pub encoder: String,
    /// False when the encoder is GPL (`libx264`): the app can refuse to ship such an export
    /// path (NFR-12).
    pub lgpl_safe: bool,
    /// Files of earlier `quick` exports of this bundle that this one replaced and deleted
    /// (bundle-relative), for example after a title rename.
    pub replaced: Vec<String>,
    pub settings: Value,
    /// Wall time of the whole export.
    pub elapsed_ms: u64,
}

impl QuickOutcome {
    /// The `exports[]` entry of the manifest: `{ file, preset, created, duration_ms, size_bytes,
    /// encoder, settings }`.
    pub fn to_entry(&self, created: &str) -> Value {
        json!({
            "file": self.file,
            "preset": self.preset,
            "created": created,
            "duration_ms": self.duration_ms,
            "size_bytes": self.size_bytes,
            "encoder": self.encoder,
            "settings": self.settings,
        })
    }
}

/// How an export ends without a file.
#[derive(Debug)]
pub enum ExportError {
    /// The cancel closure fired; partial output is deleted.
    Cancelled,
    Failed(CmError),
}

impl ExportError {
    pub fn into_cm(self) -> CmError {
        match self {
            ExportError::Cancelled => CmError::internal("The export was cancelled"),
            ExportError::Failed(e) => e,
        }
    }
}

impl From<CmError> for ExportError {
    fn from(e: CmError) -> Self {
        ExportError::Failed(e)
    }
}

/// Longest stem in chars and in bytes: with ` - quick.mp4` and the `.part` temp suffix the
/// name stays under the 255-byte limit of APFS, NTFS and ext4.
const MAX_STEM_CHARS: usize = 120;
const MAX_STEM_BYTES: usize = 200;

/// A file name for `title`: no path separators, `%` (cmd.exe expands `%VAR%` when a path is
/// opened with `start`), characters Windows refuses, bidi/zero-width characters or Windows
/// device names; at most 120 chars and 200 bytes (a CJK title is 3 bytes a char).
pub fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|c| {
            !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}' | '\u{061C}' | '\u{FEFF}')
        })
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '%' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let mut out = String::new();
    for c in cleaned
        .trim()
        .trim_matches('.')
        .chars()
        .take(MAX_STEM_CHARS)
    {
        if out.len() + c.len_utf8() > MAX_STEM_BYTES {
            break;
        }
        out.push(c);
    }
    let mut trimmed = out.trim().trim_end_matches('.').trim().to_owned();
    if trimmed.is_empty() {
        return "Recording".to_owned();
    }
    let upper = trimmed.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or("").trim_end();
    let reserved = matches!(stem, "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            stem.strip_prefix(p)
                .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
        });
    if reserved {
        trimmed.push('_');
    }
    trimmed
}

/// `<title> - quick.mp4`.
pub fn output_name(title: &str) -> String {
    output_name_for(title, PRESET_QUICK, "mp4")
}

/// `<title> - <preset>.<ext>`.
pub fn output_name_for(title: &str, preset: &str, ext: &str) -> String {
    format!("{} - {preset}.{ext}", file_stem(title))
}

/// Deletes the temp files on every way out unless disarmed.
pub(crate) struct Cleanup {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) dirs: Vec<PathBuf>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = fs::remove_file(p);
        }
        for d in &self.dirs {
            let _ = fs::remove_dir_all(d);
        }
    }
}

pub(crate) fn io_err(what: &str, p: &Path, e: &std::io::Error) -> CmError {
    crate::run::map_io_error(&format!("cannot {what} `{}`", p.display()), e)
}

/// Sweeps only what nobody has touched for this long (an export keeps writing to its files).
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

/// Is process `pid` running? Always false where this cannot be checked (the age rule still
/// applies there).
fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 only checks that the process exists and may be signalled.
        let r = unsafe { libc::kill(pid, 0) };
        r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// The owning pid encoded in a temp name: `.work-<pid>-<nonce>`, `<name>.<pid>-<nonce>.part`, or
/// the scratch files of an edited export, `<name>.<pid>-<nonce>.mix.part` and `.mid.part`.
fn temp_owner_pid(name: &str) -> Option<u32> {
    let tag = if let Some(rest) = name.strip_prefix(".work-") {
        rest
    } else {
        let stem = name.strip_suffix(".part")?;
        let stem = stem
            .strip_suffix(".mix")
            .or_else(|| stem.strip_suffix(".mid"))
            .unwrap_or(stem);
        stem.rsplit('.').next()?
    };
    tag.split('-').next()?.parse().ok()
}

/// Removes what a crashed export left in `exports/`: `.work-*` folders and `*.part` files that
/// nobody has touched for 6 hours, and whose owning process (named in the temp name) is gone.
/// Another export running in the same bundle keeps its files, however slow it is.
pub(crate) fn sweep_stale(exports: &Path) {
    sweep_older_than(exports, STALE_AFTER, &pid_alive);
}

fn sweep_older_than(exports: &Path, age_limit: std::time::Duration, alive: &dyn Fn(u32) -> bool) {
    let Ok(entries) = fs::read_dir(exports) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let stale_name = name.starts_with(".work-") || name.ends_with(".part");
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > age_limit);
        if stale_name && old && !temp_owner_pid(&name).is_some_and(alive) {
            let p = e.path();
            let _ = if p.is_dir() {
                fs::remove_dir_all(&p)
            } else {
                fs::remove_file(&p)
            };
        }
    }
}

/// Deletes the files of earlier exports of `preset` listed in the manifest that this export
/// replaced (a different name after a title rename). Only plain files directly inside
/// `exports/` are touched.
pub(crate) fn remove_replaced(
    bundle: &Path,
    manifest: &Value,
    preset: &str,
    keep: &Path,
) -> Vec<String> {
    let mut removed = Vec::new();
    let Some(entries) = manifest.get("exports").and_then(Value::as_array) else {
        return removed;
    };
    for e in entries {
        if e.get("preset").and_then(Value::as_str) != Some(preset) {
            continue;
        }
        let Some(rel) = e.get("file").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = rel.strip_prefix("exports/") else {
            continue;
        };
        if name.is_empty() || name.contains(['/', '\\']) || name == ".." || name.starts_with('.') {
            continue;
        }
        let path = bundle.join(EXPORTS_DIR).join(name);
        if path != keep && path.is_file() && fs::remove_file(&path).is_ok() {
            removed.push(rel.to_owned());
        }
    }
    removed
}

/// The screen master's size: what ffprobe reads from the file, else the manifest's.
pub(crate) fn master_size(tools: &Tools, screen_path: &Path, spec: &QuickSpec) -> (u32, u32) {
    tools
        .ffprobe
        .as_ref()
        .and_then(|_| probe_file(tools, screen_path).ok())
        .and_then(|p| Some((p.width?, p.height?)))
        .unwrap_or(spec.screen.size)
}

/// Runs the quick export. `progress` gets 0..1 (1.0 only after the rename); `cancel` is polled
/// about every 50 ms. See the module docs.
pub fn export_quick(
    tools: &Tools,
    req: &QuickRequest<'_>,
    progress: &mut dyn FnMut(f64),
    cancel: &dyn Fn() -> bool,
) -> Result<QuickOutcome, ExportError> {
    let t0 = Instant::now();
    let spec = QuickSpec::from_manifest(req.manifest)?;
    let bundle = req.bundle_dir;
    let screen_path = bundle.join(&spec.screen.file);
    if !screen_path.is_file() {
        return Err(CmError::not_found(format!(
            "The screen recording `{}` is missing from `{}`",
            spec.screen.file,
            bundle.display()
        ))
        .into());
    }
    // A preset is made concrete for this recording (EXP-2.AC3 refusals come from here).
    let target: Option<Target> = match &req.preset {
        None => None,
        Some(pr) => Some(preset::resolve(
            pr.presets,
            pr.id,
            master_size(tools, &screen_path, &spec),
            spec.screen.fps,
            &pr.options,
        )?),
    };
    // GIF, WebM and a fixed frame need the compositor; so does an edit list that changes the
    // picture. `source` and `quick` keep the FFmpeg graph for an untouched recording.
    let target_needs_compositor = target
        .as_ref()
        .is_some_and(|t| t.kind != Kind::Mp4 || t.frame.is_some());
    let burns_captions = req.captions.is_some();
    if target_needs_compositor || burns_captions || req.path != ExportPath::Ffmpeg {
        let resolved = resolve_edits(req.manifest, &spec);
        if target_needs_compositor
            || burns_captions
            || req.path == ExportPath::Compositor
            || !resolved.trivial
        {
            return export_edited(
                tools,
                req,
                spec,
                resolved,
                target.as_ref(),
                &default_audio(),
                progress,
                cancel,
            );
        }
    }
    let preset_id = target.as_ref().map_or(PRESET_QUICK, |t| t.id.as_str());
    let caps = probe_caps(tools)?;
    let mut candidates = encoder_candidates_for(&caps, req.prefer_encoder, EncoderPolicy::host())?;
    if candidates[0].compositor_only() {
        // VAAPI (frames are uploaded to the GPU) and VP9-in-MP4 are written by the compositor
        // path only; it picks its encoder the same way.
        let resolved = resolve_untouched(req.manifest, &spec);
        return export_edited(
            tools,
            req,
            spec,
            resolved,
            target.as_ref(),
            &default_audio(),
            progress,
            cancel,
        );
    }
    candidates.retain(|e| !e.compositor_only());
    let mut tried = 0;
    let mut encoder = candidates[tried];

    // What is really in screen.mp4: duration, size (ffprobe when present, else the manifest).
    let manifest_duration = file_duration_seconds(spec.screen.start, spec.screen.end, &spec.gaps);
    let probed = tools
        .ffprobe
        .as_ref()
        .and_then(|_| probe_file(tools, &screen_path).ok());
    let file_duration = probed
        .as_ref()
        .and_then(|p| p.duration_s)
        .filter(|d| *d > 0.05)
        .unwrap_or(manifest_duration);
    if file_duration <= 0.05 {
        return Err(CmError::invalid_input("The screen recording is empty.").into());
    }
    // The export starts at `live_at`; a `live_at` that would leave nothing falls back to the
    // screen's first frame.
    let mut spec = spec;
    if file_duration - spec.front_trim_seconds() <= 0.05 {
        spec.live_at = None;
    }
    let duration = file_duration - spec.front_trim_seconds();
    let master = probed
        .as_ref()
        .and_then(|p| Some((p.width?, p.height?)))
        .unwrap_or(spec.screen.size);

    // Optional tracks that are not on disk are left out (noted), the export still succeeds.
    let exists = |f: &str| bundle.join(f).is_file();
    let camera_present = spec.camera.as_ref().is_some_and(|c| exists(&c.file));
    if spec.camera.is_some() && !camera_present {
        spec.notes
            .push("the camera file is missing, exported without the bubble".to_owned());
    }
    let have_mic = spec.mic.as_ref().is_some_and(|m| exists(&m.file));
    let have_system = spec.system.as_ref().is_some_and(|s| exists(&s.file));
    if spec.mic.is_some() && !have_mic {
        spec.notes.push("the microphone file is missing".to_owned());
    }
    if spec.system.is_some() && !have_system {
        spec.notes
            .push("the system audio file is missing".to_owned());
    }

    let exports = bundle.join(EXPORTS_DIR);
    fs::create_dir_all(&exports).map_err(|e| io_err("create", &exports, &e))?;
    sweep_stale(&exports);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let work = exports.join(format!(".work-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&work).map_err(|e| io_err("create", &work, &e))?;
    let name = output_name_for(&spec.title, preset_id, "mp4");
    let final_path = exports.join(&name);
    // Per-process, per-run temp name: two exports of one bundle never share a `.part`.
    let part = exports.join(format!("{name}.{}-{nonce}.part", std::process::id()));
    let mut cleanup = Cleanup {
        paths: vec![part.clone()],
        dirs: vec![work.clone()],
    };

    let prepared = prepare(&spec, master, camera_present);
    let assets = match &prepared.bubble {
        Some(g) => {
            let (mask, ring) = write_assets(&work, g)?;
            Some(BubbleAssets { mask, ring })
        }
        None => None,
    };
    if cancel() {
        return Err(ExportError::Cancelled);
    }

    let make_plan = |encoder: Encoder, caps: &crate::tools::Caps| -> Plan {
        build(&BuildInput {
            spec: &spec,
            bundle,
            prepared: &prepared,
            assets: assets.as_ref(),
            encoder,
            caps,
            duration_s: duration,
            have_mic,
            have_system,
            out: &part,
        })
    };
    let mut plan = make_plan(encoder, &caps);
    let mut fallback_note = None;
    loop {
        let mut on_progress = |p: crate::run::Progress| progress((p.fraction * 0.98).min(0.98));
        match run_ffmpeg(
            &tools.ffmpeg,
            &plan.args,
            plan.duration_s,
            &mut on_progress,
            cancel,
        ) {
            Ok(()) => break,
            Err(RunError::Cancelled) => return Err(ExportError::Cancelled),
            Err(RunError::Failed(e)) => {
                // Another encoder is tried only when this one could not START (a size, driver
                // or device it refuses); bad input, a full disk or a forced encoder fail as
                // they are (R5-4).
                let next = candidates.get(tried + 1).copied();
                let retry = req.prefer_encoder != Some(encoder)
                    && e.code() != cm_core::ErrorCode::DiskFull
                    && is_encoder_init_failure(&e.to_string(), encoder);
                if let (true, Some(next)) = (retry, next) {
                    let note = format!(
                        "{} failed, used {} instead: {}",
                        encoder.name(),
                        next.name(),
                        e.to_string().lines().next().unwrap_or("")
                    );
                    fallback_note = Some(match fallback_note {
                        Some(prev) => format!("{prev}; {note}"),
                        None => note,
                    });
                    let _ = fs::remove_file(&part);
                    tried += 1;
                    encoder = next;
                    plan = make_plan(encoder, &caps);
                    progress(0.0);
                    continue;
                }
                return Err(ExportError::Failed(e));
            }
        }
    }
    if cancel() {
        return Err(ExportError::Cancelled);
    }
    let size = fs::metadata(&part)
        .map_err(|e| io_err("read", &part, &e))?
        .len();
    if size == 0 {
        return Err(CmError::internal("FFmpeg produced an empty file").into());
    }
    fs::rename(&part, &final_path).map_err(|e| io_err("write", &final_path, &e))?;
    // A re-export after a title rename has a new name; the old file would be orphaned once the
    // manifest entry (one per preset) is replaced.
    let replaced = remove_replaced(bundle, req.manifest, preset_id, &final_path);
    // The temp name is gone; only the work folder is left to clean.
    cleanup.paths.clear();
    drop(cleanup);

    // Verify the result when we can: the duration is the file's, not the plan's.
    let measured = tools
        .ffprobe
        .as_ref()
        .and_then(|_| probe_file(tools, &final_path).ok())
        .and_then(|p| p.duration_s)
        .unwrap_or(plan.duration_s);
    let mut settings = plan.settings.clone();
    if let (Some(o), Some(n)) = (settings.as_object_mut(), fallback_note) {
        o.insert("encoder_fallback".into(), json!(n));
    }
    progress(1.0);
    Ok(QuickOutcome {
        preset: preset_id.to_owned(),
        file: format!("{EXPORTS_DIR}/{name}"),
        path: final_path,
        duration_ms: (measured * 1000.0).round() as i64,
        size_bytes: size,
        encoder: encoder.name().to_owned(),
        lgpl_safe: encoder.lgpl_safe(),
        replaced,
        settings,
        elapsed_ms: u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe_and_follow_the_spec_pattern() {
        assert_eq!(output_name("Lesson 1"), "Lesson 1 - quick.mp4");
        assert_eq!(output_name("a/b:c*d"), "a-b-c-d - quick.mp4");
        assert_eq!(output_name("  "), "Recording - quick.mp4");
        assert_eq!(output_name("..hidden.."), "hidden - quick.mp4");
        assert!(file_stem(&"x".repeat(300)).chars().count() == 120);
    }

    /// R5-16: names are limited in bytes too, free of `%` and device names, and valid for CJK.
    #[test]
    fn r5_16_file_stems_for_unicode_and_very_long_titles() {
        let cjk = file_stem(&"字".repeat(120));
        assert!(cjk.len() <= MAX_STEM_BYTES, "{} bytes", cjk.len());
        assert_eq!(cjk.chars().count(), 66);
        // The whole output name fits a 255-byte file name even with the `.part` temp suffix.
        let full = format!(
            "{}.{}-{}.part",
            output_name(&"字".repeat(300)),
            u32::MAX,
            u64::MAX
        );
        assert!(full.len() < 255, "{} bytes", full.len());
        assert_eq!(file_stem("100% %PATH%"), "100- -PATH-");
        for n in ["CON", "com3", "lpt9.txt", "nul"] {
            assert_eq!(file_stem(n), format!("{n}_"), "{n}");
        }
        assert_eq!(file_stem("COM10"), "COM10");
        assert_eq!(file_stem("evil\u{202E}gnp.exe"), "evilgnp.exe");
        assert_eq!(file_stem("\u{202E}"), "Recording");
        assert_eq!(
            file_stem("emoji \u{1F3AC} take 2"),
            "emoji \u{1F3AC} take 2"
        );
    }

    /// R5-17: temp names carry the owner pid; old, orphaned ones are swept, live ones are not.
    #[test]
    fn r5_17_sweep_removes_old_orphans_but_not_a_running_exports_files() {
        assert_eq!(temp_owner_pid(".work-4242-99"), Some(4242));
        assert_eq!(temp_owner_pid("A - quick.mp4.4242-99.part"), Some(4242));
        assert_eq!(temp_owner_pid("A - quick.mp4.4242-99.mix.part"), Some(4242));
        assert_eq!(temp_owner_pid("A - gif.gif.4242-99.mid.part"), Some(4242));
        assert_eq!(temp_owner_pid("A - quick.mp4.part"), None);
        assert_eq!(temp_owner_pid("A - quick.mp4"), None);
        let d = tempfile::tempdir().unwrap();
        for dir in [".work-111-1", ".work-222-1", ".work-nopid"] {
            fs::create_dir_all(d.path().join(dir)).unwrap();
            fs::write(d.path().join(dir).join("mask.png"), b"x").unwrap();
        }
        fs::write(d.path().join("A - quick.mp4.111-1.part"), b"x").unwrap();
        fs::write(d.path().join("A - quick.mp4.222-1.part"), b"x").unwrap();
        fs::write(d.path().join("B - quick.mp4.part"), b"x").unwrap();
        fs::write(d.path().join("A - quick.mp4"), b"keep").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        // "Old" is anything older than 10 ms here; pid 111 is alive, 222 is gone.
        sweep_older_than(d.path(), std::time::Duration::from_millis(10), &|p| {
            p == 111
        });
        let left: std::collections::BTreeSet<String> = fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let want: std::collections::BTreeSet<String> =
            [".work-111-1", "A - quick.mp4.111-1.part", "A - quick.mp4"]
                .map(String::from)
                .into();
        assert_eq!(left, want);
        // The real sweep uses 6 hours: fresh files survive.
        fs::create_dir_all(d.path().join(".work-333-1")).unwrap();
        sweep_stale(d.path());
        assert!(d.path().join(".work-333-1").is_dir());
        assert_eq!(STALE_AFTER.as_secs(), 6 * 3600);
        // Our own pid is alive.
        assert!(pid_alive(std::process::id()) || cfg!(not(unix)));
    }

    /// A re-export after a title rename deletes the previous quick file, only inside exports/.
    #[test]
    fn r5_16_replaced_exports_are_deleted_only_inside_exports() {
        let d = tempfile::tempdir().unwrap();
        let ex = d.path().join(EXPORTS_DIR);
        fs::create_dir_all(&ex).unwrap();
        fs::write(ex.join("Old - quick.mp4"), b"x").unwrap();
        fs::write(ex.join("New - quick.mp4"), b"x").unwrap();
        fs::write(ex.join("Other - 4k.mp4"), b"x").unwrap();
        fs::write(d.path().join("screen.mp4"), b"master").unwrap();
        let m = json!({"exports": [
            {"file": "exports/Old - quick.mp4", "preset": "quick"},
            {"file": "exports/New - quick.mp4", "preset": "quick"},
            {"file": "exports/Other - 4k.mp4", "preset": "4k"},
            {"file": "../screen.mp4", "preset": "quick"},
            {"file": "exports/../screen.mp4", "preset": "quick"},
            {"file": "screen.mp4", "preset": "quick"}
        ]});
        let removed = remove_replaced(d.path(), &m, "quick", &ex.join("New - quick.mp4"));
        assert_eq!(removed, ["exports/Old - quick.mp4"]);
        assert!(!ex.join("Old - quick.mp4").exists());
        assert!(ex.join("New - quick.mp4").exists() && ex.join("Other - 4k.mp4").exists());
        assert!(
            d.path().join("screen.mp4").exists(),
            "masters are never touched"
        );
    }

    /// A crashed export's leftovers are swept, but only when they are old: an export in progress
    /// (fresh `.part` and `.work-*`) is never touched.
    #[test]
    fn fresh_work_files_are_not_swept() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join(".work-1-2")).unwrap();
        fs::write(d.path().join("A - quick.mp4.part"), b"x").unwrap();
        fs::write(d.path().join("A - quick.mp4"), b"x").unwrap();
        sweep_stale(d.path());
        assert!(d.path().join(".work-1-2").is_dir());
        assert!(d.path().join("A - quick.mp4.part").is_file());
        assert!(d.path().join("A - quick.mp4").is_file());
    }

    #[test]
    fn entry_has_the_manifest_fields() {
        let o = QuickOutcome {
            preset: "quick".into(),
            file: "exports/A - quick.mp4".into(),
            path: "/b/exports/A - quick.mp4".into(),
            duration_ms: 12_345,
            size_bytes: 99,
            encoder: "libx264".into(),
            lgpl_safe: false,
            replaced: Vec::new(),
            settings: json!({"fps": 30}),
            elapsed_ms: 5,
        };
        let e = o.to_entry("2026-10-21T10:00:00Z");
        for k in [
            "file",
            "preset",
            "created",
            "duration_ms",
            "size_bytes",
            "encoder",
            "settings",
        ] {
            assert!(e.get(k).is_some(), "{k}");
        }
        assert_eq!(e["preset"], "quick");
    }
}
