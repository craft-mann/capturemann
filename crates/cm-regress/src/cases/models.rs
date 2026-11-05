//! Speech and text models: the no-model messages always, real transcription only with a model given.

use super::common::{
    import_file, library_entry, need_ffmpeg, wait_job, write_clips, wrong_platform_words,
};
use crate::check;
use crate::ctx::Ctx;
use crate::mcp::McpClient;
use crate::model::{fail, skip, Case, Need, Stop, Tier, ALL, R};
use crate::proc::{exit_text, is_crash_code};
use crate::sys;
use serde_json::{json, Value};

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "ai.no_model",
            title: "without a model, transcribe and the text model say what to do (exit 2) instead of failing",
            area: "ai",
            specs: &["AI-1", "AI-7"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: no_model,
        },
        Case {
            id: "ai.llama_version",
            title: "the bundled llama-server runs --version and is not started by the daemon",
            area: "ai",
            specs: &["AI-7", "bug class: llama-server missing in the Windows package"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 60,
            run: llama_version,
        },
        Case {
            id: "ai.transcribe",
            title: "a transcription with a real model runs to the end without a crash",
            area: "ai",
            specs: &["AI-1", "AI-3", "bug: Transcribe illegal instruction (AVX-512)"],
            tier: Tier::Full,
            needs: &[Need::Models],
            platforms: ALL,
            timeout_s: 900,
            run: transcribe,
        },
        Case {
            id: "ai.speakers",
            title: "speaker labels with the speaker models (not driven by this suite yet)",
            area: "ai",
            specs: &["AI-8"],
            tier: Tier::Full,
            needs: &[Need::Models],
            platforms: ALL,
            timeout_s: 60,
            run: speakers_stub,
        },
    ]
}

fn llama_version(ctx: &mut Ctx) -> R {
    if !ctx.pkg.has("llama-server") {
        return if sys::PLATFORM == "windows" && !ctx.opts.dev {
            fail("the package has no llama-server")
        } else {
            skip("this package has no llama-server")
        };
    }
    let mut c = ctx.product_cmd("llama-server");
    c.arg("--version");
    let out = ctx.run(c, 30)?;
    let text = crate::util::one_line(&out.text());
    ctx.note(format!(
        "llama-server --version: {} {}",
        exit_text(out.code),
        crate::util::cut(&text, 120)
    ));
    check!(
        out.success(),
        "llama-server --version ended with {}",
        exit_text(out.code)
    );
    check!(
        text.to_lowercase().contains("version"),
        "llama-server printed no version: {text}"
    );
    // The daemon must not start it on its own.
    ctx.start_daemon()?;
    let _ = ctx.cli_text(&["ai", "text-model"]);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let running = sys::list_processes()
        .iter()
        .any(|(_, n)| n.to_lowercase().starts_with("llama-server"));
    check!(
        !running,
        "llama-server runs although no text model was asked for"
    );
    Ok(())
}

