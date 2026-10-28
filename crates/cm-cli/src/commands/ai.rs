//! `capturemann ai ...` and `export --captions`: speech models, transcripts, captions and
//! suggestions (M5). Every command is one daemon action with the same name as its MCP tool.

use std::fmt::Write as _;
use std::io::{Read, Write};

use cm_core::ErrorCode;
use serde_json::{json, Value};

use super::{plain, Ctx};
use crate::args::{
    AiCmd, CaptionsCmd, ModelsCmd, ReportArgs, ReportCmd, SpeakersArgs, SpeakersCmd, SuggestCmd,
    TextModelCmd, TextModelsCmd, TranscribeArgs, TranslateArgs,
};
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
        AiCmd::TextModels { cmd: None } => {
            let mut c = cx.client().await?;
            let r = c.call("list_text_models", json!({})).await?;
            cx.emit(&r, text_model_table);
            Ok(())
        }
        AiCmd::TextModels {
            cmd:
                Some(TextModelsCmd::Download {
                    model,
                    wait,
                    timeout,
                }),
        } => text_model_download(cx, model, wait, timeout).await,
        AiCmd::TextModels {
            cmd: Some(TextModelsCmd::Use { model }),
        } => {
            let params = cx.mutating(json!({ "model": model }), "");
            let mut c = cx.client().await?;
            let r = c.call("select_text_model", params).await?;
            cx.emit(&r, |r| {
                format!("`{}` is now the built-in text model", plain(&r["active"]))
            });
            Ok(())
        }
        AiCmd::TextModels {
            cmd: Some(TextModelsCmd::Rm { model }),
        } => {
            let params = cx.mutating(json!({ "model": model }), "");
            let mut c = cx.client().await?;
            let r = c.call("remove_text_model", params).await?;
            cx.emit(&r, |r| {
                format!("Deleted the text model `{}`", plain(&r["model"]))
            });
            Ok(())
        }
        AiCmd::Report(a) => report(cx, a).await,
        AiCmd::Transcribe(a) => transcribe(cx, a).await,
        AiCmd::Transcript { id, words } => transcript(cx, id, words).await,
        AiCmd::Translate(a) => translate(cx, a).await,
        AiCmd::Captions {
            cmd: CaptionsCmd::Get { id, language },
        } => captions_get(cx, id, language).await,
        AiCmd::Captions {
            cmd:
                CaptionsCmd::Set {
                    id,
                    file,
                    clear,
                    language,
                },
        } => captions_set(cx, id, file, clear, language).await,
        AiCmd::Captions {
            cmd:
                CaptionsCmd::Replace {
                    id,
                    find,
                    replace,
                    match_case,
                    whole_word,
                    file,
                    language,
                },
        } => {
            captions_replace(
                cx, id, find, replace, match_case, whole_word, file, language,
            )
            .await
        }
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
        AiCmd::Speakers(a) => speakers(cx, a).await,
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
            return "No text model is set up. Download the built-in model on the AI page of the app.\n"
                .to_owned();
        }
        format!(
            "Text model: {}, model {}\n  context {} tokens, timeout {} s, structured output {}\n",
            plain(&r["provider"]),
            plain(&r["model"]),
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

/// A time argument: seconds (`12.5`) or ticks (`t:1500000`), passed on for the daemon to check.
fn time_arg(field: &str, s: &str) -> Result<Value, CliError> {
    if s.strip_prefix("t:")
        .is_some_and(|t| t.parse::<i64>().is_ok())
    {
        return Ok(json!(s));
    }
    s.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|n| json!(n))
        .ok_or_else(|| {
            CliError::invalid_input(format!("{field}: `{s}` is not seconds or t:<ticks>"))
        })
}

