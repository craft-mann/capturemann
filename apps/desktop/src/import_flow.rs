//! Import media (IMP-5, boards I1 to I4): the state and the words of the import sheet, the drop
//! overlay and the progress cards. Pure: no windows here, so it is unit tested. The windows are
//! `ui::import_sheet`; the calls to the daemon are `ui::import_flow`.
//!
//! The sheet's rows are the `dry_run` rows of `import_media` (`specs/features/18-import.md`):
//! `{ path, name, size_bytes, title, kind, status, reasons, container, duration_s, size, fps,
//! video_codec, audio_codec, audio_channels, other_audio_streams, hdr, rotation, error? }`.

use std::path::{Path, PathBuf};

use cm_i18n::{tr, tr_ctx};
use serde_json::{json, Map, Value};

use crate::daemon_text::{duration_text, size_text};

/// Shown in the sheet and the drop overlay: only what the shipped FFmpeg reads.
pub const SUPPORTED_VIDEO: &str = "MP4, MOV, M4V, MKV";
pub const SUPPORTED_AUDIO: &str = "M4A, WAV, MP3";
/// Pictures (IMP-6).
pub const SUPPORTED_IMAGE: &str = "PNG, JPEG";
/// Extensions the drop overlay counts as pictures (the content decides for real).
pub const IMAGE_EXT: [&str; 3] = ["png", "jpg", "jpeg"];
/// Extensions the drop overlay counts as media (the probe decides for real).
pub const VIDEO_EXT: [&str; 5] = ["mp4", "mov", "m4v", "mkv", "webm"];
pub const AUDIO_EXT: [&str; 4] = ["m4a", "wav", "aac", "mp3"];
/// The extensions the macOS bundle registers for Open With and the Dock (`CFBundleDocumentTypes`).
pub const DOCUMENT_EXT: [&str; 7] = ["mp4", "mov", "m4v", "mkv", "m4a", "wav", "mp3"];

/// The free-space floor the daemon keeps (NFR-7), for the footer's "free" figure.
pub const FLOOR_BYTES: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
    /// A PNG or JPEG picture (IMP-6).
    Image,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Ready,
    Prepared(Vec<String>),
    /// `(reason, message)`: `protected_media`, `unsupported_format`, ...
    Cant(String, String),
}

/// One file of the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub title: String,
    pub size_bytes: u64,
    pub kind: Kind,
    pub status: Status,
    pub meta: String,
    pub other_audio: u64,
    /// What the daemon says the import writes for this file (the copy, an editing copy and the
    /// decoded sound); `None` from an older daemon.
    pub planned_bytes: Option<u64>,
}

impl Row {
    pub fn importable(&self) -> bool {
        !matches!(self.status, Status::Cant(..))
    }

    /// Bytes the import writes for this file (the copy, twice with an editing copy, plus the decoded sound).
    pub fn needed(&self) -> u64 {
        if !self.importable() {
            return 0;
        }
        if let Some(n) = self.planned_bytes {
            return n;
        }
        self.size_bytes
            * if matches!(self.status, Status::Prepared(_)) {
                2
            } else {
                1
            }
    }

    /// The reason id of a file that can't be imported.
    pub fn reason(&self) -> Option<&str> {
        match &self.status {
            Status::Cant(r, _) => Some(r),
            _ => None,
        }
    }
}

fn channels_word(n: u64) -> String {
    match n {
        1 => tr!("mono"),
        2 => tr!("stereo"),
        n => tr!("{n} channels", n = n),
    }
}

/// `product.mov · 12:40 · 3840x2160 · hevc · aac stereo · 1.2 GB`: the mono line of a row.
pub fn meta_line(r: &Value) -> String {
    let mut parts = vec![r["name"].as_str().unwrap_or("").to_owned()];
    if let Some(d) = r["duration_s"].as_f64().filter(|d| *d > 0.0) {
        let s = d.round() as u64;
        parts.push(if s >= 3600 {
            format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
        } else {
            format!("{}:{:02}", s / 60, s % 60)
        });
    }
    if let Some(sz) = r["size"].as_array() {
        if let (Some(w), Some(h)) = (
            sz.first().and_then(Value::as_u64),
            sz.get(1).and_then(Value::as_u64),
        ) {
            parts.push(format!("{w}\u{d7}{h}"));
        }
    }
    if r["kind"] == "image" {
        if let Some(c) = r["container"].as_str() {
            parts.push(c.to_uppercase());
        }
    }
    if let Some(c) = r["video_codec"].as_str() {
        parts.push(c.to_uppercase());
    }
    if let Some(c) = r["audio_codec"].as_str() {
        let ch = r["audio_channels"].as_u64().map(channels_word);
        parts.push(match ch {
            Some(ch) => format!("{} {ch}", c.to_uppercase()),
            None => c.to_uppercase(),
        });
    }
    parts.push(size_text(r["size_bytes"].as_u64().unwrap_or(0)));
    parts.join(" \u{b7} ")
}

fn kind_of(v: &Value) -> Kind {
    match v.as_str() {
        Some("audio") => Kind::Audio,
        Some("image") => Kind::Image,
        _ => Kind::Video,
    }
}

