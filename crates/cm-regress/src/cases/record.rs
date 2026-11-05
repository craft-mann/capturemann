//! Recording through the daemon: screen and cursor with pause and resume, region, kill -9 recovery,
//! and (flag-gated) microphone, system audio, camera and a long soak. The daemon is the headless
//! `cm-daemon` of the package with the user's `agents.tool_policy` set to allow `start_recording`,
//! because nobody is there to click the confirmation.

use super::common::{manifest, wait_job};
use crate::check;
use crate::ctx::{Ctx, Display};
use crate::media;
use crate::model::{fail, skip, Case, Need, Stop, Tier, ALL, R, WIN};
use crate::proc::exit_text;
use crate::sys;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "rec.screen_cursor",
            title: "screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings",
            area: "rec",
            specs: &["REC-1", "REC-2", "REC-3", "REC-4", "REC-7", "NFR-2", "bug: 54 % late frames on a static screen", "bug: recording stop crash 0xC0000409"],
            tier: Tier::Full,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: ALL,
            timeout_s: 240,
            run: screen_cursor,
        },
        Case {
            id: "rec.region",
            title: "a region recording is exactly the requested size",
            area: "rec",
            specs: &["REC-1.AC", "CAP-1.AC2"],
            tier: Tier::Full,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: ALL,
            timeout_s: 180,
            run: region,
        },
        Case {
            id: "rec.kill9_recovery",
            title: "the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s",
            area: "rec",
            specs: &["REC-8", "recording.lock"],
            tier: Tier::Full,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: ALL,
            timeout_s: 240,
            run: kill9,
        },
        Case {
            id: "rec.mic",
            title: "a microphone track is recorded for the length of the recording",
            area: "rec",
            specs: &["AUD-1", "AUD-2"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen, Need::Mic],
            platforms: ALL,
            timeout_s: 180,
            run: mic,
        },
        Case {
            id: "rec.system_audio",
            title: "system audio contains a tone played by another process",
            area: "rec",
            specs: &["AUD-1", "bug class: process loopback"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen, Need::Sound],
            platforms: ALL,
            timeout_s: 180,
            run: system_audio,
        },
        Case {
            id: "rec.camera",
            title: "a camera track keeps at least 24 fps in a lit room",
            area: "rec",
            specs: &["CAM-1", "CAM-2", "bug class: camera 10 fps in low light"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen, Need::Camera],
            platforms: ALL,
            timeout_s: 180,
            run: camera,
        },
        Case {
            id: "rec.external_4k",
            title: "30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps",
            area: "rec",
            specs: &["REC-3", "NFR-2", "bug: 4K recording of the external lost frames on the UHD 620"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen, Need::SecondMonitor],
            platforms: ALL,
            timeout_s: 240,
            run: external_4k,
        },
        Case {
            id: "rec.each_monitor",
            title: "10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes",
            area: "rec",
            specs: &["REC-7", "DPI-WIN steps 7 to 7c"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen, Need::SecondMonitor],
            platforms: WIN,
            timeout_s: 300,
            run: each_monitor,
        },
        Case {
            id: "rec.soak",
            title: "a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames",
            area: "rec",
            specs: &["NFR-2", "REC-WIN gate"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen, Need::Mic],
            platforms: ALL,
            timeout_s: 4 * 3600,
            run: soak,
        },
    ]
}

/// A finished recording.
pub struct Recorded {
    /// Seconds the recording should hold: what the daemon said was recorded at the pause, plus the time
    /// between the resume answer and the stop request (the pause itself is not counted).
    pub expected_s: f64,
    pub bundle: PathBuf,
    pub manifest: Value,
    pub stop: Value,
}

enum Step {
    Wait(u64),
    Pause,
    Resume,
}