/// `ai report ...`: the AI-9 reports.
async fn report(cx: &mut Ctx<'_>, a: ReportArgs) -> Result<(), CliError> {
    match a.cmd {
        Some(ReportCmd::Templates) => {
            let mut c = cx.client().await?;
            let r = c.call("list_report_templates", json!({})).await?;
            cx.emit(&r, |r| {
                let mut s = String::new();
                for t in r["templates"].as_array().cloned().unwrap_or_default() {
                    let _ = writeln!(s, "{:<14} {}", plain(&t["id"]), plain(&t["name"]));
                    let _ = writeln!(s, "{:<14} {}", "", plain(&t["description"]));
                    let fields: Vec<String> = t["context_fields"]
                        .as_array()
                        .map(|a| a.iter().map(plain).collect())
                        .unwrap_or_default();
                    let _ = writeln!(s, "{:<14} context: {}", "", fields.join(", "));
                    if let Some(n) = t["notes_only"].as_str() {
                        let _ = writeln!(s, "{:<14} {n}", "");
                    }
                }
                s
            });
            Ok(())
        }
        Some(ReportCmd::Get { id, report }) => {
            let mut p = json!({ "capture_id": id });
            if let Some(r) = report {
                p["report_id"] = json!(r);
            }
            let mut c = cx.client().await?;
            let r = c.call("get_report", p).await?;
            cx.emit(&r, |r| {
                if let Some(md) = r["markdown"].as_str().filter(|_| r["report"].is_object()) {
                    return md.to_owned();
                }
                let mut s = format!("Reports of {}:\n", plain(&r["capture_id"]));
                for x in r["reports"].as_array().cloned().unwrap_or_default() {
                    let _ = writeln!(
                        s,
                        "  {}  {}  {}{}",
                        plain(&x["report_id"]),
                        plain(&x["template_name"]),
                        plain(&x["created"]),
                        if x["edited"] == json!(true) {
                            "  (edited)"
                        } else {
                            ""
                        },
                    );
                }
                if r["reports"].as_array().is_none_or(Vec::is_empty) {
                    s.push_str("  (none)\n");
                }
                s
            });
            Ok(())
        }
        Some(ReportCmd::Export {
            id,
            report,
            format,
            out_dir,
            times,
        }) => {
            let mut p = json!({ "capture_id": id, "report_id": report, "format": format });
            if times {
                p["include_times"] = json!(true);
            }
            if let Some(d) = out_dir {
                p["out_dir"] = json!(d.to_string_lossy());
            }
            let params = cx.mutating(p, "");
            let mut c = cx.client().await?;
            let r = c.call("export_report", params).await?;
            cx.emit(&r, |r| format!("Wrote {}", plain(&r["path"])));
            Ok(())
        }
        Some(ReportCmd::Delete { id, report }) => {
            let params = cx.mutating(json!({ "capture_id": id, "report_id": report }), "");
            let mut c = cx.client().await?;
            let r = c.call("delete_report", params).await?;
            cx.emit(&r, |r| {
                format!(
                    "Deleted the report {} of {}\n  undo with `capturemann undo {}`",
                    plain(&r["report_id"]),
                    plain(&r["capture_id"]),
                    plain(&r["capture_id"]),
                )
            });
            Ok(())
        }
        Some(ReportCmd::Edit { id, report, file }) => {
            let markdown = if file.as_os_str() == "-" {
                let mut s = String::new();
                cx.env
                    .stdin
                    .read_to_string(&mut s)
                    .map_err(|e| CliError::invalid_input(format!("cannot read stdin: {e}")))?;
                s
            } else {
                std::fs::read_to_string(&file).map_err(|e| {
                    CliError::invalid_input(format!("cannot read `{}`: {e}", file.display()))
                })?
            };
            let p = json!({ "capture_id": id, "report_id": report, "markdown": markdown });
            let params = cx.mutating(p, "");
            let mut c = cx.client().await?;
            let r = c.call("edit_report", params).await?;
            cx.emit(&r, |r| {
                format!(
                    "Edited the report {} of {} (revision {})\n  undo with `capturemann undo {}`",
                    plain(&r["report_id"]),
                    plain(&r["capture_id"]),
                    plain(&r["revision"]),
                    plain(&r["capture_id"]),
                )
            });
            Ok(())
        }
        Some(ReportCmd::Check {
            id,
            report,
            section,
            item,
            uncheck,
        }) => {
            let p = json!({ "capture_id": id, "report_id": report, "section": section,
                            "index": item, "checked": !uncheck });
            let params = cx.mutating(p, "");
            let mut c = cx.client().await?;
            let r = c.call("check_report_item", params).await?;
            cx.emit(&r, |r| {
                let word = if r["checked"] == json!(true) {
                    "Ticked"
                } else {
                    "Unticked"
                };
                format!(
                    "{word} item {} of {} in the report {} of {}\n  undo with `capturemann undo {}`",
                    plain(&r["index"]),
                    plain(&r["section"]),
                    plain(&r["report_id"]),
                    plain(&r["capture_id"]),
                    plain(&r["capture_id"]),
                )
            });
            Ok(())
        }
        None => generate_report(cx, a).await,
    }
}

