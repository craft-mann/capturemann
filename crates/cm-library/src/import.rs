//! Imported captures (IMP-2): the bundle of a media file that CaptureMann did not record.
//!
//! An import is built in a staging folder under `<library>/.capturemann/import/` and moved into
//! the library with one rename when it is complete ([`install`]), so the library never lists a
//! half-written import and a crash leaves only a staging folder ([`sweep_staging`] removes it at
//! the next start). The media work (copy, editing copy, sound decode, verify) is `cm_media::import`;
//! this module only describes the result in `manifest.json`.
//!
//! ```text
//! <title>.cmrec/
//! ├─ screen.mov        the file as imported (the master), or screen.edit.mp4, the editing copy
//! ├─ original.mkv      (prepared videos and audio files) the file as imported
//! ├─ mic.wav           the sound decoded once: 48 kHz, 32-bit float, 1 or 2 channels
//! └─ manifest.json     tracks, `source`, one `import` revision
//! ```
//!
//! A video is a `recording` bundle (`tracks.screen`, `tracks.mic`); an audio file is an `audio`
//! bundle (`kind: "audio"`, `tracks.mic` only). There is never a cursor, camera or system track.
//! `source` is `{ path, imported_at, original_name, original_file }`.

use std::path::{Path, PathBuf};

use cm_core::{Actor, CmError, MediaTime, Revision};
use serde_json::{json, Map};
use time::OffsetDateTime;

use crate::index::{CaptureRecord, Library};
use crate::manifest::{CaptureKind, RecManifest, SCHEMA_VERSION};
use crate::util::{io_err, json_err, new_id, sanitize_title, write_atomic};

/// Where staging folders live, relative to the library root.
pub const STAGING_RELATIVE: &str = ".capturemann/import";
/// The decoded sound.
pub const SOUND_FILE: &str = "mic.wav";
/// The editing copy of a prepared video.
pub const EDITING_COPY_FILE: &str = "screen.edit.mp4";

/// `manifest.source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSource {
    /// Absolute path of the file when it was imported.
    pub path: String,
    pub imported_at: OffsetDateTime,
    /// The file name, as the user knew it.
    pub original_name: String,
    /// The master inside the bundle.
    pub original_file: String,
}

/// The picture of an imported video.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoTrack {
    /// The file the editor plays (the master, or the editing copy).
    pub file: String,
    /// Displayed size in pixels.
    pub size: (u32, u32),
    pub fps: f64,
    pub codec: String,
    /// The original, when `file` is an editing copy.
    pub master: Option<String>,
    /// Why the editing copy was made (`variable_frame_rate`, `container_mkv`, ...).
    pub prepared: Vec<String>,
    pub hdr: bool,
}

/// The decoded sound.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundTrack {
    pub channels: u32,
    pub duration_s: f64,
    /// Stereo was mixed down to mono to fit a WAV file.
    pub downmixed: bool,
}

/// Everything the manifest of an import says.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportBundle {
    /// The id reserved when the import was queued (`import_media` returned it).
    pub id: String,
    pub title: String,
    pub duration_s: f64,
    /// `None` for an audio file.
    pub video: Option<VideoTrack>,
    pub sound: Option<SoundTrack>,
    pub source: ImportSource,
    pub created: OffsetDateTime,
    pub by: Actor,
}

/// A fresh capture id for an import (`rec_` + the usual sortable form).
pub fn mint_id(now: OffsetDateTime) -> String {
    new_id("rec", now)
}

/// A fresh capture id for an imported picture (`shot_`, like a screenshot).
pub fn mint_shot_id(now: OffsetDateTime) -> String {
    new_id("shot", now)
}

