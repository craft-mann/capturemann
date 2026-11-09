//! The Export popover's state (EXP-5, DESIGN 5.13): the choices (format, size, GIF width, "keep
//! under N MB"), what the size block shows, the requests it sends and how a late reply is
//! treated. No display and no daemon here: `ui::export_popover` draws it and makes the calls
//! (`estimate_export`, `export_video`), so everything below is tested with fakes.
//!
//! Decisions of Nov 9, 2026 (`specs/plans/release-0.3.1-plan.md` section 3): the popover always
//! opens on MP4 and the recording's size and nothing is stored between exports; the estimate is an
//! upper bound ("Up to 290 MB"), never a range; no presets; GIF has no estimate.

use cm_core::ErrorCode;
use serde_json::{json, Map, Value};

/// A GIF is for clips of at most this many seconds (the daemon refuses longer ones).
pub const GIF_MAX_SECS: f64 = 30.0;
/// The width a GIF starts with.
pub const DEFAULT_GIF_WIDTH: u32 = 800;
/// The "Keep the file under" value a GIF starts with, in MB.
pub const DEFAULT_MAX_MB: u32 = 10;
/// The largest "Keep the file under" value taken (a GIF is never this large; it only stops typos).
pub const MAX_MB_LIMIT: u32 = 5000;

/// The file formats of the chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp4,
    Webm,
    Gif,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Mp4, Format::Webm, Format::Gif];

    /// The `format` of `estimate_export` and `export_video`.
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Mp4 => "mp4",
            Format::Webm => "webm",
            Format::Gif => "gif",
        }
    }

    /// The name on the cards and the button: English in every language.
    pub fn label(self) -> &'static str {
        match self {
            Format::Mp4 => "MP4",
            Format::Webm => "WebM",
            Format::Gif => "GIF",
        }
    }
}

/// The frame of an MP4 or WebM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// The recording's own size (the default).
    Source,
    /// A height step: 1440, 1080, 720 or 480.
    Step(u32),
}

impl Size {
    pub fn to_json(self) -> Value {
        match self {
            Size::Source => json!("source"),
            Size::Step(h) => json!(h),
        }
    }

    pub fn from_json(v: &Value) -> Option<Size> {
        match v {
            Value::String(s) if s == "source" => Some(Size::Source),
            Value::Number(n) => n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Size::Step),
            _ => None,
        }
    }
}

/// One row of the Size list: a choice and its real pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeOption {
    pub size: Size,
    pub width: u32,
    pub height: u32,
}

/// What the recording offers (`options` of the estimate reply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub source: (u32, u32),
    /// Only the steps below the recording, with their real pixels.
    pub sizes: Vec<SizeOption>,
    pub gif_widths: Vec<u32>,
}

impl Options {
    pub fn from_json(v: &Value) -> Option<Options> {
        let px = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
        let source = (px(&v["source"]["width"])?, px(&v["source"]["height"])?);
        let sizes = v["sizes"]
            .as_array()?
            .iter()
            .filter_map(|o| {
                Some(SizeOption {
                    size: Size::from_json(&o["size"])?,
                    width: px(&o["width"])?,
                    height: px(&o["height"])?,
                })
            })
            .collect();
        let gif_widths = v["gif_widths"].as_array()?.iter().filter_map(px).collect();
        Some(Options {
            source,
            sizes,
            gif_widths,
        })
    }

    /// The Size list: Recording size first, then the steps below it.
    pub fn size_rows(&self) -> Vec<SizeOption> {
        let mut rows = vec![SizeOption {
            size: Size::Source,
            width: self.source.0,
            height: self.source.1,
        }];
        rows.extend(
            self.sizes
                .iter()
                .copied()
                .filter(|o| o.size != Size::Source),
        );
        rows
    }
}

/// The video codec of the file (`codec` of the estimate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Vp9,
}

impl Codec {
    fn parse(s: &str) -> Codec {
        if s == "vp9" {
            Codec::Vp9
        } else {
            Codec::H264
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Vp9 => "VP9",
        }
    }
}

