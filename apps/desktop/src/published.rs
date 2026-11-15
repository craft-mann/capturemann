//! Publish records and the share links of uploads, as display-free logic (PUB-2, STO-4): parsing
//! the `publishes` and `uploads` records of `list_library` items, plain-words labels, the status
//! line of a card on the Recordings page with the Published filter on (S22), the Published card
//! of the details page, and which upload link "Copy link" copies. The Published page is gone
//! (0.3.4): its list is that filter. The window code only draws what is decided here.

use cm_i18n::{tr, tr_ctx};
use serde_json::Value;
use time::OffsetDateTime;

/// What the platform says about a published video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishState {
    /// Uploaded: also a status this version does not know.
    Published,
    /// Recorded as `processing` when the video was sent. The app never asks the platform again,
    /// so [`status_line`] says "is processing" only while the record is young.
    Processing,
    Processed,
    /// The platform kept the video private although another privacy was asked for.
    PrivacyLocked,
}

/// One entry of a capture's `publishes[]`.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishRecord {
    /// `youtube`.
    pub platform: String,
    /// The label the user gave the account (never a key).
    pub account: String,
    pub remote_id: String,
    pub url: String,
    /// The privacy the platform reports (`private`, `unlisted`, `public`).
    pub privacy: String,
    pub privacy_requested: Option<String>,
    pub state: PublishState,
    pub warnings: Vec<String>,
    pub at: Option<OffsetDateTime>,
}

impl PublishRecord {
    /// `None` when the record has no link (it could not be opened or copied anyway).
    pub fn from_json(v: &Value) -> Option<PublishRecord> {
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        let url = text("url").filter(|u| !u.is_empty())?;
        Some(PublishRecord {
            platform: text("platform").unwrap_or_default(),
            account: text("account").unwrap_or_default(),
            remote_id: text("remote_id").unwrap_or_default(),
            url,
            privacy: text("privacy").unwrap_or_default(),
            privacy_requested: text("privacy_requested"),
            state: match v.get("status").and_then(Value::as_str) {
                Some("processed") => PublishState::Processed,
                Some("processing") => PublishState::Processing,
                Some("privacy_locked") => PublishState::PrivacyLocked,
                _ => PublishState::Published,
            },
            warnings: v
                .get("warnings")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            at: parse_time(v.get("at")),
        })
    }
}

/// One entry of a capture's `uploads[]`.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadRecord {
    pub destination_id: String,
    pub destination: String,
    /// `export` or `original`.
    pub what: String,
    pub bytes: u64,
    pub share_link: Option<String>,
    pub at: Option<OffsetDateTime>,
}

impl UploadRecord {
    pub fn from_json(v: &Value) -> Option<UploadRecord> {
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        Some(UploadRecord {
            destination_id: text("destination_id").unwrap_or_default(),
            destination: text("destination")?,
            what: text("what").unwrap_or_else(|| "export".to_owned()),
            bytes: v.get("bytes").and_then(Value::as_u64).unwrap_or(0),
            share_link: text("share_link").filter(|l| !l.is_empty()),
            at: parse_time(v.get("at")),
        })
    }

    /// "The video" or "The original recording": what this copy holds.
    pub fn what_label(&self) -> String {
        if self.what == "original" {
            tr!("Original recording")
        } else {
            tr_ctx!("upload content", "Video")
        }
    }
}

fn parse_time(v: Option<&Value>) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(v?.as_str()?, &time::format_description::well_known::Rfc3339).ok()
}

pub fn parse_publishes(v: Option<&Value>) -> Vec<PublishRecord> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(PublishRecord::from_json).collect())
        .unwrap_or_default()
}

pub fn parse_uploads(v: Option<&Value>) -> Vec<UploadRecord> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(UploadRecord::from_json).collect())
        .unwrap_or_default()
}

/// The upload whose link "Copy link" copies (STO-4.AC3): the newest record with a link, a copy
/// of the video before a copy of the original recording. `None` when no upload has a link.
pub fn link_upload(uploads: &[UploadRecord]) -> Option<&UploadRecord> {
    let newest = |what_export: bool| {
        uploads
            .iter()
            .enumerate()
            .filter(|(_, u)| u.share_link.is_some() && (u.what != "original") == what_export)
            // Records are oldest first; a missing time sorts as oldest.
            .max_by_key(|(i, u)| (u.at, *i))
            .map(|(_, u)| u)
    };
    newest(true).or_else(|| newest(false))
}