fn no_model(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let m = ctx.cli_ok(&["ai", "models"])?;
    let models = m
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    check!(!models.is_empty(), "`ai models` lists no model");
    check!(
        models
            .iter()
            .all(|x| x.get("installed").and_then(Value::as_bool) == Some(false)),
        "a model is installed in a brand-new config folder"
    );
    let t = ctx.cli_ok(&["ai", "text-models"])?;
    check!(
        t.get("models")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty()),
        "`ai text-models` lists no model"
    );
    let (id, _) = import_file(ctx, &clips.join("clip-h264.mp4"))?;
    let r = ctx.cli(&["ai", "transcribe", &id])?;
    ctx.note(format!(
        "transcribe without a model: {} {:?} {}",
        exit_text(r.code()),
        r.error_code(),
        crate::util::cut(&r.error_message(), 160)
    ));
    check!(
        r.code() == Some(2) && r.error_reason().as_deref() == Some("no_model"),
        "expected exit 2 with reason no_model, got {} {:?} {:?}",
        exit_text(r.code()),
        r.error_code(),
        r.error_reason()
    );
    let wrong = wrong_platform_words(&r.error_message(), sys::PLATFORM);
    check!(
        wrong.is_empty(),
        "the message uses another platform's words {wrong:?}"
    );
    let tm = ctx.cli(&["ai", "text-model", "test"])?;
    ctx.note(format!(
        "text-model test: {} {:?}",
        exit_text(tm.code()),
        tm.error_code()
    ));
    check!(
        tm.code() == Some(2) && tm.error_reason().as_deref() == Some("no_text_model"),
        "expected exit 2 with reason no_text_model, got {} {:?} {:?}",
        exit_text(tm.code()),
        tm.error_code(),
        tm.error_reason()
    );
    // The same error has the same shape everywhere (maintainer decision Nov 5, 2026): code invalid_input,
    // data.reason no_model / no_text_model, a hint, through the CLI and through MCP, on every platform.
    check!(
        r.error_code().as_deref() == Some("invalid_input")
            && tm.error_code().as_deref() == Some("invalid_input"),
        "the shared code should be invalid_input, got {:?} and {:?}",
        r.error_code(),
        tm.error_code()
    );
    let hint = r
        .error
        .as_ref()
        .and_then(|e| e.pointer("/data/hint"))
        .and_then(Value::as_str);
    check!(
        hint.is_some_and(|h| h.contains("ai models")),
        "the no_model error has no usable hint: {hint:?}"
    );
    let mut mc = McpClient::spawn(ctx, &[])?;
    mc.initialize().map_err(Stop::Fail)?;
    let res = mc
        .call("transcribe", json!({"capture_id": id}))
        .map_err(Stop::Fail)?
        .map_err(|(c, m)| Stop::Fail(format!("JSON-RPC error {c}: {m}")))?;
    let s = res.structured.clone().unwrap_or(Value::Null);
    ctx.note(format!(
        "MCP transcribe without a model: error {} {}",
        res.is_error,
        crate::util::cut(&s.to_string(), 200)
    ));
    mc.close();
    check!(
        res.is_error,
        "MCP transcribe without a model did not report an error"
    );
    check!(
        s.get("code").and_then(Value::as_str) == Some("invalid_input")
            && s.pointer("/data/reason").and_then(Value::as_str) == Some("no_model"),
        "MCP error shape differs from the CLI's: {s}"
    );
    Ok(())
}

fn transcribe(ctx: &mut Ctx) -> R {
    need_ffmpeg(ctx)?;
    let Some(model) = ctx.opts.whisper_model.clone() else {
        return skip("no --whisper-model given");
    };
    let model_id = ctx.opts.whisper_model_id.clone();
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let m = model.to_string_lossy().into_owned();
    ctx.cli_ok(&["ai", "models", "import", &model_id, &m])?;
    ctx.cli_ok(&["ai", "models", "use", &model_id])?;
    let (id, _) = import_file(ctx, &clips.join("clip-h264.mp4"))?;
    let r = ctx.cli_long(
        &[
            "ai",
            "transcribe",
            &id,
            "--language",
            "en",
            "--wait",
            "--timeout",
            "600",
        ],
        700,
    )?;
    ctx.note(format!(
        "transcribe: {} {}",
        exit_text(r.code()),
        crate::util::cut(&crate::util::one_line(&r.out.text()), 200)
    ));
    check!(
        !is_crash_code(r.code()),
        "the transcription crashed: {}",
        exit_text(r.code())
    );
    check!(!r.out.timed_out, "the transcription hung");
    let job = r.json.clone().unwrap_or(Value::Null);
    let job_id = job
        .get("job_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if !job_id.is_empty() {
        let done = wait_job(ctx, &job_id, 60)?;
        check!(
            done.get("state").and_then(Value::as_str) == Some("done"),
            "the transcription job ended `{}`: {}",
            done.get("state").unwrap_or(&Value::Null),
            crate::util::cut(
                &done.get("error").map(Value::to_string).unwrap_or_default(),
                300
            )
        );
    } else {
        check!(
            r.ok(),
            "transcribe failed: {}",
            crate::util::cut(&r.out.text(), 300)
        );
    }
    let e = library_entry(ctx, &id)?;
    ctx.note(format!(
        "library entry after transcription: {}",
        crate::util::cut(&e.to_string(), 200)
    ));
    let t = ctx.cli(&["ai", "transcript", &id])?;
    check!(
        t.ok(),
        "ai transcript failed after a finished transcription: {}",
        exit_text(t.code())
    );
    Ok(())
}

fn speakers_stub(_ctx: &mut Ctx) -> R {
    Err(Stop::Skip("not driven yet: needs the two speaker models; the real-model check is cm-ai__real_diarize in the Windows tests artifact".to_owned()))
}
