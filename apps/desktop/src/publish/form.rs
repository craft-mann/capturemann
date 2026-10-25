//! The publish sheet's form (PUB-2): the fields, their defaults, which one is wrong and why
//! the Publish button is disabled. Display-free; the sheet only draws what this says.
//!
//! The limits are not repeated here: [`cm_publish::validate`] is run on the same
//! [`PublishMeta`] the daemon will validate again, and its first complaint is mapped to the
//! field it names (`title: 101 characters, the limit is 100`). It is run again with the
//! complaint's field set to something valid until it is satisfied, so every wrong field is
//! shown at once instead of one per click.

use std::path::{Path, PathBuf};

use cm_i18n::{tr, tr_ctx};
use cm_publish::{validate, PlatformRules, Privacy, PublishMeta};

/// A field of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Video,
    Title,
    Description,
    Tags,
    Privacy,
    MadeForKids,
    Thumbnail,
    Playlist,
}

impl Field {
    fn named(name: &str) -> Option<Field> {
        Some(match name {
            "video" => Field::Video,
            "title" => Field::Title,
            "description" => Field::Description,
            "tags" => Field::Tags,
            "privacy" => Field::Privacy,
            "made_for_kids" => Field::MadeForKids,
            "thumbnail" => Field::Thumbnail,
            "playlist_id" => Field::Playlist,
            _ => return None,
        })
    }
}

/// "x of 100 uploads used today", as the daemon reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota {
    pub used: u32,
    pub limit: u32,
}

impl Quota {
    pub fn label(self) -> String {
        tr!(
            "{used} of {limit} uploads used today",
            used = self.used,
            limit = self.limit
        )
    }

    pub fn exhausted(self) -> bool {
        self.used >= self.limit
    }
}

/// What the sheet knows besides the fields.
#[derive(Debug, Clone, Copy, Default)]
pub struct Readiness<'a> {
    /// A connected YouTube account is chosen.
    pub account: bool,
    /// The export that will be uploaded; `None` until the recording has one.
    pub export: Option<&'a Path>,
    pub duration_secs: Option<f64>,
    pub quota: Option<Quota>,
}

/// The state of the form.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishForm {
    pub title: String,
    pub description: String,
    /// Comma separated, as typed.
    pub tags_text: String,
    pub privacy: Privacy,
    /// Public needs this extra, explicit step.
    pub public_confirmed: bool,
    /// Required yes/no, no default.
    pub made_for_kids: Option<bool>,
    pub thumbnail: Option<PathBuf>,
    /// A playlist id or the address of a playlist, as typed; empty for none.
    pub playlist_text: String,
    /// Upload the recording's captions (SRT) with the video. Only offered, and only sent, when
    /// the recording has captions.
    pub captions: bool,
}

/// The result of checking the form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Check {
    /// Wrong fields with the words to show under them.
    pub errors: Vec<(Field, String)>,
    /// Why Publish is disabled, in the order the user can fix things; `None` when it is enabled.
    pub blocker: Option<String>,
}

impl Check {
    pub fn can_publish(&self) -> bool {
        self.blocker.is_none()
    }

    pub fn error_of(&self, f: Field) -> Option<&str> {
        self.errors
            .iter()
            .find(|(g, _)| *g == f)
            .map(|(_, m)| m.as_str())
    }
}

/// The playlist id in what the user typed: the id itself, or the `list=` part of a playlist
/// address. `None` for nothing. Whatever is left is checked by `cm_publish::validate`, which
/// complains about an id that cannot be one.
pub fn parse_playlist(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(i) = t.find("list=") {
        let rest = &t[i + "list=".len()..];
        let end = rest.find(['&', '#', ' ']).unwrap_or(rest.len());
        return Some(rest[..end].to_owned());
    }
    Some(t.to_owned())
}

