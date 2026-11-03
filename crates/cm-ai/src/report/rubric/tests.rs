use serde_json::json;

use super::*;
use crate::report::{find_template, Line};

fn line(start: f64, end: f64, who: &str, text: &str) -> Line {
    Line {
        start,
        end,
        speaker: Some(who.to_owned()),
        text: text.to_owned(),
    }
}

fn transcript() -> Vec<Line> {
    vec![
        line(
            0.0,
            6.0,
            "Alice",
            "The billing page is behind schedule by thirty-three days.",
        ),
        line(
            7.0,
            14.0,
            "Brandon",
            "We need twelve more people and Chloe will own the search index.",
        ),
        line(
            15.0,
            20.0,
            "Alice",
            "Ready on the twenty-eighth, about 40,000 dollars.",
        ),
    ]
}

fn minutes() -> &'static Template {
    find_template("minutes").unwrap()
}

#[test]
fn spelled_and_written_numbers_meet() {
    let n = numbers_in(
        "thirty-three days, 12 people, the twenty-eighth, two hundred and five, 40,000, 3.5",
    );
    for want in ["33", "12", "28", "205", "40000", "3.5"] {
        assert!(n.contains(want), "{want} in {n:?}");
    }
    assert!(numbers_in("fourth, sixth thirtieth")
        .is_superset(&["4", "6", "30"].map(String::from).into()));
}

#[test]
fn ordinary_words_are_not_stated_numbers() {
    assert!(numbers_stated("the second quarter, one of the first two").is_empty());
    let n = numbers_stated("twenty-one items, 7 people, two hundred");
    assert!(
        n.contains("21") && n.contains("7") && n.contains("200"),
        "{n:?}"
    );
}

#[test]
fn a_faithful_report_passes_the_floor() {
    let content = json!({
        "summary": [{"text": "Alice and Brandon discussed the billing page, 33 days late.", "at": [1.0]}],
        "decisions": [{"text": "Chloe will own the search index.", "at": [8.0]}],
        "action_items": [{"task": "Staff twelve more people", "owner": "Brandon", "at": [9.0, 30.0]}],
    });
    let s = score(minutes(), &content, &transcript(), &[], &[]);
    assert_eq!(s.items, 3);
    assert_eq!(s.items_with_valid_time, 3, "{s:?}");
    assert!(s.floor_ok(), "{}", s.summary());
}

#[test]
fn it_flags_a_missing_time_a_missing_speaker_and_invented_names_and_numbers() {
    let content = json!({
        "summary": [{"text": "Alice reviewed the billing page. Dmitri joined for 99 minutes.", "at": [500.0]}],
        "decisions": [{"text": "Ship on the 3rd with Elena", "at": [8.0]}],
    });
    let s = score(minutes(), &content, &transcript(), &[], &[]);
    assert_eq!(s.items, 2);
    assert_eq!(s.items_with_valid_time, 1);
    assert_eq!(s.items_missing_time, 1);
    assert_eq!(s.speakers_expected, 2);
    assert_eq!(s.speakers_present, 1);
    assert_eq!(s.speakers_missing, vec!["Brandon".to_owned()]);
    assert_eq!(s.invented_numbers, vec!["3".to_owned(), "99".to_owned()]);
    assert_eq!(s.invented_names, vec!["Elena".to_owned()]);
    assert!(!s.floor_ok());
}

#[test]
fn a_sentence_start_and_an_attendee_are_not_invented_names() {
    let content = json!({
        "summary": [{"text": "Overall the plan holds. Alice thanked Brandon, and Priya agreed.", "at": [1.0]}],
    });
    let s = score(
        minutes(),
        &content,
        &transcript(),
        &[],
        &["Priya Nair".to_owned()],
    );
    assert!(s.invented_names.is_empty(), "{s:?}");
}

#[test]
fn an_empty_report_passes_only_without_speakers() {
    let content = json!({});
    assert!(!score(minutes(), &content, &transcript(), &[], &[]).floor_ok());
    let no_speakers = vec![Line {
        start: 0.0,
        end: 5.0,
        speaker: None,
        text: "hello there".into(),
    }];
    assert!(score(minutes(), &content, &no_speakers, &[], &[]).floor_ok());
}
