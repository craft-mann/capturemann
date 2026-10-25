//! The glossary of the AI page (AI-6a), display-free: the list of names and terms the user wants
//! spelled their way, and the rules of adding, editing and removing an entry. The list is the
//! app-only setting `ai.glossary` (`cm_ai::glossary`); the daemon uses it to prime Whisper and to
//! correct the words of every new transcript. Nothing here touches the network.

use cm_ai::glossary::{clean_entry, fits, GlossaryEntry, MAX_ENTRIES};
use cm_ai::textmatch::key;
use cm_i18n::tr;

/// Why an entry was not saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlossaryError {
    /// The term is empty.
    EmptyTerm,
    /// Another entry already has this term.
    Duplicate,
    /// The glossary is full (entries or size).
    Full,
}

impl GlossaryError {
    /// Worded when drawn, so a note on screen follows a language switch.
    pub fn text(self) -> String {
        match self {
            GlossaryError::EmptyTerm => tr!("Type the term first."),
            GlossaryError::Duplicate => tr!("That term is already in the glossary."),
            GlossaryError::Full => tr!("The glossary is full. Remove an entry to add another."),
        }
    }
}

/// The wrong spellings typed in one box: separated by commas, semicolons or line breaks.
pub fn parse_heard(text: &str) -> Vec<String> {
    text.split([',', ';', '\n'])
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

/// The wrong spellings of an entry as the box shows them.
pub fn heard_text(e: &GlossaryEntry) -> String {
    e.heard_as.join(", ")
}

/// Adds an entry (`at == None`) or changes the one at `at`. The entry is cleaned like the daemon
/// cleans it (spaces, duplicate spellings, length limits). Order is kept: new entries go last, and
/// the first entries are the ones that reach the Whisper prompt first.
pub fn upsert(
    list: &mut Vec<GlossaryEntry>,
    at: Option<usize>,
    term: &str,
    heard: &str,
) -> Result<(), GlossaryError> {
    let entry = clean_entry(&GlossaryEntry {
        term: term.to_owned(),
        heard_as: parse_heard(heard),
    })
    .ok_or(GlossaryError::EmptyTerm)?;
    let k = key(&entry.term, false);
    if list
        .iter()
        .enumerate()
        .any(|(i, e)| Some(i) != at && key(&e.term, false) == k)
    {
        return Err(GlossaryError::Duplicate);
    }
    let mut next = list.clone();
    match at {
        Some(i) if i < next.len() => next[i] = entry,
        _ => next.push(entry),
    }
    if next.len() > MAX_ENTRIES || !fits(&next) {
        return Err(GlossaryError::Full);
    }
    *list = next;
    Ok(())
}

/// Removes the entry at `i`. `false` when there is none.
pub fn remove(list: &mut Vec<GlossaryEntry>, i: usize) -> bool {
    if i < list.len() {
        list.remove(i);
        true
    } else {
        false
    }
}

/// "Often heard as: a, b" for the row of an entry, or the quiet line when it has no spellings.
pub fn row_hint(e: &GlossaryEntry) -> String {
    if e.heard_as.is_empty() {
        tr!("Only helps the model spell it")
    } else {
        tr!("Often heard as: {spellings}", spellings = heard_text(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_split_on_commas_semicolons_and_lines() {
        assert_eq!(
            parse_heard(" Logo  Drive, Gu gồ Drai;\n  \nx "),
            vec!["Logo Drive", "Gu gồ Drai", "x"]
        );
        assert!(parse_heard("  ,; ").is_empty());
    }

    #[test]
    fn adding_editing_and_removing_keep_the_rules() {
        let mut l = Vec::new();
        upsert(&mut l, None, "  Google   Drive ", "logo drive, Logo Drive").unwrap();
        assert_eq!(l[0].term, "Google Drive");
        assert_eq!(l[0].heard_as, vec!["logo drive"], "duplicates are folded");
        upsert(&mut l, None, "CaptureMann", "").unwrap();
        assert_eq!(
            upsert(&mut l, None, " ", "x"),
            Err(GlossaryError::EmptyTerm)
        );
        // The same term in another case or NFD is the same term.
        assert_eq!(
            upsert(&mut l, None, "google drive", ""),
            Err(GlossaryError::Duplicate)
        );
        // Editing an entry may keep its own term.
        upsert(&mut l, Some(0), "Google Drive", "gu gồ drai").unwrap();
        assert_eq!(heard_text(&l[0]), "gu gồ drai");
        assert_eq!(
            upsert(&mut l, Some(1), "Google Drive", ""),
            Err(GlossaryError::Duplicate)
        );
        assert!(remove(&mut l, 0));
        assert!(!remove(&mut l, 5));
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn a_full_glossary_refuses_more_and_is_left_unchanged() {
        let mut l: Vec<GlossaryEntry> = (0..MAX_ENTRIES)
            .map(|i| GlossaryEntry::new(&format!("term {i}"), &[]))
            .collect();
        assert_eq!(
            upsert(&mut l, None, "one more", ""),
            Err(GlossaryError::Full)
        );
        assert_eq!(l.len(), MAX_ENTRIES);
        // Too many bytes for one setting: the big entries stop before the entry limit.
        let mut big: Vec<GlossaryEntry> = Vec::new();
        let mut stopped = None;
        for i in 0..MAX_ENTRIES {
            let heard: Vec<String> = (0..8)
                .map(|j| format!("{}{i}{j}", "y".repeat(40)))
                .collect();
            let r = upsert(
                &mut big,
                None,
                &format!("{}{i}", "t".repeat(40)),
                &heard.join(", "),
            );
            if r.is_err() {
                stopped = Some((i, r));
                break;
            }
        }
        let (at, r) = stopped.expect("the size limit is reached");
        assert_eq!(r, Err(GlossaryError::Full));
        assert!(at < MAX_ENTRIES && big.len() == at && fits(&big));
    }

    #[test]
    fn row_hints_and_errors_are_translated_text() {
        assert_eq!(
            row_hint(&GlossaryEntry::new("A", &["b", "c"])),
            "Often heard as: b, c"
        );
        assert_eq!(
            row_hint(&GlossaryEntry::new("A", &[])),
            "Only helps the model spell it"
        );
        assert!(GlossaryError::Full.text().contains("full"));
    }
}
