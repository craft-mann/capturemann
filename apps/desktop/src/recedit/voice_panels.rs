//! The rules behind the Speakers and Report tabs (redesign V5 and V6, AI-8 and AI-9), display-free
//! and unit tested: talk-time shares, speaker names, which template a preset suggests, the report
//! context sent to `generate_report`, and a stored report turned into rows the panel draws (every
//! item with the times it cites). Nothing here sends a call; the panels do, through the daemon.

use cm_ai::report::store::Report;
use cm_ai::report::{find_template, FieldKind, Layout, Person, Source, Template};
use cm_ai::speakers::{Segment, Speaker};
use cm_core::MediaTime;
use serde_json::{json, Value};

// ---- speakers ---------------------------------------------------------------------------

/// Each speaker's share of the talk time, as a fraction of the time anyone spoke (all zero when
/// nobody did). The order is the speakers' order.
pub fn talk_shares(speakers: &[Speaker], segments: &[Segment]) -> Vec<(u32, f32)> {
    let total: i64 = segments
        .iter()
        .map(|s| (s.end.ticks() - s.start.ticks()).max(0))
        .sum();
    speakers
        .iter()
        .map(|sp| {
            let mine: i64 = segments
                .iter()
                .filter(|s| s.speaker == sp.id)
                .map(|s| (s.end.ticks() - s.start.ticks()).max(0))
                .sum();
            let f = if total > 0 {
                mine as f64 / total as f64
            } else {
                0.0
            };
            (sp.id, f as f32)
        })
        .collect()
}

/// A share as the whole percent the row shows (never "0%" for a speaker who did speak).
pub fn percent(fraction: f32) -> u32 {
    if fraction <= 0.0 {
        0
    } else {
        ((fraction * 100.0).round() as u32).max(1)
    }
}

/// Why a typed speaker name is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    /// Another speaker already has it.
    Taken,
}

/// The longest speaker name `set_speakers` takes.
pub const MAX_NAME_CHARS: usize = 60;

/// The name to send for speaker `id`: trimmed, 1 to 60 characters, not another speaker's.
/// `Ok(None)` when it is the name it already has (nothing to send).
pub fn checked_name(
    typed: &str,
    id: u32,
    speakers: &[Speaker],
) -> Result<Option<String>, NameError> {
    let name = typed.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(NameError::TooLong);
    }
    if speakers
        .iter()
        .any(|s| s.id != id && s.name.trim().eq_ignore_ascii_case(name))
    {
        return Err(NameError::Taken);
    }
    if speakers.iter().any(|s| s.id == id && s.name == name) {
        return Ok(None);
    }
    Ok(Some(name.to_owned()))
}

/// The speakers `from` can be merged into: everyone else.
pub fn merge_targets(speakers: &[Speaker], from: u32) -> Vec<&Speaker> {
    speakers.iter().filter(|s| s.id != from).collect()
}

/// "How many people?" answers: `None` is "Not sure".
pub const PEOPLE_CHOICES: [Option<u32>; 5] = [None, Some(2), Some(3), Some(4), Some(5)];

// ---- report templates ---------------------------------------------------------------------

/// The template a recording preset suggests (the board marks it "suggested"). The preset keys are
/// the `start_recording` ones; the templates list theirs as use cases.
pub fn suggested_template(preset: Option<&str>) -> Option<&'static str> {
    let case = match preset? {
        "room" => "in_the_room",
        "online" => "online_meeting",
        "lecture" => "lecture",
        "product" => "product_demo",
        "note" => "voice_note",
        _ => return None,
    };
    cm_ai::report::templates()
        .iter()
        .find(|t| t.suggested_for.iter().any(|s| s == case))
        .map(|t| t.id.as_str())
}

/// What the user typed in the setup form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub title: String,
    pub attendees: Vec<String>,
    pub agenda: String,
    pub role: String,
    pub competencies: Vec<String>,
}

