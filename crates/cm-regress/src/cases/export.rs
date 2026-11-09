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
            id: "export.estimate",
            title: "export --estimate prints a range with codec and pixels; a GIF has none; the refusals are the shared ones",
            area: "export",
            specs: &["EXP-5.AC3", "EXP-5.AC4"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: estimate_cli,
        },
        Case {
            id: "export.estimate_mcp",
            title: "the MCP tool estimate_export answers the same range as the CLI",
            area: "export",
            specs: &["EXP-5.AC3", "AGT-1"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: estimate_mcp,
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
    /// The codec the job says the file has, and why when it is not hardware H.264 (O10).
    codec: String,
    codec_note: Option<String>,
}

/// EXP-1 / EXP-2 on Linux (decision O10): an MP4 preset is hardware H.264 (VA-API) or, where the
/// computer has none, VP9 video that the job result labels (`codec` and a `codec_note`). A
/// VP9 file with no label, or H.264 from a software encoder, is a failure; macOS and Windows
/// still require H.264. Pure, so `cargo test -p cm-regress` covers it.
fn video_codec_problem(os: &str, file_codec: &str, e: &Exported) -> Option<String> {
    if file_codec == "h264" {
        let software = ["libx264", "libopenh264"];
        return (os == "linux" && software.contains(&e.encoder.as_str()))
            .then(|| format!("software H.264 encoder {} must never ship", e.encoder));
    }
    if os != "linux" {
        return Some(format!("the export is {file_codec} not h264"));
    }
    if file_codec != "vp9" {
        return Some(format!(
            "the export is {file_codec}, expected h264 or labelled vp9"
        ));
    }
    if e.codec != "vp9" {
        return Some(format!(
            "the file is VP9 but the job result says codec `{}`",
            e.codec
        ));
    }
    match e.codec_note.as_deref() {
        Some(n) if n.contains("VP9") && n.contains("H.264") => None,
        _ => Some("the file is VP9 but the job result gives no codec_note saying why".to_owned()),
    }
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
    let codec = job
        .pointer("/result/codec")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let codec_note = job
        .pointer("/result/codec_note")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(Exported {
        path: path.into(),
        encoder,
        codec,
        codec_note,
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
    if let Some(problem) = video_codec_problem(sys::PLATFORM, &v.codec_name, &e) {
        return fail(format!("the quick export: {problem}"));
    }
    if let Some(n) = &e.codec_note {
        ctx.note(format!("codec note: {n}"));
    }
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
    if let Some(problem) = tag_problem(v, &e.encoder) {
        return Err(crate::model::Stop::Fail(format!(
            "the quick export: {problem}"
        )));
    } else {
        ctx.note(format!(
            "encoder {}: colour tags not asserted (only the Media Foundation path had the bug)",
            e.encoder
        ));
    }
    Ok(())
}

/// The colour tags an H.264 export by a NV12 hardware encoder must carry in the file (ffprobe's
/// view of the stream and the `colr` atom): BT.709 matrix, primaries and transfer, limited range.
/// Media Foundation writes none of them by itself, so a missing field means the exporter's
/// `colr`/`h264_metadata` step is not working (or the installed FFmpeg lacks `h264_metadata`).
fn tag_problem(v: &media::Stream, encoder: &str) -> Option<String> {
    if !(encoder.contains("_mf") || encoder.contains("qsv")) {
        return None;
    }
    let got = [
        ("color_space", v.color_space.as_str(), "bt709"),
        ("color_primaries", v.color_primaries.as_str(), "bt709"),
        ("color_transfer", v.color_transfer.as_str(), "bt709"),
        ("color_range", v.color_range.as_str(), "tv"),
    ];
    let bad: Vec<String> = got
        .iter()
        .filter(|(_, have, want)| have != want)
        .map(|(k, have, want)| format!("{k} '{have}' (expected {want})"))
        .collect();
    (!bad.is_empty()).then(|| format!("{encoder} export is badly tagged: {}", bad.join(", ")))
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
                if let Some(p) = video_codec_problem(sys::PLATFORM, &v.codec_name, &e) {
                    problems.push(format!("{name}: {p}"));
                }
                if v.width > 1920 || v.height > 1080 {
                    problems.push(format!("{name}: {} {}x{}", v.codec_name, v.width, v.height));
                }
                if let Some(n) = &e.codec_note {
                    ctx.note(format!("{name}: codec note: {n}"));
                }
                if let Some(t) = tag_problem(v, &e.encoder) {
                    problems.push(format!("{name}: {t}"));
                }
            }
            "vertical-1080" | "square-1080" => {
                if name == "vertical-1080" && v.height <= v.width {
                    problems.push(format!("{name}: {}x{} is not portrait", v.width, v.height));
                }
                if name == "square-1080" && v.height != v.width {
                    problems.push(format!("{name}: {}x{} is not square", v.width, v.height));
                }
                if let Some(t) = tag_problem(v, &e.encoder) {
                    problems.push(format!("{name}: {t}"));
                }
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

/// EXP-5: `export <id> --estimate` (no export is made): a range, the codec and the pixels; the
/// MP4 on Linux without H.264 says `vp9`; a GIF has no estimate; a step above the recording is
/// refused with `size_larger_than_source`.
fn estimate_cli(ctx: &mut Ctx) -> R {
    let id = import_clip(ctx)?;
    let j = ctx.cli_ok(&["export", &id, "--estimate"])?;
    let (lo, hi) = (
        j.get("min_bytes").and_then(Value::as_u64),
        j.get("max_bytes").and_then(Value::as_u64),
    );
    check!(
        matches!((lo, hi), (Some(a), Some(b)) if a > 0 && a < b),
        "no sensible range: {j}"
    );
    let codec = j.get("codec").and_then(Value::as_str).unwrap_or("");
    check!(
        codec == "h264" || (sys::PLATFORM == "linux" && codec == "vp9"),
        "the MP4 estimate says codec `{codec}`"
    );
    check!(
        j.get("label")
            .and_then(Value::as_str)
            .is_some_and(|l| l.starts_with("About ") || l == "Under 5 MB"),
        "the label is not 'About N-M MB' or 'Under 5 MB': {j}"
    );
    let w = j.get("width").and_then(Value::as_u64).unwrap_or(0);
    let h = j.get("height").and_then(Value::as_u64).unwrap_or(0);
    check!(w > 0 && h > 0 && w % 2 == 0 && h % 2 == 0, "pixels {w}x{h}");
    // A WebM at a smaller step: VP9, smaller pixels, and nothing was exported.
    let sizes = j
        .pointer("/options/sizes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    check!(!sizes.is_empty(), "the options list no sizes: {j}");
    if let Some(step) = sizes.get(1) {
        let size = step.get("size").map(Value::to_string).unwrap_or_default();
        let r = ctx.cli_ok(&[
            "export",
            &id,
            "--format",
            "webm",
            "--size",
            &size,
            "--estimate",
        ])?;
        check!(
            r.get("codec").and_then(Value::as_str) == Some("vp9"),
            "webm: {r}"
        );
        check!(
            r.get("height").and_then(Value::as_u64) == step.get("height").and_then(Value::as_u64),
            "webm {size}: {r}"
        );
    }
    let jobs = ctx.cli_ok(&["jobs", "list", "--type", "export"])?;
    check!(
        jobs.get("jobs")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty),
        "--estimate queued an export: {jobs}"
    );
    // GIF: no range ("known after export"), or the 30 s refusal for a long clip.
    let g = ctx.cli(&["export", &id, "--format", "gif", "--estimate"])?;
    check!(
        (g.ok()
            && g.json.as_ref().and_then(|v| v.get("known_after_export"))
                == Some(&Value::Bool(true)))
            || g.error_code().as_deref() == Some("invalid_input"),
        "GIF estimate: {}",
        crate::util::cut(&g.out.text(), 200)
    );
    // A step above the recording is refused with the shared text; `--size` with GIF is invalid.
    let r = ctx.cli(&["export", &id, "--size", "1440p", "--estimate"])?;
    if r.ok() {
        ctx.note("the clip is larger than 1440p: the step is allowed".to_owned());
    } else {
        check!(
            r.code() == Some(2) && r.error_code().as_deref() == Some("invalid_input"),
            "1440p above the recording gave {} {:?}",
            exit_text(r.code()),
            r.error_code()
        );
        check!(
            r.error_message().contains("is larger than the recording"),
            "text: {}",
            r.error_message()
        );
    }
    let r = ctx.cli(&[
        "export",
        &id,
        "--format",
        "gif",
        "--size",
        "720p",
        "--estimate",
    ])?;
    check!(
        !r.ok() && r.error_code().as_deref() == Some("invalid_input"),
        "--size with GIF gave {}",
        exit_text(r.code())
    );
    Ok(())
}

/// EXP-5: the MCP tool `estimate_export` gives the numbers of `export --estimate`.
fn estimate_mcp(ctx: &mut Ctx) -> R {
    let id = import_clip(ctx)?;
    let cli = ctx.cli_ok(&["export", &id, "--estimate"])?;
    let mut c = crate::mcp::McpClient::spawn(ctx, &[])?;
    if let Err(e) = c.initialize() {
        return fail(format!("initialize failed: {e}\n{}", c.transcript()));
    }
    let r = match c.call("estimate_export", serde_json::json!({ "capture_id": id })) {
        Ok(Ok(r)) => r,
        Ok(Err((code, msg))) => return fail(format!("estimate_export: JSON-RPC {code} {msg}")),
        Err(e) => return fail(format!("estimate_export: {e}\n{}", c.transcript())),
    };
    let v = r.json().unwrap_or(Value::Null);
    let g = c.call(
        "estimate_export",
        serde_json::json!({ "capture_id": id, "format": "webm", "size": "source" }),
    );
    c.close();
    check!(
        !r.is_error,
        "estimate_export failed: {}",
        crate::util::cut(&r.text, 200)
    );
    for key in [
        "min_bytes",
        "max_bytes",
        "label",
        "codec",
        "width",
        "height",
    ] {
        check!(
            v.get(key) == cli.get(key),
            "MCP and CLI differ in `{key}`: {v} / {cli}"
        );
    }
    if let Ok(Ok(g)) = g {
        check!(
            !g.is_error,
            "webm estimate failed: {}",
            crate::util::cut(&g.text, 200)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn exported(encoder: &str, codec: &str, note: Option<&str>) -> Exported {
        Exported {
            path: "x.mp4".into(),
            encoder: encoder.into(),
            codec: codec.into(),
            codec_note: note.map(str::to_owned),
        }
    }

    /// EXP-1 / EXP-2 under O10: Linux accepts hardware H.264 or labelled VP9, nothing else.
    #[test]
    fn linux_accepts_hardware_h264_or_a_labelled_vp9() {
        let hw = exported("h264_vaapi", "h264", None);
        assert_eq!(video_codec_problem("linux", "h264", &hw), None);
        let note = "This export is VP9 video, not H.264: this computer has no VA-API render node.";
        let vp9 = exported("libvpx-vp9", "vp9", Some(note));
        assert_eq!(video_codec_problem("linux", "vp9", &vp9), None);
        // An unlabelled VP9 file is the product being dishonest.
        let silent = exported("libvpx-vp9", "vp9", None);
        assert!(video_codec_problem("linux", "vp9", &silent)
            .unwrap()
            .contains("no codec_note"));
        // The result must agree with the file.
        let liar = exported("libvpx-vp9", "h264", Some(note));
        assert!(video_codec_problem("linux", "vp9", &liar).is_some());
        // Software H.264 never ships.
        let x264 = exported("libx264", "h264", None);
        assert!(video_codec_problem("linux", "h264", &x264).is_some());
    }

    #[test]
    fn macos_and_windows_still_require_h264() {
        let vp9 = exported(
            "libvpx-vp9",
            "vp9",
            Some("This export is VP9 video, not H.264: x."),
        );
        for os in ["macos", "windows"] {
            assert!(video_codec_problem(os, "vp9", &vp9).is_some(), "{os}");
            let hw = exported("h264_videotoolbox", "h264", None);
            assert_eq!(video_codec_problem(os, "h264", &hw), None, "{os}");
        }
    }
}
