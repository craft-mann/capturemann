//! Layout guard for long translations (see `locales/README.md`, "Layout: long translations").
//!
//! GPUI measures a text element with no wrapping when its parent asks for the minimum content
//! size, so a flex item that holds text can never be narrower than its whole text unless it says
//! so (`min_w(px(0.))`, `truncate`, `overflow_hidden`). In English the text happens to fit; a
//! longer translation then runs out of its card. This test reads the sources (no window) and
//! fails for the two shapes that caused it:
//!
//! * **L1**: a `.flex_1()` child of a flex row that holds text and has no shrink rule in its own
//!   builder chain (`min_w`, `truncate`, `overflow_hidden`, a fixed `w`).
//! * **L2**: a bare text child of a flex row that has other children (an icon, a dot, a value)
//!   and does not shrink or clip: wrap the text as `div().flex_1().min_w(px(0.)).child(text)`
//!   (`components::label_fill`).
//!
//! Justified exceptions go into `tests/layout_guard_allow.txt`, one per line:
//! `L1|path|snippet  # why`. The snippet is the flagged child exactly as the failure prints it
//! (whitespace collapsed). A stale entry (nothing matches it) fails too, so the list cannot rot.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Id(String),
    P(char),
    Str,
    Open(char),
    Close(char),
}

struct Lexed {
    toks: Vec<Tok>,
    /// Byte offset of each token start and the line, for the report.
    pos: Vec<(usize, usize)>,
    mate: Vec<usize>,
}