/// The `context` of `generate_report`: only the fields the template lists, only the ones filled.
pub fn context_json(template: &Template, d: &Draft) -> Value {
    let mut o = serde_json::Map::new();
    let wants = |f: &str| template.context_fields.iter().any(|x| x == f);
    let text = |v: &str| {
        let t = v.trim();
        (!t.is_empty()).then(|| json!(t))
    };
    if wants("title") {
        if let Some(v) = text(&d.title) {
            o.insert("title".into(), v);
        }
    }
    if wants("attendees") && !d.attendees.is_empty() {
        o.insert("attendees".into(), json!(d.attendees));
    }
    if wants("agenda") {
        if let Some(v) = text(&d.agenda) {
            o.insert("agenda".into(), v);
        }
    }
    if wants("role") {
        if let Some(v) = text(&d.role) {
            o.insert("role".into(), v);
        }
    }
    if wants("competencies") && !d.competencies.is_empty() {
        o.insert("competencies".into(), json!(d.competencies));
    }
    Value::Object(o)
}

/// Adds a chip (an attendee, a competency): trimmed, not empty, not a duplicate, within `max`.
/// Returns whether it was added.
pub fn add_chip(chips: &mut Vec<String>, typed: &str, max: usize, max_chars: usize) -> bool {
    let t = typed.trim();
    if t.is_empty()
        || t.chars().count() > max_chars
        || chips.len() >= max
        || chips.iter().any(|c| c.eq_ignore_ascii_case(t))
    {
        return false;
    }
    chips.push(t.to_owned());
    true
}

// ---- a stored report as rows ----------------------------------------------------------------

/// One item of a section.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemView {
    /// The first field (the task, the decision, the topic).
    pub head: String,
    /// The other fields that have text (id, label, value), a list joined with `; `.
    pub extra: Vec<(String, String, String)>,
    /// The owner or attendee name when the section has one (a person field), `None` for the
    /// section's other items. "Unassigned" is kept as the daemon wrote it.
    pub person: Option<String>,
    /// The seconds the item cites.
    pub at: Vec<f64>,
}

/// One section of a report.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionView {
    pub id: String,
    pub title: String,
    pub layout: Layout,
    /// Built from the recording's markers (not written by the model).
    pub from_markers: bool,
    pub items: Vec<ItemView>,
}

fn text_of(item: &Value, id: &str) -> String {
    item.get(id)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .replace('\n', " ")
}

fn list_of(item: &Value, id: &str) -> Vec<String> {
    item.get(id)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The sections of `r` in the template's order. A section with no items is kept (with no items):
/// the panel says "Nothing found" for it. A template this build does not know gives no sections.
pub fn sections(r: &Report) -> Vec<SectionView> {
    let Some(t) = find_template(&r.template) else {
        return Vec::new();
    };
    t.sections
        .iter()
        .map(|s| {
            let items = r
                .content
                .get(&s.id)
                .and_then(Value::as_array)
                .map(|a| a.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter_map(|item| {
                    let first = s.fields.first()?;
                    let field_text = |f: &cm_ai::report::Field| match f.kind {
                        FieldKind::Text => text_of(item, &f.id),
                        FieldKind::List => list_of(item, &f.id).join("; "),
                    };
                    let person = s
                        .fields
                        .iter()
                        .find(|f| f.person.is_some())
                        .map(|f| text_of(item, &f.id))
                        .filter(|p| !p.is_empty());
                    let is_person = |f: &cm_ai::report::Field| f.person.is_some();
                    // A person field is the owner chip, not text.
                    let head_field = if is_person(first) {
                        s.fields.iter().find(|f| !is_person(f)).unwrap_or(first)
                    } else {
                        first
                    };
                    let head = field_text(head_field);
                    let extra = s
                        .fields
                        .iter()
                        .filter(|f| f.id != head_field.id && !is_person(f))
                        .filter_map(|f| {
                            let v = field_text(f);
                            (!v.is_empty()).then(|| (f.id.clone(), f.label.clone(), v))
                        })
                        .collect();
                    let at = item
                        .get("at")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_f64).collect())
                        .unwrap_or_default();
                    Some(ItemView {
                        head,
                        extra,
                        person,
                        at,
                    })
                })
                .collect();
            SectionView {
                id: s.id.clone(),
                title: s.title.clone(),
                layout: s.layout,
                from_markers: s.source == Source::Markers,
                items,
            }
        })
        .collect()
}