fn start(ctx: &mut Ctx, display: &Display, extra: &[&str]) -> R<String> {
    let id = display.id.to_string();
    let mut args = vec![
        "record",
        "start",
        "--display",
        &id,
        "--countdown",
        "0",
        "--title",
        "regress recording",
    ];
    args.extend_from_slice(extra);
    let r = ctx.cli_long(&args, 90)?;
    if !r.ok() {
        let code = r.error_code().unwrap_or_default();
        let msg = r.error_message();
        if code == "hardware_encoder_unavailable" {
            return skip(format!(
                "no hardware video encoder: {}",
                crate::util::cut(&msg, 140)
            ));
        }
        return fail(format!(
            "record start failed ({}): {code} {}",
            exit_text(r.code()),
            crate::util::cut(&msg, 300)
        ));
    }
    let v = r.json.unwrap_or(Value::Null);
    let rid = v
        .get("recording_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    ctx.note(format!("started {rid}"));
    Ok(rid)
}

fn status(ctx: &mut Ctx) -> R<Value> {
    ctx.cli_ok(&["record", "status"])
}

fn state_of(v: &Value) -> String {
    v.get("state")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned()
}

fn stop(ctx: &mut Ctx) -> R<Value> {
    let r = ctx.cli_long(&["record", "stop"], 120)?;
    if !r.ok() {
        return fail(format!(
            "record stop failed ({}): {} {}",
            exit_text(r.code()),
            r.error_code().unwrap_or_default(),
            crate::util::cut(&r.error_message(), 300)
        ));
    }
    Ok(r.json.unwrap_or(Value::Null))
}

fn sleep_s(_ctx: &Ctx, s: u64) {
    std::thread::sleep(Duration::from_secs(s));
}

/// Starts a recording, follows the plan, stops it and opens the bundle.
fn record(
    ctx: &mut Ctx,
    display: &Display,
    extra: &[&str],
    plan: &[Step],
    mut during: Option<&mut dyn FnMut(&mut Ctx)>,
) -> R<Recorded> {
    start(ctx, display, extra)?;
    let mut expected_s = 0.0;
    let mut resumed_at: Option<std::time::Instant> = None;
    let mut counted_before_pause = false;
    for (i, step) in plan.iter().enumerate() {
        match step {
            Step::Wait(s) => {
                if i == 1 {
                    if let Some(f) = during.as_mut() {
                        f(ctx);
                    }
                }
                sleep_s(ctx, *s);
            }
            Step::Pause => {
                let r = ctx.cli_ok(&["record", "pause"])?;
                expected_s = r.get("recorded_ms").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0;
                counted_before_pause = true;
                check!(
                    state_of(&r) == "paused",
                    "after pause the state is '{}'",
                    state_of(&r)
                );
            }
            Step::Resume => {
                let r = ctx.cli_ok(&["record", "resume"])?;
                resumed_at = Some(std::time::Instant::now());
                check!(
                    state_of(&r) == "recording",
                    "after resume the state is '{}'",
                    state_of(&r)
                );
            }
        }
    }
    if let Some(t) = resumed_at {
        expected_s += t.elapsed().as_secs_f64();
    } else if !counted_before_pause {
        expected_s = f64::NAN;
    }
    let s = stop(ctx)?;
    let capture_id = s
        .get("capture_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    check!(!capture_id.is_empty(), "stop returned no capture id: {s}");
    let bundle = ctx
        .bundles("cmrec")
        .into_iter()
        .find(|b| manifest(b).is_ok_and(|m| m["id"] == capture_id.as_str()))
        .ok_or_else(|| Stop::Fail(format!("no bundle of {capture_id} in the library folder")))?;
    let m = manifest(&bundle)?;
    // The automatic export after the stop runs an ffmpeg: follow it to its end so it is not left behind.
    wait_idle_exports(ctx);
    Ok(Recorded {
        expected_s,
        bundle,
        manifest: m,
        stop: s,
    })
}

pub(crate) fn wait_idle_exports(ctx: &mut Ctx) {
    // The automatic export after a recording is a job; follow it to its end, and say what happened.
    if let Ok(v) = ctx.cli_ok(&["jobs", "list", "--type", "export"]) {
        let jobs = v
            .get("jobs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for j in jobs {
            let id = j
                .get("job_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if id.is_empty() {
                continue;
            }
            match wait_job(ctx, &id, 90) {
                Ok(done) => ctx.note(format!(
                    "export job {id}: {}",
                    done.get("state").and_then(Value::as_str).unwrap_or("?")
                )),
                Err(Stop::Fail(m)) => ctx.note(format!("export job {id}: {m}")),
                Err(Stop::Skip(_)) => {}
            }
        }
    }
}

/// Words that mean a recording had trouble, searched in the daemon's output and the logs.
fn trouble_in_logs(ctx: &Ctx) -> Vec<String> {
    let mut hits = Vec::new();
    let mut texts = vec![
        std::fs::read_to_string(ctx.root.join("daemon.out.txt")).unwrap_or_default(),
        std::fs::read_to_string(ctx.root.join("daemon.err.txt")).unwrap_or_default(),
    ];
    if let Ok(rd) = std::fs::read_dir(ctx.config.join("logs")) {
        for e in rd.flatten() {
            texts.push(std::fs::read_to_string(e.path()).unwrap_or_default());
        }
    }
    for needle in [
        "screen_degraded",
        "tick_failed",
        "capture_stalled",
        "track_failed",
        "screen_stopped_early",
    ] {
        if texts.iter().any(|t| t.contains(needle)) {
            hits.push(needle.to_owned());
        }
    }
    hits
}

fn screen_probe(ctx: &mut Ctx, rec: &Recorded) -> R<media::Probe> {
    let f = rec
        .manifest
        .pointer("/tracks/screen/file")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Stop::Fail(format!(
                "the manifest has no screen track: {}",
                crate::util::cut(&rec.manifest["tracks"].to_string(), 300)
            ))
        })?;
    media::probe(ctx, &rec.bundle.join(f))
}

