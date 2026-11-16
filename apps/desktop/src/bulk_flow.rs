//! Bulk export (BLK-1, design board S12): the state and the words of the "Export selected" sheet.
//! Pure: no windows here, so it is unit tested. The window is `ui::bulk_sheet`; the calls to the
//! daemon (`export_images`, `get_job`, `cancel_job`) are made from there.
//!
//! The sheet builds the `export_images` input of the agent interface, so the app and an agent
//! do exactly the same thing (every user-facing action has the same tool).

use cm_i18n::{tr, trn};
use serde_json::{json, Value};

/// Which side the size value is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeMode {
    Longest,
    Width,
    Height,
    Percent,
}

impl SizeMode {
    pub const ALL: [SizeMode; 4] = [
        SizeMode::Longest,
        SizeMode::Width,
        SizeMode::Height,
        SizeMode::Percent,
    ];

    pub fn label(self) -> String {
        match self {
            SizeMode::Longest => tr!("Longest side"),
            SizeMode::Width => tr!("Width"),
            SizeMode::Height => tr!("Height"),
            SizeMode::Percent => tr!("Percent"),
        }
    }

    /// The key of the `size` object of `export_images`.
    fn key(self) -> &'static str {
        match self {
            SizeMode::Longest => "longest",
            SizeMode::Width => "width",
            SizeMode::Height => "height",
            SizeMode::Percent => "percent",
        }
    }
}

/// The picture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    /// Lossless only.
    Webp,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Png, Format::Jpeg, Format::Webp];

    pub fn label(self) -> String {
        match self {
            Format::Png => "PNG".to_owned(),
            Format::Jpeg => "JPEG".to_owned(),
            Format::Webp => tr!("WebP (lossless)"),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpeg",
            Format::Webp => "webp",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
        }
    }
}

/// What the watermark is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    None,
    Text,
    Image,
}

/// The nine positions, row by row (the 3 x 3 grid of the sheet).
pub const POSITIONS: [&str; 9] = [
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];

/// The tokens of the name pattern, as the keycaps of the sheet.
pub const TOKENS: [&str; 5] = ["{name}", "{date}", "{n}", "{w}", "{h}"];

/// How far a run is.
#[derive(Debug, Clone, PartialEq)]
pub enum RunState {
    Running,
    Done,
    Failed(String),
    Cancelled,
}

/// One skipped file of the summary.
#[derive(Debug, Clone, PartialEq)]
pub struct Skipped {
    pub capture_id: String,
    pub title: String,
    pub reason: String,
}

/// A started export: its job, its progress and, when done, its summary.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub job_id: String,
    pub total: usize,
    pub done: usize,
    pub state: RunState,
    pub exported: usize,
    pub skipped: Vec<Skipped>,
    /// The name of the file being written, from the job's message.
    pub current: String,
    pub dest: String,
}

impl Run {
    /// "Exporting 12 of 23".
    pub fn progress_line(&self) -> String {
        tr!(
            "Exporting {n} of {total}",
            n = (self.done + 1).min(self.total.max(1)),
            total = self.total
        )
    }

    /// "22 exported, 1 skipped" (BLK-1 summary).
    pub fn summary_line(&self) -> String {
        summary(self.exported, self.skipped.len())
    }

