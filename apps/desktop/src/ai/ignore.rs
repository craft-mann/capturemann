//! The ignore list of the AI page (AI-1.AC13), display-free: phrases whose lines are left out of
//! new transcripts. The list is the app-only setting `ai.ignore_phrases` (`cm_ai::guard`), empty
//! until the user adds something; the daemon compares it with the heard text folded (case,
//! accents, punctuation) and drops the lines that contain a phrase. Nothing here is a built-in
//! list.

use cm_ai::guard::{clean_phrases, MAX_PHRASES};
use cm_ai::pieces::fold;
use cm_i18n::tr;

/// Why a phrase was not added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreError {
    /// Nothing typed, or fewer than three letters and digits.
    TooShort,
    /// Already in the list (same letters, whatever the case, accents or punctuation).
    Duplicate,
    /// The list is full.
    Full,
}

impl IgnoreError {
    /// Worded when drawn, so a note on screen follows a language switch.
    pub fn text(self) -> String {
        match self {
            IgnoreError::TooShort => tr!("Type at least three letters first."),
            IgnoreError::Duplicate => tr!("That phrase is already in the list."),
            IgnoreError::Full => tr!("The list is full. Remove a phrase to add another."),
        }
    }
}

/// Adds `text`, cleaned like the daemon reads the setting.
pub fn add(list: &mut Vec<String>, text: &str) -> Result<(), IgnoreError> {
    let Some(phrase) = clean_phrases([text]).into_iter().next() else {
        return Err(IgnoreError::TooShort);
    };
    let k = fold(&phrase);
    if list.iter().any(|p| fold(p) == k) {
        return Err(IgnoreError::Duplicate);
    }
    if list.len() >= MAX_PHRASES {
        return Err(IgnoreError::Full);
    }
    list.push(phrase);
    Ok(())
}

/// Removes the phrase at `i`. `false` when there is none.
pub fn remove(list: &mut Vec<String>, i: usize) -> bool {
    if i < list.len() {
        list.remove(i);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phrase_is_cleaned_and_a_repeat_in_other_letters_is_refused() {
        let mut l = Vec::new();
        add(&mut l, "  Cảm   ơn các bạn ").unwrap();
        assert_eq!(l, ["Cảm ơn các bạn"]);
        assert_eq!(add(&mut l, "cam on, cac ban!"), Err(IgnoreError::Duplicate));
        assert_eq!(add(&mut l, " a "), Err(IgnoreError::TooShort));
        assert_eq!(add(&mut l, ""), Err(IgnoreError::TooShort));
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn the_list_is_limited_and_phrases_can_be_removed() {
        let mut l: Vec<String> = (0..MAX_PHRASES).map(|i| format!("phrase {i:03}")).collect();
        assert_eq!(add(&mut l, "one more"), Err(IgnoreError::Full));
        assert!(remove(&mut l, 0));
        assert!(!remove(&mut l, 999));
        add(&mut l, "one more").unwrap();
        assert_eq!(l.last().map(String::as_str), Some("one more"));
    }
}
