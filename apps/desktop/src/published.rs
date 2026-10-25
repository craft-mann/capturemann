//! The Published page and the share links of uploads, as display-free logic (PUB-2, STO-4):
//! parsing the `publishes` and `uploads` records of `list_library` items, one row per publish
//! newest first, plain-words labels, empty-state text, and which upload link "Copy link" copies.
//! The window code (`ui::published_view`) only draws what is decided here.

use cm_i18n::{tr, tr_ctx};
use serde_json::Value;
use time::OffsetDateTime;

use crate::library::{CaptureItem, Kind};

/// What the platform says about a published video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishState {
    Processing,
    Processed,
    /// The platform kept the video private although another privacy was asked for.
    PrivacyLocked,
    /// A status this version does not know (a newer daemon).
    Unknown,
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
                Some("processing") => PublishState::Processing,
                Some("processed") => PublishState::Processed,
                Some("privacy_locked") => PublishState::PrivacyLocked,
                _ => PublishState::Unknown,
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

// ---- the Published page ------------------------------------------------------------------------

/// One published video: a row of the Published page.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedRow {
    pub capture_id: String,
    /// The recording's title (the video's title on the platform can differ).
    pub capture_title: String,
    pub kind: Kind,
    pub record: PublishRecord,
}

/// Every publish of every capture, newest first (records without a time last). Ties keep a
/// stable order by capture id and video id.
pub fn rows(items: &[CaptureItem]) -> Vec<PublishedRow> {
    let mut out: Vec<PublishedRow> = items
        .iter()
        .flat_map(|i| {
            i.publishes.iter().map(|r| PublishedRow {
                capture_id: i.id.clone(),
                capture_title: i.title.clone(),
                kind: i.kind,
                record: r.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.record
            .at
            .cmp(&a.record.at)
            .then_with(|| a.capture_id.cmp(&b.capture_id))
            .then_with(|| a.record.remote_id.cmp(&b.record.remote_id))
    });
    out
}

/// The sidebar number: how many videos are published.
pub fn count(items: &[CaptureItem]) -> usize {
    items.iter().map(|i| i.publishes.len()).sum()
}

/// The empty page: a title and how to publish.
pub fn empty_text() -> (String, String) {
    (
        tr!("Nothing published yet"),
        tr!(
            "Open a recording and press Publish to send it to YouTube. Your published videos are listed here with their links."
        ),
    )
}

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
        PublishState::Processing => {
            tr!("{platform} is still processing the video", platform = p)
        }
        PublishState::Processed => tr!("Processed"),
        PublishState::PrivacyLocked => tr!(
            "{platform} kept it {privacy}",
            platform = p,
            privacy = privacy_in_sentence(&r.privacy)
        ),
        PublishState::Unknown => tr!("Published"),
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

/// The icon and tone of the status line: a spinner while the platform processes, a check when
/// it is done, a triangle when the platform changed what the user asked for.
pub fn status_icon(state: PublishState) -> (&'static str, crate::ui::kit::Tone) {
    use crate::ui::kit::Tone;
    match state {
        PublishState::Processing => ("loader-circle", Tone::Neutral),
        PublishState::Processed | PublishState::Unknown => ("circle-check", Tone::Success),
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

/// Screen-reader label of a row: everything it shows in one sentence.
pub fn accessible_label(row: &PublishedRow, when: &str) -> String {
    if when.is_empty() {
        tr!(
            "{title}, {platform}, {privacy}, {status}",
            title = row.capture_title,
            platform = platform_account(&row.record),
            privacy = privacy_line(&row.record),
            status = status_text(&row.record)
        )
    } else {
        tr!(
            "{title}, {platform}, {privacy}, {status}, {when}",
            title = row.capture_title,
            platform = platform_account(&row.record),
            privacy = privacy_line(&row.record),
            status = status_text(&row.record),
            when = when
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
    fn rows_are_newest_first_across_captures() {
        let items = vec![
            item(
                "a",
                "Lesson 1",
                vec![pub_rec(
                    "v1",
                    "2026-10-21T10:00:00Z",
                    "processed",
                    "public",
                    "public",
                )],
                vec![],
            ),
            item(
                "b",
                "Lesson 2",
                vec![
                    pub_rec(
                        "v2",
                        "2026-10-23T10:00:00Z",
                        "processing",
                        "private",
                        "private",
                    ),
                    pub_rec(
                        "v3",
                        "2026-10-22T10:00:00Z",
                        "processed",
                        "public",
                        "public",
                    ),
                ],
                vec![],
            ),
            item("c", "Never published", vec![], vec![]),
        ];
        let r = rows(&items);
        let ids: Vec<&str> = r.iter().map(|r| r.record.remote_id.as_str()).collect();
        assert_eq!(ids, ["v2", "v3", "v1"]);
        assert_eq!(r[0].capture_id, "b");
        assert_eq!(r[0].capture_title, "Lesson 2");
        assert_eq!(count(&items), 3);
    }

    #[test]
    fn an_empty_library_has_no_rows_and_a_zero_count() {
        assert!(rows(&[]).is_empty());
        assert_eq!(count(&[]), 0);
        let (title, body) = empty_text();
        assert_eq!(title, "Nothing published yet");
        assert!(body.contains("Publish"));
        // Only the platforms the app offers are named.
        for other in ["TikTok", "Facebook"] {
            assert!(!body.contains(other));
        }
    }

    #[test]
    fn a_record_without_a_time_sorts_last_and_ties_are_stable() {
        let items = vec![item(
            "a",
            "A",
            vec![
                json!({ "platform": "youtube", "remote_id": "old", "url": "https://youtu.be/old" }),
                pub_rec(
                    "same2",
                    "2026-10-21T10:00:00Z",
                    "processed",
                    "public",
                    "public",
                ),
                pub_rec(
                    "same1",
                    "2026-10-21T10:00:00Z",
                    "processed",
                    "public",
                    "public",
                ),
            ],
            vec![],
        )];
        let ids: Vec<String> = rows(&items)
            .into_iter()
            .map(|r| r.record.remote_id)
            .collect();
        assert_eq!(ids, ["same1", "same2", "old"]);
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

    #[test]
    fn status_and_privacy_are_in_plain_words() {
        let rec = |s, p, asked| {
            PublishRecord::from_json(&pub_rec("v", "2026-10-21T10:00:00Z", s, p, asked)).unwrap()
        };
        let processing = rec("processing", "unlisted", "unlisted");
        assert_eq!(
            status_text(&processing),
            "YouTube is still processing the video"
        );
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
        assert_eq!(status_icon(PublishState::Processing).0, "loader-circle");
        assert_eq!(
            status_icon(PublishState::PrivacyLocked),
            ("triangle-alert", Tone::Brass)
        );
        assert_eq!(status_icon(PublishState::Processed).0, "circle-check");
    }

    #[test]
    fn the_accessible_label_has_everything_on_the_row() {
        let i = item(
            "a",
            "Lesson",
            vec![pub_rec(
                "v",
                "2026-10-21T10:00:00Z",
                "processed",
                "public",
                "public",
            )],
            vec![],
        );
        let l = accessible_label(&rows(&[i])[0], "Oct 21, 2026");
        assert_eq!(
            l,
            "Lesson, YouTube \u{00B7} My channel, Public, Processed, Oct 21, 2026"
        );
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
        let (title, _) = with_language(Lang::EnXa, empty_text);
        assert_ne!(title, "Nothing published yet");
    }
}
