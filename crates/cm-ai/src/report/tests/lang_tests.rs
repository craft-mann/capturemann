//! AI-9.AC9 to AC11: the output language of a report (the prompt names it, a check by the same
//! model, headings and labels in the report's language). Fakes only.

use serde_json::{json, Value};

use super::*;
use crate::report::{labels, langcheck};

/// The notes of the `minutes` template with the given prose.
fn minutes_notes(summary: &str, topic: &str) -> Value {
    json!({
        "summary": [{"text": summary, "at": [0.0]}],
        "attendees": [],
        "topics": [{"topic": topic, "key_points": ["gui bang gia"], "at": [10.0]}],
        "decisions": [{"text": "chot gia", "at": [20.0]}],
        "action_items": [{"task": "gui bang gia", "owner": "Linh", "due": "", "at": [30.0]}],
        "open_questions": [],
    })
}

const VI_SUMMARY: &str = "Nhom bàn về bang gia";
const EN_SUMMARY: &str = "EN: The team talked about the price sheet";

/// The scripted language check: a part whose text carries the marker `EN:` is not in the language.
fn english_marker_check(req: &JsonRequest) -> Result<Value, TextModelError> {
    let user = &req.messages[0].content;
    let parts: Vec<Value> = user
        .split("\n### ")
        .skip(1)
        .map(|p| {
            let (id, body) = p.split_once('\n').unwrap_or((p, ""));
            json!({"id": id, "in_language": !body.contains("EN:")})
        })
        .collect();
    Ok(json!({ "parts": parts }))
}

fn is_rewrite(req: &JsonRequest) -> bool {
    req.schema_name.starts_with("report_minutes_")
}

#[test]
fn the_output_language_is_named_in_both_cases_and_repeated_after_the_transcript() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    // A small model that drifts to English unless the prompt names the language.
    let answer = |req: &JsonRequest| {
        let sys = req.system.clone().unwrap_or_default();
        let named = sys.contains("note text in Vietnamese");
        minutes_notes(if named { VI_SUMMARY } else { EN_SUMMARY }, "Gia ban")
    };
    // The transcript's language (no choice), a language that differs from the transcript's, and
    // one chosen that equals it.
    for (language, source) in [(None, "vi"), (Some("vi"), "en"), (Some("vi"), "vi")] {
        let model = Scripted::new(answer).with_check(english_marker_check);
        let mut inp = input(t, &lines, &ctx, &[]);
        inp.language = language;
        inp.source_language = source;
        let g = generate(&model, &inp, &mut no_progress()).unwrap();
        let seen = model.seen.lock().unwrap();
        let req = &seen[0];
        let sys = req.system.clone().unwrap();
        assert!(
            sys.contains("Write all the note text in Vietnamese"),
            "{sys}"
        );
        let user = &req.messages[0].content;
        let last_line = user.rfind("we should send").unwrap();
        let reminder = user
            .find("Reminder: write all the note text in Vietnamese")
            .unwrap();
        assert!(
            reminder > last_line,
            "the rule follows the transcript: {user}"
        );
        assert!(user
            .trim_end()
            .ends_with("Answer with the JSON object only."));
        assert_eq!(g.content["summary"][0]["text"], VI_SUMMARY);
        assert_eq!(g.language_unsure, 0);
    }
    // A transcript in English with no choice: English is named too.
    let model = Scripted::new(|_| minutes_notes("The plan", "Pricing"));
    generate(&model, &input(t, &lines, &ctx, &[]), &mut no_progress()).unwrap();
    let seen = model.seen.lock().unwrap();
    assert!(seen[0]
        .system
        .as_ref()
        .unwrap()
        .contains("Write all the note text in English"));
    // The merge pass of a long recording repeats the rule as well.
    assert!(
        prompt::merge_prompt("pre", &[json!({})], Some("Vietnamese"))
            .contains("Reminder: write all the note text in Vietnamese")
    );
}

#[test]
fn a_report_that_passes_costs_one_extra_short_call() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| minutes_notes(VI_SUMMARY, "Gia ban"));
    let mut inp = input(t, &lines, &ctx, &[]);
    inp.language = Some("vi");
    let g = generate(&model, &inp, &mut no_progress()).unwrap();
    assert_eq!(g.model_calls, 1);
    assert_eq!(
        g.language_calls, 1,
        "one batched question for every section"
    );
    assert_eq!(model.check_count(), 1);
    assert_eq!(model.seen.lock().unwrap().len(), 1);
    let checks = model.checks.lock().unwrap();
    let q = &checks[0].messages[0].content;
    for id in ["summary", "topics", "decisions", "action_items"] {
        assert!(q.contains(&format!("### {id}\n")), "{q}");
    }
    assert!(!q.contains("Linh"), "the owner is not prose: {q}");
    assert!(!q.contains("### attendees") && !q.contains("### open_questions"));
    assert!(checks[0].system.as_ref().unwrap().contains("Vietnamese"));
    assert!(checks[0].max_tokens <= 512);
    // Nothing to judge, nothing to ask.
    let empty = Scripted::new(|_| {
        json!({"summary": [], "attendees": [], "topics": [], "decisions": [],
               "action_items": [], "open_questions": []})
    });
    let g = generate(&empty, &inp, &mut no_progress()).unwrap();
    assert_eq!(g.language_calls, 0);
}

