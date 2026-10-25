//! What a publish carries (`PublishMeta`), what a platform allows (`PlatformRules`) and the
//! check of one against the other. The check runs before any network call and names the field
//! that is wrong (`invalid_input`, "field: reason").

use std::path::{Path, PathBuf};

use cm_core::CmError;
use serde::{Deserialize, Serialize};

/// Who can see the video. The wire names are the ones YouTube uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privacy {
    Private,
    Unlisted,
    Public,
}

impl Privacy {
    pub fn as_str(self) -> &'static str {
        match self {
            Privacy::Private => "private",
            Privacy::Unlisted => "unlisted",
            Privacy::Public => "public",
        }
    }

    pub fn parse(s: &str) -> Option<Privacy> {
        match s {
            "private" => Some(Privacy::Private),
            "unlisted" => Some(Privacy::Unlisted),
            "public" => Some(Privacy::Public),
            _ => None,
        }
    }
}

/// Limits and defaults of one platform (the table in `specs/03-integrations.md`, Publishing).
/// The daemon can serve this as `get_platform_rules`; values are data, not code paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformRules {
    pub platform: String,
    pub max_bytes: u64,
    /// `None` when the platform states no limit.
    pub max_duration_secs: Option<u64>,
    pub video_extensions: Vec<String>,
    pub title_required: bool,
    pub title_max_chars: usize,
    pub description_max_bytes: usize,
    /// Sum of the tag lengths (a tag with a space counts its two quotes), as YouTube counts.
    pub tags_max_total_chars: usize,
    pub thumbnail_max_bytes: u64,
    pub thumbnail_extensions: Vec<String>,
    pub privacies: Vec<Privacy>,
    /// What the sheet preselects. For YouTube this is Unlisted (PUB-2).
    pub default_privacy: Privacy,
    /// If an unverified API project is locked to one privacy by the platform, it is named here so
    /// the sheet can say so before the upload. The provider still asks for what the caller chose.
    pub unverified_project_privacy: Option<Privacy>,
    pub requires_made_for_kids: bool,
    pub default_category: String,
}

impl PlatformRules {
    /// YouTube Data API v3 `videos.insert` (checked Oct 19, 2026).
    pub fn youtube() -> Self {
        PlatformRules {
            platform: "youtube".to_owned(),
            // "256 GB" in the YouTube help; taken as GiB, the more generous reading.
            max_bytes: 256 * 1024 * 1024 * 1024,
            // 12 hours for a verified channel (15 minutes without phone verification).
            max_duration_secs: Some(12 * 3600),
            video_extensions: ["mp4", "m4v", "mov", "webm", "mkv", "avi"]
                .map(String::from)
                .to_vec(),
            title_required: true,
            title_max_chars: 100,
            description_max_bytes: 5000,
            tags_max_total_chars: 500,
            thumbnail_max_bytes: 2 * 1024 * 1024,
            thumbnail_extensions: ["jpg", "jpeg", "png"].map(String::from).to_vec(),
            privacies: vec![Privacy::Private, Privacy::Unlisted, Privacy::Public],
            default_privacy: Privacy::Unlisted,
            unverified_project_privacy: Some(Privacy::Private),
            requires_made_for_kids: true,
            default_category: "28".to_owned(),
        }
    }
}

/// The fields of one publish (PUB-2: title, description, tags, privacy, category, made for kids,
/// playlist, thumbnail, captions). Scheduling (`scheduled_at_unix`) is PUB-6 and already carried
/// because YouTube does it natively with `publishAt`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PublishMeta {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    /// `None` takes the platform default ([`PlatformRules::default_privacy`]).
    pub privacy: Option<Privacy>,
    /// YouTube category id as a string, `None` takes the default ("28", Science & Technology).
    pub category: Option<String>,
    /// Required by YouTube; `None` is refused (PUB-2.AC4).
    pub made_for_kids: Option<bool>,
    pub thumbnail: Option<PathBuf>,
    /// An SRT file (AI-1) uploaded as a caption track.
    pub captions: Option<PathBuf>,
    /// BCP-47 language of the caption track; `None` is "en".
    pub captions_language: Option<String>,
    pub playlist_id: Option<String>,
    /// Unix seconds. YouTube publishes at that time; the video is uploaded as private until then.
    pub scheduled_at_unix: Option<i64>,
    /// Duration of the export, if known, for the duration limit.
    pub duration_secs: Option<f64>,
}

