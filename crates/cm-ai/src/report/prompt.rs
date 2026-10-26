//! The prompts of a report (AI-9): what the model is told, built from the template, the context
//! the user typed, the speaker names, the markers and the transcript.

use serde_json::Value;

use super::{language_name, Line, MarkerIn, ReportContext, Template, NOTHING_FOUND};

/// Seconds with one decimal, the way the transcript lines and the model's `at` values are written.
pub fn secs(t: f64) -> String {
    format!("{t:.1}")
}

/// The transcript as the model reads it: one line per caption line, `[seconds] Speaker: text`
/// (no speaker prefix for a recording without speaker labels). Rows of a two-row caption are
/// joined with a space.
pub fn transcript_text(lines: &[Line]) -> String {
    let mut out = String::new();
    for l in lines {
        let text = l.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            continue;
        }
        out.push('[');
        out.push_str(&secs(l.start));
        out.push_str("] ");
        if let Some(s) = &l.speaker {
            out.push_str(s);
            out.push_str(": ");
        }
        out.push_str(&text);
        out.push('\n');
    }
    out
}

/// The system prompt: the rules every template shares, the language, then the template's own
/// instructions.
pub fn system_prompt(template: &Template, language: Option<&str>, source_language: &str) -> String {
    let mut s = String::from(
        "You turn the transcript of a recorded conversation into structured notes. Answer with one JSON \
         object that matches the given schema and nothing else.\n\
         Rules:\n\
         1. Use only what the transcript says. Never invent facts, names, numbers, dates or decisions. \
            When a section has nothing to report, return an empty array for it.\n\
         2. Every item has `at`: one or more times in seconds, copied from the [seconds] marker at the \
            start of the transcript lines that show it. Never cite a time that is not on a line.\n\
         3. A person's name (an owner, an attendee) must be one of the attendees or speakers given, or a \
            name that is spoken in the transcript. When you do not know who owns something, write \
            \"Unassigned\".\n\
         4. Notes are written from the words of the transcript. Do not describe tone, emotion, age, \
            accent or personality.\n",
    );
    match language {
        Some(code) if !same_language(code, source_language) => {
            s.push_str(&format!(
                "5. Write all the note text in {}. Quotes stay in the language they were spoken in.\n",
                language_name(code)
            ));
        }
        _ => {
            s.push_str(
                "5. Write the note text in the language of the transcript. Quotes stay as spoken.\n",
            );
        }
    }
    s.push_str(&format!(
        "6. Keep each item short: one or two sentences.\n\nTemplate: {}.\n{}\n",
        template.name, template.instructions
    ));
    s
}

fn same_language(a: &str, b: &str) -> bool {
    a.split('-').next() == b.split('-').next()
}

/// The part of the user prompt before the transcript: context, speakers and markers.
pub fn preamble(
    template: &Template,
    context: &ReportContext,
    speakers: &[String],
    markers: &[MarkerIn],
) -> String {
    let mut s = String::new();
    if let Some(t) = &context.title {
        s.push_str(&format!("Title: {t}\n"));
    }
    if !context.attendees.is_empty() {
        s.push_str(&format!("Attendees: {}\n", context.attendees.join(", ")));
    }
    if let Some(a) = &context.agenda {
        s.push_str(&format!("Agenda: {a}\n"));
    }
    if let Some(r) = &context.role {
        s.push_str(&format!("Role being interviewed for: {r}\n"));
    }
    if !context.competencies.is_empty() {
        s.push_str(&format!(
            "Competencies to collect evidence for: {}\n",
            context.competencies.join("; ")
        ));
    }
    if !speakers.is_empty() {
        s.push_str(&format!("Speakers: {}\n", speakers.join(", ")));
    }
    if !markers.is_empty() {
        s.push_str("Moments the user marked (use them as hints for what matters):\n");
        for m in markers {
            match &m.label {
                Some(l) => s.push_str(&format!("[{}] {l}\n", secs(m.at))),
                None => s.push_str(&format!("[{}] (marker)\n", secs(m.at))),
            }
        }
    }
    if s.is_empty() {
        s.push_str("No further context was given.\n");
    }
    s.push_str(&format!(
        "\nWrite the \"{}\" notes. If a section has nothing, return an empty array (the report \
         shows \"{NOTHING_FOUND}\").\n",
        template.name
    ));
    s
}

/// The prompt for a whole recording or for one chunk of it (`part` is `(index, of)`, 1-based).
pub fn notes_prompt(preamble: &str, transcript: &str, part: Option<(usize, usize)>) -> String {
    let mut s = String::from(preamble);
    match part {
        Some((i, n)) => s.push_str(&format!(
            "\nThis is part {i} of {n} of a longer recording. Write notes for this part only; the \
             other parts are handled separately.\n"
        )),
        None => s.push('\n'),
    }
    s.push_str("Transcript (each line starts with its time in seconds):\n");
    s.push_str(transcript);
    s
}

/// The prompt that merges the notes of consecutive parts into one set of notes.
pub fn merge_prompt(preamble: &str, notes: &[Value]) -> String {
    let mut s = String::from(preamble);
    s.push_str(
        "\nBelow are the notes made from consecutive parts of one recording, as JSON. Merge them \
         into one set of notes with the same schema. Remove duplicates, join items about the same \
         thing (keep all their `at` times), keep the order of the recording and keep items from \
         every part: do not drop an item only to be short. Do not add anything that is not in the \
         notes. Keep the `at` times as they are.\n",
    );
    for (i, n) in notes.iter().enumerate() {
        s.push_str(&format!("\nNotes of part {}:\n{}\n", i + 1, n));
    }
    s
}
