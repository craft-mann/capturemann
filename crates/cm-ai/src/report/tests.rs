use std::sync::Mutex;

use serde_json::{json, Value};

use super::generate::{generate, Input};
use super::*;
use crate::text_model::{JsonReply, JsonRequest, TextModel, TextModelError};

/// A model that answers from a closure and records every request.
struct Scripted {
    seen: Mutex<Vec<JsonRequest>>,
    answer: Box<dyn Fn(&JsonRequest) -> Value + Send + Sync>,
    /// The streamed token counts reported before each answer (none: does not stream).
    ticks: Vec<usize>,
}

impl Scripted {
    fn new(f: impl Fn(&JsonRequest) -> Value + Send + Sync + 'static) -> Scripted {
        Scripted {
            seen: Mutex::new(vec![]),
            answer: Box::new(f),
            ticks: vec![],
        }
    }

    fn streaming(mut self, ticks: &[usize]) -> Scripted {
        self.ticks = ticks.to_vec();
        self
    }
}

impl TextModel for Scripted {
    fn complete_json_streaming(
        &self,
        req: &JsonRequest,
        on_progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<JsonReply, TextModelError> {
        for t in &self.ticks {
            if !on_progress(*t) {
                return Err(TextModelError {
                    kind: crate::text_model::ErrorKind::Cancelled,
                    message: "cancelled".into(),
                });
            }
        }
        self.complete_json(req)
    }

    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        self.seen.lock().unwrap().push(req.clone());
        Ok(JsonReply {
            value: (self.answer)(req),
            attempts: 1,
        })
    }
}

fn line(a: f64, b: f64, who: Option<&str>, text: &str) -> Line {
    Line {
        start: a,
        end: b,
        speaker: who.map(str::to_owned),
        text: text.into(),
    }
}

fn meeting() -> Vec<Line> {
    (0..12)
        .map(|i| {
            let t = f64::from(i) * 10.0;
            line(
                t,
                t + 8.0,
                Some(if i % 2 == 0 { "Linh" } else { "Minh" }),
                "we should send the pricing sheet to the customer by Friday please",
            )
        })
        .collect()
}

fn input<'a>(
    t: &'a Template,
    lines: &'a [Line],
    ctx: &'a ReportContext,
    speakers: &'a [String],
) -> Input<'a> {
    Input {
        template: t,
        lines,
        markers: &[],
        context: ctx,
        speakers,
        language: None,
        source_language: "en",
        context_tokens: 8192,
    }
}

fn no_progress() -> impl FnMut(f32) -> bool {
    |_| true
}

#[test]
fn six_templates_with_closed_schemas() {
    let all = templates();
    assert_eq!(all.len(), 6);
    fn closed(v: &Value) {
        if let Some(o) = v.as_object() {
            if o.get("type") == Some(&json!("object")) {
                assert_eq!(o.get("additionalProperties"), Some(&json!(false)), "{v}");
            }
            o.values().for_each(closed);
        } else if let Some(a) = v.as_array() {
            a.iter().for_each(closed);
        }
    }
    for t in all {
        closed(&t.schema);
        assert!(t.version >= 1);
        assert!(find_template(&t.name).is_some(), "{}", t.name);
    }
    assert_eq!(find_template("meeting-minutes").unwrap().id, "minutes");
    assert!(find_template("nope").is_none());
}

/// AI-9.AC4: no score, rating, ranking or recommendation anywhere in the Interview schema.
#[test]
fn interview_schema_has_no_judging_field() {
    let t = find_template("interview").unwrap();
    assert!(Template::forbidden_judging_fields(&t.schema).is_empty());
    assert!(t.notes_only.as_deref().unwrap().starts_with("Notes only"));
    // The check itself catches the usual names.
    let bad = json!({"properties": {"overall_score": {}, "hire_recommendation": {}, "ranks": {}}});
    assert_eq!(Template::forbidden_judging_fields(&bad).len(), 3);
    let bad = json!({"required": ["rating"]});
    assert_eq!(Template::forbidden_judging_fields(&bad), vec!["rating"]);
}

#[test]
fn context_is_checked_against_the_template() {
    let t = find_template("minutes").unwrap();
    let c = ReportContext {
        title: Some("  Weekly ".into()),
        attendees: vec!["Linh".into(), " ".into()],
        ..Default::default()
    }
    .checked(t)
    .unwrap();
    assert_eq!(c.title.as_deref(), Some("Weekly"));
    assert_eq!(c.attendees, vec!["Linh"]);
    let e = ReportContext {
        role: Some("Engineer".into()),
        ..Default::default()
    }
    .checked(t)
    .unwrap_err();
    assert!(e.to_string().contains("context.role"), "{e}");
}

