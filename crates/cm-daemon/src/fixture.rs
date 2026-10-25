//! Synthetic `.cmrec` bundles for edit, export and playback tests and for manual runs.
//!
//! Nothing here touches a screen, camera or microphone: every track is synthesized with FFmpeg's
//! `lavfi` sources and written into a real bundle with [`RecBundle`], so the library, the editor,
//! `capturemann edit` and the exporter treat it like a recording.
//!
//! The content is built to make edits visible and measurable:
//!
//! * **Screen** (1280x720 at 30 fps by default): FFmpeg's `testsrc` pattern, whose clock text
//!   changes every frame (the frame counter), with a **white flash** every [`SYNC_PERIOD_S`]
//!   starting at [`SYNC_FIRST_S`] of file time, [`SYNC_LEN_S`] long.
//! * **Microphone** (mono): silence with a 1 kHz **beep** at the same session times as the flashes.
//! * **System audio** (stereo): a steady quiet 300 Hz tone (a pitch reference for speed changes).
//! * **Camera** (640x480): colour bars with a white flash at the same session times.
//! * **Pause**: one gap of [`FixturePause::len_s`] seconds at [`FixturePause::at_s`] of screen file
//!   time. Media files are compacted (no gap content), exactly like a real pause.
//! * **Cursor** (`cursor.jsonl`): a smooth path with three left clicks, in session time.
//!
//! Track starts are offset like a real session: screen 390 ms, camera 580 ms, microphone 100 ms,
//! system audio 390 ms after the session origin (all before the pause).
//!
//! ```text
//! cargo run -p cm-daemon --example make_fixture -- --library /tmp/cm-fixture-library
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cm_core::{
    CmError, MediaTime, PauseGap, PhysicalSize, TrackInfo, TrackKind, TrackStats, TICKS_PER_SECOND,
};
use cm_cursor::{
    Availability, Button, CursorEvent, CursorHeader, CursorTarget, MoveSource, TargetKind,
    SCHEMA_VERSION,
};
use cm_library::rec::{RecBundle, SessionInfo, LAYOUT_KEY};
use cm_media::Tools;
use serde_json::{json, Value};
use time::OffsetDateTime;

/// File time (s) of the first flash and beep.
pub const SYNC_FIRST_S: f64 = 1.0;
/// Seconds between flashes.
pub const SYNC_PERIOD_S: f64 = 3.0;
/// Length of a flash or beep.
pub const SYNC_LEN_S: f64 = 0.2;
/// Where the tracks start after the session origin (ms).
pub const SCREEN_START_MS: i64 = 390;
pub const CAMERA_START_MS: i64 = 580;
pub const MIC_START_MS: i64 = 100;
pub const SYSTEM_START_MS: i64 = 390;

/// RGB to YUV with the BT.709 matrix, limited range: how the real masters are made, so a player,
/// FFmpeg and the compositor all read the same colours.
const TO_BT709: &str = "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p";
/// The matching stream tags.
const BT709_TAGS: [&str; 8] = [
    "-colorspace",
    "bt709",
    "-color_primaries",
    "bt709",
    "-color_trc",
    "bt709",
    "-color_range",
    "tv",
];

/// One pause inside the recording.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixturePause {
    /// Screen file time (s) at which the pause happens.
    pub at_s: f64,
    pub len_s: f64,
}

/// What to synthesize.
#[derive(Debug, Clone)]
pub struct FixtureSpec {
    pub title: String,
    /// Screen file length in seconds.
    pub secs: u32,
    pub size: (u32, u32),
    pub fps: u32,
    pub camera: bool,
    pub audio: bool,
    pub cursor: bool,
    pub pause: Option<FixturePause>,
    /// `edits[0].layout.camera` (the bubble at stop).
    pub camera_layout: Value,
}

impl Default for FixtureSpec {
    fn default() -> Self {
        FixtureSpec {
            title: "Fixture".to_owned(),
            secs: 12,
            size: (1280, 720),
            fps: 30,
            camera: true,
            audio: true,
            cursor: true,
            pause: Some(FixturePause {
                at_s: 5.5,
                len_s: 1.0,
            }),
            camera_layout: json!({
                "visible": true, "shape": "circle", "rect": [0.70, 0.55, 0.20, 0.36],
                "mirror": true, "size": "m"
            }),
        }
    }
}

