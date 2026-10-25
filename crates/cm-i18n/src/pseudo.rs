//! The pseudo-locale `en-XA`: accented letters, about 30 percent longer, in brackets.
//!
//! `Copy link` becomes `[Çöpý ļîñķ ~~~]`. It exposes clipped layouts (the padding makes every text
//! longer), hard-coded strings (they stay plain) and concatenated sentences (the brackets show
//! where a message starts and ends). Placeholders (`{title}`) are copied unchanged, so values
//! substituted later are never mangled. It is selected only by `CAPTUREMANN_PSEUDO_LOCALE=1`.

use crate::subst;

/// Whether the environment value asks for the pseudo-locale (`1` or `true`).
pub fn requested(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1") | Some("true"))
}

fn accent(c: char) -> char {
    match c {
        'a' => 'å',
        'b' => 'ƀ',
        'c' => 'ç',
        'd' => 'ð',
        'e' => 'é',
        'f' => 'ƒ',
        'g' => 'ĝ',
        'h' => 'ĥ',
        'i' => 'î',
        'j' => 'ĵ',
        'k' => 'ķ',
        'l' => 'ļ',
        'n' => 'ñ',
        'o' => 'ö',
        'p' => 'þ',
        'r' => 'ŕ',
        's' => 'š',
        't' => 'ţ',
        'u' => 'ü',
        'w' => 'ŵ',
        'y' => 'ý',
        'z' => 'ž',
        'A' => 'Å',
        'B' => 'Ɓ',
        'C' => 'Ç',
        'D' => 'Ð',
        'E' => 'É',
        'G' => 'Ĝ',
        'H' => 'Ĥ',
        'I' => 'Î',
        'J' => 'Ĵ',
        'K' => 'Ķ',
        'L' => 'Ļ',
        'N' => 'Ñ',
        'O' => 'Ö',
        'P' => 'Þ',
        'R' => 'Ŕ',
        'S' => 'Š',
        'T' => 'Ţ',
        'U' => 'Û',
        'W' => 'Ŵ',
        'Y' => 'Ý',
        'Z' => 'Ž',
        other => other,
    }
}

/// Turns a message (before substitution) into its pseudo form.
pub fn localize(text: &str) -> String {
    let names = subst::placeholders(text);
    let mut out = String::with_capacity(text.len() * 2);
    out.push('[');
    let mut chars = 0usize;
    let mut rest = text;
    while !rest.is_empty() {
        // Copy a whole `{name}` untouched.
        if let Some(end) = placeholder_len(rest, &names) {
            out.push_str(&rest[..end]);
            // A value of unknown length: count as ten characters of growth budget.
            chars += 10;
            rest = &rest[end..];
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        out.push(accent(c));
        chars += 1;
        rest = &rest[c.len_utf8()..];
    }
    let pad = (chars * 3).div_ceil(10);
    if pad > 0 {
        out.push(' ');
        out.extend(std::iter::repeat_n('~', pad.min(60)));
    }
    out.push(']');
    out
}

fn placeholder_len(s: &str, names: &std::collections::BTreeSet<String>) -> Option<usize> {
    let rest = s.strip_prefix('{')?;
    let close = rest.find('}')?;
    names.contains(&rest[..close]).then_some(close + 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_accents_and_growth() {
        let s = localize("Copy link");
        assert!(s.starts_with('[') && s.ends_with(']'));
        assert!(s.contains("Çöþý ļîñķ"), "{s}");
        // 9 characters + 3 tildes (30 percent, rounded up).
        assert!(s.ends_with(" ~~~]"), "{s}");
        assert!(s.chars().count() >= "Copy link".chars().count() * 13 / 10);
    }

    #[test]
    fn placeholders_survive_and_substitute_afterwards() {
        let s = localize("Delete {title}?");
        assert!(s.contains("{title}"), "{s}");
        let out = subst::substitute(&s, &[("title", "My clip".to_owned())]);
        assert!(out.contains("My clip") && !out.contains("{title}"), "{out}");
    }

    #[test]
    fn empty_stays_bracketed_and_the_env_switch_is_strict() {
        assert_eq!(localize(""), "[]");
        assert!(requested(Some("1")) && requested(Some("true")));
        assert!(!requested(Some("0")) && !requested(Some("")) && !requested(None));
    }
}