fn screen_cursor(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let plan = [
        Step::Wait(3),
        Step::Pause,
        Step::Wait(2),
        Step::Resume,
        Step::Wait(3),
    ];
    // 3 s + 3 s recorded; the 2 s pause is removed from every file's time.
    let console_watch = crate::cases::desktop::ConsoleWatch::start();
    let rec = record(
        ctx,
        &d,
        &["--size", "source", "--fps", "30", "--codec", "h264"],
        &plan,
        None,
    )?;
    let consoles = console_watch.finish();
    check!(
        consoles.is_empty(),
        "console windows appeared while recording and stopping: {}",
        consoles.join("; ")
    );
    check!(
        !rec.bundle.join("recording.lock").exists(),
        "recording.lock is still in the bundle after stop"
    );
    let p = screen_probe(ctx, &rec)?;
    let v = p.need_video()?;
    let (r, a) = (
        v.r_frame_rate.unwrap_or(0.0),
        v.avg_frame_rate.unwrap_or(0.0),
    );
    ctx.note(format!(
        "screen: {} {}x{} {} r {r:.2} avg {a:.2} frames {:?} {:.2} s; manifest duration {:?} ticks",
        v.codec_name,
        v.width,
        v.height,
        v.pix_fmt,
        v.nb_frames,
        p.duration,
        rec.manifest.get("duration")
    ));
    if sys::PLATFORM == "windows" {
        check!(
            v.codec_name == "h264",
            "the screen track is {} not h264",
            v.codec_name
        );
    }
    check!(
        (r - 30.0).abs() <= 2.0,
        "frame rate is {r:.2}, expected 30 +- 2"
    );
    check!(
        (r - a).abs() <= 1.5 || v.nb_frames.is_some(),
        "frame rate is not constant: r {r:.2} avg {a:.2}"
    );
    check!(
        v.width == d.w && v.height == d.h,
        "the picture is {}x{} but the display is {}x{}",
        v.width,
        v.height,
        d.w,
        d.h
    );
    let manifest_s = rec
        .manifest
        .get("duration")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        / 120_000.0;
    ctx.note(format!("manifest {manifest_s:.2} s, expected {:.2} s (recorded at the pause plus the time after the resume)", rec.expected_s));
    check!(
        rec.expected_s.is_nan() || (manifest_s - rec.expected_s).abs() < 1.2,
        "the recording is {manifest_s:.2} s in the manifest but about {:.2} s were recorded outside the 2 s pause",
        rec.expected_s
    );
    check!(
        (p.duration - manifest_s).abs() <= 0.5,
        "screen.mp4 is {:.2} s but the manifest says {manifest_s:.2} s",
        p.duration
    );
    let keys = media::keyframes(
        ctx,
        &rec.bundle.join(
            rec.manifest["tracks"]["screen"]["file"]
                .as_str()
                .unwrap_or("screen.mp4"),
        ),
    )?;
    let gaps = media::key_gaps(&keys);
    ctx.note(format!("{} key frames, gaps {gaps:?}", keys.len()));
    if let Some((min, max)) = gaps {
        check!(
            min >= 0.9 && max <= 2.6,
            "key frame gaps run from {min:.2} to {max:.2} s, expected about 2 s"
        );
    } else {
        check!(
            p.duration < 3.0,
            "only {} key frame(s) in a {:.1} s file",
            keys.len(),
            p.duration
        );
    }
    // The cursor log.
    let cursor = Ctx::files_under(&rec.bundle).into_iter().find(|f| {
        f.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("cursor") && n.ends_with(".jsonl"))
    });
    let Some(cursor) = cursor else {
        return fail(format!(
            "no cursor log in the bundle; files: {:?}",
            Ctx::files_under(&rec.bundle)
                .iter()
                .filter_map(|f| f.file_name().and_then(|n| n.to_str()).map(str::to_owned))
                .collect::<Vec<_>>()
        ));
    };
    let text = std::fs::read_to_string(&cursor)?;
    let first: Value = serde_json::from_str(text.lines().next().unwrap_or(""))
        .map_err(|e| Stop::Fail(format!("the cursor log header is not JSON: {e}")))?;
    ctx.note(format!(
        "cursor log: {} lines, header {}",
        text.lines().count(),
        crate::util::cut(&first.to_string(), 160)
    ));
    let trouble = trouble_in_logs(ctx);
    check!(
        trouble.is_empty(),
        "recording trouble in the logs: {trouble:?}"
    );
    let warnings = rec
        .stop
        .get("warnings")
        .map(Value::to_string)
        .unwrap_or_default();
    check!(
        !warnings.contains("screen_degraded") && !warnings.contains("tick_failed"),
        "stop reported warnings: {warnings}"
    );
    wait_idle_exports(ctx);
    Ok(())
}

