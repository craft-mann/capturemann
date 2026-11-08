//! Import: fixtures in several containers, rotated and variable-frame-rate sources, audio, error paths.

use super::common::{
    import_file, library_entry, manifest, need_ffmpeg, write_clips, wrong_platform_words,
};
use crate::check;
use crate::ctx::Ctx;
use crate::fixtures;
use crate::media;
use crate::model::{fail, Case, Tier, ALL, R};
use crate::proc::{exit_text, is_crash_code};
use crate::sys;
use serde_json::Value;
use std::path::Path;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "import.formats",
            title: "mp4, mov and mkv import as recordings with the right size and duration",
            area: "import",
            specs: &["IMP-1", "IMP-2"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 360,
            run: formats,
        },
        Case {
            id: "import.rotated",
            title: "a source with a rotation tag imports upright",
            area: "import",
            specs: &["IMP-2", "bug: Import editing copy not upright"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: rotated,
        },
        Case {
            id: "import.vfr",
            title: "a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux)",
            area: "import",
            specs: &["IMP-2", "bug: yuv420p refused by Quick Sync (NV12 needed)", "bug: Import copy not constant 30 fps"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: vfr,
        },
        Case {
            id: "import.audio",
            title: "a WAV file imports as an audio capture of the right length",
            area: "import",
            specs: &["IMP-1", "VOC-2"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 180,
            run: audio,
        },
        Case {
            id: "import.errors",
            title: "not a media file, a missing file, a file held open and a Unicode name end cleanly",
            area: "import",
            specs: &["IMP-3", "IMP-WIN error paths"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 360,
            run: errors,
        },
        Case {
            id: "import.dry_run",
            title: "import --dry-run reports and creates nothing",
            area: "import",
            specs: &["IMP-4"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: dry_run,
        },
    ]
}

fn seconds_of(entry: &Value) -> f64 {
    entry.get("duration").and_then(Value::as_f64).unwrap_or(0.0) / 120_000.0
}

fn dims_of(entry: &Value) -> (i64, i64) {
    (
        entry.get("width").and_then(Value::as_i64).unwrap_or(0),
        entry.get("height").and_then(Value::as_i64).unwrap_or(0),
    )
}

/// The newest recording bundle of the sandbox and its screen file.
fn screen_file(ctx: &Ctx, capture_id: &str) -> crate::model::R<std::path::PathBuf> {
    for b in ctx.bundles("cmrec") {
        let m = manifest(&b)?;
        if m["id"] == capture_id {
            let f = m
                .pointer("/tracks/screen/file")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    crate::model::Stop::Fail(format!(
                        "the manifest of {capture_id} has no screen track"
                    ))
                })?;
            return Ok(b.join(f));
        }
    }
    fail(format!("no bundle of {capture_id} in the library folder"))
}

fn formats(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    for name in ["clip-h264.mp4", "clip-h264.mov", "clip-h264.mkv"] {
        let (id, job) = import_file(ctx, &clips.join(name))?;
        let e = library_entry(ctx, &id)?;
        let (w, h) = dims_of(&e);
        let secs = seconds_of(&e);
        ctx.note(format!(
            "{name}: {id}, {w}x{h}, {secs:.2} s, job {} ms",
            job.get("elapsed_ms").and_then(Value::as_u64).unwrap_or(0)
        ));
        check!(
            e.get("kind").and_then(Value::as_str) == Some("recording"),
            "{name}: kind {:?}",
            e.get("kind")
        );
        check!(
            (w, h) == (320, 240),
            "{name}: imported as {w}x{h}, expected 320x240"
        );
        check!(
            (secs - 3.0).abs() < 0.35,
            "{name}: duration {secs:.2} s, expected about 3.0"
        );
        check!(
            e.get("broken").and_then(Value::as_bool) != Some(true),
            "{name}: the capture is marked broken"
        );
    }
    Ok(())
}

fn rotated(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let (id, _) = import_file(ctx, &clips.join("clip-rotated.mp4"))?;
    let e = library_entry(ctx, &id)?;
    let (w, h) = dims_of(&e);
    ctx.note(format!(
        "library size {w}x{h} (stored 320x240 with a 90 degree tag)"
    ));
    check!(
        (w, h) == (240, 320),
        "the rotated clip is listed {w}x{h}, expected upright 240x320"
    );
    let file = screen_file(ctx, &id)?;
    let p = media::probe(ctx, &file)?;
    let v = p.need_video()?;
    let upright = if v.rotation.rem_euclid(180) == 90 {
        (v.height, v.width)
    } else {
        (v.width, v.height)
    };
    ctx.note(format!(
        "stored track {}: {}x{} rotation {} fps {:?}",
        file.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        v.width,
        v.height,
        v.rotation,
        v.r_frame_rate
    ));
    check!(
        upright == (240, 320),
        "the stored picture is {}x{} with rotation {}: not upright",
        v.width,
        v.height,
        v.rotation
    );
    Ok(())
}

fn vfr(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let (id, job) = import_file(ctx, &clips.join("clip-vfr.mp4"))?;
    let file = screen_file(ctx, &id)?;
    let p = media::probe(ctx, &file)?;
    let v = p.need_video()?;
    let prepared = job
        .pointer("/inputs/files/0/prepared")
        .map(Value::to_string)
        .unwrap_or_default();
    ctx.note(format!(
        "stored track: {} {}x{} r {:?} avg {:?} {:.2} s; prepared {prepared}",
        v.codec_name, v.width, v.height, v.r_frame_rate, v.avg_frame_rate, p.duration
    ));
    check!(
        v.codec_name == "h264",
        "the editing copy is {} not h264",
        v.codec_name
    );
    let (r, a) = (
        v.r_frame_rate.unwrap_or(0.0),
        v.avg_frame_rate.unwrap_or(0.0),
    );
    check!(
        prepared.contains("variable_frame_rate"),
        "the import did not prepare a constant-rate copy (prepared: {prepared})"
    );
    check!(
        (r - a).abs() < 0.5,
        "frame rate is not constant: r {r}, avg {a}"
    );
    check!(
        (24.0..=31.0).contains(&r),
        "the constant rate {r} is outside 24 to 31 fps (the source averages 27.75, nominal 30)"
    );
    check!(
        (p.duration - 4.0).abs() < 0.5,
        "duration {:.2} s, expected about 4",
        p.duration
    );
    Ok(())
}

fn audio(ctx: &mut Ctx) -> R {
    let wav = fixtures::make_wav(440.0, 2.0, 16_000, 0.3);
    check!(
        fixtures::wav_info(&wav) == Ok((16_000, 1, 32_000)),
        "the generated WAV is wrong"
    );
    let path = ctx.write_file("tone.wav", &wav)?;
    ctx.start_daemon()?;
    let (id, _) = import_file(ctx, &path)?;
    let e = library_entry(ctx, &id)?;
    let secs = seconds_of(&e);
    ctx.note(format!("{id}: kind {:?}, {secs:.2} s", e.get("kind")));
    check!(
        matches!(e.get("kind").and_then(Value::as_str), Some("audio")),
        "the WAV imported as kind {:?}, expected audio",
        e.get("kind")
    );
    check!(
        (secs - 2.0).abs() < 0.2,
        "duration {secs:.2} s, expected about 2.0"
    );
    Ok(())
}

fn terminal_state(job: &Value) -> String {
    job.get("state")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned()
}

/// Imports a file that cannot import cleanly and checks the way it ends.
fn bad_import(ctx: &mut Ctx, label: &str, path: &Path) -> R<String> {
    let p = path.to_string_lossy().into_owned();
    let r = ctx.cli_long(&["import", &p, "--wait", "--timeout", "120"], 150)?;
    let text = crate::util::one_line(&r.out.text());
    ctx.note(format!(
        "{label}: {} state {:?} {}",
        exit_text(r.code()),
        r.json.as_ref().map(terminal_state),
        crate::util::cut(&text, 160)
    ));
    check!(!r.out.timed_out, "{label}: the import hung");
    check!(
        !is_crash_code(r.code()),
        "{label}: the CLI crashed ({})",
        exit_text(r.code())
    );
    check!(
        !text.to_lowercase().contains("panicked"),
        "{label}: a panic message reached the user: {}",
        crate::util::cut(&text, 200)
    );
    let wrong = wrong_platform_words(&text, sys::PLATFORM);
    check!(
        wrong.is_empty(),
        "{label}: the message uses another platform's words {wrong:?}"
    );
    Ok(text)
}

fn errors(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    // 1. Not a media file.
    let txt = ctx.write_file("not-media.mp4", b"this is plain text, not a video\n")?;
    let text = bad_import(ctx, "not a media file", &txt)?;
    let v = ctx.cli_ok(&["library", "list"])?;
    check!(
        v["captures"].as_array().is_some_and(Vec::is_empty),
        "a text file became a capture"
    );
    check!(
        !text.trim().is_empty(),
        "the failure of a text file said nothing"
    );
    // 1b. A picture is not media either.
    let png = ctx.write_file(
        "picture.png",
        &fixtures::make_png(64, 48, |x, y| [x as u8 * 4, y as u8 * 5, 90]),
    )?;
    bad_import(ctx, "a PNG picture", &png)?;
    let v = ctx.cli_ok(&["library", "list"])?;
    check!(
        v["captures"].as_array().is_some_and(Vec::is_empty),
        "a picture became a capture"
    );
    // 2. A file that does not exist is refused up front.
    let r = ctx.cli(&[
        "import",
        &ctx.root.join("no-such-file.mp4").to_string_lossy(),
    ])?;
    ctx.note(format!(
        "missing file: {} {:?}",
        exit_text(r.code()),
        r.error_code()
    ));
    check!(
        !r.ok() && r.code() == Some(2),
        "a missing file gave {} (expected exit 2)",
        exit_text(r.code())
    );
    // 3. A file held open without sharing (Windows): ends cleanly, one way or the other.
    let held = ctx.write_file("held.mp4", fixtures::CLIP_MP4)?;
    {
        match sys::hold_file_open(&held) {
            Ok(_guard) => {
                bad_import(ctx, "file open elsewhere", &held)?;
            }
            Err(e) => ctx.note(format!("could not hold the file open here: {e}")),
        }
    }
    // 4. A long Unicode name imports and keeps its title.
    let name = "Chào bạn – bản ghi màn hình đầy đủ ✓.mp4";
    let uni = clips.join(name);
    std::fs::copy(clips.join("clip-h264.mp4"), &uni)?;
    let (id, _) = import_file(ctx, &uni)?;
    let e = library_entry(ctx, &id)?;
    let title = e
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    ctx.note(format!("Unicode name imported as title '{title}'"));
    check!(
        title.contains("Chào bạn"),
        "the Unicode title was changed to '{title}'"
    );
    Ok(())
}

fn dry_run(ctx: &mut Ctx) -> R {
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let p = clips.join("clip-h264.mp4").to_string_lossy().into_owned();
    let r = ctx.cli(&["import", &p, "--dry-run"])?;
    ctx.note(format!(
        "dry run: {} {}",
        exit_text(r.code()),
        crate::util::cut(&crate::util::one_line(&r.out.stdout), 200)
    ));
    check!(
        r.ok(),
        "import --dry-run failed: {}",
        crate::util::cut(&r.out.text(), 200)
    );
    let v = ctx.cli_ok(&["library", "list"])?;
    check!(
        v["captures"].as_array().is_some_and(Vec::is_empty),
        "a dry run created a capture"
    );
    let jobs = ctx.cli_ok(&["jobs", "list", "--type", "import"])?;
    let n = jobs["jobs"].as_array().map_or(0, Vec::len);
    check!(n == 0, "a dry run queued {n} import job(s)");
    Ok(())
}