/// What the size block shows (the four looks of board E2, plus the moment before the first reply
/// and a failure that is neither of the looks).
#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    /// No reply yet for this kind of choice.
    Pending,
    /// MP4 or WebM: the upper bound.
    Bound {
        max_bytes: u64,
        codec: Codec,
        width: u32,
        height: u32,
    },
    /// GIF: "Size known after export" (grey, not an error).
    Unknown,
    /// A GIF over 30 s (amber): the daemon's refusal. The message is worded when drawn.
    NotAllowed {
        code: Option<ErrorCode>,
        message: String,
    },
    /// Any other refusal or failure of the estimate.
    Failed {
        code: Option<ErrorCode>,
        message: String,
    },
}

/// A refused or failed call, as the popover keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub code: Option<ErrorCode>,
    pub message: String,
}

impl Refusal {
    /// The stable id of an export refusal (`gif_too_long`, `size_larger_than_source`, ...).
    pub fn reason(&self) -> Option<&'static str> {
        cm_core::message::export_refusal(&self.message).map(|p| p.id)
    }
}

/// The header's second line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeaderLine {
    /// "3:12 after cuts".
    AfterCuts(f64),
    /// "0:24 between In and Out" (a GIF with In and Out set).
    BetweenInOut(f64),
}

/// The GIF clip line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipLine {
    /// "In 0:42 → Out 1:06 · 24 s (30 s at most)".
    InOut { from: f64, to: f64, secs: f64 },
    /// "Whole recording · 3:12 (30 s at most)".
    Whole { secs: f64 },
}

/// The Export button's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonText {
    /// "Export…".
    Export,
    /// "Exporting 42%".
    Exporting(u32),
}

/// `progress` is the running export's fraction (0 to 1) when a file is exporting. The button
/// still opens the popover then, so another export can be queued.
pub fn button_text(progress: Option<f64>) -> ButtonText {
    match progress {
        Some(p) => ButtonText::Exporting((p.clamp(0.0, 1.0) * 100.0).floor() as u32),
        None => ButtonText::Export,
    }
}

/// `3:12`, `0:24`, `1:02:03`.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The open popover.
#[derive(Debug, Clone)]
pub struct Popover {
    pub capture_id: String,
    pub title: String,
    /// The captions choice of the page that opened it (`export_video`'s `captions`).
    pub captions: Option<bool>,
    /// In and Out in seconds on the recording, when the page knows them and they are set.
    pub range: Option<(f64, f64)>,
    pub format: Format,
    pub size: Size,
    pub gif_width: u32,
    pub keep_under: bool,
    /// The "Keep the file under" value; `None` while the field holds something that is not a
    /// whole number of MB.
    pub max_mb: Option<u32>,
    pub options: Option<Options>,
    /// Length after cuts and In/Out (`duration_s` of the last reply).
    pub duration_s: Option<f64>,
    pub look: Look,
    /// The codec an MP4 gets on this computer, once an MP4 estimate has said so.
    pub mp4_codec: Option<Codec>,
    /// The number of the latest estimate request; a reply with another number is ignored.
    seq: u64,
    /// `export_video` is on its way.
    pub busy: bool,
    /// Why `export_video` was refused.
    pub error: Option<Refusal>,
}

impl Popover {
    /// The popover as it opens: MP4, the recording's size, GIF width 800, 10 MB (off).
    pub fn new(
        capture_id: &str,
        title: &str,
        captions: Option<bool>,
        range: Option<(f64, f64)>,
    ) -> Popover {
        Popover {
            capture_id: capture_id.to_owned(),
            title: title.to_owned(),
            captions,
            range,
            format: Format::Mp4,
            size: Size::Source,
            gif_width: DEFAULT_GIF_WIDTH,
            keep_under: false,
            max_mb: Some(DEFAULT_MAX_MB),
            options: None,
            duration_s: None,
            look: Look::Pending,
            mp4_codec: None,
            seq: 0,
            busy: false,
            error: None,
        }
    }

    // ---- requests ---------------------------------------------------------------------------

    /// The fields that choose the file, shared by `estimate_export` and `export_video`.
    fn choice_fields(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("capture_id".into(), json!(self.capture_id));
        m.insert("format".into(), json!(self.format.as_str()));
        if self.format == Format::Gif {
            m.insert("width".into(), json!(self.gif_width));
            if let (true, Some(mb)) = (self.keep_under, self.max_mb) {
                m.insert("max_mb".into(), json!(mb));
            }
        } else {
            m.insert("size".into(), self.size.to_json());
        }
        m
    }