impl PublishMeta {
    /// The privacy that will be requested.
    pub fn effective_privacy(&self, rules: &PlatformRules) -> Privacy {
        self.privacy.unwrap_or(rules.default_privacy)
    }
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

fn file_size(field: &str, path: &Path) -> Result<u64, CmError> {
    let m = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            CmError::invalid_field(field, format!("{} does not exist", path.display()))
        }
        _ => CmError::invalid_field(field, format!("cannot read {}: {e}", path.display())),
    })?;
    if m.is_file() {
        Ok(m.len())
    } else {
        Err(CmError::invalid_field(
            field,
            format!("{} is not a file", path.display()),
        ))
    }
}

/// Seconds since the Unix epoch now.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Checks `meta` and the files it names against `rules`. The first problem is returned as
/// `invalid_input` whose message starts with the field name (`title`, `description`, `tags`,
/// `privacy`, `made_for_kids`, `video`, `thumbnail`, `captions`, `scheduled_at`, ...). Returns the
/// size of the video.
pub fn validate(rules: &PlatformRules, video: &Path, meta: &PublishMeta) -> Result<u64, CmError> {
    let size = validate_video(rules, video, meta)?;
    validate_meta(rules, meta)?;
    Ok(size)
}

/// The video file part of [`validate`]: it exists, has an accepted extension, a size within the
/// limit and (if known) a duration within the limit. Returns the size.
pub fn validate_video(
    rules: &PlatformRules,
    video: &Path,
    meta: &PublishMeta,
) -> Result<u64, CmError> {
    // Video file.
    let size = file_size("video", video)?;
    let ext = ext_of(video);
    if !rules.video_extensions.contains(&ext) {
        return Err(CmError::invalid_field(
            "video",
            format!(
                "{} files are not accepted by {} (use {})",
                if ext.is_empty() { "this kind of" } else { &ext },
                rules.platform,
                rules.video_extensions.join(", ")
            ),
        ));
    }
    if size == 0 {
        return Err(CmError::invalid_field("video", "the file is empty"));
    }
    if size > rules.max_bytes {
        return Err(CmError::invalid_field(
            "video",
            format!(
                "{size} bytes is over the {} limit of {} bytes",
                rules.platform, rules.max_bytes
            ),
        ));
    }
    if let (Some(limit), Some(d)) = (rules.max_duration_secs, meta.duration_secs) {
        if d > limit as f64 {
            return Err(CmError::invalid_field(
                "video",
                format!("{d:.0} s is over the {} limit of {limit} s", rules.platform),
            ));
        }
    }
    Ok(size)
}

