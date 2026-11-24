//! Library view model (LIB-1, LIB-2): parsing `list_library` pages, filtering, sorting,
//! search state, selection, grid layout and display formatting. No GPUI types here.

use crate::published::{parse_publishes, parse_uploads, PublishRecord, UploadRecord};
use cm_core::TICKS_PER_SECOND;
use cm_i18n::{fmt, tr, tr_ctx};
use serde_json::Value;
use time::{OffsetDateTime, UtcOffset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Screenshot,
    Recording,
    /// An audio-only recording (`kind: "audio"`, VOC-1).
    Audio,
    /// A Click Guide (`kind: "guide"`, GDE-1).
    Guide,
}

/// What the context menu of a library card offers (LIB-1): Open, Reveal in Finder, Rename, Delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Open,
    Reveal,
    /// Only for captures with an upload that has a share link (STO-4.AC3). "Copy file link" when
    /// the capture is also published (S22).
    CopyLink,
    /// Only for a capture with a publish record: opens the video on its platform (S22).
    OpenPublished,
    /// Only for a capture with a publish record: copies the video's link (S22).
    CopyPublishedLink,
    /// Only for a screenshot, and only where this computer has a text recognizer (ANN-5).
    CopyText,
    Rename,
    Delete,
}

impl CardAction {
    pub const ALL: [CardAction; 4] = [
        CardAction::Open,
        CardAction::Reveal,
        CardAction::Rename,
        CardAction::Delete,
    ];

    /// The entries of one card's menu: "Copy link" appears only when an upload has a link.
    pub fn for_item(item: &CaptureItem) -> Vec<CardAction> {
        let mut v = CardAction::ALL.to_vec();
        if crate::published::share_link_of(&item.uploads).is_some() {
            v.insert(2, CardAction::CopyLink);
        }
        if item.latest_publish().is_some() {
            // S22: Open, Open on YouTube, Copy YouTube link, then the file's own items.
            v.insert(1, CardAction::OpenPublished);
            v.insert(2, CardAction::CopyPublishedLink);
            if let Some(at) = v.iter().position(|a| *a == CardAction::CopyLink) {
                v.remove(at);
                v.insert(3, CardAction::CopyLink);
            }
        }
        if item.kind == Kind::Screenshot && !item.broken && crate::editor::copytext::available() {
            let at = v
                .iter()
                .position(|a| *a == CardAction::Rename)
                .unwrap_or(v.len());
            v.insert(at, CardAction::CopyText);
        }
        v
    }

    pub fn label(self, platform: crate::platform_text::Platform) -> String {
        match self {
            CardAction::Open => tr_ctx!("card menu", "Open"),
            CardAction::CopyLink => tr!("Copy link"),
            CardAction::OpenPublished => tr!(
                "Open on {platform}",
                platform = crate::published::platform_label("youtube")
            ),
            CardAction::CopyPublishedLink => tr!(
                "Copy {platform} link",
                platform = crate::published::platform_label("youtube")
            ),
            CardAction::CopyText => tr!("Copy text"),
            CardAction::Reveal => {
                crate::platform_text::term(platform, crate::platform_text::Term::RevealLabel)
            }
            CardAction::Rename => tr_ctx!("card menu", "Rename\u{2026}"),
            CardAction::Delete => tr_ctx!("card menu", "Delete\u{2026}"),
        }
    }

    /// The label on one card's menu: the publish entries name the platform of the capture's
    /// newest publish, and "Copy link" reads "Copy file link" next to "Copy YouTube link" (S22).
    pub fn label_for(
        self,
        os: crate::platform_text::Platform,
        item: Option<&CaptureItem>,
    ) -> String {
        let record = item.and_then(CaptureItem::latest_publish);
        match (self, record) {
            (CardAction::OpenPublished, Some(r)) => crate::published::open_label(r),
            (CardAction::CopyPublishedLink, Some(r)) => tr!(
                "Copy {platform} link",
                platform = crate::published::platform_label(&r.platform)
            ),
            (CardAction::CopyLink, Some(_)) => tr!("Copy file link"),
            _ => self.label(os),
        }
    }

    /// A stable element id part (never translated).
    pub fn key(self, mac: bool) -> &'static str {
        match self {
            CardAction::Open => "Open",
            CardAction::OpenPublished => "Open published",
            CardAction::CopyPublishedLink => "Copy published link",
            CardAction::CopyLink => "Copy link",
            CardAction::CopyText => "Copy text",
            CardAction::Reveal if mac => "Reveal in Finder",
            CardAction::Reveal => "Show in folder",
            CardAction::Rename => "Rename\u{2026}",
            CardAction::Delete => "Delete\u{2026}",
        }
    }

    /// The 15 px icon of the menu row.
    pub fn glyph(self) -> &'static str {
        match self {
            CardAction::Open => "external-link",
            CardAction::CopyLink => "link",
            CardAction::OpenPublished => "external-link",
            CardAction::CopyPublishedLink => "link",
            CardAction::CopyText => "scan-text",
            CardAction::Reveal => "folder-open",
            CardAction::Rename => "pencil",
            CardAction::Delete => "trash-2",
        }
    }

    /// The key hint at the right of the row: only for bindings that exist (Enter opens the
    /// selected card; there is no Delete key, OD-20).
    pub fn hint(self) -> Option<&'static str> {
        match self {
            CardAction::Open => Some("\u{21B5}"),
            _ => None,
        }
    }
}

impl Kind {
    pub fn label(self) -> String {
        match self {
            Kind::Screenshot => tr_ctx!("capture kind", "Screenshot"),
            Kind::Recording => tr_ctx!("capture kind", "Recording"),
            Kind::Audio => tr_ctx!("capture kind", "Audio"),
            Kind::Guide => tr_ctx!("capture kind", "Click Guide"),
        }
    }
}

/// The sidebar's per-kind totals of a list of the whole library.
pub fn kind_counts(items: &[CaptureItem]) -> crate::nav::NavCounts {
    let n = |k: Kind| items.iter().filter(|i| i.kind == k).count();
    crate::nav::NavCounts {
        screenshots: Some(n(Kind::Screenshot)),
        recordings: Some(n(Kind::Recording)),
        audio: Some(n(Kind::Audio)),
        guides: Some(n(Kind::Guide)),
    }
}

/// The "Needs attention" rule (S037): shown only when its count is above 0; an active filter
/// stays visible so it can be turned off.
pub fn attention_chip_visible(count: Option<usize>, active: bool) -> bool {
    active || count.is_some_and(|n| n > 0)
}

/// The search field's text for a type (S037): it names its scope.
pub fn search_placeholder(page: Filter) -> String {
    match page {
        Filter::All => tr!("Search all captures"),
        Filter::Recordings => tr!("Search recordings"),
        Filter::Screenshots => tr!("Search screenshots"),
        Filter::Audio => tr!("Search audio"),
        Filter::Guides => tr!("Search click guides"),
    }
}

/// The captures that have a publish, with whether one has a warning or a locked privacy.
pub fn published_flags(items: &[CaptureItem]) -> Vec<(String, bool)> {
    items
        .iter()
        .filter(|i| !i.publishes.is_empty())
        .map(|i| {
            (
                i.id.clone(),
                i.publishes.iter().any(|r| {
                    !r.warnings.is_empty()
                        || r.state == crate::published::PublishState::PrivacyLocked
                }),
            )
        })
        .collect()
}

/// How many captures need attention: the whole library's publishes (`(id, has a warning)`) and
/// the captures whose upload waits for a sign-in.
pub fn attention_count(
    published: &[(String, bool)],
    waiting: &std::collections::HashSet<String>,
) -> usize {
    published
        .iter()
        .filter(|(id, warned)| *warned || waiting.contains(id))
        .count()
}

/// One row of `list_library`.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureItem {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    pub created: OffsetDateTime,
    /// Recordings only, in `MediaTime` ticks.
    pub duration_ticks: Option<i64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: u64,
    /// The bundle could not be read (LIB-1 edge case: broken badge).
    pub broken: bool,
    /// Copies of the capture at a destination (`uploads[]`), oldest first (STO-4).
    pub uploads: Vec<UploadRecord>,
    /// Videos published from the capture (`publishes[]`), oldest first (PUB-2).
    pub publishes: Vec<PublishRecord>,
    /// The file it was imported from (`source.path`): an imported capture (IMP-2).
    pub imported_from: Option<String>,
    /// The layout of a board (`pair`, `grid`, `strip`, `free`; BRD-1), `None` for any other capture.
    pub board: Option<String>,
}

