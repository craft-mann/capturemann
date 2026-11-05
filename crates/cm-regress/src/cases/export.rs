//! Export of an imported clip: presets, colour tags, audio, error paths; checked with the bundled ffprobe.

use super::common::{import_file, need_ffmpeg, write_clips, wrong_platform_words};
use crate::check;
use crate::ctx::Ctx;
use crate::media;
use crate::model::{fail, skip, Case, Tier, ALL, R};
use crate::proc::{exit_text, is_crash_code};
use crate::sys;
use serde_json::Value;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "export.quick",
            title: "the quick export is H.264 MP4 of the right length, tagged BT.709 limited range",
            area: "export",
            specs: &["EXP-1", "bug: export colour tags (NV12 encoders converted with BT.601, stream untagged)"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: quick,
        },
        Case {
            id: "export.presets",
            title: "youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length",
            area: "export",
            specs: &["EXP-2", "EXP-3"],
            tier: Tier::Full,
            needs: &[],
            platforms: ALL,
            timeout_s: 480,
            run: presets,
        },
        Case {
            id: "export.audio",
            title: "audio export to WAV and M4A has the right codec and length",
            area: "export",
            specs: &["VOC-2", "AUD-4"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: audio,
        },
        Case {
            id: "export.errors",
            title: "an unknown preset or capture is refused with the shared error codes",
            area: "export",
            specs: &["EXP-1", "errors: invalid_input, not_found"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: errors,
        },
    ]
}

struct Exported {
    path: std::path::PathBuf,
    encoder: String,
}