#[test]
fn prose_text_leaves_out_names_quotes_and_dates() {
    let t = find_template("interview").unwrap();
    let s = t.section("evidence").unwrap();
    let items = vec![json!({"competency": "Communication", "quote": "toi da lam",
                            "note": "gave an example", "at": [1.0]})];
    assert_eq!(langcheck::prose_text(s, &items), "gave an example\n");
    let m = find_template("minutes").unwrap();
    let a = m.section("action_items").unwrap();
    let items = vec![json!({"task": "Send it", "owner": "Linh", "due": "Friday", "at": [1.0]})];
    assert_eq!(langcheck::prose_text(a, &items), "Send it\n");
}

#[test]
fn an_english_section_of_a_vietnamese_report_is_written_again_once() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|req| {
        if is_rewrite(req) {
            return json!({"summary": [{"text": VI_SUMMARY, "at": [0.0]}]});
        }
        minutes_notes(EN_SUMMARY, "Gia ban")
    })
    .with_check(english_marker_check);
    let mut inp = input(t, &lines, &ctx, &[]);
    inp.language = Some("vi");
    inp.source_language = "vi";
    let g = generate(&model, &inp, &mut no_progress()).unwrap();
    assert_eq!(
        g.content["summary"][0]["text"], VI_SUMMARY,
        "the second version replaced it"
    );
    assert_eq!(
        g.content["summary"][0]["at"],
        json!([0.0]),
        "the citation is kept"
    );
    assert_eq!(g.content["topics"][0]["topic"], "Gia ban");
    assert_eq!(g.language_unsure, 0);
    assert_eq!(
        model.check_count(),
        2,
        "the question, then the one about the rewrite"
    );
    let seen = model.seen.lock().unwrap();
    assert_eq!(
        seen.len(),
        2,
        "the notes and one rewrite, only for the failing section"
    );
    let rewrite = &seen[1];
    assert!(rewrite.system.as_ref().unwrap().contains("correction"));
    let props = rewrite.schema["properties"].as_object().unwrap();
    assert_eq!(props.keys().collect::<Vec<_>>(), ["summary"]);
    assert_eq!(rewrite.schema["required"], json!(["summary"]));
    assert!(rewrite.messages[0]
        .content
        .contains("completely in Vietnamese"));
    assert!(rewrite.messages[0].content.contains(EN_SUMMARY));
    assert_eq!(g.model_calls, 1);
    assert_eq!(g.language_calls, 3);
}

#[test]
fn a_rewrite_that_is_still_wrong_is_not_used_and_is_counted() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|req| {
        if is_rewrite(req) {
            return json!({"summary": [{"text": "EN: still English", "at": [0.0]}]});
        }
        minutes_notes(EN_SUMMARY, "Gia ban")
    })
    .with_check(english_marker_check);
    let mut inp = input(t, &lines, &ctx, &[]);
    inp.language = Some("vi");
    let g = generate(&model, &inp, &mut no_progress()).unwrap();
    assert_eq!(
        g.content["summary"][0]["text"], EN_SUMMARY,
        "the first version stays"
    );
    assert_eq!(g.language_unsure, 1);
    assert_eq!(
        g.language_calls, 3,
        "a question, a rewrite, a question about it"
    );
    assert_eq!(
        model.seen.lock().unwrap().len(),
        2,
        "regenerated once, never again"
    );
}

#[test]
fn a_rewrite_with_another_number_of_items_is_not_used() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|req| {
        if is_rewrite(req) {
            return json!({"summary": [{"text": VI_SUMMARY, "at": [0.0]},
                                      {"text": VI_SUMMARY, "at": [10.0]}]});
        }
        minutes_notes(EN_SUMMARY, "Gia ban")
    })
    .with_check(english_marker_check);
    let mut inp = input(t, &lines, &ctx, &[]);
    inp.language = Some("vi");
    let g = generate(&model, &inp, &mut no_progress()).unwrap();
    assert_eq!(g.content["summary"][0]["text"], EN_SUMMARY);
    assert_eq!(g.language_unsure, 1);
    assert_eq!(
        model.check_count(),
        1,
        "no question about a rewrite that was not used"
    );
}

#[test]
fn a_language_check_with_no_answer_changes_nothing() {
    let t = find_template("minutes").unwrap();
    let lines = meeting();
    let ctx = ReportContext::default();
    let model = Scripted::new(|_| minutes_notes(EN_SUMMARY, "Gia ban")).with_check(|_| {
        Err(TextModelError {
            kind: crate::text_model::ErrorKind::Server,
            message: "busy".into(),
        })
    });
    let mut inp = input(t, &lines, &ctx, &[]);
    inp.language = Some("vi");
    let g = generate(&model, &inp, &mut no_progress()).unwrap();
    assert_eq!(g.language_unsure, 0);
    assert_eq!(g.content["summary"][0]["text"], EN_SUMMARY);
    assert_eq!(model.seen.lock().unwrap().len(), 1);
}