impl CaptureItem {
    /// Parses one entry of `captures`; `None` if a required field is missing or malformed.
    pub fn from_json(v: &Value) -> Option<CaptureItem> {
        let kind = match v.get("kind")?.as_str()? {
            "screenshot" => Kind::Screenshot,
            "recording" => Kind::Recording,
            "audio" => Kind::Audio,
            "guide" => Kind::Guide,
            _ => return None,
        };
        let created = OffsetDateTime::parse(
            v.get("created")?.as_str()?,
            &time::format_description::well_known::Rfc3339,
        )
        .ok()?;
        Some(CaptureItem {
            id: v.get("id")?.as_str()?.to_owned(),
            title: v.get("title")?.as_str()?.to_owned(),
            kind,
            created,
            duration_ticks: v.get("duration").and_then(Value::as_i64),
            width: v
                .get("width")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            height: v
                .get("height")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            size_bytes: v.get("size_bytes").and_then(Value::as_u64).unwrap_or(0),
            broken: v.get("broken").and_then(Value::as_bool).unwrap_or(false),
            uploads: parse_uploads(v.get("uploads")),
            publishes: parse_publishes(v.get("publishes")),
            imported_from: v
                .get("source")
                .and_then(|s| s.get("path"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            board: v.get("board").and_then(Value::as_str).map(str::to_owned),
        })
    }

    /// The newest publish (S22): what a published card shows.
    pub fn latest_publish(&self) -> Option<&PublishRecord> {
        crate::published::latest(&self.publishes)
    }

    /// When the newest publish happened (the "Date published" order); `None` if never published
    /// or the record has no time.
    pub fn published_at(&self) -> Option<OffsetDateTime> {
        self.latest_publish().and_then(|r| r.at)
    }

    /// Screen-reader label (NFR-13): everything the card shows, in one sentence.
    pub fn accessible_label(&self, clock: &LocalClock) -> String {
        // A list of facts joined with commas: each part is a whole message of its own.
        let mut parts = vec![self.title.clone(), self.kind.label()];
        if self.broken {
            parts.push(tr!("unreadable"));
        }
        parts.push(clock.format_date(self.created));
        if let Some(t) = self.duration_ticks {
            parts.push(format_duration(t));
        }
        if let (Some(w), Some(h)) = (self.width, self.height) {
            parts.push(tr!("{w} by {h} pixels", w = w, h = h));
        }
        parts.join(", ")
    }
}

/// Parses a `list_library` result: items and the next cursor. Entries that do not parse are
/// skipped (counted in the second tuple field) rather than failing the whole page.
pub fn parse_page(v: &Value) -> Option<(Vec<CaptureItem>, Option<String>, usize)> {
    let arr = v.get("captures")?.as_array()?;
    let items: Vec<CaptureItem> = arr.iter().filter_map(CaptureItem::from_json).collect();
    let skipped = arr.len() - items.len();
    let cursor = v
        .get("next_cursor")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some((items, cursor, skipped))
}

/// Collects pages until the daemon has no cursor or `cap` items are loaded. The daemon's
/// `list_library` has no sort parameter, so sort orders other than date are applied here over
/// everything loaded.
#[derive(Debug, Default)]
pub struct PageCollector {
    pub items: Vec<CaptureItem>,
    pub skipped: usize,
    pub truncated: bool,
    cap: usize,
}

impl PageCollector {
    pub fn new(cap: usize) -> Self {
        PageCollector {
            cap,
            ..Default::default()
        }
    }

    /// Adds a page; returns the cursor to fetch next, or `None` when done.
    pub fn push(&mut self, page: &Value) -> Result<Option<String>, String> {
        let (items, cursor, skipped) =
            parse_page(page).ok_or_else(|| "unexpected list_library result".to_owned())?;
        self.skipped += skipped;
        self.items.extend(items);
        if self.items.len() >= self.cap {
            self.items.truncate(self.cap);
            self.truncated = cursor.is_some();
            return Ok(None);
        }
        Ok(cursor)
    }
}

/// LIB-1 kind filter (the sidebar sections and the "All" chip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Recordings,
    Screenshots,
    /// Audio-only recordings (the Audio sidebar item).
    Audio,
    /// Click Guides (GDE-2, the Click Guides sidebar item).
    Guides,
}

impl Filter {
    /// The `kind` argument of `list_library`.
    pub fn daemon_kind(self) -> Option<&'static str> {
        match self {
            Filter::Recordings => Some("recording"),
            Filter::Screenshots => Some("screenshot"),
            Filter::Audio => Some("audio"),
            Filter::Guides => Some("guide"),
            Filter::All => None,
        }
    }
}

/// LIB-1 remote-copy filter (the "Uploaded" and "Published" chips): at most one is on, and it
/// combines with the kind filter and the search box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteFilter {
    Uploaded,
    Published,
}

impl RemoteFilter {
    /// The `status` argument of `list_library`.
    pub fn daemon_status(self) -> &'static str {
        match self {
            RemoteFilter::Uploaded => "uploaded",
            RemoteFilter::Published => "published",
        }
    }
}

/// The chips of the toolbar, in order. The kind chips (All, Recordings, Screenshots) are gone
/// (OD-16): the sidebar expresses the kind. Only what it cannot express stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    /// Captures made by importing a file (`manifest.source`, IMP-2); filtered on this side.
    Imported,
    Uploaded,
    Published,
    /// Publishes with a warning, or an upload waiting for a sign-in (S22); filtered on this side.
    NeedsAttention,
    /// Boards (BRD-1): screenshots made with "Make a board"; filtered on this side. A view, not
    /// a status: the last chip, after a divider (S037).
    Boards,
}

impl Chip {
    /// The bar's order (S037): Needs attention first, then the statuses, then the Boards view.
    pub const ALL: [Chip; 5] = [
        Chip::NeedsAttention,
        Chip::Imported,
        Chip::Uploaded,
        Chip::Published,
        Chip::Boards,
    ];

    pub fn label(self) -> String {
        match self {
            Chip::Imported => tr_ctx!("filter chip", "Imported"),
            Chip::Uploaded => tr_ctx!("filter chip", "Uploaded"),
            Chip::Published => tr_ctx!("filter chip", "Published"),
            Chip::NeedsAttention => tr_ctx!("filter chip", "Needs attention"),
            Chip::Boards => tr_ctx!("filter chip", "Boards"),
        }
    }

    /// Whether the chip means something on a page of this kind: a recording is the only thing
    /// that is published (YouTube) and a board is a screenshot. The statuses Imported and
    /// Uploaded combine with every type (S037).
    pub fn applies_to(self, page: Filter) -> bool {
        match self {
            Chip::Imported | Chip::Uploaded => true,
            Chip::Published | Chip::NeedsAttention => {
                matches!(page, Filter::All | Filter::Recordings)
            }
            Chip::Boards => matches!(page, Filter::All | Filter::Screenshots),
        }
    }

    /// A stable element id part (never translated).
    pub fn key(self) -> &'static str {
        match self {
            Chip::Imported => "Imported",
            Chip::Uploaded => "Uploaded",
            Chip::Published => "Published",
            Chip::NeedsAttention => "NeedsAttention",
            Chip::Boards => "Boards",
        }
    }

    /// The leading icon of the chip.
    pub fn glyph(self) -> &'static str {
        match self {
            Chip::Imported => "cm-import",
            Chip::Uploaded => "cloud-upload",
            Chip::Published => "send",
            Chip::NeedsAttention => "triangle-alert",
            Chip::Boards => "layout-panel-left",
        }
    }
}

/// LIB-1 sort orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Date,
    /// When the newest video was published: stands in for Date while the Published chip is on
    /// (S22).
    DatePublished,
    Title,
    Duration,
    Size,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [
        SortKey::Date,
        SortKey::Title,
        SortKey::Duration,
        SortKey::Size,
    ];

    /// The orders a page offers: pictures and guides have no length, so Duration is only on the
    /// pages that can hold a recording (QA L4).
    pub fn offered(page: Filter) -> Vec<SortKey> {
        SortKey::ALL
            .into_iter()
            .filter(|k| {
                *k != SortKey::Duration || !matches!(page, Filter::Screenshots | Filter::Guides)
            })
            .collect()
    }

    /// The orders the segmented control shows: with the Published filter on, "Date published"
    /// takes the place of "Date" (S22).
    pub fn offered_in(page: Filter, published: bool) -> Vec<SortKey> {
        SortKey::offered(page)
            .into_iter()
            .map(|k| {
                if published && k == SortKey::Date {
                    SortKey::DatePublished
                } else {
                    k
                }
            })
            .collect()
    }

    pub fn label(self) -> String {
        match self {
            SortKey::Date => tr_ctx!("sort key", "Date"),
            SortKey::DatePublished => tr_ctx!("sort key", "Date published"),
            SortKey::Title => tr_ctx!("sort key", "Title"),
            SortKey::Duration => tr_ctx!("sort key", "Duration"),
            SortKey::Size => tr_ctx!("sort key", "Size"),
        }
    }

    /// A stable element id part (never translated).
    pub fn key(self) -> &'static str {
        match self {
            SortKey::Date => "Date",
            SortKey::DatePublished => "DatePublished",
            SortKey::Title => "Title",
            SortKey::Duration => "Duration",
            SortKey::Size => "Size",
        }
    }

    /// The leading icon of the sort segment.
    pub fn glyph(self) -> &'static str {
        match self {
            SortKey::Date | SortKey::DatePublished => "calendar",
            SortKey::Title => "arrow-down-a-z",
            SortKey::Duration => "clock",
            SortKey::Size => "hard-drive",
        }
    }

    /// Default direction: newest, largest and longest first; titles A to Z.
    pub fn default_descending(self) -> bool {
        !matches!(self, SortKey::Title)
    }
}

pub fn sort_items(items: &mut [CaptureItem], key: SortKey, descending: bool) {
    items.sort_by(|a, b| {
        let ord = match key {
            SortKey::Date => a.created.cmp(&b.created),
            SortKey::DatePublished => a.published_at().cmp(&b.published_at()),
            SortKey::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            SortKey::Duration => a.duration_ticks.cmp(&b.duration_ticks),
            SortKey::Size => a.size_bytes.cmp(&b.size_bytes),
        }
        // Stable, deterministic tiebreak.
        .then_with(|| a.id.cmp(&b.id));
        if descending {
            ord.reverse()
        } else {
            ord
        }
    });
}

/// What the main area shows (LIB-1, LIB-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryStatus {
    Loading,
    Ready,
    /// The configured folder cannot be opened (LIB-2). Captures go to `fallback` meanwhile.
    Unavailable {
        hint: String,
        path: String,
        reason: String,
        fallback_in_use: bool,
    },
    /// The daemon call failed (worded when drawn).
    Failed(crate::daemon_text::ErrorNote),
}

/// Why the list is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmptyKind {
    NoCaptures,
    NoResults { query: String },
}

#[derive(Debug)]
pub struct LibraryState {
    pub status: LibraryStatus,
    pub filter: Filter,
    /// The "Uploaded" or "Published" chip, if on.
    pub remote: Option<RemoteFilter>,
    /// The "Imported" chip: only captures that have a `source` (applied to the loaded page).
    pub imported_only: bool,
    /// The "Boards" chip: only boards (applied to the loaded page, like Imported).
    pub boards_only: bool,
    /// The "Needs attention" chip: only captures whose publishes need a look (applied to the
    /// loaded page, S22).
    pub attention_only: bool,
    /// Captures whose upload or publish waits for a sign-in (B1), set by the view.
    waiting: std::collections::HashSet<String>,
    /// The captures of the page that have a publish, with whether a publish has a warning: the
    /// base of the "Needs attention" count, from the last load without a chip or query.
    published_ids: Option<Vec<(String, bool)>>,
    /// How many captures of the page are uploaded and published, from the last load without a
    /// chip or query (the chips show them).
    pub remote_counts: Option<(usize, usize)>,
    pub sort: SortKey,
    pub descending: bool,
    pub query: String,
    pub selected: Option<usize>,
    items: Vec<CaptureItem>,
    /// Bumped by every request so a slow earlier response cannot overwrite a later one.
    generation: u64,
    pub truncated: bool,
}

impl LibraryState {
    pub fn new(status: LibraryStatus) -> Self {
        LibraryState {
            status,
            filter: Filter::All,
            remote: None,
            imported_only: false,
            boards_only: false,
            attention_only: false,
            waiting: std::collections::HashSet::new(),
            published_ids: None,
            remote_counts: None,
            sort: SortKey::Date,
            descending: true,
            query: String::new(),
            selected: None,
            items: Vec::new(),
            generation: 0,
            truncated: false,
        }
    }

