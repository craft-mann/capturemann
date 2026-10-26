use super::*;
use serde_json::json;

fn parse_ok(method: &str, params: Value) -> AiParams {
    match Action::parse(method, &params).unwrap() {
        Action::Ai(p) => p,
        other => panic!("not an AI action: {other:?}"),
    }
}

fn parse_err(method: &str, params: Value) -> String {
    Action::parse(method, &params).unwrap_err().to_string()
}

/// Every method name parses to an AI action with the same method name.
#[test]
fn ai_methods_parse_to_themselves() {
    for (m, p) in [
        ("list_models", json!({})),
        (
            "download_model",
            json!({"model": "small", "request_id": "r"}),
        ),
        (
            "import_model",
            json!({"model": "small", "path": "/tmp/m.bin"}),
        ),
        ("select_model", json!({"model": "base"})),
        ("remove_model", json!({"model": "base"})),
        (
            "transcribe",
            json!({"capture_id": "rec_1", "language": "auto"}),
        ),
        ("get_transcript", json!({"capture_id": "rec_1"})),
        ("get_captions", json!({"capture_id": "rec_1"})),
        (
            "set_captions",
            json!({"capture_id": "rec_1", "lines": [{"start": 1, "end": 2.5, "text": "Hi"}]}),
        ),
        (
            "replace_in_captions",
            json!({"capture_id": "rec_1", "replacements": [{"find": "a", "replace": "b"}]}),
        ),
        (
            "export_captions",
            json!({"capture_id": "rec_1", "format": "srt"}),
        ),
        ("suggest_zooms", json!({"capture_id": "rec_1"})),
        ("suggest_cuts", json!({"capture_id": "rec_1"})),
        (
            "diarize",
            json!({"capture_id": "rec_1", "num_speakers": 2, "headset": true}),
        ),
        (
            "set_speakers",
            json!({"capture_id": "rec_1", "rename": {"2": "Linh"},
                   "merge": [{"from": 3, "into": "1"}],
                   "assign": [{"start": 1.0, "end": 2.5, "speaker": 2}]}),
        ),
    ] {
        assert_eq!(parse_ok(m, p).op.method(), m);
    }
    assert_eq!(AI_METHODS.len(), 16);
}

#[test]
fn language_and_model_ids_are_checked() {
    let p = parse_ok("transcribe", json!({"capture_id": "r", "language": "auto"}));
    assert!(matches!(p.op, AiOp::Transcribe { language: None, .. }));
    let p = parse_ok("transcribe", json!({"capture_id": "r", "language": "VI"}));
    assert!(matches!(p.op, AiOp::Transcribe { language: Some(l), .. } if l == "vi"));
    assert!(parse_err(
        "transcribe",
        json!({"capture_id": "r", "language": "english!"})
    )
    .starts_with("language"));
    assert!(parse_err("download_model", json!({"model": "../x"})).starts_with("model"));
    assert!(parse_err("download_model", json!({})).starts_with("model"));
    assert!(parse_err("list_models", json!({"x": 1})).starts_with('x'));
}

/// AI-1: caption lines keep to 2 rows of 42 characters and 7 s, in order, without overlap.
#[test]
fn ai_1_caption_lines_are_validated() {
    let ok = parse_ok(
        "set_captions",
        json!({"capture_id": "r", "lines": [
            {"start": 1, "end": 3, "text": "First line"},
            {"start": "t:480000", "end": 5.5, "text": "Two\nrows"}]}),
    );
    let AiOp::SetCaptions { lines, .. } = &ok.op else {
        panic!()
    };
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].start.ticks(), 480_000);
    let long = "x".repeat(43);
    for (lines, field) in [
        (json!([{"start": 2, "end": 1, "text": "a"}]), "lines[0].end"),
        (json!([{"start": 0, "end": 9, "text": "a"}]), "lines[0].end"),
        (json!([{"start": 0, "end": 1, "text": ""}]), "lines[0].text"),
        (
            json!([{"start": 0, "end": 1, "text": long}]),
            "lines[0].text",
        ),
        (
            json!([{"start": 0, "end": 1, "text": "a\nb\nc"}]),
            "lines[0].text",
        ),
        (
            json!([{"start": 0, "end": 2, "text": "a"}, {"start": 1, "end": 3, "text": "b"}]),
            "lines[1].start",
        ),
        (
            json!([{"start": 0, "end": 1, "text": "a", "x": 1}]),
            "lines[0].x",
        ),
        (json!([{"end": 1, "text": "a"}]), "lines[0].start"),
    ] {
        let e = parse_err("set_captions", json!({"capture_id": "r", "lines": lines}));
        assert!(e.starts_with(field), "{field}: {e}");
    }
    // An empty list clears the captions.
    assert!(matches!(
        parse_ok("set_captions", json!({"capture_id": "r", "lines": []})).op,
        AiOp::SetCaptions { lines, .. } if lines.is_empty()
    ));
}

/// The automatic transcription defaults on only with enough RAM; the user's choice wins.
#[test]
fn ai_1_auto_transcribe_default_depends_on_ram() {
    let none = Map::new();
    assert!(auto_transcribe_enabled(&none, Some(16)));
    assert!(auto_transcribe_enabled(&none, Some(64)));
    assert!(!auto_transcribe_enabled(&none, Some(8)));
    assert!(!auto_transcribe_enabled(&none, None));
    let mut on = Map::new();
    on.insert(SETTING_AUTO_TRANSCRIBE.into(), json!(true));
    assert!(auto_transcribe_enabled(&on, Some(8)));
    let mut off = Map::new();
    off.insert(SETTING_AUTO_TRANSCRIBE.into(), json!(false));
    assert!(!auto_transcribe_enabled(&off, Some(64)));
}

fn list(models: &[(&str, bool, bool)]) -> ModelList {
    ModelList {
        models: models
            .iter()
            .map(|(id, installed, active)| ModelInfo {
                id: (*id).to_owned(),
                label: (*id).to_owned(),
                size_mb: 100,
                ram_mb: 0,
                hint: String::new(),
                installed: *installed,
                active: *active,
                path: None,
            })
            .collect(),
        active: None,
        dir: "/models".to_owned(),
    }
}

#[test]
fn model_choice_prefers_the_active_one_and_says_no_model() {
    let l = list(&[
        ("base", true, false),
        ("small", true, true),
        ("medium", false, false),
    ]);
    assert_eq!(Inner::pick_model(&l, None).unwrap(), "small");
    assert_eq!(Inner::pick_model(&l, Some("base")).unwrap(), "base");
    let e = Inner::pick_model(&l, Some("medium")).unwrap_err();
    assert!(
        matches!(&e, DaemonError::WithData { data, .. } if data["reason"] == "model_not_installed")
    );
    let none = list(&[("base", false, false)]);
    let e = Inner::pick_model(&none, None).unwrap_err();
    assert!(matches!(&e, DaemonError::WithData { data, .. } if data["reason"] == "no_model"));
    let e = Inner::pick_model(&none, Some("base")).unwrap_err();
    assert!(matches!(&e, DaemonError::WithData { data, .. } if data["reason"] == "no_model"));
}