    /// Updates the run from `get_job`.
    pub fn apply_job(&mut self, job: &Value, titles: &[(String, String)]) {
        match job["state"].as_str() {
            Some("done") => {
                self.state = RunState::Done;
                let r = &job["result"];
                self.exported = r["exported"].as_u64().unwrap_or(0) as usize;
                self.done = self.total;
                self.skipped = r["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|f| f["status"] == "skipped")
                    .map(|f| {
                        let id = f["capture_id"].as_str().unwrap_or("").to_owned();
                        Skipped {
                            title: titles
                                .iter()
                                .find(|(i, _)| *i == id)
                                .map_or_else(|| id.clone(), |(_, t)| t.clone()),
                            capture_id: id,
                            reason: f["reason"].as_str().unwrap_or("").to_owned(),
                        }
                    })
                    .collect();
            }
            Some("failed") => {
                self.state = RunState::Failed(
                    job["error"]["reason"]
                        .as_str()
                        .unwrap_or("the export failed")
                        .to_owned(),
                );
            }
            Some("cancelled") => self.state = RunState::Cancelled,
            _ => {
                let p = job["progress"].as_f64().unwrap_or(0.0);
                self.done = ((p * self.total as f64).floor() as usize).min(self.total);
                if let Some(m) = job["message"].as_str() {
                    self.current = m.to_owned();
                }
            }
        }
    }
}

/// "23 exported, 1 skipped" (BLK-1).
pub fn summary(exported: usize, skipped: usize) -> String {
    let e = trn!("{n} exported", "{n} exported", exported);
    if skipped == 0 {
        e
    } else {
        tr!(
            "{exported}, {skipped} skipped",
            exported = e,
            skipped = skipped
        )
    }
}

/// The words of a skip reason.
pub fn reason_text(reason: &str) -> String {
    match reason {
        "not_found" => tr!("The capture is not in the library"),
        "not_a_screenshot" => tr!("Not a screenshot"),
        "unreadable" => tr!("The picture could not be read"),
        "no_space" => tr!("No space left"),
        "write_failed" => tr!("The file could not be written"),
        "encode_failed" => tr!("The picture could not be encoded"),
        "watermark_failed" => tr!("The watermark could not be drawn"),
        other => other.to_owned(),
    }
}

/// What the app keeps: the Export selected sheet and the Make a board sheet (at most one is open).
#[derive(Debug, Clone, Default)]
pub struct BulkUi {
    pub sheet: Option<BulkSheet>,
    pub board: Option<crate::board_flow::BoardSheet>,
    /// Bumped when a sheet opens, so its text fields are refilled.
    pub epoch: u64,
}

/// The state of the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct BulkSheet {
    /// The captures, in the order of the selection: `(id, title)`.
    pub captures: Vec<(String, String)>,
    pub size_on: bool,
    pub size_mode: SizeMode,
    /// The typed value (a number).
    pub size_value: String,
    pub allow_larger: bool,
    pub format: Format,
    /// JPEG quality, 60 to 100.
    pub quality: u8,
    pub mark: MarkKind,
    pub mark_text: String,
    pub mark_image: Option<String>,
    /// Index into [`POSITIONS`].
    pub mark_pos: usize,
    /// Percent, 5 to 100.
    pub mark_opacity: u8,
    /// Percent of the picture width, 5 to 100.
    pub mark_size: u8,
    /// Blueprint (ANN-11): every text line becomes a bar in the exported files (keep-list empty).
    pub blueprint: bool,
    pub pattern: String,
    /// The folder to write to.
    pub dest: Option<String>,
    pub busy: bool,
    pub error: Option<String>,
    pub run: Option<Run>,
}

impl BulkSheet {
    pub fn new(captures: Vec<(String, String)>) -> Self {
        BulkSheet {
            captures,
            size_on: false,
            size_mode: SizeMode::Longest,
            size_value: "1600".to_owned(),
            allow_larger: false,
            format: Format::Png,
            quality: 90,
            mark: MarkKind::None,
            mark_text: String::new(),
            mark_image: None,
            mark_pos: 8,
            mark_opacity: 50,
            mark_size: 25,
            blueprint: false,
            pattern: "{name}".to_owned(),
            dest: None,
            busy: false,
            error: None,
            run: None,
        }
    }

    /// The title of the sheet: "Export 23 screenshots".
    pub fn title(&self) -> String {
        let n = self.captures.len();
        trn!("Export {n} screenshot", "Export {n} screenshots", n)
    }

    /// The primary button: "Export 23 images".
    pub fn button_label(&self) -> String {
        let n = self.captures.len();
        trn!("Export {n} image", "Export {n} images", n)
    }