/// Tags typed as `a, b, c`: trimmed, empty ones dropped, repeats (ignoring case) dropped.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in text.split(',') {
        let t = t.trim();
        if t.is_empty() || out.iter().any(|o| o.eq_ignore_ascii_case(t)) {
            continue;
        }
        out.push(t.to_owned());
    }
    out
}

impl PublishForm {
    /// A new form: the recording's title, the account's default privacy (public still needs
    /// its extra step), and the kids question unanswered.
    pub fn new(title: &str, default_privacy: Privacy) -> PublishForm {
        PublishForm {
            title: title.to_owned(),
            description: String::new(),
            tags_text: String::new(),
            privacy: default_privacy,
            public_confirmed: false,
            made_for_kids: None,
            thumbnail: None,
            playlist_text: String::new(),
            captions: false,
        }
    }

    /// Choosing another privacy withdraws the public confirmation.
    pub fn set_privacy(&mut self, p: Privacy) {
        if self.privacy != p {
            self.public_confirmed = false;
        }
        self.privacy = p;
    }

    pub fn tags(&self) -> Vec<String> {
        parse_tags(&self.tags_text)
    }

    /// What the daemon is asked to publish.
    pub fn meta(&self, duration_secs: Option<f64>) -> PublishMeta {
        PublishMeta {
            title: self.title.trim().to_owned(),
            description: self.description.clone(),
            tags: self.tags(),
            privacy: Some(self.privacy),
            made_for_kids: self.made_for_kids,
            thumbnail: self.thumbnail.clone(),
            playlist_id: parse_playlist(&self.playlist_text),
            duration_secs,
            ..PublishMeta::default()
        }
    }

    /// Checks the fields against the platform rules and says why Publish is disabled, if it is.
    pub fn check(&self, rules: &PlatformRules, ready: Readiness<'_>) -> Check {
        let mut errors: Vec<(Field, String)> = Vec::new();
        if let Some(video) = ready.export {
            let mut meta = self.meta(ready.duration_secs);
            // Each pass fixes the field the last complaint named, so all wrong fields are found.
            for _ in 0..8 {
                let Err(e) = validate(rules, video, &meta) else {
                    break;
                };
                let text = e.to_string();
                let (name, why) = text.split_once(": ").unwrap_or(("", &text));
                let field = Field::named(name);
                match field {
                    Some(Field::MadeForKids) => {
                        // Not an error: the control asks for an answer.
                        meta.made_for_kids = Some(false);
                    }
                    Some(
                        f @ (Field::Title
                        | Field::Description
                        | Field::Tags
                        | Field::Thumbnail
                        | Field::Playlist),
                    ) => {
                        errors.push((f, sentence(why)));
                        match f {
                            Field::Title => meta.title = "x".into(),
                            Field::Description => meta.description.clear(),
                            Field::Tags => meta.tags.clear(),
                            Field::Playlist => meta.playlist_id = None,
                            _ => meta.thumbnail = None,
                        }
                    }
                    Some(f @ (Field::Privacy | Field::Video)) => {
                        errors.push((f, sentence(why)));
                        if f == Field::Video {
                            break;
                        }
                        meta.privacy = Some(rules.default_privacy);
                    }
                    None => {
                        // A complaint no field owns: show it on the video, the one all fields
                        // depend on, and stop.
                        errors.push((Field::Video, sentence(&text)));
                        break;
                    }
                }
            }
        }
        let blocker = if !ready.account {
            Some(tr!(
                "Connect a YouTube account in Settings \u{2192} Destinations first."
            ))
        } else if ready.export.is_none() {
            Some(tr!(
                "Export the recording first; the export is what gets uploaded."
            ))
        } else if ready.quota.is_some_and(Quota::exhausted) {
            Some(tr!(
                "The upload limit for today is used up; try again after midnight Pacific time."
            ))
        } else if let Some((_, why)) = errors.first() {
            Some(why.clone())
        } else if self.made_for_kids.is_none() {
            Some(tr!(
                "Answer \u{201c}Is this video made for kids?\u{201d} to publish."
            ))
        } else if self.privacy == Privacy::Public && !self.public_confirmed {
            Some(tr!("Confirm that everyone will be able to see this video."))
        } else {
            None
        };
        Check { errors, blocker }
    }
}

