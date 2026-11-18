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

/// Text as plain text in Markdown: every mark that would make a link, an image, HTML, code,
/// emphasis, a table cell, a heading or a list is escaped with a backslash (CommonMark renders
/// `\[` as `[`), line breaks become spaces unless `multiline`, `block` also guards what starts a
/// line (`#`, `-`, `1.`), and the three things a viewer
/// turns into a live link by itself (`scheme://`, `www.`, an address with `@`) are broken up.
/// A title comes from the clicked screen or from an agent and must never become markup (QA
/// macOS pass 7: live links in titles).
fn md_text(s: &str, multiline: bool, block: bool) -> String {
    let chars: Vec<char> = s
        .chars()
        .filter(|c| *c != '\r' && (!c.is_control() || matches!(c, '\n' | '\t')))
        .collect();
    let mut out = String::with_capacity(chars.len() + 8);
    // Everything before the character on its line, to know a heading or a list marker.
    let mut line_start = 0;
    for (i, &c) in chars.iter().enumerate() {
        if c == '\n' {
            if multiline {
                out.push('\n');
                line_start = i + 1;
            } else {
                out.push(' ');
            }
            continue;
        }
        if c == '\t' {
            out.push(' ');
            continue;
        }
        let before = &chars[line_start..i];
        let first_on_line = block && before.iter().all(|b| *b == ' ');
        let after_is = |t: &str| chars[i + 1..].iter().take(t.len()).copied().eq(t.chars());
        let escape = match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '<' | '>' | '|' | '~' | '!' | '&'
            | '@' => true,
            // The colon of `https://`, `ftp://`.
            ':' => after_is("//"),
            // The dot of `www.`.
            '.' => {
                (i >= 3
                    && chars[i - 3..i]
                        .iter()
                        .collect::<String>()
                        .eq_ignore_ascii_case("www"))
                    // `1.` at the start of a line is a list item.
                    || (block
                        && !before.is_empty()
                        && before.iter().all(char::is_ascii_digit)
                        && after_is(" "))
            }
            // A heading, a list, a setext underline.
            '#' | '-' | '+' | '=' => first_on_line,
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn md_inline(s: &str) -> String {
    md_text(s, false, false)
}

fn md_alt(s: &str) -> String {
    md_text(s, false, false)
}

/// `guide.md`: relative image links, no absolute path anywhere (GDE-3.AC1). Every text of the
/// guide is written as plain text (see [`md_text`]).
pub fn markdown(doc: &ExportDoc, headings: Headings, intro: bool, notes: bool) -> String {
    let mut out = format!("# {}\n\n", md_inline(&doc.title));
    if intro && !doc.intro.trim().is_empty() {
        let _ = write!(out, "{}\n\n", md_text(doc.intro.trim(), true, true));
    }
    for (i, s) in doc.steps.iter().enumerate() {
        let h = heading(headings, &s.label, &s.title, i + 1);
        let _ = write!(out, "## {}\n\n", md_inline(&h));
        if notes && !s.note.trim().is_empty() {
            let _ = write!(out, "{}\n\n", md_text(s.note.trim(), true, true));
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

fn io_err(path: &Path, e: &std::io::Error) -> CmError {
    if e.kind() == std::io::ErrorKind::StorageFull {
        CmError::disk_full(format!("cannot write `{}`: {e}", path.display()))
    } else {
        CmError::internal(format!("cannot write `{}`: {e}", path.display()))
    }
}

/// `name`, `name (2)`, `name (3)` ... : the n-th candidate of a name that is taken.
fn numbered(stem: &str, ext: Option<&str>, n: u32) -> String {
    let base = if n <= 1 {
        stem.to_owned()
    } else {
        format!("{stem} ({n})")
    };
    match ext {
        Some(e) => format!("{base}.{e}"),
        None => base,
    }
}

/// The local date and time in a file name: `2026-11-17-153045` (BUG-2).
pub fn stamp_of(local: time::OffsetDateTime) -> String {
    let fmt = time::macros::format_description!("[year]-[month]-[day]-[hour][minute][second]");
    local
        .format(&fmt)
        .unwrap_or_else(|_| "0000-00-00-000000".to_owned())
}

/// The Markdown file name of an export made at `stamp`.
pub fn markdown_name(stamp: &str) -> String {
    format!("guide-{stamp}.md")
}

/// The pictures folder of an export made at `stamp`.
pub fn images_dir_name(stamp: &str) -> String {
    format!("images-{stamp}")
}

/// The first stamp, from `local` on, whose file and folder are both free in `dest` (two exports in
/// the same second move on by a second), so nothing that is there is ever written over.
pub fn free_stamp(dest: &Path, local: time::OffsetDateTime) -> Result<String, CmError> {
    for n in 0..3600 {
        let stamp = stamp_of(local + time::Duration::seconds(n));
        if !dest.join(markdown_name(&stamp)).exists()
            && !dest.join(images_dir_name(&stamp)).exists()
        {
            return Ok(stamp);
        }
    }
    Err(CmError::internal("no free export name in the folder"))
}

/// Points every picture of `doc` at the folder `images-<stamp>` (the links of the Markdown).
pub fn use_images_dir(doc: &mut ExportDoc, stamp: &str) {
    let dir = images_dir_name(stamp);
    for s in &mut doc.steps {
        if let Some(img) = &mut s.image {
            let name = img.file.rsplit('/').next().unwrap_or(&img.file).to_owned();
            img.file = format!("{dir}/{name}");
        }
    }
}

/// What `export_guide` refuses before anything is written (BUG-2, BUG-3): a `dest` that is a
/// file that already exists (an export never replaces one) or is missing. The agent's dialog is
/// not opened for such a call.
pub fn check_dest(format: Format, dest: Option<&str>) -> Result<(), CmError> {
    if matches!(format, Format::RichText) {
        return Ok(());
    }
    let dest = dest.unwrap_or_default();
    if dest.is_empty() {
        return Err(CmError::invalid_field(
            "dest",
            "is required: the folder to write to",
        ));
    }
    if Path::new(dest).is_file() {
        return Err(CmError::invalid_field(
            "dest",
            format!(
                "`{dest}` is a file that already exists; an export never writes over a file, name a folder"
            ),
        ));
    }
    Ok(())
}

/// The folders an agent may export into without a question: the library, the user's export
/// folders (`export.folders`, an app-only setting) and the Documents, Desktop and Downloads
/// folders of the home folder (BUG-3).
pub fn trusted_folders(
    library: &Path,
    settings: &serde_json::Map<String, Value>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut v = vec![library.to_owned()];
    if let Some(list) = settings.get("export.folders").and_then(Value::as_array) {
        v.extend(
            list.iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from),
        );
    }
    if let Some(h) = home {
        v.extend(["Documents", "Desktop", "Downloads"].map(|n| h.join(n)));
    }
    v
}

/// `p` made absolute and free of `.` and `..`, by its words alone.
fn lexical(p: &Path) -> PathBuf {
    let mut abs = if p.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().unwrap_or_default()
    };
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                abs.pop();
            }
            std::path::Component::CurDir => {}
            c => abs.push(c.as_os_str()),
        }
    }
    abs
}