    /// `estimate_export`'s parameters for the current choice.
    pub fn estimate_params(&self) -> Value {
        Value::Object(self.choice_fields())
    }

    /// `export_video`'s parameters: the same choice, and the page's captions choice.
    pub fn export_params(&self) -> Value {
        let mut m = self.choice_fields();
        if let Some(burn) = self.captions {
            m.insert("captions".into(), json!(if burn { "burn" } else { "none" }));
        }
        Value::Object(m)
    }

    /// Starts an estimate request: returns its number and parameters. What the block shows stays
    /// until the reply comes, unless it is of another kind than the new choice (a bound under
    /// GIF, or "known after export" under MP4), which would mislead.
    pub fn begin_estimate(&mut self) -> (u64, Value) {
        self.seq += 1;
        let keep = matches!(
            (&self.look, self.format),
            (Look::Bound { .. }, Format::Mp4 | Format::Webm)
                | (Look::Unknown | Look::NotAllowed { .. }, Format::Gif)
        );
        if !keep {
            self.look = Look::Pending;
        }
        (self.seq, self.estimate_params())
    }

    /// Applies the reply of request `seq`. A reply that is not the latest request's is ignored
    /// (`false`).
    pub fn apply_estimate(&mut self, seq: u64, reply: Result<Value, Refusal>) -> bool {
        if seq != self.seq {
            return false;
        }
        self.look = match reply {
            Ok(v) => self.read_reply(&v),
            Err(r) if r.reason() == Some("gif_too_long") => Look::NotAllowed {
                code: r.code,
                message: r.message,
            },
            Err(r) => Look::Failed {
                code: r.code,
                message: r.message,
            },
        };
        true
    }

    fn read_reply(&mut self, v: &Value) -> Look {
        if let Some(o) = Options::from_json(&v["options"]) {
            self.options = Some(o);
        }
        if let Some(d) = v["duration_s"].as_f64() {
            self.duration_s = Some(d);
        }
        if v["known_after_export"].as_bool() == Some(true) {
            return Look::Unknown;
        }
        let px = |k: &str| v[k].as_u64().and_then(|n| u32::try_from(n).ok());
        match (v["max_bytes"].as_u64(), px("width"), px("height")) {
            (Some(max_bytes), Some(width), Some(height)) => {
                let codec = Codec::parse(v["codec"].as_str().unwrap_or("h264"));
                if v["format"].as_str() == Some("mp4") {
                    self.mp4_codec = Some(codec);
                }
                Look::Bound {
                    max_bytes,
                    codec,
                    width,
                    height,
                }
            }
            _ => Look::Failed {
                code: None,
                message: "The estimate came back without a size.".to_owned(),
            },
        }
    }

    // ---- choices ----------------------------------------------------------------------------

    /// Chooses a format. A GIF keeps a width the recording offers; an MP4 or WebM keeps a size it
    /// offers. `true` when the choice changed (an estimate is needed).
    pub fn set_format(&mut self, f: Format) -> bool {
        if self.format == f {
            return false;
        }
        self.format = f;
        self.fit_choices();
        true
    }

    pub fn set_size(&mut self, s: Size) -> bool {
        if self.size == s || !self.offers_size(s) {
            return false;
        }
        self.size = s;
        true
    }

    pub fn set_gif_width(&mut self, w: u32) -> bool {
        if self.gif_width == w || !self.offers_width(w) {
            return false;
        }
        self.gif_width = w;
        true
    }

    pub fn set_keep_under(&mut self, on: bool) {
        self.keep_under = on;
    }

