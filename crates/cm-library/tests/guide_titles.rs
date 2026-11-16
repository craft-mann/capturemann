#![allow(clippy::unwrap_used)]
//! `title_source` of a Click Guide step (optional, schema 1 unchanged): a drafted title says where
//! it came from, a title somebody wrote does not, and a draft can be replaced while recording.
mod common;
use cm_core::Actor;
use cm_library::guide::{ClickPoint, GuideBundle, StepEdit, TITLE_FROM_SCREEN, TITLE_GENERIC};
use common::*;
use std::fs;

fn click(x: f64) -> ClickPoint {
    ClickPoint {
        x,
        y: 20.0,
        button: "left".into(),
        count: 1,
        scale: 1.0,
    }
}

#[test]
fn a_drafted_title_says_where_it_came_from_and_an_edit_clears_it() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = GuideBundle::create(tmp.path(), "T", serde_json::json!({}), T0).unwrap();
    let a = g
        .add_click_step_titled(
            &png(120, 80, 1),
            click(10.0),
            "Click here",
            Some(TITLE_GENERIC),
            &Actor::User,
            T0,
        )
        .unwrap();
    assert_eq!(a.title_source.as_deref(), Some("generic"));
    // The text read arrives after the step was written.
    assert!(g
        .set_draft_title(&a.id, "Click \"Save\"", TITLE_FROM_SCREEN)
        .unwrap());
    assert_eq!(g.step(&a.id).unwrap().title, "Click \"Save\"");
    assert_eq!(g.click_of(&a.id).unwrap().count, 1);
    // It is in guide.json, and a guide without the field still opens (schema 1).
    let text = fs::read_to_string(g.dir().join("guide.json")).unwrap();
    assert!(text.contains("\"title_source\": \"screen_text\""), "{text}");
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    for s in v["steps"].as_array_mut().unwrap() {
        s.as_object_mut().unwrap().remove("title_source");
    }
    fs::write(g.dir().join("guide.json"), v.to_string()).unwrap();
    let mut g = GuideBundle::open(g.dir()).unwrap();
    assert_eq!(g.step(&a.id).unwrap().title_source, None);
    // A title nobody drafted is not replaced by a late text read.
    assert!(!g
        .set_draft_title(&a.id, "Click \"Other\"", TITLE_FROM_SCREEN)
        .unwrap());
    // A step with a draft: an edit that sends the same title back keeps it a draft; a changed
    // title is the person's.
    let b = g
        .add_click_step_titled(
            &png(120, 80, 2),
            click(30.0),
            "Click here",
            Some(TITLE_GENERIC),
            &Actor::User,
            T0,
        )
        .unwrap();
    let same = StepEdit {
        id: Some(b.id.clone()),
        title: Some("Click here".into()),
        ..StepEdit::default()
    };
    g.edit_steps(&[same], &Actor::User, T0).unwrap();
    assert_eq!(
        g.step(&b.id).unwrap().title_source.as_deref(),
        Some("generic")
    );
    g.set_step_text(&b.id, Some("Press Save"), None, &Actor::User, T0)
        .unwrap();
    assert_eq!(g.step(&b.id).unwrap().title_source, None);
    assert!(!g
        .set_draft_title(&b.id, "Click \"Late\"", TITLE_FROM_SCREEN)
        .unwrap());
}
