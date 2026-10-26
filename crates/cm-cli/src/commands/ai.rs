//! `capturemann ai ...` and `export --captions`: speech models, transcripts, captions and
//! suggestions (M5). Every command is one daemon action with the same name as its MCP tool.

use std::fmt::Write as _;
use std::io::{Read, Write};

use cm_core::ErrorCode;
use serde_json::{json, Value};

use super::{plain, Ctx};
use crate::args::{AiCmd, CaptionsCmd, ModelsCmd, SuggestCmd, TextModelCmd, TranscribeArgs};
use crate::client::Client;
use crate::error::CliError;

const TICKS_PER_SECOND: f64 = 120_000.0;

fn secs(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0) / TICKS_PER_SECOND
}

/// `mm:ss.mmm`, or `h:mm:ss.mmm` past an hour.
fn stamp(seconds: f64) -> String {
    let ms = (seconds * 1000.0).round().max(0.0) as u64;
    let (h, m, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{ms:03}")
    } else {
        format!("{m:02}:{s:02}.{ms:03}")
    }
}

pub(super) async fn ai(cx: &mut Ctx<'_>, cmd: AiCmd) -> Result<(), CliError> {
    match cmd {
        AiCmd::Models { cmd: None } => models_list(cx).await,
        AiCmd::Models {
            cmd:
                Some(ModelsCmd::Download {
                    model,
                    wait,
                    timeout,
                }),
        } => model_download(cx, model, wait, timeout).await,
        AiCmd::Models {
            cmd: Some(ModelsCmd::Import { model, path }),
        } => {
            let params = cx.mutating(
                json!({ "model": model, "path": path.to_string_lossy() }),
                "",
            );
            let mut c = cx.client().await?;
            let r = c.call("import_model", params).await?;
            cx.emit(&r, |r| {
                format!("Installed the model `{}`", plain(&r["model"]))
            });
            Ok(())
        }
        AiCmd::Models {
            cmd: Some(ModelsCmd::Use { model }),
        } => {
            let params = cx.mutating(json!({ "model": model }), "");
            let mut c = cx.client().await?;
            let r = c.call("select_model", params).await?;
            cx.emit(&r, |r| {
                format!("`{}` is now the default model", plain(&r["active"]))
            });
            Ok(())
        }
        AiCmd::Models {
            cmd: Some(ModelsCmd::Rm { model }),
        } => {
            let params = cx.mutating(json!({ "model": model }), "");
            let mut c = cx.client().await?;
            let r = c.call("remove_model", params).await?;
            cx.emit(&r, |r| {
                format!("Deleted the model `{}`", plain(&r["model"]))
            });
            Ok(())
        }
        AiCmd::Transcribe(a) => transcribe(cx, a).await,
        AiCmd::Transcript { id, words } => transcript(cx, id, words).await,
        AiCmd::Captions {
            cmd: CaptionsCmd::Get { id },
        } => captions_get(cx, id).await,
        AiCmd::Captions {
            cmd: CaptionsCmd::Set { id, file, clear },
        } => captions_set(cx, id, file, clear).await,
        AiCmd::Captions {
            cmd:
                CaptionsCmd::Replace {
                    id,
                    find,
                    replace,
                    match_case,
                    whole_word,
                    file,
                },
        } => captions_replace(cx, id, find, replace, match_case, whole_word, file).await,
        AiCmd::Captions {
            cmd:
                CaptionsCmd::Options {
                    id,
                    show,
                    hide,
                    burn,
                    no_burn,
                },
        } => captions_options(cx, id, show, hide, burn, no_burn).await,
        AiCmd::Suggest {
            cmd: SuggestCmd::Zooms { id },
        } => suggest(cx, id, true).await,
        AiCmd::Suggest {
            cmd: SuggestCmd::Cuts { id },
        } => suggest(cx, id, false).await,
        AiCmd::TextModel { cmd: None } => text_model_show(cx).await,
        AiCmd::TextModel {
            cmd: Some(TextModelCmd::Test),
        } => text_model_test(cx).await,
    }
}

async fn text_model_show(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("get_text_model", json!({})).await?;
    cx.emit(&r, |r| {
        if r["provider"] == json!("none") {
            return "No text model is set up. Choose Ollama, LM Studio or a provider on the AI page of the app.\n"
                .to_owned();
        }
        format!(
            "Text model: {} ({}), model {}\n  address {}\n  context {} tokens, timeout {} s, structured output {}\n",
            plain(&r["provider"]),
            if r["local"] == json!(true) {
                "on this computer"
            } else {
                "remote: text is sent to the host"
            },
            plain(&r["model"]),
            plain(&r["base_url"]),
            plain(&r["context_tokens"]),
            plain(&r["timeout_s"]),
            match &r["structured_output"] {
                Value::Bool(true) => "works",
                Value::Bool(false) => "did not work in the last test",
                _ => "not tested yet",
            },
        )
    });
    Ok(())
}