    pub fn items(&self) -> &[CaptureItem] {
        &self.items
    }

    /// Whether a chip is on.
    pub fn chip_active(&self, chip: Chip) -> bool {
        match chip {
            Chip::Imported => self.imported_only,
            Chip::Boards => self.boards_only,
            Chip::NeedsAttention => self.attention_only,
            Chip::Uploaded => self.remote == Some(RemoteFilter::Uploaded),
            Chip::Published => self.remote == Some(RemoteFilter::Published),
        }
    }

    /// The chips the current page shows, in order. "Needs attention" shows only while its count
    /// is above 0 (or while it is on, so it can be turned off).
    pub fn page_chips(&self) -> Vec<Chip> {
        Chip::ALL
            .into_iter()
            .filter(|c| c.applies_to(self.filter))
            .filter(|c| {
                *c != Chip::NeedsAttention
                    || attention_chip_visible(self.chip_count(*c), self.attention_only)
            })
            .collect()
    }

    /// Turns off the chips the page does not show (after a change of page), so a hidden filter
    /// never keeps hiding captures.
    pub fn drop_inapplicable_chips(&mut self) {
        let page = self.filter;
        let base_sort = if self.sort == SortKey::DatePublished {
            SortKey::Date
        } else {
            self.sort
        };
        if !SortKey::offered(page).contains(&base_sort) {
            self.sort = SortKey::Date;
            self.descending = SortKey::Date.default_descending();
        }
        if !Chip::Imported.applies_to(page) {
            self.imported_only = false;
        }
        if !Chip::Boards.applies_to(page) {
            self.boards_only = false;
        }
        if !Chip::NeedsAttention.applies_to(page) {
            self.attention_only = false;
        }
        let drop_remote = match self.remote {
            Some(RemoteFilter::Uploaded) => !Chip::Uploaded.applies_to(page),
            Some(RemoteFilter::Published) => !Chip::Published.applies_to(page),
            None => false,
        };
        if drop_remote {
            self.remote = None;
        }
        self.sync_sort_key();
    }

    /// Whether the page shows published videos: the Published or the Needs attention chip is on.
    /// Cards then show the platform, the publish time and the status of the newest publish.
    pub fn published_mode(&self) -> bool {
        self.remote == Some(RemoteFilter::Published) || self.attention_only
    }

    /// "Date published" replaces "Date" while the page shows published videos, and back (S22).
    fn sync_sort_key(&mut self) {
        let want = if self.published_mode() {
            SortKey::DatePublished
        } else {
            SortKey::Date
        };
        let from = if self.published_mode() {
            SortKey::Date
        } else {
            SortKey::DatePublished
        };
        if self.sort == from {
            self.sort = want;
            let selected_id = self.selected_item().map(|i| i.id.clone());
            sort_items(&mut self.items, self.sort, self.descending);
            self.selected = selected_id.and_then(|id| self.items.iter().position(|i| i.id == id));
        }
    }

    /// The sort segments the page shows.
    pub fn offered_sorts(&self) -> Vec<SortKey> {
        SortKey::offered_in(self.filter, self.published_mode())
    }

    /// Records which captures wait for a sign-in; returns whether it changed.
    pub fn set_waiting<'a>(&mut self, ids: impl IntoIterator<Item = &'a String>) -> bool {
        let new: std::collections::HashSet<String> = ids.into_iter().cloned().collect();
        let changed = new != self.waiting;
        self.waiting = new;
        changed
    }

    /// Whether this capture's publishes need a look (S22).
    pub fn item_needs_attention(&self, item: &CaptureItem) -> bool {
        crate::published::needs_attention(&item.publishes, self.waiting.contains(&item.id))
    }

    /// The loaded captures the page shows: the client-side chips (Imported, Boards, Click
    /// Guides, Needs attention) applied to what the daemon returned.
    pub fn visible_items(&self) -> Vec<CaptureItem> {
        self.items
            .iter()
            .filter(|i| !self.imported_only || i.imported_from.is_some())
            .filter(|i| !self.boards_only || i.board.is_some())
            .filter(|i| !self.attention_only || self.item_needs_attention(i))
            .cloned()
            .collect()
    }

    /// The words after the title while a published filter is on (S22): "6 of 61 \u{b7} published".
    /// `None` when no such filter is on, the list is not loaded or the total is unknown (no
    /// placeholder).
    pub fn count_note(&self, total: Option<usize>) -> Option<String> {
        if !self.published_mode() || !matches!(self.status, LibraryStatus::Ready) {
            return None;
        }
        let (shown, total) = (self.visible_items().len(), total?);
        Some(if self.attention_only {
            tr!(
                "{shown} of {total} \u{00B7} needs attention",
                shown = shown,
                total = total
            )
        } else {
            tr!(
                "{shown} of {total} \u{00B7} published",
                shown = shown,
                total = total
            )
        })
    }

    /// The published filter, as an old deep link asks for it: Recordings page, Published on, no
    /// other remote chip (S22).
    pub fn show_published(&mut self) {
        self.filter = Filter::Recordings;
        self.drop_inapplicable_chips();
        self.remote = Some(RemoteFilter::Published);
        self.selected = None;
        self.sync_sort_key();
    }

    /// A click on a chip: it toggles, and turning one on turns the other off. Returns whether the
    /// query changed (the caller reloads).
    pub fn click_chip(&mut self, chip: Chip) -> bool {
        let want = match chip {
            Chip::Imported => {
                self.imported_only = !self.imported_only;
                self.selected = None;
                return false;
            }
            Chip::Boards => {
                self.boards_only = !self.boards_only;
                self.selected = None;
                return false;
            }
            Chip::NeedsAttention => {
                self.attention_only = !self.attention_only;
                self.selected = None;
                self.sync_sort_key();
                return false;
            }
            Chip::Uploaded => RemoteFilter::Uploaded,
            Chip::Published => RemoteFilter::Published,
        };
        self.remote = if self.remote == Some(want) {
            None
        } else {
            Some(want)
        };
        self.sync_sort_key();
        true
    }

    /// The count a chip shows, when known: the uploaded and published captures of the page's kind
    /// (all captures on "All captures").
    pub fn chip_count(&self, chip: Chip) -> Option<usize> {
        if chip == Chip::Imported {
            return Some(
                self.items
                    .iter()
                    .filter(|i| i.imported_from.is_some())
                    .count(),
            );
        }
        if chip == Chip::Boards {
            return Some(self.items.iter().filter(|i| i.board.is_some()).count());
        }
        if chip == Chip::NeedsAttention {
            self.remote_counts?;
            return self.published_ids.as_ref().map(|v| {
                v.iter()
                    .filter(|(id, warned)| *warned || self.waiting.contains(id))
                    .count()
            });
        }
        let (up, publ) = self.remote_counts?;
        match chip {
            Chip::Imported | Chip::Boards | Chip::NeedsAttention => None,
            Chip::Uploaded => Some(up),
            Chip::Published => Some(publ),
        }
    }

    /// Call after a load lands: when it saw the whole page (no remote chip or query; the kind is
    /// the page's), its totals become the chips' counts.
    pub fn update_remote_counts(&mut self) {
        if self.remote.is_none() && self.query.trim().is_empty() {
            self.remote_counts = Some((
                self.items.iter().filter(|i| !i.uploads.is_empty()).count(),
                self.items
                    .iter()
                    .filter(|i| !i.publishes.is_empty())
                    .count(),
            ));
            self.published_ids = Some(published_flags(&self.items));
        }
    }

    /// Whether the loaded list is the whole library (no query, remote chip or kind page): only
    /// then are its per-kind totals the sidebar's counts.
    pub fn is_whole_library(&self) -> bool {
        self.query.is_empty() && self.filter == Filter::All && self.remote.is_none()
    }

    /// Starts a (re)load and returns its generation token.
    pub fn begin_load(&mut self) -> u64 {
        self.generation += 1;
        // Old items stay on screen while a refresh runs; only a first load shows "Loading".
        self.generation
    }

    /// The first load of a library has not answered in time: shows `message` as the failure (with
    /// "Try again") instead of "Loading library..." for ever. Does nothing once the load landed or
    /// a newer one started. Returns whether it changed the state.
    pub fn load_timed_out(&mut self, token: u64, message: String) -> bool {
        if token != self.generation || !matches!(self.status, LibraryStatus::Loading) {
            return false;
        }
        self.finish_load(token, Err(crate::daemon_text::ErrorNote::plain(message)))
    }

    /// Applies a response if it is still the latest. Returns whether it was applied.
    pub fn finish_load(
        &mut self,
        token: u64,
        result: Result<PageCollector, crate::daemon_text::ErrorNote>,
    ) -> bool {
        if token != self.generation {
            return false;
        }
        match result {
            Ok(c) => {
                let selected_id = self.selected_item().map(|i| i.id.clone());
                self.items = c.items;
                self.truncated = c.truncated;
                sort_items(&mut self.items, self.sort, self.descending);
                if !matches!(self.status, LibraryStatus::Unavailable { .. }) {
                    self.status = LibraryStatus::Ready;
                }
                self.selected = selected_id
                    .and_then(|id| self.items.iter().position(|i| i.id == id))
                    .or(if self.items.is_empty() { None } else { Some(0) });
            }
            Err(e) => {
                if !matches!(self.status, LibraryStatus::Unavailable { .. }) {
                    self.status = LibraryStatus::Failed(e);
                }
            }
        }
        true
    }

    pub fn set_sort(&mut self, key: SortKey) {
        if self.sort == key {
            self.descending = !self.descending;
        } else {
            self.sort = key;
            self.descending = key.default_descending();
        }
        let selected_id = self.selected_item().map(|i| i.id.clone());
        sort_items(&mut self.items, self.sort, self.descending);
        self.selected = selected_id.and_then(|id| self.items.iter().position(|i| i.id == id));
    }

    pub fn selected_item(&self) -> Option<&CaptureItem> {
        self.selected.and_then(|i| self.items.get(i))
    }

    /// Arrow-key navigation: moves the selection by `delta` items, clamped.
    pub fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            self.selected = None;
            return;
        }
        let last = self.items.len() - 1;
        let cur = self.selected.unwrap_or(0) as isize;
        let next = if self.selected.is_none() {
            0
        } else {
            cur + delta
        };
        self.selected = Some(next.clamp(0, last as isize) as usize);
    }

    /// The day group headers show only when sorting by date (OD-20); any other sort is a flat grid.
    pub fn group_by_day(&self) -> bool {
        self.sort == SortKey::Date
    }

    /// Arrow up and down: moves the selection one grid row, skipping day headers.
    pub fn move_vertical(&mut self, rows: &[Row], dir: isize) {
        if self.items.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            None => 0,
            Some(cur) => vertical_target(rows, cur, dir),
        });
    }

    pub fn empty_kind(&self) -> Option<EmptyKind> {
        if !self.items.is_empty() || self.status != LibraryStatus::Ready {
            return None;
        }
        let q = self.query.trim();
        // The kind is the page's (the sidebar): an empty Recordings page is "nothing captured".
        if q.is_empty() && self.remote.is_none() {
            Some(EmptyKind::NoCaptures)
        } else {
            Some(EmptyKind::NoResults {
                query: q.to_owned(),
            })
        }
    }
}