impl FixtureSpec {
    /// Flash times in screen file time (s) inside the recording.
    pub fn sync_times_s(&self) -> Vec<f64> {
        let mut v = Vec::new();
        let mut t = SYNC_FIRST_S;
        while t + SYNC_LEN_S < f64::from(self.secs) {
            v.push(t);
            t += SYNC_PERIOD_S;
        }
        v
    }

    /// Session time (s) of a screen file time: the start offset plus the pause if it is passed.
    pub fn session_of_screen_s(&self, file_s: f64) -> f64 {
        let gap = self
            .pause
            .filter(|p| file_s >= p.at_s)
            .map_or(0.0, |p| p.len_s);
        SCREEN_START_MS as f64 / 1000.0 + file_s + gap
    }
}

/// A fixture that was written.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub dir: PathBuf,
    pub id: String,
    pub title: String,
    /// Session time (s) at which every track has ended.
    pub session_end_s: f64,
}

fn err(msg: impl Into<String>) -> CmError {
    CmError::internal(msg.into())
}

fn ff(tools: &Tools, args: &[String]) -> Result<(), CmError> {
    let out = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| err(format!("cannot run FFmpeg: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(err(format!(
            "fixture FFmpeg failed: {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )))
    }
}

fn s(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// `enable` expression true during each flash, with the track's own start `shift_s` seconds
/// before the screen's (so the flashes line up on the session clock).
fn flash_enable(spec: &FixtureSpec, shift_s: f64) -> String {
    // Times are in the compacted file time of the track. The pause is after the track starts, so
    // a screen file time T is T + shift in this track's file time.
    let parts: Vec<String> = spec
        .sync_times_s()
        .into_iter()
        .map(|t| {
            let a = t + shift_s;
            format!("between(t,{a:.3},{:.3})", a + SYNC_LEN_S)
        })
        .collect();
    parts.join("+")
}

fn ticks(secs: f64) -> i64 {
    (secs * TICKS_PER_SECOND as f64).round() as i64
}

fn cursor_lines(spec: &FixtureSpec, end_session_s: f64) -> Result<String, CmError> {
    let (w, h) = (f64::from(spec.size.0), f64::from(spec.size.1));
    let target = CursorTarget {
        kind: TargetKind::Display,
        display_id: Some(1),
        origin: [0.0, 0.0],
        size: PhysicalSize {
            width: spec.size.0,
            height: spec.size.1,
        },
        scale: 1.0,
    };
    let header = CursorHeader {
        ty: "session".into(),
        schema: SCHEMA_VERSION,
        tick_rate: TICKS_PER_SECOND,
        clock: "fixture".into(),
        platform: "fixture".into(),
        target,
        clicks: Availability::Available,
        scroll: Availability::Available,
        move_source: MoveSource::Fake,
        permission_missing: None,
    };
    let gap = spec.pause.map(|p| {
        let from = spec.session_of_screen_s(p.at_s - 1e-6);
        (from, from + p.len_s)
    });
    let in_gap = |t: f64| gap.is_some_and(|(a, b)| t > a && t < b);
    let start = SCREEN_START_MS as f64 / 1000.0;
    let pos = |t: f64| -> (i32, i32) {
        let u = t - start;
        (
            (w * (0.5 + 0.35 * (u * 0.9).sin())) as i32,
            (h * (0.5 + 0.30 * (u * 1.3).cos())) as i32,
        )
    };
    let click_times = [2.0, 6.5, 9.0].map(|f| spec.session_of_screen_s(f));
    let mut events: Vec<CursorEvent> = Vec::new();
    let mut t = start;
    while t < end_session_s {
        if !in_gap(t) {
            let (x, y) = pos(t);
            events.push(CursorEvent::Move {
                t: ticks(t),
                x,
                y,
                outside: false,
            });
        }
        t += 1.0 / 60.0;
    }
    for c in click_times {
        if c < end_session_s && !in_gap(c) {
            let (x, y) = pos(c);
            events.push(CursorEvent::Down {
                t: ticks(c),
                button: Button::Left,
                x,
                y,
                mods: vec![],
                outside: false,
            });
            events.push(CursorEvent::Up {
                t: ticks(c + 0.08),
                button: Button::Left,
                x,
                y,
                mods: vec![],
                outside: false,
            });
        }
    }
    events.sort_by_key(CursorEvent::t);
    let mut out = serde_json::to_string(&header).map_err(|e| err(e.to_string()))?;
    out.push('\n');
    for e in events {
        out.push_str(&serde_json::to_string(&e).map_err(|e| err(e.to_string()))?);
        out.push('\n');
    }
    Ok(out)
}

/// Writes a finished fixture recording into the library folder `library` (created when missing)
/// and returns where it is. Needs FFmpeg with `libx264`; nothing else.
pub fn make_fixture(tools: &Tools, library: &Path, spec: &FixtureSpec) -> Result<Fixture, CmError> {
    let secs = f64::from(spec.secs);
    let (w, h) = spec.size;
    let now = OffsetDateTime::now_utc();
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "fixture".into(),
        displays: vec![],
        devices: json!({}),
        app_version: env!("CARGO_PKG_VERSION").into(),
        extra: Default::default(),
    };
    let mut bundle = RecBundle::create(library, &spec.title, now, json!({}), &session)?;
    let d = bundle.dir().to_path_buf();
    let path = |n: &str| d.join(n).to_string_lossy().into_owned();

    // Screen.
    let flash = format!(
        "drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable='{}',{TO_BT709}",
        flash_enable(spec, 0.0)
    );
    let mut a = s(&["-f", "lavfi", "-i"]);
    a.push(format!(
        "testsrc=size={w}x{h}:rate={}:duration={}",
        spec.fps, spec.secs
    ));
    a.extend(s(&["-vf"]));
    a.push(flash);
    a.extend(s(&[
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
    ]));
    a.extend(s(&BT709_TAGS));
    a.push("-output_ts_offset".into());
    a.push(format!("{}", SCREEN_START_MS as f64 / 1000.0));
    a.extend(s(&["-movflags", "frag_keyframe+empty_moov"]));
    a.push(path("screen.mp4"));
    ff(tools, &a)?;

    let gap_len = spec.pause.map_or(0.0, |p| p.len_s);
    let end_of = |start_ms: i64, file_s: f64| -> MediaTime {
        // Session end of a track that starts at `start_ms` and holds `file_s` seconds of samples.
        let pause_inside = spec
            .pause
            .is_some_and(|p| start_ms as f64 / 1000.0 + p.at_s < start_ms as f64 / 1000.0 + file_s);
        MediaTime::from_ticks(ticks(
            start_ms as f64 / 1000.0 + file_s + if pause_inside { gap_len } else { 0.0 },
        ))
    };
    let mut tracks: Vec<TrackInfo> = Vec::new();
    // A recording made with "record the cursor separately" says the pointer is not baked into the
    // screen file (`tracks.screen.cursor = false`), which is what makes the cursor drawable and
    // editable (EDT-4). The fixture's test pattern has no pointer in it, so this is true of it.
    let screen_details = if spec.cursor {
        json!({"size": [w, h], "fps": spec.fps, "codec": "h264",
               "source_size": [w, h], "scale": 1.0, "cursor": false})
    } else {
        json!({"size": [w, h], "fps": spec.fps, "codec": "h264",
               "source_size": [w, h], "scale": 1.0})
    };
    let info = |kind, file: &str, start_ms: i64, end: MediaTime, details: Value| TrackInfo {
        kind,
        file: file.to_owned(),
        start: MediaTime::from_millis(start_ms),
        end,
        details,
        stats: TrackStats::default(),
    };
    tracks.push(info(
        TrackKind::Screen,
        "screen.mp4",
        SCREEN_START_MS,
        end_of(SCREEN_START_MS, secs),
        screen_details,
    ));

    if spec.camera {
        let shift = (SCREEN_START_MS - CAMERA_START_MS) as f64 / 1000.0;
        let cam_secs = secs - 0.2;
        let mut a = s(&["-f", "lavfi", "-i"]);
        a.push(format!(
            "smptebars=size=640x480:rate={}:duration={cam_secs}",
            spec.fps
        ));
        a.push("-vf".into());
        a.push(format!(
            "drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable='{}',{TO_BT709}",
            flash_enable(spec, shift)
        ));
        a.extend(s(&[
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ]));
        a.extend(s(&BT709_TAGS));
        a.push("-output_ts_offset".into());
        a.push(format!("{}", CAMERA_START_MS as f64 / 1000.0));
        a.extend(s(&["-movflags", "frag_keyframe+empty_moov"]));
        a.push(path("camera.mp4"));
        ff(tools, &a)?;
        tracks.push(info(
            TrackKind::Camera,
            "camera.mp4",
            CAMERA_START_MS,
            end_of(CAMERA_START_MS, cam_secs),
            json!({"size": [640, 480], "fps": spec.fps, "mirror": true}),
        ));
    }

    if spec.audio {
        // Microphone: silence with a beep at each flash (the file starts MIC_START_MS after the
        // origin, so a screen file time T is T + 0.29 s in the mic file).
        let shift = (SCREEN_START_MS - MIC_START_MS) as f64 / 1000.0;
        let mic_secs = secs + shift;
        let mut a = s(&["-f", "lavfi", "-i"]);
        a.push(format!(
            "sine=frequency=1000:sample_rate=48000:duration={mic_secs}"
        ));
        a.push("-af".into());
        a.push(format!(
            "volume=volume=0.6,volume=volume=0:enable='not({})',aformat=sample_fmts=flt:channel_layouts=mono",
            flash_enable(spec, shift)
        ));
        a.extend(s(&["-c:a", "pcm_f32le"]));
        a.push(path("mic.denoised.wav"));
        ff(tools, &a)?;
        tracks.push(info(
            TrackKind::Mic,
            "mic.denoised.wav",
            MIC_START_MS,
            end_of(MIC_START_MS, mic_secs),
            json!({"rate": 48000, "channels": 1}),
        ));

        // System audio: a quiet steady 300 Hz tone.
        let mut a = s(&["-f", "lavfi", "-i"]);
        a.push(format!(
            "sine=frequency=300:sample_rate=48000:duration={secs}"
        ));
        a.extend(s(&[
            "-af",
            "volume=0.1,aformat=sample_fmts=flt:channel_layouts=stereo",
            "-c:a",
            "pcm_f32le",
        ]));
        a.push(path("system.wav"));
        ff(tools, &a)?;
        tracks.push(info(
            TrackKind::System,
            "system.wav",
            SYSTEM_START_MS,
            end_of(SYSTEM_START_MS, secs),
            json!({"rate": 48000, "channels": 2}),
        ));
    }

    let screen_end = tracks[0].end;
    if let Some(p) = spec.pause {
        let from = MediaTime::from_ticks(ticks(spec.session_of_screen_s(p.at_s - 1e-6)));
        bundle.add_gap(PauseGap {
            from,
            to: MediaTime::from_ticks(from.ticks() + ticks(p.len_s)),
        })?;
    }
    if spec.cursor {
        let text = cursor_lines(spec, screen_end.ticks() as f64 / TICKS_PER_SECOND as f64)?;
        fs::write(d.join("cursor.jsonl"), text).map_err(|e| err(e.to_string()))?;
        tracks.push(info(
            TrackKind::Cursor,
            "cursor.jsonl",
            0,
            screen_end,
            json!({}),
        ));
    }
    if spec.camera {
        bundle.set_extra(LAYOUT_KEY, json!({ "camera": spec.camera_layout.clone() }))?;
    }
    // `live_at`: the moment every video and audio track is running, the default in-point.
    let live_at = tracks
        .iter()
        .filter(|t| t.kind != TrackKind::Cursor)
        .map(|t| t.start)
        .max()
        .unwrap_or(MediaTime::ZERO);
    bundle.set_extra("live_at", json!(live_at.ticks()))?;
    let id = bundle.id().to_owned();
    let title = bundle.title().to_owned();
    let session_end_s = tracks
        .iter()
        .map(|t| t.end)
        .max()
        .map_or(0.0, |t| t.ticks() as f64 / TICKS_PER_SECOND as f64);
    bundle.finalize(&tracks, now)?;
    Ok(Fixture {
        dir: d,
        id,
        title,
        session_end_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Option<Tools> {
        match cm_media::tools::locate() {
            Ok(t) if t.ffprobe.is_some() => Some(t),
            _ => {
                eprintln!("SKIPPED: FFmpeg/ffprobe not found (brew install ffmpeg)");
                None
            }
        }
    }

    /// The pure schedule: flashes every 3 s from 1 s, and a pause shifts later session times.
    #[test]
    fn schedule_follows_the_pause() {
        let spec = FixtureSpec::default();
        assert_eq!(spec.sync_times_s(), vec![1.0, 4.0, 7.0, 10.0]);
        assert!((spec.session_of_screen_s(1.0) - 1.39).abs() < 1e-9);
        // 7 s of file time is after the 5.5 s pause: +1 s.
        assert!((spec.session_of_screen_s(7.0) - 8.39).abs() < 1e-9);
    }

    /// The fixture is a finished bundle: the library reads it, the tracks are on disk and the
    /// pause is in the manifest.
    #[test]
    fn writes_a_readable_recording() {
        let Some(t) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let spec = FixtureSpec {
            secs: 8,
            ..FixtureSpec::default()
        };
        let f = make_fixture(&t, dir.path(), &spec).unwrap();
        let m = cm_library::rec::read_manifest(&f.dir).unwrap();
        for k in ["screen", "camera", "mic", "system", "cursor"] {
            assert!(m.tracks.contains_key(k), "track {k}");
        }
        for file in [
            "screen.mp4",
            "camera.mp4",
            "mic.denoised.wav",
            "system.wav",
            "cursor.jsonl",
            "manifest.json",
        ] {
            assert!(f.dir.join(file).is_file(), "{file}");
        }
        assert_eq!(m.gaps.len(), 1);
        assert_eq!(m.gaps[0].to.ticks() - m.gaps[0].from.ticks(), ticks(1.0));
        assert_eq!(m.edits[0]["layout"]["camera"]["shape"], "circle");
        // The default in-point is when the last track (the camera, 580 ms) started.
        assert_eq!(m.extra.get("live_at"), Some(&json!(ticks(0.58))));
        // The screen file is `secs` long (the pause is not in the file).
        let probe = cm_media::tools::probe_file(&t, &f.dir.join("screen.mp4")).unwrap();
        assert!(
            (probe.duration_s.unwrap_or(0.0) - 8.0).abs() < 0.2,
            "{probe:?}"
        );
        // EDT-4: the screen track says the cursor is not baked in, so it is drawable and editable.
        assert_eq!(m.tracks["screen"]["cursor"], json!(false));
        // The cursor log parses and has the three clicks.
        let log = cm_cursor::CursorLog::read(&f.dir.join("cursor.jsonl")).unwrap();
        let downs = log
            .events
            .iter()
            .filter(|e| matches!(e, CursorEvent::Down { .. }))
            .count();
        assert_eq!(downs, 2, "clicks at 2.0 and 6.5 s fall inside an 8 s file");
        // And the library indexes it.
        let mut lib = cm_library::Library::open_or_create(dir.path()).unwrap();
        lib.rescan().unwrap();
        assert!(lib.get(&f.id).is_ok());
    }

    /// EDT-4: a fixture without a cursor log does not claim the cursor is stored separately.
    #[test]
    fn a_fixture_without_cursor_telemetry_keeps_the_cursor_flag_unset() {
        let Some(t) = tools() else { return };
        let dir = tempfile::tempdir().unwrap();
        let spec = FixtureSpec {
            secs: 4,
            cursor: false,
            camera: false,
            audio: false,
            ..FixtureSpec::default()
        };
        let f = make_fixture(&t, dir.path(), &spec).unwrap();
        let m = cm_library::rec::read_manifest(&f.dir).unwrap();
        assert!(
            m.tracks["screen"].get("cursor").is_none(),
            "{}",
            m.tracks["screen"]
        );
    }
}
