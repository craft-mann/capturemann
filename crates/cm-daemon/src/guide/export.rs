//! `export_guide` (GDE-3, GDE-4): Markdown, HTML and rich text.
//!
//! The pictures are **rendered** (`render_png`: annotations, redactions and the focus crop
//! applied), so a blur is in every export and OCR finds no text inside it (GDE-3.AC2). The
//! document builders are pure functions over [`ExportDoc`], tested without a library.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use cm_core::annotation::step_label;
use cm_core::{Actor, CmError};
use cm_library::guide::GuideBundle;
use serde_json::{json, Value};

use super::{ExportArgs, Format, Headings};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;

/// The width an image is shown at in the documents; `image_scale` 2 writes twice that.
pub const READING_WIDTH: u32 = 800;

#[derive(Debug, Clone, PartialEq)]
pub struct ExportImage {
    /// `images/step-01.png` (relative, forward slashes).
    pub file: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportStep {
    /// `1`, `2` or `A`, `B` per the guide's numbering style.
    pub label: String,
    pub title: String,
    pub note: String,
    pub image: Option<ExportImage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportDoc {
    pub title: String,
    pub intro: String,
    pub steps: Vec<ExportStep>,
}

/// The visible heading of a step.
pub fn heading(h: Headings, label: &str, title: &str, position: usize) -> String {
    let title = if title.trim().is_empty() {
        format!("Step {position}")
    } else {
        title.trim().to_owned()
    };
    match h {
        Headings::Numbered => format!("{label}. {title}"),
        Headings::Step => format!("Step {label} · {title}"),
        Headings::TitleOnly => title,
    }
}

fn md_inline(s: &str) -> String {
    s.replace('\n', " ").replace('<', "\\<")
}

fn md_alt(s: &str) -> String {
    s.replace(['[', ']', '\n'], " ").replace('<', " ")
}

/// `guide.md`: relative image links, no absolute path anywhere (GDE-3.AC1).
pub fn markdown(doc: &ExportDoc, headings: Headings, intro: bool, notes: bool) -> String {
    let mut out = format!("# {}\n\n", md_inline(&doc.title));
    if intro && !doc.intro.trim().is_empty() {
        let _ = write!(out, "{}\n\n", doc.intro.trim().replace('<', "\\<"));
    }
    for (i, s) in doc.steps.iter().enumerate() {
        let h = heading(headings, &s.label, &s.title, i + 1);
        let _ = write!(out, "## {}\n\n", md_inline(&h));
        if notes && !s.note.trim().is_empty() {
            let _ = write!(out, "{}\n\n", s.note.trim().replace('<', "\\<"));
        }
        if let Some(img) = &s.image {
            let _ = write!(out, "![{}]({})\n\n", md_alt(&h), img.file);
        }
    }
    out
}

/// The text of the guide with no pictures (the plain-text half of a rich-text copy).
pub fn plain_text(doc: &ExportDoc, headings: Headings, intro: bool, notes: bool) -> String {
    let mut out = format!("{}\n\n", doc.title);
    if intro && !doc.intro.trim().is_empty() {
        let _ = write!(out, "{}\n\n", doc.intro.trim());
    }
    for (i, s) in doc.steps.iter().enumerate() {
        let _ = writeln!(out, "{}", heading(headings, &s.label, &s.title, i + 1));
        if notes && !s.note.trim().is_empty() {
            let _ = writeln!(out, "{}", s.note.trim());
        }
        out.push('\n');
    }
    out
}

pub fn html_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

fn paragraphs(s: &str) -> String {
    s.split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| format!("<p>{}</p>", html_escape(p.trim()).replace('\n', "<br>")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn data_uri(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

const CSS: &str = r"
:root{--bg:#fff;--fg:#16181d;--muted:#5b6270;--line:#e3e6eb;--accent:#1f6feb;--card:#f6f7f9}
@media (prefers-color-scheme:dark){:root{--bg:#14161a;--fg:#e8eaee;--muted:#9aa1ae;--line:#2a2e36;--accent:#6aa2ff;--card:#1c1f25}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.55 -apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Helvetica Neue',Arial,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 20px 64px}
h1{font-size:2rem;line-height:1.2;margin:0 0 .5rem}
h2{font-size:1.25rem;margin:0 0 .5rem}
.intro{color:var(--muted);margin:0 0 1.5rem}
nav.toc{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:12px 20px;margin:0 0 2rem}
nav.toc ol{margin:.25rem 0;padding-left:1.4rem}
nav.toc a{color:var(--accent);text-decoration:none}
section.step{margin:0 0 2.5rem;break-inside:avoid}
section.step img{display:block;max-width:100%;height:auto;border:1px solid var(--line);border-radius:10px;margin-top:.75rem}
section.step p{margin:.4rem 0}
@media print{:root{--bg:#fff;--fg:#000;--muted:#333;--line:#bbb;--card:#fff;--accent:#000}main{max-width:none;padding:0}nav.toc{display:none}section.step{margin-bottom:1.2rem;page-break-inside:avoid}}
";

/// One self-contained HTML file: images embedded, light and dark styles, a table of contents and
/// print styles.
pub fn html(doc: &ExportDoc, headings: Headings, intro: bool, notes: bool, lang: &str) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
         <meta name=\"color-scheme\" content=\"light dark\">\n<title>{}</title>\n<style>{CSS}</style>\n</head>\n<body>\n<main>\n",
        html_escape(lang),
        html_escape(&doc.title)
    );
    let _ = writeln!(out, "<h1>{}</h1>", html_escape(&doc.title));
    if intro && !doc.intro.trim().is_empty() {
        let _ = writeln!(out, "<div class=\"intro\">{}</div>", paragraphs(&doc.intro));
    }
    if doc.steps.len() > 1 {
        out.push_str("<nav class=\"toc\" aria-label=\"Contents\"><ol>\n");
        for (i, s) in doc.steps.iter().enumerate() {
            let _ = writeln!(
                out,
                "<li><a href=\"#step-{}\">{}</a></li>",
                i + 1,
                html_escape(&heading(Headings::TitleOnly, &s.label, &s.title, i + 1))
            );
        }
        out.push_str("</ol></nav>\n");
    }
    out.push_str(&html_steps(doc, headings, notes));
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// The steps alone (the body of a rich-text copy).
pub fn html_steps(doc: &ExportDoc, headings: Headings, notes: bool) -> String {
    let mut out = String::new();
    for (i, s) in doc.steps.iter().enumerate() {
        let h = heading(headings, &s.label, &s.title, i + 1);
        let _ = writeln!(
            out,
            "<section class=\"step\" id=\"step-{}\">\n<h2>{}</h2>",
            i + 1,
            html_escape(&h)
        );
        if notes && !s.note.trim().is_empty() {
            let _ = writeln!(out, "{}", paragraphs(&s.note));
        }
        if let Some(img) = &s.image {
            let _ = writeln!(
                out,
                "<img src=\"{}\" alt=\"{}\" width=\"{}\" height=\"{}\">",
                data_uri(&img.png),
                html_escape(&h),
                img.width,
                img.height
            );
        }
        out.push_str("</section>\n");
    }
    out
}

/// Scales a rendered PNG to at most `max_w` wide (never larger than itself).
fn fit(png: &[u8], max_w: u32) -> Result<(Vec<u8>, u32, u32), CmError> {
    let img = image::load_from_memory(png)
        .map_err(|e| CmError::internal(format!("cannot decode a step image: {e}")))?
        .to_rgba8();
    if img.width() <= max_w {
        return Ok((png.to_vec(), img.width(), img.height()));
    }
    let h = (f64::from(img.height()) * f64::from(max_w) / f64::from(img.width()))
        .round()
        .max(1.0) as u32;
    let small = image::imageops::resize(&img, max_w, h, image::imageops::FilterType::Lanczos3);
    Ok((cm_render::encode_png(&small)?, max_w, h))
}

/// Reads a guide into an [`ExportDoc`] with its step pictures rendered.
pub fn build_doc(g: &GuideBundle, a: &ExportArgs) -> Result<ExportDoc, CmError> {
    let doc = g.doc();
    let mut steps = Vec::new();
    for (i, s) in doc.steps.iter().enumerate() {
        if s.is_note() && !a.note_steps {
            continue;
        }
        let image = if s.image.is_some() {
            let bundle = g.open_step(&s.id)?;
            let rendered = crate::handlers::render_png(&bundle)?;
            let (png, width, height) = fit(&rendered, READING_WIDTH * a.image_scale)?;
            Some(ExportImage {
                file: format!("images/step-{:02}.png", i + 1),
                png,
                width,
                height,
            })
        } else {
            None
        };
        steps.push(ExportStep {
            label: step_label(doc.numbering, 1, u32::try_from(i + 1).unwrap_or(u32::MAX)),
            title: s.title.clone(),
            note: s.note.clone(),
            image,
        });
    }
    Ok(ExportDoc {
        title: doc.title.clone(),
        intro: doc.intro.clone(),
        steps,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), CmError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)
            .map_err(|e| CmError::internal(format!("cannot create `{}`: {e}", p.display())))?;
    }
    std::fs::write(path, bytes).map_err(|e| {
        if e.kind() == std::io::ErrorKind::StorageFull {
            CmError::disk_full(format!("cannot write `{}`: {e}", path.display()))
        } else {
            CmError::internal(format!("cannot write `{}`: {e}", path.display()))
        }
    })
}

fn file_stem(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let s = s.trim().trim_end_matches('.').trim().to_owned();
    if s.is_empty() {
        "guide".to_owned()
    } else {
        s
    }
}

impl Inner {
    pub(crate) async fn export_guide(
        &self,
        _actor: &Actor,
        a: ExportArgs,
    ) -> Result<Value, DaemonError> {
        if self.guide.active_id().as_deref() == Some(a.guide_id.as_str()) {
            return Err(CmError::busy("stop the guide before exporting it").into());
        }
        if a.format == Format::Pdf {
            return Err(DaemonError::NotImplemented(
                "PDF export is not in this version: export HTML and print it (its print styles \
                 are made for that)"
                    .to_owned(),
            ));
        }
        let _serial = self.capture_locks.lock(&a.guide_id).await;
        let dir = {
            let lib = lock_library(&self.library);
            let rec = lib.get(&a.guide_id)?;
            if rec.kind != cm_library::manifest::CaptureKind::Guide {
                return Err(CmError::invalid_field("guide_id", "not a Click Guide").into());
            }
            lib.bundle_path(&rec)
        };
        let clipboard = self.clipboard.clone();
        blocking(move || {
            let g = GuideBundle::open(&dir)?;
            let doc = build_doc(&g, &a)?;
            let pending: Vec<Value> = g
                .steps()
                .iter()
                .enumerate()
                .filter(|(_, s)| s.has_proposals)
                .map(|(i, s)| json!({ "step_id": s.id, "number": i + 1 }))
                .collect();
            let mut warnings: Vec<String> = pending
                .iter()
                .map(|p| {
                    format!(
                        "Step {} still has redaction proposals to review; it is exported \
                         without those blurs",
                        p["number"]
                    )
                })
                .collect();
            let mut out = json!({
                "guide_id": a.guide_id,
                "format": a.format.as_str(),
                "steps": doc.steps.len(),
                "steps_to_review": pending,
            });
            match a.format {
                Format::Markdown => {
                    let root = PathBuf::from(a.dest.as_deref().unwrap_or_default());
                    let mut files = Vec::new();
                    for s in &doc.steps {
                        if let Some(img) = &s.image {
                            write_file(&root.join(&img.file), &img.png)?;
                            files.push(img.file.clone());
                        }
                    }
                    let md = markdown(&doc, a.headings, a.intro, a.notes);
                    write_file(&root.join("guide.md"), md.as_bytes())?;
                    files.insert(0, "guide.md".to_owned());
                    out["path"] = json!(root);
                    out["files"] = json!(files);
                }
                Format::Html => {
                    let dest = PathBuf::from(a.dest.as_deref().unwrap_or_default());
                    let path = if dest.is_dir() || dest.extension().is_none() {
                        dest.join(format!("{}.html", file_stem(&doc.title)))
                    } else {
                        dest
                    };
                    let page = html(&doc, a.headings, a.intro, a.notes, "en");
                    write_file(&path, page.as_bytes())?;
                    out["path"] = json!(path);
                    out["files"] = json!([path.file_name().map(|n| n.to_string_lossy())]);
                }
                Format::RichText => {
                    let body = html_steps(&doc, a.headings, a.notes);
                    let mut frag = format!("<h1>{}</h1>\n", html_escape(&doc.title));
                    if a.intro && !doc.intro.trim().is_empty() {
                        frag.push_str(&paragraphs(&doc.intro));
                    }
                    frag.push_str(&body);
                    let text = plain_text(&doc, a.headings, a.intro, a.notes);
                    let copied = match clipboard.copy_rich_text(&frag, &text) {
                        Ok(()) => true,
                        Err(e) => {
                            warnings.push(format!("Not copied to the clipboard: {e}"));
                            false
                        }
                    };
                    out["copied_to_clipboard"] = json!(copied);
                    out["html"] = json!(frag);
                }
                Format::Pdf => {}
            }
            out["warnings"] = json!(warnings);
            Ok(out)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> ExportDoc {
        let img = |n: u32| ExportImage {
            file: format!("images/step-{n:02}.png"),
            png: vec![1, 2, 3],
            width: 10,
            height: 10,
        };
        ExportDoc {
            title: "Set up <SSO>".into(),
            intro: "Before you start.".into(),
            steps: vec![
                ExportStep {
                    label: "1".into(),
                    title: "Click \"Save\"".into(),
                    note: "Wait a second.\n\nThen continue.".into(),
                    image: Some(img(1)),
                },
                ExportStep {
                    label: "2".into(),
                    title: "Type your password".into(),
                    note: String::new(),
                    image: None,
                },
            ],
        }
    }

    #[test]
    fn markdown_has_relative_links_and_headings_gde_3_ac1() {
        let md = markdown(&doc(), Headings::Numbered, true, true);
        assert!(md.contains("## 1. Click \"Save\""));
        assert!(md.contains("![1. Click \"Save\"](images/step-01.png)"));
        assert!(md.contains("## 2. Type your password"));
        assert!(!md.contains("](/") && !md.contains(":\\"));
        assert!(md.contains("# Set up \\<SSO>"));
        let md = markdown(&doc(), Headings::Step, false, false);
        assert!(md.contains("## Step 1 · Click"));
        assert!(!md.contains("Before you start") && !md.contains("Wait a second"));
        assert!(markdown(&doc(), Headings::TitleOnly, true, true).contains("## Click \"Save\""));
    }

    #[test]
    fn html_is_self_contained_with_toc_dark_and_print() {
        let h = html(&doc(), Headings::Numbered, true, true, "en");
        assert!(h.contains("<nav class=\"toc\""));
        assert!(h.contains("prefers-color-scheme:dark"));
        assert!(h.contains("@media print"));
        assert!(h.contains("data:image/png;base64,"));
        assert!(!h.contains("src=\"images/"));
        assert!(h.contains("Set up &lt;SSO&gt;"));
        assert!(!h.contains("<SSO>"));
        assert!(h.contains("<br>") || h.contains("<p>Then continue.</p>"));
    }

    #[test]
    fn empty_title_falls_back_to_the_position() {
        assert_eq!(heading(Headings::Numbered, "3", " ", 3), "3. Step 3");
        assert_eq!(heading(Headings::Step, "B", "Go", 2), "Step B · Go");
    }
}
