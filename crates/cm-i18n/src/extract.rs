//! Extraction and checks, used by the `cm-i18n` tool and the tests (no network, no display).
//!
//! * [`scan_source`] / [`scan_workspace`] find every `tr!`, `tr_ctx!` and `trn!` call and build the
//!   template `locales/capturemann.pot` ([`render_pot`]); the template is generated, never edited.
//! * [`check_catalogue`] reports stale msgids, duplicates, placeholder mismatches and empty or
//!   missing plural forms of a `.po` file against the template.
//! * [`coverage`] counts translated messages per language.
//! * [`count_unwrapped`] counts UI string literals in `apps/desktop/src` that are not wrapped yet;
//!   the ratchet test keeps that number from going up (`locales/unwrapped-baseline.txt`).
//!
//! The scanner reads Rust source with a small lexer (strings, raw strings, comments, char
//! literals), not a full parser. It needs the message text to be a string literal.
//!
//! A `// TRANSLATORS: note` comment is carried into the template as an extracted comment
//! (`#. note`) of the message it sits above (see [`scan_source`] for the scope rules). The
//! review marker `// TRANSLATORS: FACT: owner review required` therefore reaches the translators.

use crate::po::{self, Catalogue, Entry};
use crate::{placeholders, Lang};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------
// Lexer

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Ident(String),
    Punct(char),
    Str(String),
}

#[derive(Debug, Clone)]
struct Tok {
    kind: Kind,
    line: usize,
}

struct Lexed {
    toks: Vec<Tok>,
    /// Lines carrying an `i18n-ignore` comment.
    ignore_lines: BTreeSet<usize>,
    /// `// TRANSLATORS:` comments.
    notes: Vec<TranslatorNote>,
}

/// A `// TRANSLATORS:` comment: its text (without the prefix) and the last line it covers.
#[derive(Debug, Clone)]
struct TranslatorNote {
    text: String,
    last_line: usize,
}

/// The body of a plain `//` line comment (`None` for `///` and `//!` doc comments).
fn plain_comment_body(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("//")?;
    if rest.starts_with('/') || rest.starts_with('!') {
        return None;
    }
    Some(rest.trim())
}

fn lex(src: &str) -> Lexed {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut ignore_lines = BTreeSet::new();
    let mut notes: Vec<TranslatorNote> = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let n = chars.len();
    while i < n {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            let start = i;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            if text.contains("i18n-ignore") {
                ignore_lines.insert(line);
            }
            if let Some(body) = plain_comment_body(&text) {
                if let Some(note) = body.strip_prefix("TRANSLATORS:") {
                    notes.push(TranslatorNote {
                        text: note.trim().to_owned(),
                        last_line: line,
                    });
                } else if let Some(open) = notes.last_mut() {
                    // A plain comment on the very next line continues the note.
                    if open.last_line + 1 == line
                        && !body.is_empty()
                        && !text.contains("i18n-ignore")
                    {
                        open.text.push(' ');
                        open.text.push_str(body);
                        open.last_line = line;
                    }
                }
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if c == '"' {
            let start_line = line;
            let (s, next, lines) = read_string(&chars, i + 1);
            toks.push(Tok {
                kind: Kind::Str(s),
                line: start_line,
            });
            line += lines;
            i = next;
        } else if c == '\'' {
            // A char literal ('a', '\n', '"') or a lifetime ('a).
            if chars.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < n && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if chars.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // String prefixes: r"..", r#".."#, b"..", br"..".
            if matches!(word.as_str(), "r" | "b" | "br" | "rb") {
                let mut j = i;
                let mut hashes = 0;
                while j < n && chars[j] == '#' && word.contains('r') {
                    hashes += 1;
                    j += 1;
                }
                if j < n && chars[j] == '"' {
                    let start_line = line;
                    if word.contains('r') {
                        let (s, next, lines) = read_raw(&chars, j + 1, hashes);
                        toks.push(Tok {
                            kind: Kind::Str(s),
                            line: start_line,
                        });
                        line += lines;
                        i = next;
                    } else {
                        let (s, next, lines) = read_string(&chars, j + 1);
                        toks.push(Tok {
                            kind: Kind::Str(s),
                            line: start_line,
                        });
                        line += lines;
                        i = next;
                    }
                    continue;
                }
            }
            toks.push(Tok {
                kind: Kind::Ident(word),
                line,
            });
        } else if c.is_ascii_digit() {
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
        } else {
            toks.push(Tok {
                kind: Kind::Punct(c),
                line,
            });
            i += 1;
        }
    }
    Lexed {
        toks,
        ignore_lines,
        notes,
    }
}

/// Reads a normal string body starting after the opening quote. Returns the decoded value, the
/// index after the closing quote and the number of newlines crossed.
fn read_string(chars: &[char], mut i: usize) -> (String, usize, usize) {
    let mut out = String::new();
    let mut lines = 0;
    while i < chars.len() {
        match chars[i] {
            '"' => return (out, i + 1, lines),
            '\\' => {
                i += 1;
                match chars.get(i) {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('0') => out.push('\0'),
                    Some('\\') => out.push('\\'),
                    Some('\'') => out.push('\''),
                    Some('"') => out.push('"'),
                    Some('x') => {
                        let h: String = chars[i + 1..(i + 3).min(chars.len())].iter().collect();
                        if let Some(c) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                            out.push(c);
                        }
                        i += 2;
                    }
                    Some('u') => {
                        // \u{1234}
                        let mut j = i + 1;
                        if chars.get(j) == Some(&'{') {
                            j += 1;
                            let mut h = String::new();
                            while j < chars.len() && chars[j] != '}' {
                                if chars[j] != '_' {
                                    h.push(chars[j]);
                                }
                                j += 1;
                            }
                            if let Some(c) =
                                u32::from_str_radix(&h, 16).ok().and_then(char::from_u32)
                            {
                                out.push(c);
                            }
                            i = j;
                        }
                    }
                    Some('\n') => {
                        // A backslash before a newline skips the newline and the next indent.
                        lines += 1;
                        while chars.get(i + 1).is_some_and(|c| c.is_whitespace()) {
                            if chars[i + 1] == '\n' {
                                lines += 1;
                            }
                            i += 1;
                        }
                    }
                    Some(other) => out.push(*other),
                    None => {}
                }
                i += 1;
            }
            c => {
                if c == '\n' {
                    lines += 1;
                }
                out.push(c);
                i += 1;
            }
        }
    }
    (out, i, lines)
}