/// One `dry_run` row as a [`Row`].
pub fn row_from_json(r: &Value) -> Row {
    let kind = kind_of(&r["kind"]);
    let status = match r["status"].as_str() {
        Some("ready") => Status::Ready,
        Some("prepared") => Status::Prepared(
            r["reasons"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        ),
        _ => Status::Cant(
            r["error"]["reason"]
                .as_str()
                .unwrap_or("unreadable_file")
                .to_owned(),
            r["error"]["message"].as_str().unwrap_or("").to_owned(),
        ),
    };
    let name = r["name"].as_str().unwrap_or("").to_owned();
    let meta = match &status {
        Status::Cant(reason, msg) => cant_text(reason, msg),
        _ => meta_line(r),
    };
    Row {
        path: PathBuf::from(r["path"].as_str().unwrap_or("")),
        title: r["title"].as_str().unwrap_or(&name).to_owned(),
        name,
        size_bytes: r["size_bytes"].as_u64().unwrap_or(0),
        kind,
        status,
        meta,
        other_audio: r["other_audio_streams"].as_u64().unwrap_or(0),
        planned_bytes: r["needed_bytes"].as_u64(),
    }
}

/// What a row that can't be imported says (the sheet's mono line, the failed card).
pub fn cant_text(reason: &str, message: &str) -> String {
    match reason {
        "protected_media" => tr!("Protected by the store it came from (DRM); it can't be opened"),
        "unsupported_format" => tr!("This kind of file isn't supported yet"),
        "too_large" => tr!("Too large: pictures up to 16,384 px on a side and 50 megapixels"),
        "no_streams" => tr!("No video or audio stream could be read. The file may be damaged."),
        "unreadable_file" => tr!("The file can't be read. It may be damaged or cut short."),
        _ if !message.is_empty() => message.to_owned(),
        _ => tr!("The file can't be imported"),
    }
}

/// The chip label of a status.
pub fn chip_label(s: &Status) -> String {
    match s {
        Status::Ready => tr_ctx!("import status", "Ready"),
        Status::Prepared(why) if why.iter().any(|w| is_picture_reason(w)) => {
            tr_ctx!("import status", "Converted")
        }
        Status::Prepared(_) => tr_ctx!("import status", "Prepared for editing"),
        Status::Cant(..) => tr_ctx!("import status", "Can't import"),
    }
}

/// Reasons that only pictures have (IMP-6).
fn is_picture_reason(id: &str) -> bool {
    matches!(id, "converted" | "oriented" | "first_frame_only")
}

/// Words for a "prepared" reason, inside the info note.
pub fn prepare_reason_text(id: &str) -> String {
    match id {
        "variable_frame_rate" => tr!("a variable frame rate"),
        "container_mkv" => tr!("an MKV container"),
        "rotated" => tr!("a rotated picture"),
        "pixel_format" => tr!("a colour format editing doesn't use"),
        "converted" => tr!("colours converted to 8-bit RGB"),
        "oriented" => tr!("turned upright"),
        "first_frame_only" => tr!("an animation, first frame only"),
        _ => tr!("an unusual format"),
    }
}

/// The state of the sheet (board I2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    pub rows: Vec<Row>,
    pub transcribe: bool,
    pub diarize: bool,
    pub report: bool,
    /// Whether the speaker model is installed (Find speakers is dimmed without it).
    pub speaker_model: bool,
    pub remove_originals: bool,
    pub free_bytes: Option<u64>,
    /// Folders skipped from a drop (they are never rows).
    pub folders_skipped: usize,
    /// A check or the start is running.
    pub busy: bool,
    pub error: Option<String>,
}

impl Sheet {
    /// Adds the rows of a `dry_run` answer (a path already in the sheet is not added twice).
    pub fn add_rows(&mut self, files: &[Value]) {
        for f in files {
            let row = row_from_json(f);
            if !self.rows.iter().any(|r| r.path == row.path) {
                self.rows.push(row);
            }
        }
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.rows.len() {
            self.rows.remove(i);
        }
    }

    pub fn set_title(&mut self, i: usize, title: &str) {
        if let Some(r) = self.rows.get_mut(i) {
            r.title = title.to_owned();
        }
    }

    pub fn importable(&self) -> usize {
        self.rows.iter().filter(|r| r.importable()).count()
    }

    pub fn needed_bytes(&self) -> u64 {
        self.rows.iter().map(Row::needed).sum()
    }

    /// Space the originals free when they are moved.
    pub fn freed_bytes(&self) -> u64 {
        self.rows
            .iter()
            .filter(|r| r.importable())
            .map(|r| r.size_bytes)
            .sum()
    }

    /// Not enough room: the daemon would refuse with `disk_full`.
    pub fn too_big(&self) -> bool {
        self.free_bytes
            .is_some_and(|f| f < self.needed_bytes().saturating_add(FLOOR_BYTES))
    }

    /// Whether the Import button works.
    pub fn can_import(&self) -> bool {
        !self.busy && self.importable() > 0 && !self.too_big()
    }

    /// Whether a video or audio file can be imported (the transcript options apply to them).
    pub fn has_media(&self) -> bool {
        self.rows
            .iter()
            .any(|r| r.importable() && r.kind != Kind::Image)
            || self.rows.is_empty()
    }

    pub fn has_audio(&self) -> bool {
        self.rows
            .iter()
            .any(|r| r.importable() && r.kind == Kind::Audio)
    }

    pub fn heading(&self) -> String {
        match self.rows.len() {
            0 => tr!("Import files"),
            1 => tr!("Import 1 file"),
            n => tr!("Import {n} files", n = n),
        }
    }

    pub fn button_label(&self) -> String {
        match self.importable() {
            1 => tr!("Import 1 file"),
            n => tr!("Import {n} files", n = n),
        }
    }

    /// The first footer line: space the import takes and what is free.
    pub fn space_line(&self) -> String {
        match self.free_bytes {
            Some(f) => tr!(
                "Copies into your library: {size} \u{b7} {free} free",
                size = size_text(self.needed_bytes()),
                free = size_text(f)
            ),
            None => tr!(
                "Copies into your library: {size}",
                size = size_text(self.needed_bytes())
            ),
        }
    }

    /// The second footer line.
    pub fn originals_line(&self) -> String {
        if self.remove_originals {
            tr!(
                "Each original goes to {trash} after its copy is checked",
                trash = crate::platform_text::here(crate::platform_text::Term::Trash)
            )
        } else {
            tr!("Originals stay where they are unless you tick \u{201c}Remove the originals\u{201d}")
        }
    }

    /// The hint under the Remove-the-originals checkbox.
    pub fn remove_hint(&self) -> String {
        tr!(
            "Moves instead of copies, so the files don't take space twice. Each original goes to {trash} only after its copy in the library is checked. Frees {size}.",
            size = size_text(self.freed_bytes()),
            trash = crate::platform_text::here(crate::platform_text::Term::Trash)
        )
    }

