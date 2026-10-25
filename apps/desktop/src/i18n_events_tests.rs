//! Text that used to be made in English at the event and kept (or shown) as a `String`, now a
//! reason worded when drawn: poster notes, tool failures, destination rules, pasted-field
//! errors, recording-editor notes, library load failures, HUD warnings and error toasts.
//!
//! Each test pins the English (byte-identical to the previous text), the Vietnamese draft, and,
//! where the text was kept, that the same stored value follows a language switch. Everything is
//! tested with fakes.

use cm_core::message::{render_warning, WARNING_TEMPLATES};
use cm_i18n::{with_language, Lang};

use crate::cloud_connect::FieldError;
use crate::daemon_text::{any_error_text, warning_text, AppNote, ErrorNote};
use crate::destinations::RuleError;
use crate::flows;
use crate::recedit::model::EditNote;
use crate::recording::files::{PosterError, RunError};
use crate::toast::ToastQueue;

fn en<T>(f: impl FnOnce() -> T) -> T {
    with_language(Lang::En, f)
}
fn vi<T>(f: impl FnOnce() -> T) -> T {
    with_language(Lang::Vi, f)
}
fn xa<T>(f: impl FnOnce() -> T) -> T {
    with_language(Lang::EnXa, f)
}

// ---- poster notes and tool failures ---------------------------------------------------------

#[test]
fn poster_errors_read_as_before_in_english() {
    en(|| {
        assert_eq!(
            PosterError::NoScreenTrack.to_string(),
            "the recording has no screen track"
        );
        assert_eq!(
            PosterError::TrackUnreadable("denied".into()).to_string(),
            "cannot read the screen track: denied"
        );
        assert_eq!(
            PosterError::NotMp4.to_string(),
            "the screen track is not an MP4 file"
        );
        assert_eq!(
            PosterError::FfmpegMissing.to_string(),
            "ffmpeg is not installed"
        );
        assert_eq!(
            PosterError::FfmpegNoFrame.to_string(),
            "ffmpeg could not read a frame"
        );
        assert_eq!(
            PosterError::Ffmpeg(RunError::TimedOut(20)).to_string(),
            "ffmpeg: timed out after 20 s"
        );
        assert_eq!(
            PosterError::Ffmpeg(RunError::Os("No such file".into())).to_string(),
            "ffmpeg: No such file"
        );
        assert_eq!(
            PosterError::QuickLook(RunError::TimedOut(5)).to_string(),
            "Quick Look: timed out after 5 s"
        );
        assert_eq!(
            PosterError::QuickLookNoThumbnail.to_string(),
            "Quick Look made no thumbnail"
        );
        assert_eq!(
            PosterError::NoThumbnailTool.to_string(),
            "no thumbnail tool on this OS"
        );
        assert_eq!(
            PosterError::Both(
                Box::new(PosterError::FfmpegMissing),
                Box::new(PosterError::NoThumbnailTool)
            )
            .to_string(),
            "ffmpeg is not installed; no thumbnail tool on this OS"
        );
        assert_eq!(
            PosterError::Decode("bad header".into()).to_string(),
            "cannot decode poster: bad header"
        );
        assert_eq!(PosterError::Os("disk full".into()).to_string(), "disk full");
    });
}

/// A stored poster note is worded when drawn, so it follows the language; the tool's own text
/// stays as it came.
#[test]
fn a_stored_poster_note_follows_the_language() {
    let note = PosterError::Both(
        Box::new(PosterError::Ffmpeg(RunError::Os("odd tool text".into()))),
        Box::new(PosterError::QuickLookNoThumbnail),
    );
    let english = en(|| note.to_string());
    assert_eq!(
        english,
        "ffmpeg: odd tool text; Quick Look made no thumbnail"
    );
    let viet = vi(|| note.to_string());
    assert_ne!(viet, english);
    assert!(viet.contains("odd tool text"), "{viet}");
    assert!(viet.contains("ffmpeg"), "{viet}");
    assert!(xa(|| note.to_string()).starts_with('['));
    let timeout = PosterError::Ffmpeg(RunError::TimedOut(20));
    assert!(vi(|| timeout.to_string()).contains("20"));
    assert_ne!(vi(|| timeout.to_string()), en(|| timeout.to_string()));
}

