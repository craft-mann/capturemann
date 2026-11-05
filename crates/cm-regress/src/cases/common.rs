//! Steps several cases share.

use crate::ctx::Ctx;
use crate::fixtures;
use crate::model::{fail, Stop, R};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The id the seeded screenshot bundle has.
pub const SEEDED_SHOT: &str = "shot_REGRESSFIXTURE0001";

/// Writes a screenshot bundle holding the committed text picture (an e-mail address and an IPv4
/// address) into the sandbox library. Call before `start_daemon`: the library is read at start. Gives
/// the library, annotate, undo and Auto-redact cases a capture with no screen involved.
pub fn seed_screenshot(ctx: &Ctx) -> R<PathBuf> {
    let dir = ctx.library.join("Fixture.cmshot");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("original.png"), fixtures::REDACT_PNG)?;
    let sha = crate::util::sha256_hex(fixtures::REDACT_PNG);
    let manifest = format!(
        r#"{{"schema":1,"id":"{SEEDED_SHOT}","title":"Fixture","kind":"screenshot","created":"2026-11-05T07:00:00Z","original":{{"file":"original.png","size":[1000,260],"bytes":{},"sha256":"{sha}"}},"render":null,"ops":[],"op_seq":0,"revisions":[],"history":{{"undo":[],"redo":[]}}}}"#,
        fixtures::REDACT_PNG.len()
    );
    std::fs::write(dir.join("manifest.json"), manifest)?;
    Ok(dir)
}

/// Waits for a job to end (`jobs get`), returning its final document.
pub fn wait_job(ctx: &mut Ctx, job_id: &str, secs: u64) -> R<Value> {
    let start = std::time::Instant::now();
    loop {
        let v = ctx.cli_ok(&["jobs", "get", job_id])?;
        let state = v
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if matches!(state.as_str(), "done" | "failed" | "cancelled") {
            return Ok(v);
        }
        if start.elapsed() > ctx.secs(secs) {
            return fail(format!("job {job_id} still `{state}` after {secs} s"));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Imports a file with `import --wait`; returns (capture id, the job document).
pub fn import_file(ctx: &mut Ctx, path: &Path) -> R<(String, Value)> {
    let p = path.to_string_lossy().into_owned();
    let r = ctx.cli_long(&["import", &p, "--wait", "--timeout", "180"], 200)?;
    let Some(job) = r.json.clone() else {
        return fail(format!(
            "import of {} printed no job ({}): {}",
            path.display(),
            crate::proc::exit_text(r.out.code),
            crate::util::cut(&crate::util::one_line(&r.out.text()), 300)
        ));
    };
    let state = job.get("state").and_then(Value::as_str).unwrap_or("");
    if state != "done" || !r.ok() {
        return fail(format!(
            "import of {} ended `{state}` ({}): {}",
            path.display(),
            crate::proc::exit_text(r.out.code),
            crate::util::cut(
                &job.get("error").map(Value::to_string).unwrap_or_default(),
                300
            )
        ));
    }
    let id = job
        .pointer("/result/files/0/capture_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Stop::Fail(format!(
                "the import job has no capture id: {}",
                crate::util::cut(&job.to_string(), 300)
            ))
        })?
        .to_owned();
    Ok((id, job))
}

/// The library entry of a capture from `library list`.
pub fn library_entry(ctx: &mut Ctx, id: &str) -> R<Value> {
    let v = ctx.cli_ok(&["library", "list"])?;
    v.get("captures")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|c| c.get("id").and_then(Value::as_str) == Some(id))
        })
        .cloned()
        .ok_or_else(|| Stop::Fail(format!("`library list` does not list {id}")))
}

/// Reads `manifest.json` of a bundle.
pub fn manifest(bundle: &Path) -> R<Value> {
    let t = std::fs::read_to_string(bundle.join("manifest.json"))
        .map_err(|e| Stop::Fail(format!("{}: {e}", bundle.join("manifest.json").display())))?;
    Ok(serde_json::from_str(&t)?)
}

/// Needs the test fixtures written to disk: returns the folder.
pub fn write_clips(ctx: &Ctx) -> R<PathBuf> {
    let d = ctx.dir("clips");
    for (n, b) in [
        ("clip-h264.mp4", fixtures::CLIP_MP4),
        ("clip-h264.mov", fixtures::CLIP_MOV),
        ("clip-h264.mkv", fixtures::CLIP_MKV),
        ("clip-rotated.mp4", fixtures::CLIP_ROTATED),
        ("clip-vfr.mp4", fixtures::CLIP_VFR),
    ] {
        std::fs::write(d.join(n), b)?;
    }
    Ok(d)
}

/// Needs the bundled FFmpeg (or a PATH copy for a build tree): SKIP when neither runs.
pub fn need_ffmpeg(ctx: &mut Ctx) -> R {
    if ctx.pkg.has("ffmpeg") {
        return Ok(());
    }
    if ctx.opts.dev {
        ctx.note("--dev: no bundled FFmpeg, the one on PATH is used");
        return Ok(());
    }
    fail("the package has no ffmpeg next to the programs")
}

/// Checks a file is a real PNG; returns its size.
pub fn check_png(path: &Path) -> R<(u32, u32)> {
    let b = std::fs::read(path).map_err(|e| Stop::Fail(format!("{}: {e}", path.display())))?;
    fixtures::png_info(&b).map_err(|e| Stop::Fail(format!("{}: {e}", path.display())))
}

/// The error text of an error message must not read like a different platform.
pub fn wrong_platform_words(text: &str, platform: &str) -> Vec<&'static str> {
    let lower = text.to_lowercase();
    let mut hits = Vec::new();
    if platform != "macos" {
        for w in [
            "homebrew",
            "brew install",
            "videotoolbox",
            "apple silicon",
            "macos",
        ] {
            if lower.contains(w) {
                hits.push(match w {
                    "homebrew" => "homebrew",
                    "brew install" => "brew install",
                    "videotoolbox" => "videotoolbox",
                    "apple silicon" => "apple silicon",
                    _ => "macos",
                });
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_words() {
        assert_eq!(
            wrong_platform_words("Install it with Homebrew", "windows"),
            ["homebrew"]
        );
        assert!(wrong_platform_words("Install it with Homebrew", "macos").is_empty());
        assert!(wrong_platform_words("FFmpeg was not found next to the app", "windows").is_empty());
    }
}