/// The metadata part of [`validate`] (title, description, tags, privacy, scheduling, made for
/// kids, category, thumbnail, captions, playlist), for a request whose video does not exist yet
/// (it will be exported first). Same messages, same order.
pub fn validate_meta(rules: &PlatformRules, meta: &PublishMeta) -> Result<(), CmError> {
    // Title.
    if rules.title_required && meta.title.trim().is_empty() {
        return Err(CmError::invalid_field("title", "a title is required"));
    }
    let n = meta.title.chars().count();
    if n > rules.title_max_chars {
        return Err(CmError::invalid_field(
            "title",
            format!("{n} characters, the limit is {}", rules.title_max_chars),
        ));
    }
    if rules.platform == "youtube" {
        // The API refuses angle brackets in these fields (invalidTitle, invalidDescription).
        if meta.title.contains(['<', '>']) {
            return Err(CmError::invalid_field("title", "must not contain < or >"));
        }
        if meta.description.contains(['<', '>']) {
            return Err(CmError::invalid_field(
                "description",
                "must not contain < or >",
            ));
        }
    }

    // Description.
    if meta.description.len() > rules.description_max_bytes {
        return Err(CmError::invalid_field(
            "description",
            format!(
                "{} bytes, the limit is {}",
                meta.description.len(),
                rules.description_max_bytes
            ),
        ));
    }

    // Tags.
    let mut total = 0usize;
    for (i, t) in meta.tags.iter().enumerate() {
        if t.trim().is_empty() {
            return Err(CmError::invalid_field(
                "tags",
                format!("tag {} is empty", i + 1),
            ));
        }
        if t.contains(['<', '>', ',']) {
            return Err(CmError::invalid_field(
                "tags",
                format!("tag \"{t}\" must not contain < > or a comma"),
            ));
        }
        total += t.chars().count() + if t.contains(' ') { 2 } else { 0 };
    }
    // Separators between tags count too.
    total += meta.tags.len().saturating_sub(1);
    if total > rules.tags_max_total_chars {
        return Err(CmError::invalid_field(
            "tags",
            format!(
                "{total} characters in total, the limit is {}",
                rules.tags_max_total_chars
            ),
        ));
    }

    // Privacy, scheduling.
    let privacy = meta.effective_privacy(rules);
    if !rules.privacies.contains(&privacy) {
        return Err(CmError::invalid_field(
            "privacy",
            format!("{} does not offer \"{}\"", rules.platform, privacy.as_str()),
        ));
    }
    if let Some(at) = meta.scheduled_at_unix {
        if privacy != Privacy::Private {
            return Err(CmError::invalid_field(
                "scheduled_at",
                "a scheduled video is uploaded as private; set privacy to private",
            ));
        }
        if at <= now_unix() {
            return Err(CmError::invalid_field(
                "scheduled_at",
                "must be in the future",
            ));
        }
    }

    // Made for kids.
    if rules.requires_made_for_kids && meta.made_for_kids.is_none() {
        return Err(CmError::invalid_field(
            "made_for_kids",
            "answer yes or no; there is no default",
        ));
    }

    // Category: digits only for YouTube.
    if let Some(c) = &meta.category {
        if c.is_empty() || !c.bytes().all(|b| b.is_ascii_digit()) {
            return Err(CmError::invalid_field(
                "category",
                "expected a numeric category id such as \"28\"",
            ));
        }
    }

    // Thumbnail.
    if let Some(t) = &meta.thumbnail {
        let ext = ext_of(t);
        if !rules.thumbnail_extensions.contains(&ext) {
            return Err(CmError::invalid_field(
                "thumbnail",
                format!("use {}", rules.thumbnail_extensions.join(" or ")),
            ));
        }
        let sz = file_size("thumbnail", t)?;
        if sz > rules.thumbnail_max_bytes {
            return Err(CmError::invalid_field(
                "thumbnail",
                format!(
                    "{sz} bytes, the limit is {} bytes",
                    rules.thumbnail_max_bytes
                ),
            ));
        }
    }

    // Captions.
    if let Some(c) = &meta.captions {
        if ext_of(c) != "srt" {
            return Err(CmError::invalid_field("captions", "expected an .srt file"));
        }
        file_size("captions", c)?;
    }
    if let Some(l) = &meta.captions_language {
        if l.is_empty()
            || l.len() > 16
            || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(CmError::invalid_field(
                "captions_language",
                "expected a language tag such as \"en\" or \"pt-BR\"",
            ));
        }
    }

    if let Some(p) = &meta.playlist_id {
        if p.is_empty()
            || !p
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(CmError::invalid_field("playlist_id", "not a playlist id"));
        }
    }
    Ok(())
}