#[test]
fn language_unsure_is_stored_and_listed() {
    let t = find_template("minutes").unwrap();
    let g = generate::Generated {
        content: json!({}),
        dropped_uncited: 0,
        reassigned_names: 0,
        dropped_names: 0,
        language_unsure: 2,
        chunks: 1,
        model_calls: 1,
        language_calls: 3,
    };
    let r = store::new_report(
        "rpt_00000002".into(),
        1,
        t,
        "m",
        "embedded",
        "2026-11-17T10:00:00Z",
        "vi",
        ReportContext::default(),
        &g,
    );
    assert_eq!(r.language_unsure, 2);
    assert_eq!(r.summary_json()["language_unsure"], 2);
    let back: store::Report = serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
    assert_eq!(back.language_unsure, 2);
    // A report made before the check has no such field and reads as 0.
    let mut v = serde_json::to_value(&r).unwrap();
    v.as_object_mut().unwrap().remove("language_unsure");
    let old: store::Report = serde_json::from_value(v).unwrap();
    assert_eq!(old.language_unsure, 0);
}

fn vi_report(t: &Template) -> store::Report {
    let mut r = sample_report(t);
    r.language = "vi".into();
    r.content["action_items"][0]["owner"] = json!("Unassigned");
    r
}

#[test]
fn headings_and_labels_follow_the_report_language_not_the_app_language() {
    use cm_i18n::{with_language, Lang};
    let t = find_template("minutes").unwrap();
    let r = vi_report(t);
    let in_en_app = with_language(Lang::En, || render::markdown(t, &r));
    let in_vi_app = with_language(Lang::Vi, || render::markdown(t, &r));
    assert_eq!(in_en_app, in_vi_app, "the app's language does not matter");
    let vi = |s: &str| labels::section_title_in(Lang::Vi, s);
    for title in [
        "Summary",
        "Topics",
        "Decisions",
        "Action items",
        "Open questions",
    ] {
        assert_ne!(vi(title), title, "{title} has a Vietnamese entry");
        assert!(
            in_en_app.contains(&format!("## {}\n", vi(title))),
            "{in_en_app}"
        );
        assert!(!in_en_app.contains(&format!("## {title}\n")), "{in_en_app}");
    }
    assert!(!in_en_app.contains("*Meeting minutes"), "{in_en_app}");
    assert!(in_en_app.contains(&labels::template_name_in(Lang::Vi, "minutes", "")));
    assert!(in_en_app.contains(&format!("*{}*", labels::nothing_found_in(Lang::Vi))));
    assert!(!in_en_app.contains("Nothing found") && !in_en_app.contains("Owner:"));
    assert!(in_en_app.contains(&format!("{}: ", labels::field_label_in(Lang::Vi, "Owner"))));
    assert!(in_en_app.contains(&labels::unassigned_in(Lang::Vi)));
    assert!(!in_en_app.contains("Unassigned"));
    // The date, the model and the report's own text are not touched.
    assert!(in_en_app.contains("2026-10-26 \u{b7} qwen3"));
    assert!(in_en_app.contains("Pricing plan."));
    // The stored content is the same whatever is drawn.
    let before = r.content.clone();
    let _ = render::markdown(t, &r);
    assert_eq!(r.content, before);
    assert_eq!(r.content["action_items"][0]["owner"], "Unassigned");
}

#[test]
fn a_language_without_a_catalogue_renders_the_english_template() {
    let t = find_template("minutes").unwrap();
    let english = render::markdown(t, &sample_report(t));
    for code in ["xx", "es", "fr-CA", "", "en-GB"] {
        let mut r = sample_report(t);
        r.language = code.into();
        assert_eq!(render::markdown(t, &r), english, "{code}");
    }
    let mut r = sample_report(t);
    r.language = "vi-VN".into();
    assert_ne!(
        render::markdown(t, &r),
        english,
        "a region code picks the base language"
    );
}

/// Every title, label and template name of every built-in template has a Vietnamese entry.
#[test]
fn every_built_in_template_has_vietnamese_headings_and_labels() {
    use cm_i18n::Lang;
    let mut missing = Vec::new();
    for t in templates() {
        let en = labels::template_name_in(Lang::En, &t.id, &t.name);
        if labels::template_name_in(Lang::Vi, &t.id, &t.name) == en {
            missing.push(format!("template {}", t.id));
        }
        for s in &t.sections {
            if labels::section_title_in(Lang::Vi, &s.title) == s.title {
                missing.push(format!("{}: section {}", t.id, s.title));
            }
            for f in &s.fields {
                if labels::field_label_in(Lang::Vi, &f.label) == f.label {
                    missing.push(format!("{}: field {}", t.id, f.label));
                }
            }
        }
    }
    assert!(missing.is_empty(), "no Vietnamese entry: {missing:?}");
}
