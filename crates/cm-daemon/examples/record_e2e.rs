//! End-to-end recording harness: runs the REAL [`RecordingSession`] on this Mac (screen, cursor,
//! mic, camera, system audio, in any combination), then verifies the bundle and prints a report
//! with PASS/FAIL per check against the specs (NFR-2/3/4/5/6, REC-1..4, REC-7, REC-8).
//!
//! ```text
//! cargo run -p cm-daemon --example record_e2e -- --tracks screen,cursor --secs 10
//! cargo run -p cm-daemon --example record_e2e -- --tracks screen,cursor,mic,camera,system \
//!     --secs 20 --pause-at 8 --pause-for 3 --fps 30 --json
//! cargo run -p cm-daemon --example record_e2e -- --kill9-test
//! cargo run -p cm-daemon --example record_e2e -- --analyze-only <bundle.cmrec>
//! ```
//!
//! Privacy and safety:
//! * The screen track is always recorded (the session requires it); the banner names exactly which
//!   sensors are going to be used before anything starts, then waits 3 s.
//! * Mic, camera and system audio are refused unless their permission is ALREADY granted: nothing
//!   here ever prompts. Cursor without Input Monitoring is allowed (it degrades to polling).
//! * No keystrokes, no input injection, no clipboard.
//! * `--secs` (default 12, max 30) bounds the whole session, counted from the clock origin (the
//!   camera can take up to 3 s to start; that time is pre-roll inside the same budget).
//! * The bundle contains your screen: it is deleted at the end unless `--keep`.
//!
//! All times in the report are in milliseconds on the session clock unless noted.
//!
//! Flags: `--tracks screen,cursor,mic,camera,system`, `--secs N`, `--pause-at S --pause-for S`
//! (session seconds), `--fps 30|60`, `--size source|2160p|1440p|1080p|720p` (default 1080p),
//! `--out DIR` (library folder, default a temp dir), `--keep`, `--json` (JSON on stdout, the text
//! report on stderr), `--kill9-test`, `--analyze-only <bundle>`.

#![allow(clippy::too_many_lines)]
#![cfg_attr(not(target_os = "macos"), allow(unused))]

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("record_e2e runs the real recorders and is macOS only for now");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() {
    imp::main();
}

#[cfg(target_os = "macos")]
mod imp {
    use std::collections::BTreeSet;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use cm_audio::{
        list_audio_devices, microphone_permission, screen_recording_permission, MicConfig,
        MicRecorder, SystemAudioConfig, SystemAudioRecorder,
    };
    use cm_camera::{check_camera_permission, list_cameras, CameraConfig, CameraRecorder};
    use cm_capture::record::sck::SckSource;
    use cm_capture::record::{plan_capture, RecorderOptions, ScreenSettings};
    use cm_capture::{CaptureTarget, Capturer, DisplayInfo};
    use cm_core::MediaTime;
    use cm_cursor::{CursorConfig, CursorEvent, CursorLog, CursorRecorder};
    use cm_daemon::recording::{
        CameraTrack, CursorTrack, MicTrack, NoteLevel, RecordingSession, ScreenTrack,
        SessionConfig, SessionTrack, SystemAudioTrack,
    };
    use cm_encode::{scan_file, HardwareEncoder, OutputSize, VideoSettings};
    use cm_library::manifest::RecManifest;
    use cm_library::rec::SessionDisplay;
    use cm_library::Library;
    use serde_json::{json, Map, Value};

    const MAX_SECS: f64 = 30.0;
    const DEFAULT_SECS: f64 = 12.0;

    // ---- thresholds (each is named in the report) -------------------------------------------
    const LATE_PCT_MAX: f64 = 1.0; // NFR-2
    const SYNC_MS: f64 = 50.0; // NFR-3
    const RSS_MB_MAX: f64 = 600.0; // NFR-5 (recording at 4K)
    const CPU_MACHINE_PCT_MAX: f64 = 30.0; // NFR-4 (1080p30 with webcam)
    const STOP_MS_MAX: f64 = 2000.0; // REC-5.AC2
    const FRAGMENT_SECS: f64 = 2.0; // NFR-6
    const KILL9_LOSS_MAX_S: f64 = 2.25; // REC-8.AC1: 2 s plus 250 ms of measurement skew
    const FILE_SPAN_TOL_S: f64 = 0.12; // REC-1.AC1 / REC-4.AC1: 1 frame at 30 fps plus slack
                                       // The last 50-65 ms of system audio is still in flight when stop() runs (measured on hardware in two runs),
                                       // so audio files end that much early; matches the 'ends at the stop time' tolerance below.
    const AUDIO_SPAN_TOL_MS: f64 = 100.0;

    // ---- arguments --------------------------------------------------------------------------

    #[derive(Debug, Clone)]
    struct Args {
        tracks: BTreeSet<String>,
        secs: f64,
        pause_at: Option<f64>,
        pause_for: f64,
        fps: u32,
        size: OutputSize,
        out: Option<PathBuf>,
        keep: bool,
        json: bool,
        kill9: bool,
        analyze_only: Option<PathBuf>,
        child_record: Option<PathBuf>,
    }

    fn usage() -> ! {
        eprintln!(
            "usage: record_e2e [--tracks screen,cursor,mic,camera,system] [--secs N<=30] \
             [--pause-at S --pause-for S] [--fps 30|60] [--size source|2160p|1440p|1080p|720p] \
             [--out DIR] [--keep] [--json] [--kill9-test] [--analyze-only BUNDLE]"
        );
        std::process::exit(2);
    }

    fn parse_args() -> Args {
        let mut a = Args {
            tracks: ["screen", "cursor"].iter().map(|s| s.to_string()).collect(),
            secs: DEFAULT_SECS,
            pause_at: None,
            pause_for: 0.0,
            fps: 30,
            size: OutputSize::P1080,
            out: None,
            keep: false,
            json: false,
            kill9: false,
            analyze_only: None,
            child_record: None,
        };
        let mut it = std::env::args().skip(1);
        let num = |v: Option<String>| -> f64 {
            v.and_then(|s| s.parse::<f64>().ok())
                .unwrap_or_else(|| usage())
        };
        while let Some(f) = it.next() {
            match f.as_str() {
                "--tracks" => {
                    let v = it.next().unwrap_or_else(|| usage());
                    a.tracks = v
                        .split(',')
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect();
                    for t in &a.tracks {
                        if !["screen", "cursor", "mic", "camera", "system"].contains(&t.as_str()) {
                            eprintln!("unknown track `{t}`");
                            usage();
                        }
                    }
                }
                "--secs" => a.secs = num(it.next()).clamp(2.0, MAX_SECS),
                "--pause-at" => a.pause_at = Some(num(it.next())),
                "--pause-for" => a.pause_for = num(it.next()),
                "--fps" => {
                    a.fps = num(it.next()) as u32;
                    if a.fps != 30 && a.fps != 60 {
                        usage();
                    }
                }
                "--size" => {
                    let v = it.next().unwrap_or_else(|| usage());
                    a.size = serde_json::from_value(Value::String(v)).unwrap_or_else(|_| usage());
                }
                "--out" => a.out = Some(PathBuf::from(it.next().unwrap_or_else(|| usage()))),
                "--keep" => a.keep = true,
                "--json" => a.json = true,
                "--kill9-test" => a.kill9 = true,
                "--analyze-only" => {
                    a.analyze_only = Some(PathBuf::from(it.next().unwrap_or_else(|| usage())));
                }
                "--child-record" => {
                    a.child_record = Some(PathBuf::from(it.next().unwrap_or_else(|| usage())));
                }
                _ => usage(),
            }
        }
        a.tracks.insert("screen".into());
        if let Some(p) = a.pause_at {
            if a.pause_for <= 0.0 || p < 1.0 || p + a.pause_for > a.secs - 1.0 {
                eprintln!("--pause-at/--pause-for must fit inside --secs with 1 s to spare");
                usage();
            }
        }
        a
    }

    pub fn main() {
        let args = parse_args();
        let code = if let Some(b) = args.analyze_only.clone() {
            analyze_only(&args, &b)
        } else if let Some(dir) = args.child_record.clone() {
            child_record(&dir)
        } else if args.kill9 {
            kill9_test(&args)
        } else {
            record(&args)
        };
        std::process::exit(code);
    }

    // ---- report -----------------------------------------------------------------------------

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum St {
        Pass,
        Fail,
        Warn,
        Info,
        Skip,
    }