/// A path as a person reads it: the Windows extended-path prefix (`\\?\` before a drive path,
/// `\\?\UNC\` before a network share) is dropped, since the path means the same without it
/// (0.3.6 P-5).
pub fn plain_path(p: &Path) -> String {
    let s = p.display().to_string();
    if let Some(rest) = s.strip_prefix("\\\\?\\UNC\\") {
        return format!("\\\\{rest}");
    }
    match s.strip_prefix("\\\\?\\") {
        Some(rest) => rest.to_owned(),
        None => s,
    }
}

/// `p` made absolute and free of `.` and `..`, with the links of its deepest existing part
/// resolved, so `inside/../../etc` and a link out of a trusted folder do not pass.
pub fn resolve_path(p: &Path) -> PathBuf {
    let abs = lexical(p);
    let mut tail = Vec::new();
    let mut base = abs.clone();
    let mut links = 0;
    loop {
        if let Ok(real) = base.canonicalize() {
            let mut out = real;
            out.extend(tail.iter().rev());
            return out;
        }
        // A link whose target is not there yet: follow it by hand, so it cannot lead out of a
        // trusted folder unseen.
        if let (Ok(target), true) = (std::fs::read_link(&base), links < 16) {
            links += 1;
            let parent = base.parent().map(Path::to_owned).unwrap_or_default();
            base = lexical(&if target.is_absolute() {
                target
            } else {
                parent.join(target)
            });
            continue;
        }
        match (
            base.file_name().map(std::ffi::OsStr::to_owned),
            base.parent(),
        ) {
            (Some(n), Some(parent)) => {
                tail.push(n);
                base = parent.to_owned();
            }
            _ => return abs,
        }
    }
}

