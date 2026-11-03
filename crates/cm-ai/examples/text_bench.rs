//! Speed and floor-quality benchmark of the built-in text model on reports and translation
//! (release 0.2.7, M2c). Prints one JSON line. Not shipped. It runs a real `llama-server` and a
//! real model that you point at (it never downloads either), so ask before running it.
//!
//! ```text
//! cargo run --release -p cm-ai --example text_bench -- --task report|translate
//!     --lines <name.lines.json> --model <gguf> --model-id <id> --llama-server <path>
//!     [--template summary|minutes] [--target Vietnamese] [--ctx 16384] [--label <id>] [--log <file>]
//! ```
//!
//! `lines.json` comes from `tests/tools/make_synthetic_meeting.py` (`start`, `end`, `speaker`,
//! `text`). Reports: `generate` over the lines, then [`cm_ai::report::rubric::score`] on the result.
//! Translation: `translate_with_text_model` over the lines as captions, English to `--target`.
//! Time is wall clock around the call (the server start is reported apart). Tokens and tokens per
//! second are the totals the server itself logs (`prompt eval time` and `eval time` lines).

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cm_ai::caption::CaptionLine;
use cm_ai::report::{
    self, generate::generate, generate::Input, rubric, Line, ReportContext,
};
use cm_ai::text_model::engine::{EmbeddedTextModel, EngineManager, EngineSpec, TextEngine};
use cm_ai::text_model::{JsonReply, JsonRequest, TextModel, TextModelConfig, TextModelError};
use cm_ai::translate::{translate_with_text_model, ModelTranslator};
use cm_core::MediaTime;
use serde_json::{json, Value};

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

/// Counts calls and times each one; passes everything through.
struct Timed<'a> {
    inner: &'a dyn TextModel,
    calls: std::sync::Mutex<Vec<(f64, usize)>>,
}

impl TextModel for Timed<'_> {
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        self.complete_json_streaming(req, &mut |_| true)
    }

    fn complete_json_streaming(
        &self,
        req: &JsonRequest,
        on_progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<JsonReply, TextModelError> {
        let began = Instant::now();
        let mut pieces = 0usize;
        let r = self.inner.complete_json_streaming(req, &mut |n| {
            pieces = n;
            on_progress(n)
        });
        self.calls
            .lock()
            .unwrap()
            .push((began.elapsed().as_secs_f64(), pieces));
        r
    }
}