/// The link itself, see [`link_upload`].
pub fn share_link_of(uploads: &[UploadRecord]) -> Option<&str> {
    link_upload(uploads).and_then(|u| u.share_link.as_deref())
}

/// The details page line of a finished upload: "Google Drive \u{b7} Video \u{b7} 12.3 MB \u{b7} Oct 21, 2026"
/// (the size and the time only when known). `when` is the formatted time, empty if unknown.
pub fn upload_line(u: &UploadRecord, when: &str) -> String {
    let mut parts = vec![u.destination.clone(), u.what_label()];
    if u.bytes > 0 {
        parts.push(cm_i18n::fmt::size(u.bytes));
    }
    if !when.is_empty() {
        parts.push(when.to_owned());
    }
    parts.join(" \u{00B7} ")
}

/// How long after sending a `processing` record still reads "is processing".
pub const PROCESSING_WINDOW: time::Duration = time::Duration::hours(24);

/// "YouTube" for `youtube`; an unknown platform shows its name with a capital.
pub fn platform_label(platform: &str) -> String {
    match platform {
        "youtube" => "YouTube".to_owned(),
        "" => tr!("Unknown platform"),
        other => {
            let mut c = other.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase().chain(c).collect::<String>()
            })
        }
    }
}

/// `private` as "Private"; what the platform reports.
pub fn privacy_label(privacy: &str) -> String {
    match privacy {
        "private" => tr_ctx!("visibility", "Private"),
        "unlisted" => tr_ctx!("visibility", "Unlisted"),
        "public" => tr_ctx!("visibility", "Public"),
        "" => tr_ctx!("visibility", "Unknown"),
        other => other.to_owned(),
    }
}

/// [`privacy_label`] as it reads inside a sentence.
fn privacy_in_sentence(privacy: &str) -> String {
    match privacy {
        "private" => tr_ctx!("visibility, in a sentence", "private"),
        "unlisted" => tr_ctx!("visibility, in a sentence", "unlisted"),
        "public" => tr_ctx!("visibility, in a sentence", "public"),
        "" => tr_ctx!("visibility, in a sentence", "unknown"),
        other => other.to_lowercase(),
    }
}

/// The privacy line: what the platform reports, and what was asked for when they differ.
pub fn privacy_line(r: &PublishRecord) -> String {
    let reported = privacy_label(&r.privacy);
    match r.privacy_requested.as_deref() {
        Some(asked) if !asked.is_empty() && asked != r.privacy => {
            tr!(
                "{reported} (you asked for {asked})",
                reported = reported,
                asked = privacy_label(asked)
            )
        }
        _ => reported,
    }
}

/// The status in plain words.
pub fn status_text(r: &PublishRecord) -> String {
    let p = platform_label(&r.platform);
    match r.state {
        PublishState::Processed => tr!("Processed"),
        PublishState::PrivacyLocked => tr!(
            "{platform} kept it {privacy}",
            platform = p,
            privacy = privacy_in_sentence(&r.privacy)
        ),
        PublishState::Published | PublishState::Processing => tr!("Published"),
    }
}

/// The icon of a privacy pill: `lock`, `link`, `globe`; a privacy this version does not know
/// gets the dashed circle.
pub fn privacy_icon(privacy: &str) -> &'static str {
    match privacy {
        "private" => "lock",
        "unlisted" => "link",
        "public" => "globe",
        _ => "circle-dashed",
    }
}

/// The pill tone: brass when the platform locked the privacy, neutral otherwise.
pub fn privacy_tone(r: &PublishRecord) -> crate::ui::kit::Tone {
    if r.state == PublishState::PrivacyLocked {
        crate::ui::kit::Tone::Brass
    } else {
        crate::ui::kit::Tone::Neutral
    }
}

/// The icon and tone of the status line: a check, or a triangle when the platform changed what
/// the user asked for.
pub fn status_icon(state: PublishState) -> (&'static str, crate::ui::kit::Tone) {
    use crate::ui::kit::Tone;
    match state {
        PublishState::Published | PublishState::Processing | PublishState::Processed => {
            ("circle-check", Tone::Success)
        }
        PublishState::PrivacyLocked => ("triangle-alert", Tone::Brass),
    }
}

/// "Open on YouTube": the aria label and tooltip of the icon button that opens the link.
pub fn open_label(r: &PublishRecord) -> String {
    tr!("Open on {platform}", platform = platform_label(&r.platform))
}