#[test]
fn too_little_speech() {
    let e = check_enough_speech(&[line(0.0, 1.0, None, "hello there")]).unwrap_err();
    assert_eq!(e.to_string(), TOO_LITTLE_SPEECH);
    assert!(check_enough_speech(&meeting()).is_ok());
}

fn minutes_reply(items_action: Value) -> Value {
    json!({
        "summary": [{"text": "They plan the pricing sheet.", "at": [0.0]}],
        "attendees": [{"name": "Linh", "at": [0.0]}],
        "topics": [{"topic": "Pricing", "key_points": ["send the sheet"], "at": [10.0]}],
        "decisions": [{"text": "Send by Friday", "at": [10.0, 20.5]}],
        "action_items": items_action,
        "open_questions": []
    })
}

#[test]
fn schema_success_with_markers_and_speakers() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let speakers = vec!["Linh".to_owned(), "Minh".to_owned()];
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| {
        minutes_reply(json!([
            {"task": "Send pricing", "owner": "Linh", "due": "Friday", "at": [10.0]},
            {"task": "Call legal", "owner": "Zaphod", "due": "", "at": [20.0]},
        ]))
    });
    let markers = [MarkerIn {
        at: 55.0,
        label: Some("pricing".into()),
    }];
    let mut inp = input(t, &lines, &ctx, &speakers);
    inp.markers = &markers;
    let g = generate(&model, &inp, &mut no_progress()).unwrap();
    assert_eq!(g.model_calls, 1);
    assert_eq!(g.dropped_uncited, 0);
    assert_eq!(g.reassigned_names, 1);
    let owners: Vec<&str> = g.content["action_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["owner"].as_str().unwrap())
        .collect();
    assert_eq!(owners, ["Linh", "Unassigned"]);
    assert_eq!(g.content["marked_moments"][0]["label"], "pricing");
    // The prompt carries the speaker names, the marker and the cited-time format.
    let seen = model.seen.lock().unwrap();
    let user = &seen[0].messages[0].content;
    assert!(user.contains("Speakers: Linh, Minh"), "{user}");
    assert!(user.contains("[55.0] pricing"), "{user}");
    assert!(user.contains("[10.0] Minh: we should"), "{user}");
}

/// AI-9.AC3: an invented action item with a time outside the transcript is dropped and counted.
#[test]
fn uncited_item_is_dropped_and_counted() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| {
        minutes_reply(json!([
            {"task": "Real", "owner": "Linh", "due": "", "at": [10.0]},
            {"task": "Invented", "owner": "Linh", "due": "", "at": [9999.0]},
            {"task": "Half", "owner": "Linh", "due": "", "at": [9999.0, 30.0]},
        ]))
    });
    let g = generate(&model, &input(t, &lines, &ctx, &[]), &mut no_progress()).unwrap();
    assert_eq!(g.dropped_uncited, 1);
    let items = g.content["action_items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[1]["at"], json!([30.0]));
}

#[test]
fn a_reply_that_breaks_the_schema_is_an_error() {
    let t = find_template("summary").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| json!({"summary": [], "key_points": [], "score": 5}));
    let e = generate(&model, &input(t, &lines, &ctx, &[]), &mut no_progress()).unwrap_err();
    assert!(matches!(e, generate::Error::Model(m) if m.reason() == "invalid_model_output"));
}

/// AI-9.AC7: the output language reaches the prompt and the report keeps the quotes' times.
#[test]
fn output_language_is_in_the_prompt() {
    let t = find_template("summary").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model =
        Scripted::new(|_| json!({"summary": [{"text": "Tom tat", "at": [0.0]}], "key_points": []}));
    let mut inp = input(t, &lines, &ctx, &[]);
    inp.language = Some("vi");
    generate(&model, &inp, &mut no_progress()).unwrap();
    let sys = model.seen.lock().unwrap()[0].system.clone().unwrap();
    assert!(
        sys.contains("Write all the note text in Vietnamese"),
        "{sys}"
    );
}

