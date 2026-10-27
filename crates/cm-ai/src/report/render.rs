//! The Markdown and plain text of a report (AI-9). Citations print as `[mm:ss]` (`[h:mm:ss]` past
//! an hour); the stored JSON keeps the exact seconds for the editor to seek to.

use serde_json::Value;

use super::store::Report;
use super::{find_template, FieldKind, Layout, Section, Template, NOTHING_FOUND};

/// `mm:ss`, or `h:mm:ss` from an hour on.
pub fn stamp(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

fn cites(item: &Value) -> String {
    item.get("at")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_f64)
                .map(|t| format!(" [{}]", stamp(t)))
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn text_of(item: &Value, id: &str) -> String {
    item.get(id)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .replace('\n', " ")
}

fn list_of(item: &Value, id: &str) -> Vec<String> {
    item.get(id)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// One field of an item as text (a list joins with `; `).
fn field_text(item: &Value, f: &super::Field) -> String {
    match f.kind {
        FieldKind::Text => text_of(item, &f.id),
        FieldKind::List => list_of(item, &f.id).join("; "),
    }
}

fn render_item(section: &Section, item: &Value, tick: Option<bool>, out: &mut String) {
    // A task item of a section that takes ticks starts with its box.
    let bullet = match tick {
        Some(true) => "- [x] ",
        Some(false) => "- [ ] ",
        None => "- ",
    };
    let Some(first) = section.fields.first() else {
        return;
    };
    let head = field_text(item, first);
    let rest: Vec<&super::Field> = section.fields.iter().skip(1).collect();
    // A section with one field and a few items reads as paragraphs (the summary).
    if rest.is_empty() && section.max_items > 0 && section.max_items <= 3 {
        out.push_str(&format!("{head}{}\n\n", cites(item)));
        return;
    }
    match section.layout {
        Layout::Inline => {
            let parts: Vec<String> = rest
                .iter()
                .filter_map(|f| {
                    let v = field_text(item, f);
                    (!v.is_empty()).then(|| format!("{}: {v}", f.label))
                })
                .collect();
            let tail = if parts.is_empty() {
                String::new()
            } else {
                format!(" \u{2014} {}", parts.join(" \u{b7} "))
            };
            out.push_str(&format!("{bullet}{head}{tail}{}\n", cites(item)));
        }
        Layout::Block => {
            let head = if first.kind == FieldKind::List {
                list_of(item, &first.id).join("; ")
            } else {
                head
            };
            out.push_str(&format!(
                "{bullet}**{}:** {head}{}\n",
                first.label,
                cites(item)
            ));
            for f in rest {
                match f.kind {
                    FieldKind::Text => {
                        let v = text_of(item, &f.id);
                        if !v.is_empty() {
                            out.push_str(&format!("  - **{}:** {v}\n", f.label));
                        }
                    }
                    FieldKind::List => {
                        let l = list_of(item, &f.id);
                        if !l.is_empty() {
                            out.push_str(&format!("  - **{}:**\n", f.label));
                            for x in l {
                                out.push_str(&format!("    - {x}\n"));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The Markdown of the stored content. Items of the task sections print as `- [ ]` / `- [x]`
/// (from [`Report::checked`]). The edited text of a report the user changed is
/// [`Report::markdown`], not this.
pub fn markdown(template: &Template, report: &Report) -> String {
    let title = report
        .context
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| template.name.clone());
    let day = report.created.split('T').next().unwrap_or(&report.created);
    let mut out = format!("# {title}\n\n");
    let model = if report.model.is_empty() {
        String::new()
    } else {
        format!(" \u{b7} {}", report.model)
    };
    out.push_str(&format!("*{} \u{b7} {day}{model}*\n\n", template.name));
    if let Some(n) = &template.notes_only {
        out.push_str(&format!("> {n}\n\n"));
    }
    for section in &template.sections {
        out.push_str(&format!("## {}\n\n", section.title));
        let items = report
            .content
            .get(&section.id)
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        if items.is_empty() {
            out.push_str(&format!("*{NOTHING_FOUND}*\n\n"));
            continue;
        }
        let ticks = super::is_checkable(&section.id);
        for (i, item) in items.iter().enumerate() {
            let tick = ticks.then(|| report.is_checked(&section.id, i));
            render_item(section, item, tick, &mut out);
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// The Markdown of any report: the edited text, else the rendering of its content.
pub fn report_markdown(report: &Report) -> String {
    if let Some(m) = &report.markdown {
        return m.clone();
    }
    match find_template(&report.template) {
        Some(t) => markdown(t, report),
        None => format!("# {}\n", report.template_name),
    }
}

/// Plain text from Markdown: headings in capitals, no emphasis marks or quote marker.
pub fn plain_text(md: &str) -> String {
    let mut out = String::new();
    for line in md.lines() {
        let t = line.trim_end();
        let t = if let Some(h) = t.strip_prefix("## ") {
            h.to_uppercase()
        } else if let Some(h) = t.strip_prefix("# ") {
            h.to_uppercase()
        } else if let Some(q) = t.strip_prefix("> ") {
            q.to_owned()
        } else {
            t.to_owned()
        };
        let t = t.replace("**", "");
        let t = if t.starts_with('*') && t.ends_with('*') && t.len() > 1 {
            t[1..t.len() - 1].to_owned()
        } else {
            t
        };
        out.push_str(&t);
        out.push('\n');
    }
    out
}