/// `101 characters, the limit is 100` becomes `101 characters, the limit is 100.`
fn sentence(s: &str) -> String {
    let s = s.trim();
    let mut c = s.chars();
    let mut out = match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    };
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// A line under the privacy choice about what YouTube may do, or `None` when nothing applies.
pub fn privacy_note(rules: &PlatformRules, privacy: Privacy) -> Option<String> {
    let locked = rules.unverified_project_privacy?;
    (privacy != locked).then(|| {
        let kept = match locked {
            Privacy::Private => tr_ctx!("visibility, in a sentence", "private"),
            Privacy::Unlisted => tr_ctx!("visibility, in a sentence", "unlisted"),
            Privacy::Public => tr_ctx!("visibility, in a sentence", "public"),
        };
        tr!(
            "If your Google Cloud project is not verified, YouTube may keep the video {privacy} whatever you choose here. CaptureMann tells you when that happens and never changes a video\u{2019}s visibility afterwards.",
            privacy = kept
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(dir: &tempfile::TempDir) -> PathBuf {
        let p = dir.path().join("rec.mp4");
        std::fs::write(&p, b"not really a video, but a file with bytes").unwrap();
        p
    }

    fn ready<'a>(video: &'a Path) -> Readiness<'a> {
        Readiness {
            account: true,
            export: Some(video),
            duration_secs: Some(60.0),
            quota: Some(Quota {
                used: 3,
                limit: 100,
            }),
        }
    }

    fn answered(mut f: PublishForm) -> PublishForm {
        f.made_for_kids = Some(false);
        f
    }

    #[test]
    fn defaults_are_the_recording_title_the_accounts_privacy_and_no_kids_answer() {
        let f = PublishForm::new("Weekly demo", Privacy::Unlisted);
        assert_eq!(f.title, "Weekly demo");
        assert_eq!(f.privacy, Privacy::Unlisted);
        assert_eq!(f.made_for_kids, None);
        assert!(!f.public_confirmed);
        // The account's default can be Public; the extra step is still needed.
        let p = PublishForm::new("t", Privacy::Public);
        assert!(!p.public_confirmed);
    }

    /// PUB-2.AC4: kids not answered, Publish stays disabled, with the reason.
    #[test]
    fn publish_is_disabled_until_made_for_kids_is_answered() {
        let dir = tempfile::tempdir().unwrap();
        let v = video(&dir);
        let rules = PlatformRules::youtube();
        let mut f = PublishForm::new("Demo", Privacy::Unlisted);
        let c = f.check(&rules, ready(&v));
        assert!(!c.can_publish());
        assert!(c.blocker.as_deref().unwrap().contains("made for kids"));
        assert!(
            c.errors.is_empty(),
            "an unanswered question is not an error"
        );
        f.made_for_kids = Some(true);
        assert!(f.check(&rules, ready(&v)).can_publish());
        f.made_for_kids = Some(false);
        assert!(f.check(&rules, ready(&v)).can_publish());
    }

    #[test]
    fn public_needs_an_extra_explicit_step_and_changing_privacy_withdraws_it() {
        let dir = tempfile::tempdir().unwrap();
        let v = video(&dir);
        let rules = PlatformRules::youtube();
        let mut f = answered(PublishForm::new("Demo", Privacy::Unlisted));
        f.set_privacy(Privacy::Public);
        let c = f.check(&rules, ready(&v));
        assert!(c.blocker.as_deref().unwrap().contains("everyone"));
        f.public_confirmed = true;
        assert!(f.check(&rules, ready(&v)).can_publish());
        f.set_privacy(Privacy::Private);
        f.set_privacy(Privacy::Public);
        assert!(!f.public_confirmed);
        assert!(!f.check(&rules, ready(&v)).can_publish());
        assert_eq!(f.meta(None).privacy, Some(Privacy::Public));
    }

    #[test]
    fn every_wrong_field_is_named_inline_by_the_platform_rules() {
        let dir = tempfile::tempdir().unwrap();
        let v = video(&dir);
        let rules = PlatformRules::youtube();
        let mut f = answered(PublishForm::new(&"t".repeat(101), Privacy::Unlisted));
        f.description = "a < b".into();
        f.tags_text = "ok, , bad<tag".into();
        let c = f.check(&rules, ready(&v));
        assert!(c.error_of(Field::Title).unwrap().contains("101 characters"));
        assert!(c.error_of(Field::Description).unwrap().contains("< or >"));
        assert!(c.error_of(Field::Tags).unwrap().contains("bad<tag"));
        assert!(!c.can_publish());
        // The first wrong field is the reason shown on the button.
        assert_eq!(
            c.blocker.as_deref(),
            c.errors.first().map(|(_, m)| m.as_str())
        );

        f.title = "   ".into();
        f.description.clear();
        f.tags_text.clear();
        let c = f.check(&rules, ready(&v));
        assert!(c
            .error_of(Field::Title)
            .unwrap()
            .contains("title is required"));
    }

    #[test]
    fn a_missing_thumbnail_file_or_a_wrong_type_is_reported_on_the_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let v = video(&dir);
        let rules = PlatformRules::youtube();
        let mut f = answered(PublishForm::new("Demo", Privacy::Unlisted));
        f.thumbnail = Some(dir.path().join("t.gif"));
        let c = f.check(&rules, ready(&v));
        assert!(c.error_of(Field::Thumbnail).unwrap().contains("jpg"));
        f.thumbnail = Some(dir.path().join("t.png"));
        let c = f.check(&rules, ready(&v));
        assert!(c
            .error_of(Field::Thumbnail)
            .unwrap()
            .contains("does not exist"));
        std::fs::write(dir.path().join("t.png"), b"png").unwrap();
        assert!(f.check(&rules, ready(&v)).can_publish());
    }

    #[test]
    fn the_blocker_follows_the_order_the_user_can_fix_things_in() {
        let dir = tempfile::tempdir().unwrap();
        let v = video(&dir);
        let rules = PlatformRules::youtube();
        let f = PublishForm::new("Demo", Privacy::Unlisted);
        let no_account = Readiness {
            account: false,
            ..ready(&v)
        };
        assert!(f
            .check(&rules, no_account)
            .blocker
            .unwrap()
            .contains("Connect"));
        let no_export = Readiness {
            export: None,
            ..ready(&v)
        };
        assert!(f
            .check(&rules, no_export)
            .blocker
            .unwrap()
            .contains("Export the recording"));
        let spent = Readiness {
            quota: Some(Quota {
                used: 100,
                limit: 100,
            }),
            ..ready(&v)
        };
        assert!(f
            .check(&rules, spent)
            .blocker
            .unwrap()
            .contains("limit for today"));
    }

    #[test]
    fn a_video_over_the_duration_limit_is_refused_on_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let v = video(&dir);
        let rules = PlatformRules::youtube();
        let f = answered(PublishForm::new("Demo", Privacy::Unlisted));
        let long = Readiness {
            duration_secs: Some(13.0 * 3600.0),
            ..ready(&v)
        };
        let c = f.check(&rules, long);
        assert!(c.error_of(Field::Video).unwrap().contains("limit"));
        assert!(!c.can_publish());
    }

    #[test]
    fn a_playlist_is_an_id_or_the_list_part_of_an_address() {
        assert_eq!(parse_playlist("  "), None);
        assert_eq!(
            parse_playlist(" PLabc_12-x ").as_deref(),
            Some("PLabc_12-x")
        );
        assert_eq!(
            parse_playlist("https://www.youtube.com/playlist?list=PLabc123&si=zz").as_deref(),
            Some("PLabc123")
        );
        assert_eq!(
            parse_playlist("https://youtube.com/watch?v=x&list=PLq9#t").as_deref(),
            Some("PLq9")
        );
    }

    /// A multi-line description goes through with its line breaks; a wrong playlist is named on
    /// its own field and blocks Publish.
    #[test]
    fn the_description_keeps_its_line_breaks_and_a_bad_playlist_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let v = video(&dir);
        let rules = PlatformRules::youtube();
        let mut f = answered(PublishForm::new("Demo", Privacy::Unlisted));
        f.description = "Line one\n\nLine three".into();
        f.playlist_text = "https://www.youtube.com/playlist?list=PLgood_1".into();
        assert!(f.check(&rules, ready(&v)).can_publish());
        let meta = f.meta(Some(60.0));
        assert_eq!(meta.description, "Line one\n\nLine three");
        assert_eq!(meta.playlist_id.as_deref(), Some("PLgood_1"));
        f.playlist_text = "not a playlist!".into();
        let c = f.check(&rules, ready(&v));
        assert!(c.error_of(Field::Playlist).is_some(), "{c:?}");
        assert!(!c.can_publish());
        f.playlist_text.clear();
        assert!(f.check(&rules, ready(&v)).can_publish());
        assert_eq!(f.meta(None).playlist_id, None);
    }

    #[test]
    fn tags_are_split_trimmed_and_deduplicated() {
        assert_eq!(
            parse_tags(" demo, Screen recording ,, DEMO ,tips"),
            ["demo", "Screen recording", "tips"]
        );
        assert!(parse_tags("").is_empty());
    }

    #[test]
    fn the_quota_reads_as_the_spec_says() {
        let q = Quota {
            used: 4,
            limit: 100,
        };
        assert_eq!(q.label(), "4 of 100 uploads used today");
        assert!(!q.exhausted());
        assert!(Quota {
            used: 100,
            limit: 100
        }
        .exhausted());
    }

    /// I18N: the reason Publish is disabled is worded in the current language.
    #[test]
    fn the_blocker_is_worded_in_the_current_language() {
        use cm_i18n::{with_language, Lang};
        let f = PublishForm::new("Demo", Privacy::Private);
        let rules = PlatformRules::youtube();
        let no_account = Readiness::default();
        let en = f.check(&rules, no_account).blocker.unwrap();
        assert_eq!(
            en,
            "Connect a YouTube account in Settings \u{2192} Destinations first."
        );
        let xa = with_language(Lang::EnXa, || f.check(&rules, no_account).blocker.unwrap());
        assert!(xa.starts_with('['), "{xa}");
        let quota = Quota {
            used: 3,
            limit: 100,
        };
        assert!(with_language(Lang::EnXa, || quota.label()).starts_with('['));
        assert_eq!(quota.label(), "3 of 100 uploads used today");
    }

    #[test]
    fn the_privacy_note_warns_about_unverified_projects_except_for_private() {
        let rules = PlatformRules::youtube();
        let n = privacy_note(&rules, Privacy::Public).unwrap();
        assert!(n.contains("not verified") && n.contains("private"));
        assert!(privacy_note(&rules, Privacy::Private).is_none());
    }

    #[test]
    fn the_meta_carries_what_the_sheet_shows() {
        let mut f = answered(PublishForm::new("  Demo ", Privacy::Unlisted));
        f.tags_text = "a, b".into();
        f.description = "Hello".into();
        let m = f.meta(Some(12.0));
        assert_eq!(m.title, "Demo");
        assert_eq!(m.tags, ["a", "b"]);
        assert_eq!(m.privacy, Some(Privacy::Unlisted));
        assert_eq!(m.made_for_kids, Some(false));
        assert_eq!(m.duration_secs, Some(12.0));
    }
}