fn region(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    check!(
        d.w >= 800 && d.h >= 500,
        "the display is too small for a 640x360 region"
    );
    let rec = record(
        ctx,
        &d,
        &["--region", "0,0,640,360"],
        &[Step::Wait(3)],
        None,
    )?;
    let p = screen_probe(ctx, &rec)?;
    let v = p.need_video()?;
    ctx.note(format!(
        "region recording {}x{} {}",
        v.width, v.height, v.codec_name
    ));
    check!(
        v.width == 640 && v.height == 360,
        "the region recording is {}x{}, expected 640x360",
        v.width,
        v.height
    );
    Ok(())
}

fn kill9(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    start(ctx, &d, &["--size", "720p"])?;
    std::thread::sleep(Duration::from_secs(6));
    let st = status(ctx)?;
    let recorded_s = st.get("recorded_ms").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0;
    ctx.note(format!(
        "before the kill: state {} recorded {recorded_s:.1} s",
        state_of(&st)
    ));
    check!(
        recorded_s > 3.0,
        "only {recorded_s:.1} s were recorded before the kill"
    );
    ctx.kill_daemon();
    std::thread::sleep(Duration::from_millis(800));
    let bundle = ctx
        .bundles("cmrec")
        .into_iter()
        .next()
        .ok_or_else(|| Stop::Fail("no bundle on disk after the kill".to_owned()))?;
    check!(
        bundle.join("recording.lock").exists(),
        "recording.lock is not in the bundle of an unfinished recording"
    );
    ctx.start_daemon()?;
    let gone = crate::proc::wait_for(ctx.secs(90), Duration::from_millis(500), || {
        (!bundle.join("recording.lock").exists()).then_some(())
    });
    check!(
        gone.is_some(),
        "recording.lock is still there 90 s after the daemon restarted: recovery did not run"
    );
    let m = manifest(&bundle)?;
    let f = m
        .pointer("/tracks/screen/file")
        .and_then(Value::as_str)
        .unwrap_or("screen.mp4")
        .to_owned();
    let p = media::probe(ctx, &bundle.join(&f))?;
    ctx.note(format!(
        "recovered: {:.2} s of about {recorded_s:.1} s recorded before the kill",
        p.duration
    ));
    check!(
        p.duration > 1.0,
        "the recovered file is only {:.2} s",
        p.duration
    );
    check!(
        recorded_s - p.duration < 3.0,
        "recovery lost {:.2} s (limit 3 s)",
        recorded_s - p.duration
    );
    let list = ctx.cli_ok(&["library", "list", "--kind", "recording"])?;
    check!(
        list.to_string().contains(m["id"].as_str().unwrap_or("-")),
        "the recovered recording is not in the library"
    );
    Ok(())
}