/// The platform and the account: "YouTube \u{b7} My channel".
pub fn platform_account(r: &PublishRecord) -> String {
    if r.account.is_empty() {
        platform_label(&r.platform)
    } else {
        tr!(
            "{platform} \u{00B7} {account}",
            platform = platform_label(&r.platform),
            account = r.account
        )
    }
}

/// The pill on a published card's thumbnail: "YouTube \u{b7} Unlisted" (S22).
pub fn platform_pill(r: &PublishRecord) -> String {
    tr!(
        "{platform} \u{00B7} {privacy}",
        platform = platform_label(&r.platform),
        privacy = privacy_label(&r.privacy)
    )
}

/// The meta line of a published card: "Published Today 15:41 \u{b7} 11.9 MB" (the time and the
/// size only when known). `when` is the formatted publish time, `size` the formatted size.
pub fn published_meta(when: &str, size: &str) -> String {
    let head = if when.is_empty() {
        tr!("Published")
    } else {
        tr!("Published {when}", when = when)
    };
    if size.is_empty() {
        head
    } else {
        format!("{head} \u{00B7} {size}")
    }
}

/// What the status line of a published card shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
    pub kind: StatusKind,
}

/// The look of a status line (S22): a warning is brass with a triangle, "processing" a ring,
/// "live" a green dot; all other text is the quiet grey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Warning,
    Processing,
    Live,
    /// Published, with nothing more known (an older `processing` record, an unknown status).
    Plain,
}

/// The one status line under a published card: the first warning of the platform (a refusal such
/// as "captions not added") if there is one, else [`state_line`].
pub fn status_line(r: &PublishRecord, now: OffsetDateTime) -> StatusLine {
    match r.warnings.first() {
        Some(w) if r.state != PublishState::PrivacyLocked => StatusLine {
            text: w.clone(),
            kind: StatusKind::Warning,
        },
        _ => state_line(r, now),
    }
}

/// What the platform says about the video, without its warnings: a locked privacy, "is
/// processing" while a `processing` record is young, "Live on" once processed, else plain
/// "Published on". `now` is passed in so it is testable.
pub fn state_line(r: &PublishRecord, now: OffsetDateTime) -> StatusLine {
    let p = platform_label(&r.platform);
    match r.state {
        PublishState::PrivacyLocked => StatusLine {
            text: status_text(r),
            kind: StatusKind::Warning,
        },
        PublishState::Processed => StatusLine {
            text: tr!("Live on {platform}", platform = p),
            kind: StatusKind::Live,
        },
        PublishState::Processing
            if r.at
                .is_some_and(|a| now >= a && now - a < PROCESSING_WINDOW) =>
        {
            StatusLine {
                text: tr!("{platform} is processing", platform = p),
                kind: StatusKind::Processing,
            }
        }
        _ => StatusLine {
            text: tr!("Published on {platform}", platform = p),
            kind: StatusKind::Plain,
        },
    }
}