/// Builds the `.cmshot` of an imported picture inside `stage` and returns its folder (IMP-6): a
/// plain `original.png`, a manifest with `source { path, imported_at, original_name }`, an empty
/// ops list and the thumbnail. The folder is complete, ready for [`install_as`].
///
/// `id` is the capture id reserved when the import was queued. The thumbnail is best effort; a
/// failure is returned in the second field.
pub fn build_picture_bundle(
    stage: &Path,
    id: &str,
    title: &str,
    png: &[u8],
    source: &ImportSource,
) -> Result<(PathBuf, Option<String>), CmError> {
    let bundle = crate::bundle::ShotBundle::create(stage, title, png, source.imported_at)?;
    let dir = bundle.dir().to_path_buf();
    let thumb_warning = crate::thumbs::write_poster(&bundle, crate::thumbs::DEFAULT_POSTER_EDGE)
        .err()
        .map(|e| e.to_string());
    // The id and the source go into the manifest as plain JSON: every other key stays as the
    // bundle wrote it.
    let mpath = dir.join(crate::bundle::MANIFEST_FILE);
    let text = std::fs::read_to_string(&mpath).map_err(|e| io_err("cannot read", &mpath, &e))?;
    let mut m: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| json_err("manifest.json", &e))?;
    m["id"] = json!(id);
    m["source"] = json!({
        "path": source.path,
        "imported_at": rfc3339(source.imported_at),
        "original_name": source.original_name,
    });
    let out = serde_json::to_string_pretty(&m).map_err(|e| json_err("manifest.json", &e))?;
    write_atomic(&mpath, out.as_bytes())?;
    Ok((dir, thumb_warning))
}

fn ticks(seconds: f64) -> i64 {
    (seconds * f64::from(cm_core::TICKS_PER_SECOND as u32)).round() as i64
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// The manifest of an import. Pure.
pub fn build_manifest(b: &ImportBundle) -> RecManifest {
    let end = ticks(b.duration_s);
    let mut tracks = Map::new();
    if let Some(v) = &b.video {
        let mut t = json!({
            "file": v.file,
            "size": [v.size.0, v.size.1],
            "fps": v.fps,
            "codec": v.codec,
            "source_size": [v.size.0, v.size.1],
            "scale": 1.0,
            "start": 0,
            "end": end,
            "stats": {},
        });
        if let Some(m) = &v.master {
            t["master"] = json!(m);
        }
        if !v.prepared.is_empty() {
            t["prepared"] = json!(v.prepared);
        }
        if v.hdr {
            t["hdr"] = json!(true);
        }
        tracks.insert("screen".to_owned(), t);
    }
    if let Some(s) = &b.sound {
        let mut t = json!({
            "file": SOUND_FILE,
            "rate": 48_000,
            "channels": s.channels,
            "start": 0,
            "end": ticks(s.duration_s),
            "stats": {},
        });
        if s.downmixed {
            t["downmixed"] = json!(true);
        }
        tracks.insert("mic".to_owned(), t);
    }
    let mut extra = Map::new();
    extra.insert(
        "source".to_owned(),
        json!({
            "path": b.source.path,
            "imported_at": rfc3339(b.source.imported_at),
            "original_name": b.source.original_name,
            "original_file": b.source.original_file,
        }),
    );
    RecManifest {
        schema: SCHEMA_VERSION,
        id: b.id.clone(),
        title: b.title.clone(),
        created: b.created,
        tick_rate: Some(cm_core::TICKS_PER_SECOND as u32),
        duration: Some(MediaTime::from_ticks(end)),
        tracks,
        edits: vec![json!({"in": 0, "out": end, "speed": 1.0})],
        revisions: vec![Revision {
            rev: 1,
            action: "import".into(),
            by: b.by.clone(),
            at: b.source.imported_at,
        }],
        settings: json!({}),
        gaps: Vec::new(),
        captions: json!({"enabled": false, "burn_in": false}),
        kind: b.video.is_none().then(|| "audio".to_owned()),
        markers: Vec::new(),
        extra,
    }
}

/// Writes `manifest.json` into the staging folder.
pub fn write_manifest(dir: &Path, b: &ImportBundle) -> Result<RecManifest, CmError> {
    let m = build_manifest(b);
    let text = serde_json::to_string_pretty(&m).map_err(|e| json_err("manifest.json", &e))?;
    write_atomic(&dir.join("manifest.json"), text.as_bytes())?;
    Ok(m)
}

/// The staging root of a library.
pub fn staging_root(library_root: &Path) -> PathBuf {
    library_root.join(STAGING_RELATIVE)
}

/// A fresh, empty staging folder for one file of a job.
pub fn new_staging_dir(library_root: &Path, job_id: &str, n: usize) -> Result<PathBuf, CmError> {
    let dir = staging_root(library_root).join(format!("{job_id}-{n}"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| io_err("cannot clear", &dir, &e))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| io_err("cannot create", &dir, &e))?;
    Ok(dir)
}

/// Removes a staging folder, also when it holds a bundle whose `original.png` is read-only
/// (an imported picture, IMP-6; Windows refuses to delete a read-only file).
pub fn remove_staging(dir: &Path) {
    if std::fs::remove_dir_all(dir).is_ok() {
        return;
    }
    fn writable(dir: &Path) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                writable(&p);
            } else if let Ok(md) = std::fs::metadata(&p) {
                let mut perms = md.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(&p, perms);
            }
        }
    }
    writable(dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// Removes staging folders. With `job_id`, only that job's (a cancelled or restarted import);
/// without, all of them (the start of the daemon, when no import can be running).
pub fn sweep_staging(library_root: &Path, job_id: Option<&str>) {
    let root = staging_root(library_root);
    let Ok(rd) = std::fs::read_dir(&root) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let mine = job_id.is_none_or(|j| name.starts_with(&format!("{j}-")));
        if mine {
            remove_staging(&e.path());
        }
    }
}