fn lex(src: &str) -> Lexed {
    let b: Vec<char> = src.chars().collect();
    let (mut toks, mut pos) = (Vec::new(), Vec::new());
    let (mut i, mut line) = (0usize, 1usize);
    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == '*' && b.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if b[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if c == '"'
            || (c == 'r' && matches!(b.get(i + 1), Some('"') | Some('#')) && raw_start(&b, i))
        {
            let start_line = line;
            if c == 'r' {
                let mut j = i + 1;
                let mut hashes = 0;
                while b[j] == '#' {
                    hashes += 1;
                    j += 1;
                }
                j += 1;
                loop {
                    if b[j] == '\n' {
                        line += 1;
                    }
                    if b[j] == '"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&'#')) {
                        i = j + 1 + hashes;
                        break;
                    }
                    j += 1;
                }
            } else {
                i += 1;
                while b[i] != '"' {
                    if b[i] == '\\' {
                        i += 1;
                    }
                    if b[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            toks.push(Tok::Str);
            pos.push((i, start_line));
        } else if c == '\'' {
            // A char literal or a lifetime.
            if b.get(i + 1) == Some(&'\\') {
                i += 2;
                while b[i] != '\'' {
                    i += 1;
                }
                i += 1;
                toks.push(Tok::Str);
                pos.push((i, line));
            } else if b.get(i + 2) == Some(&'\'') {
                i += 3;
                toks.push(Tok::Str);
                pos.push((i, line));
            } else {
                i += 1;
            }
        } else if c.is_alphanumeric() || c == '_' {
            let s = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            // `0.` / `1.5` stay one number so the dot is not a method call.
            if b[s].is_ascii_digit() {
                while i < b.len()
                    && (b[i].is_alphanumeric()
                        || b[i] == '_'
                        || (b[i] == '.' && b.get(i + 1).is_some_and(|n| n.is_ascii_digit())))
                {
                    i += 1;
                }
                if b.get(i) == Some(&'.')
                    && !b
                        .get(i + 1)
                        .is_some_and(|n| n.is_alphabetic() || *n == '.' || *n == '_')
                {
                    i += 1;
                }
            }
            toks.push(Tok::Id(b[s..i].iter().collect()));
            pos.push((s, line));
        } else if "([{".contains(c) {
            toks.push(Tok::Open(c));
            pos.push((i, line));
            i += 1;
        } else if ")]}".contains(c) {
            toks.push(Tok::Close(c));
            pos.push((i, line));
            i += 1;
        } else {
            toks.push(Tok::P(c));
            pos.push((i, line));
            i += 1;
        }
    }
    let mut mate = vec![usize::MAX; toks.len()];
    let mut stack = Vec::new();
    for (k, t) in toks.iter().enumerate() {
        match t {
            Tok::Open(_) => stack.push(k),
            Tok::Close(_) => {
                if let Some(o) = stack.pop() {
                    mate[o] = k;
                    mate[k] = o;
                }
            }
            _ => {}
        }
    }
    Lexed { toks, pos, mate }
}

fn raw_start(b: &[char], i: usize) -> bool {
    // `r"`, `r#"`: not the tail of an identifier.
    if i > 0 && (b[i - 1].is_alphanumeric() || b[i - 1] == '_') {
        return false;
    }
    let mut j = i + 1;
    while b.get(j) == Some(&'#') {
        j += 1;
    }
    b.get(j) == Some(&'"')
}

/// Token index ranges `[from, to)` of `#[cfg(test)]` items, which the guard skips.
fn test_ranges(l: &Lexed) -> Vec<(usize, usize)> {
    let t = &l.toks;
    let mut out = Vec::new();
    let mut i = 0;
    while i + 6 < t.len() {
        let is_cfg_test = t[i] == Tok::P('#')
            && t[i + 1] == Tok::Open('[')
            && t[i + 2] == Tok::Id("cfg".into())
            && t[i + 3] == Tok::Open('(')
            && t[i + 4] == Tok::Id("test".into())
            && t[i + 5] == Tok::Close(')');
        if is_cfg_test {
            let mut j = l.mate[i + 1] + 1;
            while j < t.len() && !matches!(t[j], Tok::Open('{') | Tok::P(';')) {
                if let Tok::Open(_) = t[j] {
                    j = l.mate[j];
                }
                j += 1;
            }
            let end = if j < t.len() && matches!(t[j], Tok::Open('{')) {
                l.mate[j]
            } else {
                j
            };
            out.push((i, end + 1));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

#[derive(Debug)]
struct Method {
    name: String,
    /// Token index of the opening bracket of the arguments (or `usize::MAX` for none).
    open: usize,
}

#[derive(Debug)]
struct Chain {
    root: String,
    start: usize,
    end: usize,
    methods: Vec<Method>,
}

impl Chain {
    fn has(&self, name: &str) -> bool {
        self.methods.iter().any(|m| m.name == name)
    }
    fn has_any(&self, names: &[&str]) -> bool {
        names.iter().any(|n| self.has(n))
    }
    fn is_row(&self) -> bool {
        self.root == "div" && self.has("flex") && !self.has_any(&["flex_col", "flex_col_reverse"])
    }
}

const WHEN: [&str; 4] = ["when", "when_some", "map", "when_else"];

fn parse_chain(l: &Lexed, s: usize, variable_root: bool) -> Option<Chain> {
    let t = &l.toks;
    let Some(Tok::Id(first)) = t.get(s) else {
        return None;
    };
    let mut root = first.clone();
    let mut i = s + 1;
    while t.get(i) == Some(&Tok::P(':')) && t.get(i + 1) == Some(&Tok::P(':')) {
        if let Some(Tok::Id(n)) = t.get(i + 2) {
            root = n.clone();
            i += 3;
        } else {
            break;
        }
    }
    let mut methods = Vec::new();
    if t.get(i) == Some(&Tok::P('!')) {
        return None;
    }
    if matches!(t.get(i), Some(Tok::Open('('))) {
        i = l.mate[i] + 1;
    } else if !(variable_root && t.get(i) == Some(&Tok::P('.'))) {
        return None;
    }
    loop {
        if t.get(i) == Some(&Tok::P('?')) {
            i += 1;
        }
        if t.get(i) == Some(&Tok::P('.')) {
            if let Some(Tok::Id(name)) = t.get(i + 1) {
                let mut j = i + 2;
                // `.collect::<Vec<_>>()` turbofish: skip to the call.
                if t.get(j) == Some(&Tok::P(':')) {
                    while j < t.len() && !matches!(t[j], Tok::Open('(')) {
                        j += 1;
                    }
                }
                if matches!(t.get(j), Some(Tok::Open('('))) {
                    methods.push(Method {
                        name: name.clone(),
                        open: j,
                    });
                    i = l.mate[j] + 1;
                    continue;
                }
                // A field access (`self.x`): part of the expression, not a call.
                i += 2;
                continue;
            }
        }
        break;
    }
    Some(Chain {
        root,
        start: s,
        end: i,
        methods,
    })
}

/// Methods of the closure bodies of `when`, `when_some`, `map`: they act on the same element.
fn merged(l: &Lexed, c: &mut Chain) {
    let mut extra = Vec::new();
    for m in &c.methods {
        if !WHEN.contains(&m.name.as_str()) {
            continue;
        }
        let close = l.mate[m.open];
        // The closure body starts after the second `|` at depth 0.
        let mut k = m.open + 1;
        let mut pipes = 0;
        while k < close {
            match l.toks[k] {
                Tok::Open(_) => k = l.mate[k],
                Tok::P('|') => {
                    pipes += 1;
                    if pipes == 2 {
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        if pipes == 2 && k + 1 < close {
            let mut body = k + 1;
            if matches!(l.toks[body], Tok::Open('{')) {
                body += 1;
            }
            if let Some(mut inner) = parse_chain(l, body, true) {
                merged(l, &mut inner);
                extra.append(&mut inner.methods);
            }
        }
    }
    c.methods.append(&mut extra);
}

const TEXT_NAMES: [&str; 24] = [
    "label",
    "text",
    "title",
    "name",
    "body",
    "hint",
    "caption",
    "desc",
    "description",
    "msg",
    "message",
    "detail",
    "note",
    "summary",
    "sub",
    "subtitle",
    "heading",
    "prefix",
    "tag",
    "headline",
    "question",
    "reason",
    "explanation",
    "verb",
];

const ICON_ROOTS: [&str; 12] = [
    "icon",
    "svg",
    "dot",
    "rec_dot",
    "keycap",
    "keycaps",
    "keycap_lg",
    "logo",
    "badge",
    "status_pill",
    "switch",
    "icon_button",
];

#[derive(Debug)]
enum Item {
    Text(usize, usize),
    Icon,
    /// A `div()` chain, by index.
    Div(usize),
    /// A bare variable or field path.
    Var,
    /// Anything else (a call to a helper that builds an element).
    Other,
}

struct File {
    path: String,
    src: String,
    l: Lexed,
    chains: Vec<Chain>,
}

impl File {
    fn text_of(&self, from: usize, to: usize) -> String {
        let (a, b) = (
            self.l.pos[from].0,
            self.l.pos[to.min(self.l.toks.len() - 1)].0,
        );
        let chars: Vec<char> = self.src.chars().collect();
        let s: String = chars[a..b.max(a)].iter().collect();
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn chain_at(&self, start: usize) -> Option<usize> {
        self.chains.iter().position(|c| c.start == start)
    }

    /// The children of a chain: arguments of `.child(..)` and `.children(..)`.
    fn items(&self, c: &Chain) -> Vec<Item> {
        let mut out = Vec::new();
        for m in &c.methods {
            if m.name != "child" && m.name != "children" {
                continue;
            }
            let (from, to) = (m.open + 1, self.l.mate[m.open]);
            out.push(self.classify(from, to));
        }
        out
    }

    fn classify(&self, from: usize, mut to: usize) -> Item {
        let t = &self.l.toks;
        if to > from && t[to - 1] == Tok::P(',') {
            to -= 1;
        }
        if from >= to {
            return Item::Other;
        }
        // A chain that spans the whole argument.
        if let Some(ix) = self.chain_at(from) {
            let c = &self.chains[ix];
            if c.end >= to {
                if ICON_ROOTS.contains(&c.root.as_str()) {
                    return Item::Icon;
                }
                if c.root == "div" {
                    return Item::Div(ix);
                }
                if !matches!(c.root.as_str(), "from" | "new") {
                    return Item::Other;
                }
            }
        }
        let mut macro_text = false;
        let mut has_div = false;
        // Only what sits at the top of the argument counts: `button(.., tr!(..))` is a button.
        let mut k = from;
        while k < to {
            match &t[k] {
                Tok::Id(n)
                    if matches!(n.as_str(), "tr" | "tr_ctx" | "trn" | "format")
                        && t.get(k + 1) == Some(&Tok::P('!')) =>
                {
                    macro_text = true;
                    k += 2;
                }
                Tok::Id(n) if n == "div" && matches!(t.get(k + 1), Some(Tok::Open('('))) => {
                    has_div = true;
                }
                Tok::Id(n) if n == "SharedString" || n == "String" => {
                    macro_text = true;
                    while k < to && !matches!(t[k], Tok::Open(_)) {
                        k += 1;
                    }
                }
                // `if c { tr!("a") } else { tr!("b") }`: look inside braces too.
                Tok::Open('{') => {
                    k += 1;
                    continue;
                }
                _ => {}
            }
            if matches!(t[k], Tok::Open('(') | Tok::Open('[')) {
                k = self.l.mate[k];
            }
            k += 1;
        }
        if has_div {
            return Item::Other;
        }
        if macro_text || (to - from == 1 && t[from] == Tok::Str) {
            return Item::Text(from, to);
        }
        // `label`, `self.label`, `x.title.clone()`, `title.into()`: the last name decides.
        let mut last = None;
        let mut k = from;
        while k < to {
            match &t[k] {
                Tok::Id(n) => last = Some(n.as_str()),
                Tok::Open(_) => k = self.l.mate[k],
                _ => {}
            }
            k += 1;
        }
        let mut names: Vec<&str> = Vec::new();
        let mut depth_ok = true;
        #[allow(clippy::needless_range_loop)]
        for k in from..to {
            match &t[k] {
                Tok::Id(n) => names.push(n),
                Tok::Open(_) | Tok::Close(_) | Tok::P('.') | Tok::P('&') | Tok::P('*') => {}
                Tok::P(_) | Tok::Str => depth_ok = false,
            }
        }
        // Skip trailing `clone`, `into`, `to_owned`, `as_str`, `unwrap_or_default`.
        let core: Vec<&&str> = names
            .iter()
            .filter(|n| {
                !matches!(
                    **n,
                    "clone" | "into" | "to_owned" | "to_string" | "as_str" | "self"
                )
            })
            .collect();
        if depth_ok {
            if let Some(n) = core.last() {
                if TEXT_NAMES.contains(n)
                    || n.ends_with("_label")
                    || n.ends_with("_text")
                    || n.ends_with("_title")
                {
                    return Item::Text(from, to);
                }
            }
        }
        let _ = last;
        // A lone variable or field path (`n`, `self.body`): text or an element we cannot see. It
        // counts as text where a `flex_1` wrapper holds it (L1), not as a sibling (L2).
        if names.len() == core.len() && depth_ok && !names.is_empty() {
            return Item::Var;
        }
        Item::Other
    }

    /// Does any text sit in this chain or below it (through `div()` children)?
    fn holds_text(&self, ix: usize, depth: usize) -> bool {
        if depth > 12 {
            return false;
        }
        let c = &self.chains[ix];
        self.items(c).iter().any(|it| match it {
            Item::Text(..) | Item::Var => true,
            Item::Div(j) => self.holds_text(*j, depth + 1),
            _ => false,
        })
    }
}

fn load(path: &Path, rel: &str) -> File {
    let src = std::fs::read_to_string(path).expect("read source");
    let l = lex(&src);
    let skip = test_ranges(&l);
    let mut chains = Vec::new();
    let t = &l.toks;
    for s in 0..t.len() {
        if skip.iter().any(|&(a, b)| s >= a && s < b) {
            continue;
        }
        if !matches!(t[s], Tok::Id(_)) {
            continue;
        }
        if s > 0 {
            match &t[s - 1] {
                Tok::P('.') | Tok::P(':') | Tok::P('!') => continue,
                Tok::Id(p) if !matches!(p.as_str(), "return" | "in" | "else" | "move") => continue,
                _ => {}
            }
        }
        if let Some(mut c) = parse_chain(&l, s, false) {
            merged(&l, &mut c);
            chains.push(c);
        }
    }
    File {
        path: rel.to_string(),
        src,
        l,
        chains,
    }
}

const SHRINK: [&str; 8] = [
    "min_w",
    "min_w_0",
    "truncate",
    "overflow_hidden",
    "overflow_x_hidden",
    "overflow_x_scroll",
    "w",
    "max_w",
];
const KEEP_SIZE: [&str; 3] = ["flex_none", "flex_shrink_0", "flex_basis"];

#[derive(Debug)]
struct Hit {
    rule: &'static str,
    file: String,
    line: usize,
    snippet: String,
}

fn scan(f: &File) -> Vec<Hit> {
    let mut hits = Vec::new();
    for c in &f.chains {
        if !c.is_row() {
            continue;
        }
        let items = f.items(c);
        let others = items
            .iter()
            .filter(|i| !matches!(i, Item::Text(..)))
            .count();
        for it in &items {
            match it {
                Item::Div(j) => {
                    let d = &f.chains[*j];
                    let grows = d.has_any(&["flex_1", "flex_auto", "flex_grow"]);
                    if grows && f.holds_text(*j, 0) && !d.has_any(&SHRINK) && !d.has_any(&KEEP_SIZE)
                    {
                        hits.push(Hit {
                            rule: "L1",
                            file: f.path.clone(),
                            line: f.l.pos[d.start].1,
                            snippet: f.text_of(d.start, d.end.min(d.start + 60)),
                        });
                    }
                }
                Item::Text(a, b) => {
                    let shrinks = c.has_any(&[
                        "truncate",
                        "whitespace_nowrap",
                        "flex_none",
                        "flex_shrink_0",
                    ]);
                    if others > 0 && !shrinks {
                        hits.push(Hit {
                            rule: "L2",
                            file: f.path.clone(),
                            line: f.l.pos[*a].1,
                            snippet: f.text_of(*a, (*b).min(a + 24)),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    hits
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut es: Vec<_> = std::fs::read_dir(dir)
        .expect("dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    es.sort();
    for p in es {
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            sources(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if n.ends_with("tests.rs") || n == "tests.rs" {
                continue;
            }
            out.push(p);
        }
    }
}

fn all_hits() -> Vec<Hit> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&root.join("src"), &mut files);
    sources(&root.join("../overlay/src"), &mut files);
    let mut hits = Vec::new();
    for p in files {
        let rel = p
            .strip_prefix(root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace("../", "")
            .replace('\\', "/");
        let f = load(&p, &rel);
        hits.extend(scan(&f));
    }
    hits
}

fn allowlist() -> Vec<(String, String, String)> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/layout_guard_allow.txt");
    let txt = std::fs::read_to_string(p).unwrap_or_default();
    txt.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut it = l.splitn(3, '|');
            let rule = it.next().unwrap_or("").trim().to_string();
            let file = it.next().unwrap_or("").trim().to_string();
            let snip = it.next().unwrap_or("").trim().to_string();
            (rule, file, snip)
        })
        .collect()
}

#[test]
fn flex_rows_let_their_text_shrink() {
    let hits = all_hits();
    if std::env::var("LAYOUT_GUARD_LIST").is_ok() {
        for h in &hits {
            println!("{}|{}|{}  # line {}", h.rule, h.file, h.snippet, h.line);
        }
    }
    let allow = allowlist();
    let mut used = BTreeSet::new();
    let mut bad = Vec::new();
    for h in &hits {
        let m = allow
            .iter()
            .position(|(r, f, s)| *r == h.rule && *f == h.file && h.snippet == *s);
        match m {
            Some(k) => {
                used.insert(k);
            }
            None => bad.push(format!("{} {}:{}  {}", h.rule, h.file, h.line, h.snippet)),
        }
    }
    let stale: Vec<String> = allow
        .iter()
        .enumerate()
        .filter(|(k, _)| !used.contains(k))
        .map(|(_, (r, f, s))| format!("{r}|{f}|{s}"))
        .collect();
    assert!(
        bad.is_empty(),
        "text in a flex row that cannot shrink (locales/README.md, \"Layout: long translations\"):\n  {}\n\
         Give the text child `.min_w(px(0.))` (with `.flex_1()`) or use `components::label_fill`, or add a \
         justified line to tests/layout_guard_allow.txt.",
        bad.join("\n  ")
    );
    assert!(
        stale.is_empty(),
        "stale lines in tests/layout_guard_allow.txt:\n  {}",
        stale.join("\n  ")
    );
}

#[test]
fn the_scanner_finds_the_two_shapes() {
    let bad = r#"
fn a() {
    div().flex().child(icon("x", 16., c)).child(div().flex_1().child(label)).child(dot(c, 6.))
}
fn b() {
    div().flex().items_center().child(icon("x", 16., c)).child(tr!("Hello"))
}
// The three shapes of eacf480 and 45b150a, as they were.
fn permission_note(n: String) {
    div().flex().gap(px(8.)).items_start().child(icon("x", 14., c)).child(
        div().flex_1().text_size(px(12.)).text_color(rgba_of(c)).child(n),
    )
}
fn publish_note() {
    div().flex().items_center().child(icon("x", 16., c)).child(
        div().flex_1().text_size(px(14.)).child(SharedString::from(tr!("No export yet."))),
    )
}
fn camera_tile(label: String) {
    div().h(px(52.)).flex().items_center().gap(px(10.)).child(icon("x", 20., c)).child(label)
}
"#;
    let ok = r#"
fn a() {
    div().flex().child(icon("x", 16., c)).child(div().flex_1().min_w(px(0.)).child(label))
}
fn fixed(label: String) {
    div().flex().child(icon("x", 20., c)).child(label_fill(label))
}
fn b() {
    div().flex().flex_none().child(icon("x", 16., c)).child(tr!("Hello"))
}
fn c() {
    div().flex().flex_col().child(div().flex_1().child(label)).child(tr!("Hello"))
}
#[cfg(test)]
mod tests {
    fn d() { div().flex().child(icon("x", 16., c)).child(tr!("Hello")) }
}
"#;
    let dir = std::env::temp_dir().join(format!("cm-layout-guard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (pb, po) = (dir.join("bad.rs"), dir.join("ok.rs"));
    std::fs::write(&pb, bad).expect("write sample");
    std::fs::write(&po, ok).expect("write sample");
    let hb = scan(&load(&pb, "bad.rs"));
    let ho = scan(&load(&po, "ok.rs"));
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        hb.iter().map(|h| h.rule).collect::<Vec<_>>(),
        ["L1", "L2", "L1", "L1", "L2"],
        "{hb:?}"
    );
    assert!(ho.is_empty(), "{ho:?}");
}