/// Whether an agent writing to `dest` needs the user's yes: it is not inside one of `trusted`.
pub fn dest_needs_confirmation(dest: &str, trusted: &[PathBuf]) -> bool {
    if dest.is_empty() {
        return false;
    }
    let target = resolve_path(Path::new(dest));
    !trusted.iter().any(|t| target.starts_with(resolve_path(t)))
}

/// Writes `bytes` to a file that did not exist: `path`, or `stem (2).ext`, `(3)` ... when it
/// did. Returns the path written. The file is made with `create_new`, so a file that appears
/// meanwhile is not replaced either.
fn write_unique(path: &Path, bytes: &[u8]) -> Result<PathBuf, CmError> {
    use std::io::Write as _;
    if let Some(p) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(p).map_err(|e| io_err(p, &e))?;
    }
    let stem = path
        .file_stem()
        .map_or_else(|| "guide".to_owned(), |n| n.to_string_lossy().into_owned());
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 1..10_000 {
        let p = path.with_file_name(numbered(&stem, ext.as_deref(), n));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(mut f) => {
                f.write_all(bytes).map_err(|e| io_err(&p, &e))?;
                return Ok(p);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io_err(&p, &e)),
        }
    }
    Err(CmError::internal(format!(
        "no free file name next to `{}`",
        path.display()
    )))
}

/// The file an HTML or PDF export is for: `dest` itself when it names a file, `<title>.<ext>` in
/// it when it is a folder (or has no extension).
fn single_file_target(dest: &str, title: &str, ext: &str) -> Result<PathBuf, CmError> {
    if dest.is_empty() {
        return Err(CmError::invalid_field(
            "dest",
            "is required: the folder or file to write",
        ));
    }
    let dest = PathBuf::from(dest);
    Ok(if dest.is_dir() || dest.extension().is_none() {
        dest.join(format!("{}.{ext}", file_stem(title)))
    } else {
        dest
    })
}

impl Inner {
    /// BUG-3: an agent's `export_guide` to a place outside the library and the user's export
    /// folders is gated as Confirm (the dialog shows the exact path); inside them it is Free.
    pub(crate) fn export_needs_confirmation(&self, a: &ExportArgs) -> bool {
        if matches!(a.format, Format::RichText) {
            return false;
        }
        let library = lock_library(&self.library).root().to_owned();
        let settings = self.settings.all().unwrap_or_default();
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let trusted = trusted_folders(&library, &settings, home.as_deref());
        dest_needs_confirmation(a.dest.as_deref().unwrap_or_default(), &trusted)
    }