// ---- destination rules and pasted fields --------------------------------------------------------

#[test]
fn destination_rules_read_as_before_and_follow_the_language() {
    let cases: Vec<(RuleError, &str)> = vec![
        (RuleError::NameMissing, "Give the destination a name."),
        (
            RuleError::NameTooLong(60),
            "Keep the name to 60 characters.",
        ),
        (
            RuleError::NameTaken("NAS".into()),
            "There is already a destination named \u{201c}NAS\u{201d}.",
        ),
        (
            RuleError::PatternEmpty,
            "The folder pattern can\u{2019}t be empty.",
        ),
        (
            RuleError::PatternDotDot,
            "The pattern can\u{2019}t contain \u{201c}..\u{201d}.",
        ),
        (
            RuleError::PatternTokens,
            "Use only {yyyy}, {MM}, {dd} and {title} between braces.",
        ),
        (
            RuleError::FolderNotAbsolute,
            "Choose a folder by its full path.",
        ),
        (
            RuleError::FolderMissing("/x/y".into()),
            "/x/y does not exist.",
        ),
        (
            RuleError::NotAFolder("/x/f".into()),
            "/x/f is not a folder.",
        ),
        (
            RuleError::FolderTaken("Drive".into()),
            "That folder is already the destination \u{201c}Drive\u{201d}.",
        ),
        (
            RuleError::DestinationGone,
            "That destination no longer exists.",
        ),
        (RuleError::Os("read-only volume".into()), "read-only volume"),
    ];
    for (e, english) in cases {
        assert_eq!(en(|| e.to_string()), english);
        if !matches!(e, RuleError::Os(_)) {
            let v = vi(|| e.to_string());
            assert_ne!(v, english, "{e:?} has no Vietnamese");
            assert!(
                !v.contains("{max}") && !v.contains("{name}") && !v.contains("{path}"),
                "{v}"
            );
        }
    }
    let taken = RuleError::NameTaken("NAS".into());
    assert!(vi(|| taken.to_string()).contains("NAS"));
    let link = RuleError::LinkNotOffered {
        name: "Google Drive".into(),
        link: cm_storage::LinkMode::Organization,
    };
    assert_eq!(
        en(|| link.to_string()),
        "Google Drive can\u{2019}t make \u{201c}People in my organization\u{201d} links."
    );
    let v = vi(|| link.to_string());
    assert!(v.contains("Google Drive"), "{v}");
    assert_ne!(v, en(|| link.to_string()));
}

#[test]
fn pasted_field_errors_read_as_before_and_follow_the_language() {
    use cm_storage::Kind;
    let cases: Vec<(FieldError, &str)> = vec![
        (FieldError::IdMissing(Kind::GoogleDrive), "Paste the client ID."),
        (
            FieldError::IdMissing(Kind::OneDrive),
            "Paste the application (client) ID.",
        ),
        (FieldError::IdMissing(Kind::Dropbox), "Paste the app key."),
        (
            FieldError::IdSpaces,
            "The client ID has no spaces in it; copy it again.",
        ),
        (
            FieldError::NotGoogleId(".apps.googleusercontent.com"),
            "That doesn\u{2019}t look like a Google client ID. It ends with .apps.googleusercontent.com.",
        ),
        (FieldError::SecretMissing, "Paste the client secret."),
        (
            FieldError::SecretSpaces,
            "The client secret has no spaces in it; copy it again.",
        ),
        (
            FieldError::NoSecretNeeded("Dropbox"),
            "Dropbox needs no secret; leave this empty.",
        ),
    ];
    for (e, english) in cases {
        assert_eq!(en(|| e.to_string()), english);
        assert_ne!(vi(|| e.to_string()), english, "{e:?}");
    }
    assert!(vi(|| FieldError::NoSecretNeeded("Dropbox").to_string()).contains("Dropbox"));
    assert!(
        vi(|| FieldError::NotGoogleId(".apps.googleusercontent.com").to_string())
            .contains(".apps.googleusercontent.com")
    );
}