    /// The text of the MB field: a whole number from 1 to [`MAX_MB_LIMIT`], else none.
    pub fn set_max_mb_text(&mut self, text: &str) {
        self.max_mb = text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=MAX_MB_LIMIT).contains(n));
    }

    fn offers_size(&self, s: Size) -> bool {
        match (&self.options, s) {
            (_, Size::Source) => true,
            (Some(o), s) => o.sizes.iter().any(|x| x.size == s),
            (None, _) => false,
        }
    }

    fn offers_width(&self, w: u32) -> bool {
        self.options
            .as_ref()
            .is_none_or(|o| o.gif_widths.contains(&w))
    }

    /// After the options arrive or the format changes: a choice the recording does not offer goes
    /// back to the default (or the nearest the recording has).
    fn fit_choices(&mut self) {
        if !self.offers_size(self.size) {
            self.size = Size::Source;
        }
        if !self.offers_width(self.gif_width) {
            if let Some(o) = &self.options {
                self.gif_width = o
                    .gif_widths
                    .iter()
                    .copied()
                    .filter(|w| *w <= self.gif_width)
                    .max()
                    .or_else(|| o.gif_widths.iter().copied().min())
                    .unwrap_or(DEFAULT_GIF_WIDTH);
            }
        }
    }

    // ---- readouts ---------------------------------------------------------------------------

    /// The Size list (MP4, WebM). Empty until the first reply has the recording's size.
    pub fn size_rows(&self) -> Vec<SizeOption> {
        self.options
            .as_ref()
            .map(Options::size_rows)
            .unwrap_or_default()
    }

    /// The GIF width choices.
    pub fn gif_widths(&self) -> Vec<u32> {
        self.options
            .as_ref()
            .map(|o| o.gif_widths.clone())
            .unwrap_or_default()
    }

    /// The header's second line (after the title). `None` until the first reply.
    pub fn header_line(&self) -> Option<HeaderLine> {
        let d = self.duration_s?;
        Some(if self.format == Format::Gif && self.range.is_some() {
            HeaderLine::BetweenInOut(d)
        } else {
            HeaderLine::AfterCuts(d)
        })
    }

    /// The GIF clip line.
    pub fn clip_line(&self) -> Option<ClipLine> {
        let secs = self.duration_s?;
        Some(match self.range {
            Some((from, to)) => ClipLine::InOut { from, to, secs },
            None => ClipLine::Whole { secs },
        })
    }

    /// The blue note under Format: MP4 with VP9 video (Linux without a hardware H.264 encoder).
    pub fn vp9_note(&self) -> bool {
        self.format == Format::Mp4 && self.mp4_codec == Some(Codec::Vp9)
    }

    /// Whether the footer button works: not while a request is on its way, not for a GIF over
    /// 30 s, and not with "keep under" on and no number in the field.
    pub fn can_export(&self) -> bool {
        !self.busy
            && !matches!(self.look, Look::NotAllowed { .. })
            && !(self.format == Format::Gif && self.keep_under && self.max_mb.is_none())
    }

    /// The text of the "Size known after export" block when "keep under" is on.
    pub fn keep_under_mb(&self) -> Option<u32> {
        (self.format == Format::Gif && self.keep_under)
            .then_some(self.max_mb)
            .flatten()
    }
}

/// What a finished export tells the done toast (`export.done` of an `export_video` job): the real
/// size, and the estimate that was shown before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Finished {
    pub size_bytes: Option<u64>,
    pub estimate_max_bytes: Option<u64>,
}