/// AI-9.AC5: a 3-hour transcript at 8,192 tokens goes through chunks and merge passes, and the
/// result cites the start and the end.
#[test]
fn three_hour_transcript_is_chunked_and_covers_the_timeline() {
    let t = find_template("minutes").unwrap();
    // 3 h, a 10 s line every 10 s, 25 words each.
    let lines: Vec<Line> = (0..1080)
        .map(|i| {
            let s = f64::from(i) * 10.0;
            line(
                s,
                s + 9.0,
                None,
                "this is a rather long sentence about the quarterly plan and the budget for the next \
                 team to review together",
            )
        })
        .collect();
    let ctx = ReportContext::default();
    let model = Scripted::new(|req| {
        let user = &req.messages[0].content;
        if user.contains("Notes of part") {
            // Merge: union of the decisions of every part.
            let mut decisions = vec![];
            for block in user.split("Notes of part ").skip(1) {
                let json_text = block.split_once(":\n").unwrap().1.trim();
                let v: Value = serde_json::from_str(json_text.lines().next().unwrap()).unwrap();
                decisions.extend(v["decisions"].as_array().unwrap().clone());
            }
            return json!({"summary": [], "attendees": [], "topics": [], "decisions": decisions,
                          "action_items": [], "open_questions": []});
        }
        let times: Vec<f64> = user
            .lines()
            .filter_map(|l| l.strip_prefix('['))
            .filter_map(|l| l.split_once(']'))
            .filter(|(_, rest)| rest.starts_with(' ') && rest.contains("quarterly"))
            .filter_map(|(n, _)| n.parse().ok())
            .collect();
        json!({"summary": [], "attendees": [], "topics": [],
               "decisions": [{"text": "first", "at": [times[0]]}, {"text": "last", "at": [times[times.len()-1]]}],
               "action_items": [], "open_questions": []})
    });
    let g = generate(&model, &input(t, &lines, &ctx, &[]), &mut no_progress()).unwrap();
    assert!(g.chunks > 10, "{}", g.chunks);
    assert!(g.model_calls > g.chunks, "merge passes ran");
    let seen = model.seen.lock().unwrap();
    // Nothing sent is longer than the context allows (3 characters per token).
    assert!(
        seen.iter().all(|r| r.chars() < 8192 * 3),
        "a prompt was too long"
    );
    let ats: Vec<f64> = g.content["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["at"][0].as_f64().unwrap())
        .collect();
    assert!(ats.iter().any(|t| *t < 1080.0), "the first tenth is cited");
    assert!(ats.iter().any(|t| *t > 9720.0), "the last tenth is cited");
    assert_eq!(g.dropped_uncited, 0);
}

#[test]
fn cancel_stops_the_run() {
    let t = find_template("summary").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| json!({"summary": [], "key_points": []}));
    let e = generate(&model, &input(t, &lines, &ctx, &[]), &mut |_| false).unwrap_err();
    assert_eq!(e, generate::Error::Cancelled);
    assert!(model.seen.lock().unwrap().is_empty());
}

/// One call (the usual meeting) moves the bar while the answer is written: values rise, stay
/// strictly between 0 and 1 until the end, 700 tokens is about half, and only the end is 1.0.
#[test]
fn one_call_reports_progress_from_the_streamed_tokens() {
    let t = find_template("summary").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| json!({"summary": [], "key_points": []}))
        .streaming(&[1, 100, 350, 700, 1400, 4000]);
    let mut seen = vec![];
    generate(&model, &input(t, &lines, &ctx, &[]), &mut |p| {
        seen.push(p);
        true
    })
    .unwrap();
    assert_eq!(seen.pop(), Some(1.0));
    assert!(seen.len() >= 7, "{seen:?}");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert!(
        seen.iter().skip(1).all(|p| *p > 0.0 && *p < 1.0),
        "{seen:?}"
    );
    // 700 tokens is about half of a single-call run (map share 0.95 of the bar).
    assert!((seen[4] - 0.95 / 2.0).abs() < 0.01, "{seen:?}");
}

/// A long recording: map chunks and merge rounds each use their slice, the bar never goes back
/// and stays below 1.0 until the end.
#[test]
fn chunks_and_merges_keep_the_bar_monotonic() {
    let t = find_template("minutes").unwrap();
    let lines: Vec<Line> = (0..600)
        .map(|i| {
            let s = f64::from(i) * 10.0;
            line(
                s,
                s + 9.0,
                None,
                "this is a rather long sentence about the quarterly plan and the budget for the next \
                 team to review together",
            )
        })
        .collect();
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| {
        json!({"summary": [], "attendees": [], "topics": [], "decisions": [],
               "action_items": [], "open_questions": []})
    })
    .streaming(&[10, 700, 3000]);
    let mut seen = vec![];
    let g = generate(&model, &input(t, &lines, &ctx, &[]), &mut |p| {
        seen.push(p);
        true
    })
    .unwrap();
    assert!(g.chunks > 2 && g.model_calls > g.chunks);
    assert_eq!(seen.pop(), Some(1.0));
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert!(seen.iter().all(|p| *p < 1.0), "{seen:?}");
}

#[test]
fn a_cancel_during_the_stream_ends_the_run() {
    let t = find_template("summary").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model =
        Scripted::new(|_| json!({"summary": [], "key_points": []})).streaming(&[1, 100, 350, 700]);
    let mut calls = 0;
    let e = generate(&model, &input(t, &lines, &ctx, &[]), &mut |_| {
        calls += 1;
        calls < 4
    })
    .unwrap_err();
    assert_eq!(e, generate::Error::Cancelled);
    assert_eq!(calls, 4, "stopped at the refusing callback");
}

