//! The word-preserving invariant of the normalisation pass (AI-10).
//!
//! The model may add punctuation, change case and break paragraphs, and nothing else. A word is
//! compared by its **key**: Unicode NFC, lower case, every character that is not a letter, a
//! digit or a combining mark dropped. The key keeps tone marks and `đ` (it is not the folding of
//! [`crate::measure`]), so `ma` and `má` are different words and a "corrected" tone is rejected.
//!
//! [`check_turn`] compares the word keys of the input with the tokens of the model's answer. It
//! passes only when the two sequences are identical (0 substitutions, 0 deletions, 0 insertions)
//! and then returns the alignment: per input word the text to show, which words stick to the
//! word before them (a hyphen the model wrote between two input words), and where the model
//! started a new paragraph. Anything else is a [`Violation`] with the edit counts.

use crate::textmatch::nfc;

/// The key of a word: NFC, lower case, letters, digits and combining marks only.
pub fn word_key(text: &str) -> String {
    nfc(text)
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(c))
        .collect()
}

/// What [`check_turn`] found wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub substituted: usize,
    pub deleted: usize,
    pub inserted: usize,
}

impl Violation {
    /// A short reason for the sidecar: the dominant kind of error.
    pub fn reason(&self) -> &'static str {
        if self.substituted >= self.deleted && self.substituted >= self.inserted {
            "changed_words"
        } else if self.deleted >= self.inserted {
            "dropped_words"
        } else {
            "added_words"
        }
    }

    pub fn errors(&self) -> usize {
        self.substituted + self.deleted + self.inserted
    }
}

/// The answer mapped back onto the input words.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aligned {
    /// One string per input word: the word as the model wrote it, punctuation attached.
    pub display: Vec<String>,
    /// Indexes of words whose display continues the word before it without a space (`twenty-` +
    /// `one`).
    pub glued: Vec<usize>,
    /// Indexes of words that start a new paragraph (never 0).
    pub breaks: Vec<usize>,
}

/// Word-level edit distance of two key sequences, split into the three kinds.
pub fn edit_counts(a: &[String], b: &[String]) -> Violation {
    let (n, m) = (a.len(), b.len());
    // (total, subs, dels, ins) per cell; sequences are a turn long, so the table is small.
    let mut row: Vec<(usize, usize, usize, usize)> = (0..=m).map(|j| (j, 0, 0, j)).collect();
    for i in 1..=n {
        let mut prev_diag = row[0];
        row[0] = (i, 0, i, 0);
        for j in 1..=m {
            let up = row[j];
            let left = row[j - 1];
            let sub = if a[i - 1] == b[j - 1] {
                prev_diag
            } else {
                (prev_diag.0 + 1, prev_diag.1 + 1, prev_diag.2, prev_diag.3)
            };
            let del = (up.0 + 1, up.1, up.2 + 1, up.3);
            let ins = (left.0 + 1, left.1, left.2, left.3 + 1);
            prev_diag = up;
            row[j] = [sub, del, ins]
                .into_iter()
                .min_by_key(|c| c.0)
                .unwrap_or(sub);
        }
    }
    let (_, s, d, i) = row[m];
    Violation {
        substituted: s,
        deleted: d,
        inserted: i,
    }
}

/// Separators inside a token that may join two input words (`well-known` for `well known`).
fn is_joiner(c: char) -> bool {
    matches!(
        c,
        '-' | '\u{2010}' | '\u{2011}' | '\u{2013}' | '\u{2014}' | '/'
    )
}

/// Splits `token` after each joiner: `a-b/c` gives `a-`, `b/`, `c`.
fn split_joined(token: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in token.char_indices() {
        if is_joiner(c) {
            let end = i + c.len_utf8();
            if end < token.len() {
                out.push(&token[start..end]);
                start = end;
            }
        }
    }
    out.push(&token[start..]);
    out
}

/// Longest run of characters that are neither letters nor digits a token may carry. Longer runs
/// are the model decorating (`*****`), not punctuating.
const MAX_SYMBOL_RUN: usize = 6;

fn decorated(token: &str) -> bool {
    let mut run = 0;
    for c in token.chars() {
        if c.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(&c) {
            run = 0;
        } else {
            run += 1;
            if run > MAX_SYMBOL_RUN {
                return true;
            }
        }
    }
    false
}