// ---- recording editor notes ---------------------------------------------------------------------

#[test]
fn editor_notes_read_as_before_and_follow_the_language() {
    let cases = [
        (
            EditNote::SelectSegmentForLayout,
            "Select a segment to change its camera layout",
        ),
        (
            EditNote::InBeforeOut,
            "The in point has to be before the out point",
        ),
        (
            EditNote::OutAfterIn,
            "The out point has to be after the in point",
        ),
        (
            EditNote::SplitInsideKept,
            "Move the playhead inside the kept part to split",
        ),
        (EditNote::AlreadySplit, "Already split here"),
        (EditNote::SelectToCut, "Select a range or a segment to cut"),
        (EditNote::NothingToUndo, "Nothing to undo"),
        (EditNote::NothingToRedo, "Nothing to redo"),
        (
            EditNote::SelectionOutside,
            "The selection is outside the kept part",
        ),
        (
            EditNote::SelectCutToRestore,
            "Select a cut (the hatched part) to restore it",
        ),
        (
            EditNote::SelectSegmentForSpeed,
            "Select a segment to change its speed",
        ),
        (
            EditNote::LoopOn,
            "Loop on: playback starts again at the end",
        ),
        (EditNote::LoopOff, "Loop off: playback stops at the end"),
        (
            EditNote::ZoomInsideKept,
            "Move the playhead inside the kept part to add a zoom",
        ),
        (
            EditNote::SelectZoomFirst,
            "Select a zoom on the zoom lane first",
        ),
    ];
    for (note, english) in cases {
        assert_eq!(en(|| note.to_string()), english);
        assert_ne!(vi(|| note.to_string()), english, "{note:?}");
        assert!(xa(|| note.to_string()).starts_with('['));
    }
}

// ---- library load failure and editor opening failure ------------------------------------------

#[test]
fn a_failed_library_load_is_worded_when_drawn() {
    let mut s = crate::library::LibraryState::new(crate::library::LibraryStatus::Loading);
    let token = s.begin_load();
    s.finish_load(token, Err(ErrorNote::app(AppNote::LibraryUnavailable)));
    let crate::library::LibraryStatus::Failed(note) = &s.status else {
        panic!("not failed: {:?}", s.status);
    };
    assert_eq!(en(|| note.text()), "The library is not available");
    assert_ne!(vi(|| note.text()), "The library is not available");

    // A daemon message with a stable id (code lost on the way) is worded too.
    let daemon = ErrorNote::plain("No recording is running");
    assert_eq!(en(|| daemon.text()), "No recording is running");
    assert_ne!(vi(|| daemon.text()), "No recording is running");
    // Anything else stays as it came.
    let odd = ErrorNote::plain("sqlite: database is locked");
    assert_eq!(vi(|| odd.text()), "sqlite: database is locked");
}

#[test]
fn editor_opening_notes_keep_their_detail() {
    let read = ErrorNote::app_with(AppNote::ReadOriginal("denied".into()), "denied".into());
    assert_eq!(en(|| read.text()), "cannot read the original: denied");
    let v = vi(|| read.text());
    assert!(v.contains("denied") && v != en(|| read.text()), "{v}");
    assert_eq!(read.message(), "denied");
    let dec = ErrorNote::app_with(AppNote::DecodeOriginal("bad".into()), "bad".into());
    assert_eq!(en(|| dec.text()), "cannot decode the original: bad");
    assert_eq!(
        en(|| ErrorNote::app(AppNote::LibraryUnavailableDot).text()),
        "The library is not available."
    );
    assert_eq!(
        en(|| ErrorNote::app(AppNote::CaptureNoId).text()),
        "The capture has no id"
    );
}

// ---- HUD warnings ---------------------------------------------------------------------------------

