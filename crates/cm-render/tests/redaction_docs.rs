//! RED-3.AC1: the table of kinds in `README.md`, `docs/user/redaction.md` and `specs/features/17-redaction.md` is generated from the
//! rule data (`RedactionKind`), so the documents cannot list a kind the code does not have.
//!
//! To regenerate after changing a kind (or adding one):
//! `REDACTION_DOCS_UPDATE=1 cargo test -p cm-render --test redaction_docs`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use cm_render::ocr::RedactionKind;
use std::path::PathBuf;

const START: &str = "<!-- redaction-kinds:start -->";
const END: &str = "<!-- redaction-kinds:end -->";
const COMMAND: &str = "REDACTION_DOCS_UPDATE=1 cargo test -p cm-render --test redaction_docs";

/// The Markdown table, one row per kind in `RedactionKind::ALL` order.
fn render_table() -> String {
    let mut s = String::from(
        "| Kind | Group | What it finds | Standards that name it | On by default | Check used |\n\
         |---|---|---|---|---|---|\n",
    );
    for k in RedactionKind::ALL {
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            k.as_str(),
            k.group(),
            k.summary(),
            k.standards().join("; "),
            if k.default_on() { "yes" } else { "no" },
            k.check(),
        ));
    }
    s
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `text` with the part between the markers replaced by `table`.
fn replace_between(text: &str, table: &str) -> Option<String> {
    let a = text.find(START)? + START.len();
    let b = text.find(END)?;
    (a <= b).then(|| format!("{}\n{}{}", &text[..a], table, &text[b..]))
}

fn check_file(rel: &str) {
    let path = root().join(rel);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
    let want = replace_between(&text, &render_table())
        .unwrap_or_else(|| panic!("{rel} has no `{START}` ... `{END}` markers"));
    if want == text {
        return;
    }
    if std::env::var_os("REDACTION_DOCS_UPDATE").is_some() {
        std::fs::write(&path, want).unwrap();
        return;
    }
    panic!("{rel} lists different kinds than the rule data. Regenerate it with:\n    {COMMAND}");
}

#[test]
fn readme_table_matches_the_rule_data() {
    check_file("README.md");
}

#[test]
fn user_guide_table_matches_the_rule_data() {
    check_file("docs/user/redaction.md");
}

#[test]
fn spec_table_matches_the_rule_data() {
    check_file("specs/features/17-redaction.md");
}

#[test]
fn every_kind_has_documentation_data() {
    let table = render_table();
    for k in RedactionKind::ALL {
        assert!(table.contains(&format!("| `{}` |", k.as_str())));
        assert!(!k.summary().is_empty() && !k.check().is_empty() && !k.group().is_empty());
        assert_eq!(k.default_on(), RedactionKind::DEFAULT.contains(&k));
    }
}