impl Finished {
    pub fn of(data: &Value) -> Finished {
        Finished {
            size_bytes: data["size_bytes"].as_u64(),
            estimate_max_bytes: data["estimate"]["max_bytes"].as_u64(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Value {
        json!({
            "source": {"width": 2880, "height": 1800},
            "sizes": [
                {"size": 1440, "width": 2304, "height": 1440},
                {"size": 1080, "width": 1728, "height": 1080},
                {"size": 720, "width": 1152, "height": 720},
                {"size": 480, "width": 768, "height": 480},
            ],
            "gif_widths": [480, 640, 800, 1024],
        })
    }

    fn bound(format: &str, codec: &str, max: u64) -> Value {
        json!({
            "max_bytes": max, "label": "Up to 290 MB", "codec": codec,
            "width": 2880, "height": 1800, "fps": 30.0, "duration_s": 192.0,
            "format": format, "options": options(),
        })
    }

    fn refusal(message: &str) -> Refusal {
        Refusal {
            code: Some(ErrorCode::InvalidInput),
            message: message.to_owned(),
        }
    }

    fn popover() -> Popover {
        Popover::new("rec_1", "Walkthrough", None, None)
    }

    /// EXP-5: the popover opens on MP4 at the recording's size, GIF at 800, "keep under" off.
    #[test]
    fn it_opens_on_the_defaults() {
        let p = popover();
        assert_eq!(
            (p.format, p.size, p.gif_width),
            (Format::Mp4, Size::Source, 800)
        );
        assert!(!p.keep_under);
        assert_eq!(p.max_mb, Some(10));
        assert_eq!(p.look, Look::Pending);
        assert_eq!(
            p.estimate_params(),
            json!({"capture_id": "rec_1", "format": "mp4", "size": "source"})
        );
        assert!(p.can_export());
    }

    #[test]
    fn the_size_list_comes_from_the_options() {
        let mut p = popover();
        assert!(p.size_rows().is_empty());
        let (seq, _) = p.begin_estimate();
        assert!(p.apply_estimate(seq, Ok(bound("mp4", "h264", 290_000_000))));
        let rows = p.size_rows();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].size, Size::Source);
        assert_eq!((rows[0].width, rows[0].height), (2880, 1800));
        assert_eq!(rows[3].size, Size::Step(720));
        assert_eq!((rows[3].width, rows[3].height), (1152, 720));
        assert!(matches!(
            p.look,
            Look::Bound {
                max_bytes: 290_000_000,
                ..
            }
        ));
        assert_eq!(p.duration_s, Some(192.0));
    }

    /// A recording smaller than a step does not list it, and a step is only chosen when offered.
    #[test]
    fn a_step_the_recording_does_not_offer_is_not_chosen() {
        let mut p = popover();
        let (seq, _) = p.begin_estimate();
        let mut v = bound("mp4", "h264", 1_000_000);
        v["options"]["sizes"] = json!([{"size": 480, "width": 768, "height": 480}]);
        p.apply_estimate(seq, Ok(v));
        assert_eq!(p.size_rows().len(), 2);
        assert!(!p.set_size(Size::Step(1080)));
        assert!(p.set_size(Size::Step(480)));
        assert_eq!(
            p.estimate_params(),
            json!({"capture_id": "rec_1", "format": "mp4", "size": 480})
        );
    }

    #[test]
    fn switching_format_changes_the_fields_sent() {
        let mut p = popover();
        let (seq, _) = p.begin_estimate();
        p.apply_estimate(seq, Ok(bound("mp4", "h264", 5_000_000)));
        assert!(p.set_size(Size::Step(720)));
        assert!(p.set_format(Format::Webm));
        assert_eq!(
            p.estimate_params(),
            json!({"capture_id": "rec_1", "format": "webm", "size": 720})
        );
        assert!(p.set_format(Format::Gif));
        assert_eq!(
            p.estimate_params(),
            json!({"capture_id": "rec_1", "format": "gif", "width": 800})
        );
        assert!(!p.set_format(Format::Gif));
        assert!(p.set_gif_width(480));
        assert!(!p.set_gif_width(500));
        p.set_keep_under(true);
        p.set_max_mb_text("25");
        assert_eq!(
            p.export_params(),
            json!({"capture_id": "rec_1", "format": "gif", "width": 480, "max_mb": 25})
        );
    }

    /// EXP-5: the MB field takes whole numbers; with "keep under" on and none, Export waits.
    #[test]
    fn the_mb_field_is_a_whole_number() {
        let mut p = popover();
        p.set_format(Format::Gif);
        p.set_keep_under(true);
        for bad in ["", "abc", "0", "1.5", "-3", "999999"] {
            p.set_max_mb_text(bad);
            assert_eq!(p.max_mb, None, "{bad}");
            assert!(!p.can_export(), "{bad}");
            assert!(p.keep_under_mb().is_none());
        }
        p.set_max_mb_text(" 12 ");
        assert_eq!(p.max_mb, Some(12));
        assert!(p.can_export());
        assert_eq!(p.keep_under_mb(), Some(12));
        p.set_keep_under(false);
        p.set_max_mb_text("");
        assert!(p.can_export());
        assert!(p.export_params().get("max_mb").is_none());
    }

    /// EXP-5: a GIF has no estimate: "Size known after export".
    #[test]
    fn a_gif_has_no_bound() {
        let mut p = popover();
        let (s1, _) = p.begin_estimate();
        p.apply_estimate(s1, Ok(bound("mp4", "h264", 5_000_000)));
        p.set_format(Format::Gif);
        let (s2, _) = p.begin_estimate();
        // The MP4 bound is not shown under GIF while the reply is on its way.
        assert_eq!(p.look, Look::Pending);
        let v = json!({"known_after_export": true, "duration_s": 24.0, "format": "gif",
                       "options": options()});
        assert!(p.apply_estimate(s2, Ok(v)));
        assert_eq!(p.look, Look::Unknown);
        assert_eq!(p.duration_s, Some(24.0));
        assert!(p.can_export());
    }

    /// EXP-5: a GIF over 30 s is refused with the design's text: not allowed, Export disabled,
    /// and the choices stay so the person can pick MP4 again.
    #[test]
    fn a_long_gif_is_not_allowed() {
        let mut p = popover();
        p.set_format(Format::Gif);
        let (seq, _) = p.begin_estimate();
        let r = refusal("GIF is for clips of 30 s or less. This one is 3:12.");
        assert_eq!(r.reason(), Some("gif_too_long"));
        assert!(p.apply_estimate(seq, Err(r)));
        assert!(matches!(p.look, Look::NotAllowed { .. }));
        assert!(!p.can_export());
        // Back to MP4: the amber block does not stay.
        p.set_format(Format::Mp4);
        let (seq, _) = p.begin_estimate();
        assert_eq!(p.look, Look::Pending);
        p.apply_estimate(seq, Ok(bound("mp4", "h264", 290_000_000)));
        assert!(p.can_export());
    }

    #[test]
    fn another_refusal_is_a_failure_not_the_gif_look() {
        let mut p = popover();
        let (seq, _) = p.begin_estimate();
        let r = refusal("720p is larger than the recording (640 \u{d7} 360). Choose the recording size or a smaller one.");
        assert_eq!(r.reason(), Some("size_larger_than_source"));
        p.apply_estimate(seq, Err(r));
        assert!(matches!(p.look, Look::Failed { .. }));
        assert!(p.can_export());
    }

    /// A reply for an earlier choice never overwrites the newer one.
    #[test]
    fn a_stale_reply_is_ignored() {
        let mut p = popover();
        let (s1, _) = p.begin_estimate();
        p.set_format(Format::Webm);
        let (s2, _) = p.begin_estimate();
        assert!(p.apply_estimate(s2, Ok(bound("webm", "vp9", 35_000_000))));
        assert!(!p.apply_estimate(s1, Ok(bound("mp4", "h264", 290_000_000))));
        assert!(matches!(
            p.look,
            Look::Bound {
                max_bytes: 35_000_000,
                ..
            }
        ));
        // The late MP4 reply did not set the MP4 codec either.
        assert_eq!(p.mp4_codec, None);
        // A late refusal is ignored too.
        let (s3, _) = p.begin_estimate();
        assert!(!p.apply_estimate(s2, Err(refusal("x"))));
        assert!(p.apply_estimate(s3, Ok(bound("webm", "vp9", 36_000_000))));
    }

    /// While the next reply is on its way the old value stays (same kind of choice).
    #[test]
    fn the_old_value_stays_until_the_new_one_arrives() {
        let mut p = popover();
        let (s1, _) = p.begin_estimate();
        p.apply_estimate(s1, Ok(bound("mp4", "h264", 290_000_000)));
        p.set_size(Size::Step(720));
        let (_s2, _) = p.begin_estimate();
        assert!(matches!(
            p.look,
            Look::Bound {
                max_bytes: 290_000_000,
                ..
            }
        ));
    }

    /// EXP-5: Linux without a hardware H.264 encoder: the MP4 estimate says VP9 and the blue note
    /// shows under Format for MP4 only.
    #[test]
    fn the_vp9_note_shows_for_mp4_with_vp9() {
        let mut p = popover();
        assert!(!p.vp9_note());
        let (seq, _) = p.begin_estimate();
        p.apply_estimate(seq, Ok(bound("mp4", "vp9", 200_000_000)));
        assert!(p.vp9_note());
        p.set_format(Format::Webm);
        assert!(!p.vp9_note());
        p.set_format(Format::Mp4);
        assert!(p.vp9_note());
        let mut q = popover();
        let (seq, _) = q.begin_estimate();
        q.apply_estimate(seq, Ok(bound("mp4", "h264", 200_000_000)));
        assert!(!q.vp9_note());
    }

    #[test]
    fn the_header_and_clip_lines() {
        let mut p = Popover::new("rec_1", "T", None, Some((42.0, 66.0)));
        assert_eq!(p.header_line(), None);
        let (seq, _) = p.begin_estimate();
        let mut v = bound("mp4", "h264", 1_000_000);
        v["duration_s"] = json!(24.0);
        p.apply_estimate(seq, Ok(v));
        assert_eq!(p.header_line(), Some(HeaderLine::AfterCuts(24.0)));
        p.set_format(Format::Gif);
        assert_eq!(p.header_line(), Some(HeaderLine::BetweenInOut(24.0)));
        assert_eq!(
            p.clip_line(),
            Some(ClipLine::InOut {
                from: 42.0,
                to: 66.0,
                secs: 24.0
            })
        );
        let mut q = popover();
        q.duration_s = Some(192.0);
        q.set_format(Format::Gif);
        assert_eq!(q.header_line(), Some(HeaderLine::AfterCuts(192.0)));
        assert_eq!(q.clip_line(), Some(ClipLine::Whole { secs: 192.0 }));
        assert_eq!(clock(192.0), "3:12");
        assert_eq!(clock(24.0), "0:24");
        assert_eq!(clock(3723.0), "1:02:03");
    }

    /// The request built for `export_video`: the choice, plus the page's captions choice; nothing
    /// about presets.
    #[test]
    fn the_export_request_carries_the_choice_and_the_captions() {
        let mut p = Popover::new("rec_9", "T", Some(true), None);
        p.set_format(Format::Webm);
        let (seq, _) = p.begin_estimate();
        p.apply_estimate(seq, Ok(bound("webm", "vp9", 35_000_000)));
        p.set_size(Size::Step(1080));
        assert_eq!(
            p.export_params(),
            json!({"capture_id": "rec_9", "format": "webm", "size": 1080, "captions": "burn"})
        );
        p.captions = Some(false);
        assert_eq!(p.export_params()["captions"], "none");
        p.captions = None;
        assert!(p.export_params().get("captions").is_none());
        assert!(p.export_params().get("preset").is_none());
    }

    /// Queueing: the button shows the progress of the running file and still opens the popover;
    /// the request does not depend on whether a file is exporting, and only `busy` (this
    /// popover's own call) blocks the footer button.
    #[test]
    fn a_second_export_can_be_queued_while_one_runs() {
        assert_eq!(button_text(None), ButtonText::Export);
        assert_eq!(button_text(Some(0.426)), ButtonText::Exporting(42));
        assert_eq!(button_text(Some(1.5)), ButtonText::Exporting(100));
        let mut p = popover();
        let before = p.export_params();
        assert!(p.can_export());
        p.busy = true;
        assert!(!p.can_export());
        p.busy = false;
        assert_eq!(p.export_params(), before);
    }

    /// A format switch fits the width to what a small recording offers.
    #[test]
    fn a_gif_width_the_recording_does_not_offer_steps_down() {
        let mut p = popover();
        let (seq, _) = p.begin_estimate();
        let mut v = bound("mp4", "h264", 1_000_000);
        v["options"]["gif_widths"] = json!([480, 640]);
        p.apply_estimate(seq, Ok(v));
        p.set_format(Format::Gif);
        assert_eq!(p.gif_width, 640);
        assert_eq!(p.gif_widths(), vec![480, 640]);
    }

    #[test]
    fn the_finished_export_reports_the_real_size_and_the_estimate() {
        let f = Finished::of(
            &json!({"size_bytes": 243_000_000u64, "estimate": {"max_bytes": 290_000_000u64}}),
        );
        assert_eq!(f.size_bytes, Some(243_000_000));
        assert_eq!(f.estimate_max_bytes, Some(290_000_000));
        let g = Finished::of(&json!({"size_bytes": 5_000_000u64, "estimate": null}));
        assert_eq!(g.estimate_max_bytes, None);
    }
}