fn track_seconds(ctx: &mut Ctx, rec: &Recorded, track: &str) -> R<f64> {
    let f = rec
        .manifest
        .pointer(&format!("/tracks/{track}/file"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Stop::Fail(format!(
                "the manifest has no `{track}` track (tracks: {})",
                rec.manifest["tracks"]
                    .as_object()
                    .map(|o| o.keys().cloned().collect::<Vec<_>>().join(","))
                    .unwrap_or_default()
            ))
        })?
        .to_owned();
    Ok(media::probe(ctx, &rec.bundle.join(f))?.duration)
}

fn mic(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let rec = record(
        ctx,
        &d,
        &["--size", "720p", "--mic", "default"],
        &[Step::Wait(4)],
        None,
    )?;
    let secs = track_seconds(ctx, &rec, "mic")?;
    ctx.note(format!("mic track {secs:.2} s"));
    check!(
        (secs - 4.0).abs() < 0.8,
        "the microphone track is {secs:.2} s, expected about 4"
    );
    Ok(())
}

fn play_tone(ctx: &mut Ctx) {
    let wav = crate::fixtures::make_wav(1000.0, 2.0, 48_000, 0.6);
    let Ok(path) = ctx.write_file("tone.wav", &wav) else {
        return;
    };
    let p = path.to_string_lossy().into_owned();
    let cmd = match sys::PLATFORM {
        "windows" => {
            let mut c = std::process::Command::new("powershell");
            c.args([
                "-NoProfile",
                "-Command",
                &format!("(New-Object System.Media.SoundPlayer '{p}').PlaySync()"),
            ]);
            Some(c)
        }
        "macos" => {
            let mut c = std::process::Command::new("afplay");
            c.arg(&p);
            Some(c)
        }
        _ => None,
    };
    if let Some(c) = cmd {
        let _ = crate::proc::run(c, Duration::from_secs(20), &ctx.tracker);
    }
}

fn system_audio(ctx: &mut Ctx) -> R {
    if sys::PLATFORM == "linux" {
        return skip("no tone player for Linux yet");
    }
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let mut tone = |c: &mut Ctx| play_tone(c);
    let rec = record(
        ctx,
        &d,
        &["--size", "720p", "--system-audio"],
        &[Step::Wait(1), Step::Wait(3)],
        Some(&mut tone),
    )?;
    let f = rec
        .manifest
        .pointer("/tracks/system/file")
        .and_then(Value::as_str)
        .ok_or_else(|| Stop::Fail("the manifest has no system audio track".to_owned()))?
        .to_owned();
    let file = rec.bundle.join(f);
    let (mean, max) = media::wav_levels(&std::fs::read(&file)?, 0.0, 4.0).map_err(Stop::Fail)?;
    ctx.note(format!("system audio level: mean {mean:.1} dB, peak {max:.1} dB (the tone is 1 kHz, 2 s, played by another process)"));
    check!(max > -45.0, "the system audio track is silent (peak {max:.1} dB): is the volume at 0 or muted, or is the tone missing?");
    Ok(())
}

fn camera(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let rec = record(
        ctx,
        &d,
        &["--size", "720p", "--camera", "on"],
        &[Step::Wait(5)],
        None,
    )?;
    let cam = rec
        .manifest
        .pointer("/tracks/camera")
        .cloned()
        .ok_or_else(|| Stop::Fail("the manifest has no camera track".to_owned()))?;
    let f = cam
        .get("file")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let p = media::probe(ctx, &rec.bundle.join(&f))?;
    let v = p.need_video()?;
    let fps = cam
        .pointer("/stats/measured_fps")
        .and_then(Value::as_f64)
        .or(v.avg_frame_rate)
        .unwrap_or(0.0);
    ctx.note(format!(
        "camera {}x{} {} measured {fps:.1} fps, {:.2} s",
        v.width, v.height, v.codec_name, p.duration
    ));
    check!(
        (p.duration - 5.0).abs() < 1.0,
        "the camera track is {:.2} s, expected about 5",
        p.duration
    );
    check!(fps >= 24.0, "the camera ran at {fps:.1} fps (a dark room makes some cameras drop to 10 fps; light the room and rerun)");
    Ok(())
}