async fn generate_report(cx: &mut Ctx<'_>, a: ReportArgs) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": a.id.unwrap_or_default(),
                        "template": a.template.unwrap_or_default() });
    if let Some(l) = a.lang {
        p["language"] = json!(l);
    }
    if let Some(r) = a.report {
        p["report_id"] = json!(r);
    }
    let mut context = serde_json::Map::new();
    if let Some(t) = a.title {
        context.insert("title".into(), json!(t));
    }
    if !a.attendees.is_empty() {
        context.insert("attendees".into(), json!(a.attendees));
    }
    if let Some(t) = a.agenda {
        context.insert("agenda".into(), json!(t));
    }
    if let Some(t) = a.role {
        context.insert("role".into(), json!(t));
    }
    if !a.competencies.is_empty() {
        context.insert("competencies".into(), json!(a.competencies));
    }
    if !context.is_empty() {
        p["context"] = Value::Object(context);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("generate_report", params).await?;
    if !a.wait {
        cx.emit(&started, |r| {
            format!(
                "Writing the `{}` report of {}: job {}{}\n  watch it with `capturemann jobs get {}`",
                plain(&r["template"]),
                plain(&r["capture_id"]),
                plain(&r["job_id"]),
                if r["reused"] == json!(true) { " (already running)" } else { "" },
                plain(&r["job_id"]),
            )
        });
        return Ok(());
    }
    let job = wait_job(
        cx,
        &mut c,
        &plain(&started["job_id"]),
        "writing the report",
        a.timeout,
    )
    .await?;
    cx.emit(&job, |j| {
        let r = &j["result"];
        format!(
            "Wrote report {} of {} ({} items dropped without a matching citation)\n  read it with `capturemann ai report get {} {}`",
            plain(&r["report_id"]),
            plain(&r["capture_id"]),
            plain(&r["dropped_uncited"]),
            plain(&r["capture_id"]),
            plain(&r["report_id"]),
        )
    });
    Ok(())
}

/// `ai speakers <id>` (`diarize`) and `ai speakers rename|merge|assign|list` (`set_speakers`,
/// `get_transcript`).
async fn speakers(cx: &mut Ctx<'_>, a: SpeakersArgs) -> Result<(), CliError> {
    match a.cmd {
        Some(SpeakersCmd::Rename { id, speaker, name }) => {
            let p = json!({ "capture_id": id, "rename": { speaker.to_string(): name } });
            set_speakers(cx, p).await
        }
        Some(SpeakersCmd::Merge { id, from, into }) => {
            let p = json!({ "capture_id": id, "merge": [{ "from": from, "into": into }] });
            set_speakers(cx, p).await
        }
        Some(SpeakersCmd::Assign {
            id,
            speaker,
            start,
            end,
        }) => {
            let p = json!({ "capture_id": id, "assign": [{
                "start": time_arg("start", &start)?, "end": time_arg("end", &end)?,
                "speaker": speaker }] });
            set_speakers(cx, p).await
        }
        Some(SpeakersCmd::List { id }) => {
            let mut c = cx.client().await?;
            let r = c
                .call("get_transcript", json!({ "capture_id": id }))
                .await?;
            cx.emit(&r, |r| match r["speakers"].as_array() {
                Some(list) => speaker_list(&plain(&r["capture_id"]), list),
                None => format!(
                    "{} has no speakers yet; run `capturemann ai speakers {}`",
                    plain(&r["capture_id"]),
                    plain(&r["capture_id"])
                ),
            });
            Ok(())
        }
        None => diarize(cx, a).await,
    }
}