async fn text_model_test(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("test_text_model", json!({})).await?;
    cx.emit(&r, |r| {
        if r["ok"] == json!(true) {
            format!(
                "The text model `{}` answered in {} ms (structured output {}).\n",
                plain(&r["model"]),
                plain(&r["latency_ms"]),
                if r["structured_output"] == json!(true) {
                    "works"
                } else {
                    "needed a retry"
                },
            )
        } else {
            format!(
                "The text model `{}` failed ({}): {}\n",
                plain(&r["model"]),
                plain(&r["reason"]),
                plain(&r["message"]),
            )
        }
    });
    Ok(())
}

fn model_table(r: &Value) -> String {
    let models = r["models"].as_array().cloned().unwrap_or_default();
    let width = models
        .iter()
        .map(|m| plain(&m["id"]).len())
        .max()
        .unwrap_or(0);
    let mut s = format!("Speech models (folder: {})\n", plain(&r["dir"]));
    for m in &models {
        let state = match (m["installed"] == json!(true), m["active"] == json!(true)) {
            (true, true) => "installed, used by default",
            (true, false) => "installed",
            _ => "not installed",
        };
        let _ = writeln!(
            s,
            "{} {:<width$}  {:>5} MB  {state}  {}",
            if m["active"] == json!(true) { "*" } else { " " },
            plain(&m["id"]),
            plain(&m["size_mb"]),
            plain(&m["hint"]),
        );
    }
    if !models.iter().any(|m| m["installed"] == json!(true)) {
        let suggest = models
            .iter()
            .find(|m| m["id"] == json!("small"))
            .or_else(|| models.first())
            .map(|m| (plain(&m["id"]), plain(&m["size_mb"])))
            .unwrap_or_else(|| ("small".to_owned(), "?".to_owned()));
        let _ = write!(
            s,
            "\nNo model is installed yet, so nothing can be transcribed. Download one with \
             `capturemann ai models download {0}` (about {1} MB; nothing is downloaded unless you \
             run it), or install a file you already have with `capturemann ai models import {0} \
             <file>`.",
            suggest.0, suggest.1
        );
    }
    s
}

async fn models_list(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("list_models", json!({})).await?;
    cx.emit(&r, model_table);
    Ok(())
}

async fn model_download(
    cx: &mut Ctx<'_>,
    model: String,
    wait: bool,
    timeout: Option<f64>,
) -> Result<(), CliError> {
    let params = cx.mutating(json!({ "model": model }), "");
    let mut c = cx.client().await?;
    let started = c.call("download_model", params).await?;
    if !wait {
        cx.emit(&started, |r| {
            format!(
                "Downloading `{}`: job {}{}\n  watch it with `capturemann jobs get {}`",
                plain(&r["model"]),
                plain(&r["job_id"]),
                if r["reused"] == json!(true) {
                    " (already running)"
                } else {
                    ""
                },
                plain(&r["job_id"]),
            )
        });
        return Ok(());
    }
    let job = wait_job(
        cx,
        &mut c,
        &plain(&started["job_id"]),
        "downloading",
        timeout,
    )
    .await?;
    cx.emit(&job, |j| {
        format!("Installed the model `{}`", plain(&j["result"]["model"]))
    });
    Ok(())
}