    impl St {
        fn s(self) -> &'static str {
            match self {
                St::Pass => "PASS",
                St::Fail => "FAIL",
                St::Warn => "WARN",
                St::Info => "INFO",
                St::Skip => "SKIP",
            }
        }
    }

    #[derive(Debug, Clone)]
    struct Check {
        section: String,
        name: String,
        status: St,
        value: String,
        threshold: String,
    }

    #[derive(Default)]
    struct Report {
        checks: Vec<Check>,
        section: String,
        facts: Map<String, Value>,
    }

    impl Report {
        fn section(&mut self, s: &str) {
            self.section = s.to_owned();
        }
        fn add(&mut self, status: St, name: &str, value: impl Into<String>, th: &str) {
            self.checks.push(Check {
                section: self.section.clone(),
                name: name.to_owned(),
                status,
                value: value.into(),
                threshold: th.to_owned(),
            });
        }
        fn check(&mut self, ok: bool, name: &str, value: impl Into<String>, th: &str) {
            self.add(if ok { St::Pass } else { St::Fail }, name, value, th);
        }
        fn info(&mut self, name: &str, value: impl Into<String>) {
            self.add(St::Info, name, value, "");
        }
        fn warn_if(&mut self, bad: bool, name: &str, value: impl Into<String>, th: &str) {
            self.add(if bad { St::Warn } else { St::Pass }, name, value, th);
        }
        fn count(&self, s: St) -> usize {
            self.checks.iter().filter(|c| c.status == s).count()
        }

        fn text(&self) -> String {
            let mut out = String::new();
            let mut last = "";
            for c in &self.checks {
                if c.section != last {
                    out.push_str(&format!("\n--- {} ---\n", c.section));
                    last = &c.section;
                }
                let th = if c.threshold.is_empty() {
                    String::new()
                } else {
                    format!("   [{}]", c.threshold)
                };
                out.push_str(&format!(
                    "{} {:<44} {}{}\n",
                    c.status.s(),
                    c.name,
                    c.value,
                    th
                ));
            }
            out.push_str(&format!(
                "\nSUMMARY: {} PASS, {} FAIL, {} WARN, {} SKIP\n",
                self.count(St::Pass),
                self.count(St::Fail),
                self.count(St::Warn),
                self.count(St::Skip)
            ));
            out
        }

        fn json(&self) -> Value {
            json!({
                "summary": {
                    "pass": self.count(St::Pass),
                    "fail": self.count(St::Fail),
                    "warn": self.count(St::Warn),
                    "skip": self.count(St::Skip),
                },
                "facts": self.facts,
                "checks": self.checks.iter().map(|c| json!({
                    "section": c.section,
                    "name": c.name,
                    "status": c.status.s(),
                    "value": c.value,
                    "threshold": c.threshold,
                })).collect::<Vec<_>>(),
            })
        }
    }

    // ---- small helpers ----------------------------------------------------------------------

    fn ms_of(t: i64) -> f64 {
        t as f64 / 120.0
    }

    fn ticks_secs(t: i64) -> f64 {
        t as f64 / 120_000.0
    }

    fn fmt_ms(v: f64) -> String {
        format!("{v:.1} ms")
    }

    fn dir_size(p: &Path) -> u64 {
        let mut total = 0;
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                match e.metadata() {
                    Ok(m) if m.is_dir() => total += dir_size(&path),
                    Ok(m) => total += m.len(),
                    Err(_) => {}
                }
            }
        }
        total
    }

    fn have(cmd: &str) -> bool {
        Command::new(cmd)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    #[derive(Debug, Clone)]
    struct Probe {
        codec: String,
        width: u64,
        height: u64,
        fps: f64,
        duration: Option<f64>,
        start_time: Option<f64>,
        packets: Option<u64>,
    }

    fn frac(s: &str) -> f64 {
        match s.split_once('/') {
            Some((a, b)) => {
                let (a, b) = (
                    a.parse::<f64>().unwrap_or(0.0),
                    b.parse::<f64>().unwrap_or(1.0),
                );
                if b == 0.0 {
                    0.0
                } else {
                    a / b
                }
            }
            None => s.parse().unwrap_or(0.0),
        }
    }

    fn ffprobe_video(path: &Path) -> Option<Probe> {
        let out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-count_packets",
                "-select_streams",
                "v:0",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let v: Value = serde_json::from_slice(&out.stdout).ok()?;
        let s = v["streams"].get(0)?;
        let num = |k: &str| s[k].as_str().and_then(|x| x.parse::<f64>().ok());
        Some(Probe {
            codec: s["codec_name"].as_str().unwrap_or("?").to_owned(),
            width: s["width"].as_u64().unwrap_or(0),
            height: s["height"].as_u64().unwrap_or(0),
            fps: frac(s["avg_frame_rate"].as_str().unwrap_or("0/1")),
            duration: num("duration"),
            start_time: num("start_time"),
            packets: s["nb_read_packets"].as_str().and_then(|x| x.parse().ok()),
        })
    }

    /// Full software decode; `Some(stderr)` is empty when the file decodes cleanly.
    fn ffmpeg_decode_errors(path: &Path) -> Option<String> {
        let out = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "null", "-"])
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }

    /// `(rss_mb, cpu_seconds)` of a process from `ps`.
    fn ps_sample(pid: u32) -> Option<(f64, f64)> {
        let out = Command::new("ps")
            .args(["-o", "rss=,cputime=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut it = text.split_whitespace();
        let rss_kb: f64 = it.next()?.parse().ok()?;
        let cpu = it.next()?.split(':').try_fold(0.0f64, |acc, p| {
            p.parse::<f64>().ok().map(|v| acc * 60.0 + v)
        })?;
        Some((rss_kb / 1024.0, cpu))
    }

    /// Samples the process's RSS and CPU time while recording.
    struct Sampler {
        stop: Arc<AtomicBool>,
        data: Arc<Mutex<(f64, f64, f64)>>,
        join: Option<std::thread::JoinHandle<()>>,
        wall0: Instant,
    }

    impl Sampler {
        fn start() -> Self {
            let pid = std::process::id();
            let stop = Arc::new(AtomicBool::new(false));
            let base = ps_sample(pid).unwrap_or((0.0, 0.0));
            // (peak rss mb, cpu seconds at start, latest cpu seconds)
            let data = Arc::new(Mutex::new((base.0, base.1, base.1)));
            let (s2, d2) = (stop.clone(), data.clone());
            let join = std::thread::spawn(move || {
                while !s2.load(Ordering::SeqCst) {
                    if let Some((rss, cpu)) = ps_sample(pid) {
                        if let Ok(mut d) = d2.lock() {
                            d.0 = d.0.max(rss);
                            d.2 = cpu;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(400));
                }
            });
            Sampler {
                stop,
                data,
                join: Some(join),
                wall0: Instant::now(),
            }
        }

        /// `(peak_rss_mb, cpu_seconds, wall_seconds)`.
        fn finish(mut self) -> (f64, f64, f64) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
            let last = ps_sample(std::process::id());
            let mut d = self.data.lock().map(|g| *g).unwrap_or((0.0, 0.0, 0.0));
            if let Some((rss, cpu)) = last {
                d.0 = d.0.max(rss);
                d.2 = cpu;
            }
            (
                d.0,
                (d.2 - d.1).max(0.0),
                self.wall0.elapsed().as_secs_f64(),
            )
        }
    }

    // ---- run context for the analysis -------------------------------------------------------

    #[derive(Default, Clone)]
    struct Ctx {
        /// What the run asked for: (pause at, pause for) in session seconds.
        pause: Option<(f64, f64)>,
        secs_limit: Option<f64>,
        process_wall_s: Option<f64>,
        begin_ms: Option<f64>,
        resources: Option<(f64, f64, f64)>,
        fps: Option<u32>,
        recovered_expected: bool,
    }

    // ---- analysis ---------------------------------------------------------------------------

    fn analyze(dir: &Path, ctx: &Ctx) -> Report {
        let mut r = Report::default();
        r.section("bundle and manifest");
        let have_ffprobe = have("ffprobe");
        let have_ffmpeg = have("ffmpeg");
        r.info("bundle", dir.display().to_string());
        r.check(
            !dir.join("recording.lock").exists(),
            "recording.lock removed",
            if dir.join("recording.lock").exists() {
                "still there"
            } else {
                "gone"
            },
            "complete bundles have no lock",
        );
        let manifest_text = std::fs::read_to_string(dir.join("manifest.json"));
        let m = match manifest_text
            .as_ref()
            .ok()
            .and_then(|t| RecManifest::from_json(t).ok())
        {
            Some(m) => m,
            None => {
                r.check(
                    false,
                    "manifest.json parses",
                    "missing or invalid",
                    "RecManifest",
                );
                return r;
            }
        };
        let sj: Value = std::fs::read_to_string(dir.join("session.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let sess = m.extra.get("session").cloned().unwrap_or(Value::Null);
        let live_at = m.extra.get("live_at").and_then(Value::as_i64).unwrap_or(0);
        let wall_end = sess.get("wall_end").and_then(Value::as_i64);
        let paused: i64 = m.gaps.iter().map(|g| g.to.ticks() - g.from.ticks()).sum();
        let recovered = m.extra.contains_key("recovered");

        let mut missing = Vec::new();
        if m.schema != 1 {
            missing.push("schema==1");
        }
        if m.id.is_empty() || m.title.is_empty() {
            missing.push("id/title");
        }
        if m.tick_rate != Some(120_000) {
            missing.push("tick_rate");
        }
        if m.duration.is_none_or(|d| d.ticks() <= 0) {
            missing.push("duration");
        }
        if m.edits.is_empty() {
            missing.push("edits");
        }
        if m.revisions.is_empty() {
            missing.push("revisions");
        }
        if m.tracks.is_empty() {
            missing.push("tracks");
        }
        if sj["clock"]["origin_ns"].as_u64().is_none() {
            missing.push("session.json clock.origin_ns");
        }
        if sj["session"]["live_at"].as_i64().is_none() && !recovered {
            missing.push("session.json session.live_at");
        }
        if !recovered {
            if m.extra.get("live_at").is_none() {
                missing.push("manifest live_at");
            }
            if wall_end.is_none() {
                missing.push("manifest session.wall_end");
            }
        }
        let requested: Vec<String> = sess["requested_tracks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_else(|| m.tracks.keys().cloned().collect());
        let warn_tracks: BTreeSet<String> = m
            .extra
            .get("warnings")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|w| w["track"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        for k in &requested {
            if !m.tracks.contains_key(k) && !warn_tracks.contains(k) {
                missing.push("a requested track is neither recorded nor explained by a warning");
            }
        }
        r.check(
            missing.is_empty(),
            "manifest completeness",
            if missing.is_empty() {
                format!("ok: tracks {:?}", m.tracks.keys().collect::<Vec<_>>())
            } else {
                format!("missing {missing:?}")
            },
            "schema, id, tick_rate, duration, edits, revisions, tracks, session.json origin and live_at",
        );
        r.info("title / id", format!("{} / {}", m.title, m.id));
        r.info(
            "duration (manifest)",
            format!("{:.3} s", m.duration.map_or(0.0, |d| d.as_secs_f64())),
        );
        if let Some(w) = wall_end {
            r.info(
                "session wall / live_at / paused",
                format!(
                    "{:.3} s / {:.3} s / {:.3} s",
                    ticks_secs(w),
                    ticks_secs(live_at),
                    ticks_secs(paused)
                ),
            );
        }
        for w in m
            .extra
            .get("warnings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            r.add(
                St::Warn,
                "recorded warning",
                format!(
                    "{}: {}",
                    w["code"].as_str().unwrap_or("?"),
                    w["message"].as_str().unwrap_or("")
                ),
                "an optional track failed or a device changed",
            );
        }
        if recovered {
            r.add(
                if ctx.recovered_expected {
                    St::Info
                } else {
                    St::Warn
                },
                "bundle was recovered after a crash",
                "manifest.recovered present",
                "",
            );
        }
        if let Some(limit) = ctx.secs_limit {
            if let Some(w) = ctx.process_wall_s {
                r.check(
                    w <= limit + 1.0,
                    "session respected --secs",
                    format!("recording wall {w:.2} s"),
                    &format!("<= {limit} s (+1 s stop)"),
                );
            }
        }

        // ---- per-track files -----------------------------------------------------------
        r.section("tracks: files and codecs");
        let screen_fps = m
            .tracks
            .get("screen")
            .and_then(|t| t["fps"].as_u64())
            .or(ctx.fps.map(u64::from));
        let mut scans = std::collections::BTreeMap::new();
        for (name, t) in &m.tracks {
            let file = t["file"].as_str().unwrap_or("");
            let path = dir.join(file);
            let size = std::fs::metadata(&path).map(|x| x.len()).unwrap_or(0);
            r.check(
                size > 0,
                &format!("{name}: file"),
                format!("{file} {} bytes", size),
                "exists, non-empty",
            );
            if t["missing"].as_bool() == Some(true) {
                r.add(
                    St::Fail,
                    &format!("{name}: track marked missing"),
                    t["reason"].to_string(),
                    "",
                );
            }
            if file.ends_with(".mp4") && size > 0 {
                let scan = scan_file(&path);
                let Ok(scan) = scan else {
                    r.check(
                        false,
                        &format!("{name}: fMP4 parse"),
                        "unreadable",
                        "valid fragments",
                    );
                    continue;
                };
                r.check(
                    scan.init.as_ref().is_some_and(|i| i.moov_first) && scan.is_clean(),
                    &format!("{name}: fMP4 structure"),
                    format!(
                        "moov first, {} fragments, clean end: {}",
                        scan.fragments.len(),
                        scan.is_clean()
                    ),
                    "moov before mdat, no partial fragment",
                );
                let durs = scan.fragment_durations_secs();
                let n = durs.len();
                let max = durs.iter().copied().fold(0.0, f64::max);
                // Fragments are cut on 2 s boundaries of file time, so the first one is as short
                // as the track's start offset; the last one is whatever was left at the stop.
                let first_start = scan.start().map_or(0.0, MediaTime::as_secs_f64);
                let off_cadence = durs
                    .iter()
                    .enumerate()
                    .filter(|(i, d)| {
                        if *i == 0 && n > 1 {
                            **d < FRAGMENT_SECS - first_start - 0.25 || **d > FRAGMENT_SECS + 0.25
                        } else if *i + 1 == n {
                            **d > FRAGMENT_SECS + 0.25
                        } else {
                            (**d - FRAGMENT_SECS).abs() > 0.25
                        }
                    })
                    .count();
                r.check(
                    max <= FRAGMENT_SECS + 0.25 && off_cadence == 0,
                    &format!("{name}: fragment cadence"),
                    format!(
                        "{} fragments, secs {:?}",
                        n,
                        durs.iter().map(|d| (d * 100.0).round() / 100.0).collect::<Vec<_>>()
                    ),
                    &format!(
                        "each {FRAGMENT_SECS} s +-0.25 (first = {FRAGMENT_SECS} - start offset, last may be shorter)"
                    ),
                );
                let all_sync = scan.fragments.iter().all(|f| f.starts_with_sync);
                r.check(
                    all_sync,
                    &format!("{name}: fragments start on a sync sample"),
                    all_sync.to_string(),
                    "true",
                );
                if have_ffprobe {
                    match ffprobe_video(&path) {
                        Some(p) => {
                            r.info(
                                &format!("{name}: ffprobe"),
                                format!(
                                    "{} {}x{} {:.2} fps duration {} start {} packets {}",
                                    p.codec,
                                    p.width,
                                    p.height,
                                    p.fps,
                                    p.duration.map_or("?".into(), |d| format!("{d:.3} s")),
                                    p.start_time.map_or("?".into(), |d| format!("{d:.3} s")),
                                    p.packets.map_or("?".into(), |d| d.to_string()),
                                ),
                            );
                            let want_size = t["size"].as_array().map(|a| {
                                (
                                    a.first().and_then(Value::as_u64),
                                    a.get(1).and_then(Value::as_u64),
                                )
                            });
                            if let Some((Some(w), Some(h))) = want_size {
                                r.check(
                                    p.width == w && p.height == h,
                                    &format!("{name}: size matches manifest"),
                                    format!("{}x{} vs {w}x{h}", p.width, p.height),
                                    "equal",
                                );
                            }
                            let want_codec = t["codec"].as_str().unwrap_or("");
                            r.check(
                                (want_codec == "hevc" && p.codec == "hevc")
                                    || (want_codec == "h264" && p.codec == "h264"),
                                &format!("{name}: codec matches manifest"),
                                format!("{} vs {want_codec}", p.codec),
                                "equal",
                            );
                            if let Some(pk) = p.packets {
                                r.check(
                                    pk == scan.frame_count(),
                                    &format!("{name}: ffprobe packets == parsed frames"),
                                    format!("{pk} vs {}", scan.frame_count()),
                                    "equal",
                                );
                            }
                            let fps_want = f64::from(t["fps"].as_u64().unwrap_or(30) as u32);
                            let tol = if name == "camera" { 6.0 } else { 1.0 };
                            r.check(
                                (p.fps - fps_want).abs() <= tol,
                                &format!("{name}: frame rate"),
                                format!("{:.2} fps vs {fps_want}", p.fps),
                                &format!("+-{tol}"),
                            );
                        }
                        None => r.add(
                            St::Fail,
                            &format!("{name}: ffprobe"),
                            "failed to read the file",
                            "readable",
                        ),
                    }
                } else {
                    r.add(
                        St::Skip,
                        &format!("{name}: ffprobe checks"),
                        "ffprobe not found",
                        "",
                    );
                }
                if have_ffmpeg {
                    match ffmpeg_decode_errors(&path) {
                        Some(e) if e.is_empty() => {
                            r.check(
                                true,
                                &format!("{name}: full decode"),
                                "no errors",
                                "ffmpeg -f null",
                            );
                        }
                        Some(e) => {
                            let first: String = e.lines().take(2).collect::<Vec<_>>().join(" | ");
                            r.check(false, &format!("{name}: full decode"), first, "no errors");
                        }
                        None => {}
                    }
                }
                scans.insert(name.clone(), scan);
            }
        }

        // ---- screen ----------------------------------------------------------------------
        if let (Some(t), Some(scan)) = (m.tracks.get("screen"), scans.get("screen")) {
            r.section("screen: timing and counters");
            let st = &t["stats"];
            let (written, late, dropped, delivered) = (
                st["written"].as_u64().unwrap_or(0),
                st["late"].as_u64().unwrap_or(0),
                st["dropped"].as_u64().unwrap_or(0),
                st["delivered"].as_u64().unwrap_or(0),
            );
            let late_pct = if written > 0 {
                100.0 * late as f64 / written as f64
            } else {
                0.0
            };
            r.check(
                late_pct <= LATE_PCT_MAX,
                "late frames (NFR-2)",
                format!(
                    "{late} late of {written} written = {late_pct:.3}%  (delivered {delivered}, dropped {dropped}, repeated {}, coalesced {}, missed slots {})",
                    t["repeated"], t["coalesced"], t["missed_slots"]
                ),
                &format!("<= {LATE_PCT_MAX}%"),
            );
            r.warn_if(dropped > 0, "dropped frames", dropped.to_string(), "0");
            if let Some(h) = t["stopped_early"].as_object() {
                r.add(
                    St::Fail,
                    "screen halted by itself",
                    format!("{h:?}"),
                    "must run to the stop",
                );
            }
            let file_end = scan.end().map_or(0.0, MediaTime::as_secs_f64);
            let file_start = scan.start().map_or(0.0, MediaTime::as_secs_f64);
            if let Some(w) = wall_end {
                let want_end = ticks_secs(w - paused);
                r.check(
                    (file_end - want_end).abs() <= FILE_SPAN_TOL_S,
                    "file end == wall - pauses (REC-1.AC1, REC-4.AC1)",
                    format!(
                        "file ends at {file_end:.3} s; wall {:.3} - paused {:.3} = {want_end:.3} s (diff {:+.0} ms)",
                        ticks_secs(w),
                        ticks_secs(paused),
                        (file_end - want_end) * 1000.0
                    ),
                    &format!("+-{} ms", (FILE_SPAN_TOL_S * 1000.0) as u32),
                );
            }
            if let Some(fps) = screen_fps {
                let span = file_end - file_start;
                let want = span * fps as f64;
                r.check(
                    (scan.frame_count() as f64 - want).abs() <= 2.0,
                    "constant frame rate (REC-3.AC3)",
                    format!(
                        "{} frames for {span:.3} s at {fps} fps (want {want:.1})",
                        scan.frame_count()
                    ),
                    "+-2 frames",
                );
            }
            r.info(
                "first screen frame",
                format!(
                    "at {} (start call to stream running {:.0} ms)",
                    fmt_ms(ms_of(t["start"].as_i64().unwrap_or(0))),
                    t["latency_ms"]["start_call_ms"].as_f64().unwrap_or(0.0)
                ),
            );
            if let Some(stop_ms) = t["latency_ms"]["stop_ms"].as_f64() {
                r.check(
                    stop_ms <= STOP_MS_MAX,
                    "screen stop latency (REC-5.AC2)",
                    fmt_ms(stop_ms),
                    &format!("<= {STOP_MS_MAX} ms"),
                );
            }
        }

        // ---- audio -----------------------------------------------------------------------
        for (name, t) in m
            .tracks
            .iter()
            .filter(|(k, _)| *k == "mic" || *k == "system")
        {
            r.section(&format!("{name}: audio file"));
            let st = &t["stats"];
            r.info(
                &format!("{name}: counters"),
                format!(
                    "delivered {} written {} dropped {} (resyncs {}, drift {} ppm, max offset {} ms)",
                    st["delivered"], st["written"], st["dropped"], t["resyncs"]["count"], t["drift_ppm"], t["max_offset_ms"]
                ),
            );
            let mut files = vec![t["raw_file"]
                .as_str()
                .unwrap_or(t["file"].as_str().unwrap_or(""))
                .to_owned()];
            if let Some(d) = t["denoised_file"].as_str() {
                files.push(d.to_owned());
            }
            let mut frames_by_file = Vec::new();
            for f in &files {
                let p = dir.join(f);
                match cm_audio::read_info(&p) {
                    Ok(info) => {
                        let len = std::fs::metadata(&p).map(|x| x.len()).unwrap_or(0);
                        // The header is 44 bytes (old files) or 80 (RIFF + JUNK, which can switch to
                        // RF64 in place past 4 GiB); the data chunk runs to the end of the file.
                        let header = len.saturating_sub(info.data_bytes);
                        let ok = info.sample_rate == 48_000
                            && len >= info.data_bytes
                            && matches!(header, 44 | 80)
                            && info.data_bytes % (u64::from(info.channels) * 4) == 0;
                        r.check(
                            ok,
                            &format!("{f}: WAV valid"),
                            format!(
                                "{} Hz, {} ch, {} frames, file {} bytes = header {} + data {}",
                                info.sample_rate,
                                info.channels,
                                info.frames(),
                                len,
                                header,
                                info.data_bytes
                            ),
                            "48 kHz, header matches the data",
                        );
                        frames_by_file.push(info.frames());
                        // Level: a dead mic or a sleeping display records digital silence.
                        if let Some((peak, rms)) = wav_levels(&p, info.data_bytes) {
                            r.add(
                                if peak <= -90.0 { St::Warn } else { St::Info },
                                &format!("{f}: level"),
                                format!("peak {peak:.1} dBFS, rms {rms:.1} dBFS"),
                                "not digital silence",
                            );
                        }
                    }
                    Err(e) => r.check(false, &format!("{f}: WAV valid"), e.to_string(), "readable"),
                }
            }
            if frames_by_file.len() == 2 {
                let d = frames_by_file[0].abs_diff(frames_by_file[1]);
                r.check(
                    d <= 480,
                    "raw vs denoised length",
                    format!("{} vs {} frames", frames_by_file[0], frames_by_file[1]),
                    "<= 10 ms apart",
                );
            }
            if let (Some(frames), Some(w)) = (frames_by_file.first(), wall_end) {
                let start = t["start"].as_i64().unwrap_or(0);
                let in_pause: i64 = m
                    .gaps
                    .iter()
                    .map(|g| (g.to.ticks().min(w) - g.from.ticks().max(start)).max(0))
                    .sum();
                let want_ms = ms_of(w - start - in_pause);
                let got_ms = *frames as f64 / 48.0;
                r.check(
                    (got_ms - want_ms).abs() <= AUDIO_SPAN_TOL_MS,
                    &format!("{name}: duration == wall - first sample - pauses"),
                    format!(
                        "file {got_ms:.1} ms vs expected {want_ms:.1} ms (diff {:+.1} ms)",
                        got_ms - want_ms
                    ),
                    &format!("+-{AUDIO_SPAN_TOL_MS} ms"),
                );
            }
            let dropped = st["dropped"].as_u64().unwrap_or(0);
            r.warn_if(
                dropped > 0,
                &format!("{name}: dropped buffers"),
                dropped.to_string(),
                "0",
            );
        }

        // ---- camera ----------------------------------------------------------------------
        if let (Some(t), Some(scan)) = (m.tracks.get("camera"), scans.get("camera")) {
            r.section("camera: timing and counters");
            let st = &t["stats"];
            let (written, late, dropped) = (
                st["written"].as_u64().unwrap_or(0),
                st["late"].as_u64().unwrap_or(0),
                st["dropped"].as_u64().unwrap_or(0),
            );
            let late_pct = if written > 0 {
                100.0 * late as f64 / written as f64
            } else {
                0.0
            };
            r.check(
                late_pct <= LATE_PCT_MAX,
                "late frames (NFR-2)",
                format!("{late} late of {written} written = {late_pct:.3}%"),
                &format!("<= {LATE_PCT_MAX}%"),
            );
            r.warn_if(dropped > 0, "dropped frames", dropped.to_string(), "0");
            r.check(
                scan.frame_count() == written,
                "frames in file == written",
                format!("{} vs {written}", scan.frame_count()),
                "equal",
            );
            r.info("delivery latency", t["delivery_latency_ms"].to_string());
            let pts_start = scan.start().map_or(0, MediaTime::ticks);
            let pts_end = scan.end().map_or(0, MediaTime::ticks);
            r.check(
                (pts_start - t["start"].as_i64().unwrap_or(0)).abs() <= 400,
                "camera file starts at manifest start",
                format!(
                    "file {} vs manifest {}",
                    fmt_ms(ms_of(pts_start)),
                    fmt_ms(ms_of(t["start"].as_i64().unwrap_or(0)))
                ),
                "+-3 ms",
            );
            if paused > 0 {
                // REC-4: like the screen and audio, the camera file has the pause gaps removed:
                // file end == manifest end (session time) - the gaps that ended before it.
                let end = t["end"].as_i64().unwrap_or(0);
                let removed: i64 = m
                    .gaps
                    .iter()
                    .filter(|g| g.to.ticks() <= end)
                    .map(|g| g.to.ticks() - g.from.ticks())
                    .sum();
                let drops = (pts_end - (end - removed)).abs() <= 2400;
                let keeps = !drops && (pts_end - end).abs() <= 2400;
                r.check(
                    drops,
                    "camera pause convention",
                    format!(
                        "file end {} ; manifest end {} ; gaps removed {}: {}",
                        fmt_ms(ms_of(pts_end)),
                        fmt_ms(ms_of(end)),
                        fmt_ms(ms_of(removed)),
                        if drops {
                            "timestamps drop the pause (continuous, like screen and audio)"
                        } else if keeps {
                            "timestamps KEEP session time (the file has a hole where the pause was)"
                        } else {
                            "neither"
                        }
                    ),
                    "file end == manifest end - pause gaps (+-20 ms); cursor alone stays in session time",
                );
            }
        }

        // ---- cursor ----------------------------------------------------------------------
        let mut cursor_first = None;
        if let Some(t) = m.tracks.get("cursor") {
            r.section("cursor");
            match CursorLog::read(&dir.join("cursor.jsonl")) {
                Ok(log) => {
                    let (mut mv, mut down, mut up, mut scroll, mut cur) = (0, 0, 0, 0, 0);
                    for e in &log.events {
                        match e {
                            CursorEvent::Move { .. } => mv += 1,
                            CursorEvent::Down { .. } => down += 1,
                            CursorEvent::Up { .. } => up += 1,
                            CursorEvent::Scroll { .. } => scroll += 1,
                            CursorEvent::Cursor { .. } => cur += 1,
                            CursorEvent::Unknown => {}
                        }
                    }
                    r.check(
                        !log.truncated,
                        "cursor.jsonl complete",
                        format!("truncated={}", log.truncated),
                        "false after a clean stop",
                    );
                    r.info(
                        "events",
                        format!("{mv} move, {down} down, {up} up, {scroll} scroll, {cur} cursor-image, total {}", log.events.len()),
                    );
                    r.info(
                        "header",
                        format!(
                            "clicks {:?}, scroll {:?}, move_source {:?}, permission_missing {:?}, clock {}",
                            log.header.clicks, log.header.scroll, log.header.move_source, log.header.permission_missing, log.header.clock
                        ),
                    );
                    r.warn_if(
                        log.header.permission_missing.is_some(),
                        "Input Monitoring",
                        format!("{:?}", log.header.permission_missing),
                        "granted: clicks recorded; otherwise polling, no clicks (REC-7.AC4)",
                    );
                    r.warn_if(
                        mv == 0,
                        "cursor moved during the recording",
                        mv.to_string(),
                        ">0 (move the mouse)",
                    );
                    r.check(
                        down == up,
                        "down/up balanced",
                        format!("{down} down / {up} up"),
                        "equal",
                    );
                    if let (Some(f), Some(l)) = (log.events.first(), log.events.last()) {
                        cursor_first = Some(f.t());
                        let end = wall_end.unwrap_or(i64::MAX);
                        r.check(
                            f.t() >= 0 && l.t() <= end,
                            "first/last event inside the session span",
                            format!(
                                "{} .. {} (session 0 .. {})",
                                fmt_ms(ms_of(f.t())),
                                fmt_ms(ms_of(l.t())),
                                wall_end.map_or("?".into(), |w| fmt_ms(ms_of(w)))
                            ),
                            "0 <= first, last <= wall end",
                        );
                    }
                    let inside: usize = log
                        .events
                        .iter()
                        .filter(|e| {
                            m.gaps
                                .iter()
                                .any(|g| e.t() > g.from.ticks() && e.t() < g.to.ticks())
                        })
                        .count();
                    r.check(
                        inside == 0,
                        "no cursor events inside pause gaps",
                        format!("{inside} inside"),
                        "0 (re-anchor lines sit exactly on the resume time)",
                    );
                    r.check(
                        log.events.windows(2).all(|w| w[0].t() <= w[1].t()),
                        "events in time order",
                        "ordered",
                        "non-decreasing t",
                    );
                    let imgs = std::fs::read_dir(dir.join("cursors"))
                        .map(|rd| {
                            rd.flatten()
                                .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
                                .count()
                        })
                        .unwrap_or(0);
                    r.info(
                        "cursor images",
                        format!("{imgs} PNG, header images {}", t["images"]),
                    );
                    // REC-7.AC4: session.json says when clicks are unavailable.
                    if log.header.permission_missing.is_some() {
                        r.check(
                            sj["cursor"]["clicks"] == "unavailable",
                            "session.json cursor.clicks (REC-7.AC4)",
                            sj["cursor"]["clicks"].to_string(),
                            "\"unavailable\"",
                        );
                    }
                }
                Err(e) => r.check(
                    false,
                    "cursor.jsonl parses (CursorLog)",
                    e.to_string(),
                    "valid",
                ),
            }
        }

        // ---- alignment -------------------------------------------------------------------
        r.section("alignment (NFR-3) and pauses");
        let screen_start = m.tracks.get("screen").and_then(|t| t["start"].as_i64());
        if let Some(s0) = screen_start {
            r.info(
                "screen first sample",
                format!(
                    "{} on the session clock (live_at {})",
                    fmt_ms(ms_of(s0)),
                    fmt_ms(ms_of(live_at))
                ),
            );
            for (name, t) in &m.tracks {
                if name == "screen" {
                    continue;
                }
                let start = if name == "cursor" {
                    cursor_first.unwrap_or_else(|| t["start"].as_i64().unwrap_or(0))
                } else {
                    t["start"].as_i64().unwrap_or(0)
                };
                let off = ms_of(start - s0);
                r.add(
                    St::Info,
                    &format!("{name}: first sample vs screen"),
                    format!("{off:+.1} ms (start {})", fmt_ms(ms_of(start))),
                    "start offsets are device start latency, kept on the clock; not drift",
                );
                r.check(
                    start >= 0,
                    &format!("{name}: no sample before the clock origin"),
                    fmt_ms(ms_of(start)),
                    ">= 0",
                );
                r.warn_if(
                    ms_of(start - live_at) > 1000.0,
                    &format!("{name}: first sample long after live_at"),
                    fmt_ms(ms_of(start - live_at)),
                    "<= 1000 ms",
                );
            }
            if let Some(w) = wall_end {
                for (name, t) in &m.tracks {
                    let end = t["end"].as_i64().unwrap_or(0);
                    let tol = if name == "camera" { 260.0 } else { 100.0 };
                    r.check(
                        (ms_of(w - end)).abs() <= tol && end <= w + 120,
                        &format!("{name}: ends at the stop time"),
                        format!("{} before the stop", fmt_ms(ms_of(w - end))),
                        &format!("0..{tol} ms (camera delivery lag is ~75 ms)"),
                    );
                }
            }
            r.add(
                St::Skip,
                "NFR-3: A/V/camera sync within 50 ms",
                "needs an optical/acoustic stimulus; use tests/manual/CAM-2.md step 4 and AUD-1.md step 4 on this bundle",
                &format!("<= {SYNC_MS} ms"),
            );
        }
        // Pause accounting.
        r.info(
            "pause gaps (manifest)",
            format!(
                "{:?}",
                m.gaps
                    .iter()
                    .map(|g| (ms_of(g.from.ticks()), ms_of(g.to.ticks())))
                    .collect::<Vec<_>>()
            ),
        );
        if let Some((at, dur)) = ctx.pause {
            r.check(
                m.gaps.len() == 1,
                "one pause gap recorded",
                m.gaps.len().to_string(),
                "1",
            );
            if let Some(g) = m.gaps.first() {
                let len = ticks_secs(g.to.ticks() - g.from.ticks());
                r.check(
                    (len - dur).abs() <= 0.25,
                    "pause length",
                    format!("{len:.3} s vs requested {dur} s"),
                    "+-250 ms",
                );
                r.check(
                    (ticks_secs(g.from.ticks()) - at).abs() <= 0.5,
                    "pause start",
                    format!("{:.3} s vs requested {at} s", ticks_secs(g.from.ticks())),
                    "+-500 ms",
                );
            }
        }
        for (name, t) in &m.tracks {
            // R3-6: every track lists its pauses under `pauses` (`gaps` is audio dropouts only).
            let theirs: Vec<(i64, i64)> = t["pauses"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|g| {
                            Some((
                                g["from"].as_i64().or_else(|| g["from"]["0"].as_i64())?,
                                g["to"].as_i64()?,
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !m.gaps.is_empty() && !recovered {
                let mine: Vec<(i64, i64)> = m
                    .gaps
                    .iter()
                    .map(|g| (g.from.ticks(), g.to.ticks()))
                    .collect();
                // Audio only lists pauses after its first sample; others list all.
                let same = theirs.len() <= mine.len()
                    && theirs.iter().all(|g| {
                        mine.iter()
                            .any(|x| (x.0 - g.0).abs() <= 120 && (x.1 - g.1).abs() <= 120)
                    });
                r.check(
                    same && !theirs.is_empty(),
                    &format!("{name}: pause gaps agree with the session"),
                    format!("{theirs:?}"),
                    "same from/to (+-1 ms)",
                );
            }
        }

        // ---- timing, resources, size ------------------------------------------------------
        r.section("start/stop, resources, size");
        if let Some(b) = ctx.begin_ms {
            r.info("begin() (all tracks started)", fmt_ms(b));
        }
        if let Some(map) = sess["start_ms"].as_object() {
            r.info(
                "per-track start() duration",
                map.iter()
                    .map(|(k, v)| format!("{k} {:.0} ms", v.as_f64().unwrap_or(0.0)))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if let Some(s) = sess["stop_ms"].as_f64() {
            r.check(
                s <= STOP_MS_MAX,
                "stop() latency, all tracks (REC-5.AC2)",
                fmt_ms(s),
                &format!("<= {STOP_MS_MAX} ms"),
            );
        }
        if let Some((rss, cpu, wall)) = ctx.resources {
            let ncpu = std::thread::available_parallelism().map_or(1, usize::from) as f64;
            let one_core = if wall > 0.0 { 100.0 * cpu / wall } else { 0.0 };
            let machine = one_core / ncpu;
            r.check(
                rss <= RSS_MB_MAX,
                "peak RSS of this process (NFR-5)",
                format!("{rss:.0} MB"),
                &format!("<= {RSS_MB_MAX} MB"),
            );
            r.add(
                if machine < CPU_MACHINE_PCT_MAX { St::Pass } else { St::Fail },
                "CPU of this process (NFR-4)",
                format!("{one_core:.0}% of one core = {machine:.1}% of the machine ({ncpu} cores); GPU/media engine time not included"),
                &format!("< {CPU_MACHINE_PCT_MAX}% of the machine at 1080p30 + webcam"),
            );
        }
        let total = dir_size(dir);
        let secs = m.duration.map_or(0.0, |d| d.as_secs_f64());
        let mut per: Vec<(String, u64)> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| {
                        (
                            e.file_name().to_string_lossy().into_owned(),
                            if e.path().is_dir() {
                                dir_size(&e.path())
                            } else {
                                e.metadata().map(|x| x.len()).unwrap_or(0)
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        per.sort();
        r.info(
            "bundle size",
            format!(
                "{:.2} MB ({:.1} MB/min): {}",
                total as f64 / 1e6,
                if secs > 0.0 {
                    total as f64 / 1e6 * 60.0 / secs
                } else {
                    0.0
                },
                per.iter()
                    .map(|(n, s)| format!("{n} {:.2} MB", *s as f64 / 1e6))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        r.facts.insert("bundle_bytes".into(), json!(total));
        r.facts.insert(
            "manifest_tracks".into(),
            json!(m.tracks.keys().collect::<Vec<_>>()),
        );
        r.facts.insert("gaps".into(), json!(m.gaps));
        r.facts.insert("live_at_ms".into(), json!(ms_of(live_at)));
        r
    }

    /// `(peak dBFS, rms dBFS)` of a float WAV whose data chunk is `data_bytes` long and ends the file.
    fn wav_levels(p: &Path, data_bytes: u64) -> Option<(f64, f64)> {
        let mut f = std::fs::File::open(p).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        let start = buf.len().checked_sub(usize::try_from(data_bytes).ok()?)?;
        let data = buf.get(start..)?;
        let (mut peak, mut sum, mut n) = (0.0f64, 0.0f64, 0u64);
        for c in data.as_chunks::<4>().0.iter() {
            let v = f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
            peak = peak.max(v.abs());
            sum += v * v;
            n += 1;
        }
        if n == 0 {
            return None;
        }
        let db = |x: f64| if x <= 1e-6 { -120.0 } else { 20.0 * x.log10() };
        Some((db(peak), db((sum / n as f64).sqrt())))
    }

    // ---- record -----------------------------------------------------------------------------

    struct Cleanup {
        path: PathBuf,
        keep: bool,
    }

    impl Drop for Cleanup {
        fn drop(&mut self) {
            if self.keep {
                eprintln!("kept: {}", self.path.display());
            } else {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    fn primary_display() -> Result<DisplayInfo, String> {
        let t = Capturer::system()
            .list_displays()
            .map_err(|e| format!("cannot list displays: {e}"))?;
        t.displays
            .iter()
            .find(|d| d.is_primary)
            .or(t.displays.first())
            .cloned()
            .ok_or_else(|| "no display found".to_owned())
    }

    fn screen_track(args: &Args, display: &DisplayInfo) -> Result<Box<dyn SessionTrack>, String> {
        let mut s = ScreenSettings::new(CaptureTarget::Display {
            display_id: display.id,
        });
        s.video = VideoSettings {
            size: args.size,
            fps: args.fps,
            ..Default::default()
        };
        let targets = cm_capture::Targets {
            displays: vec![display.clone()],
            windows: vec![],
        };
        let plan = plan_capture(&s, &targets).map_err(|e| e.to_string())?;
        Ok(Box::new(ScreenTrack::new(
            plan,
            SckSource::new(),
            HardwareEncoder::create,
            RecorderOptions::default(),
        )))
    }

    fn cursor_track(display: &DisplayInfo) -> Box<dyn SessionTrack> {
        let target = cm_cursor::main_display_target(Some(display.id));
        Box::new(CursorTrack::new(CursorRecorder::new(CursorConfig::new(
            target,
        ))))
    }

    fn record(args: &Args) -> i32 {
        // 1. Refuse before anything starts.
        let display = match primary_display() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("REFUSED: {e}\nGrant Screen Recording to this terminal in System Settings, then restart it.");
                return 3;
            }
        };
        let want = |k: &str| args.tracks.contains(k);
        let mut refusals = Vec::new();
        if want("mic") && !microphone_permission().is_authorized() {
            refusals.push(format!(
                "mic: microphone permission is {:?}, not Authorized (this tool never prompts)",
                microphone_permission()
            ));
        }
        if want("camera") && !check_camera_permission().is_authorized() {
            refusals.push(format!(
                "camera: camera permission is {:?}, not Authorized (this tool never prompts)",
                check_camera_permission()
            ));
        }
        if want("system") && !screen_recording_permission().is_authorized() {
            refusals.push("system: Screen Recording permission is not Authorized".to_owned());
        }
        if !refusals.is_empty() {
            for r in &refusals {
                eprintln!("REFUSED: {r}");
            }
            return 3;
        }

        // 2. Banner.
        let mic_name = list_audio_devices()
            .mics
            .iter()
            .find(|m| m.default)
            .map_or("(default microphone)".to_owned(), |m| m.name.clone());
        let cam_name = list_cameras()
            .ok()
            .and_then(|c| {
                c.iter()
                    .find(|c| c.is_default)
                    .or(c.first())
                    .map(|c| c.name.clone())
            })
            .unwrap_or_else(|| "(default camera)".to_owned());
        let cursor_perm = cm_cursor::preflight();
        eprintln!("==================== record_e2e: SENSORS IN USE ====================");
        eprintln!(
            "SCREEN   display {} \"{}\" {}x{} -> {:?} at {} fps (always on)",
            display.id,
            display.name,
            display.bounds.size.width,
            display.bounds.size.height,
            args.size,
            args.fps
        );
        if want("cursor") {
            eprintln!(
                "CURSOR   pointer position, clicks and scroll direction only; NO keystrokes. Input Monitoring: {}",
                match cursor_perm.missing_permission() {
                    None => "granted (event tap)".to_owned(),
                    Some(p) => format!("NOT granted ({p:?}): position polling only, no clicks"),
                }
            );
        }
        if want("mic") {
            eprintln!("MIC      ON: microphone \"{mic_name}\" (raw + denoised WAV)");
        }
        if want("camera") {
            eprintln!("CAMERA   ON: \"{cam_name}\" 1080p (camera light will turn on)");
        }
        if want("system") {
            eprintln!("SYSTEM   ON: system audio output (everything the Mac plays; needs a non-sleeping display)");
        }
        for k in ["mic", "camera", "system", "cursor"] {
            if !want(k) {
                eprintln!("{:<8} off", k.to_uppercase());
            }
        }
        eprintln!(
            "LIMITS   {} s from the clock origin{}; bundle {}",
            args.secs,
            args.pause_at.map_or(String::new(), |p| format!(
                "; pause at {p} s for {} s",
                args.pause_for
            )),
            if args.keep {
                "KEPT"
            } else {
                "deleted at the end"
            }
        );
        eprintln!("Starting in 3 s (Ctrl-C to cancel)...");
        let _ = std::io::stderr().flush();
        std::thread::sleep(Duration::from_secs(3));

        // 3. Output location.
        let (root, created_temp) = match &args.out {
            Some(p) => (p.clone(), false),
            None => (
                std::env::temp_dir().join(format!("cm-record-e2e-{}", std::process::id())),
                true,
            ),
        };
        let _ = std::fs::create_dir_all(&root);
        // A temp folder is ours: delete all of it. A folder the user gave is only ever touched
        // through the bundle inside it, once that exists.
        let mut cleanup = Cleanup {
            path: if created_temp {
                root.clone()
            } else {
                root.join("(no bundle yet)")
            },
            keep: args.keep,
        };

        // 4. Build the tracks and the session.
        let mut tracks: Vec<Box<dyn SessionTrack>> = Vec::new();
        match screen_track(args, &display) {
            Ok(t) => tracks.push(t),
            Err(e) => {
                eprintln!("cannot plan the screen track: {e}");
                return 1;
            }
        }
        if want("cursor") {
            tracks.push(cursor_track(&display));
        }
        if want("mic") {
            tracks.push(Box::new(MicTrack::new(MicRecorder::new(
                MicConfig::default(),
            ))));
        }
        if want("system") {
            tracks.push(Box::new(SystemAudioTrack::new(SystemAudioRecorder::new(
                SystemAudioConfig::default(),
            ))));
        }
        if want("camera") {
            tracks.push(Box::new(CameraTrack::new(CameraRecorder::system(
                CameraConfig::default(),
            ))));
        }
        let mut cfg = SessionConfig::new(&root);
        cfg.title = Some("e2e".into());
        cfg.settings = json!({
            "fps": args.fps,
            "size": args.size,
            "tracks": args.tracks.iter().collect::<Vec<_>>(),
            "harness": "record_e2e",
        });
        cfg.displays = vec![SessionDisplay {
            id: display.id,
            name: display.name.clone(),
            bounds: display.bounds,
            scale: display.scale,
            is_primary: display.is_primary,
        }];

        let sampler = Sampler::start();
        let t0 = Instant::now();
        let mut session = match RecordingSession::begin(cfg, tracks) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("RECORDING NOT STARTED: {e}");
                return 1;
            }
        };
        let begin_ms = t0.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "recording... begin() took {begin_ms:.0} ms, live_at {:.0} ms",
            ms_of(session.live_at().ticks())
        );
        for w in session.warnings() {
            eprintln!(
                "  WARNING at start: {:?} {}: {}",
                w.track, w.code, w.message
            );
        }

        // 5. Run until the deadline; pause/resume on schedule.
        let deadline = t0 + Duration::from_secs_f64(args.secs);
        let mut paused_done = false;
        let mut resumed_done = args.pause_at.is_none();
        let mut early_stop = None;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            let now_s = t0.elapsed().as_secs_f64();
            if let Some(p) = args.pause_at {
                if !paused_done && now_s >= p {
                    match session.pause() {
                        Ok(()) => eprintln!("paused at {now_s:.2} s"),
                        Err(e) => eprintln!("pause failed: {e}"),
                    }
                    paused_done = true;
                }
                if paused_done && !resumed_done && now_s >= p + args.pause_for {
                    match session.resume() {
                        Ok(()) => eprintln!("resumed at {now_s:.2} s"),
                        Err(e) => eprintln!("resume failed: {e}"),
                    }
                    resumed_done = true;
                }
            }
            for n in session.tick() {
                eprintln!("  [{:?}] {:?} {}: {}", n.level, n.track, n.code, n.message);
                if n.level == NoteLevel::StopRequested {
                    early_stop = Some(n.code);
                }
            }
            if early_stop.is_some() {
                break;
            }
        }
        if let Some(c) = &early_stop {
            eprintln!("stopping early: {c}");
        }
        let stopped = match session.stop() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("STOP FAILED: {e}");
                return 1;
            }
        };
        let process_wall = t0.elapsed().as_secs_f64();
        let resources = sampler.finish();

        // 6. Finalize into a library (so indexing is exercised too), then verify.
        let finalized = match Library::open_or_create(&root) {
            Ok(mut lib) => stopped
                .finish_into(&mut lib)
                .map(|rec| (root.join(&rec.path), Some(rec))),
            Err(e) => {
                eprintln!("cannot open the library at {}: {e}", root.display());
                stopped.finalize().map(|(_, d)| (d, None))
            }
        };
        let (dir, rec) = match finalized {
            Ok(v) => v,
            Err(e) => {
                eprintln!("FINALIZE FAILED: {e}");
                return 1;
            }
        };
        if created_temp {
            cleanup.path = root.clone();
        } else {
            cleanup.path = dir.clone();
        }
        let ctx = Ctx {
            pause: args.pause_at.map(|p| (p, args.pause_for)),
            secs_limit: Some(args.secs),
            process_wall_s: Some(process_wall),
            begin_ms: Some(begin_ms),
            resources: Some(resources),
            fps: Some(args.fps),
            recovered_expected: false,
        };
        let mut report = analyze(&dir, &ctx);
        report.section("library");
        report.check(
            rec.as_ref()
                .is_some_and(|r| r.duration.is_some_and(|d| d.ticks() > 0)),
            "indexed with a duration (finish_recording)",
            format!("{:?}", rec.as_ref().map(|r| r.duration)),
            "Some(>0)",
        );
        print_report(args, &report)
    }

    fn print_report(args: &Args, report: &Report) -> i32 {
        let text = report.text();
        if args.json {
            eprintln!("\n==================== VERIFICATION REPORT ====================\n{text}");
            println!(
                "{}",
                serde_json::to_string_pretty(&report.json()).unwrap_or_default()
            );
        } else {
            println!("\n==================== VERIFICATION REPORT ====================\n{text}");
        }
        i32::from(report.count(St::Fail) > 0)
    }

    fn analyze_only(args: &Args, bundle: &Path) -> i32 {
        let report = analyze(bundle, &Ctx::default());
        print_report(args, &report)
    }

    // ---- kill -9 test -----------------------------------------------------------------------

    /// Child half: records screen + cursor into a library and never stops by itself (the parent
    /// kills it; the 30 s limit only protects against an orphan).
    fn child_record(root: &Path) -> i32 {
        let display = match primary_display() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("child: {e}");
                return 3;
            }
        };
        let args = Args {
            tracks: BTreeSet::new(),
            secs: MAX_SECS,
            pause_at: None,
            pause_for: 0.0,
            fps: 30,
            size: OutputSize::P1080,
            out: None,
            keep: false,
            json: false,
            kill9: false,
            analyze_only: None,
            child_record: None,
        };
        let tracks = match screen_track(&args, &display) {
            Ok(t) => vec![t, cursor_track(&display)],
            Err(e) => {
                eprintln!("child: {e}");
                return 1;
            }
        };
        let mut cfg = SessionConfig::new(root);
        cfg.title = Some("kill9".into());
        cfg.settings = json!({"fps": 30, "harness": "record_e2e --kill9-test"});
        let mut session = match RecordingSession::begin(cfg, tracks) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("child: begin failed: {e}");
                return 1;
            }
        };
        println!(
            "LIVE {} {}",
            session.live_at().ticks(),
            session.dir().display()
        );
        let _ = std::io::stdout().flush();
        let end = Instant::now() + Duration::from_secs_f64(MAX_SECS);
        while Instant::now() < end {
            std::thread::sleep(Duration::from_millis(100));
            let _ = session.tick();
        }
        0
    }

    fn kill9_test(args: &Args) -> i32 {
        if primary_display().is_err() {
            eprintln!("REFUSED: Screen Recording permission is missing");
            return 3;
        }
        eprintln!("==================== record_e2e --kill9-test ====================");
        eprintln!("SENSORS: SCREEN (child process) and CURSOR position only. No mic, camera or system audio.");
        let root = args.out.clone().unwrap_or_else(|| {
            std::env::temp_dir().join(format!("cm-kill9-{}", std::process::id()))
        });
        let _ = std::fs::create_dir_all(&root);
        // Never delete a folder the user named: only the bundle inside it.
        let _cleanup = Cleanup {
            path: if args.out.is_none() {
                root.clone()
            } else {
                root.join("kill9.cmrec")
            },
            keep: args.keep,
        };
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("cannot find my own executable: {e}");
                return 1;
            }
        };
        eprintln!("Starting in 3 s...");
        std::thread::sleep(Duration::from_secs(3));
        let mut child = match Command::new(exe)
            .arg("--child-record")
            .arg(&root)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cannot spawn the child: {e}");
                return 1;
            }
        };
        let Some(out) = child.stdout.take() else {
            let _ = child.kill();
            return 1;
        };
        let mut reader = BufReader::new(out);
        let mut line = String::new();
        let live = loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    eprintln!("the child ended before recording started");
                    let _ = child.wait();
                    return 1;
                }
                Ok(_) => {
                    if let Some(rest) = line.strip_prefix("LIVE ") {
                        break rest.trim().to_owned();
                    }
                }
            }
        };
        let live_instant = Instant::now();
        let live_at: i64 = live
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let bundle_dir = live
            .split_once(' ')
            .map(|(_, d)| PathBuf::from(d.trim()))
            .unwrap_or_default();
        eprintln!(
            "child recording (live_at {:.0} ms); killing it with SIGKILL in 7 s",
            ms_of(live_at)
        );
        std::thread::sleep(Duration::from_secs(7));
        let kill_elapsed = live_instant.elapsed().as_secs_f64();
        let killed = child.kill().is_ok();
        let _ = child.wait();
        eprintln!("SIGKILL sent after {kill_elapsed:.2} s: {killed}");
        let session_at_kill = ticks_secs(live_at) + kill_elapsed;

        let mut report = Report::default();
        report.section("kill -9 and recovery (REC-8.AC1, NFR-6)");
        report.check(
            killed,
            "child killed",
            killed.to_string(),
            "SIGKILL delivered",
        );
        report.check(
            bundle_dir.join("recording.lock").exists()
                && !bundle_dir.join("manifest.json").exists(),
            "crashed bundle is incomplete (lock, no manifest)",
            "lock present, manifest absent",
            "the state recovery expects",
        );
        let mut lib = match Library::open_or_create(&root) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("library: {e}");
                return 1;
            }
        };
        let summary = match lib.recover_incomplete() {
            Ok(s) => s,
            Err(e) => {
                report.check(false, "Library::recover_incomplete", e.to_string(), "ok");
                return print_report(args, &report);
            }
        };
        report.check(
            summary.recovered.len() == 1 && summary.failed.is_empty(),
            "recover_incomplete recovered the bundle",
            format!(
                "recovered {}, failed {:?}, in progress {:?}",
                summary.recovered.len(),
                summary
                    .failed
                    .iter()
                    .map(|(n, e)| format!("{n}: {e}"))
                    .collect::<Vec<_>>(),
                summary.in_progress
            ),
            "1 recovered, 0 failed",
        );
        if let Some(rep) = summary.recovered.first() {
            for (n, t) in &rep.tracks {
                report.info(&format!("recovery: {n}"), format!("{t:?}"));
            }
            let screen = rep.manifest.tracks.get("screen");
            let end = screen.and_then(|t| t["end"].as_i64()).map(ticks_secs);
            if let Some(end) = end {
                let lost = session_at_kill - end;
                report.check(
                    lost <= KILL9_LOSS_MAX_S,
                    "media lost (screen)",
                    format!("{lost:.2} s (recovered up to session time {end:.2} s, killed at ~{session_at_kill:.2} s)"),
                    &format!("<= 2 s (+0.25 s measurement skew) => <= {KILL9_LOSS_MAX_S} s"),
                );
            } else {
                report.check(
                    false,
                    "recovered screen track",
                    "missing from the manifest",
                    "present",
                );
            }
            if let Some(c) = rep.manifest.tracks.get("cursor") {
                match CursorLog::read(&bundle_dir.join("cursor.jsonl")) {
                    Ok(log) => {
                        let last = log.events.last().map_or(0.0, |e| ticks_secs(e.t()));
                        report.check(
                            !log.events.is_empty(),
                            "recovered cursor.jsonl parses (CursorLog)",
                            format!(
                                "{} events, last at {last:.2} s, truncated {}",
                                log.events.len(),
                                log.truncated
                            ),
                            "readable; <= ~1 s of events lost",
                        );
                    }
                    Err(e) => report.check(
                        false,
                        "recovered cursor.jsonl parses",
                        e.to_string(),
                        "readable",
                    ),
                }
                let _ = c;
            }
        }
        // Decode the recovered screen file.
        let screen = bundle_dir.join("screen.mp4");
        if let Some(p) = ffprobe_video(&screen) {
            report.check(
                p.packets.unwrap_or(0) > 0,
                "recovered screen.mp4 decodes (ffprobe)",
                format!(
                    "{} {}x{} {:.0} fps, {} packets, duration {}",
                    p.codec,
                    p.width,
                    p.height,
                    p.fps,
                    p.packets.unwrap_or(0),
                    p.duration.map_or("?".into(), |d| format!("{d:.2} s"))
                ),
                "ffprobe reads it, packets > 0",
            );
        } else {
            report.add(
                St::Fail,
                "recovered screen.mp4 decodes (ffprobe)",
                "ffprobe failed or is missing",
                "",
            );
        }
        match ffmpeg_decode_errors(&screen) {
            Some(e) if e.is_empty() => report.check(
                true,
                "recovered screen.mp4 full decode",
                "no errors",
                "ffmpeg -f null",
            ),
            Some(e) => report.check(
                false,
                "recovered screen.mp4 full decode",
                e.lines().next().unwrap_or("").to_owned(),
                "no errors",
            ),
            None => report.add(
                St::Skip,
                "recovered screen.mp4 full decode",
                "ffmpeg missing",
                "",
            ),
        }
        let manifest_ok = std::fs::read_to_string(bundle_dir.join("manifest.json"))
            .ok()
            .and_then(|t| RecManifest::from_json(&t).ok());
        report.check(
            manifest_ok.as_ref().is_some_and(|m| {
                m.extra.contains_key("recovered") && m.extra.contains_key("live_at")
            }),
            "recovered manifest keeps the session extras",
            format!(
                "{:?}",
                manifest_ok
                    .as_ref()
                    .map(|m| m.extra.keys().cloned().collect::<Vec<_>>())
            ),
            "recovered + live_at present",
        );
        report.check(
            !bundle_dir.join("recording.lock").exists(),
            "lock removed by recovery",
            "removed",
            "removed",
        );
        report.check(
            lib.count().map(|c| c == 1).unwrap_or(false),
            "recovered bundle is indexed",
            format!("{:?}", lib.count().ok()),
            "1 row",
        );
        print_report(args, &report)
    }
}