fn speaker_list(id: &str, list: &[Value]) -> String {
    let mut s = format!("Speakers of {id}:\n");
    for sp in list {
        let _ = writeln!(s, "  {}  {}", plain(&sp["id"]), plain(&sp["name"]));
    }
    if list.is_empty() {
        s.push_str("  (none)\n");
    }
    s
}

async fn set_speakers(cx: &mut Ctx<'_>, p: Value) -> Result<(), CliError> {
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let r = c.call("set_speakers", params).await?;
    cx.emit(&r, |r| {
        let list = r["speakers"].as_array().cloned().unwrap_or_default();
        let head = if r["changed"] == json!(true) {
            format!(
                "Changed the speakers of {} (revision {})\n  undo with `capturemann undo {}`\n",
                plain(&r["capture_id"]),
                plain(&r["revision"]),
                plain(&r["capture_id"]),
            )
        } else {
            format!("Nothing changed in {}\n", plain(&r["capture_id"]))
        };
        format!("{head}{}", speaker_list(&plain(&r["capture_id"]), &list))
    });
    Ok(())
}

async fn diarize(cx: &mut Ctx<'_>, a: SpeakersArgs) -> Result<(), CliError> {
    let id = a.id.unwrap_or_default();
    let mut p = json!({ "capture_id": id });
    for (k, v) in [
        ("num_speakers", a.num),
        ("min_speakers", a.min),
        ("max_speakers", a.max),
    ] {
        if let Some(n) = v {
            p[k] = json!(n);
        }
    }
    if a.headset {
        p["headset"] = json!(true);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("diarize", params).await?;
    if !a.wait {
        cx.emit(&started, |r| {
            format!(
                "Finding the speakers of {}: job {}{}\n  watch it with `capturemann jobs get {}`",
                plain(&r["capture_id"]),
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
        "finding the speakers",
        a.timeout,
    )
    .await?;
    cx.emit(&job, |j| {
        let r = &j["result"];
        let list = r["speakers"].as_array().cloned().unwrap_or_default();
        let mut s = format!(
            "Found {} speakers in {} (model `{}`)\n",
            list.len(),
            plain(&r["capture_id"]),
            plain(&r["model"]),
        );
        if let Some(note) = r["note"].as_str() {
            let _ = writeln!(s, "  {note}");
        }
        s.push_str(&speaker_list(&plain(&r["capture_id"]), &list));
        let _ = write!(
            s,
            "  rename one with `capturemann ai speakers rename {} <number> <name>`",
            plain(&r["capture_id"])
        );
        s
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

fn text_model_table(r: &Value) -> String {
    let models = r["models"].as_array().cloned().unwrap_or_default();
    let mut s = format!("Built-in text models (folder: {})\n", plain(&r["dir"]));
    if models.is_empty() {
        s.push_str("No text model is available in this build yet.\n");
        return s;
    }
    let width = models
        .iter()
        .map(|m| plain(&m["id"]).len())
        .max()
        .unwrap_or(0);
    for m in &models {
        let state = match (m["installed"] == json!(true), m["active"] == json!(true)) {
            (true, true) => "downloaded, used by default",
            (true, false) => "downloaded",
            _ => "not downloaded",
        };
        let _ = writeln!(
            s,
            "{} {:<width$}  {:>5} MB  needs {} MB{}  {}  {state}",
            if m["active"] == json!(true) { "*" } else { " " },
            plain(&m["id"]),
            plain(&m["size_mb"]),
            plain(&m["ram_mb"]),
            if m["recommended"] == json!(true) {
                ", recommended".to_owned()
            } else if let Some(gb) = m["min_memory_gb"].as_u64().filter(|g| *g > 0) {
                format!(", for Macs with {gb} GB or more")
            } else {
                String::new()
            },
            plain(&m["license"]),
        );
    }
    s
}

async fn text_model_download(
    cx: &mut Ctx<'_>,
    model: String,
    wait: bool,
    timeout: Option<f64>,
) -> Result<(), CliError> {
    let params = cx.mutating(json!({ "model": model }), "");
    let mut c = cx.client().await?;
    let started = c.call("download_text_model", params).await?;
    if !wait {
        cx.emit(&started, |r| {
            format!(
                "Downloading the text model `{}`: job {}{}\n  watch it with `capturemann jobs get {}`",
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
        format!(
            "Downloaded the text model `{}`",
            plain(&j["result"]["model"])
        )
    });
    Ok(())
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
    if a.original_audio {
        p["original_audio"] = json!(true);
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

/// `ai translate`: `translate_captions`, then (with `--wait`) polls the job.
async fn translate(cx: &mut Ctx<'_>, a: TranslateArgs) -> Result<(), CliError> {
    let p = json!({ "capture_id": a.id, "target_language": a.to,
                    "engine": a.engine.replace('-', "_") });
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("translate_captions", params).await?;
    if !a.wait {
        cx.emit(&started, |r| {
            format!(
                "Translating {} into `{}` with {}{}: job {}{}\n  watch it with `capturemann jobs get {}`",
                plain(&r["capture_id"]),
                plain(&r["language"]),
                plain(&r["engine"]),
                match r["model"].as_str() {
                    Some(m) if !m.is_empty() => format!(" (model `{m}`)"),
                    _ => String::new(),
                },
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
        "translating",
        a.timeout,
    )
    .await?;
    cx.emit(&job, |j| {
        let r = &j["result"];
        format!(
            "Translated {} into `{}`: {} lines ({} left untranslated), {} `{}`\n  read it with `capturemann ai captions get {} --language {}`",
            plain(&r["capture_id"]),
            plain(&r["language"]),
            plain(&r["lines"]),
            plain(&r["untranslated"]),
            plain(&r["engine"]),
            plain(&r["model"]),
            plain(&r["capture_id"]),
            plain(&r["language"]),
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

async fn captions_get(
    cx: &mut Ctx<'_>,
    id: String,
    language: Option<String>,
) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": id });
    if let Some(l) = language {
        p["language"] = json!(l);
    }
    let mut c = cx.client().await?;
    let r = c.call("get_captions", p).await?;
    cx.emit(&r, |r| {
        let lines = r["lines"].as_array().cloned().unwrap_or_default();
        let mut s = format!(
            "Caption lines of {} ({}, {}):\n",
            plain(&r["capture_id"]),
            lines.len(),
            match r["source"].as_str() {
                Some("edited") => "edited".to_owned(),
                Some("translation") => format!(
                    "translation to {} by {} `{}`{}",
                    plain(&r["language"]),
                    plain(&r["engine"]),
                    plain(&r["model"]),
                    if r["edited"] == json!(true) {
                        ", edited"
                    } else {
                        ""
                    }
                ),
                _ => "generated".to_owned(),
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
    language: Option<String>,
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
    let mut p = json!({ "capture_id": id, "lines": lines });
    if let Some(l) = &language {
        p["language"] = json!(l);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let r = c.call("set_captions", params).await?;
    cx.emit(&r, |r| {
        if r["language"].is_string() {
            return format!(
                "Set {} lines of the `{}` translation of {}",
                plain(&r["lines"]),
                plain(&r["language"]),
                plain(&r["capture_id"]),
            );
        }
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
#[allow(clippy::too_many_arguments)]
async fn captions_replace(
    cx: &mut Ctx<'_>,
    id: String,
    find: Option<String>,
    replace: Option<String>,
    match_case: bool,
    whole_word: bool,
    file: Option<String>,
    language: Option<String>,
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
    let mut p = json!({ "capture_id": id, "replacements": pairs });
    if let Some(l) = &language {
        p["language"] = json!(l);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let r = c.call("replace_in_captions", params).await?;
    cx.emit(&r, |r| {
        if r["changed"] == json!(true) && r["language"].is_string() {
            format!(
                "Replaced {} matches in {} lines of the `{}` translation of {}",
                plain(&r["replacements"]),
                plain(&r["lines_touched"]),
                plain(&r["language"]),
                plain(&r["capture_id"]),
            )
        } else if r["changed"] == json!(true) {
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
    language: Option<String>,
    speaker_names: bool,
) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": id, "format": format });
    if timestamps {
        p["timestamps"] = json!(true);
    }
    if let Some(l) = language {
        p["language"] = json!(l);
    }
    if speaker_names {
        p["speaker_names"] = json!(true);
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
