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
}

/// What the context menu of a library card offers (LIB-1): Open, Reveal in Finder, Rename, Delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Open,
    Reveal,
    /// Only for captures with an upload that has a share link (STO-4.AC3).
    CopyLink,
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
        v
    }

    pub fn label(self, mac: bool) -> String {
        match self {
            CardAction::Open => tr_ctx!("card menu", "Open"),
            CardAction::CopyLink => tr!("Copy link"),
            CardAction::Reveal if mac => tr!("Reveal in Finder"),
            CardAction::Reveal => tr!("Show in folder"),
            CardAction::Rename => tr_ctx!("card menu", "Rename\u{2026}"),
            CardAction::Delete => tr_ctx!("card menu", "Delete\u{2026}"),
        }
    }

    /// A stable element id part (never translated).
    pub fn key(self, mac: bool) -> &'static str {
        match self {
            CardAction::Open => "Open",
            CardAction::CopyLink => "Copy link",
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
        }
    }
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
}

impl CaptureItem {
    /// Parses one entry of `captures`; `None` if a required field is missing or malformed.
    pub fn from_json(v: &Value) -> Option<CaptureItem> {
        let kind = match v.get("kind")?.as_str()? {
            "screenshot" => Kind::Screenshot,
            "recording" => Kind::Recording,
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
        })
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
}

impl Filter {
    /// The `kind` argument of `list_library`.
    pub fn daemon_kind(self) -> Option<&'static str> {
        match self {
            Filter::Recordings => Some("recording"),
            Filter::Screenshots => Some("screenshot"),
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
    Uploaded,
    Published,
}

impl Chip {
    pub const ALL: [Chip; 2] = [Chip::Uploaded, Chip::Published];

    pub fn label(self) -> String {
        match self {
            Chip::Uploaded => tr_ctx!("filter chip", "Uploaded"),
            Chip::Published => tr_ctx!("filter chip", "Published"),
        }
    }

    /// A stable element id part (never translated).
    pub fn key(self) -> &'static str {
        match self {
            Chip::Uploaded => "Uploaded",
            Chip::Published => "Published",
        }
    }

    /// The leading icon of the chip.
    pub fn glyph(self) -> &'static str {
        match self {
            Chip::Uploaded => "cloud-upload",
            Chip::Published => "send",
        }
    }
}

/// LIB-1 sort orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Date,
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

    pub fn label(self) -> String {
        match self {
            SortKey::Date => tr_ctx!("sort key", "Date"),
            SortKey::Title => tr_ctx!("sort key", "Title"),
            SortKey::Duration => tr_ctx!("sort key", "Duration"),
            SortKey::Size => tr_ctx!("sort key", "Size"),
        }
    }

    /// A stable element id part (never translated).
    pub fn key(self) -> &'static str {
        match self {
            SortKey::Date => "Date",
            SortKey::Title => "Title",
            SortKey::Duration => "Duration",
            SortKey::Size => "Size",
        }
    }

    /// The leading icon of the sort segment.
    pub fn glyph(self) -> &'static str {
        match self {
            SortKey::Date => "calendar",
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
            Chip::Uploaded => self.remote == Some(RemoteFilter::Uploaded),
            Chip::Published => self.remote == Some(RemoteFilter::Published),
        }
    }

    /// A click on a chip: it toggles, and turning one on turns the other off. Returns whether the
    /// query changed (the caller reloads).
    pub fn click_chip(&mut self, chip: Chip) -> bool {
        let want = match chip {
            Chip::Uploaded => RemoteFilter::Uploaded,
            Chip::Published => RemoteFilter::Published,
        };
        self.remote = if self.remote == Some(want) {
            None
        } else {
            Some(want)
        };
        true
    }

    /// The count a chip shows, when known: the uploaded and published captures of the page's kind
    /// (all captures on "All captures").
    pub fn chip_count(&self, chip: Chip) -> Option<usize> {
        let (up, publ) = self.remote_counts?;
        match chip {
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
        }
    }

    /// Starts a (re)load and returns its generation token.
    pub fn begin_load(&mut self) -> u64 {
        self.generation += 1;
        // Old items stay on screen while a refresh runs; only a first load shows "Loading".
        self.generation
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

/// Index of the row that contains item `ix`.
pub fn row_of_item(rows: &[Row], ix: usize) -> Option<usize> {
    rows.iter().position(|r| match r {
        Row::Header { .. } => false,
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
    #[test]
    fn lib1_remote_chips_toggle_and_keep_the_kind() {
        let mut s = LibraryState::new(LibraryStatus::Ready);
        assert_eq!(Chip::ALL, [Chip::Uploaded, Chip::Published]);
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
        let labels: Vec<_> = CardAction::ALL.iter().map(|a| a.label(true)).collect();
        assert_eq!(
            labels,
            [
                "Open",
                "Reveal in Finder",
                "Rename\u{2026}",
                "Delete\u{2026}"
            ]
        );
        assert_eq!(CardAction::Reveal.label(false), "Show in folder");
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
            .map(|a| a.label(true))
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
        assert_eq!(CardAction::Reveal.label(false), "Show in folder");
    }
}