fn read_raw(chars: &[char], mut i: usize, hashes: usize) -> (String, usize, usize) {
    let mut out = String::new();
    let mut lines = 0;
    while i < chars.len() {
        if chars[i] == '"' && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#')) {
            return (out, i + 1 + hashes, lines);
        }
        if chars[i] == '\n' {
            lines += 1;
        }
        out.push(chars[i]);
        i += 1;
    }
    (out, i, lines)
}

/// Removes the tokens of items marked `#[cfg(test)]` (a module, a function, a const).
fn drop_test_items(toks: Vec<Tok>) -> Vec<Tok> {
    let is = |t: &Tok, k: &Kind| &t.kind == k;
    let mut out = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let attr = i + 6 < toks.len()
            && is(&toks[i], &Kind::Punct('#'))
            && is(&toks[i + 1], &Kind::Punct('['))
            && is(&toks[i + 2], &Kind::Ident("cfg".into()))
            && is(&toks[i + 3], &Kind::Punct('('))
            && is(&toks[i + 4], &Kind::Ident("test".into()))
            && is(&toks[i + 5], &Kind::Punct(')'))
            && is(&toks[i + 6], &Kind::Punct(']'));
        if !attr {
            out.push(toks[i].clone());
            i += 1;
            continue;
        }
        i += 7;
        // Skip the item: up to `;`, or through its brace block.
        let mut depth = 0usize;
        while i < toks.len() {
            match toks[i].kind {
                Kind::Punct('{') => depth += 1,
                Kind::Punct('}') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                Kind::Punct(';') if depth == 0 => {
                    i += 1;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Messages

/// A translatable message found in the source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Message {
    pub ctx: Option<String>,
    pub id: String,
    pub plural: Option<String>,
    /// `// TRANSLATORS:` comments above the call, for the translators (`#. ` in the template).
    pub notes: Vec<String>,
}

impl Message {
    pub fn key(&self) -> String {
        po::key(self.ctx.as_deref(), &self.id)
    }
}

/// A problem found while scanning or checking, with where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

/// Whether the tokens at `i` start a `tr!(`, `tr_ctx!(` or `trn!(` call (not a macro definition).
fn is_message_call(toks: &[Tok], i: usize) -> bool {
    if i + 2 >= toks.len() {
        return false;
    }
    let named =
        matches!(&toks[i].kind, Kind::Ident(n) if matches!(n.as_str(), "tr" | "tr_ctx" | "trn"));
    let in_definition = i > 0
        && (matches!(&toks[i - 1].kind, Kind::Ident(m) if m == "macro_rules")
            || toks[i - 1].kind == Kind::Punct('$'));
    named
        && toks[i + 1].kind == Kind::Punct('!')
        && toks[i + 2].kind == Kind::Punct('(')
        && !in_definition
}

/// The token indexes of the calls a `// TRANSLATORS:` comment ending on `last_line` is about.
///
/// From the first token after the comment: when a message call comes first (a comment inside a
/// struct literal, an argument list or a match arm) it is that call only; when a `{` comes first
/// (the comment sits above a `fn`, a `match` or a block) it is every call inside that block.
fn calls_noted(toks: &[Tok], last_line: usize) -> Vec<usize> {
    let Some(start) = toks.iter().position(|t| t.line > last_line) else {
        return Vec::new();
    };
    let mut k = start;
    while k < toks.len() {
        if is_message_call(toks, k) {
            return vec![k];
        }
        match toks[k].kind {
            Kind::Punct('{') => {
                let mut depth = 0usize;
                let mut calls = Vec::new();
                for (j, t) in toks.iter().enumerate().skip(k) {
                    match t.kind {
                        Kind::Punct('{') => depth += 1,
                        Kind::Punct('}') => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {
                            if is_message_call(toks, j) {
                                calls.push(j);
                            }
                        }
                    }
                }
                return calls;
            }
            // The comment is about something that holds no message.
            Kind::Punct(';') | Kind::Punct('}') => return Vec::new(),
            _ => {}
        }
        k += 1;
    }
    Vec::new()
}

/// Finds the `tr!`, `tr_ctx!` and `trn!` calls of one source file.
///
/// A `// TRANSLATORS: text` comment becomes a note of the message it is above: the call that
/// follows it, or every call of the block (`fn`, `match`, `{ .. }`) that follows it.
pub fn scan_source(file: &str, src: &str) -> (Vec<Message>, Vec<Problem>) {
    let lexed = lex(src);
    let toks = lexed.toks;
    let mut msgs: Vec<Message> = Vec::new();
    // The token index of each message, to attach the notes.
    let mut at: Vec<usize> = Vec::new();
    let mut problems = Vec::new();
    let mut i = 0;
    while i + 2 < toks.len() {
        let name = match &toks[i].kind {
            Kind::Ident(n) if matches!(n.as_str(), "tr" | "tr_ctx" | "trn") => n.as_str(),
            _ => {
                i += 1;
                continue;
            }
        };
        let is_call = toks[i + 1].kind == Kind::Punct('!') && toks[i + 2].kind == Kind::Punct('(');
        // `$crate::tr!` inside a macro definition and `macro_rules! tr` are not calls.
        let in_definition = i > 0
            && matches!(&toks[i - 1].kind, Kind::Ident(m) if m == "macro_rules")
            || i > 0 && toks[i - 1].kind == Kind::Punct('$');
        if !is_call || in_definition {
            i += 1;
            continue;
        }
        let line = toks[i].line;
        let lit = |at: usize| match toks.get(at).map(|t| &t.kind) {
            Some(Kind::Str(s)) => Some(s.clone()),
            _ => None,
        };
        let comma = |at: usize| toks.get(at).map(|t| &t.kind) == Some(&Kind::Punct(','));
        let mut bad = |what: &str| {
            problems.push(Problem {
                file: file.to_owned(),
                line,
                message: format!("{name}! needs {what} as string literals"),
            })
        };
        match name {
            "tr" => match lit(i + 3) {
                Some(id) => {
                    at.push(i);
                    msgs.push(Message {
                        ctx: None,
                        id,
                        plural: None,
                        notes: Vec::new(),
                    })
                }
                None => bad("its message"),
            },
            "tr_ctx" => match (lit(i + 3), comma(i + 4), lit(i + 5)) {
                (Some(ctx), true, Some(id)) => {
                    at.push(i);
                    msgs.push(Message {
                        ctx: Some(ctx),
                        id,
                        plural: None,
                        notes: Vec::new(),
                    })
                }
                _ => bad("its context and message"),
            },
            _ => match (lit(i + 3), comma(i + 4), lit(i + 5)) {
                (Some(id), true, Some(plural)) => {
                    at.push(i);
                    msgs.push(Message {
                        ctx: None,
                        id,
                        plural: Some(plural),
                        notes: Vec::new(),
                    })
                }
                _ => bad("its singular and plural"),
            },
        }
        i += 3;
    }
    for note in &lexed.notes {
        if note.text.is_empty() {
            continue;
        }
        for call in calls_noted(&toks, note.last_line) {
            if let Some(m) = at.iter().position(|a| *a == call).map(|p| &mut msgs[p]) {
                if !m.notes.contains(&note.text) {
                    m.notes.push(note.text.clone());
                }
            }
        }
    }
    (msgs, problems)
}

/// Rust source files of the workspace that can hold UI strings: `crates/*/src` and `apps/*/src`,
/// without this crate (its tests and docs use sample strings).
pub fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for group in ["crates", "apps"] {
        let Ok(rd) = std::fs::read_dir(root.join(group)) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            if d.file_name().is_some_and(|n| n == "cm-i18n") {
                continue;
            }
            walk(&d.join("src"), &mut out);
        }
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The scan of the whole workspace: messages by key (sorted), and the problems.
#[derive(Debug, Default)]
pub struct Scan {
    pub messages: Vec<Message>,
    pub problems: Vec<Problem>,
}

pub fn scan_workspace(root: &Path) -> Scan {
    let mut by_key: BTreeMap<String, Message> = BTreeMap::new();
    let mut problems = Vec::new();
    for path in source_files(root) {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = rel(root, &path);
        let (msgs, mut p) = scan_source(&name, &src);
        problems.append(&mut p);
        for m in msgs {
            match by_key.get(&m.key()) {
                Some(old) if old.plural != m.plural => problems.push(Problem {
                    file: name.clone(),
                    line: 0,
                    message: format!(
                        "`{}` is used with different plural forms ({:?} and {:?})",
                        m.id, old.plural, m.plural
                    ),
                }),
                Some(_) => {
                    // The same message elsewhere: its notes join the first one's.
                    if let Some(old) = by_key.get_mut(&m.key()) {
                        for n in m.notes {
                            if !old.notes.contains(&n) {
                                old.notes.push(n);
                            }
                        }
                    }
                }
                None => {
                    by_key.insert(m.key(), m);
                }
            }
        }
    }
    let mut messages: Vec<Message> = by_key.into_values().collect();
    messages.sort();
    Scan { messages, problems }
}

/// The text of `locales/capturemann.pot` for these messages (sorted, no line references, so the
/// file only changes when a message does).
pub fn render_pot(messages: &[Message]) -> String {
    let mut out = String::new();
    out.push_str("# CaptureMann UI strings (the template for every language).\n");
    out.push_str("# GENERATED by `cargo run -p cm-i18n -- extract`. Do not edit by hand.\n");
    out.push_str("msgid \"\"\nmsgstr \"\"\n");
    out.push_str("\"Project-Id-Version: CaptureMann\\n\"\n");
    out.push_str("\"MIME-Version: 1.0\\n\"\n");
    out.push_str("\"Content-Type: text/plain; charset=UTF-8\\n\"\n");
    out.push_str("\"Content-Transfer-Encoding: 8bit\\n\"\n");
    let mut sorted: Vec<&Message> = messages.iter().collect();
    sorted.sort();
    for m in sorted {
        out.push('\n');
        let e = Entry {
            ctx: m.ctx.clone(),
            id: m.id.clone(),
            plural: m.plural.clone(),
            notes: m.notes.clone(),
            strs: if m.plural.is_some() {
                vec![String::new(), String::new()]
            } else {
                vec![String::new()]
            },
            ..Entry::default()
        };
        po::write_entry(&mut out, &e);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Catalogue checks

/// Why a translation cannot be used as written, if it cannot: it names a placeholder the English
/// does not have (so it could never be filled).
pub fn placeholder_problem(e: &Entry) -> Option<String> {
    let mut allowed = placeholders(&e.id);
    if let Some(p) = &e.plural {
        allowed.extend(placeholders(p));
        allowed.insert("n".to_owned());
    }
    for s in &e.strs {
        let extra: Vec<String> = placeholders(s).difference(&allowed).cloned().collect();
        if !extra.is_empty() {
            return Some(format!("unknown placeholder {{{}}}", extra.join("}, {")));
        }
    }
    None
}

/// Whether the runtime may use this entry's translation.
pub fn entry_is_usable(e: &Entry, _lang: Lang) -> bool {
    placeholder_problem(e).is_none()
}

/// Checks a `.po` against the template messages.
pub fn check_catalogue(
    file: &str,
    template: &[Message],
    cat: &Catalogue,
    lang: Lang,
) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut add = |line: usize, message: String| {
        out.push(Problem {
            file: file.to_owned(),
            line,
            message,
        })
    };
    if cat.header_value("Language") != Some(lang.code()) {
        add(0, format!("header `Language` must be `{}`", lang.code()));
    }
    if cat.header_value("Plural-Forms") != Some(lang.plural_forms_header()) {
        add(
            0,
            format!(
                "header `Plural-Forms` must be `{}`",
                lang.plural_forms_header()
            ),
        );
    }
    for d in cat.duplicates() {
        add(d.line, format!("duplicate msgid `{}`", d.id));
    }
    let known: BTreeMap<String, &Message> = template.iter().map(|m| (m.key(), m)).collect();
    for e in &cat.entries {
        let Some(m) = known.get(&e.key()) else {
            add(
                e.line,
                format!(
                    "stale msgid `{}` (not in capturemann.pot; remove it or run extract)",
                    e.id
                ),
            );
            continue;
        };
        if m.plural.is_some() != e.plural.is_some() {
            add(
                e.line,
                format!("`{}`: plural-ness differs from the template", e.id),
            );
            continue;
        }
        if !e.is_translated() {
            continue;
        }
        if let Some(p) = &e.plural {
            if e.strs.len() != lang.plural_count() {
                add(
                    e.line,
                    format!(
                        "`{}`: {} plural forms, {} needs {}",
                        e.id,
                        e.strs.len(),
                        lang.code(),
                        lang.plural_count()
                    ),
                );
            }
            if e.strs.iter().any(String::is_empty) {
                add(e.line, format!("`{}`: an empty plural form", e.id));
            }
            if p != m.plural.as_deref().unwrap_or("") {
                add(
                    e.line,
                    format!("`{}`: msgid_plural differs from the template", e.id),
                );
            }
        } else if e.strs.len() > 1 {
            add(e.line, format!("`{}`: more than one msgstr", e.id));
        }
        match placeholder_problem(e) {
            Some(p) => add(e.line, format!("`{}`: {p}", e.id)),
            None => {
                // A singular message must keep every placeholder; plural forms may drop {n}.
                if e.plural.is_none() {
                    let want = placeholders(&e.id);
                    let got = placeholders(e.strs.first().map(String::as_str).unwrap_or(""));
                    if want != got {
                        add(
                            e.line,
                            format!(
                                "`{}`: placeholders differ (English {:?}, translation {:?})",
                                e.id, want, got
                            ),
                        );
                    }
                }
            }
        }
    }
    out
}

/// How much of the template a catalogue covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coverage {
    pub total: usize,
    pub translated: usize,
    /// Of the translated ones, marked `fuzzy` (a draft to review).
    pub fuzzy: usize,
}

impl Coverage {
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            self.translated as f64 * 100.0 / self.total as f64
        }
    }
}

pub fn coverage(template: &[Message], cat: &Catalogue) -> Coverage {
    let mut c = Coverage {
        total: template.len(),
        translated: 0,
        fuzzy: 0,
    };
    for m in template {
        if let Some(e) = cat.get(m.ctx.as_deref(), &m.id) {
            if e.is_translated() {
                c.translated += 1;
                if e.is_fuzzy() {
                    c.fuzzy += 1;
                }
            }
        }
    }
    c
}

/// The coverage report, one line per language.
pub fn coverage_report(template: &[Message], cats: &[(Lang, Catalogue)]) -> String {
    let mut out = format!("{} messages in the template\n", template.len());
    out.push_str("en: 100.0% (source text)\n");
    for (lang, cat) in cats {
        let c = coverage(template, cat);
        out.push_str(&format!(
            "{}: {:.1}% ({} of {} translated, {} draft)\n",
            lang.code(),
            c.percent(),
            c.translated,
            c.total,
            c.fuzzy
        ));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Unwrapped literal ratchet

/// One UI-looking string literal that is not wrapped in `tr!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unwrapped {
    pub file: String,
    pub line: usize,
    pub text: String,
}

impl std::fmt::Display for Unwrapped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {:?}", self.file, self.line, self.text)
    }
}

