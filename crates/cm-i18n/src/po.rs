//! A small reader and writer for the gettext `.po` subset we use.
//!
//! Supported: `msgctxt`, `msgid`, `msgid_plural`, `msgstr`, `msgstr[N]`, strings split over several
//! lines, translator (`# `), extracted (`#. `) and flag (`#, `) comments. Obsolete entries (`#~`)
//! and reference comments (`#:`) are skipped. The header is the entry with an empty `msgid`.

use std::collections::HashMap;
use std::fmt;

/// One message of a catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub ctx: Option<String>,
    pub id: String,
    pub plural: Option<String>,
    /// `msgstr` (one element) or `msgstr[0..]`.
    pub strs: Vec<String>,
    /// `#, fuzzy` and friends.
    pub flags: Vec<String>,
    /// `#. ` extracted comments.
    pub notes: Vec<String>,
    /// Line of the `msgid` in the file (1-based), for messages about it.
    pub line: usize,
}

impl Entry {
    /// The lookup key (`ctx` + EOT + `id`, as gettext does).
    pub fn key(&self) -> String {
        key(self.ctx.as_deref(), &self.id)
    }

    pub fn is_fuzzy(&self) -> bool {
        self.flags.iter().any(|f| f == "fuzzy")
    }

    /// Whether any form has text.
    pub fn is_translated(&self) -> bool {
        self.strs.iter().any(|s| !s.is_empty())
    }
}

/// The lookup key of a message.
pub fn key(ctx: Option<&str>, id: &str) -> String {
    match ctx {
        Some(c) => format!("{c}\u{4}{id}"),
        None => id.to_owned(),
    }
}

/// A parsed catalogue.
#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    /// Header lines as `(name, value)`, e.g. `("Language", "vi")`.
    pub header: Vec<(String, String)>,
    /// Comment lines before the header (`# DRAFT: ...`), without the leading `# `.
    pub header_comments: Vec<String>,
    pub entries: Vec<Entry>,
    index: HashMap<String, usize>,
}

impl Catalogue {
    pub fn get(&self, ctx: Option<&str>, id: &str) -> Option<&Entry> {
        self.index.get(&key(ctx, id)).map(|&i| &self.entries[i])
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.header
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Keys that occur more than once (the first occurrence wins at run time).
    pub fn duplicates(&self) -> Vec<&Entry> {
        let mut seen = std::collections::HashSet::new();
        self.entries
            .iter()
            .filter(|e| !seen.insert(e.key()))
            .collect()
    }
}

/// A syntax error with its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Ctx,
    Id,
    Plural,
    Str(usize),
}

fn unquote(s: &str, line: usize) -> Result<String, ParseError> {
    let err = |m: &str| ParseError {
        line,
        message: m.to_owned(),
    };
    let s = s.trim();
    let inner = s
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .ok_or_else(|| err("expected a quoted string"))?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(o) => return Err(err(&format!("unknown escape \\{o}"))),
            None => return Err(err("dangling backslash")),
        }
    }
    Ok(out)
}