    /// Why Export cannot start yet, in words; `None` when it can.
    pub fn problem(&self) -> Option<String> {
        if self.captures.is_empty() {
            return Some(tr!("Nothing is selected."));
        }
        if self.dest.as_deref().is_none_or(|d| d.trim().is_empty()) {
            return Some(tr!("Choose a folder to save to."));
        }
        if self.size_on && self.size_number().is_none() {
            return Some(match self.size_mode {
                SizeMode::Percent => tr!("The size must be a percentage from 1 to 400."),
                _ => tr!("The size must be a number of pixels from 1 to 16384."),
            });
        }
        match self.mark {
            MarkKind::Text if self.mark_text.trim().is_empty() => {
                return Some(tr!("Type the watermark text."));
            }
            MarkKind::Image if self.mark_image.is_none() => {
                return Some(tr!("Choose the watermark picture."));
            }
            _ => {}
        }
        if self.pattern.trim().is_empty() {
            return Some(tr!("The file name pattern is empty."));
        }
        // The same check the daemon makes, so the sheet names the mistake before the start.
        if cm_daemon::export_images::check_pattern(self.pattern.trim()).is_err() {
            return Some(tr!(
                "The file name pattern has a mistake: use only the keys below, each between braces."
            ));
        }
        None
    }

    pub fn can_start(&self) -> bool {
        self.problem().is_none() && !self.busy && self.run.is_none()
    }

    fn size_number(&self) -> Option<f64> {
        let v: f64 = self.size_value.trim().parse().ok()?;
        let ok = match self.size_mode {
            SizeMode::Percent => (1.0..=400.0).contains(&v),
            _ => v.fract() == 0.0 && (1.0..=16384.0).contains(&v),
        };
        ok.then_some(v)
    }

    /// The `export_images` input. `Err` holds [`BulkSheet::problem`].
    pub fn request(&self) -> Result<Value, String> {
        if let Some(p) = self.problem() {
            return Err(p);
        }
        let mut p = json!({
            "capture_ids": self.captures.iter().map(|(i, _)| i).collect::<Vec<_>>(),
            "dest": self.dest,
            "format": self.format.key(),
            "name_pattern": self.pattern.trim(),
        });
        if self.format == Format::Jpeg {
            p["quality"] = json!(self.quality.clamp(60, 100));
        }
        if self.size_on {
            let v = self.size_number().unwrap_or(0.0);
            let mut size = json!({});
            size[self.size_mode.key()] = if self.size_mode == SizeMode::Percent {
                json!(v)
            } else {
                json!(v as u32)
            };
            if self.allow_larger {
                size["allow_larger"] = json!(true);
            }
            p["size"] = size;
        }
        let mut wm = json!({
            "position": POSITIONS[self.mark_pos.min(8)],
            "opacity": f64::from(self.mark_opacity.clamp(5, 100)) / 100.0,
            "size": f64::from(self.mark_size.clamp(5, 100)) / 100.0,
        });
        match self.mark {
            MarkKind::None => {}
            MarkKind::Text => {
                wm["text"] = json!(self.mark_text.trim());
                p["watermark"] = wm;
            }
            MarkKind::Image => {
                wm["image_path"] = json!(self.mark_image);
                p["watermark"] = wm;
            }
        }
        if self.blueprint {
            p["blueprint"] = json!(true);
        }
        Ok(p)
    }

    /// The example file name under the pattern field, from the first capture.
    pub fn example(&self) -> String {
        let (name, n) = (
            self.captures
                .first()
                .map_or_else(|| "Screenshot".to_owned(), |(_, t)| t.clone()),
            1,
        );
        let pat = self.pattern.trim();
        let mut out = String::new();
        let mut rest = pat;
        while let Some(i) = rest.find('{') {
            out.push_str(&rest[..i]);
            let Some(j) = rest[i..].find('}') else {
                out.push_str(&rest[i..]);
                rest = "";
                break;
            };
            match &rest[i..=i + j] {
                "{name}" => out.push_str(&name),
                "{date}" => out.push_str("2026-11-14"),
                "{n}" => out.push_str(&n.to_string()),
                "{w}" => out.push_str("1600"),
                "{h}" => out.push_str("1000"),
                other => out.push_str(other),
            }
            rest = &rest[i + j + 1..];
        }
        out.push_str(rest);
        format!("{out}.{}", self.format.ext())
    }