fn export(ctx: &mut Ctx, id: &str, extra: &[&str]) -> R<Exported> {
    let mut args = vec!["export", id];
    args.extend_from_slice(extra);
    args.extend(["--wait", "--timeout", "240"]);
    let r = ctx.cli_long(&args, 260)?;
    let job = r.json.clone().ok_or_else(|| {
        crate::model::Stop::Fail(format!(
            "export {} printed no job ({}): {}",
            extra.join(" "),
            exit_text(r.code()),
            crate::util::cut(&crate::util::one_line(&r.out.text()), 300)
        ))
    })?;
    let state = job.get("state").and_then(Value::as_str).unwrap_or("?");
    if state != "done" {
        let msg = job
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("");
        let code = job
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("");
        let text = format!("{code} {msg}");
        if code == "hardware_encoder_unavailable" {
            let wrong = wrong_platform_words(&text, sys::PLATFORM);
            check!(
                wrong.is_empty(),
                "the hardware-encoder message uses another platform's words {wrong:?}: {text}"
            );
            return skip(format!(
                "this machine has no hardware video encoder: {}",
                crate::util::cut(msg, 120)
            ));
        }
        return fail(format!(
            "export {} ended `{state}`: {text}",
            extra.join(" ")
        ));
    }
    let path = job
        .pointer("/result/path")
        .and_then(Value::as_str)
        .ok_or_else(|| crate::model::Stop::Fail("the export result has no path".to_owned()))?;
    let encoder = job
        .pointer("/result/encoder")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    ctx.note(format!(
        "export {}: {} by {encoder}, {} ms",
        extra.join(" "),
        std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?"),
        job.pointer("/result/elapsed_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    Ok(Exported {
        path: path.into(),
        encoder,
    })
}

fn import_clip(ctx: &mut Ctx) -> R<String> {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    Ok(import_file(ctx, &clips.join("clip-h264.mp4"))?.0)
}

fn quick(ctx: &mut Ctx) -> R {
    let id = import_clip(ctx)?;
    let e = export(ctx, &id, &["--preset", "quick"])?;
    let p = media::probe(ctx, &e.path)?;
    let v = p.need_video()?;
    ctx.note(format!(
        "{} {}x{} {} range {} space {} trc {} prim {}, {:.2} s",
        v.codec_name,
        v.width,
        v.height,
        v.pix_fmt,
        v.color_range,
        v.color_space,
        v.color_transfer,
        v.color_primaries,
        p.duration
    ));
    check!(
        v.codec_name == "h264",
        "the quick export is {} not h264",
        v.codec_name
    );
    check!(
        p.format_name.contains("mp4"),
        "container is {}",
        p.format_name
    );
    check!(
        (p.duration - 3.0).abs() < 0.4,
        "duration {:.2} s, expected about 3.0",
        p.duration
    );
    check!(v.width > 0 && v.height > 0, "no picture size");
    if e.encoder.contains("_mf") || e.encoder.contains("qsv") {
        check!(
            v.color_space == "bt709" && v.color_range == "tv",
            "the {} export is tagged space '{}' range '{}', expected bt709 and tv",
            e.encoder,
            v.color_space,
            v.color_range
        );
    } else {
        ctx.note(format!(
            "encoder {}: colour tags not asserted (only the Media Foundation path had the bug)",
            e.encoder
        ));
    }
    Ok(())
}

fn presets(ctx: &mut Ctx) -> R {
    let id = import_clip(ctx)?;
    let mut problems = Vec::new();
    let table: [(&str, Vec<&str>); 5] = [
        ("youtube-1080p", vec!["--preset", "youtube-1080p"]),
        ("vertical-1080", vec!["--preset", "vertical-1080"]),
        ("square-1080", vec!["--preset", "square-1080"]),
        ("webm", vec!["--preset", "webm"]),
        ("gif", vec!["--preset", "gif"]),
    ];
    for (name, args) in table {
        let e = match export(ctx, &id, &args) {
            Ok(e) => e,
            Err(crate::model::Stop::Skip(why)) => {
                ctx.note(format!("{name}: skipped, {why}"));
                continue;
            }
            Err(crate::model::Stop::Fail(m)) => {
                problems.push(format!("{name}: {m}"));
                continue;
            }
        };
        let p = media::probe(ctx, &e.path)?;
        let Some(v) = p.video() else {
            problems.push(format!("{name}: no video stream"));
            continue;
        };
        ctx.note(format!(
            "{name}: {} {}x{} {:.2} s {} fps {:?} ({} bytes)",
            v.codec_name, v.width, v.height, p.duration, p.format_name, v.avg_frame_rate, p.size
        ));
        if (p.duration - 3.0).abs() > 0.6 && name != "gif" {
            problems.push(format!(
                "{name}: duration {:.2} s, expected about 3",
                p.duration
            ));
        }
        match name {
            "youtube-1080p" => {
                if v.codec_name != "h264" || v.width > 1920 || v.height > 1080 {
                    problems.push(format!("{name}: {} {}x{}", v.codec_name, v.width, v.height));
                }
                if (e.encoder.contains("_mf") || e.encoder.contains("qsv"))
                    && (v.color_space != "bt709" || v.color_range != "tv")
                {
                    problems.push(format!(
                        "{name}: tagged space '{}' range '{}' by {}",
                        v.color_space, v.color_range, e.encoder
                    ));
                }
            }
            "vertical-1080" if v.height <= v.width => {
                problems.push(format!("{name}: {}x{} is not portrait", v.width, v.height))
            }
            "square-1080" if v.height != v.width => {
                problems.push(format!("{name}: {}x{} is not square", v.width, v.height))
            }
            "webm"
                if !(p.format_name.contains("webm") || p.format_name.contains("matroska"))
                    || !matches!(v.codec_name.as_str(), "vp9" | "vp8" | "av1") =>
            {
                problems.push(format!("{name}: {} in {}", v.codec_name, p.format_name))
            }
            "gif" if p.format_name != "gif" || v.codec_name != "gif" => {
                problems.push(format!("{name}: {} in {}", v.codec_name, p.format_name))
            }
            _ => {}
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}

fn audio(ctx: &mut Ctx) -> R {
    let id = import_clip(ctx)?;
    for (fmt, codec) in [("wav", "pcm_s16le"), ("m4a", "aac")] {
        let e = export(ctx, &id, &["--audio", "--format", fmt])?;
        let p = media::probe(ctx, &e.path)?;
        let a = p
            .audio()
            .ok_or_else(|| crate::model::Stop::Fail(format!("{fmt}: no audio stream")))?;
        ctx.note(format!(
            "{fmt}: {} {} Hz {} ch {:.2} s",
            a.codec_name,
            a.sample_rate.unwrap_or(0),
            a.channels.unwrap_or(0),
            p.duration
        ));
        check!(
            a.codec_name.starts_with(&codec[..3]) || a.codec_name == codec,
            "{fmt}: codec {} expected {codec}",
            a.codec_name
        );
        check!(
            (p.duration - 3.0).abs() < 0.4,
            "{fmt}: duration {:.2} s, expected about 3",
            p.duration
        );
    }
    Ok(())
}

fn errors(ctx: &mut Ctx) -> R {
    let id = import_clip(ctx)?;
    let mut problems = Vec::new();
    let r = ctx.cli(&["export", &id, "--preset", "no-such-preset"])?;
    ctx.note(format!(
        "unknown preset: {} {:?} {}",
        exit_text(r.code()),
        r.error_code(),
        crate::util::cut(&r.error_message(), 140)
    ));
    if r.ok() || r.code() != Some(2) || r.error_code().as_deref() != Some("invalid_input") {
        problems.push(format!(
            "unknown preset gave {} {:?}",
            exit_text(r.code()),
            r.error_code()
        ));
    }
    let r = ctx.cli(&["export", "rec_DOESNOTEXIST", "--preset", "quick"])?;
    ctx.note(format!(
        "unknown capture: {} {:?}",
        exit_text(r.code()),
        r.error_code()
    ));
    if r.ok() || r.code() != Some(2) || r.error_code().as_deref() != Some("not_found") {
        problems.push(format!(
            "unknown capture gave {} {:?}",
            exit_text(r.code()),
            r.error_code()
        ));
    }
    if is_crash_code(r.code()) {
        problems.push("the CLI crashed".to_owned());
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}