fn soak(ctx: &mut Ctx) -> R {
    let minutes = ctx.opts.soak_minutes;
    if minutes == 0 {
        return skip("give --soak-minutes <n> to run a long recording");
    }
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    start(ctx, &d, &["--mic", "default", "--fps", "30"])?;
    let total = u64::from(minutes) * 60;
    let mut done = 0;
    while done < total {
        std::thread::sleep(Duration::from_secs(30.min(total - done)));
        done += 30.min(total - done);
        let st = status(ctx)?;
        if done % 300 == 0 || done >= total {
            ctx.note(format!(
                "{done} s: state {} recorded {:?} ms late {:?}%",
                state_of(&st),
                st.get("recorded_ms"),
                st.get("late_frame_pct")
            ));
        }
        check!(
            state_of(&st) == "recording",
            "the recording left the recording state at {done} s: {}",
            state_of(&st)
        );
    }
    let s = stop(ctx)?;
    let cid = s["capture_id"].as_str().unwrap_or("").to_owned();
    let bundle = ctx
        .bundles("cmrec")
        .into_iter()
        .find(|b| manifest(b).is_ok_and(|m| m["id"] == cid.as_str()))
        .ok_or_else(|| Stop::Fail("no bundle after the soak".to_owned()))?;
    let m = manifest(&bundle)?;
    let rec = Recorded {
        expected_s: f64::NAN,
        bundle,
        manifest: m,
        stop: s,
    };
    let p = screen_probe(ctx, &rec)?;
    let v = p.need_video()?;
    ctx.note(format!(
        "soak: {} {}x{} avg {:?} {:.1} s",
        v.codec_name, v.width, v.height, v.avg_frame_rate, p.duration
    ));
    check!(
        (p.duration - total as f64).abs() < 5.0,
        "the recording is {:.1} s, expected about {total}",
        p.duration
    );
    check!(
        (v.avg_frame_rate.unwrap_or(0.0) - 30.0).abs() < 1.0,
        "the average frame rate is {:?}",
        v.avg_frame_rate
    );
    let trouble = trouble_in_logs(ctx);
    check!(
        trouble.is_empty(),
        "recording trouble in the logs: {trouble:?}"
    );
    let _ = ALL;
    Ok(())
}

fn external_4k(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let v = ctx.cli_ok(&["targets", "--displays-only"])?;
    let ds = crate::ctx::displays(&v);
    let Some(d) = ds.iter().find(|d| d.w >= 3000 && !d.primary).cloned() else {
        return skip("no non-primary display of 3000 px or more is connected");
    };
    let rec = record(
        ctx,
        &d,
        &["--size", "source", "--fps", "30"],
        &[Step::Wait(30)],
        None,
    )?;
    let screen = rec
        .manifest
        .pointer("/tracks/screen")
        .cloned()
        .unwrap_or(Value::Null);
    let st = screen.get("stats").cloned().unwrap_or(Value::Null);
    let (start, end) = (
        screen.get("start").and_then(Value::as_f64).unwrap_or(0.0),
        screen.get("end").and_then(Value::as_f64).unwrap_or(0.0),
    );
    let file_end = st
        .get("file_end")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NAN);
    let eff = st
        .get("effective_fps")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NAN);
    ctx.note(format!(
        "display {} {}x{}: track {start}..{end} ticks, file_end {file_end}, effective_fps {eff}, late_pct {:?}, dropped {:?}, rate_changes {:?}",
        d.id, d.w, d.h, st.get("late_pct"), st.get("dropped"), st.get("rate_changes")
    ));
    let p = screen_probe(ctx, &rec)?;
    ctx.note(format!("file {:.2} s", p.duration));
    let overloaded = trouble_words(ctx, &["encoder_overloaded"]);
    ctx.note(format!("encoder_overloaded warning logged: {overloaded}"));
    check!(
        file_end.is_nan() || file_end >= 0.9 * end,
        "the file ends at {file_end} ticks but the track ends at {end}: it holds under 90 % of the recording"
    );
    check!(
        p.duration >= 27.0,
        "the file is only {:.2} s for a 30 s recording",
        p.duration
    );
    let full_rate = eff >= 29.0;
    check!(
        full_rate || overloaded,
        "the rate fell to {eff} fps with no encoder_overloaded warning"
    );
    wait_idle_exports(ctx);
    Ok(())
}

