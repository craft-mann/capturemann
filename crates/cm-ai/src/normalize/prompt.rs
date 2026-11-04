//! The prompts of the normalisation pass. General rules only: no word lists, no phrases taken
//! from a recording. The two examples are invented and exist to show the format.

/// Bumped when the prompt changes in a way that can change results; stored in the sidecar.
pub const PROMPT_VERSION: u32 = 1;

/// The language name for the prompt (the code itself for a language without one here).
pub fn language_name(code: &str) -> String {
    let base = code.split(['-', '_']).next().unwrap_or(code);
    match base {
        "vi" => "Vietnamese",
        "en" => "English",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "it" => "Italian",
        "nl" => "Dutch",
        "id" => "Indonesian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        "ru" => "Russian",
        other => return other.to_owned(),
    }
    .to_owned()
}

pub fn system() -> String {
    "You restore punctuation, capital letters and paragraph breaks in speech-to-text transcripts. \
     You never change the words. You answer with JSON only."
        .to_owned()
}

fn language_hints(code: &str) -> &'static str {
    match code.split(['-', '_']).next().unwrap_or(code) {
        "vi" => {
            "Vietnamese: write a capital letter on every syllable of a person's name and of a \
             place name, on the first word of a sentence, and on organisations and products. Keep \
             every tone mark and every \u{111} exactly as given."
        }
        "en" => {
            "English: capitalise the word I, the first word of a sentence, names of people, \
             places, organisations and products, days and months, and acronyms."
        }
        _ => {
            "Follow the capitalisation and punctuation conventions of this language; keep every \
             accent and diacritic exactly as given."
        }
    }
}

fn example(code: &str) -> &'static str {
    match code.split(['-', '_']).next().unwrap_or(code) {
        "vi" => {
            "Example. Input item: \"hôm nay chúng ta họp về kế hoạch quý ba anh nam sẽ báo cáo \
             trước rồi chị lan bổ sung nhé\" gives \"Hôm nay chúng ta họp về kế hoạch quý ba. Anh \
             Nam sẽ báo cáo trước, rồi chị Lan bổ sung nhé.\""
        }
        _ => {
            "Example. Input item: \"so today we are meeting about the third quarter plan mark \
             will report first and then anna can add anything\" gives \"So today we are meeting \
             about the third quarter plan. Mark will report first, and then Anna can add \
             anything.\""
        }
    }
}

/// The prompt for `items` (one raw turn each). `before` is the end of the turn before the first
/// item, for context only. `strict` adds the warning of the retry.
pub fn user(language: &str, items: &[String], before: &str, strict: Option<&str>) -> String {
    let mut p = format!(
        "Language: {}.\n\
         Each item in \"turns\" is the raw transcript of one speaker turn. Return the same items \
         with punctuation, capital letters and line breaks added.\n\
         Rules:\n\
         1. Keep every word exactly as given, in the same order, with the same spelling, accents \
         and tone marks. Never add, remove, replace, translate, correct, reorder, merge or split \
         words. Keep numbers as words if they are words.\n\
         2. You may add only: . , ? ! : ; and quotation marks, hyphens between words that are \
         already given, capital letters, and a line break (\\n) where a long turn changes topic. \
         Do not add line breaks to short turns.\n\
         3. {}\n\
         4. Return exactly {} items, in the same order, one per input item. Do not add notes.\n\
         {}\n",
        language_name(language),
        language_hints(language),
        items.len(),
        example(language),
    );
    if let Some(why) = strict {
        p.push_str(&format!(
            "WARNING: your previous answer was rejected because {why}. Copy every input word \
             exactly; if you are unsure where a mark belongs, leave it out.\n"
        ));
    }
    if !before.trim().is_empty() {
        p.push_str(&format!(
            "Text just before the first item (context only, do not output it): {}\n",
            serde_json::to_string(before.trim()).unwrap_or_default()
        ));
    }
    p.push_str(&format!(
        "Input: {}\nReply with the JSON object {{\"turns\": [...]}}.",
        serde_json::to_string(&serde_json::json!({ "turns": items })).unwrap_or_default()
    ));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_names_the_language_the_count_and_the_context() {
        let items = vec!["xin chào".to_owned(), "ừ".to_owned()];
        let p = user("vi", &items, "câu trước", None);
        assert!(p.contains("Language: Vietnamese."));
        assert!(p.contains("exactly 2 items"));
        assert!(p.contains("câu trước"));
        assert!(p.contains("\"turns\""));
        assert!(!p.contains("WARNING"));
        let p = user("en-US", &items[..1], "", Some("words were changed"));
        assert!(p.contains("Language: English."));
        assert!(p.contains("words were changed"));
        assert!(!p.contains("just before"));
    }

    #[test]
    fn unknown_languages_use_their_code() {
        assert_eq!(language_name("sw"), "sw");
        assert_eq!(language_name("pt-BR"), "Portuguese");
    }
}