async fn transcribe(cx: &mut Ctx<'_>, a: TranscribeArgs) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": a.id });
    if let Some(l) = &a.language {
        p["language"] = json!(l);
    }
    if let Some(m) = &a.model {
        p["model"] = json!(m);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("transcribe", params).await?;
    if !a.wait {
        cx.emit(&started, |r| {
            format!(
                "Transcribing {} with `{}`: job {}{}\n  watch it with `capturemann jobs get {}`",
                plain(&r["capture_id"]),
                plain(&r["model"]),
                plain(&r["job_id"]),
                if r["reused"] == json!(true) {
                    " (already running)"
                } else {
                    ""
                },
                plain(&r["job_id"]),
            )
        });
        return Ok(());
    }
    let job = wait_job(
        cx,
        &mut c,
        &plain(&started["job_id"]),
        "transcribing",
        a.timeout,
    )
    .await?;
    cx.emit(&job, |j| {
        format!(
            "Transcribed {}: {} words, language {}, model `{}`\n  read it with `capturemann ai transcript {}`",
            plain(&j["result"]["capture_id"]),
            plain(&j["result"]["words"]),
            plain(&j["result"]["language"]),
            plain(&j["result"]["model"]),
            plain(&j["result"]["capture_id"]),
        )
    });
    Ok(())
}

/// Polls a job to its end. A failed job keeps its kind: `not_found` and `invalid_input` stay
/// themselves (exit 2), everything else is `job_failed` (exit 6).
async fn wait_job(
    cx: &mut Ctx<'_>,
    c: &mut Client,
    job_id: &str,
    verb: &str,
    timeout: Option<f64>,
) -> Result<Value, CliError> {
    let deadline =
        timeout.map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs_f64(s));
    let (mut last_pct, mut last_held) = (-1i64, false);
    loop {
        let j = c.call("get_job", json!({ "job_id": job_id })).await?;
        match j["state"].as_str() {
            Some("done") => return Ok(j),
            Some("failed") => {
                let reason = j["error"]["reason"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned();
                let mut e = match j["error"]["code"].as_str() {
                    Some("not_found") => CliError::not_found(reason),
                    Some("invalid_input") => CliError::invalid_input(reason),
                    _ => CliError::new(ErrorCode::JobFailed, reason),
                };
                e.data = json!({ "job_id": job_id });
                return Err(e);
            }
            Some("cancelled") => {
                return Err(CliError::new(ErrorCode::JobFailed, "the job was cancelled"));
            }
            _ => {}
        }
        let held = j["held"] == json!(true) || j["state"] == json!("paused");
        if held && !last_held && !cx.json {
            let _ = writeln!(
                cx.env.err,
                "the job is held (paused while a recording runs or by a user pause); waiting..."
            );
        }
        last_held = held;
        if let Some(d) = deadline {
            if tokio::time::Instant::now() >= d {
                let mut e = CliError::new(
                    ErrorCode::Busy,
                    format!(
                        "the job is still running after {} s; it keeps running in the daemon (job {job_id})",
                        timeout.unwrap_or_default()
                    ),
                );
                e.data = json!({ "job_id": job_id, "state": j["state"], "timed_out": true });
                return Err(e);
            }
        }
        if !cx.json {
            let pct = (j["progress"].as_f64().unwrap_or(0.0) * 100.0).floor() as i64;
            if pct != last_pct {
                last_pct = pct;
                let _ = writeln!(cx.env.err, "{verb}... {pct}%");
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

async fn transcript(cx: &mut Ctx<'_>, id: String, words: bool) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c
        .call("get_transcript", json!({ "capture_id": id }))
        .await?;
    cx.emit(&r, |r| {
        let t = &r["transcript"];
        let ws = t["words"].as_array().cloned().unwrap_or_default();
        let mut s = format!(
            "Transcript of {}: {} words, language {}, model `{}`\n",
            plain(&r["capture_id"]),
            ws.len(),
            plain(&t["language"]),
            plain(&t["model"]),
        );
        if let Some(note) = t["note"].as_str() {
            let _ = writeln!(s, "  {note}");
        }
        if words {
            for w in &ws {
                let _ = writeln!(
                    s,
                    "{} - {}  {}  ({:.0}%)",
                    stamp(secs(&w["start"])),
                    stamp(secs(&w["end"])),
                    plain(&w["text"]),
                    w["confidence"].as_f64().unwrap_or(0.0) * 100.0
                );
            }
        } else if let Some(lines) = r["captions"].as_array() {
            let _ = writeln!(
                s,
                "Caption lines ({}):",
                match r["captions_source"].as_str() {
                    Some("edited") => "edited",
                    _ => "generated",
                }
            );
            for l in lines {
                let _ = writeln!(
                    s,
                    "{} - {}  {}",
                    stamp(secs(&l["start"])),
                    stamp(secs(&l["end"])),
                    plain(&l["text"]).replace('\n', " / ")
                );
            }
        }
        s
    });
    Ok(())
}

async fn captions_get(cx: &mut Ctx<'_>, id: String) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("get_captions", json!({ "capture_id": id })).await?;
    cx.emit(&r, |r| {
        let lines = r["lines"].as_array().cloned().unwrap_or_default();
        let mut s = format!(
            "Caption lines of {} ({}, {}):\n",
            plain(&r["capture_id"]),
            lines.len(),
            match r["source"].as_str() {
                Some("edited") => "edited",
                _ => "generated",
            }
        );
        for l in &lines {
            let _ = writeln!(
                s,
                "{} - {}  {}",
                stamp(secs(&l["start"])),
                stamp(secs(&l["end"])),
                plain(&l["text"]).replace('\n', " / ")
            );
        }
        s
    });
    Ok(())
}

async fn captions_set(
    cx: &mut Ctx<'_>,
    id: String,
    file: Option<String>,
    clear: bool,
) -> Result<(), CliError> {
    let lines: Value = if clear {
        json!([])
    } else {
        let name = file.unwrap_or_default();
        let text = if name == "-" {
            let mut t = String::new();
            cx.env
                .stdin
                .read_to_string(&mut t)
                .map_err(|e| CliError::invalid_input(format!("cannot read stdin: {e}")))?;
            t
        } else {
            std::fs::read_to_string(&name)
                .map_err(|e| CliError::invalid_input(format!("cannot read `{name}`: {e}")))?
        };
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| CliError::invalid_input(format!("the caption file is not JSON: {e}")))?;
        if !v.is_array() {
            return Err(CliError::invalid_input(
                "the caption file must be a JSON array of { start, end, text }",
            ));
        }
        v
    };
    let params = cx.mutating(json!({ "capture_id": id, "lines": lines }), "");
    let mut c = cx.client().await?;
    let r = c.call("set_captions", params).await?;
    cx.emit(&r, |r| {
        format!(
            "Set {} caption lines of {} (revision {})\n  undo with `capturemann undo {}`",
            plain(&r["lines"]),
            plain(&r["capture_id"]),
            plain(&r["revision"]),
            plain(&r["capture_id"]),
        )
    });
    Ok(())
}

/// `ai captions replace`: `replace_in_captions` with one pair from the flags or many from a file.
async fn captions_replace(
    cx: &mut Ctx<'_>,
    id: String,
    find: Option<String>,
    replace: Option<String>,
    match_case: bool,
    whole_word: bool,
    file: Option<String>,
) -> Result<(), CliError> {
    let pairs: Value = match (find, file) {
        (Some(find), _) => json!([{
            "find": find, "replace": replace.unwrap_or_default(),
            "match_case": match_case, "whole_word": whole_word }]),
        (None, file) => {
            let name = file.unwrap_or_default();
            let text = if name == "-" {
                let mut t = String::new();
                cx.env
                    .stdin
                    .read_to_string(&mut t)
                    .map_err(|e| CliError::invalid_input(format!("cannot read stdin: {e}")))?;
                t
            } else {
                std::fs::read_to_string(&name)
                    .map_err(|e| CliError::invalid_input(format!("cannot read `{name}`: {e}")))?
            };
            let v: Value = serde_json::from_str(&text)
                .map_err(|e| CliError::invalid_input(format!("the file is not JSON: {e}")))?;
            if !v.is_array() {
                return Err(CliError::invalid_input(
                    "the file must be a JSON array of { find, replace }",
                ));
            }
            v
        }
    };
    let params = cx.mutating(json!({ "capture_id": id, "replacements": pairs }), "");
    let mut c = cx.client().await?;
    let r = c.call("replace_in_captions", params).await?;
    cx.emit(&r, |r| {
        if r["changed"] == json!(true) {
            format!(
                "Replaced {} matches in {} caption lines of {} (revision {})\n  undo with `capturemann undo {}`",
                plain(&r["replacements"]),
                plain(&r["lines_touched"]),
                plain(&r["capture_id"]),
                plain(&r["revision"]),
                plain(&r["capture_id"]),
            )
        } else {
            format!(
                "Nothing found in the captions of {}",
                plain(&r["capture_id"])
            )
        }
    });
    Ok(())
}

/// `ai captions options`: the show and burn-in switches (`set_caption_options`).
async fn captions_options(
    cx: &mut Ctx<'_>,
    id: String,
    show: bool,
    hide: bool,
    burn: bool,
    no_burn: bool,
) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": id });
    if show || hide {
        p["enabled"] = json!(show);
    }
    if burn || no_burn {
        p["burn_in"] = json!(burn);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let r = c.call("set_caption_options", params).await?;
    cx.emit(&r, |r| {
        let on = |k: &str| if r[k] == json!(true) { "on" } else { "off" };
        let head = format!(
            "Captions of {}: shown {}, burned into exports {}",
            plain(&r["capture_id"]),
            on("enabled"),
            on("burn_in"),
        );
        if r["changed"] == json!(true) {
            format!(
                "{head}\n  undo with `capturemann undo {}`",
                plain(&r["capture_id"])
            )
        } else {
            format!("{head} (nothing changed)")
        }
    });
    Ok(())
}

async fn suggest(cx: &mut Ctx<'_>, id: String, zooms: bool) -> Result<(), CliError> {
    let method = if zooms {
        "suggest_zooms"
    } else {
        "suggest_cuts"
    };
    let mut c = cx.client().await?;
    let r = c.call(method, json!({ "capture_id": id })).await?;
    cx.emit(&r, |r| {
        let list = r["suggestions"].as_array().cloned().unwrap_or_default();
        let cid = plain(&r["capture_id"]);
        if list.is_empty() {
            return format!(
                "No {} suggestions for {cid}.",
                if zooms { "zoom" } else { "cut" }
            );
        }
        let mut s = String::new();
        for (i, x) in list.iter().enumerate() {
            let (a, b) = (
                x["start_s"].as_f64().unwrap_or(0.0),
                x["end_s"].as_f64().unwrap_or(0.0),
            );
            if zooms {
                let _ = writeln!(
                    s,
                    "{}. {} - {}  zoom {:.1}x at ({:.2}, {:.2})  {}",
                    i + 1,
                    stamp(a),
                    stamp(b),
                    x["scale"].as_f64().unwrap_or(0.0),
                    x["focus"][0].as_f64().unwrap_or(0.5),
                    x["focus"][1].as_f64().unwrap_or(0.5),
                    plain(&x["reason"]),
                );
            } else {
                let _ = writeln!(
                    s,
                    "{}. {} - {}  {}  {}",
                    i + 1,
                    stamp(a),
                    stamp(b),
                    plain(&x["kind"]),
                    plain(&x["detail"]),
                );
            }
        }
        let first = &list[0];
        let (a, b) = (
            first["start_s"].as_f64().unwrap_or(0.0),
            first["end_s"].as_f64().unwrap_or(0.0),
        );
        let _ = if zooms {
            write!(
                s,
                "Nothing was applied. Accept one with, for example, `capturemann edit zoom {cid} \
                 --at {a:.2} --length {:.2} --focus {:.2},{:.2}`.",
                b - a,
                first["focus"][0].as_f64().unwrap_or(0.5),
                first["focus"][1].as_f64().unwrap_or(0.5),
            )
        } else {
            write!(
                s,
                "Nothing was applied. Accept one with, for example, `capturemann edit cut {cid} {a:.2}-{b:.2}`."
            )
        };
        s
    });
    Ok(())
}

/// `export --captions srt|vtt|txt`: the caption file instead of a video.
pub(super) async fn export_captions(
    cx: &mut Ctx<'_>,
    id: String,
    format: String,
    timestamps: bool,
) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": id, "format": format });
    if timestamps {
        p["timestamps"] = json!(true);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let r = c.call("export_captions", params).await?;
    cx.emit(&r, |r| {
        format!(
            "Wrote {} caption lines of {} as {}\n  file: {}",
            plain(&r["lines"]),
            plain(&r["capture_id"]),
            plain(&r["format"]).to_uppercase(),
            plain(&r["path"]),
        )
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_minutes_seconds_millis() {
        assert_eq!(stamp(1.5), "00:01.500");
        assert_eq!(stamp(61.25), "01:01.250");
        assert_eq!(stamp(3661.0), "1:01:01.000");
        assert_eq!(secs(&json!(240_000)), 2.0);
    }

    /// With no model installed the table tells the user how to get one, and says nothing is
    /// downloaded on its own.
    #[test]
    fn the_empty_model_table_points_to_download_and_import() {
        let r = json!({ "dir": "/models", "active": null, "models": [
            { "id": "base", "size_mb": 142, "hint": "fast", "installed": false, "active": false },
            { "id": "small", "size_mb": 466, "hint": "balanced", "installed": false, "active": false }] });
        let t = model_table(&r);
        assert!(t.contains("capturemann ai models download small"), "{t}");
        assert!(t.contains("nothing is downloaded unless you run it"), "{t}");
        assert!(t.contains("not installed"), "{t}");
        let installed = json!({ "dir": "/models", "active": "small", "models": [
            { "id": "small", "size_mb": 466, "hint": "balanced", "installed": true, "active": true }] });
        let t = model_table(&installed);
        assert!(
            t.contains("* small") && t.contains("installed, used by default"),
            "{t}"
        );
        assert!(!t.contains("No model is installed"), "{t}");
    }
}