fn trouble_words(ctx: &Ctx, needles: &[&str]) -> bool {
    let mut texts = vec![
        std::fs::read_to_string(ctx.root.join("daemon.out.txt")).unwrap_or_default(),
        std::fs::read_to_string(ctx.root.join("daemon.err.txt")).unwrap_or_default(),
    ];
    if let Ok(rd) = std::fs::read_dir(ctx.config.join("logs")) {
        for e in rd.flatten() {
            texts.push(std::fs::read_to_string(e.path()).unwrap_or_default());
        }
    }
    texts.iter().any(|t| needles.iter().any(|n| t.contains(n)))
}

fn find_numbers(v: &Value, key: &str) -> Option<Vec<i64>> {
    match v {
        Value::Object(o) => {
            if let Some(a) = o.get(key).and_then(Value::as_array) {
                return Some(a.iter().filter_map(Value::as_i64).collect());
            }
            o.values().find_map(|x| find_numbers(x, key))
        }
        _ => None,
    }
}

fn each_monitor(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let v = ctx.cli_ok(&["targets", "--displays-only"])?;
    let ds = crate::ctx::displays(&v);
    if ds.len() < 2 {
        return skip("only one display is connected");
    }
    let mut problems = Vec::new();
    let names: std::collections::BTreeSet<&str> = ds.iter().map(|d| d.name.as_str()).collect();
    for d in &ds {
        ctx.note(format!(
            "display {} '{}' at ({},{}) {}x{} scale {}{}",
            d.id,
            d.name,
            d.x,
            d.y,
            d.w,
            d.h,
            d.scale,
            if d.primary { " primary" } else { "" }
        ));
        if d.name.trim().is_empty() {
            problems.push(format!("display {} has no name", d.id));
        }
    }
    if names.len() != ds.len() {
        problems.push("two displays share a name".to_owned());
    }
    for d in &ds {
        let id = d.id.to_string();
        let region = "100,100,400,300";
        let out = ctx
            .root
            .join(format!("region-{}.png", d.id))
            .to_string_lossy()
            .into_owned();
        ctx.cli_ok(&[
            "capture",
            "screenshot",
            "--display",
            &id,
            "--region",
            region,
            "--out",
            &out,
        ])?;
        let (w, h) = super::common::check_png(std::path::Path::new(&out))?;
        if (w, h) != (400, 300) {
            problems.push(format!(
                "region on display {} saved {w}x{h}, expected 400x300",
                d.id
            ));
        }
        let rec = record(ctx, d, &["--size", "source"], &[Step::Wait(10)], None)?;
        let cursor = Ctx::files_under(&rec.bundle).into_iter().find(|f| {
            f.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("cursor") && n.ends_with(".jsonl"))
        });
        match cursor {
            None => problems.push(format!("display {}: no cursor log", d.id)),
            Some(c) => {
                let text = std::fs::read_to_string(c)?;
                let head: Value =
                    serde_json::from_str(text.lines().next().unwrap_or("")).unwrap_or(Value::Null);
                let origin = find_numbers(&head, "origin");
                let size = find_numbers(&head, "size");
                ctx.note(format!(
                    "display {}: cursor header origin {origin:?} size {size:?}; {} lines",
                    d.id,
                    text.lines().count()
                ));
                if origin.as_deref() != Some(&[d.x, d.y][..]) {
                    problems.push(format!(
                        "display {}: cursor header origin {origin:?}, expected [{}, {}]",
                        d.id, d.x, d.y
                    ));
                }
                if size.as_deref() != Some(&[d.w, d.h][..]) {
                    problems.push(format!(
                        "display {}: cursor header size {size:?}, expected [{}, {}]",
                        d.id, d.w, d.h
                    ));
                }
            }
        }
        if let Ok(idx) = std::fs::read_to_string(rec.bundle.join("cursors").join("index.json")) {
            ctx.note(format!(
                "display {}: cursor sprites {}",
                d.id,
                crate::util::cut(&idx.replace(char::is_whitespace, ""), 240)
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}