    /// The blue note about files that get an editing copy, `None` when there are none.
    pub fn prepared_note(&self) -> Option<String> {
        let p: Vec<&Row> = self
            .rows
            .iter()
            .filter(|r| matches!(r.status, Status::Prepared(_)) && r.kind != Kind::Image)
            .collect();
        let first = p.first()?;
        let Status::Prepared(why) = &first.status else {
            return None;
        };
        let reasons = why
            .iter()
            .map(|w| prepare_reason_text(w))
            .collect::<Vec<_>>()
            .join(", ");
        Some(if p.len() == 1 {
            tr!(
                "{name} has {reason}. A copy is prepared for smooth editing; the original file is kept unchanged in the bundle.",
                name = first.name.clone(),
                reason = reasons
            )
        } else {
            tr!(
                "{name} and {n} more files need a copy prepared for smooth editing (for example because of {reason}); the original files are kept unchanged in the bundles.",
                name = first.name.clone(),
                n = p.len() - 1,
                reason = reasons
            )
        })
    }

    /// The note about pictures that were changed on the way in, `None` when there are none.
    /// Pictures are always stored as PNG without their metadata (IMP-6).
    pub fn picture_note(&self) -> Option<String> {
        let p: Vec<&Row> = self
            .rows
            .iter()
            .filter(|r| r.kind == Kind::Image && r.importable())
            .collect();
        let first = p.first()?;
        let changed = matches!(&first.status, Status::Prepared(w) if !w.is_empty());
        Some(if p.len() == 1 && changed {
            let Status::Prepared(why) = &first.status else {
                return None;
            };
            let reasons = why
                .iter()
                .map(|w| prepare_reason_text(w))
                .collect::<Vec<_>>()
                .join(", ");
            tr!(
                "{name} is saved as a PNG ({reason}). Camera and location data are not copied.",
                name = first.name.clone(),
                reason = reasons
            )
        } else {
            tr!("Pictures are saved as PNG. Camera and location data are not copied.")
        })
    }

    /// The `import_media` input for this sheet (as the user: the sheet is the consent).
    pub fn request(&self) -> Value {
        let go: Vec<&Row> = self.rows.iter().filter(|r| r.importable()).collect();
        let mut p = Map::new();
        p.insert(
            "paths".into(),
            json!(go
                .iter()
                .map(|r| r.path.to_string_lossy())
                .collect::<Vec<_>>()),
        );
        p.insert(
            "titles".into(),
            json!(go.iter().map(|r| r.title.trim()).collect::<Vec<_>>()),
        );
        if self.remove_originals {
            p.insert("remove_originals".into(), json!(true));
        }
        if self.transcribe && self.has_media() {
            let mut after = Map::new();
            after.insert("transcribe".into(), json!(true));
            if self.diarize && self.speaker_model {
                after.insert("diarize".into(), json!(true));
            }
            p.insert("after".into(), Value::Object(after));
        }
        Value::Object(p)
    }
}

/// What a key does in the open sheet (the footer shows the Enter key on the Import button).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetKey {
    Close,
    Import,
}

impl Sheet {
    /// Esc closes the sheet (also while it is busy: nothing has started). Enter imports when the
    /// button would work; with the button dimmed it does nothing. Other keys are not the sheet's.
    pub fn key(&self, key: &str) -> Option<SheetKey> {
        match key {
            "escape" => Some(SheetKey::Close),
            "enter" if self.can_import() => Some(SheetKey::Import),
            _ => None,
        }
    }
}

/// Which imported files a library page shows pending cards for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKinds {
    All,
    Video,
    Audio,
    /// The Screenshots page (imported pictures are screenshots).
    Images,
    None,
}

/// Everything the import UI keeps in the app model.
#[derive(Debug, Clone, Default)]
pub struct ImportUi {
    /// The sheet while it is open.
    pub sheet: Option<Sheet>,
    /// Imports started in this run (the cards, the toast).
    pub runs: Vec<ImportRun>,
    /// What is being dragged over the window now (the drop overlay).
    pub hover: Option<DropCount>,
    /// The job toast.
    pub toast_id: Option<u64>,
    /// Bumped when the rows of the sheet change (its title fields are rebuilt).
    pub epoch: u64,
}

impl ImportUi {
    /// The card of a capture id, for the library grid.
    pub fn card_for(&self, capture_id: &str) -> Option<&Card> {
        self.runs
            .iter()
            .flat_map(|r| r.cards.iter())
            .find(|c| c.capture_id.as_deref() == Some(capture_id) && !c.dismissed)
    }

    /// Cards that never became a capture (failed files), newest run first.
    pub fn failed_cards(&self) -> Vec<(usize, usize, &Card)> {
        let mut out = Vec::new();
        for (ri, r) in self.runs.iter().enumerate().rev() {
            for (ci, c) in r.cards.iter().enumerate() {
                if !c.dismissed && matches!(c.state, CardState::Failed(_)) {
                    out.push((ri, ci, c));
                }
            }
        }
        out
    }

    /// The step and fraction to draw on the card of capture `id` while its import still runs;
    /// nothing once the job ended (even when the library list has not refreshed yet).
    pub fn strip_for(&self, id: &str) -> Option<(String, Option<f64>)> {
        self.runs.iter().filter(|r| r.running()).find_map(|r| {
            r.cards
                .iter()
                .find(|c| c.capture_id.as_deref() == Some(id) && !c.dismissed)
                .and_then(|c| match &c.state {
                    CardState::Step(s, f) => Some((s.clone(), *f)),
                    _ => None,
                })
        })
    }

    /// The cards that have no capture of their own on screen: waiting or in a step before the
    /// capture exists, and failed ones. `(run, card)` indexes, newest run first. A card whose
    /// capture is in `existing` is never listed (its own card carries the strip). `page` limits
    /// them to the kind the library page shows.
    pub fn pending_cards(
        &self,
        existing: &std::collections::HashSet<String>,
        page: PageKinds,
    ) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (ri, r) in self.runs.iter().enumerate().rev() {
            for (ci, c) in r.cards.iter().enumerate() {
                let kind_ok = match page {
                    PageKinds::All => true,
                    PageKinds::Video => c.kind == Kind::Video,
                    PageKinds::Audio => c.kind == Kind::Audio,
                    PageKinds::Images => c.kind == Kind::Image,
                    PageKinds::None => false,
                };
                let has_capture = c
                    .capture_id
                    .as_deref()
                    .is_some_and(|id| existing.contains(id));
                let wanted = match c.state {
                    CardState::Done => false,
                    CardState::Failed(_) | CardState::Waiting | CardState::Step(..) => true,
                };
                if !c.dismissed && kind_ok && wanted && !has_capture {
                    out.push((ri, ci));
                }
            }
        }
        out
    }

    pub fn running(&self) -> Option<&ImportRun> {
        self.runs.iter().rev().find(|r| r.running())
    }
}