/// Whether a person field of the section is an owner (so "Unassigned" can happen).
pub fn has_owner(template_id: &str, section_id: &str) -> bool {
    find_template(template_id)
        .and_then(|t| t.section(section_id))
        .is_some_and(|s| s.fields.iter().any(|f| f.person == Some(Person::Owner)))
}

/// The time a citation seeks to.
pub fn at_time(secs: f64) -> MediaTime {
    MediaTime::from_secs_f64(secs.max(0.0)).unwrap_or(MediaTime::ZERO)
}

/// `m:ss` (or `h:mm:ss`) of a citation, as the Markdown prints it.
pub fn at_label(secs: f64) -> String {
    cm_ai::report::render::stamp(secs)
}

/// The date part of the report's `created` stamp (`2026-10-26`).
pub fn created_day(created: &str) -> String {
    created.split('T').next().unwrap_or(created).to_owned()
}

/// The number of items every section holds (a report with none is "nothing found").
pub fn item_count(sections: &[SectionView]) -> usize {
    sections.iter().map(|s| s.items.len()).sum()
}

// ---- the left column of the report view (V6) ----------------------------------------------

/// What kind of item a citation tick belongs to (the colour of the tick and its legend entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickKind {
    Decision,
    Action,
    Question,
    Marked,
    Other,
}

/// The kind of a section by its id.
pub fn tick_kind(section_id: &str) -> TickKind {
    match section_id {
        "decisions" => TickKind::Decision,
        s if cm_ai::report::is_checkable(s) => TickKind::Action,
        "open_questions" | "questions_asked" | "review_questions" => TickKind::Question,
        "marked_moments" => TickKind::Marked,
        _ => TickKind::Other,
    }
}

