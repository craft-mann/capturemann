//! Named placeholders: `{title}`, `{n}`.
//!
//! A placeholder is `{` + an identifier (`[A-Za-z_][A-Za-z0-9_]*`) + `}`. Any other brace is plain
//! text, so a message may contain `{` and `}` as long as they do not enclose an identifier.
//! Substitution is by name, so a translation may reorder placeholders freely. A name with no
//! argument stays as written (`{name}`), which makes a mistake visible instead of silent.

use std::collections::BTreeSet;

/// Where a placeholder starts and ends in `s` (byte offsets of `{` and just past `}`), and its name.
fn next_placeholder(s: &str, from: usize) -> Option<(usize, usize, &str)> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let first_ok = j > start && !bytes[start].is_ascii_digit();
            if first_ok && j < bytes.len() && bytes[j] == b'}' {
                return Some((i, j + 1, &s[start..j]));
            }
        }
        i += 1;
    }
    None
}

/// The distinct placeholder names of `s`.
pub fn placeholders(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut at = 0;
    while let Some((_, end, name)) = next_placeholder(s, at) {
        out.insert(name.to_owned());
        at = end;
    }
    out
}

/// Replaces every `{name}` that has an argument; the rest stay as written.
pub fn substitute(template: &str, args: &[(&str, String)]) -> String {
    if args.is_empty() {
        return template.to_owned();
    }
    let mut out = String::with_capacity(template.len() + 16);
    let mut at = 0;
    while let Some((start, end, name)) = next_placeholder(template, at) {
        out.push_str(&template[at..start]);
        match args.iter().find(|(k, _)| *k == name) {
            Some((_, v)) => out.push_str(v),
            None => out.push_str(&template[start..end]),
        }
        at = end;
    }
    out.push_str(&template[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
        v.iter().map(|(k, v)| (*k, (*v).to_owned())).collect()
    }

    #[test]
    fn names_are_found_once() {
        let p = placeholders("Delete {title}? {n} of {n} left, {x_1}");
        assert_eq!(
            p.into_iter().collect::<Vec<_>>(),
            vec!["n".to_owned(), "title".to_owned(), "x_1".to_owned()]
        );
        assert!(placeholders("{} {1} { a } {a b} {").is_empty());
    }

    #[test]
    fn substitution_is_by_name_so_order_may_change() {
        let a = args(&[("title", "Demo"), ("n", "3")]);
        assert_eq!(
            substitute("Delete {title} and {n} more?", &a),
            "Delete Demo and 3 more?"
        );
        assert_eq!(
            substitute("{n} mục nữa và {title}", &a),
            "3 mục nữa và Demo"
        );
    }

    #[test]
    fn unknown_names_and_plain_braces_stay() {
        let a = args(&[("a", "1")]);
        assert_eq!(substitute("{a} {b} { c } {}", &a), "1 {b} { c } {}");
        assert_eq!(substitute("no args {a}", &[]), "no args {a}");
    }

    #[test]
    fn a_value_is_never_rescanned() {
        let a = args(&[("a", "{b}"), ("b", "X")]);
        assert_eq!(substitute("{a}", &a), "{b}");
    }
}
