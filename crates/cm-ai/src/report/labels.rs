//! The fixed words of a report in the report's own language (AI-9.AC11): the template name, the
//! section titles, the field labels, "Nothing found" and "Unassigned". They come from the UI
//! catalogues (`locales/<code>.po`, through `cm_i18n`) but the **report's** language code picks the
//! catalogue, not the language of the app: minutes in Vietnamese read in Vietnamese in an English
//! app. A code with no catalogue shows the English of the template files.
//!
//! The templates (`data/reports/*.json`), their ids and the stored `content` stay English; only
//! what is drawn or exported changes. The `*_in` functions always translate (the English literal
//! when the catalogue has no entry); [`localized`] keeps the template file's own wording for English.

use cm_i18n::{tr, tr_ctx, with_language, Lang};

use super::Section;

/// The catalogue language of a report language code (`vi`, `vi-VN`): the shipped language with that
/// base code, else English.
pub fn lang_of(code: &str) -> Lang {
    let base = code.split(['-', '_']).next().unwrap_or(code);
    Lang::parse(base).unwrap_or(Lang::En)
}

/// `raw` for a language with a catalogue; the template file's own `english` for English, so the
/// English report text stays exactly what it was.
pub fn localized(lang: Lang, english: &str, raw: String) -> String {
    if lang == Lang::En {
        english.to_owned()
    } else {
        raw
    }
}

/// The template's name (by id).
pub fn template_name_in(lang: Lang, id: &str, fallback: &str) -> String {
    with_language(lang, || match id {
        "summary" => tr_ctx!("report template", "Summary"),
        "minutes" => tr_ctx!("report template", "Meeting minutes"),
        "one_on_one" => tr_ctx!("report template", "One-on-one notes"),
        "interview" => tr_ctx!("report template", "Interview notes"),
        "product_recap" => tr_ctx!("report template", "Product recap"),
        "lecture" => tr_ctx!("report template", "Lecture notes"),
        _ => fallback.to_owned(),
    })
}

/// A section's title, from the English title of the template file.
pub fn section_title_in(lang: Lang, english: &str) -> String {
    with_language(lang, || match english {
        "Summary" => tr_ctx!("report section", "Summary"),
        "Attendees" => tr_ctx!("report section", "Attendees"),
        "Topics" => tr_ctx!("report section", "Topics"),
        "Decisions" => tr_ctx!("report section", "Decisions"),
        "Action items" => tr_ctx!("report section", "Action items"),
        "Open questions" => tr_ctx!("report section", "Open questions"),
        "Marked moments" => tr_ctx!("report section", "Marked moments"),
        "Updates" => tr_ctx!("report section", "Updates"),
        "Wins" => tr_ctx!("report section", "Wins"),
        "Concerns" => tr_ctx!("report section", "Concerns"),
        "Feedback given" => tr_ctx!("report section", "Feedback given"),
        "Feedback received" => tr_ctx!("report section", "Feedback received"),
        "Agreed actions" => tr_ctx!("report section", "Agreed actions"),
        "Follow-ups" => tr_ctx!("report section", "Follow-ups"),
        "Follow-ups for next time" => tr_ctx!("report section", "Follow-ups for next time"),
        "Role and candidate" => tr_ctx!("report section", "Role and candidate"),
        "Questions and answers" => tr_ctx!("report section", "Questions and answers"),
        "Evidence per competency" => tr_ctx!("report section", "Evidence per competency"),
        "Candidate's questions" | "Candidate\u{2019}s questions" => {
            tr_ctx!("report section", "Candidate\u{2019}s questions")
        }
        "Product and audience" => tr_ctx!("report section", "Product and audience"),
        "Features shown" => tr_ctx!("report section", "Features shown"),
        "Questions asked" => tr_ctx!("report section", "Questions asked"),
        "Concerns raised" => tr_ctx!("report section", "Concerns raised"),
        "Next steps" => tr_ctx!("report section", "Next steps"),
        "Key points" => tr_ctx!("report section", "Key points"),
        "Definitions" => tr_ctx!("report section", "Definitions"),
        "Examples" => tr_ctx!("report section", "Examples"),
        "Review questions" => tr_ctx!("report section", "Review questions"),
        _ => english.to_owned(),
    })
}

/// The title of the section with this id (the first template that has it), for a place that
/// knows the id only; `fallback` when no template has it.
pub fn section_title_by_id_in(lang: Lang, id: &str, fallback: &str) -> String {
    super::templates()
        .iter()
        .flat_map(|t| t.sections.iter())
        .find(|s| s.id == id)
        .map_or_else(|| fallback.to_owned(), |s| section_title_in(lang, &s.title))
}

/// A field's label, from the English label of the template file.
pub fn field_label_in(lang: Lang, english: &str) -> String {
    with_language(lang, || match english {
        "Role" => tr_ctx!("report field", "Role"),
        "Due" => tr_ctx!("report field", "Due"),
        "Key points" => tr_ctx!("report field", "Key points"),
        "Answer" => tr_ctx!("report field", "Answer"),
        "Quote" => tr_ctx!("report field", "Quote"),
        "What it shows" => tr_ctx!("report field", "What it shows"),
        "Competency" => tr_ctx!("report field", "Competency"),
        "Audience" => tr_ctx!("report field", "Audience"),
        "Product" => tr_ctx!("report field", "Product"),
        "Definition" => tr_ctx!("report field", "Definition"),
        "People" => tr_ctx!("report field", "People"),
        "Question" => tr_ctx!("report field", "Question"),
        "Topic" => tr_ctx!("report field", "Topic"),
        "Text" => tr_ctx!("report field", "Text"),
        "Term" => tr_ctx!("report field", "Term"),
        "Name" => tr_ctx!("report field", "Name"),
        "Task" => tr_ctx!("report field", "Task"),
        "Step" => tr_ctx!("report field", "Step"),
        "Owner" => tr_ctx!("report field", "Owner"),
        "Label" => tr_ctx!("report field", "Label"),
        _ => english.to_owned(),
    })
}

/// What an empty section says.
pub fn nothing_found_in(lang: Lang) -> String {
    with_language(lang, || tr!("Nothing found"))
}

/// The owner "Unassigned" as the report stores it.
pub fn unassigned_in(lang: Lang) -> String {
    with_language(lang, || tr_ctx!("report owner", "Unassigned"))
}

/// The words a rendering of one report uses, for one language.
#[derive(Debug, Clone, Copy)]
pub struct Words {
    pub lang: Lang,
}

impl Words {
    /// For a report's language code.
    pub fn for_code(code: &str) -> Words {
        Words {
            lang: lang_of(code),
        }
    }

    pub fn template_name(&self, id: &str, english: &str) -> String {
        localized(self.lang, english, template_name_in(self.lang, id, english))
    }

    pub fn section_title(&self, section: &Section) -> String {
        localized(
            self.lang,
            &section.title,
            section_title_in(self.lang, &section.title),
        )
    }

    pub fn field_label(&self, english: &str) -> String {
        localized(self.lang, english, field_label_in(self.lang, english))
    }

    pub fn nothing_found(&self) -> String {
        localized(self.lang, super::NOTHING_FOUND, nothing_found_in(self.lang))
    }

    /// `value` as shown: "Unassigned" in the language of the report, anything else as it is.
    pub fn owner(&self, value: &str) -> String {
        if value == super::check::UNASSIGNED {
            localized(self.lang, value, unassigned_in(self.lang))
        } else {
            value.to_owned()
        }
    }
}