/// Moves the finished staging folder into the library as `<title>.cmrec` (a ` (2)` suffix when the
/// name is taken) and indexes it. The folder must already hold `manifest.json`.
pub fn install(
    library: &mut Library,
    staging: &Path,
    title: &str,
) -> Result<CaptureRecord, CmError> {
    install_as(library, staging, title, CaptureKind::Recording)
}

/// [`install`] for a bundle of another kind (an imported picture is a `.cmshot`).
pub fn install_as(
    library: &mut Library,
    staging: &Path,
    title: &str,
    kind: CaptureKind,
) -> Result<CaptureRecord, CmError> {
    let base = sanitize_title(title)?;
    let ext = kind.extension();
    let root = library.root().to_path_buf();
    for n in 1..10_000u32 {
        let name = if n == 1 {
            format!("{base}.{ext}")
        } else {
            format!("{base} ({n}).{ext}")
        };
        let target = root.join(name);
        if target.exists() {
            continue;
        }
        std::fs::rename(staging, &target)
            .map_err(|e| io_err("cannot move into the library", &target, &e))?;
        return match library.index_dir(&target) {
            Ok(r) => Ok(r),
            Err(e) => {
                // Keep the library clean: an import that cannot be indexed is not left behind.
                let _ = std::fs::remove_dir_all(&target);
                Err(e)
            }
        };
    }
    Err(CmError::busy("too many bundles with the same title"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn bundle(video: bool) -> ImportBundle {
        ImportBundle {
            id: "rec_TEST".into(),
            title: "Phone clip".into(),
            duration_s: 12.5,
            video: video.then(|| VideoTrack {
                file: EDITING_COPY_FILE.into(),
                size: (1080, 1920),
                fps: 30.0,
                codec: "h264".into(),
                master: Some("original.mkv".into()),
                prepared: vec!["container_mkv".into(), "rotated".into()],
                hdr: false,
            }),
            sound: Some(SoundTrack {
                channels: 2,
                duration_s: 12.5,
                downmixed: false,
            }),
            source: ImportSource {
                path: "/Users/me/Movies/clip.mkv".into(),
                imported_at: datetime!(2026-11-03 10:00 UTC),
                original_name: "clip.mkv".into(),
                original_file: "original.mkv".into(),
            },
            created: datetime!(2026-11-03 10:00 UTC),
            by: Actor::User,
        }
    }

    #[test]
    fn a_video_manifest_has_the_tracks_and_the_source() {
        let m = build_manifest(&bundle(true));
        assert_eq!(m.capture_kind(), CaptureKind::Recording);
        assert_eq!(m.duration, Some(MediaTime::from_ticks(1_500_000)));
        let screen = &m.tracks["screen"];
        assert_eq!(screen["file"], "screen.edit.mp4");
        assert_eq!(screen["size"], json!([1080, 1920]));
        assert_eq!(screen["master"], "original.mkv");
        assert_eq!(screen["prepared"], json!(["container_mkv", "rotated"]));
        assert_eq!(screen["start"], 0);
        assert_eq!(screen["end"], 1_500_000);
        assert!(screen.get("cursor").is_none(), "no cursor data");
        let mic = &m.tracks["mic"];
        assert_eq!(
            (
                mic["file"].clone(),
                mic["rate"].clone(),
                mic["channels"].clone()
            ),
            (json!("mic.wav"), json!(48000), json!(2))
        );
        assert_eq!(m.tracks.len(), 2, "no camera, system or cursor track");
        assert_eq!(m.extra["source"]["original_name"], "clip.mkv");
        assert_eq!(m.extra["source"]["original_file"], "original.mkv");
        assert_eq!(m.extra["source"]["imported_at"], "2026-11-03T10:00:00Z");
        assert_eq!(m.revisions[0].action, "import");
        assert_eq!(m.edits[0]["out"], 1_500_000);
    }

    #[test]
    fn an_audio_manifest_is_kind_audio_with_only_the_sound() {
        let m = build_manifest(&bundle(false));
        assert_eq!(m.capture_kind(), CaptureKind::Audio);
        assert!(!m.tracks.contains_key("screen"));
        assert!(m.tracks.contains_key("mic"));
    }

    #[test]
    fn an_import_is_installed_atomically_and_indexed() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open_or_create(dir.path()).unwrap();
        for (i, expect) in ["Phone clip.cmrec", "Phone clip (2).cmrec"]
            .iter()
            .enumerate()
        {
            let stage = new_staging_dir(dir.path(), "job1", i).unwrap();
            std::fs::write(stage.join("original.mkv"), b"x").unwrap();
            let mut b = bundle(true);
            b.id = format!("rec_TEST{i}");
            write_manifest(&stage, &b).unwrap();
            let rec = install(&mut lib, &stage, "Phone clip").unwrap();
            assert_eq!(rec.path, *expect);
            assert!(!stage.exists());
            assert_eq!(rec.kind, CaptureKind::Recording);
            assert_eq!(rec.duration, Some(MediaTime::from_ticks(1_500_000)));
            assert_eq!((rec.width, rec.height), (Some(1080), Some(1920)));
            assert_eq!(lib.get(&b.id).unwrap().title, "Phone clip");
        }
        // A rescan agrees with the index (the manifest is valid for the library).
        let report = lib.rescan().unwrap();
        assert!(report.duplicates.is_empty());
        assert_eq!(lib.count().unwrap(), 2);
        // An audio import is indexed as audio.
        let stage = new_staging_dir(dir.path(), "job1", 5).unwrap();
        let mut b = bundle(false);
        b.id = "rec_AUDIO".into();
        write_manifest(&stage, &b).unwrap();
        assert_eq!(
            install(&mut lib, &stage, "Memo").unwrap().kind,
            CaptureKind::Audio
        );
    }

    #[test]
    fn staging_folders_are_swept_by_job_or_all() {
        let dir = tempfile::tempdir().unwrap();
        let a = new_staging_dir(dir.path(), "job1", 0).unwrap();
        let b = new_staging_dir(dir.path(), "job2", 0).unwrap();
        sweep_staging(dir.path(), Some("job1"));
        assert!(!a.exists() && b.exists());
        // The same name again starts clean.
        std::fs::write(b.join("junk"), b"x").unwrap();
        let b2 = new_staging_dir(dir.path(), "job2", 0).unwrap();
        assert!(!b2.join("junk").exists());
        sweep_staging(dir.path(), None);
        assert!(!b2.exists());
    }
}
