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
            title: "speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models)",
            area: "ai",
            specs: &["AI-8"],
            tier: Tier::Full,
            needs: &[Need::Models],
            platforms: ALL,
            timeout_s: 420,
            run: speakers,
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
    // Speaker models can be installed through CM_SPEAKER_MODELS_DIR (--speaker-models) or be bundled; only the
    // speech models count here.
    let models: Vec<Value> = m
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|x| x.get("kind").and_then(Value::as_str) != Some("speaker"))
        .collect();
    check!(!models.is_empty(), "`ai models` lists no speech model");
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
    let speech = ctx.opts.speech_audio.clone();
    let (id, _) = match &speech {
        Some(f) => import_file(ctx, f)?,
        None => import_file(ctx, &clips.join("clip-h264.mp4"))?,
    };
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
    if speech.is_some() {
        // With real speech the transcript must hold words, and about as many as a person speaks (about 1 to 4
        // a second; the lower bound only rejects an empty or near-empty result, the upper an invented flood).
        let j = ctx.cli_ok(&["ai", "transcript", &id])?;
        let words = j
            .pointer("/transcript/words")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let secs = library_entry(ctx, &id)?
            .get("duration")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            / 120_000.0;
        ctx.note(format!(
            "transcript: {words} words over {secs:.1} s of speech"
        ));
        check!(
            words > 0,
            "the transcript of a speech recording has no words"
        );
        if secs > 5.0 {
            let per_s = words as f64 / secs;
            check!(
                (0.3..=6.0).contains(&per_s),
                "{words} words in {secs:.1} s of speech is not plausible ({per_s:.2} a second)"
            );
        }
    }
    Ok(())
}

/// Speaker labels on a two-voice recording (`--speech`, for example `win-make-speech.ps1`'s speech2.wav) with the
/// two speaker models in `--speaker-models`: two speakers are found when two are asked for, their turns are
/// stored, and words get a speaker after a transcription. Synthetic voices: no accuracy claim.
fn speakers(ctx: &mut Ctx) -> R {
    let Some(speech) = ctx.opts.speech_audio.clone() else {
        return skip("no --speech <two-voice recording> given");
    };
    // Without --speaker-models the models the package bundles (Resources/Models/speaker next to the programs)
    // must be found by the product itself: no CM_SPEAKER_MODELS_DIR is set then.
    let bundled = ctx.pkg.dir.join("Resources").join("Models").join("speaker");
    let bundled_onnx = std::fs::read_dir(&bundled)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "onnx"))
                .count()
        })
        .unwrap_or(0);
    let mut started = false;
    if ctx.opts.speaker_models.is_none() {
        if bundled_onnx < 2 {
            return skip(
                "no --speaker-models <dir> given and the package bundles no speaker models (Resources/Models/speaker)",
            );
        }
        ctx.start_daemon()?;
        started = true;
        let d = ctx.cli_text(&["diagnostics"])?;
        let text = d.out.stdout.clone();
        ctx.note(format!(
            "bundled speaker models: {bundled_onnx} .onnx files"
        ));
        check!(
            text.contains("bundled with this build"),
            "diagnostics does not report the bundled speaker models"
        );
    }
    // `speakers list` reads the transcript, so the recording is transcribed first (as a user does).
    let Some(model) = ctx.opts.whisper_model.clone() else {
        return skip("no --whisper-model given (`speakers list` reads the transcript)");
    };
    let model_id = ctx.opts.whisper_model_id.clone();
    if !started {
        ctx.start_daemon()?;
    }
    let m = model.to_string_lossy().into_owned();
    ctx.cli_ok(&["ai", "models", "import", &model_id, &m])?;
    ctx.cli_ok(&["ai", "models", "use", &model_id])?;
    let (id, _) = import_file(ctx, &speech)?;
    let t = ctx.cli_long(
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
    check!(
        t.ok(),
        "transcribe before the speakers failed: {}",
        crate::util::cut(&crate::util::one_line(&t.out.text()), 300)
    );
    let r = ctx.cli_long(
        &[
            "ai",
            "speakers",
            &id,
            "--num",
            "2",
            "--wait",
            "--timeout",
            "300",
        ],
        360,
    )?;
    ctx.note(format!(
        "speakers: {} {}",
        exit_text(r.code()),
        crate::util::cut(&crate::util::one_line(&r.out.text()), 200)
    ));
    check!(
        !is_crash_code(r.code()),
        "the speaker engine crashed: {}",
        exit_text(r.code())
    );
    if r.code() == Some(2) {
        return fail(format!(
            "the speaker models were not found or not accepted: {}",
            crate::util::cut(&crate::util::one_line(&r.out.text()), 300)
        ));
    }
    check!(
        r.ok(),
        "ai speakers failed: {}",
        crate::util::cut(&crate::util::one_line(&r.out.text()), 300)
    );
    let l = ctx.cli_ok(&["ai", "speakers", "list", &id])?;
    let n = l
        .get("speakers")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    ctx.note(format!("speakers listed: {n}"));
    check!(n == 2, "asked for 2 speakers, `speakers list` shows {n}");
    // After a transcription the words carry their speaker.
    let j = ctx.cli_ok(&["ai", "transcript", &id])?;
    let mut seen = std::collections::BTreeSet::new();
    for w in j
        .pointer("/transcript/words")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(sp) = w.get("speaker") {
            seen.insert(sp.to_string());
        }
    }
    ctx.note(format!("speakers on the words: {seen:?}"));
    check!(
        seen.len() == 2,
        "the transcript's words carry {} distinct speaker(s), expected 2",
        seen.len()
    );
    Ok(())
}