    /// Adds a token to the pattern (the keycaps).
    pub fn insert_token(&mut self, token: &str) {
        self.pattern.push_str(token);
    }

    /// A new sheet for the skipped files only ("Retry skipped"): the same settings, only the
    /// captures that were skipped, a new job.
    pub fn retry_skipped(&self) -> Option<BulkSheet> {
        let run = self.run.as_ref()?;
        if run.skipped.is_empty() {
            return None;
        }
        let mut next = self.clone();
        next.captures = run
            .skipped
            .iter()
            .map(|s| (s.capture_id.clone(), s.title.clone()))
            .collect();
        next.run = None;
        next.busy = false;
        next.error = None;
        Some(next)
    }
}

// ---- Delete selected: one confirmation for all (LIB-1, BLK-1 selection bar) -------------------

use crate::confirm::{delete_dialog, DialogModel};
use crate::delete_flow::{self, DeletePrompt, Level, Notice};

/// The titles listed in the combined delete question; the rest is "and N more".
const DELETE_LIST_MAX: usize = 5;

/// What the one question for N captures knows: each capture's own remote copies (the daemon's
/// `describe_delete`, as for a single delete).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BulkDeletePrompt {
    /// Capture id, title and its copies online.
    pub items: Vec<(String, String, DeletePrompt)>,
}

impl BulkDeletePrompt {
    /// All copies the tick would delete, as one prompt (for the option text and the services).
    fn merged(&self) -> DeletePrompt {
        let mut m = DeletePrompt::default();
        for (_, _, p) in &self.items {
            m.removable.extend(p.removable.iter().cloned());
            m.stays.extend(p.stays.iter().cloned());
        }
        m
    }

    /// The one confirmation: the count, the first titles, and (only when something is online)
    /// the unticked-by-default option that covers every capture that has copies.
    pub fn dialog(&self) -> DialogModel {
        let n = self.items.len();
        let merged = self.merged();
        let trash = crate::platform_text::here(crate::platform_text::Term::Trash);
        let mut m = delete_dialog("");
        m.title = trn!("Delete {n} capture?", "Delete {n} captures?", n);
        m.summary = if merged.offers_remote() {
            tr!(
                "They are removed from your library and moved to {trash}. Their copies online stay there unless you tick the box: deleting them cannot be undone ({services}).",
                services = merged.services().join(", "),
                trash = trash
            )
        } else {
            tr!(
                "They are removed from your library and moved to {trash}.",
                trash = trash
            )
        };
        m.details = self
            .items
            .iter()
            .take(DELETE_LIST_MAX)
            .map(|(_, t, _)| (tr!("Capture"), t.clone()))
            .collect();
        if n > DELETE_LIST_MAX {
            let more = n - DELETE_LIST_MAX;
            m.details
                .push((tr!("Capture"), trn!("and {n} more", "and {n} more", more)));
        }
        m.primary_label = trn!("Delete {n} capture", "Delete {n} captures", n);
        m.option = merged.option_label();
        m
    }

    /// The `also_delete_remote` of one capture for the user's answer; `None` = declined.
    pub fn also_remote(&self, decision: cm_daemon::Decision, id: &str) -> Option<bool> {
        let own = self
            .items
            .iter()
            .find(|(i, _, _)| i == id)
            .map(|(_, _, p)| p.clone())
            .unwrap_or_default();
        delete_flow::also_remote(decision, &own)
    }
}