/// Escapes a string for a `.po` line (without the quotes).
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// Parses a `.po` or `.pot` file.
pub fn parse(text: &str) -> Result<Catalogue, ParseError> {
    let mut cat = Catalogue::default();
    let mut cur = Entry::default();
    let mut started = false; // any msg* line seen for `cur`
    let mut field: Option<Field> = None;
    let mut ctx_seen = false;
    let mut before_header = true;
    let mut pending_comments: Vec<String> = Vec::new();

    fn finish(cat: &mut Catalogue, cur: &mut Entry, started: &mut bool, ctx_seen: &mut bool) {
        if !*started {
            return;
        }
        let e = std::mem::take(cur);
        if e.id.is_empty() && e.ctx.is_none() && e.plural.is_none() {
            // The header: "Name: value\n" lines in msgstr.
            if cat.header.is_empty() {
                for l in e.strs.first().map(String::as_str).unwrap_or("").lines() {
                    if let Some((k, v)) = l.split_once(':') {
                        cat.header.push((k.trim().to_owned(), v.trim().to_owned()));
                    }
                }
            }
        } else {
            let i = cat.entries.len();
            cat.index.entry(e.key()).or_insert(i);
            cat.entries.push(e);
        }
        *started = false;
        *ctx_seen = false;
    }

    for (n, raw) in text.lines().enumerate() {
        let line_no = n + 1;
        let line = raw.trim();
        if line.is_empty() {
            finish(&mut cat, &mut cur, &mut started, &mut ctx_seen);
            field = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            if rest.starts_with('~') || rest.starts_with(':') {
                continue;
            }
            // A comment after msgstr lines starts the next entry.
            if started && matches!(field, Some(Field::Str(_))) {
                finish(&mut cat, &mut cur, &mut started, &mut ctx_seen);
                field = None;
            }
            if let Some(f) = rest.strip_prefix(',') {
                cur.flags.extend(
                    f.split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty()),
                );
            } else if let Some(nt) = rest.strip_prefix('.') {
                cur.notes.push(nt.trim().to_owned());
            } else if before_header {
                pending_comments.push(rest.trim().to_owned());
            }
            continue;
        }
        if line.starts_with('"') {
            let s = unquote(line, line_no)?;
            match field {
                Some(Field::Ctx) => cur.ctx.get_or_insert_with(String::new).push_str(&s),
                Some(Field::Id) => cur.id.push_str(&s),
                Some(Field::Plural) => cur.plural.get_or_insert_with(String::new).push_str(&s),
                Some(Field::Str(i)) => {
                    if let Some(t) = cur.strs.get_mut(i) {
                        t.push_str(&s);
                    }
                }
                None => {
                    return Err(ParseError {
                        line: line_no,
                        message: "string continuation without a keyword".into(),
                    })
                }
            }
            continue;
        }
        let (kw, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let (name, idx) = match kw.split_once('[') {
            Some((n, r)) => (n, r.strip_suffix(']').and_then(|d| d.parse::<usize>().ok())),
            None => (kw, None),
        };
        match name {
            "msgctxt" => {
                finish(&mut cat, &mut cur, &mut started, &mut ctx_seen);
                started = true;
                ctx_seen = true;
                cur.ctx = Some(unquote(rest, line_no)?);
                field = Some(Field::Ctx);
            }
            "msgid" => {
                if started && !ctx_seen || started && !cur.id.is_empty() || !cur.strs.is_empty() {
                    finish(&mut cat, &mut cur, &mut started, &mut ctx_seen);
                }
                if !started || cur.line == 0 {
                    cur.line = line_no;
                }
                started = true;
                cur.id = unquote(rest, line_no)?;
                field = Some(Field::Id);
                if cat.header.is_empty() && cat.entries.is_empty() && cur.id.is_empty() {
                    cat.header_comments = std::mem::take(&mut pending_comments);
                }
            }
            "msgid_plural" => {
                cur.plural = Some(unquote(rest, line_no)?);
                field = Some(Field::Plural);
            }
            "msgstr" => {
                let i = idx.unwrap_or(0);
                if cur.strs.len() <= i {
                    cur.strs.resize(i + 1, String::new());
                }
                cur.strs[i] = unquote(rest, line_no)?;
                field = Some(Field::Str(i));
                before_header = false;
            }
            other => {
                return Err(ParseError {
                    line: line_no,
                    message: format!("unknown keyword `{other}`"),
                })
            }
        }
    }
    finish(&mut cat, &mut cur, &mut started, &mut ctx_seen);
    Ok(cat)
}

fn push_field(out: &mut String, keyword: &str, value: &str) {
    out.push_str(keyword);
    out.push_str(" \"");
    out.push_str(&escape(value));
    out.push_str("\"\n");
}