/// One row of the virtualized library list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A day header ("Today", "Yesterday", a date) with the number of captures under it.
    Header { label: String, count: usize },
    /// A run of grid cards (item indexes).
    Grid(std::ops::Range<usize>),
    /// A run of cards of files being imported that have no capture yet (indexes into the
    /// pending list; IMP-5).
    Pending(std::ops::Range<usize>),
}

pub const CARD_MIN_WIDTH: f32 = 232.;
pub const GRID_GAP: f32 = 16.;

/// Columns for a grid of `available` px: as many `CARD_MIN_WIDTH` cards as fit (min 1).
pub fn columns_for_width(available: f32) -> usize {
    let n = ((available + GRID_GAP) / (CARD_MIN_WIDTH + GRID_GAP)).floor();
    if n < 1.0 {
        1
    } else {
        n as usize
    }
}

/// Splits `items` (already sorted) into rows of `cols` cards. With `days`, a header row starts
/// every run of captures made on the same local day, with the run's count; the order is the
/// items' order, so an ascending date sort lists the oldest day first.
pub fn build_rows(
    items: &[CaptureItem],
    cols: usize,
    days: Option<(&LocalClock, OffsetDateTime)>,
) -> Vec<Row> {
    let cols = cols.max(1);
    let push_grid = |rows: &mut Vec<Row>, mut start: usize, end: usize| {
        while start < end {
            let stop = (start + cols).min(end);
            rows.push(Row::Grid(start..stop));
            start = stop;
        }
    };
    let mut rows = Vec::new();
    let Some((clock, now)) = days else {
        push_grid(&mut rows, 0, items.len());
        return rows;
    };
    let day_of = |i: &CaptureItem| i.created.to_offset(clock.offset).date();
    let mut start = 0;
    while start < items.len() {
        let day = day_of(&items[start]);
        let end = start
            + items[start..]
                .iter()
                .take_while(|i| day_of(i) == day)
                .count();
        rows.push(Row::Header {
            label: clock.day_label_at(items[start].created, now),
            count: end - start,
        });
        push_grid(&mut rows, start, end);
        start = end;
    }
    rows
}

/// [`build_rows`] with `pending` cards of files that are being imported and have no capture yet.
/// With day groups they go at the top of "Today" (a new "Today" group when the page has none);
/// without (a sort that is not by date) at the top of the flat grid.
pub fn build_rows_with_pending(
    items: &[CaptureItem],
    cols: usize,
    days: Option<(&LocalClock, OffsetDateTime)>,
    pending: usize,
) -> Vec<Row> {
    let mut rows = build_rows(items, cols, days);
    if pending == 0 {
        return rows;
    }
    let cols = cols.max(1);
    let mut extra = Vec::new();
    let mut start = 0;
    while start < pending {
        let stop = (start + cols).min(pending);
        extra.push(Row::Pending(start..stop));
        start = stop;
    }
    let Some((clock, now)) = days else {
        extra.append(&mut rows);
        return extra;
    };
    let today = clock.day_label_at(now, now);
    match rows.first_mut() {
        Some(Row::Header { label, count }) if *label == today => {
            *count += pending;
            rows.splice(1..1, extra);
        }
        _ => {
            let mut head = vec![Row::Header {
                label: today,
                count: pending,
            }];
            head.append(&mut extra);
            head.append(&mut rows);
            rows = head;
        }
    }
    rows
}

/// Index of the row that contains item `ix`.
pub fn row_of_item(rows: &[Row], ix: usize) -> Option<usize> {
    rows.iter().position(|r| match r {
        Row::Header { .. } | Row::Pending(_) => false,
        Row::Grid(g) => g.contains(&ix),
    })
}

/// The item one grid row up (`dir` < 0) or down (`dir` > 0) from `ix`, in the same column or the
/// last card of a shorter row; header rows are skipped. Stays put when there is no such row.
pub fn vertical_target(rows: &[Row], ix: usize, dir: isize) -> usize {
    let Some(r) = row_of_item(rows, ix) else {
        return ix;
    };
    let Row::Grid(here) = &rows[r] else {
        return ix;
    };
    let col = ix - here.start;
    let step = if dir < 0 { -1 } else { 1 };
    let mut at = r as isize + step;
    while at >= 0 && (at as usize) < rows.len() {
        if let Row::Grid(g) = &rows[at as usize] {
            return (g.start + col).min(g.end - 1);
        }
        at += step;
    }
    ix
}

/// `1:05`, `1:02:03` (tabular numbers are applied by the UI).
pub fn format_duration(ticks: i64) -> String {
    let secs = (ticks.max(0) / TICKS_PER_SECOND) as u64;
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// MB = 10^6 bytes (specs convention).
/// A thin wrapper over `cm_i18n::fmt::size` (decimal comma in Vietnamese; English unchanged).
pub fn format_size(bytes: u64) -> String {
    fmt::size(bytes)
}

fn ymd_of(t: OffsetDateTime) -> fmt::Ymd {
    fmt::Ymd::new(t.year(), u8::from(t.month()), t.day())
}

/// Local-time formatting. The UTC offset is read once at startup, before other threads
/// exist (the `time` crate refuses to read it from a multi-threaded process).
#[derive(Debug, Clone, Copy)]
pub struct LocalClock {
    pub offset: UtcOffset,
}

impl LocalClock {
    pub fn detect() -> LocalClock {
        LocalClock {
            offset: UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC),
        }
    }

    /// `Today 14:05`, `Yesterday 09:30`, else `Oct 12, 2026`.
    pub fn format_date_at(&self, t: OffsetDateTime, now: OffsetDateTime) -> String {
        let (t, now) = (t.to_offset(self.offset), now.to_offset(self.offset));
        let hm = format!("{:02}:{:02}", t.hour(), t.minute());
        if t.date() == now.date() {
            tr!("Today {time}", time = hm)
        } else if now.date().previous_day() == Some(t.date()) {
            tr!("Yesterday {time}", time = hm)
        } else {
            tr!(
                "{month} {day}, {year}",
                month = fmt::month_short(cm_i18n::language(), u8::from(t.month())),
                day = t.day(),
                year = t.year()
            )
        }
    }

    /// A day header: `Today`, `Yesterday`, else `Oct 12` (this year) or `Oct 12, 2025`.
    pub fn day_label_at(&self, t: OffsetDateTime, now: OffsetDateTime) -> String {
        let (t, now) = (t.to_offset(self.offset), now.to_offset(self.offset));
        fmt::day_label(ymd_of(t), ymd_of(now))
    }

    pub fn format_date(&self, t: OffsetDateTime) -> String {
        self.format_date_at(t, OffsetDateTime::now_utc())
    }
}

#[cfg(test)]
mod tests {
    /// LIB-1: the Uploaded and Published chips toggle, one at a time, and keep the page's kind
    /// (the sidebar sets it); the daemon gets `kind` and `status` for the query. The kind chips are
    /// gone (OD-16).
    /// QA L4: no Duration order on the Screenshots or Click Guides page, and a Duration sort
    /// falls back to Date when the page changes to one of them.
    #[test]
    fn duration_sort_is_not_offered_for_pictures() {
        assert!(!SortKey::offered(Filter::Screenshots).contains(&SortKey::Duration));
        assert!(!SortKey::offered(Filter::Guides).contains(&SortKey::Duration));
        assert!(SortKey::offered(Filter::Recordings).contains(&SortKey::Duration));
        assert!(SortKey::offered(Filter::All).contains(&SortKey::Duration));
        let mut s = LibraryState::new(LibraryStatus::Ready);
        s.sort = SortKey::Duration;
        s.filter = Filter::Screenshots;
        s.drop_inapplicable_chips();
        assert_eq!(s.sort, SortKey::Date);
    }

    #[test]
    fn lib1_remote_chips_toggle_and_keep_the_kind() {
        let mut s = LibraryState::new(LibraryStatus::Ready);
        assert_eq!(
            Chip::ALL,
            [
                Chip::NeedsAttention,
                Chip::Imported,
                Chip::Uploaded,
                Chip::Published,
                Chip::Boards,
            ]
        );
        assert!(!s.chip_active(Chip::Uploaded) && !s.chip_active(Chip::Published));
        s.filter = Filter::Recordings;
        assert!(s.click_chip(Chip::Uploaded));
        assert!(s.chip_active(Chip::Uploaded));
        assert_eq!(s.remote.map(RemoteFilter::daemon_status), Some("uploaded"));
        assert_eq!(s.filter.daemon_kind(), Some("recording"));
        // One remote chip at a time: Published replaces Uploaded.
        assert!(s.click_chip(Chip::Published));
        assert!(!s.chip_active(Chip::Uploaded) && s.chip_active(Chip::Published));
        // Clicking it again turns it off; the kind stays.
        assert!(s.click_chip(Chip::Published));
        assert_eq!(s.remote, None);
        assert_eq!(s.filter, Filter::Recordings);
        assert_eq!(Chip::Uploaded.glyph(), "cloud-upload");
        assert_eq!(Chip::Published.glyph(), "send");
    }