/// Calls whose string arguments are never UI text.
const NOT_UI_CALLS: &[&str] = &[
    "expect",
    "expect_err",
    "unwrap_or",
    "unwrap_or_else",
    "push_str",
    "join",
    "split",
    "splitn",
    "rsplit",
    "starts_with",
    "ends_with",
    "contains",
    "find",
    "strip_prefix",
    "strip_suffix",
    "trim_start_matches",
    "trim_end_matches",
    "trim_matches",
    "replace",
    "env",
    "var",
    "arg",
    "args",
    "parse",
    "insert",
    "get",
    "remove",
    "entry",
    "contains_key",
    "mark",
    "id",
    "internal",
    "invalid_input",
    "invalid_field",
    "not_found",
    "open",
    "create",
    "tempdir",
    "set_extension",
    "with_extension",
    "with_file_name",
    "file_name",
    "from_str",
    "to_str",
    "eq_ignore_ascii_case",
    "ok_or",
    "ok_or_else",
    "context",
    "with_context",
    "msg",
    "anyhow",
    "bail",
];

/// Whether a literal reads like text a person would see: starts with a capital letter, has a
/// lowercase letter, and is not an identifier-like token (`CaptureMann`, `ThemePreference`) or a
/// path.
fn looks_like_ui_text(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_uppercase() || !s.chars().any(char::is_lowercase) {
        return false;
    }
    if !s.contains(char::is_whitespace) {
        if s.contains(['/', '\\', '_', '=', ':']) {
            return false;
        }
        // CamelCase single token: an uppercase letter right after a lowercase one.
        let v: Vec<char> = s.chars().collect();
        if v.windows(2)
            .any(|w| w[0].is_lowercase() && w[1].is_uppercase())
        {
            return false;
        }
    }
    true
}