/// Every citation of a report as (seconds, kind), by time. Marked moments are included: they are
/// the recording's own markers.
pub fn citation_ticks(sections: &[SectionView]) -> Vec<(f64, TickKind)> {
    let mut v: Vec<(f64, TickKind)> = sections
        .iter()
        .flat_map(|s| {
            let k = tick_kind(&s.id);
            s.items
                .iter()
                .flat_map(move |i| i.at.iter().map(move |a| (*a, k)))
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

/// The caption lines around a cited moment: the line that holds it (or the one that starts
/// nearest after it, within a second of tolerance either way) and its neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CitedContext {
    pub before: Option<usize>,
    pub hit: usize,
    pub after: Option<usize>,
}

/// Finds [`CitedContext`] of `at` seconds in `lines` (`(start, end)` in seconds, by time).
/// `None` when there are no lines or the moment is more than a second outside all of them.
pub fn cited_context(lines: &[(f64, f64)], at: f64) -> Option<CitedContext> {
    const TOL: f64 = cm_ai::report::CITE_TOLERANCE_S;
    let hit = lines
        .iter()
        .position(|(a, b)| at >= *a && at < *b)
        .or_else(|| {
            lines
                .iter()
                .enumerate()
                .filter(|(_, (a, b))| at >= a - TOL && at <= b + TOL)
                .min_by(|x, y| {
                    let d = |l: &(f64, f64)| (l.0 - at).abs().min((l.1 - at).abs());
                    d(x.1).total_cmp(&d(y.1))
                })
                .map(|(i, _)| i)
        })?;
    Some(CitedContext {
        before: hit.checked_sub(1),
        hit,
        after: (hit + 1 < lines.len()).then_some(hit + 1),
    })
}

/// The 1-based place of item `index` in its section, for "Decisions \u{b7} 1".
pub fn cite_number(index: usize) -> usize {
    index + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_ai::report::ReportContext;

    fn sp(id: u32, name: &str) -> Speaker {
        Speaker {
            id,
            name: name.into(),
        }
    }

    fn seg(a: i64, b: i64, speaker: u32) -> Segment {
        Segment {
            start: MediaTime::from_ticks(a),
            end: MediaTime::from_ticks(b),
            speaker,
            energy: 0.0,
        }
    }

    #[test]
    fn shares_add_up_and_a_silent_recording_is_all_zero() {
        let speakers = [sp(1, "A"), sp(2, "B")];
        let s = talk_shares(&speakers, &[seg(0, 300, 1), seg(300, 400, 2)]);
        assert_eq!(s[0], (1, 0.75));
        assert_eq!(s[1], (2, 0.25));
        assert!(talk_shares(&speakers, &[]).iter().all(|(_, f)| *f == 0.0));
        assert_eq!(percent(0.004), 1, "a speaker who spoke is never 0%");
        assert_eq!(percent(0.0), 0);
        assert_eq!(percent(0.345), 35);
    }

    #[test]
    fn names_are_trimmed_unique_and_bounded() {
        let speakers = [sp(1, "Trung"), sp(2, "Speaker 2")];
        assert_eq!(
            checked_name("  Linh ", 2, &speakers),
            Ok(Some("Linh".into()))
        );
        assert_eq!(checked_name("Trung", 1, &speakers), Ok(None));
        assert_eq!(checked_name("trung", 2, &speakers), Err(NameError::Taken));
        assert_eq!(checked_name("   ", 2, &speakers), Err(NameError::Empty));
        assert_eq!(
            checked_name(&"x".repeat(61), 2, &speakers),
            Err(NameError::TooLong)
        );
        assert_eq!(merge_targets(&speakers, 2).len(), 1);
    }

    #[test]
    fn presets_suggest_their_template() {
        assert_eq!(suggested_template(Some("room")), Some("minutes"));
        assert_eq!(suggested_template(Some("online")), Some("minutes"));
        assert_eq!(suggested_template(Some("lecture")), Some("lecture"));
        assert_eq!(suggested_template(Some("product")), Some("product_recap"));
        assert_eq!(suggested_template(Some("note")), Some("summary"));
        assert_eq!(suggested_template(Some("other")), None);
        assert_eq!(suggested_template(None), None);
    }

    #[test]
    fn the_context_has_only_the_fields_the_template_lists() {
        let d = Draft {
            title: " Sync ".into(),
            attendees: vec!["Trung".into()],
            agenda: "".into(),
            role: "Designer".into(),
            competencies: vec!["Research".into()],
        };
        let minutes = find_template("minutes").unwrap();
        assert_eq!(
            context_json(minutes, &d),
            json!({"title": "Sync", "attendees": ["Trung"]})
        );
        let interview = find_template("interview").unwrap();
        let c = context_json(interview, &d);
        assert_eq!(c["role"], "Designer");
        assert_eq!(c["competencies"], json!(["Research"]));
        let summary = find_template("summary").unwrap();
        assert_eq!(context_json(summary, &d), json!({"title": "Sync"}));
    }

    #[test]
    fn chips_are_trimmed_unique_and_capped() {
        let mut v = Vec::new();
        assert!(add_chip(&mut v, " Linh ", 2, 80));
        assert!(!add_chip(&mut v, "linh", 2, 80));
        assert!(!add_chip(&mut v, "  ", 2, 80));
        assert!(add_chip(&mut v, "Minh", 2, 80));
        assert!(!add_chip(&mut v, "Trung", 2, 80), "over the cap");
        assert_eq!(v, ["Linh", "Minh"]);
    }

    fn report(content: Value) -> Report {
        Report {
            version: 1,
            id: "rpt_00000001".into(),
            v: 1,
            template: "minutes".into(),
            template_name: "Meeting minutes".into(),
            template_version: 1,
            model: "m".into(),
            provider: "local".into(),
            created: "2026-10-26T10:51:00Z".into(),
            language: "en".into(),
            context: ReportContext::default(),
            content,
            dropped_uncited: 2,
            reassigned_names: 0,
            chunks: 1,
            edited: false,
            checked: Vec::new(),
            markdown: None,
        }
    }

    #[test]
    fn a_report_becomes_sections_with_owners_and_citations() {
        let r = report(json!({
            "summary": [{"text": "All agreed.", "at": [12.0]}],
            "decisions": [{"text": "Ship Friday", "at": [1105.5, 1200.0]}],
            "action_items": [
                {"task": "Draft the post", "owner": "Linh", "due": "Thu", "at": [1117.0]},
                {"task": "Post the role", "owner": "Unassigned", "due": "", "at": [1960.0]}
            ]
        }));
        let s = sections(&r);
        assert_eq!(s.len(), 7, "every section of the template, empty or not");
        assert_eq!(s[0].items[0].head, "All agreed.");
        let actions = s.iter().find(|x| x.id == "action_items").unwrap();
        assert_eq!(actions.items[0].head, "Draft the post");
        assert_eq!(actions.items[0].person.as_deref(), Some("Linh"));
        assert_eq!(
            actions.items[0].extra,
            [("due".to_owned(), "Due".to_owned(), "Thu".to_owned())]
        );
        assert!(
            actions.items[1].extra.is_empty(),
            "an empty due is not shown"
        );
        assert_eq!(actions.items[1].person.as_deref(), Some("Unassigned"));
        let decisions = s.iter().find(|x| x.id == "decisions").unwrap();
        assert_eq!(decisions.items[0].at, [1105.5, 1200.0]);
        assert!(s
            .iter()
            .find(|x| x.id == "topics")
            .unwrap()
            .items
            .is_empty());
        assert!(
            s.iter()
                .find(|x| x.id == "marked_moments")
                .unwrap()
                .from_markers
        );
        assert_eq!(item_count(&s), 4);
        assert!(has_owner("minutes", "action_items"));
        assert!(!has_owner("minutes", "decisions"));
        assert_eq!(created_day(&r.created), "2026-10-26");
        assert_eq!(at_label(1105.5), "18:25");
        assert_eq!(at_time(1.0), MediaTime::from_ticks(120_000));
    }

    #[test]
    fn an_unknown_template_gives_no_sections() {
        let mut r = report(json!({}));
        r.template = "nope".into();
        assert!(sections(&r).is_empty());
    }

    #[test]
    fn ticks_are_sorted_and_coloured_by_section() {
        let r = report(json!({
            "decisions": [{"text": "Go.", "at": [1200.0]}],
            "action_items": [{"task": "Draft", "owner": "Linh", "at": [1117.0]}],
        }));
        let t = citation_ticks(&sections(&r));
        assert_eq!(
            t,
            [(1117.0, TickKind::Action), (1200.0, TickKind::Decision)]
        );
        assert_eq!(tick_kind("open_questions"), TickKind::Question);
        assert_eq!(tick_kind("marked_moments"), TickKind::Marked);
        assert_eq!(tick_kind("topics"), TickKind::Other);
    }

    #[test]
    fn a_cited_moment_finds_its_line_and_neighbours() {
        let lines = [(0.0, 5.0), (5.0, 10.0), (10.0, 15.0)];
        let c = cited_context(&lines, 7.0).unwrap();
        assert_eq!((c.before, c.hit, c.after), (Some(0), 1, Some(2)));
        let c = cited_context(&lines, 0.5).unwrap();
        assert_eq!((c.before, c.hit, c.after), (None, 0, Some(1)));
        let c = cited_context(&lines, 15.5).unwrap();
        assert_eq!((c.before, c.hit, c.after), (Some(1), 2, None));
        assert_eq!(cited_context(&lines, 20.0), None);
        assert_eq!(cited_context(&[], 1.0), None);
        // A gap: the nearer line within a second wins.
        let gap = [(0.0, 5.0), (5.8, 10.0)];
        assert_eq!(cited_context(&gap, 5.5).unwrap().hit, 1);
        assert_eq!(cited_context(&gap, 5.1).unwrap().hit, 0);
    }
}