    /// The chips follow the page: Audio has no Boards, Click Guides or Published; only recordings
    /// are published; a hidden chip is switched off when the page changes.
    #[test]
    fn lib1_chips_follow_the_page() {
        let on = |f: Filter| {
            let mut s = LibraryState::new(LibraryStatus::Ready);
            s.filter = f;
            s.page_chips()
        };
        // "Needs attention" has no count yet, so it is not shown (S037).
        assert_eq!(
            on(Filter::All),
            [
                Chip::Imported,
                Chip::Uploaded,
                Chip::Published,
                Chip::Boards
            ]
        );
        assert_eq!(
            on(Filter::Recordings),
            [Chip::Imported, Chip::Uploaded, Chip::Published]
        );
        assert_eq!(
            on(Filter::Screenshots),
            [Chip::Imported, Chip::Uploaded, Chip::Boards]
        );
        assert_eq!(on(Filter::Audio), [Chip::Imported, Chip::Uploaded]);
        // The statuses combine with every type, Click Guides included (S037).
        assert_eq!(on(Filter::Guides), [Chip::Imported, Chip::Uploaded]);
        let mut s = LibraryState::new(LibraryStatus::Ready);
        s.boards_only = true;
        s.remote = Some(RemoteFilter::Published);
        s.attention_only = true;
        s.filter = Filter::Audio;
        s.drop_inapplicable_chips();
        assert!(!s.boards_only && s.remote.is_none() && !s.attention_only);
    }

    /// S037: the status chips combine with the type of the sidebar. Uploaded and Published go to
    /// the daemon with the page's kind; the chip bar holds no type.
    #[test]
    fn s037_status_combines_with_every_type() {
        let kinds = [
            (Filter::All, None),
            (Filter::Recordings, Some("recording")),
            (Filter::Screenshots, Some("screenshot")),
            (Filter::Audio, Some("audio")),
            (Filter::Guides, Some("guide")),
        ];
        for (page, kind) in kinds {
            let mut s = LibraryState::new(LibraryStatus::Ready);
            s.filter = page;
            assert!(s.click_chip(Chip::Uploaded), "{page:?}");
            assert_eq!(s.filter.daemon_kind(), kind);
            assert_eq!(s.remote.map(RemoteFilter::daemon_status), Some("uploaded"));
            // Imported is filtered here and keeps the type.
            assert!(!s.click_chip(Chip::Imported));
            assert!(s.chip_active(Chip::Imported) && s.chip_active(Chip::Uploaded));
            assert_eq!(s.filter, page);
            // Changing the type keeps the statuses that mean something there.
            s.filter = Filter::Recordings;
            s.drop_inapplicable_chips();
            assert!(s.chip_active(Chip::Imported) && s.chip_active(Chip::Uploaded));
        }
        // Published stays on All captures and Recordings, as before.
        for page in [Filter::Screenshots, Filter::Audio, Filter::Guides] {
            assert!(!Chip::Published.applies_to(page), "{page:?}");
        }
        assert!(Chip::Published.applies_to(Filter::All));
        assert!(Chip::Published.applies_to(Filter::Recordings));
    }

    /// S037: Needs attention comes first, and shows only while its count is above 0 (or while it
    /// is on); Boards is the last chip, a view after the statuses.
    #[test]
    fn s037_needs_attention_is_first_and_hidden_at_zero() {
        assert!(!attention_chip_visible(None, false));
        assert!(!attention_chip_visible(Some(0), false));
        assert!(attention_chip_visible(Some(1), false));
        assert!(attention_chip_visible(Some(0), true), "on stays visible");
        assert_eq!(Chip::ALL.first(), Some(&Chip::NeedsAttention));
        assert_eq!(Chip::ALL.last(), Some(&Chip::Boards));
        let mut s = LibraryState::new(LibraryStatus::Ready);
        let t = s.begin_load();
        s.finish_load(
            t,
            Ok(PageCollector {
                skipped: 0,
                cap: 500,
                items: vec![published_item(
                    "warned",
                    "2026-10-22T10:00:00Z",
                    Some("Captions not added"),
                    5,
                )],
                truncated: false,
            }),
        );
        s.update_remote_counts();
        assert_eq!(
            s.page_chips(),
            [
                Chip::NeedsAttention,
                Chip::Imported,
                Chip::Uploaded,
                Chip::Published,
                Chip::Boards
            ]
        );
        let published = vec![("a".to_owned(), true), ("b".to_owned(), false)];
        let none = std::collections::HashSet::new();
        assert_eq!(attention_count(&published, &none), 1);
        let waiting: std::collections::HashSet<String> = ["b".to_owned()].into();
        assert_eq!(attention_count(&published, &waiting), 2);
    }

    /// S037: the search field names its scope.
    #[test]
    fn s037_search_placeholder_follows_the_type() {
        assert_eq!(search_placeholder(Filter::All), "Search all captures");
        assert_eq!(search_placeholder(Filter::Recordings), "Search recordings");
        assert_eq!(
            search_placeholder(Filter::Screenshots),
            "Search screenshots"
        );
        assert_eq!(search_placeholder(Filter::Audio), "Search audio");
        assert_eq!(search_placeholder(Filter::Guides), "Search click guides");
    }

    /// LIB-1: the chips show how many captures are uploaded and published, counted from an
    /// unfiltered load only; a filtered load keeps the old numbers, and an empty result under a
    /// remote chip reads as "no results", not as an empty library.
    #[test]
    fn lib1_chip_counts_come_from_unfiltered_loads() {
        let mut s = LibraryState::new(LibraryStatus::Ready);
        let mut a = item("shot_a", "A", "2026-10-20T09:30:00Z", None, 1);
        a.uploads = parse_uploads(Some(
            &json!([{"destination_id": "d", "destination": "Drive",
            "what": "export", "bytes": 1, "at": "2026-10-20T09:30:00Z"}]),
        ));
        let b = item("shot_b", "B", "2026-10-20T09:31:00Z", None, 1);
        let t = s.begin_load();
        let mut c = PageCollector::new(10);
        c.items = vec![a.clone(), b];
        s.finish_load(t, Ok(c));
        s.update_remote_counts();
        assert_eq!(s.chip_count(Chip::Uploaded), Some(1));
        assert_eq!(s.chip_count(Chip::Published), Some(0));
        // Counts also come from a kind page's own load, so they match the page's list.
        s.filter = Filter::Screenshots;
        let t = s.begin_load();
        let mut c = PageCollector::new(10);
        c.items = vec![item("shot_c", "C", "2026-10-20T09:32:00Z", None, 1)];
        s.finish_load(t, Ok(c));
        s.update_remote_counts();
        assert_eq!(s.chip_count(Chip::Uploaded), Some(0));
        s.filter = Filter::All;
        let t = s.begin_load();
        let mut c = PageCollector::new(10);
        c.items = vec![
            a.clone(),
            item("shot_b", "B", "2026-10-20T09:31:00Z", None, 1),
        ];
        s.finish_load(t, Ok(c));
        s.update_remote_counts();
        assert_eq!(s.chip_count(Chip::Uploaded), Some(1));
        s.click_chip(Chip::Uploaded);
        let t = s.begin_load();
        let mut c = PageCollector::new(10);
        c.items = vec![a];
        s.finish_load(t, Ok(c));
        s.update_remote_counts();
        assert_eq!(
            s.chip_count(Chip::Uploaded),
            Some(1),
            "kept from the full load"
        );
        let t = s.begin_load();
        s.click_chip(Chip::Published);
        s.finish_load(t, Ok(PageCollector::new(10)));
        assert_eq!(
            s.empty_kind(),
            Some(EmptyKind::NoResults {
                query: String::new()
            })
        );
    }

    /// LIB-1: the card menu has Open, Reveal, Rename and Delete, in that order, with the platform's
    /// word for the file manager.
    #[test]
    fn lib1_card_menu_actions() {
        let labels: Vec<_> = CardAction::ALL
            .iter()
            .map(|a| a.label(Platform::Mac))
            .collect();
        assert_eq!(
            labels,
            [
                "Open",
                "Reveal in Finder",
                "Rename\u{2026}",
                "Delete\u{2026}"
            ]
        );
        assert_eq!(CardAction::Reveal.label(Platform::Linux), "Show in folder");
        assert_eq!(
            CardAction::Reveal.label(Platform::Windows),
            "Show in File Explorer"
        );
    }

    /// Board R1: every row has an icon; only Open has a key hint (Enter), and Delete has none.
    #[test]
    fn card_menu_icons_and_hints() {
        for a in CardAction::ALL.iter().chain([&CardAction::CopyLink]) {
            assert!(!a.glyph().is_empty());
        }
        assert_eq!(CardAction::Open.hint(), Some("\u{21B5}"));
        for a in [
            CardAction::CopyLink,
            CardAction::Reveal,
            CardAction::Rename,
            CardAction::Delete,
        ] {
            assert_eq!(a.hint(), None, "{a:?}");
        }
    }

    fn published_item(
        id: &str,
        published_at: &str,
        warning: Option<&str>,
        size: u64,
    ) -> CaptureItem {
        let mut v = rec(id, id, "recording", "2026-10-20T09:30:00Z", Some(1), size);
        v["publishes"] = json!([{ "platform": "youtube", "account": "Chan", "remote_id": id,
            "url": format!("https://youtu.be/{id}"), "privacy": "unlisted",
            "privacy_requested": "unlisted", "status": "processed",
            "warnings": warning.into_iter().collect::<Vec<_>>(), "at": published_at }]);
        CaptureItem::from_json(&v).unwrap()
    }

