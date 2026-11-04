//! Scores a normalised text against a reference that has its own punctuation and case (the
//! benchmark of AI-10). Both texts are lists of tokens of the same length, token `i` being the
//! same word (the invariant guarantees it for an accepted turn).
//!
//! * Punctuation: the mark after each word, in three classes: comma-like (`, ; :`), sentence end
//!   (`. ! …`) and question (`?`). Precision, recall and F1 per class and for the sentence
//!   boundaries (sentence end or question).
//! * Case: whether a word starts with a capital letter; accuracy over the words that start with
//!   a letter, and precision and recall of the capitals.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Comma,
    End,
    Question,
}

/// The mark that follows the letters of `token` (closing quotes and brackets are skipped).
pub fn mark_after(token: &str) -> Mark {
    let t = token.trim_end_matches(['"', '\'', '\u{201D}', '\u{2019}', ')', ']', '\u{00BB}']);
    match t.chars().last() {
        Some(',' | ';' | ':') => Mark::Comma,
        Some('.' | '!' | '\u{2026}') => Mark::End,
        Some('?') => Mark::Question,
        _ => Mark::None,
    }
}

/// Whether the token starts with a capital letter (leading quotes and brackets are skipped);
/// `None` when it does not start with a letter.
pub fn starts_capital(token: &str) -> Option<bool> {
    let c = token
        .trim_start_matches(['"', '\'', '\u{201C}', '\u{2018}', '(', '[', '\u{00AB}'])
        .chars()
        .next()?;
    c.is_alphabetic().then(|| c.is_uppercase())
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Prf {
    pub tp: usize,
    pub fp: usize,
    pub fn_: usize,
}

impl Prf {
    pub fn precision(&self) -> f64 {
        ratio(self.tp, self.tp + self.fp)
    }
    pub fn recall(&self) -> f64 {
        ratio(self.tp, self.tp + self.fn_)
    }
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 {
        0.0
    } else {
        a as f64 / b as f64
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Tally {
    pub words: usize,
    pub comma: Prf,
    pub period: Prf,
    pub question: Prf,
    pub boundary: Prf,
    pub cap_words: usize,
    pub cap_correct: usize,
    pub capitals: Prf,
}

impl Tally {
    /// Adds one text. The slices must have the same length (the extra tokens are ignored).
    pub fn add(&mut self, reference: &[String], hypothesis: &[String]) {
        for (r, h) in reference.iter().zip(hypothesis) {
            self.words += 1;
            let (mr, mh) = (mark_after(r), mark_after(h));
            let put = |p: &mut Prf, in_r: bool, in_h: bool| match (in_r, in_h) {
                (true, true) => p.tp += 1,
                (false, true) => p.fp += 1,
                (true, false) => p.fn_ += 1,
                _ => {}
            };
            put(&mut self.comma, mr == Mark::Comma, mh == Mark::Comma);
            put(&mut self.period, mr == Mark::End, mh == Mark::End);
            put(
                &mut self.question,
                mr == Mark::Question,
                mh == Mark::Question,
            );
            let bound = |m: Mark| matches!(m, Mark::End | Mark::Question);
            put(&mut self.boundary, bound(mr), bound(mh));
            if let (Some(cr), Some(ch)) = (starts_capital(r), starts_capital(h)) {
                self.cap_words += 1;
                self.cap_correct += usize::from(cr == ch);
                put(&mut self.capitals, cr, ch);
            }
        }
    }

    pub fn cap_accuracy(&self) -> f64 {
        ratio(self.cap_correct, self.cap_words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn marks_and_capitals_are_read_from_the_token_edges() {
        assert_eq!(mark_after("word,"), Mark::Comma);
        assert_eq!(mark_after("word.\""), Mark::End);
        assert_eq!(mark_after("word?"), Mark::Question);
        assert_eq!(mark_after("word"), Mark::None);
        assert_eq!(starts_capital("\"Hello"), Some(true));
        assert_eq!(starts_capital("hello"), Some(false));
        assert_eq!(starts_capital("42"), None);
        assert_eq!(starts_capital("\u{110}\u{1EA1}i"), Some(true));
    }

    #[test]
    fn a_perfect_copy_scores_one_and_raw_text_scores_zero_recall() {
        let reference = t("Hello, world. Is it? Yes");
        let mut a = Tally::default();
        a.add(&reference, &reference);
        assert_eq!(a.comma.f1(), 1.0);
        assert_eq!(a.boundary.f1(), 1.0);
        assert_eq!(a.question.f1(), 1.0);
        assert_eq!(a.cap_accuracy(), 1.0);
        let raw = t("hello world is it yes");
        let mut b = Tally::default();
        b.add(&reference, &raw);
        assert_eq!(b.comma.recall(), 0.0);
        assert_eq!(b.boundary.recall(), 0.0);
        // Raw text gets the lower-case words right and misses the three capitals.
        assert_eq!(b.cap_correct, 2);
        assert_eq!(b.capitals.fn_, 3);
    }

    #[test]
    fn a_wrong_mark_is_a_false_positive_and_a_miss() {
        let mut a = Tally::default();
        a.add(&t("one, two"), &t("one. two"));
        assert_eq!((a.comma.tp, a.comma.fn_), (0, 1));
        assert_eq!((a.period.fp, a.boundary.fp), (1, 1));
    }
}