fn sample(names: &[&str]) -> Vec<(String, String)> {
    names
        .iter()
        .map(|n| {
            let v = match *n {
                "track" => "camera".to_owned(),
                "minutes" => "12".to_owned(),
                "mb" => "300".to_owned(),
                "pct" => "4.5".to_owned(),
                "secs" => "3.2".to_owned(),
                other => format!("<{other}>"),
            };
            ((*n).to_owned(), v)
        })
        .collect()
}

fn names_of(english: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = english;
    while let Some(open) = rest.find('{') {
        let close = open + rest[open..].find('}').unwrap();
        out.push(&rest[open + 1..close]);
        rest = &rest[close + 1..];
    }
    out
}

/// Every session note with a stable id: English unchanged, a Vietnamese sentence that keeps the
/// values, and the pseudo-locale proves it goes through the catalogue.
#[test]
fn every_recording_warning_is_worded_from_its_id() {
    for (id, english) in WARNING_TEMPLATES {
        let params = sample(&names_of(english));
        let refs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let message = render_warning(id, &refs).unwrap();
        assert_eq!(en(|| warning_text(&message)), message, "{id}");
        let xa_text = xa(|| warning_text(&message));
        assert_ne!(xa_text, message, "{id} has no arm");
        let v = vi(|| warning_text(&message));
        assert_ne!(v, message, "{id} has no Vietnamese");
        for (k, value) in &params {
            // The track word is translated; every other value is kept as received.
            if k != "track" {
                assert!(v.contains(value.as_str()), "{id}: {v} lost {value}");
            }
        }
        assert!(!v.contains('{'), "{id}: {v}");
    }
}

#[test]
fn a_hud_warning_with_an_unknown_text_is_shown_as_sent() {
    for text in ["cursor log is slow", "tick failed: boom"] {
        assert_eq!(vi(|| warning_text(text)), text);
    }
    // Track names inside a sentence are translated too.
    let m = "camera track could not start: no device";
    assert_eq!(en(|| warning_text(m)), m);
    let v = vi(|| warning_text(m));
    assert!(
        v.contains("no device") && !v.contains("camera track"),
        "{v}"
    );
}

// ---- error toasts -----------------------------------------------------------------------------------

#[test]
fn error_toast_bodies_with_a_stable_id_are_worded_and_others_are_kept() {
    let t = en(|| flows::error_toast("Title", "A recording is already running"));
    assert_eq!(t.body.as_deref(), Some("A recording is already running"));
    let t = vi(|| flows::error_toast("Title", "A recording is already running"));
    assert_ne!(t.body.as_deref(), Some("A recording is already running"));
    let t = vi(|| flows::error_toast("Title", "Google said: quota"));
    assert_eq!(t.body.as_deref(), Some("Google said: quota"));
    assert!(vi(|| any_error_text("Display 3 not found")).contains('3'));
}

/// A live error toast is worded again when the language changes while it is on screen.
#[test]
fn a_live_error_toast_follows_a_language_switch() {
    let now = std::time::Instant::now();
    let mut q = ToastQueue::default();
    en(|| {
        q.push(
            flows::error_toast_live(
                || cm_i18n::tr!("Couldn\u{2019}t save the setting"),
                "A recording is already running",
            ),
            now,
        )
    });
    assert_eq!(q.toasts()[0].title, "Couldn\u{2019}t save the setting");
    assert_eq!(
        q.toasts()[0].body.as_deref(),
        Some("A recording is already running")
    );
    assert!(vi(|| q.relabel()));
    assert_ne!(q.toasts()[0].title, "Couldn\u{2019}t save the setting");
    assert_ne!(
        q.toasts()[0].body.as_deref(),
        Some("A recording is already running")
    );
    assert!(en(|| q.relabel()));
    assert_eq!(q.toasts()[0].title, "Couldn\u{2019}t save the setting");
    assert_eq!(
        q.toasts()[0].body.as_deref(),
        Some("A recording is already running")
    );
}