    /// S22: with Published on the sort control says "Date published" in place of "Date" (a flat
    /// grid), orders by the newest publish, and goes back when the chip goes off.
    #[test]
    fn s22_date_published_sort_follows_the_published_chip() {
        let mut s = LibraryState::new(LibraryStatus::Ready);
        s.filter = Filter::Recordings;
        let labels = |s: &LibraryState| -> Vec<String> {
            s.offered_sorts().iter().map(|k| k.label()).collect()
        };
        assert_eq!(labels(&s), ["Date", "Title", "Duration", "Size"]);
        assert!(s.group_by_day());
        s.click_chip(Chip::Published);
        assert_eq!(labels(&s), ["Date published", "Title", "Duration", "Size"]);
        assert_eq!(s.sort, SortKey::DatePublished);
        assert!(!s.group_by_day(), "no day headers under Date published");
        // Newest publish first; created dates play no part.
        let mut items = vec![
            published_item("a", "2026-10-21T10:00:00Z", None, 1),
            published_item("b", "2026-10-23T10:00:00Z", None, 1),
            published_item("c", "2026-10-22T10:00:00Z", None, 1),
        ];
        sort_items(&mut items, SortKey::DatePublished, true);
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["b", "c", "a"]);
        // A chosen Title order is kept; only Date and Date published swap.
        s.set_sort(SortKey::Title);
        s.click_chip(Chip::Published);
        assert_eq!(s.sort, SortKey::Title);
        s.click_chip(Chip::Published);
        s.set_sort(SortKey::DatePublished);
        s.click_chip(Chip::Published);
        assert_eq!(s.sort, SortKey::Date, "back to Date with the chip off");
        // Needs attention is a published view too.
        s.click_chip(Chip::NeedsAttention);
        assert_eq!(s.sort, SortKey::DatePublished);
        // The old deep link: Recordings, Published on.
        let mut d = LibraryState::new(LibraryStatus::Ready);
        d.show_published();
        assert_eq!(d.filter, Filter::Recordings);
        assert_eq!(d.remote, Some(RemoteFilter::Published));
        assert_eq!(d.sort, SortKey::DatePublished);
        assert!(d.chip_active(Chip::Published));
        // Leaving for a page without the chip drops it and the sort.
        d.filter = Filter::Audio;
        d.drop_inapplicable_chips();
        assert_eq!((d.remote, d.sort), (None, SortKey::Date));
    }

    /// S22: "Needs attention" counts publishes with a warning and uploads waiting for a sign-in,
    /// is hidden while unknown, filters the loaded page, and the title reads "2 of 61 \u{b7} ...".
    #[test]
    fn s22_needs_attention_chip_counts_and_filters() {
        let mut s = LibraryState::new(LibraryStatus::Ready);
        s.filter = Filter::Recordings;
        assert_eq!(
            s.chip_count(Chip::NeedsAttention),
            None,
            "unknown until loaded"
        );
        let t = s.begin_load();
        s.finish_load(
            t,
            Ok(PageCollector {
                skipped: 0,
                cap: 500,
                items: vec![
                    published_item("ok", "2026-10-23T10:00:00Z", None, 5),
                    published_item(
                        "warned",
                        "2026-10-22T10:00:00Z",
                        Some("Captions not added"),
                        5,
                    ),
                    published_item("waiting", "2026-10-21T10:00:00Z", None, 5),
                    CaptureItem::from_json(&rec(
                        "never",
                        "never",
                        "recording",
                        "2026-10-20T09:30:00Z",
                        Some(1),
                        5,
                    ))
                    .unwrap(),
                ],
                truncated: false,
            }),
        );
        s.update_remote_counts();
        assert_eq!(s.chip_count(Chip::Published), Some(3));
        assert_eq!(s.chip_count(Chip::NeedsAttention), Some(1));
        let ids = ["waiting".to_owned(), "never".to_owned()];
        assert!(s.set_waiting(ids.iter()));
        assert!(!s.set_waiting(ids.iter()), "unchanged");
        // A capture that was never published never needs attention, waiting or not.
        assert_eq!(s.chip_count(Chip::NeedsAttention), Some(2));
        // Off: every loaded capture shows; on: the two.
        assert_eq!(s.visible_items().len(), 4);
        assert!(!s.click_chip(Chip::NeedsAttention), "filtered on this side");
        let shown: Vec<String> = s.visible_items().into_iter().map(|i| i.id).collect();
        assert_eq!(shown, ["warned", "waiting"]);
        assert!(s.published_mode());
        assert_eq!(
            s.count_note(Some(61)).as_deref(),
            Some("2 of 61 \u{00B7} needs attention")
        );
        s.click_chip(Chip::NeedsAttention);
        // The Published chip: the daemon filters, the title says so.
        s.click_chip(Chip::Published);
        assert_eq!(
            s.count_note(Some(61)).as_deref(),
            Some("4 of 61 \u{00B7} published")
        );
        assert_eq!(s.count_note(None), None, "no made-up total");
        s.click_chip(Chip::Published);
        assert_eq!(s.count_note(Some(61)), None, "no filter, no note");
    }

    /// S22: the menu of a published recording: Open, Open on YouTube, Copy YouTube link, then the
    /// file's own items (Copy file link when an upload has one), Rename, Delete.
    #[test]
    fn s22_published_menu_entries() {
        let mut it = published_item("p", "2026-10-23T10:00:00Z", None, 5);
        let labels = |it: &CaptureItem| -> Vec<String> {
            CardAction::for_item(it)
                .iter()
                .map(|a| a.label_for(Platform::Mac, Some(it)))
                .collect()
        };
        assert_eq!(
            labels(&it),
            [
                "Open",
                "Open on YouTube",
                "Copy YouTube link",
                "Reveal in Finder",
                "Rename\u{2026}",
                "Delete\u{2026}"
            ]
        );
        it.uploads = parse_uploads(Some(&json!([{ "destination": "Drive", "what": "export",
            "share_link": "https://d/1" }])));
        assert_eq!(
            labels(&it),
            [
                "Open",
                "Open on YouTube",
                "Copy YouTube link",
                "Copy file link",
                "Reveal in Finder",
                "Rename\u{2026}",
                "Delete\u{2026}"
            ]
        );
        // Never published: no publish entries, and "Copy link" keeps its name.
        let plain =
            CaptureItem::from_json(&rec("a", "T", "recording", "2026-10-20T09:30:00Z", None, 1))
                .unwrap();
        assert_eq!(CardAction::for_item(&plain), CardAction::ALL.to_vec());
        for a in [CardAction::OpenPublished, CardAction::CopyPublishedLink] {
            assert!(!a.glyph().is_empty());
            assert_eq!(a.hint(), None);
        }
    }

    /// STO-4.AC3: "Copy link" is in the menu of a capture whose upload has a link, after Reveal.
    #[test]
    fn sto4_copy_link_only_with_a_share_link() {
        let mut v = rec("a", "T", "recording", "2026-10-20T09:30:00Z", None, 1);
        let plain = CaptureItem::from_json(&v).unwrap();
        assert_eq!(CardAction::for_item(&plain), CardAction::ALL.to_vec());
        v["uploads"] = json!([{ "destination": "Drive", "what": "export", "share_link": null }]);
        assert_eq!(
            CardAction::for_item(&CaptureItem::from_json(&v).unwrap()),
            CardAction::ALL.to_vec(),
            "an upload without a link (local folder, link mode none)"
        );
        v["uploads"] =
            json!([{ "destination": "Drive", "what": "export", "share_link": "https://d/1" }]);
        let labels: Vec<_> = CardAction::for_item(&CaptureItem::from_json(&v).unwrap())
            .iter()
            .map(|a| a.label(Platform::Mac))
            .collect();
        assert_eq!(
            labels,
            [
                "Open",
                "Reveal in Finder",
                "Copy link",
                "Rename\u{2026}",
                "Delete\u{2026}"
            ]
        );
    }

    use super::*;
    use crate::platform_text::Platform;
    use serde_json::json;

    fn rec(id: &str, title: &str, kind: &str, created: &str, dur: Option<i64>, size: u64) -> Value {
        json!({"id": id, "title": title, "kind": kind, "created": created,
               "duration": dur, "width": 1920, "height": 1080, "size_bytes": size, "broken": false})
    }

    fn page(items: Vec<Value>, next: Option<&str>) -> Value {
        json!({"captures": items, "next_cursor": next})
    }

    fn item(id: &str, title: &str, created: &str, dur: Option<i64>, size: u64) -> CaptureItem {
        CaptureItem::from_json(&rec(id, title, "screenshot", created, dur, size)).unwrap()
    }

    /// LIB-1.AC7: the sidebar counts come from the whole library, whatever page is showing: a
    /// kind page, a search or a chip is not "the whole library" (the view then asks for its own
    /// unfiltered list), and the totals count each kind.
    #[test]
    fn lib1_ac7_sidebar_counts_follow_the_whole_library() {
        let mk = |id: &str, kind: &str| {
            CaptureItem::from_json(&rec(id, id, kind, "2026-10-20T09:30:00Z", None, 1)).unwrap()
        };
        let items = vec![
            mk("a", "screenshot"),
            mk("b", "screenshot"),
            mk("c", "recording"),
            mk("d", "audio"),
        ];
        let c = kind_counts(&items);
        assert_eq!(
            (c.screenshots, c.recordings, c.audio),
            (Some(2), Some(1), Some(1))
        );
        // A new screenshot moves only its own count, and never backwards.
        let mut more = items.clone();
        more.push(mk("e", "screenshot"));
        let c2 = kind_counts(&more);
        assert_eq!(c2.screenshots, Some(3));
        assert_eq!((c2.recordings, c2.audio), (c.recordings, c.audio));
        // An empty library reads 0, not "unknown".
        assert_eq!(kind_counts(&[]).screenshots, Some(0));

        let mut s = LibraryState::new(LibraryStatus::Ready);
        assert!(s.is_whole_library());
        s.filter = Filter::Screenshots;
        assert!(!s.is_whole_library(), "a kind page lists one kind");
        s.filter = Filter::All;
        s.query = "k8s".into();
        assert!(!s.is_whole_library(), "a search lists some captures");
        s.query.clear();
        assert!(s.click_chip(Chip::Uploaded));
        assert!(!s.is_whole_library(), "a chip lists some captures");
    }

    #[test]
    fn parses_daemon_items() {
        let v = rec(
            "shot_1",
            "K8s probes",
            "recording",
            "2026-10-20T09:30:00Z",
            Some(240_000),
            5_000_000,
        );
        let i = CaptureItem::from_json(&v).unwrap();
        assert_eq!(i.kind, Kind::Recording);
        assert_eq!(i.duration_ticks, Some(240_000));
        assert_eq!((i.width, i.height), (Some(1920), Some(1080)));
        assert!(CaptureItem::from_json(&json!({"id": "x"})).is_none());
        assert!(
            CaptureItem::from_json(&rec("x", "t", "gif", "2026-10-20T09:30:00Z", None, 1))
                .is_none()
        );
    }

    /// LIB-1.AC1 (data side): 1,000 captures arrive as 200-item pages and are all collected.
    #[test]
    fn collects_all_pages() {
        let mut c = PageCollector::new(5000);
        let mut cursor = Some("0".to_owned());
        let mut fetched = 0;
        while let Some(cur) = cursor {
            let start: usize = cur.parse().unwrap();
            let end = (start + 200).min(1000);
            let items = (start..end)
                .map(|n| {
                    rec(
                        &format!("shot_{n}"),
                        &format!("T{n}"),
                        "screenshot",
                        "2026-10-20T09:30:00Z",
                        None,
                        1,
                    )
                })
                .collect();
            let next = (end < 1000).then(|| end.to_string());
            cursor = c.push(&page(items, next.as_deref())).unwrap();
            fetched += 1;
        }
        assert_eq!(fetched, 5);
        assert_eq!(c.items.len(), 1000);
        assert!(!c.truncated);
    }

    #[test]
    fn collector_caps_and_reports_truncation() {
        let mut c = PageCollector::new(2);
        let p = page(
            vec![
                rec("a", "A", "screenshot", "2026-10-20T09:30:00Z", None, 1),
                rec("b", "B", "screenshot", "2026-10-20T09:30:00Z", None, 1),
                rec("c", "C", "screenshot", "2026-10-20T09:30:00Z", None, 1),
            ],
            Some("3"),
        );
        assert_eq!(c.push(&p).unwrap(), None);
        assert_eq!(c.items.len(), 2);
        assert!(c.truncated);
        assert!(PageCollector::new(5).push(&json!({"nope": 1})).is_err());
    }

    /// LIB-1: sort by date (default), title, duration, size.
    #[test]
    fn sorting() {
        let mut v = vec![
            item("a", "banana", "2026-10-18T00:00:00Z", Some(10), 300),
            item("b", "Apple", "2026-10-20T00:00:00Z", Some(30), 100),
            item("c", "cherry", "2026-10-19T00:00:00Z", None, 200),
        ];
        sort_items(&mut v, SortKey::Date, true);
        assert_eq!(ids(&v), ["b", "c", "a"]);
        sort_items(&mut v, SortKey::Title, false);
        assert_eq!(ids(&v), ["b", "a", "c"]); // case-insensitive
        sort_items(&mut v, SortKey::Size, true);
        assert_eq!(ids(&v), ["a", "c", "b"]);
        sort_items(&mut v, SortKey::Duration, true);
        assert_eq!(ids(&v), ["b", "a", "c"]); // None sorts as smallest
    }

    fn ids(v: &[CaptureItem]) -> Vec<&str> {
        v.iter().map(|i| i.id.as_str()).collect()
    }

    /// A first load that never answers ends as a failure, not "Loading library..." for ever; a
    /// load that did answer (or was replaced) is left alone.
    #[test]
    fn a_load_that_never_answers_times_out_into_a_failure() {
        let mut s = LibraryState::new(LibraryStatus::Loading);
        let t = s.begin_load();
        assert!(s.load_timed_out(t, "too slow".into()));
        assert!(matches!(s.status, LibraryStatus::Failed(_)));

        let mut s = LibraryState::new(LibraryStatus::Loading);
        let t1 = s.begin_load();
        let _t2 = s.begin_load();
        assert!(
            !s.load_timed_out(t1, "too slow".into()),
            "a newer load runs"
        );
        assert!(matches!(s.status, LibraryStatus::Loading));

        let mut s = LibraryState::new(LibraryStatus::Loading);
        let t = s.begin_load();
        assert!(s.finish_load(t, Ok(PageCollector::new(10))));
        assert!(!s.load_timed_out(t, "too slow".into()));
        assert!(matches!(s.status, LibraryStatus::Ready));
    }

    #[test]
    fn stale_responses_are_dropped() {
        let mut s = LibraryState::new(LibraryStatus::Loading);
        let t1 = s.begin_load();
        let t2 = s.begin_load();
        let mut late = PageCollector::new(10);
        late.items
            .push(item("old", "old", "2026-10-20T00:00:00Z", None, 1));
        assert!(!s.finish_load(t1, Ok(late)));
        assert!(s.items().is_empty());
        let mut fresh = PageCollector::new(10);
        fresh
            .items
            .push(item("new", "new", "2026-10-20T00:00:00Z", None, 1));
        assert!(s.finish_load(t2, Ok(fresh)));
        assert_eq!(s.items().len(), 1);
        assert_eq!(s.status, LibraryStatus::Ready);
        assert_eq!(s.selected, Some(0));
    }

    /// LIB-1: empty state for an empty library versus a search without results.
    #[test]
    fn empty_states() {
        let mut s = LibraryState::new(LibraryStatus::Loading);
        assert_eq!(s.empty_kind(), None);
        let t = s.begin_load();
        s.finish_load(t, Ok(PageCollector::new(10)));
        assert_eq!(s.empty_kind(), Some(EmptyKind::NoCaptures));
        s.query = " readiness ".into();
        assert_eq!(
            s.empty_kind(),
            Some(EmptyKind::NoResults {
                query: "readiness".into()
            })
        );
    }

    /// LIB-2: while the library is unavailable the state is kept even after loads from the
    /// fallback folder.
    #[test]
    fn unavailable_status_sticks() {
        let st = LibraryStatus::Unavailable {
            hint: "Big Drive".into(),
            path: "/Volumes/Big Drive/Caps".into(),
            reason: "not found".into(),
            fallback_in_use: true,
        };
        let mut s = LibraryState::new(st.clone());
        let t = s.begin_load();
        s.finish_load(t, Ok(PageCollector::new(10)));
        assert_eq!(s.status, st);
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut s = LibraryState::new(LibraryStatus::Ready);
        s.move_selection(1);
        assert_eq!(s.selected, None);
        let t = s.begin_load();
        let mut c = PageCollector::new(10);
        for n in 0..5 {
            c.items
                .push(item(&format!("i{n}"), "t", "2026-10-20T00:00:00Z", None, 1));
        }
        s.finish_load(t, Ok(c));
        s.move_selection(1);
        assert_eq!(s.selected, Some(1));
        s.move_selection(10);
        assert_eq!(s.selected, Some(4));
        s.move_selection(-10);
        assert_eq!(s.selected, Some(0));
    }

    #[test]
    fn changing_sort_toggles_direction_on_same_key() {
        let mut s = LibraryState::new(LibraryStatus::Ready);
        assert!(s.descending);
        s.set_sort(SortKey::Date);
        assert!(!s.descending);
        s.set_sort(SortKey::Title);
        assert!(!s.descending); // titles default A to Z
        s.set_sort(SortKey::Size);
        assert!(s.descending);
        // Date sort groups by day; every other sort is a flat grid.
        let mut s = LibraryState::new(LibraryStatus::Ready);
        assert!(s.group_by_day());
        s.set_sort(SortKey::Title);
        assert!(!s.group_by_day());
        s.set_sort(SortKey::Date);
        assert!(s.group_by_day());
    }

    /// Board R1: cards 232 px minimum, 16 px gap.
    #[test]
    fn grid_columns() {
        assert_eq!(columns_for_width(100.), 1);
        assert_eq!(columns_for_width(232.), 1);
        assert_eq!(columns_for_width(480.), 2); // 232 + 16 + 232
        assert_eq!(columns_for_width(1000.), 4);
        assert_eq!(columns_for_width(1500.), 6);
    }

    fn utc() -> LocalClock {
        LocalClock {
            offset: UtcOffset::UTC,
        }
    }

    fn at(s: &str) -> OffsetDateTime {
        OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).unwrap()
    }

    fn days(n: usize, created: &[&str]) -> Vec<CaptureItem> {
        (0..n)
            .map(|i| item(&format!("i{i}"), "t", created[i], None, 1))
            .collect()
    }

    fn header(label: &str, count: usize) -> Row {
        Row::Header {
            label: label.to_owned(),
            count,
        }
    }

    /// OD-20: without day grouping the grid is flat, rows of `cols` cards.
    #[test]
    fn flat_rows_have_no_headers() {
        let items = days(7, &["2026-10-20T00:00:00Z"; 7]);
        let rows = build_rows(&items, 3, None);
        assert_eq!(
            rows,
            vec![Row::Grid(0..3), Row::Grid(3..6), Row::Grid(6..7)]
        );
        assert!(build_rows(&[], 3, None).is_empty());
        assert_eq!(row_of_item(&rows, 4), Some(1));
    }

    /// IMP-5: pending import cards go at the top of "Today" (joining its header), in a new "Today"
    /// group when the page has none, and at the top of a flat grid when the sort is not by date.
    #[test]
    fn pending_import_cards_lead_today_or_the_flat_grid() {
        let now = at("2026-10-23T12:00:00Z");
        let items = days(2, &["2026-10-23T11:00:00Z", "2026-10-22T20:00:00Z"]);
        let rows = build_rows_with_pending(&items, 2, Some((&utc(), now)), 3);
        assert_eq!(
            rows,
            vec![
                header("Today", 4),
                Row::Pending(0..2),
                Row::Pending(2..3),
                Row::Grid(0..1),
                header("Yesterday", 1),
                Row::Grid(1..2),
            ]
        );
        // The page has nothing from today: a Today group of its own comes first.
        let old = days(1, &["2026-10-20T11:00:00Z"]);
        let rows = build_rows_with_pending(&old, 2, Some((&utc(), now)), 1);
        assert_eq!(rows[0], header("Today", 1));
        assert_eq!(rows[1], Row::Pending(0..1));
        assert_eq!(rows.len(), 4);
        // An empty page still shows them.
        assert_eq!(
            build_rows_with_pending(&[], 2, Some((&utc(), now)), 1),
            vec![header("Today", 1), Row::Pending(0..1)]
        );
        // Not sorted by date: the top of the flat grid, no header.
        let rows = build_rows_with_pending(&items, 2, None, 1);
        assert_eq!(rows, vec![Row::Pending(0..1), Row::Grid(0..2)]);
        // None pending: unchanged.
        assert_eq!(
            build_rows_with_pending(&items, 2, None, 0),
            build_rows(&items, 2, None)
        );
        assert_eq!(row_of_item(&rows, 1), Some(1));
    }

    /// OD-20: date sort buckets by local day: Today, Yesterday, then dates, each with its count
    /// and its own grid rows (a day never shares a row with the next one).
    #[test]
    fn day_rows_bucket_today_yesterday_and_older() {
        let now = at("2026-10-23T12:00:00Z");
        let items = days(
            6,
            &[
                "2026-10-23T11:00:00Z",
                "2026-10-23T09:00:00Z",
                "2026-10-23T01:00:00Z",
                "2026-10-22T20:00:00Z",
                "2026-10-12T20:00:00Z",
                "2025-03-02T20:00:00Z",
            ],
        );
        let rows = build_rows(&items, 2, Some((&utc(), now)));
        assert_eq!(
            rows,
            vec![
                header("Today", 3),
                Row::Grid(0..2),
                Row::Grid(2..3),
                header("Yesterday", 1),
                Row::Grid(3..4),
                header("Oct 12", 1),
                Row::Grid(4..5),
                header("Mar 2, 2025", 1),
                Row::Grid(5..6),
            ]
        );
        // The count is the number of cards under the header.
        let total: usize = rows
            .iter()
            .filter_map(|r| match r {
                Row::Header { count, .. } => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(total, items.len());
        assert_eq!(row_of_item(&rows, 3), Some(4), "header rows hold no item");
    }

    /// OD-20: an ascending date sort still buckets by day, oldest day first.
    #[test]
    fn ascending_date_sort_lists_the_oldest_day_first() {
        let now = at("2026-10-23T12:00:00Z");
        let mut items = days(
            3,
            &[
                "2026-10-23T11:00:00Z",
                "2026-10-22T09:00:00Z",
                "2026-10-22T08:00:00Z",
            ],
        );
        sort_items(&mut items, SortKey::Date, false);
        let rows = build_rows(&items, 4, Some((&utc(), now)));
        assert_eq!(
            rows,
            vec![
                header("Yesterday", 2),
                Row::Grid(0..2),
                header("Today", 1),
                Row::Grid(2..3),
            ]
        );
    }

    /// Day grouping with nothing or one capture; the local day, not the UTC day, decides.
    #[test]
    fn day_rows_edge_cases_and_local_days() {
        let now = at("2026-10-23T12:00:00Z");
        assert!(build_rows(&[], 3, Some((&utc(), now))).is_empty());
        let one = days(1, &["2026-10-23T12:00:00Z"]);
        assert_eq!(
            build_rows(&one, 3, Some((&utc(), now))),
            vec![header("Today", 1), Row::Grid(0..1)]
        );
        // 23:30 UTC on the 22nd is 01:30 on the 23rd at +2: today there, yesterday in UTC.
        let late = days(1, &["2026-10-22T23:30:00Z"]);
        let plus2 = LocalClock {
            offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
        };
        assert_eq!(
            build_rows(&late, 3, Some((&plus2, now)))[0],
            header("Today", 1)
        );
        assert_eq!(
            build_rows(&late, 3, Some((&utc(), now)))[0],
            header("Yesterday", 1)
        );
    }

    /// Arrow up and down move one grid row and skip headers; a short last row takes its last card;
    /// left and right walk the item indexes across rows and groups.
    #[test]
    fn selection_skips_header_rows() {
        let now = at("2026-10-23T12:00:00Z");
        let items = days(
            6,
            &[
                "2026-10-23T11:00:00Z",
                "2026-10-23T10:00:00Z",
                "2026-10-23T09:00:00Z",
                "2026-10-22T20:00:00Z",
                "2026-10-22T19:00:00Z",
                "2026-10-12T20:00:00Z",
            ],
        );
        // cols 2: [H Today 3] [0 1] [2] [H Yesterday 2] [3 4] [H Oct 12] [5]
        let rows = build_rows(&items, 2, Some((&utc(), now)));
        assert_eq!(vertical_target(&rows, 1, 1), 2, "short row: last card");
        assert_eq!(vertical_target(&rows, 2, 1), 3, "over the header");
        assert_eq!(vertical_target(&rows, 4, 1), 5);
        assert_eq!(vertical_target(&rows, 3, -1), 2, "back over the header");
        assert_eq!(vertical_target(&rows, 4, -1), 2);
        assert_eq!(vertical_target(&rows, 0, -1), 0, "stays at the top");
        assert_eq!(vertical_target(&rows, 5, 1), 5, "stays at the bottom");

        let mut s = LibraryState::new(LibraryStatus::Ready);
        let t = s.begin_load();
        let mut c = PageCollector::new(10);
        c.items = items;
        s.finish_load(t, Ok(c));
        s.sort = SortKey::Date;
        s.selected = Some(0);
        s.move_vertical(&rows, 1);
        assert_eq!(s.selected, Some(2));
        s.move_vertical(&rows, 1);
        assert_eq!(s.selected, Some(3));
        s.move_selection(1);
        s.move_selection(1);
        assert_eq!(s.selected, Some(5), "right walks across groups");
        s.move_vertical(&rows, -1);
        assert_eq!(s.selected, Some(3));
        s.selected = None;
        s.move_vertical(&rows, 1);
        assert_eq!(s.selected, Some(0));
    }

    /// Day headers read Today, Yesterday, then a date (year only when it is not this one).
    #[test]
    fn day_labels() {
        let now = at("2026-10-23T12:00:00Z");
        let c = utc();
        assert_eq!(c.day_label_at(at("2026-10-23T01:00:00Z"), now), "Today");
        assert_eq!(c.day_label_at(at("2026-10-22T23:00:00Z"), now), "Yesterday");
        assert_eq!(c.day_label_at(at("2026-01-05T23:00:00Z"), now), "Jan 5");
        assert_eq!(
            c.day_label_at(at("2025-12-31T23:00:00Z"), now),
            "Dec 31, 2025"
        );
    }

    /// I18N P1: `cm_i18n::fmt` in English is byte-identical to `format_size` and `day_label_at`,
    /// so P2 can swap the callers; Vietnamese uses the decimal comma and its own day words.
    #[test]
    fn i18n_fmt_matches_the_library_formatters_in_english() {
        use cm_i18n::fmt::{day_label_in, size_in, Ymd};
        use cm_i18n::Lang;
        for bytes in [
            0u64,
            1,
            512,
            999,
            1_000,
            12_345,
            999_499,
            999_999,
            1_000_000,
            2_400_000,
            999_999_999,
            1_230_000_000,
            47_800_000_000,
        ] {
            assert_eq!(size_in(Lang::En, bytes), format_size(bytes), "{bytes}");
        }
        let now = at("2026-10-23T12:00:00Z");
        let c = utc();
        let ymd = |t: OffsetDateTime| Ymd::new(t.year(), u8::from(t.month()), t.day());
        for when in [
            "2026-10-23T01:00:00Z",
            "2026-10-22T23:00:00Z",
            "2026-01-05T23:00:00Z",
            "2025-12-31T23:00:00Z",
            "2026-10-12T08:00:00Z",
            "2024-02-29T08:00:00Z",
            "2026-12-01T00:00:00Z",
        ] {
            let t = at(when);
            assert_eq!(
                day_label_in(Lang::En, ymd(t), ymd(now)),
                c.day_label_at(t, now),
                "{when}"
            );
        }
        assert_eq!(size_in(Lang::Vi, 2_400_000), "2,4 MB");
        assert_eq!(
            day_label_in(Lang::Vi, ymd(at("2026-10-12T08:00:00Z")), ymd(now)),
            "12 thg 10"
        );
    }

    #[test]
    fn formatting() {
        assert_eq!(format_duration(65 * TICKS_PER_SECOND), "1:05");
        assert_eq!(format_duration(3723 * TICKS_PER_SECOND), "1:02:03");
        assert_eq!(format_duration(-5), "0:00");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2_400_000), "2.4 MB");
        assert_eq!(format_size(1_230_000_000), "1.23 GB");
    }

    #[test]
    fn dates_are_relative_in_local_time() {
        let clock = LocalClock {
            offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
        };
        let now = OffsetDateTime::parse(
            "2026-10-20T10:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        let p =
            |s| OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).unwrap();
        assert_eq!(
            clock.format_date_at(p("2026-10-20T07:05:00Z"), now),
            "Today 09:05"
        );
        assert_eq!(
            clock.format_date_at(p("2026-10-19T07:05:00Z"), now),
            "Yesterday 09:05"
        );
        assert_eq!(
            clock.format_date_at(p("2026-10-01T07:05:00Z"), now),
            "Oct 1, 2026"
        );
    }

    /// NFR-13: each card has one accessible sentence.
    #[test]
    fn accessible_label_lists_the_card_content() {
        let clock = LocalClock {
            offset: UtcOffset::UTC,
        };
        let mut i = item(
            "a",
            "K8s probes",
            "2020-01-02T03:04:00Z",
            Some(65 * TICKS_PER_SECOND),
            1,
        );
        i.kind = Kind::Recording;
        let l = i.accessible_label(&clock);
        assert!(l.starts_with("K8s probes, Recording, Jan 2, 2020"), "{l}");
        assert!(
            l.contains("1:05") && l.contains("1920 by 1080 pixels"),
            "{l}"
        );
    }

    /// I18N P2: the thin wrappers follow the language (decimal comma, day words) and stay
    /// byte-identical in English.
    #[test]
    fn wrappers_follow_the_language() {
        use cm_i18n::{with_language, Lang};
        assert_eq!(with_language(Lang::Vi, || format_size(2_400_000)), "2,4 MB");
        let now = at("2026-10-23T12:00:00Z");
        let c = utc();
        assert_eq!(
            with_language(Lang::Vi, || c.day_label_at(at("2026-10-23T01:00:00Z"), now)),
            "H\u{f4}m nay"
        );
        assert_eq!(
            with_language(Lang::Vi, || c.day_label_at(at("2026-10-22T23:00:00Z"), now)),
            "H\u{f4}m qua"
        );
        assert_eq!(
            with_language(Lang::Vi, || c.day_label_at(at("2026-01-05T23:00:00Z"), now)),
            "5 thg 1"
        );
        assert_eq!(
            with_language(Lang::En, || c
                .format_date_at(at("2025-03-04T10:00:00Z"), now)),
            "Mar 4, 2025"
        );
    }

    /// The ids of chips, sort keys and menu rows are not the (translated) labels.
    #[test]
    fn element_id_keys_are_the_english_names() {
        assert_eq!(Chip::Uploaded.key(), "Uploaded");
        assert_eq!(SortKey::Size.key(), "Size");
        assert_eq!(CardAction::Reveal.key(true), "Reveal in Finder");
        assert_eq!(CardAction::Reveal.label(Platform::Linux), "Show in folder");
        assert_eq!(
            CardAction::Reveal.label(Platform::Windows),
            "Show in File Explorer"
        );
    }

    /// VOC-1.AC10: an audio capture parses as its own kind with a duration, and appears in the
    /// unfiltered list next to the others.
    #[test]
    fn audio_captures_parse_and_list_with_the_rest() {
        let page = json!({"captures": [
            {"id": "rec_a", "title": "Weekly sync", "kind": "audio", "created": "2026-10-26T10:02:00Z",
             "duration": 5_544_000_i64, "size_bytes": 1000},
            {"id": "rec_b", "title": "Demo", "kind": "recording", "created": "2026-10-26T09:00:00Z",
             "duration": 120_000_i64},
        ]});
        let (items, _, skipped) = parse_page(&page).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(items[0].kind, Kind::Audio);
        assert_eq!(items[0].duration_ticks, Some(5_544_000));
        assert_eq!(items[0].kind.label(), "Audio");
        assert_eq!(items[1].kind, Kind::Recording);
        assert_eq!(Filter::Audio.daemon_kind(), Some("audio"));
        assert_eq!(Filter::All.daemon_kind(), None);
    }
}