/// Counts the unwrapped UI literals of one source file (test items excluded).
pub fn unwrapped_in_source(file: &str, src: &str) -> Vec<Unwrapped> {
    let lexed = lex(src);
    let ignore = lexed.ignore_lines;
    let toks = drop_test_items(lexed.toks);
    // The enclosing bracket: (opening char, identifier before it, is a macro call, is SharedString::from).
    struct Open {
        ch: char,
        ident: Option<String>,
        is_macro: bool,
        shared_from: bool,
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        match &t.kind {
            Kind::Punct(c @ ('(' | '[' | '{')) => {
                let mut ident = None;
                let mut is_macro = false;
                let mut shared_from = false;
                if *c == '(' && i > 0 {
                    match &toks[i - 1].kind {
                        Kind::Ident(n) => {
                            ident = Some(n.clone());
                            shared_from = n == "from"
                                && i >= 4
                                && toks[i - 2].kind == Kind::Punct(':')
                                && toks[i - 3].kind == Kind::Punct(':')
                                && toks[i - 4].kind == Kind::Ident("SharedString".into());
                        }
                        Kind::Punct('!') if i >= 2 => {
                            if let Kind::Ident(n) = &toks[i - 2].kind {
                                ident = Some(n.clone());
                                is_macro = true;
                            }
                        }
                        _ => {}
                    }
                }
                stack.push(Open {
                    ch: *c,
                    ident,
                    is_macro,
                    shared_from,
                });
            }
            Kind::Punct(')' | ']' | '}') => {
                stack.pop();
            }
            Kind::Str(s) => {
                let Some(top) = stack.last() else { continue };
                if top.ch == '{' {
                    continue;
                }
                let prev_ok = i > 0 && matches!(toks[i - 1].kind, Kind::Punct('(' | ',' | '['));
                let next_ok = matches!(
                    toks.get(i + 1).map(|t| &t.kind),
                    Some(Kind::Punct(')' | ',' | ']'))
                );
                if !prev_ok || !next_ok || !looks_like_ui_text(s) {
                    continue;
                }
                let denied = match &top.ident {
                    Some(n) if top.is_macro => n != "format",
                    Some(_) if top.shared_from => false,
                    Some(n) => NOT_UI_CALLS.contains(&n.as_str()),
                    None => false,
                };
                if denied {
                    continue;
                }
                if ignore.contains(&t.line) || ignore.contains(&(t.line.saturating_sub(1))) {
                    continue;
                }
                out.push(Unwrapped {
                    file: file.to_owned(),
                    line: t.line,
                    text: s.clone(),
                });
            }
            _ => {}
        }
    }
    out
}