/// Writes an entry in the layout `msgcat` would: flags, notes, then the fields.
pub fn write_entry(out: &mut String, e: &Entry) {
    for n in &e.notes {
        out.push_str("#. ");
        out.push_str(n);
        out.push('\n');
    }
    if !e.flags.is_empty() {
        out.push_str("#, ");
        out.push_str(&e.flags.join(", "));
        out.push('\n');
    }
    if let Some(c) = &e.ctx {
        push_field(out, "msgctxt", c);
    }
    push_field(out, "msgid", &e.id);
    match &e.plural {
        Some(p) => {
            push_field(out, "msgid_plural", p);
            for (i, s) in e.strs.iter().enumerate() {
                push_field(out, &format!("msgstr[{i}]"), s);
            }
        }
        None => push_field(
            out,
            "msgstr",
            e.strs.first().map(String::as_str).unwrap_or(""),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# DRAFT: owner review pending
msgid ""
msgstr ""
"Language: vi\n"
"Plural-Forms: nplurals=1; plural=0;\n"

#. FACT
#, fuzzy
msgid "Delete {title}?"
msgstr "Xóa {title}?"

msgctxt "button"
msgid "Open"
msgstr "Mở"

msgid "{n} recording"
msgid_plural "{n} recordings"
msgstr[0] "{n} bản ghi"

msgid "Two "
"lines\n"
msgstr ""
"Hai "
"dòng\n"

#~ msgid "Old"
#~ msgstr "Cũ"
"#;

    #[test]
    fn parses_header_entries_context_plural_and_continuations() {
        let c = parse(SAMPLE).unwrap();
        assert_eq!(c.header_value("Language"), Some("vi"));
        assert_eq!(
            c.header_value("plural-forms"),
            Some("nplurals=1; plural=0;")
        );
        assert_eq!(c.header_comments, vec!["DRAFT: owner review pending"]);
        assert_eq!(c.entries.len(), 4);
        let d = c.get(None, "Delete {title}?").unwrap();
        assert_eq!(d.strs, vec!["Xóa {title}?"]);
        assert!(d.is_fuzzy() && d.notes == vec!["FACT"]);
        assert_eq!(c.get(Some("button"), "Open").unwrap().strs, vec!["Mở"]);
        assert!(c.get(None, "Open").is_none(), "context is part of the key");
        let p = c.get(None, "{n} recording").unwrap();
        assert_eq!(p.plural.as_deref(), Some("{n} recordings"));
        assert_eq!(p.strs, vec!["{n} bản ghi"]);
        assert_eq!(c.get(None, "Two lines\n").unwrap().strs, vec!["Hai dòng\n"]);
        assert!(c.get(None, "Old").is_none(), "obsolete entries are skipped");
    }

    #[test]
    fn duplicates_are_reported_and_the_first_wins() {
        let c = parse("msgid \"A\"\nmsgstr \"1\"\n\nmsgid \"A\"\nmsgstr \"2\"\n").unwrap();
        assert_eq!(c.duplicates().len(), 1);
        assert_eq!(c.get(None, "A").unwrap().strs, vec!["1"]);
    }

    #[test]
    fn syntax_errors_carry_the_line() {
        let e = parse("msgid \"A\"\nmsgstr oops\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(parse("bogus \"x\"\n").is_err());
        assert!(parse("\"stray\"\n").is_err());
    }

    #[test]
    fn write_then_parse_round_trips() {
        let c = parse(SAMPLE).unwrap();
        let mut out = String::new();
        for e in &c.entries {
            write_entry(&mut out, e);
            out.push('\n');
        }
        let back = parse(&out).unwrap();
        assert_eq!(back.entries.len(), c.entries.len());
        for (a, b) in c.entries.iter().zip(&back.entries) {
            assert_eq!(
                (a.ctx.as_ref(), &a.id, &a.plural, &a.strs, &a.flags),
                (b.ctx.as_ref(), &b.id, &b.plural, &b.strs, &b.flags)
            );
        }
    }
}