/// The distinct platforms of a recording's publishes, in the order they were first used.
pub fn platforms(publishes: &[PublishRecord]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in publishes {
        let name = platform_label(&r.platform);
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// "1 \u{b7} YouTube": the Published tile of the details page (the number of platforms and
/// their names); `None` when the recording was never published.
pub fn platforms_summary(publishes: &[PublishRecord]) -> Option<String> {
    let names = platforms(publishes);
    (!names.is_empty()).then(|| format!("{} \u{00B7} {}", names.len(), names.join(", ")))
}

/// "youtube.com/watch?v=abc": a link without its scheme and "www.", as the Published card
/// shows it.
pub fn short_link(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    rest.strip_prefix("www.").unwrap_or(rest).to_owned()
}

/// The record a card is about: the newest publish (a record without a time sorts oldest; the
/// later one in the list wins a tie).
pub fn latest(publishes: &[PublishRecord]) -> Option<&PublishRecord> {
    publishes
        .iter()
        .enumerate()
        .max_by_key(|(i, r)| (r.at, *i))
        .map(|(_, r)| r)
}

/// Whether a capture's publishes need a look (the "Needs attention" chip, S22): a publish with a
/// warning or a locked privacy, or an upload of the capture waiting for a sign-in.
pub fn needs_attention(publishes: &[PublishRecord], upload_waiting: bool) -> bool {
    !publishes.is_empty()
        && (upload_waiting
            || publishes
                .iter()
                .any(|r| !r.warnings.is_empty() || r.state == PublishState::PrivacyLocked))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::library::CaptureItem;

    fn pub_rec(id: &str, at: &str, status: &str, privacy: &str, asked: &str) -> Value {
        json!({ "platform": "youtube", "account_id": "a1", "account": "My channel",
                "remote_id": id, "url": format!("https://youtu.be/{id}"), "title": "T",
                "privacy": privacy, "privacy_requested": asked, "status": status,
                "warnings": [], "at": at })
    }

    fn item(id: &str, title: &str, publishes: Vec<Value>, uploads: Vec<Value>) -> CaptureItem {
        CaptureItem::from_json(&json!({
            "id": id, "title": title, "kind": "recording", "created": "2026-10-20T09:30:00Z",
            "duration": 1, "width": 1, "height": 1, "size_bytes": 1, "broken": false,
            "publishes": publishes, "uploads": uploads,
        }))
        .unwrap()
    }

    fn upload(what: &str, link: Option<&str>, at: &str) -> Value {
        json!({ "destination_id": "d1", "destination": "Google Drive", "what": what,
                "remote_id": "x", "bytes": 5, "files": 1, "share_link": link, "at": at })
    }

    #[test]
    fn a_record_with_no_link_is_skipped() {
        let i = item(
            "a",
            "A",
            vec![json!({ "platform": "youtube", "remote_id": "x", "url": "" })],
            vec![],
        );
        assert!(i.publishes.is_empty());
    }

    fn rec(id: &str, at: &str, status: &str) -> PublishRecord {
        PublishRecord::from_json(&pub_rec(id, at, status, "unlisted", "unlisted")).unwrap()
    }

    fn at(s: &str) -> OffsetDateTime {
        OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).unwrap()
    }

    #[test]
    fn the_newest_publish_is_the_one_a_card_is_about() {
        let old = rec("old", "2026-10-21T10:00:00Z", "processed");
        let new = rec("new", "2026-10-23T10:00:00Z", "processed");
        let none = PublishRecord::from_json(
            &json!({ "platform": "youtube", "remote_id": "t", "url": "https://youtu.be/t" }),
        )
        .unwrap();
        assert_eq!(latest(&[]), None);
        assert_eq!(
            latest(&[old.clone(), new.clone(), none.clone()])
                .unwrap()
                .remote_id,
            "new"
        );
        // A record without a time is the oldest; the later of two equal ones wins.
        assert_eq!(
            latest(&[none.clone(), old.clone()]).unwrap().remote_id,
            "old"
        );
        let same = rec("same", "2026-10-21T10:00:00Z", "processed");
        assert_eq!(latest(&[old, same]).unwrap().remote_id, "same");
    }

    /// S22: the pill on the thumbnail, the meta line and the status line of a published card.
    #[test]
    fn a_published_card_says_platform_visibility_time_size_and_status() {
        let r = rec("v", "2026-10-23T15:41:00Z", "processed");
        assert_eq!(platform_pill(&r), "YouTube \u{00B7} Unlisted");
        assert_eq!(
            published_meta("Today 15:41", "11.9 MB"),
            "Published Today 15:41 \u{00B7} 11.9 MB"
        );
        assert_eq!(published_meta("", "11.9 MB"), "Published \u{00B7} 11.9 MB");
        assert_eq!(published_meta("Nov 12", ""), "Published Nov 12");
        let now = at("2026-10-23T16:00:00Z");
        assert_eq!(
            status_line(&r, now),
            StatusLine {
                text: "Live on YouTube".into(),
                kind: StatusKind::Live
            }
        );
        // Processing reads as processing while young, then as published (never refreshed).
        let p = rec("v", "2026-10-23T15:41:00Z", "processing");
        assert_eq!(status_line(&p, now).text, "YouTube is processing");
        assert_eq!(status_line(&p, now).kind, StatusKind::Processing);
        let later = at("2026-10-25T16:00:00Z");
        assert_eq!(status_line(&p, later).text, "Published on YouTube");
        assert_eq!(status_line(&p, later).kind, StatusKind::Plain);
        // A record dated in the future (clock skew) is not "processing" either.
        assert_eq!(
            status_line(&p, at("2026-10-23T10:00:00Z")).kind,
            StatusKind::Plain
        );
    }

    #[test]
    fn a_warning_or_a_locked_privacy_takes_the_status_line() {
        let now = at("2026-10-23T16:00:00Z");
        let mut v = pub_rec(
            "v",
            "2026-10-23T15:41:00Z",
            "processed",
            "unlisted",
            "unlisted",
        );
        v["warnings"] = json!(["Captions not added", "The thumbnail was refused."]);
        let w = PublishRecord::from_json(&v).unwrap();
        assert_eq!(
            status_line(&w, now),
            StatusLine {
                text: "Captions not added".into(),
                kind: StatusKind::Warning
            }
        );
        let locked = PublishRecord::from_json(&pub_rec(
            "v",
            "2026-10-23T15:41:00Z",
            "privacy_locked",
            "private",
            "public",
        ))
        .unwrap();
        assert_eq!(status_line(&locked, now).text, "YouTube kept it private");
        assert_eq!(status_line(&locked, now).kind, StatusKind::Warning);
        // The details card lists the warnings itself: its line is the state alone.
        assert_eq!(state_line(&w, now).text, "Live on YouTube");
    }

    #[test]
    fn the_published_tile_and_the_short_link() {
        let a = rec("a", "2026-10-21T10:00:00Z", "processed");
        let b = rec("b", "2026-10-23T10:00:00Z", "processed");
        assert_eq!(platforms_summary(&[]), None);
        assert_eq!(
            platforms_summary(std::slice::from_ref(&a)).as_deref(),
            Some("1 \u{00B7} YouTube")
        );
        // Two videos on one platform are one platform.
        assert_eq!(
            platforms_summary(&[a, b]).as_deref(),
            Some("1 \u{00B7} YouTube")
        );
        assert_eq!(
            short_link("https://www.youtube.com/watch?v=x"),
            "youtube.com/watch?v=x"
        );
        assert_eq!(short_link("https://youtu.be/x"), "youtu.be/x");
        assert_eq!(short_link("youtu.be/x"), "youtu.be/x");
    }

    /// S22: "Needs attention" is a publish with a warning (or a locked privacy), or an upload of
    /// the capture waiting for a sign-in; a capture that was never published never is.
    #[test]
    fn needs_attention_means_a_warning_a_locked_privacy_or_a_waiting_upload() {
        let ok = rec("a", "2026-10-23T15:41:00Z", "processed");
        let mut v = pub_rec("b", "2026-10-23T15:41:00Z", "processed", "public", "public");
        v["warnings"] = json!(["Captions not added"]);
        let warned = PublishRecord::from_json(&v).unwrap();
        let locked = PublishRecord::from_json(&pub_rec(
            "c",
            "2026-10-23T15:41:00Z",
            "privacy_locked",
            "private",
            "public",
        ))
        .unwrap();
        assert!(!needs_attention(&[], true), "never published");
        assert!(!needs_attention(std::slice::from_ref(&ok), false));
        assert!(needs_attention(std::slice::from_ref(&ok), true));
        assert!(needs_attention(&[ok.clone(), warned], false));
        assert!(needs_attention(&[locked], false));
    }

    #[test]
    fn status_and_privacy_are_in_plain_words() {
        let rec = |s, p, asked| {
            PublishRecord::from_json(&pub_rec("v", "2026-10-21T10:00:00Z", s, p, asked)).unwrap()
        };
        // A `processing` record is never refreshed, so it reads as published.
        let processing = rec("processing", "unlisted", "unlisted");
        assert_eq!(status_text(&processing), "Published");
        assert_eq!(privacy_line(&processing), "Unlisted");
        assert_eq!(
            status_text(&rec("processed", "public", "public")),
            "Processed"
        );
        let locked = rec("privacy_locked", "private", "public");
        assert_eq!(status_text(&locked), "YouTube kept it private");
        assert_eq!(privacy_line(&locked), "Private (you asked for Public)");
        // A status from a newer daemon is not shown as a failure.
        assert_eq!(status_text(&rec("future", "public", "public")), "Published");
    }

    #[test]
    fn warnings_and_account_labels_are_kept() {
        let mut v = pub_rec("v", "2026-10-21T10:00:00Z", "processed", "public", "public");
        v["warnings"] = json!(["The thumbnail was refused.", ""]);
        let r = PublishRecord::from_json(&v).unwrap();
        assert_eq!(r.warnings, ["The thumbnail was refused."]);
        assert_eq!(platform_account(&r), "YouTube \u{00B7} My channel");
        let anon = PublishRecord {
            account: String::new(),
            ..r
        };
        assert_eq!(platform_account(&anon), "YouTube");
        assert_eq!(platform_label("vimeo"), "Vimeo");
    }

    #[test]
    fn privacy_pill_icons_and_status_icons() {
        use crate::ui::kit::Tone;
        assert_eq!(privacy_icon("private"), "lock");
        assert_eq!(privacy_icon("unlisted"), "link");
        assert_eq!(privacy_icon("public"), "globe");
        assert_eq!(privacy_icon(""), "circle-dashed");
        let locked = PublishRecord::from_json(&pub_rec(
            "a",
            "2026-01-01T00:00:00Z",
            "privacy_locked",
            "private",
            "public",
        ))
        .unwrap();
        assert_eq!(privacy_tone(&locked), Tone::Brass);
        assert_eq!(open_label(&locked), "Open on YouTube");
        assert_eq!(status_icon(PublishState::Published).0, "circle-check");
        assert_eq!(
            status_icon(PublishState::PrivacyLocked),
            ("triangle-alert", Tone::Brass)
        );
        assert_eq!(status_icon(PublishState::Processed).0, "circle-check");
    }

    /// STO-4.AC3: the link "Copy link" copies is the newest link of the video, else of the
    /// original; none when no upload has one (a local folder, or link mode none).
    #[test]
    fn copy_link_picks_the_newest_video_link() {
        let up = |v: Vec<Value>| parse_uploads(Some(&Value::Array(v)));
        assert_eq!(share_link_of(&[]), None);
        let none = up(vec![upload("export", None, "2026-10-21T10:00:00Z")]);
        assert_eq!(share_link_of(&none), None);
        let two = up(vec![
            upload("export", Some("https://drive/old"), "2026-10-21T10:00:00Z"),
            upload("export", Some("https://drive/new"), "2026-10-22T10:00:00Z"),
            upload(
                "original",
                Some("https://drive/orig"),
                "2026-10-23T10:00:00Z",
            ),
        ]);
        assert_eq!(share_link_of(&two), Some("https://drive/new"));
        let only_original = up(vec![upload(
            "original",
            Some("https://drive/orig"),
            "2026-10-23T10:00:00Z",
        )]);
        assert_eq!(share_link_of(&only_original), Some("https://drive/orig"));
        assert_eq!(only_original[0].what_label(), "Original recording");
        assert_eq!(two[0].what_label(), "Video");
    }

    #[test]
    fn a_finished_upload_line_names_the_destination_and_what_was_sent() {
        let u = UploadRecord::from_json(&upload("export", None, "2026-10-21T10:00:00Z")).unwrap();
        assert_eq!(
            upload_line(&u, "Oct 21, 2026"),
            format!(
                "Google Drive \u{b7} Video \u{b7} {} \u{b7} Oct 21, 2026",
                crate::library::format_size(5)
            )
        );
        let o = UploadRecord {
            bytes: 0,
            what: "original".into(),
            ..u
        };
        assert_eq!(
            upload_line(&o, ""),
            "Google Drive \u{b7} Original recording"
        );
    }

    #[test]
    fn an_item_without_the_fields_still_parses() {
        let i = CaptureItem::from_json(&json!({
            "id": "a", "title": "T", "kind": "screenshot", "created": "2026-10-20T09:30:00Z",
        }))
        .unwrap();
        assert!(i.publishes.is_empty() && i.uploads.is_empty());
        assert_eq!(share_link_of(&i.uploads), None);
    }

    /// I18N P2: placeholders are filled in every language; the labels are made when asked for.
    #[test]
    fn labels_keep_their_values_in_every_language() {
        use cm_i18n::{with_language, Lang};
        let r = PublishRecord {
            platform: "youtube".into(),
            account: "My channel".into(),
            remote_id: "abc".into(),
            url: "https://youtu.be/abc".into(),
            privacy: "private".into(),
            privacy_requested: Some("public".into()),
            state: PublishState::PrivacyLocked,
            warnings: Vec::new(),
            at: None,
        };
        assert_eq!(privacy_line(&r), "Private (you asked for Public)");
        assert_eq!(status_text(&r), "YouTube kept it private");
        for lang in [Lang::Vi, Lang::EnXa] {
            with_language(lang, || {
                for s in [privacy_line(&r), status_text(&r), open_label(&r)] {
                    assert!(!s.contains('{'), "{lang:?}: {s}");
                }
                assert!(open_label(&r).contains("YouTube"));
            });
        }
        let now = OffsetDateTime::now_utc();
        let live = with_language(Lang::EnXa, || status_line(&r, now).text);
        assert_ne!(live, "YouTube kept it private");
    }
}