/// Unix seconds as RFC 3339 UTC (`2026-10-24T09:30:00Z`), for YouTube's `publishAt`.
pub fn rfc3339_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(dir: &tempfile::TempDir) -> PathBuf {
        let p = dir.path().join("v.mp4");
        std::fs::write(&p, b"0123456789").unwrap();
        p
    }

    fn ok_meta() -> PublishMeta {
        PublishMeta {
            title: "Demo".into(),
            made_for_kids: Some(false),
            ..PublishMeta::default()
        }
    }

    fn field_of(e: CmError) -> String {
        match e {
            CmError::InvalidInput { message } => message.split(':').next().unwrap().to_owned(),
            other => panic!("not invalid_input: {other:?}"),
        }
    }

    #[test]
    fn accepts_a_complete_request() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            validate(&PlatformRules::youtube(), &video(&d), &ok_meta()).unwrap(),
            10
        );
    }

    #[test]
    fn names_the_field() {
        let d = tempfile::tempdir().unwrap();
        let v = video(&d);
        let r = PlatformRules::youtube();
        let bad = |f: &dyn Fn(&mut PublishMeta)| {
            let mut m = ok_meta();
            f(&mut m);
            field_of(validate(&r, &v, &m).unwrap_err())
        };
        assert_eq!(bad(&|m| m.title = String::new()), "title");
        assert_eq!(bad(&|m| m.title = "x".repeat(101)), "title");
        assert_eq!(bad(&|m| m.title = "a<b".into()), "title");
        assert_eq!(bad(&|m| m.description = "x".repeat(5001)), "description");
        assert_eq!(
            bad(&|m| m.tags = vec!["x".repeat(300), "y".repeat(300)]),
            "tags"
        );
        assert_eq!(bad(&|m| m.tags = vec!["a,b".into()]), "tags");
        assert_eq!(bad(&|m| m.made_for_kids = None), "made_for_kids");
        assert_eq!(bad(&|m| m.category = Some("tech".into())), "category");
        assert_eq!(
            bad(&|m| m.thumbnail = Some("/nope/t.jpg".into())),
            "thumbnail"
        );
        assert_eq!(bad(&|m| m.thumbnail = Some("t.gif".into())), "thumbnail");
        assert_eq!(bad(&|m| m.captions = Some("c.txt".into())), "captions");
        assert_eq!(bad(&|m| m.duration_secs = Some(13.0 * 3600.0)), "video");
        assert_eq!(
            bad(&|m| {
                m.privacy = Some(Privacy::Public);
                m.scheduled_at_unix = Some(now_unix() + 3600);
            }),
            "scheduled_at"
        );
        assert_eq!(
            bad(&|m| {
                m.privacy = Some(Privacy::Private);
                m.scheduled_at_unix = Some(1);
            }),
            "scheduled_at"
        );
    }

    #[test]
    fn tag_total_counts_quotes_and_separators() {
        let d = tempfile::tempdir().unwrap();
        let v = video(&d);
        let r = PlatformRules::youtube();
        let mut m = ok_meta();
        // (247 + 2 quotes) + 248 + 1 separator = 498: fits.
        m.tags = vec![format!("{} a", "x".repeat(245)), "y".repeat(248)];
        assert!(validate(&r, &v, &m).is_ok());
        m.tags[1] = "y".repeat(251); // 501
        assert_eq!(field_of(validate(&r, &v, &m).unwrap_err()), "tags");
    }

    #[test]
    fn video_file_problems_name_video() {
        let d = tempfile::tempdir().unwrap();
        let r = PlatformRules::youtube();
        let m = ok_meta();
        assert_eq!(
            field_of(validate(&r, &d.path().join("missing.mp4"), &m).unwrap_err()),
            "video"
        );
        let txt = d.path().join("v.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert_eq!(field_of(validate(&r, &txt, &m).unwrap_err()), "video");
        let empty = d.path().join("e.mp4");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(field_of(validate(&r, &empty, &m).unwrap_err()), "video");
        let mut small = r.clone();
        small.max_bytes = 5;
        assert_eq!(
            field_of(validate(&small, &video(&d), &m).unwrap_err()),
            "video"
        );
    }

    #[test]
    fn rfc3339() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(rfc3339_utc(1_791_000_000), "2026-10-03T04:00:00Z");
    }

    #[test]
    fn default_privacy_is_the_platforms() {
        let r = PlatformRules::youtube();
        assert_eq!(
            PublishMeta::default().effective_privacy(&r),
            Privacy::Unlisted
        );
    }
}