/// One toast for the whole run: how many left the library, and what went wrong for the others
/// (each with its title and the daemon's words).
pub fn bulk_delete_notice(results: &[(String, Notice)]) -> Notice {
    let ok = results.iter().filter(|(_, n)| n.local_deleted).count();
    let failed: Vec<String> = results
        .iter()
        .filter(|(_, n)| !n.local_deleted)
        .map(|(t, n)| match &n.body {
            Some(b) => tr!("{title}: {reason}", title = t.clone(), reason = b.clone()),
            None => t.clone(),
        })
        .collect();
    if failed.is_empty() {
        Notice {
            level: Level::Success,
            title: trn!("{n} capture deleted", "{n} captures deleted", ok),
            body: None,
            local_deleted: true,
        }
    } else {
        let kept = failed.len();
        Notice {
            level: Level::Error,
            title: trn!(
                "{n} capture wasn\u{2019}t deleted",
                "{n} captures weren\u{2019}t deleted",
                kept
            ),
            body: Some(failed.join(" ")),
            local_deleted: ok > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> BulkSheet {
        let mut s = BulkSheet::new(vec![
            ("a".into(), "Login".into()),
            ("b".into(), "Home".into()),
        ]);
        s.dest = Some("/tmp/out".into());
        s
    }

    #[test]
    fn the_request_is_the_export_images_input() {
        let mut s = sheet();
        s.size_on = true;
        s.size_value = "1600".into();
        s.format = Format::Jpeg;
        s.quality = 80;
        s.mark = MarkKind::Text;
        s.mark_text = "DRAFT".into();
        s.pattern = "{name}-{n}".into();
        let p = s.request().unwrap();
        assert_eq!(p["capture_ids"], json!(["a", "b"]));
        assert_eq!(p["size"], json!({"longest": 1600}));
        assert_eq!(p["format"], "jpeg");
        assert_eq!(p["quality"], 80);
        assert_eq!(p["watermark"]["text"], "DRAFT");
        assert_eq!(p["watermark"]["position"], "bottom_right");
        assert_eq!(p["watermark"]["opacity"], 0.5);
        assert_eq!(p["name_pattern"], "{name}-{n}");
        assert!(p.get("blueprint").is_none());
        s.blueprint = true;
        assert_eq!(s.request().unwrap()["blueprint"], json!(true));
    }

    #[test]
    fn problems_are_named_and_block_the_start() {
        let mut s = sheet();
        assert!(s.can_start());
        s.dest = None;
        assert!(s.problem().unwrap().contains("folder"));
        s.dest = Some("/x".into());
        s.size_on = true;
        s.size_value = "0".into();
        assert!(!s.can_start());
        s.size_mode = SizeMode::Percent;
        s.size_value = "50".into();
        assert!(s.can_start());
        s.mark = MarkKind::Image;
        assert!(s.problem().unwrap().contains("watermark"));
        s.mark = MarkKind::None;
        s.pattern = " ".into();
        assert!(s.problem().is_some());
    }

    #[test]
    fn a_pattern_with_an_unknown_token_or_a_stray_brace_blocks_the_start() {
        for bad in ["{nam}", "shot}", "{name", "../{foo}"] {
            let mut s = sheet();
            s.dest = Some("/tmp/out".into());
            s.pattern = bad.into();
            assert!(s.problem().unwrap().contains("pattern"), "{bad}");
            assert!(!s.can_start());
        }
        let mut s = sheet();
        s.dest = Some("/tmp/out".into());
        s.pattern = "{date}-{n}-{name}-{w}x{h}".into();
        assert!(s.problem().is_none());
    }

    #[test]
    fn the_example_follows_the_pattern_and_the_format() {
        let mut s = sheet();
        s.pattern = "{date} {name} {w}x{h} #{n}".into();
        s.format = Format::Jpeg;
        assert_eq!(s.example(), "2026-11-14 Login 1600x1000 #1.jpg");
        s.insert_token("{n}");
        assert!(s.pattern.ends_with("{n}{n}"));
    }

    #[test]
    fn a_finished_job_gives_the_summary_and_the_skipped_ones_to_retry() {
        let mut s = sheet();
        s.run = Some(Run {
            job_id: "j".into(),
            total: 2,
            done: 0,
            state: RunState::Running,
            exported: 0,
            skipped: vec![],
            current: String::new(),
            dest: "/tmp/out".into(),
        });
        let job = json!({"state": "done", "result": {"exported": 1, "skipped": 1, "files": [
            {"capture_id": "a", "status": "exported"},
            {"capture_id": "b", "status": "skipped", "reason": "no_space"}]}});
        let titles = s.captures.clone();
        let run = s.run.as_mut().unwrap();
        run.apply_job(&job, &titles);
        assert_eq!(run.state, RunState::Done);
        assert_eq!(run.summary_line(), "1 exported, 1 skipped");
        assert_eq!(run.skipped[0].title, "Home");
        assert_eq!(reason_text("no_space"), "No space left");
        let retry = s.retry_skipped().unwrap();
        assert_eq!(retry.captures, vec![("b".to_owned(), "Home".to_owned())]);
        assert!(retry.run.is_none());
    }

    #[test]
    fn progress_follows_the_job() {
        let mut run = Run {
            job_id: "j".into(),
            total: 4,
            done: 0,
            state: RunState::Running,
            exported: 0,
            skipped: vec![],
            current: String::new(),
            dest: String::new(),
        };
        run.apply_job(
            &json!({"state": "running", "progress": 0.5, "message": "x"}),
            &[],
        );
        assert_eq!(run.done, 2);
        assert_eq!(run.progress_line(), "Exporting 3 of 4");
    }

    fn prompt_item(id: &str, title: &str, remote: bool) -> (String, String, DeletePrompt) {
        let mut p = DeletePrompt::default();
        if remote {
            p.removable
                .push(json!({"form": "drive_file", "name": "a.png"}));
        }
        (id.to_owned(), title.to_owned(), p)
    }

    #[test]
    fn bulk_delete_asks_once_and_lists_the_first_titles() {
        let items: Vec<_> = (0..8)
            .map(|i| prompt_item(&format!("c{i}"), &format!("Shot {i}"), false))
            .collect();
        let d = BulkDeletePrompt { items }.dialog();
        assert!(d.title.contains('8'), "{}", d.title);
        assert!(d.primary_is_danger);
        assert!(d.primary_label.contains('8'));
        // Five titles and "and 3 more".
        assert_eq!(d.details.len(), 6);
        assert!(d.details[5].1.contains('3'));
        assert!(d.option.is_none());
    }

    #[test]
    fn bulk_delete_offers_the_remote_option_only_for_captures_that_have_copies() {
        let p = BulkDeletePrompt {
            items: vec![prompt_item("a", "A", false), prompt_item("b", "B", true)],
        };
        assert!(p.dialog().option.is_some());
        use cm_daemon::Decision::{Approve, ApproveForSession, Decline};
        assert_eq!(p.also_remote(Decline, "a"), None);
        assert_eq!(p.also_remote(Approve, "b"), Some(false));
        // Ticked: only the capture with copies online asks for them to go.
        assert_eq!(p.also_remote(ApproveForSession, "a"), Some(false));
        assert_eq!(p.also_remote(ApproveForSession, "b"), Some(true));
        let none = BulkDeletePrompt {
            items: vec![prompt_item("a", "A", false)],
        };
        assert!(none.dialog().option.is_none());
    }

    #[test]
    fn bulk_delete_notice_counts_and_names_the_failures() {
        let good = Notice {
            level: Level::Success,
            title: "x".into(),
            body: None,
            local_deleted: true,
        };
        let bad = Notice {
            level: Level::Error,
            title: "y".into(),
            body: Some("busy".into()),
            local_deleted: false,
        };
        let n = bulk_delete_notice(&[("A".into(), good.clone()), ("B".into(), good.clone())]);
        assert_eq!(n.level, Level::Success);
        assert!(n.title.contains('2'));
        let n = bulk_delete_notice(&[("A".into(), good), ("B".into(), bad)]);
        assert_eq!(n.level, Level::Error);
        assert!(n.local_deleted);
        assert!(n.title.contains('1'));
        assert!(n.body.unwrap().contains("B"));
    }
}