    pub(crate) async fn export_guide(
        &self,
        _actor: &Actor,
        a: ExportArgs,
    ) -> Result<Value, DaemonError> {
        check_dest(a.format, a.dest.as_deref())?;
        if self.guide.active_id().as_deref() == Some(a.guide_id.as_str()) {
            return Err(CmError::busy("stop the guide before exporting it").into());
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
        let local_now = cm_library::index::to_local(self.clock.now());
        blocking(move || {
            let g = GuideBundle::open(&dir)?;
            let mut doc = build_doc(&g, &a)?;
            if doc.steps.is_empty() {
                return Err(CmError::invalid_field(
                    "guide_id",
                    "the guide has no step to export",
                ));
            }
            let mut warnings: Vec<String> = Vec::new();
            let mut out = json!({
                "guide_id": a.guide_id,
                "format": a.format.as_str(),
                "steps": doc.steps.len(),
            });
            match a.format {
                Format::Markdown => {
                    // BUG-2: straight into the folder, every name carries the local date and
                    // time, so an earlier export is never written over.
                    let root = PathBuf::from(a.dest.as_deref().unwrap_or_default());
                    std::fs::create_dir_all(&root).map_err(|e| io_err(&root, &e))?;
                    let stamp = free_stamp(&root, local_now)?;
                    use_images_dir(&mut doc, &stamp);
                    let mut files = Vec::new();
                    for s in &doc.steps {
                        if let Some(img) = &s.image {
                            write_file(&root.join(&img.file), &img.png)?;
                            files.push(img.file.clone());
                        }
                    }
                    let name = markdown_name(&stamp);
                    let md = markdown(&doc, a.headings, a.intro, a.notes);
                    write_file(&root.join(&name), md.as_bytes())?;
                    files.insert(0, name.clone());
                    out["path"] = json!(root);
                    out["file"] = json!(name);
                    out["files"] = json!(files);
                }
                Format::Html => {
                    let wanted = single_file_target(
                        a.dest.as_deref().unwrap_or_default(),
                        &doc.title,
                        "html",
                    )?;
                    let page = html(&doc, a.headings, a.intro, a.notes, "en");
                    let path = write_unique(&wanted, page.as_bytes())?;
                    out["renamed"] = json!(path != wanted);
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
                Format::Pdf => {
                    let wanted = single_file_target(
                        a.dest.as_deref().unwrap_or_default(),
                        &doc.title,
                        "pdf",
                    )?;
                    let bytes = super::pdf::pdf(&doc, a.headings, a.intro, a.notes, a.paper)?;
                    let path = write_unique(&wanted, &bytes)?;
                    out["renamed"] = json!(path != wanted);
                    out["path"] = json!(path);
                    out["files"] = json!([path.file_name().map(|n| n.to_string_lossy())]);
                }
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

    /// 0.3.6 P-5: the confirmation never shows the extended-path prefix.
    #[test]
    fn plain_path_drops_the_extended_prefix() {
        assert_eq!(plain_path(Path::new("\\\\?\\C:\\lab\\out")), "C:\\lab\\out");
        assert_eq!(
            plain_path(Path::new("\\\\?\\UNC\\nas\\share\\a")),
            "\\\\nas\\share\\a"
        );
        assert_eq!(plain_path(Path::new("/tmp/x")), "/tmp/x");
    }

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
        assert!(md.contains("# Set up \\<SSO\\>"), "{md}");
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

    /// QA macOS pass 7: a title from the screen or from an agent must not become a link, an
    /// image, HTML, code, a table or a list in the Markdown.
    #[test]
    fn markdown_titles_and_notes_are_plain_text() {
        let mut d = doc();
        d.title = "[Docs](https://evil.example/x) <b>hi</b> `code` a|b".into();
        d.steps[0].title = "Click \"![x](http://t.example/p.png)\"\n# not a heading".into();
        d.steps[0].note = "See www.example.com or mail jane@example.com or http://example.com/a\n\n1. one\n- two\n# three".into();
        let md = markdown(&d, Headings::Numbered, true, true);
        // No unescaped bracket-paren pair outside the one image of the step.
        let without_image = md
            .replace("![1. Click", "IMG")
            .replace("](images/step-01.png)", "");
        assert!(!without_image.contains("]("), "{md}");
        assert!(!md.contains("<b>"), "{md}");
        assert!(!md.contains("](https") && !md.contains("](http"), "{md}");
        assert!(!md.contains("http://") && !md.contains("https://"), "{md}");
        assert!(!md.contains("www.example"), "{md}");
        assert!(!md.contains("jane@"), "{md}");
        assert!(md.contains("\\[Docs\\]\\(https\\://evil"), "{md}");
        // A title is one line of the heading.
        for line in md.lines().filter(|l| l.starts_with("## ")) {
            assert!(!line.contains("\n"));
        }
        assert!(
            md.contains("\\# three") && md.contains("\\- two") && md.contains("1\\. one"),
            "{md}"
        );
        // Ordinary words are not changed.
        assert_eq!(
            md_text("Click \"Save\" now", false, false),
            "Click \"Save\" now"
        );
        assert_eq!(md_text("Đăng nhập", false, false), "Đăng nhập");
    }

    /// BUG-2: the names carry the local date and time, and the links point at that folder.
    #[test]
    fn bug_2_names_carry_the_date_and_time_and_the_links_follow() {
        let t = time::macros::datetime!(2026-11-17 15:30:45 +7);
        let stamp = stamp_of(t);
        assert_eq!(stamp, "2026-11-17-153045");
        assert_eq!(markdown_name(&stamp), "guide-2026-11-17-153045.md");
        assert_eq!(images_dir_name(&stamp), "images-2026-11-17-153045");
        let mut d = doc();
        use_images_dir(&mut d, &stamp);
        let md = markdown(&d, Headings::Numbered, true, true);
        assert!(
            md.contains("(images-2026-11-17-153045/step-01.png)"),
            "{md}"
        );
        assert!(!md.contains("(images/"), "{md}");
    }

    /// BUG-2: a second export, even in the same second, never reuses a name that is there.
    #[test]
    fn bug_2_a_taken_name_moves_on_and_nothing_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let t = time::macros::datetime!(2026-11-17 15:30:45 +0);
        let first = free_stamp(dir.path(), t).unwrap();
        assert_eq!(first, "2026-11-17-153045");
        std::fs::write(dir.path().join(markdown_name(&first)), "mine").unwrap();
        assert_eq!(free_stamp(dir.path(), t).unwrap(), "2026-11-17-153046");
        // A leftover pictures folder alone also takes the name.
        std::fs::create_dir(dir.path().join(images_dir_name("2026-11-17-153046"))).unwrap();
        assert_eq!(free_stamp(dir.path(), t).unwrap(), "2026-11-17-153047");
    }

    /// BUG-3: a `dest` that is a file already is refused; a folder, or a path that is not there,
    /// is not; `rich_text` has no `dest`.
    #[test]
    fn bug_3_a_destination_file_that_exists_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("report.docx");
        std::fs::write(&file, "mine").unwrap();
        let f = file.to_str().unwrap();
        for fmt in [Format::Markdown, Format::Html, Format::Pdf] {
            assert!(check_dest(fmt, Some(f)).is_err(), "{fmt:?}");
            assert!(check_dest(fmt, None).is_err());
            assert!(check_dest(fmt, Some(dir.path().to_str().unwrap())).is_ok());
            assert!(check_dest(fmt, Some(dir.path().join("new.x").to_str().unwrap())).is_ok());
        }
        assert!(check_dest(Format::RichText, Some(f)).is_ok());
        assert!(check_dest(Format::RichText, None).is_ok());
    }

    /// BUG-3: inside the library or an export folder runs at once; anywhere else, including a
    /// `..` escape and a link out, asks.
    #[test]
    fn bug_3_only_the_library_and_the_export_folders_are_trusted() {
        let lib = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut settings = serde_json::Map::new();
        settings.insert("export.folders".into(), json!([other.path()]));
        let trusted = trusted_folders(lib.path(), &settings, Some(home.path()));
        let asks = |p: &Path| dest_needs_confirmation(p.to_str().unwrap(), &trusted);
        assert!(!asks(&lib.path().join("exports/new")));
        assert!(!asks(lib.path()));
        assert!(!asks(&other.path().join("a")));
        assert!(!asks(&home.path().join("Documents/guides")));
        assert!(asks(&home.path().join(".ssh")));
        assert!(asks(Path::new("/etc")));
        assert!(asks(&lib.path().join("../escape")));
        assert!(asks(&home.path().join("Documents/../.ssh")));
        #[cfg(unix)]
        {
            let link = lib.path().join("out");
            std::os::unix::fs::symlink(home.path().join(".ssh"), &link).unwrap();
            assert!(asks(&link.join("x")), "a link out of the library asks");
        }
        // Without an export setting only the library and the home folders count.
        let bare = trusted_folders(lib.path(), &serde_json::Map::new(), None);
        assert!(dest_needs_confirmation(
            other.path().to_str().unwrap(),
            &bare
        ));
        assert!(!dest_needs_confirmation(
            lib.path().to_str().unwrap(),
            &bare
        ));
    }
}