/// Checks the model's `output` for one turn against the input words' keys (`input_keys`, none of
/// them empty) and aligns it. See the module documentation for the rule.
pub fn check_turn(input_keys: &[String], output: &str) -> Result<Aligned, Violation> {
    // Tokens with a flag for a line break in the white space before them.
    let mut tokens: Vec<(&str, bool)> = Vec::new();
    let mut rest = output;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            break;
        }
        let gap = &rest[..rest.len() - trimmed.len()];
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        tokens.push((&trimmed[..end], gap.contains('\n')));
        rest = &trimmed[end..];
    }

    let n = input_keys.len();
    let mut aligned = Aligned {
        display: Vec::with_capacity(n),
        ..Aligned::default()
    };
    let mut pending_prefix = String::new();
    let mut pending_break = false;
    let mut fail = false;
    for (token, newline) in &tokens {
        if decorated(token) {
            fail = true;
            break;
        }
        let key = word_key(token);
        if key.is_empty() {
            // Punctuation on its own (`—`, `…`, a quote mark): it belongs to the neighbour.
            match aligned.display.last_mut() {
                Some(last) if !*newline => {
                    last.push_str(token);
                }
                _ => {
                    pending_prefix.push_str(token);
                    pending_break |= *newline;
                }
            }
            continue;
        }
        let i = aligned.display.len();
        if i < n && key == input_keys[i] {
            if (*newline || pending_break) && i > 0 {
                aligned.breaks.push(i);
            }
            pending_break = false;
            let mut d = std::mem::take(&mut pending_prefix);
            d.push_str(token);
            aligned.display.push(d);
            continue;
        }
        // One output token for several input words, joined by hyphens or slashes.
        let pieces = split_joined(token);
        if pieces.len() > 1 && i + pieces.len() <= n {
            let all = pieces
                .iter()
                .enumerate()
                .all(|(j, p)| word_key(p) == input_keys[i + j]);
            if all {
                if (*newline || pending_break) && i > 0 {
                    aligned.breaks.push(i);
                }
                pending_break = false;
                for (j, p) in pieces.iter().enumerate() {
                    let mut d = String::new();
                    if j == 0 {
                        d.push_str(&std::mem::take(&mut pending_prefix));
                    } else {
                        aligned.glued.push(i + j);
                    }
                    d.push_str(p);
                    aligned.display.push(d);
                }
                continue;
            }
        }
        fail = true;
        break;
    }
    if !fail && aligned.display.len() == n {
        if !pending_prefix.is_empty() {
            if let Some(last) = aligned.display.last_mut() {
                last.push_str(&pending_prefix);
            }
        }
        return Ok(aligned);
    }
    let out_keys: Vec<String> = tokens
        .iter()
        .flat_map(|(t, _)| {
            let pieces = split_joined(t);
            pieces.into_iter().map(word_key).collect::<Vec<_>>()
        })
        .filter(|k| !k.is_empty())
        .collect();
    let mut v = edit_counts(input_keys, &out_keys);
    if v.errors() == 0 {
        // Same keys but not accepted: decoration or an unmatched split. Count it as one change.
        v.substituted = 1;
    }
    Err(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| word_key(w)).collect()
    }

    #[test]
    fn the_key_drops_case_and_punctuation_and_keeps_tone_marks() {
        assert_eq!(word_key("Xin,"), "xin");
        assert_eq!(word_key("\"Đây\""), "đây");
        assert_ne!(word_key("ma"), word_key("má"));
        assert_ne!(word_key("da"), word_key("đa"));
        // Composed and decomposed spellings of one syllable are one key.
        assert_eq!(word_key("vie\u{0323}\u{0302}t"), word_key("việt"));
        assert_eq!(word_key("don't"), "dont");
        assert_eq!(word_key("3,5"), "35");
        assert_eq!(word_key("—"), "");
    }

    #[test]
    fn identical_words_with_punctuation_and_case_pass() {
        let input = keys(&["xin", "chào", "anh", "nam", "hôm", "nay", "ra", "sao"]);
        let a = check_turn(&input, "Xin chào anh Nam. Hôm nay ra sao?").unwrap();
        assert_eq!(a.display[0], "Xin");
        assert_eq!(a.display[3], "Nam.");
        assert_eq!(a.display[7], "sao?");
        assert!(a.breaks.is_empty() && a.glued.is_empty());
    }

    #[test]
    fn a_new_line_is_a_paragraph_break_before_that_word() {
        let input = keys(&["a", "b", "c", "d"]);
        let a = check_turn(&input, "A b.\n\nC d.").unwrap();
        assert_eq!(a.breaks, vec![2]);
        // Never before the first word.
        let a = check_turn(&input, "\nA b c d").unwrap();
        assert!(a.breaks.is_empty());
    }

    #[test]
    fn a_changed_tone_mark_is_a_substitution() {
        let input = keys(&["ma", "ba", "ta"]);
        let v = check_turn(&input, "Má ba ta.").unwrap_err();
        assert_eq!((v.substituted, v.deleted, v.inserted), (1, 0, 0));
        assert_eq!(v.reason(), "changed_words");
        // đ against d too.
        let input = keys(&["da", "ba"]);
        assert!(check_turn(&input, "Đa ba.").is_err());
    }

    #[test]
    fn dropped_and_added_words_are_counted() {
        let input = keys(&["one", "two", "three", "four"]);
        let v = check_turn(&input, "One two four.").unwrap_err();
        assert_eq!((v.substituted, v.deleted, v.inserted), (0, 1, 0));
        assert_eq!(v.reason(), "dropped_words");
        let v = check_turn(&input, "One, two, three and four.").unwrap_err();
        assert_eq!((v.substituted, v.deleted, v.inserted), (0, 0, 1));
        assert_eq!(v.reason(), "added_words");
        let v = check_turn(&input, "").unwrap_err();
        assert_eq!(v.deleted, 4);
    }

    #[test]
    fn a_reordered_or_numeric_answer_is_rejected() {
        let input = keys(&["two", "one"]);
        assert!(check_turn(&input, "One two.").is_err());
        let input = keys(&["twenty", "one"]);
        assert!(check_turn(&input, "21.").is_err());
    }

    #[test]
    fn lone_punctuation_sticks_to_its_neighbour() {
        let input = keys(&["well", "yes"]);
        let a = check_turn(&input, "\" Well \u{2014} yes \u{2026}").unwrap();
        assert_eq!(a.display, vec!["\"Well\u{2014}", "yes\u{2026}"]);
    }

    #[test]
    fn a_hyphen_between_two_input_words_glues_them() {
        let input = keys(&["twenty", "one", "days"]);
        let a = check_turn(&input, "Twenty-one days.").unwrap();
        assert_eq!(a.display, vec!["Twenty-", "one", "days."]);
        assert_eq!(a.glued, vec![1]);
        // A hyphen that would also change the words is not accepted.
        let input = keys(&["twenty", "two"]);
        assert!(check_turn(&input, "Twenty-one").is_err());
        // A word with an inner hyphen in the input matches as a single key.
        let input = keys(&["e-mail", "me"]);
        assert!(check_turn(&input, "E-mail me.").is_ok());
        let input = keys(&["email", "me"]);
        assert!(check_turn(&input, "E-mail me.").is_ok());
    }

    #[test]
    fn decoration_is_rejected() {
        let input = keys(&["a", "b"]);
        assert!(check_turn(&input, "A ********** b").is_err());
        assert!(check_turn(&input, "A, b!").is_ok());
    }

    #[test]
    fn splitting_one_input_word_in_two_is_rejected() {
        let input = keys(&["airport", "now"]);
        assert!(check_turn(&input, "Air port now.").is_err());
        let input = keys(&["air", "port"]);
        assert!(check_turn(&input, "Airport.").is_err());
    }

    #[test]
    fn the_edit_counts_are_exact() {
        let a: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let b: Vec<String> = ["a", "x", "c", "d"].iter().map(|s| s.to_string()).collect();
        let v = edit_counts(&a, &b);
        assert_eq!((v.substituted, v.deleted, v.inserted), (1, 0, 1));
        assert_eq!(edit_counts(&a, &a).errors(), 0);
    }
}