/// (ms, tokens) summed over the server's `prompt eval time` and `eval time` log lines.
fn server_totals(log: &str) -> Value {
    let (mut p_ms, mut p_tok, mut g_ms, mut g_tok) = (0.0, 0u64, 0.0, 0u64);
    for l in log.lines() {
        let Some(at) = l.find("eval time =") else { continue };
        let rest = &l[at + "eval time =".len()..];
        let ms: f64 = rest
            .split_whitespace()
            .next()
            .and_then(|x| x.parse().ok())
            .unwrap_or(0.0);
        let tok: u64 = rest
            .split('/')
            .nth(1)
            .and_then(|x| x.split_whitespace().next())
            .and_then(|x| x.parse().ok())
            .unwrap_or(0);
        if l.contains("prompt eval time") {
            p_ms += ms;
            p_tok += tok;
        } else {
            g_ms += ms;
            g_tok += tok;
        }
    }
    json!({
        "prompt_tokens": p_tok, "prompt_ms": p_ms,
        "prompt_tokens_per_s": if p_ms > 0.0 { p_tok as f64 / (p_ms / 1000.0) } else { 0.0 },
        "generated_tokens": g_tok, "generated_ms": g_ms,
        "generated_tokens_per_s": if g_ms > 0.0 { g_tok as f64 / (g_ms / 1000.0) } else { 0.0 },
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let task = option(&args, "--task").expect("--task report|translate");
    let raw: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(option(&args, "--lines").expect("--lines")).unwrap(),
    )
    .unwrap();
    let lines: Vec<Line> = raw
        .iter()
        .map(|v| Line {
            start: v["start"].as_f64().unwrap(),
            end: v["end"].as_f64().unwrap(),
            speaker: v["speaker"].as_str().map(str::to_owned),
            text: v["text"].as_str().unwrap().to_owned(),
        })
        .collect();
    let model_path = PathBuf::from(option(&args, "--model").expect("--model <gguf>"));
    let model_id = option(&args, "--model-id").expect("--model-id");
    let ctx: u32 = option(&args, "--ctx").and_then(|c| c.parse().ok()).unwrap_or(16384);
    let label = option(&args, "--label").unwrap_or_else(|| "lines".into());
    let log = PathBuf::from(option(&args, "--log").unwrap_or_else(|| "text_bench_engine.log".into()));
    std::env::set_var("CM_LLAMA_SERVER", option(&args, "--llama-server").expect("--llama-server"));

    let engine: Arc<dyn TextEngine> = EngineManager::system(log.clone());
    let spec = EngineSpec {
        model_id: model_id.clone(),
        model_path,
        ram_mb: 0,
        context_tokens: ctx,
    };
    let config = TextModelConfig::default();
    let embedded = EmbeddedTextModel::new(engine.clone(), spec.clone(), &config);
    // Warm start, timed on its own: a first tiny request loads the model.
    let started = Instant::now();
    embedded
        .complete_json(&JsonRequest::new(
            "Reply with the JSON object {\"ok\": true}.",
            json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}),
        ))
        .unwrap();
    let start_secs = started.elapsed().as_secs_f64();
    let timed = Timed {
        inner: &embedded,
        calls: Default::default(),
    };
    let words: usize = lines.iter().map(|l| l.text.split_whitespace().count()).sum();
    let audio_secs = lines.last().map_or(0.0, |l| l.end);
    let mut out = json!({
        "task": task, "label": label, "model": model_id, "context_tokens": ctx,
        "lines": lines.len(), "words": words, "audio_secs": audio_secs, "server_start_secs": start_secs,
    });
    let began = Instant::now();
    match task.as_str() {
        "report" => {
            let template =
                report::find_template(&option(&args, "--template").unwrap_or("summary".into()))
                    .expect("template");
            let mut speakers: Vec<String> = Vec::new();
            for l in &lines {
                if let Some(s) = &l.speaker {
                    if !speakers.contains(s) {
                        speakers.push(s.clone());
                    }
                }
            }
            let context = ReportContext::default();
            let input = Input {
                template,
                lines: &lines,
                markers: &[],
                context: &context,
                speakers: &speakers,
                language: None,
                source_language: "en",
                context_tokens: ctx as usize,
            };
            let g = generate(&timed, &input, &mut |_| true);
            out["wall_secs"] = json!(began.elapsed().as_secs_f64());
            match g {
                Ok(g) => {
                    let s = rubric::score(template, &g.content, &lines, &speakers, &[]);
                    out["template"] = json!(template.id);
                    out["chunks"] = json!(g.chunks);
                    out["model_calls"] = json!(g.model_calls);
                    out["dropped_uncited"] = json!(g.dropped_uncited);
                    out["reassigned_names"] = json!(g.reassigned_names);
                    out["rubric"] = json!({
                        "items": s.items, "items_with_valid_time": s.items_with_valid_time,
                        "items_missing_time": s.items_missing_time,
                        "speakers_expected": s.speakers_expected, "speakers_present": s.speakers_present,
                        "invented_numbers": s.invented_numbers, "invented_names": s.invented_names,
                        "floor_ok": s.floor_ok(),
                    });
                    if let Some(p) = option(&args, "--content-out") {
                        std::fs::write(p, serde_json::to_string_pretty(&g.content).unwrap()).unwrap();
                    }
                }
                Err(e) => out["error"] = json!(format!("{e:?}")),
            }
        }
        "translate" => {
            let target = option(&args, "--target").unwrap_or_else(|| "Vietnamese".into());
            let source: Vec<CaptionLine> = lines
                .iter()
                .map(|l| CaptionLine {
                    start: MediaTime::from_secs_f64(l.start).unwrap(),
                    end: MediaTime::from_secs_f64(l.end).unwrap(),
                    text: l.text.clone(),
                })
                .collect();
            let translator = ModelTranslator::new(&timed, model_id.clone(), "bench");
            let r = translate_with_text_model(&translator, &source, "English", &target, &[], &mut |_| true);
            out["wall_secs"] = json!(began.elapsed().as_secs_f64());
            match r {
                Ok(t) => {
                    out["translated_lines"] = json!(t.lines.len());
                    out["untranslated"] = json!(t.untranslated);
                }
                Err(e) => out["error"] = json!(format!("{e}")),
            }
        }
        other => panic!("unknown task {other}"),
    }
    let calls = timed.calls.lock().unwrap().clone();
    out["calls"] = json!(calls.len());
    out["streamed_pieces_total"] = json!(calls.iter().map(|c| c.1).sum::<usize>());
    out["call_secs"] = json!(calls.iter().map(|c| c.0).sum::<f64>());
    // The server's own totals (the warm-up request included: it is a few tokens).
    out["server"] = server_totals(&std::fs::read_to_string(&log).unwrap_or_default());
    engine.stop();
    println!("{out}");
}