/// The workspace root, from this crate's location (`crates/cm-i18n`).
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .components()
        .collect()
}

/// `locales/capturemann.pot`, relative to the root.
pub const POT_FILE: &str = "locales/capturemann.pot";
/// The ratchet: the number of unwrapped UI literals today (`locales/unwrapped-baseline.txt`).
pub const BASELINE_FILE: &str = "locales/unwrapped-baseline.txt";

/// Reads the baseline (the first line that is a number; `#` lines are comments).
pub fn read_baseline(root: &Path) -> Option<usize> {
    std::fs::read_to_string(root.join(BASELINE_FILE))
        .ok()?
        .lines()
        .find_map(|l| l.trim().parse().ok())
}

/// The text of the baseline file for a count.
pub fn baseline_text(count: usize) -> String {
    format!(
        "# Unwrapped UI string literals in apps/desktop/src (heuristic of cm_i18n::extract).\n\
         # May only go down. Wrap strings in tr!, then run `cargo run -p cm-i18n -- baseline`.\n\
         {count}\n"
    )
}

/// The shipped catalogues on disk (`locales/<code>.po`), parsed.
pub fn load_catalogues(root: &Path) -> Result<Vec<(Lang, Catalogue)>, Problem> {
    let mut out = Vec::new();
    for lang in Lang::ALL {
        if lang == Lang::En {
            continue;
        }
        let name = format!("locales/{}.po", lang.code());
        let text = std::fs::read_to_string(root.join(&name)).map_err(|e| Problem {
            file: name.clone(),
            line: 0,
            message: format!("cannot read: {e}"),
        })?;
        let cat = po::parse(&text).map_err(|e| Problem {
            file: name.clone(),
            line: e.line,
            message: e.message,
        })?;
        out.push((lang, cat));
    }
    Ok(out)
}