fn sample_report(t: &Template) -> store::Report {
    let g = generate::Generated {
        content: json!({
            "summary": [{"text": "Pricing plan.", "at": [5.0]}],
            "attendees": [{"name": "Linh", "role": "", "at": []}],
            "topics": [{"topic": "Pricing", "key_points": ["sheet", "deadline"], "at": [10.0]}],
            "decisions": [],
            "action_items": [{"task": "Send sheet", "owner": "Linh", "due": "Friday", "at": [65.0, 3725.0]}],
            "open_questions": [],
            "marked_moments": [{"label": "Marker", "at": [55.0]}],
        }),
        dropped_uncited: 0,
        reassigned_names: 0,
        dropped_names: 0,
        chunks: 1,
        model_calls: 1,
    };
    store::new_report(
        "rpt_00000001".into(),
        1,
        t,
        "qwen3",
        "local",
        "2026-10-26T10:00:00Z",
        "en",
        ReportContext {
            title: Some("Weekly".into()),
            ..Default::default()
        },
        &g,
    )
}

#[test]
fn golden_markdown() {
    let t = find_template("minutes").unwrap();
    let md = render::markdown(t, &sample_report(t));
    let expected = "\
# Weekly

*Meeting minutes · 2026-10-26 · qwen3*

## Summary

Pricing plan. [00:05]

## Attendees

- Linh

## Topics

- **Topic:** Pricing [00:10]
  - **Key points:**
    - sheet
    - deadline

## Decisions

*Nothing found*

## Action items

- [ ] Send sheet — Owner: Linh · Due: Friday [01:05] [1:02:05]

## Open questions

*Nothing found*

## Marked moments

- Marker [00:55]
";
    assert_eq!(md, expected);
    let txt = render::plain_text(&md);
    assert!(txt.starts_with("WEEKLY\n"));
    assert!(!txt.contains("**") && !txt.contains("## "));
}

#[test]
fn ticked_tasks_render_as_checkboxes_in_markdown_and_text() {
    let t = find_template("minutes").unwrap();
    let mut r = sample_report(t);
    assert!(r.set_checked("action_items", 0, true));
    let md = render::markdown(t, &r);
    assert!(md.contains("- [x] Send sheet"), "{md}");
    // Only the task sections take boxes.
    assert!(
        md.contains("- Linh\n") && md.contains("- Marker [00:55]"),
        "{md}"
    );
    assert!(md.contains("- **Topic:** Pricing"), "{md}");
    let txt = render::plain_text(&md);
    assert!(txt.contains("[x] Send sheet"), "{txt}");
    r.set_checked("action_items", 0, false);
    let md = render::markdown(t, &r);
    assert!(md.contains("- [ ] Send sheet"), "{md}");
    assert!(render::plain_text(&md).contains("[ ] Send sheet"));
}

#[test]
fn interview_markdown_carries_the_notes_only_line() {
    let t = find_template("interview").unwrap();
    let mut r = sample_report(t);
    r.content = json!({});
    let md = render::markdown(t, &r);
    assert!(md.contains("> Notes only \u{2014} CaptureMann doesn't score candidates."));
}

#[test]
fn store_versions_make_undo_possible() {
    let t = find_template("minutes").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut r = sample_report(t);
    store::write(dir.path(), &r).unwrap();
    let e1 = store::entry_of(&r);
    r.v = 2;
    r.edited = true;
    r.markdown = Some("# My own text\n".into());
    store::write(dir.path(), &r).unwrap();
    let e2 = store::entry_of(&r);
    let live =
        |d: &std::path::Path| std::fs::read_to_string(store::live_md(d, "rpt_00000001")).unwrap();
    assert_eq!(live(dir.path()), "# My own text\n");
    // Undo: the index goes back to v1; sync restores the live files.
    store::sync_live(dir.path(), std::slice::from_ref(&e1));
    assert!(live(dir.path()).starts_with("# Weekly"));
    assert!(
        !store::read(dir.path(), std::slice::from_ref(&e1), "rpt_00000001")
            .unwrap()
            .edited
    );
    // Redo, then a delete (empty index) removes the live files but not the versions.
    store::sync_live(dir.path(), &[e2]);
    assert_eq!(live(dir.path()), "# My own text\n");
    store::sync_live(dir.path(), &[]);
    assert!(!store::live_json(dir.path(), "rpt_00000001").exists());
    assert!(store::version_path(dir.path(), "rpt_00000001", 1).exists());
    assert!(store::valid_id("rpt_0a1b2c3d") && !store::valid_id("../x"));
}