/// The files of `file://` URLs the platform hands over (Dock drop, Open With): percent-decoded,
/// anything else ignored.
pub fn paths_from_urls(urls: &[String]) -> Vec<PathBuf> {
    fn decode(s: &str) -> Option<String> {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' {
                let h = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(h, 16).ok()?);
                i += 3;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        String::from_utf8(out).ok()
    }
    urls.iter()
        .filter_map(|u| u.strip_prefix("file://"))
        .map(|rest| rest.strip_prefix("localhost").unwrap_or(rest))
        .filter_map(decode)
        // A Windows file URL is `file:///C:/dir/a.mp4`: the slash before the drive letter goes.
        .map(|p| match p.as_bytes() {
            [b'/', d, b':', ..] if d.is_ascii_alphabetic() => p[1..].to_owned(),
            _ => p,
        })
        .map(PathBuf::from)
        .collect()
}

// ---- the drop overlay (board I1) ----------------------------------------------------------

/// What a drop holds, counted for the overlay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DropCount {
    pub videos: usize,
    pub audio: usize,
    /// PNG and JPEG pictures, by extension (IMP-6).
    pub images: usize,
    /// Files that are neither (they may still be probed and refused).
    pub other: usize,
    pub folders: usize,
}

impl DropCount {
    pub fn files(&self) -> usize {
        self.videos + self.audio + self.images + self.other
    }
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Counts dropped paths by kind. `is_dir` is a parameter so the count is testable.
pub fn count_drop(paths: &[PathBuf], is_dir: impl Fn(&Path) -> bool) -> DropCount {
    let mut c = DropCount::default();
    for p in paths {
        if is_dir(p) {
            c.folders += 1;
        } else if VIDEO_EXT.contains(&ext_of(p).as_str()) {
            c.videos += 1;
        } else if AUDIO_EXT.contains(&ext_of(p).as_str()) {
            c.audio += 1;
        } else if IMAGE_EXT.contains(&ext_of(p).as_str()) {
            c.images += 1;
        } else {
            c.other += 1;
        }
    }
    c
}

/// The files of a drop (folders removed).
pub fn drop_files(paths: &[PathBuf], is_dir: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    paths.iter().filter(|p| !is_dir(p)).cloned().collect()
}

pub fn drop_title() -> String {
    tr!("Drop to import")
}

pub fn drop_formats_line() -> String {
    tr!(
        "Video: {video} \u{b7} Audio: {audio} \u{b7} Pictures: {image}",
        video = SUPPORTED_VIDEO,
        audio = SUPPORTED_AUDIO,
        image = SUPPORTED_IMAGE
    )
}

pub fn folder_skipped_text(n: usize) -> String {
    if n == 1 {
        tr!("1 folder skipped")
    } else {
        tr!("{n} folders skipped", n = n)
    }
}

pub fn folder_hint() -> String {
    tr!("Folders can't be imported yet: open the folder and select the files inside.")
}

/// The overlay's counts line: `2 videos · 1 audio file · 1 can't be opened`.
pub fn drop_counts_line(c: &DropCount) -> String {
    let mut parts = Vec::new();
    if c.videos > 0 {
        parts.push(if c.videos == 1 {
            tr!("1 video")
        } else {
            tr!("{n} videos", n = c.videos)
        });
    }
    if c.audio > 0 {
        parts.push(if c.audio == 1 {
            tr!("1 audio file")
        } else {
            tr!("{n} audio files", n = c.audio)
        });
    }
    if c.images > 0 {
        parts.push(if c.images == 1 {
            tr!("1 picture")
        } else {
            tr!("{n} pictures", n = c.images)
        });
    }
    if c.other > 0 {
        parts.push(if c.other == 1 {
            tr!("1 file that may not open")
        } else {
            tr!("{n} files that may not open", n = c.other)
        });
    }
    parts.join(" \u{b7} ")
}

// ---- progress on the cards (board I3) -----------------------------------------------------

/// What a card of a running import shows.
#[derive(Debug, Clone, PartialEq)]
pub enum CardState {
    Waiting,
    /// The step message (copying, preparing, reading the sound, checking) and the file's own
    /// fraction (`None`: no progress to show). Transcripts and speakers are jobs of their own, not
    /// steps of the import.
    Step(String, Option<f64>),
    Done,
    Failed(String),
}

/// One file of a running import.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub capture_id: Option<String>,
    pub path: PathBuf,
    pub title: String,
    pub kind: Kind,
    pub size_bytes: u64,
    pub state: CardState,
    pub dismissed: bool,
}

/// A job the app started (or saw) and its cards.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRun {
    pub job_id: String,
    pub cards: Vec<Card>,
    pub fraction: f64,
    pub message: String,
    pub finished: bool,
    pub cancelled: bool,
}

impl ImportRun {
    /// From the `import_media` answer: `captures[]` with their reserved ids.
    pub fn from_started(answer: &Value, sizes: &[(PathBuf, u64)]) -> Option<ImportRun> {
        let job_id = answer["job_id"].as_str()?.to_owned();
        let cards = answer["captures"]
            .as_array()?
            .iter()
            .map(|c| {
                let path = PathBuf::from(c["path"].as_str().unwrap_or(""));
                let size = sizes
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map_or(0, |(_, s)| *s);
                let queued = c["status"] == "queued";
                Card {
                    capture_id: c["capture_id"].as_str().map(str::to_owned),
                    title: c["title"].as_str().unwrap_or("").to_owned(),
                    kind: kind_of(&c["kind"]),
                    size_bytes: size,
                    state: if queued {
                        CardState::Waiting
                    } else {
                        CardState::Failed(cant_text(
                            c["error"]["reason"].as_str().unwrap_or(""),
                            c["error"]["message"].as_str().unwrap_or(""),
                        ))
                    },
                    path,
                    dismissed: false,
                }
            })
            .collect();
        Some(ImportRun {
            job_id,
            cards,
            fraction: 0.0,
            message: String::new(),
            finished: false,
            cancelled: false,
        })
    }

    fn queued_total(&self) -> f64 {
        self.cards
            .iter()
            .filter(|c| c.capture_id.is_some())
            .map(|c| c.size_bytes as f64)
            .sum()
    }