/// Every check at once: scan problems, a stale template, and each catalogue.
pub fn check_all(root: &Path) -> Vec<Problem> {
    let scan = scan_workspace(root);
    let mut out = scan.problems;
    let want = render_pot(&scan.messages);
    match std::fs::read_to_string(root.join(POT_FILE)) {
        Ok(have) if have == want => {}
        Ok(_) => out.push(Problem {
            file: POT_FILE.to_owned(),
            line: 0,
            message: "stale: run `cargo run -p cm-i18n -- extract` and commit the result".into(),
        }),
        Err(e) => out.push(Problem {
            file: POT_FILE.to_owned(),
            line: 0,
            message: format!("cannot read: {e}"),
        }),
    }
    match load_catalogues(root) {
        Ok(cats) => {
            for (lang, cat) in &cats {
                let name = format!("locales/{}.po", lang.code());
                out.extend(check_catalogue(&name, &scan.messages, cat, *lang));
            }
        }
        Err(p) => out.push(p),
    }
    out
}

/// All unwrapped UI literals in `apps/desktop/src` (files named `*tests.rs` are skipped).
pub fn count_unwrapped(root: &Path) -> Vec<Unwrapped> {
    let mut files = Vec::new();
    walk(&root.join("apps/desktop/src"), &mut files);
    let mut out = Vec::new();
    for path in files {
        let name = rel(root, &path);
        if name.ends_with("tests.rs") || name.contains("/tests/") {
            continue;
        }
        if let Ok(src) = std::fs::read_to_string(&path) {
            out.extend(unwrapped_in_source(&name, &src));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_all_three_macros_with_escapes_and_continuations() {
        let src = r#"
            fn f() {
                let a = tr!("Copy link");
                let b = cm_i18n::tr!("Delete {title}?", title = t);
                let c = tr_ctx!("button", "Open");
                let d = trn!("{n} recording", "{n} recordings", n);
                let e = tr!("Couldn\u{2019}t save \"it\"");
                let g = tr!("One \
                             sentence");
                // tr!("in a comment")
                let h = "tr!(\"in a string\")";
                let i = str!("not ours");
            }
        "#;
        let (msgs, problems) = scan_source("x.rs", src);
        assert!(problems.is_empty(), "{problems:?}");
        let ids: Vec<&str> = msgs.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "Copy link",
                "Delete {title}?",
                "Open",
                "{n} recording",
                "Couldn\u{2019}t save \"it\"",
                "One sentence"
            ]
        );
        assert_eq!(msgs[2].ctx.as_deref(), Some("button"));
        assert_eq!(msgs[3].plural.as_deref(), Some("{n} recordings"));
    }

    #[test]
    fn a_non_literal_is_reported_and_definitions_are_ignored() {
        let (msgs, problems) = scan_source(
            "x.rs",
            "macro_rules! tr { () => {} }\nfn f() { tr!(name); trn!(\"a\", b, 1); }",
        );
        assert!(msgs.is_empty());
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn the_template_is_sorted_and_stable() {
        let (msgs, _) = scan_source(
            "x.rs",
            "fn f(){ tr!(\"B\"); tr!(\"A\"); trn!(\"{n} x\",\"{n} xs\",1); }",
        );
        let a = render_pot(&msgs);
        let mut rev = msgs.clone();
        rev.reverse();
        assert_eq!(a, render_pot(&rev));
        let cat = po::parse(&a).unwrap();
        assert_eq!(cat.entries.len(), 3);
        assert!(a.find("msgid \"A\"").unwrap() < a.find("msgid \"B\"").unwrap());
        assert!(a.contains("msgstr[1] \"\""));
    }

    fn msgs(src: &str) -> Vec<Message> {
        scan_source("t.rs", src).0
    }

    fn notes_of<'a>(m: &'a [Message], id: &str) -> &'a [String] {
        &m.iter().find(|m| m.id == id).unwrap().notes
    }

    /// A `// TRANSLATORS:` comment reaches the template as `#. ` above the message it is about.
    #[test]
    fn translator_comments_become_extracted_comments() {
        let src = r#"
            // TRANSLATORS: FACT: owner review required (privacy)
            pub fn privacy() -> String {
                tr!("No telemetry")
            }

            fn rows() {
                let a = Spec {
                    // TRANSLATORS: FACT: owner review required
                    // (what the permission is for)
                    why: tr!("Records your voice."),
                    name: tr_ctx!("permission", "Microphone"),
                };
                let b = tr!("Plain");
            }

            // TRANSLATORS: Shown on a button; keep it short.
            fn two() {
                let _ = trn!("{n} file", "{n} files", 1);
                let _ = tr!("Second");
            }

            // TRANSLATORS: nothing follows
            const X: u8 = 1;
            fn after() { tr!("After"); }
        "#;
        let (m, problems) = scan_source("x.rs", src);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            notes_of(&m, "No telemetry"),
            ["FACT: owner review required (privacy)"]
        );
        // A comment inside a literal is about the next call only; a continuation line joins.
        assert_eq!(
            notes_of(&m, "Records your voice."),
            ["FACT: owner review required (what the permission is for)"]
        );
        assert!(notes_of(&m, "Microphone").is_empty());
        assert!(notes_of(&m, "Plain").is_empty());
        // A comment above a block is about every message in it.
        let keep = ["Shown on a button; keep it short."];
        assert_eq!(notes_of(&m, "{n} file"), keep);
        assert_eq!(notes_of(&m, "Second"), keep);
        // A comment that is not above anything holding a message is dropped.
        assert!(notes_of(&m, "After").is_empty());
        let pot = render_pot(&m);
        assert!(
            pot.contains("#. FACT: owner review required (privacy)\nmsgid \"No telemetry\""),
            "{pot}"
        );
        // The template stays parseable, and keeps the notes.
        let cat = po::parse(&pot).unwrap();
        let e = cat.get(None, "No telemetry").unwrap();
        assert_eq!(e.notes, ["FACT: owner review required (privacy)"]);
    }

    /// The same message in two places keeps both notes, once each, whatever the file order.
    #[test]
    fn the_notes_of_one_message_from_two_places_are_merged() {
        let dir = std::env::temp_dir().join(format!("cmi18n-notes-{}", std::process::id()));
        let src = dir.join("apps/x/src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("a.rs"),
            "fn a() {\n// TRANSLATORS: one\nlet _ = tr!(\"Same\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            src.join("b.rs"),
            "fn b() {\n// TRANSLATORS: one\nlet _ = tr!(\"Same\");\n// TRANSLATORS: two\nlet _ = tr!(\"Same\");\n}\n",
        )
        .unwrap();
        let scan = scan_workspace(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(scan.messages.len(), 1);
        assert_eq!(scan.messages[0].notes, ["one", "two"]);
    }

    #[test]
    fn checks_catch_stale_duplicate_placeholder_and_plural_problems() {
        let template = msgs(
            "fn f(){ tr!(\"Delete {title}?\"); tr!(\"Hello\"); trn!(\"{n} file\",\"{n} files\",n); }",
        );
        let text = "msgid \"\"\nmsgstr \"\"\n\"Language: vi\\n\"\n\"Plural-Forms: nplurals=1; plural=0;\\n\"\n\n\
            msgid \"Delete {title}?\"\nmsgstr \"Xóa {name}?\"\n\n\
            msgid \"Hello\"\nmsgstr \"Xin chào\"\n\n\
            msgid \"Hello\"\nmsgstr \"Chào\"\n\n\
            msgid \"Gone\"\nmsgstr \"Đã xóa\"\n\n\
            msgid \"{n} file\"\nmsgid_plural \"{n} files\"\nmsgstr[0] \"\"\n";
        let cat = po::parse(text).unwrap();
        let probs: Vec<String> = check_catalogue("vi.po", &template, &cat, Lang::Vi)
            .into_iter()
            .map(|p| p.message)
            .collect();
        let has = |s: &str| probs.iter().any(|p| p.contains(s));
        assert!(has("duplicate msgid `Hello`"), "{probs:?}");
        assert!(has("stale msgid `Gone`"), "{probs:?}");
        assert!(has("unknown placeholder {name}"), "{probs:?}");
        // An untranslated plural entry is fine; it is just not covered.
        assert!(!has("{n} file"), "{probs:?}");
        let c = coverage(&template, &cat);
        assert_eq!((c.total, c.translated), (3, 1 + 1));
    }

    #[test]
    fn a_missing_placeholder_and_empty_plural_forms_are_reported() {
        let template =
            msgs("fn f(){ tr!(\"Delete {title}?\"); trn!(\"{n} file\",\"{n} files\",n); }");
        let text = "msgid \"\"\nmsgstr \"\"\n\"Language: en\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\n\
            msgid \"Delete {title}?\"\nmsgstr \"Delete?\"\n\n\
            msgid \"{n} file\"\nmsgid_plural \"{n} files\"\nmsgstr[0] \"{n} tệp\"\nmsgstr[1] \"\"\n";
        let cat = po::parse(text).unwrap();
        let probs: Vec<String> = check_catalogue("en.po", &template, &cat, Lang::En)
            .into_iter()
            .map(|p| p.message)
            .collect();
        assert!(
            probs.iter().any(|p| p.contains("placeholders differ")),
            "{probs:?}"
        );
        assert!(
            probs.iter().any(|p| p.contains("empty plural form")),
            "{probs:?}"
        );
    }

    #[test]
    fn the_unwrapped_heuristic_counts_ui_literals_only() {
        let src = r#"
            fn view() {
                div().child("Library folder");                 // 1
                div().child(tr!("Library folder"));            // wrapped
                settings_row(t, tile, "Theme", None);          // 2
                let x = ("System", Pref::System, "system");    // 3
                x.expect("Something failed");                  // denied call
                println!("Hello world");                       // macro
                let y = format!("Saved {n} files");            // 4
                let z = SharedString::from("Copy link");       // 5
                id("CaptureMann");                             // single CamelCase token
                f("a lowercase thing");
                f("Ignored text"); // i18n-ignore
                json!({"key": "Value"});
            }
            #[cfg(test)]
            mod tests { fn t() { f("Test only text"); } }
        "#;
        let hits = unwrapped_in_source("x.rs", src);
        let texts: Vec<&str> = hits.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Library folder",
                "Theme",
                "System",
                "Saved {n} files",
                "Copy link"
            ]
        );
    }
}