    /// `job.progress`: the whole job's fraction spread over the files by size.
    pub fn progress(&mut self, fraction: f64, message: &str) {
        self.fraction = fraction.clamp(0.0, 1.0);
        self.message = message.to_owned();
        let total = self.queued_total();
        let done_bytes = self.fraction * total;
        let mut before = 0.0;
        for c in self.cards.iter_mut().filter(|c| c.capture_id.is_some()) {
            if matches!(c.state, CardState::Failed(_)) {
                continue;
            }
            let end = before + c.size_bytes as f64;
            c.state = if done_bytes >= end
                && total > 0.0
                && c.size_bytes > 0
                && self.fraction < 1.0
                && done_bytes > end
            {
                CardState::Done
            } else if done_bytes > before || before == 0.0 {
                let own = if c.size_bytes > 0 {
                    ((done_bytes - before) / c.size_bytes as f64).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                CardState::Step(message.to_owned(), Some(own))
            } else {
                CardState::Waiting
            };
            before = end;
        }
    }

    /// `job.done`: reads the per-file results.
    pub fn finish(&mut self, result: &Value) {
        self.finished = true;
        self.fraction = 1.0;
        for f in result["files"].as_array().into_iter().flatten() {
            let path = PathBuf::from(f["path"].as_str().unwrap_or(""));
            if let Some(c) = self.cards.iter_mut().find(|c| c.path == path) {
                c.state = if f["status"] == "imported" {
                    CardState::Done
                } else {
                    CardState::Failed(cant_text(
                        f["error"]["reason"].as_str().unwrap_or(""),
                        f["error"]["message"].as_str().unwrap_or(""),
                    ))
                };
            }
        }
    }

    /// `job.failed` or a cancel: every card that did not finish is a failure with the reason.
    pub fn fail(&mut self, reason: &str, cancelled: bool) {
        self.finished = true;
        self.cancelled = cancelled;
        for c in &mut self.cards {
            if !matches!(c.state, CardState::Done | CardState::Failed(_)) {
                c.state = CardState::Failed(reason.to_owned());
            }
        }
    }

    pub fn bytes_text(&self) -> String {
        let total = self.queued_total();
        tr!(
            "{done} / {total}",
            done = size_text((self.fraction * total) as u64),
            total = size_text(total as u64)
        )
    }

    pub fn ready_count(&self) -> usize {
        self.cards
            .iter()
            .filter(|c| c.state == CardState::Done)
            .count()
    }

    pub fn running(&self) -> bool {
        !self.finished
    }

    pub fn toast_title(&self) -> String {
        let n = self.cards.iter().filter(|c| c.capture_id.is_some()).count();
        if n == 1 {
            tr!("Importing 1 file")
        } else {
            tr!("Importing {n} files", n = n)
        }
    }

    pub fn toast_body(&self) -> String {
        tr!(
            "{bytes} \u{b7} {ready} ready \u{b7} keep working",
            bytes = self.bytes_text(),
            ready = self.ready_count()
        )
    }
}

/// The step label of a `job.progress` message, in the UI language (the daemon's English is a
/// stable set).
pub fn step_label(message: &str) -> String {
    match message {
        "Copying into the library" => tr!("Copying into the library"),
        "Preparing for editing" => tr!("Preparing for editing"),
        "Reading the sound" => tr!("Reading the sound"),
        "Checking the copy" => tr!("Checking the copy"),
        "Reading the picture" => tr!("Reading the picture"),
        "Saving into the library" => tr!("Saving into the library"),
        other => other.to_owned(),
    }
}

/// The "Imported" filter chip (board I1): a bundle whose manifest has `source`.
pub fn is_imported(manifest: &Value) -> bool {
    manifest.get("source").is_some_and(Value::is_object)
}

/// The source line under an imported video (board I4).
pub fn source_line(manifest: &Value) -> Option<String> {
    let s = manifest.get("source")?;
    let at = s["imported_at"].as_str().unwrap_or("");
    let path = s["path"].as_str().unwrap_or("");
    let codec = manifest["tracks"]["screen"]["codec"]
        .as_str()
        .map(str::to_uppercase);
    Some(match codec {
        Some(c) => tr!(
            "Imported {at} \u{b7} {path} \u{b7} {codec} \u{b7} original kept in the bundle",
            at = at.get(..10).unwrap_or(at).to_owned(),
            path = path.to_owned(),
            codec = c
        ),
        None => tr!(
            "Imported {at} \u{b7} {path} \u{b7} original kept in the bundle",
            at = at.get(..10).unwrap_or(at).to_owned(),
            path = path.to_owned()
        ),
    })
}

/// Duration for the failed-card line and tests.
pub fn duration_line(secs: f64) -> String {
    duration_text(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, kind: &str, status: &str, size: u64) -> Value {
        json!({"path": format!("/m/{name}"), "name": name, "title": name.split('.').next().unwrap(),
               "size_bytes": size, "kind": kind, "status": status, "reasons": ["container_mkv"],
               "duration_s": 3_900.0, "size": [1920, 1080], "video_codec": "h264",
               "audio_codec": "aac", "audio_channels": 2})
    }

    fn cant(name: &str, reason: &str) -> Value {
        json!({"path": format!("/m/{name}"), "name": name, "title": "x", "size_bytes": 5, "kind": "video",
               "status": "cant_import", "error": {"reason": reason, "message": "m"}})
    }

    fn sheet() -> Sheet {
        let mut s = Sheet::default();
        s.add_rows(&[
            row("a.mov", "video", "ready", 1_000_000_000),
            row("b.mkv", "video", "prepared", 500_000_000),
            row("c.m4a", "audio", "ready", 39_000_000),
            cant("d.m4v", "protected_media"),
        ]);
        s
    }

    #[test]
    fn rows_come_from_the_dry_run_and_count_only_importable_files() {
        let s = sheet();
        assert_eq!(s.rows.len(), 4);
        assert_eq!(s.importable(), 3);
        assert_eq!(s.heading(), "Import 4 files");
        assert_eq!(s.button_label(), "Import 3 files");
        // The prepared file counts twice (the copy and the editing copy).
        assert_eq!(s.needed_bytes(), 1_000_000_000 + 1_000_000_000 + 39_000_000);
        assert_eq!(s.freed_bytes(), 1_539_000_000);
        assert_eq!(s.rows[0].meta, "a.mov \u{b7} 1:05:00 \u{b7} 1920\u{d7}1080 \u{b7} H264 \u{b7} AAC stereo \u{b7} 1.0 GB");
        assert_eq!(s.rows[3].reason(), Some("protected_media"));
        assert!(s.rows[3].meta.contains("DRM"));
        assert!(matches!(s.rows[1].status, Status::Prepared(_)));
        // The same path is not added twice.
        let mut t = s.clone();
        t.add_rows(&[row("a.mov", "video", "ready", 1)]);
        assert_eq!(t.rows.len(), 4);
    }

    #[test]
    fn the_request_is_what_the_daemon_parses_and_skips_unimportable_files() {
        let mut s = sheet();
        s.set_title(2, "  Interview  ");
        let r = s.request();
        assert_eq!(r["paths"], json!(["/m/a.mov", "/m/b.mkv", "/m/c.m4a"]));
        assert_eq!(r["titles"], json!(["a", "b", "Interview"]));
        assert!(r.get("remove_originals").is_none() && r.get("after").is_none());
        s.remove_originals = true;
        s.transcribe = true;
        s.diarize = true; // no speaker model: not sent
        let r = s.request();
        assert_eq!(r["remove_originals"], true);
        assert_eq!(r["after"], json!({"transcribe": true}));
        s.speaker_model = true;
        assert_eq!(s.request()["after"]["diarize"], true);
        // The daemon accepts it.
        // (absolute paths of this system: `/m/a.mov` has no drive letter on Windows)
        let mut req = s.request();
        req["paths"] = json!(["a.mov", "b.mkv", "c.m4a"].map(|n| std::env::temp_dir().join(n)));
        cm_daemon::Action::parse("import_media", &req).unwrap();
    }

    #[test]
    fn space_and_the_import_button() {
        let mut s = sheet();
        s.free_bytes = Some(412_000_000_000);
        assert!(s.can_import());
        assert!(
            s.space_line().contains("412.0 GB free"),
            "{}",
            s.space_line()
        );
        s.free_bytes = Some(1_500_000_000);
        assert!(s.too_big() && !s.can_import());
        s.free_bytes = None;
        assert!(s.can_import());
        s.busy = true;
        assert!(!s.can_import());
        s.busy = false;
        s.remove(0);
        s.remove(0);
        s.remove(0);
        assert_eq!(s.importable(), 0);
        assert!(!s.can_import());
    }

    #[test]
    fn escape_closes_and_enter_imports_only_when_the_button_works() {
        let mut s = sheet();
        assert_eq!(s.key("escape"), Some(SheetKey::Close));
        assert_eq!(s.key("enter"), Some(SheetKey::Import));
        assert_eq!(s.key("a"), None);
        s.busy = true;
        assert_eq!(s.key("enter"), None, "a check or start is running");
        assert_eq!(s.key("escape"), Some(SheetKey::Close));
        s.busy = false;
        s.free_bytes = Some(10);
        assert_eq!(s.key("enter"), None, "the disk is too small");
        let empty = Sheet::default();
        assert_eq!(empty.key("enter"), None);
        assert_eq!(empty.key("escape"), Some(SheetKey::Close));
    }

    #[test]
    fn the_info_note_and_the_originals_line() {
        let mut s = sheet();
        let n = s.prepared_note().unwrap();
        assert!(n.starts_with("b.mkv has an MKV container"), "{n}");
        s.add_rows(&[row("e.mkv", "video", "prepared", 1)]);
        assert!(s.prepared_note().unwrap().contains("1 more files"));
        assert!(s.originals_line().starts_with("Originals stay"));
        s.remove_originals = true;
        let trash =
            cm_platform::vocab::here(cm_platform::vocab::Term::Trash).trim_start_matches("the ");
        assert!(s.originals_line().contains(trash), "{}", s.originals_line());
        assert!(
            s.remove_hint().contains("Frees 1.5 GB"),
            "{}",
            s.remove_hint()
        );
        let mut none = Sheet::default();
        none.add_rows(&[row("a.mov", "video", "ready", 1)]);
        assert!(none.prepared_note().is_none());
    }

    #[test]
    fn a_drop_is_counted_by_kind_and_folders_are_skipped() {
        let paths: Vec<PathBuf> = [
            "/d/a.MOV",
            "/d/b.mp4",
            "/d/c.wav",
            "/d/notes.txt",
            "/d/folder",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        let isdir = |p: &Path| p.ends_with("folder");
        let c = count_drop(&paths, isdir);
        assert_eq!((c.videos, c.audio, c.other, c.folders), (2, 1, 1, 1));
        assert_eq!(c.files(), 4);
        assert_eq!(drop_files(&paths, isdir).len(), 4);
        assert_eq!(
            drop_counts_line(&c),
            "2 videos \u{b7} 1 audio file \u{b7} 1 file that may not open"
        );
        assert_eq!(folder_skipped_text(1), "1 folder skipped");
        assert!(folder_hint().starts_with("Folders can't be imported yet"));
        assert_eq!(
            drop_formats_line(),
            "Video: MP4, MOV, M4V, MKV \u{b7} Audio: M4A, WAV, MP3 \u{b7} Pictures: PNG, JPEG"
        );
    }

    /// IMP-6.AC9: a picture dropped on the library is counted as a picture, and its row shows
    /// the size in pixels and the format.
    #[test]
    fn a_dropped_picture_is_counted_and_listed_with_its_size_in_pixels() {
        let paths: Vec<PathBuf> = ["/d/a.PNG", "/d/b.jpg", "/d/c.jpeg", "/d/d.mov", "/d/e.gif"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let c = count_drop(&paths, |_| false);
        assert_eq!((c.images, c.videos, c.other), (3, 1, 1));
        assert_eq!(
            drop_counts_line(&c),
            "1 video \u{b7} 3 pictures \u{b7} 1 file that may not open"
        );
        let r = row_from_json(&json!({
            "path": "/d/shot.png", "name": "shot.png", "title": "shot", "size_bytes": 120_000,
            "kind": "image", "status": "ready", "reasons": [], "container": "png",
            "width": 1170, "height": 2532, "size": [1170, 2532], "needed_bytes": 11_850_000
        }));
        assert_eq!(r.kind, Kind::Image);
        assert!(r.importable());
        assert!(r.meta.contains("1170\u{d7}2532"), "{}", r.meta);
        assert!(r.meta.contains("PNG"), "{}", r.meta);
        assert!(
            !r.meta.contains(':'),
            "a picture has no duration: {}",
            r.meta
        );
    }

    #[test]
    fn a_converted_picture_says_so_and_the_sheet_has_no_transcript_options_for_pictures_only() {
        let converted = row_from_json(&json!({
            "path": "/d/p.png", "name": "p.png", "title": "p", "size_bytes": 5, "kind": "image",
            "status": "prepared", "reasons": ["converted", "oriented"], "size": [10, 20],
        }));
        assert_eq!(chip_label(&converted.status), "Converted");
        let video = row_from_json(&row("a.mov", "video", "prepared", 5));
        assert_eq!(chip_label(&video.status), "Prepared for editing");
        let mut sheet = Sheet::default();
        sheet.rows.push(converted);
        assert!(
            sheet.prepared_note().is_none(),
            "no editing-copy note for a picture"
        );
        assert!(sheet.picture_note().unwrap().contains("p.png"));
        assert!(!sheet.has_media());
        sheet.transcribe = true;
        assert!(sheet.request().get("after").is_none());
        sheet.rows.push(video);
        assert!(sheet.has_media());
        assert!(sheet.request().get("after").is_some());
        assert_eq!(
            cant_text("too_large", ""),
            "Too large: pictures up to 16,384 px on a side and 50 megapixels"
        );
    }

    #[test]
    fn progress_spreads_over_the_cards_by_size() {
        let answer = json!({"job_id": "j1", "captures": [
            {"capture_id": "rec_1", "path": "/m/a.mov", "title": "a", "kind": "video", "status": "queued"},
            {"capture_id": "rec_2", "path": "/m/b.m4a", "title": "b", "kind": "audio", "status": "queued"},
            {"capture_id": null, "path": "/m/c.avi", "title": "c", "kind": "video", "status": "cant_import",
             "error": {"reason": "unsupported_format", "message": "AVI files are not supported yet."}}]});
        let sizes = vec![
            (PathBuf::from("/m/a.mov"), 800u64),
            (PathBuf::from("/m/b.m4a"), 200u64),
        ];
        let mut run = ImportRun::from_started(&answer, &sizes).unwrap();
        assert_eq!(run.cards[0].state, CardState::Waiting);
        assert!(matches!(run.cards[2].state, CardState::Failed(_)));
        assert_eq!(run.toast_title(), "Importing 2 files");
        run.progress(0.4, "Copying into the library");
        assert_eq!(
            run.cards[0].state,
            CardState::Step("Copying into the library".into(), Some(0.5))
        );
        assert_eq!(run.cards[1].state, CardState::Waiting);
        run.progress(0.9, "Reading the sound");
        assert_eq!(run.cards[0].state, CardState::Done);
        assert!(
            matches!(run.cards[1].state, CardState::Step(_, Some(f)) if (f - 0.5).abs() < 1e-9)
        );
        assert_eq!(run.ready_count(), 1);
        assert!(run.bytes_text().contains("/"));
        run.finish(&json!({"files": [
            {"path": "/m/a.mov", "status": "imported"},
            {"path": "/m/b.m4a", "status": "failed", "error": {"reason": "unreadable_file", "message": "x"}}]}));
        assert_eq!(run.cards[0].state, CardState::Done);
        assert!(matches!(&run.cards[1].state, CardState::Failed(m) if m.contains("can't be read")));
        assert!(!run.running());
    }

    #[test]
    fn a_failed_or_cancelled_job_fails_the_unfinished_cards_only() {
        let answer = json!({"job_id": "j", "captures": [
            {"capture_id": "rec_1", "path": "/a", "title": "a", "kind": "video", "status": "queued"},
            {"capture_id": "rec_2", "path": "/b", "title": "b", "kind": "video", "status": "queued"}]});
        let mut run = ImportRun::from_started(
            &answer,
            &[(PathBuf::from("/a"), 1), (PathBuf::from("/b"), 1)],
        )
        .unwrap();
        run.progress(0.9, "Copying into the library");
        run.fail("Cancelled", true);
        assert_eq!(run.cards[0].state, CardState::Done);
        assert_eq!(run.cards[1].state, CardState::Failed("Cancelled".into()));
        assert!(run.cancelled && !run.running());
    }

    #[test]
    fn dock_and_open_with_urls_become_paths() {
        let urls = vec![
            "file:///Users/me/My%20Clip.mov".to_owned(),
            "file://localhost/Users/me/a.m4a".to_owned(),
            "https://example.com/x.mov".to_owned(),
            "file:///bad%zz".to_owned(),
        ];
        assert_eq!(
            paths_from_urls(&urls),
            vec![
                PathBuf::from("/Users/me/My Clip.mov"),
                PathBuf::from("/Users/me/a.m4a")
            ]
        );
    }

    #[test]
    fn a_row_counts_what_the_daemon_says_the_import_writes() {
        // An MP3: the copy (3 MB) plus the decoded sound, which the daemon adds up.
        let r = row_from_json(&json!({
            "path": "/m/talk.mp3", "name": "talk.mp3", "title": "Talk", "size_bytes": 3_000_000,
            "kind": "audio", "status": "ready", "reasons": [], "needed_bytes": 700_000_000
        }));
        assert_eq!(r.needed(), 700_000_000);
        // An older daemon sends no figure: the copy is counted, twice when prepared.
        let r = row_from_json(&json!({
            "path": "/m/a.wav", "name": "a.wav", "title": "a", "size_bytes": 5,
            "kind": "audio", "status": "ready", "reasons": []
        }));
        assert_eq!(r.needed(), 5);
        assert!(AUDIO_EXT.contains(&"mp3") && DOCUMENT_EXT.contains(&"mp3"));
    }

    #[test]
    fn a_windows_file_url_loses_the_slash_before_the_drive() {
        let urls = vec!["file:///C:/Users/me/My%20Clip.mov".to_owned()];
        assert_eq!(
            paths_from_urls(&urls),
            vec![PathBuf::from("C:/Users/me/My Clip.mov")]
        );
    }

    /// The macOS bundle registers exactly the extensions the app reads (Open With, Dock drop).
    #[test]
    fn the_bundle_registers_the_supported_extensions() {
        let plist = include_str!("../../../packaging/macos/Info.plist");
        for e in DOCUMENT_EXT {
            assert!(plist.contains(&format!("<string>{e}</string>")), "{e}");
        }
        for e in ["webm", "avi", "flac"] {
            assert!(!plist.contains(&format!("<string>{e}</string>")), "{e}");
        }
    }

    /// The Windows installer lists CaptureMann under Open with for exactly the same extensions
    /// (per user, not the default program).
    #[test]
    fn the_installer_registers_the_supported_extensions() {
        let nsi = include_str!("../../../packaging/windows/installer.nsi");
        for e in DOCUMENT_EXT {
            assert!(
                nsi.contains(&format!(r#"SupportedTypes" ".{e}""#)),
                "SupportedTypes .{e}"
            );
            assert!(
                nsi.contains(&format!(r"Classes\.{e}\OpenWithList\cm-desktop.exe")),
                "OpenWithList .{e}"
            );
        }
        for e in ["webm", "avi", "flac"] {
            assert!(!nsi.contains(&format!(".{e}\\OpenWithList")), "{e}");
        }
        // Never the default: no value is written on the extension key itself.
        assert!(!nsi.contains(r#"Classes\.mp4" """#));
    }

    fn ui_with(answer: &Value, sizes: &[(PathBuf, u64)]) -> ImportUi {
        ImportUi {
            runs: vec![ImportRun::from_started(answer, sizes).unwrap()],
            ..Default::default()
        }
    }

    fn two_files() -> (Value, Vec<(PathBuf, u64)>) {
        (
            json!({"job_id": "j", "captures": [
                {"capture_id": "rec_1", "path": "/a.m4a", "title": "a", "kind": "audio", "status": "queued"},
                {"capture_id": "rec_2", "path": "/b.avi", "title": "b", "kind": "video", "status": "cant_import",
                 "error": {"reason": "unsupported_format", "message": "AVI"}}]}),
            vec![(PathBuf::from("/a.m4a"), 100)],
        )
    }

    fn ids(v: &[&str]) -> std::collections::HashSet<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    /// Nothing exists yet: the waiting file and the failed one are pending cards, newest run first,
    /// and no card has a strip.
    #[test]
    fn pending_only_before_the_capture_exists() {
        let (a, s) = two_files();
        let ui = ui_with(&a, &s);
        assert_eq!(
            ui.pending_cards(&ids(&[]), PageKinds::All),
            vec![(0, 0), (0, 1)]
        );
        assert_eq!(ui.strip_for("rec_1"), None, "waiting has no strip");
    }

    /// The capture exists and the job (copy, then the after steps) still runs: no separate card, the
    /// strip is on the capture's own card, once, with no percentage during the after steps.
    #[test]
    fn the_strip_moves_onto_the_capture_when_it_exists() {
        let (a, s) = two_files();
        let mut ui = ui_with(&a, &s);
        ui.runs[0].progress(0.5, "Copying into the library");
        assert_eq!(
            ui.pending_cards(&ids(&["rec_1"]), PageKinds::All),
            vec![(0, 1)]
        );
        assert_eq!(
            ui.strip_for("rec_1"),
            Some(("Copying into the library".into(), Some(0.5)))
        );
        ui.runs[0].progress(0.9, "Checking the copy");
        assert_eq!(
            ui.strip_for("rec_1"),
            Some(("Checking the copy".into(), Some(0.9)))
        );
        assert_eq!(
            ui.pending_cards(&ids(&["rec_1"]), PageKinds::All),
            vec![(0, 1)]
        );
    }

    /// The job ended: the strip is gone and no pending card is left, even when the library list
    /// has not refreshed yet (the capture is not in `existing`).
    #[test]
    fn nothing_is_drawn_once_the_job_ended() {
        let (a, s) = two_files();
        let mut ui = ui_with(&a, &s);
        ui.runs[0].progress(0.9, "Checking the copy");
        ui.runs[0].finish(&json!({"files": [{"path": "/a.m4a", "status": "imported"}]}));
        assert_eq!(ui.strip_for("rec_1"), None);
        assert_eq!(
            ui.pending_cards(&ids(&[]), PageKinds::All),
            vec![(0, 1)],
            "only the failed file"
        );
        assert_eq!(
            ui.pending_cards(&ids(&["rec_1"]), PageKinds::All),
            vec![(0, 1)]
        );
    }

    /// A failed file is a pending (failed) card until it is dismissed; a cancelled job fails what
    /// did not finish.
    #[test]
    fn a_failed_card_stays_until_dismissed() {
        let (a, s) = two_files();
        let mut ui = ui_with(&a, &s);
        ui.runs[0].fail("Cancelled", true);
        assert!(matches!(ui.runs[0].cards[0].state, CardState::Failed(_)));
        assert_eq!(ui.pending_cards(&ids(&[]), PageKinds::All).len(), 2);
        ui.runs[0].cards[1].dismissed = true;
        assert_eq!(ui.pending_cards(&ids(&[]), PageKinds::All), vec![(0, 0)]);
    }

    /// A library page shows pending cards of its own kind only.
    #[test]
    fn a_page_shows_only_its_kind() {
        let (a, s) = two_files();
        let ui = ui_with(&a, &s);
        assert_eq!(ui.pending_cards(&ids(&[]), PageKinds::Audio), vec![(0, 0)]);
        assert_eq!(ui.pending_cards(&ids(&[]), PageKinds::Video), vec![(0, 1)]);
        assert!(ui.pending_cards(&ids(&[]), PageKinds::None).is_empty());
    }

    #[test]
    fn imported_means_the_manifest_has_a_source() {
        let m = json!({"source": {"path": "/Users/me/a.mov", "imported_at": "2026-11-03T10:00:00Z"},
                       "tracks": {"screen": {"codec": "hevc"}}});
        assert!(is_imported(&m));
        assert!(!is_imported(&json!({"tracks": {}})));
        let l = source_line(&m).unwrap();
        assert_eq!(l, "Imported 2026-11-03 \u{b7} /Users/me/a.mov \u{b7} HEVC \u{b7} original kept in the bundle");
        assert_eq!(chip_label(&Status::Ready), "Ready");
    }
}
