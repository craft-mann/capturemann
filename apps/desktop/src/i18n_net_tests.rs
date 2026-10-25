//! The pseudo-locale safety net (I18N integration, headless, no window).
//!
//! The pseudo-locale (`Lang::EnXa`) brackets and lengthens every string that goes through `tr!`.
//! This test builds the text of the label accessors of the app twice, in English and in the
//! pseudo-locale, and fails for every string that is the same in both: such a string was wrapped
//! somewhere but never reaches `tr!` on this path (an English constant, a `format!`, a helper
//! that returns the source text). It also checks the `Failure`, `ErrorNote`, toast and
//! confirmation texts that are worded when drawn.
//!
//! What it covers: the `label()`/`title()`/`text()` style functions of the enums and tables of the
//! shell, library, settings, permissions, recording sheet, HUD hints, editors, launcher, AI,
//! publish, destinations and toasts (see [`corpus`]). What it does not cover: text that only
//! exists inside a GPUI `render` (a window is needed to build it), and daemon sentences without
//! a stable id (shown as the daemon sent them, by design).
//!
//! To add a screen: push its accessors in [`corpus`]. A string that is meant to stay the same in
//! every language (a product name, a file format, a unit) goes into [`ALLOWED`].

use cm_i18n::{with_language, Lang};

/// Text that is the same in every language on purpose: product names, file formats, codecs.
const ALLOWED: &[&str] = &[
    "CaptureMann",
    "YouTube",
    "TikTok",
    "Facebook",
    "Google Drive",
    "OneDrive",
    "Dropbox",
    "MCP",
    "SRT",
    "VTT",
    "TXT",
    "HEVC",
    "H.264",
    "16:9",
    "9:16",
    "1:1",
    "1080p",
    "1440p",
    "2160p",
    "720p",
    "S",
    "M",
    "L",
];

fn allowed(s: &str) -> bool {
    // Numbers, glyphs and key names carry no sentence; product and format names are listed.
    !s.chars().any(char::is_alphabetic) || ALLOWED.contains(&s)
}

/// Every label the net looks at, with where it comes from.
#[derive(Default)]
struct Corpus {
    items: Vec<(String, String)>,
}

impl Corpus {
    fn add(&mut self, what: impl std::fmt::Display, text: impl Into<String>) {
        self.items.push((what.to_string(), text.into()));
    }

    /// A toast's words. `passes_through` is for a toast whose body is the daemon's or a
    /// provider's text (shown as it came, by design): the body is left out.
    fn toast(&mut self, what: &str, spec: &crate::toast::ToastSpec, passes_through: bool) {
        self.add(format!("{what}: title"), spec.title.clone());
        if let (Some(b), false) = (&spec.body, passes_through) {
            self.add(format!("{what}: body"), b.clone());
        }
        for (i, h) in spec.hints.iter().enumerate() {
            self.add(format!("{what}: hint {i}"), h.text.clone());
        }
        for (i, b) in spec.buttons.iter().enumerate() {
            self.add(format!("{what}: button {i}"), b.label.clone());
        }
    }
}

fn corpus() -> Vec<(String, String)> {
    use crate::agent_status::{self, decision_label, McpState, McpStatus, OfflineReason};
    use crate::ai::captions::{AddAtError, LineIssue};
    use crate::ai::models::{notice_body, notice_title, privacy_copy, transcribe_hint, RowStatus};
    use crate::ai::suggest::{nothing_found, Nothing};
    use crate::ai::summary::{language_name, regen_warning, LANGUAGES};
    use crate::cloud_connect::{self as cloud, AccountNote, ReconnectWhy};
    use crate::library::{CardAction, Chip, Kind, SortKey};
    use crate::nav::{page_heading, DockAction, NavItem};
    use crate::permissions::{self, Action, Id, Os, State};
    use crate::publish::job::{OutcomeWarning, OwnFailure};
    use crate::recording::geometry::{BubbleShape, BubbleSize};
    use crate::recording::start::{self, TargetKind};
    use crate::shortcuts::ShortcutAction;
    use crate::ui::app::{RebindError, Route};
    use crate::ui::settings::model::{self as sm, SettingsSection};
    use cm_daemon::ActivityDecision;

    let mut c = Corpus::default();

    // ---- shell: navigation, dock, library ----------------------------------------------------
    for n in NavItem::ALL {
        c.add(format!("NavItem::label({n:?})"), n.label());
    }
    let id = || "x".to_owned();
    for r in [
        Route::Library,
        Route::Screenshots,
        Route::Recordings,
        Route::Published,
        Route::Destinations,
        Route::Agents,
        Route::Editor(id()),
        Route::Recording(id()),
        Route::RecordingEditor(id()),
        Route::Publish(id()),
        Route::Settings,
        Route::Ai,
        Route::Permissions,
    ] {
        let (crumb, title) = page_heading(&r);
        c.add(format!("page_heading({r:?}): crumb"), crumb);
        c.add(format!("page_heading({r:?}): title"), title);
    }
    for d in DockAction::ALL {
        c.add(format!("DockAction::label({d:?})"), d.label());
    }
    for mac in [true, false] {
        for a in CardAction::ALL {
            c.add(format!("CardAction::label({a:?}, mac={mac})"), a.label(mac));
        }
    }
    for k in [Kind::Screenshot, Kind::Recording] {
        c.add(format!("Kind::label({k:?})"), k.label());
    }
    for ch in Chip::ALL {
        c.add(format!("Chip::label({ch:?})"), ch.label());
    }
    for s in SortKey::ALL {
        c.add(format!("SortKey::label({s:?})"), s.label());
    }

    // ---- settings -----------------------------------------------------------------------------
    for s in SettingsSection::ALL {
        c.add(format!("SettingsSection::label({s:?})"), s.label());
        c.add(format!("SettingsSection::title({s:?})"), s.title());
        c.add(format!("SettingsSection::subtitle({s:?})"), s.subtitle());
    }
    for a in ShortcutAction::ALL {
        c.add(format!("ShortcutAction::label({a:?})"), a.label());
    }
    for (i, (title, hint, _)) in sm::shortcut_groups().into_iter().enumerate() {
        c.add(format!("shortcut_groups[{i}]: title"), title);
        c.add(format!("shortcut_groups[{i}]: hint"), hint);
    }
    for p in sm::POLICIES {
        c.add(format!("policy_label({p:?})"), sm::policy_label(p));
        c.add(format!("policy_hint({p:?})"), sm::policy_hint(p));
    }
    for (i, (_icon, title, body)) in sm::privacy_facts().into_iter().enumerate() {
        c.add(format!("privacy_facts[{i}]: title"), title);
        c.add(format!("privacy_facts[{i}]: body"), body);
    }
    c.add("excluded_note", sm::excluded_note());
    c.add("excluded_caption", sm::excluded_caption());
    c.add("later_milestone", sm::later_milestone());
    for (name, _) in sm::PLATFORMS {
        if let Some(d) = sm::platform_detail(name) {
            c.add(format!("platform_detail({name})"), d);
        }
    }
    c.add(
        "ClientNameError",
        crate::settings::ClientNameError::Empty.text(),
    );
    c.add(
        "ClientNameError::TooLong",
        crate::settings::ClientNameError::TooLong.text(),
    );
    c.add(
        "ClientNameError::Control",
        crate::settings::ClientNameError::Control.text(),
    );
    c.add(
        "PolicyProblem::NotSaved",
        crate::ui::settings::PolicyProblem::NotSaved("disk full".into()).text(),
    );
    for e in [
        start::BundleIdError::Empty,
        start::BundleIdError::Spaces,
        start::BundleIdError::NotDotted,
    ] {
        c.add(format!("BundleIdError::{e:?}"), e.text());
    }
    for e in [
        RebindError::NeedsModifier,
        RebindError::BadKey,
        RebindError::Register,
    ] {
        c.add(format!("RebindError::{e:?}"), e.text());
    }

    // ---- permissions --------------------------------------------------------------------------
    for os in [Os::Mac, Os::Windows] {
        for s in permissions::specs(os) {
            c.add(format!("Spec({os:?}, {:?}): name", s.id), s.name.clone());
            c.add(format!("Spec({os:?}, {:?}): why", s.id), s.why.clone());
            c.add(
                format!("Spec({os:?}, {:?}): features", s.id),
                s.features.clone(),
            );
        }
        for a in [Action::Grant, Action::OpenSettings, Action::Restart] {
            c.add(format!("Action::label({a:?}, {os:?})"), a.label(os));
        }
    }
    let states = [
        State::Unknown,
        State::Granted,
        State::GrantedNeedsRestart,
        State::NotGranted,
        State::NotDetermined,
        State::Denied,
        State::Restricted,
    ];
    for s in states {
        c.add(format!("State::label({s:?})"), s.label());
        for id in [
            Id::ScreenRecording,
            Id::Microphone,
            Id::Camera,
            Id::InputMonitoring,
        ] {
            if let Some(n) = crate::ui::permissions_view::state_note(id, s) {
                c.add(format!("state_note({id:?}, {s:?})"), n);
            }
        }
    }
    c.add("windows_screen_note", permissions::windows_screen_note());
    c.add("screen_missing_title", permissions::screen_missing_title());
    c.add("screen_restart_title", permissions::screen_restart_title());
    c.add("screen_missing_hint", permissions::screen_missing_hint());

    // ---- recording sheet, HUD, geometry -------------------------------------------------------
    for k in TargetKind::ALL {
        c.add(format!("TargetKind::label({k:?})"), k.label());
    }
    for s in BubbleSize::ALL {
        c.add(format!("BubbleSize::label({s:?})"), s.label());
    }
    for s in [BubbleShape::Circle, BubbleShape::Rounded] {
        c.add(format!("BubbleShape::label({s:?})"), s.label());
    }
    for (k, _) in start::SIZES {
        c.add(format!("size_label({k})"), start::size_label(k));
    }
    for (k, _) in start::CODECS {
        c.add(format!("codec_label({k})"), start::codec_label(k));
    }
    for (k, _) in start::QUALITIES {
        c.add(format!("quality_label({k})"), start::quality_label(k));
    }
    for hint in crate::ui::recording::hud::all_hints() {
        c.add("HudHint", hint);
    }
    c.add("stop_label", crate::recording::hud_layout::stop_label());
    for secs in crate::delay::CHOICES {
        c.add(
            format!("delay::choice_label({secs})"),
            crate::delay::choice_label(secs),
        );
    }
    for resume in [false, true] {
        c.add(
            format!("countdown::label(3, {resume})"),
            crate::recording::countdown::label(3, resume),
        );
        c.add(
            format!("countdown::hint({resume})"),
            crate::recording::countdown::hint(resume),
        );
    }

    // ---- editors ------------------------------------------------------------------------------
    for t in crate::editor::tools::Tool::ALL {
        c.add(format!("Tool::label({t:?})"), t.label());
        for p in t.params() {
            c.add(format!("Param::label({p:?})"), p.label());
        }
    }
    for (id, _) in crate::editor::tools::PALETTE {
        c.add(
            format!("palette_name({id})"),
            crate::editor::tools::palette_name(id),
        );
    }
    for p in crate::recedit::inspector::Preset::ALL {
        c.add(format!("Preset::label({p:?})"), p.label());
    }
    for m in crate::recedit::camera::MODES {
        c.add(
            format!("mode_label({m:?})"),
            crate::recedit::camera::mode_label(m),
        );
    }
    for k in crate::recedit::camera::Corner::ALL {
        c.add(format!("Corner::label({k:?})"), k.label());
    }
    for i in crate::ui::recording_editor::tabs::Inspector::ALL {
        c.add(format!("Inspector::label({i:?})"), i.label());
    }
    for (keys, what) in crate::recedit::keys::help() {
        c.add(format!("keys::help({keys})"), what);
    }

    // ---- launcher and agents ------------------------------------------------------------------
    for g in crate::launcher::commands::Group::ORDER {
        c.add(format!("Group::label({g:?})"), g.label());
    }
    for d in [
        ActivityDecision::Approved,
        ActivityDecision::ApprovedForSession,
        ActivityDecision::Declined,
        ActivityDecision::TimedOut,
        ActivityDecision::ClientReadOnly,
        ActivityDecision::ClientBlocked,
    ] {
        c.add(format!("decision_label({d:?})"), decision_label(d));
    }
    for r in [
        OfflineReason::LibraryUnavailable,
        OfflineReason::NoEndpoint,
        OfflineReason::SocketOwned,
    ] {
        let s = McpStatus {
            state: McpState::Offline(r),
            last_call: None,
        };
        c.add(format!("McpStatus::title({r:?})"), s.title());
        for (i, l) in s.lines().into_iter().enumerate() {
            c.add(format!("McpStatus::lines({r:?})[{i}]"), l);
        }
    }
    let ready = agent_status::derive(true, true, None, &[]);
    c.add("McpStatus::title(ready)", ready.title());
    for (i, l) in ready.lines().into_iter().enumerate() {
        c.add(format!("McpStatus::lines(ready)[{i}]"), l);
    }

    // ---- AI -----------------------------------------------------------------------------------
    c.add("ai notice_title", notice_title());
    c.add("ai notice_body", notice_body());
    c.add("ai transcribe_hint", transcribe_hint());
    c.add("ai privacy_copy", privacy_copy());
    for (code, _) in LANGUAGES {
        if let Some(n) = language_name(code) {
            c.add(format!("language_name({code})"), n);
        }
    }
    c.add("regen_warning(edited)", regen_warning(true));
    c.add("regen_warning(plain)", regen_warning(false));
    for s in [
        RowStatus::NotInstalled,
        RowStatus::Installed,
        RowStatus::Active,
        RowStatus::Confirming,
        RowStatus::Downloading(0.5),
        RowStatus::Paused(0.5),
        RowStatus::Failed("network lost".into()),
        RowStatus::ConfirmingRemove,
    ] {
        c.add(format!("RowStatus::text({s:?})"), s.text());
    }
    for n in [
        Nothing::ZoomSpots,
        Nothing::Silences,
        Nothing::SilencesOrFillers,
    ] {
        c.add(format!("nothing_found({n:?})"), nothing_found(n));
    }
    for e in [AddAtError::InsideCaption, AddAtError::NoRoom] {
        c.add(format!("AddAtError::{e:?}"), e.message());
    }
    for i in [
        LineIssue::Empty,
        LineIssue::TooManyRows(3),
        LineIssue::RowTooLong(1, 50),
        LineIssue::TooShort(0.4),
        LineIssue::TooLong(9.0),
        LineIssue::OverlapsPrevious,
    ] {
        c.add(format!("LineIssue::message({i:?})"), i.message());
    }

    // ---- publish, destinations ----------------------------------------------------------------
    for o in [
        OwnFailure::NoLink,
        OwnFailure::NoDaemon,
        OwnFailure::NoJob,
        OwnFailure::UploadFailed,
    ] {
        c.add(format!("OwnFailure::{o:?}"), o.text());
    }
    c.add(
        "OutcomeWarning::PrivacyLocked",
        OutcomeWarning::PrivacyLocked.text(),
    );
    c.add(
        "privacy_locked_words",
        crate::publish::job::privacy_locked_words(),
    );
    for e in [
        crate::publish::thumb::ExtractError::NoFfmpeg,
        crate::publish::thumb::ExtractError::NoFrame,
        crate::publish::thumb::ExtractError::Ffmpeg(crate::recording::files::RunError::Os(
            "x".into(),
        )),
    ] {
        c.add(format!("ExtractError::{e:?}"), e.text());
    }
    c.add(
        "form::Quota::label",
        crate::publish::form::Quota {
            used: 3,
            limit: 100,
        }
        .label(),
    );
    c.add(
        "sheet::progress_text",
        crate::publish::sheet::progress_text(0.5, None),
    );
    for p in [
        cloud::Phase::Browser,
        cloud::Phase::Account,
        cloud::Phase::Folder,
        cloud::Phase::Saving,
    ] {
        c.add(format!("cloud Phase::label({p:?})"), p.label());
    }
    for p in [
        crate::publish_connect::Phase::Browser,
        crate::publish_connect::Phase::Channel,
        crate::publish_connect::Phase::Saving,
    ] {
        c.add(format!("youtube Phase::label({p:?})"), p.label());
    }
    for n in [
        AccountNote::PersonalMicrosoft,
        AccountNote::WorkMicrosoft,
        AccountNote::DropboxAppFolder,
        AccountNote::DropboxFull,
    ] {
        c.add(format!("AccountNote::{n:?}"), n.text());
    }
    for r in [
        ReconnectWhy::NoAccount,
        ReconnectWhy::NotSignedIn,
        ReconnectWhy::UploadsWaiting,
        ReconnectWhy::Refused,
    ] {
        c.add(format!("ReconnectWhy::{r:?}"), r.text());
    }
    for p in cloud::PROVIDERS {
        c.add(format!("{}: id_label", p.name), p.id_label());
        c.add(format!("{}: secret_label", p.name), p.secret_label());
        c.add(format!("{}: note", p.name), p.note());
        c.add(format!("{}: refusal_hint", p.name), p.refusal_hint());
        for (i, s) in p.steps().into_iter().enumerate() {
            c.add(format!("{}: step {i}", p.name), s);
        }
    }

    // ---- toasts -------------------------------------------------------------------------------
    {
        use crate::recording::model::Ended;
        use crate::recording::{exports, toasts, uploads};
        c.toast(
            "ended: stopped",
            &toasts::ended_toast(&Ended::Stopped {
                capture_id: Some("rec_1".into()),
                duration_s: Some(42.0),
                reason: "user".into(),
                error: None,
                permission: None,
            })
            .expect("a stopped take has a toast"),
            false,
        );
        c.toast(
            "ended: permission revoked",
            &toasts::ended_toast(&Ended::Stopped {
                capture_id: Some("rec_1".into()),
                duration_s: None,
                reason: "permission_revoked".into(),
                error: None,
                permission: Some(cm_core::Permission::Screen),
            })
            .expect("a revoked permission has a toast"),
            false,
        );
        c.toast(
            "exports: skipped",
            &exports::export_skipped_toast("rec_1"),
            false,
        );
        c.toast(
            "exports: failed (ffmpeg)",
            &exports::export_failed_toast("rec_1", "x", true),
            false,
        );
        c.toast(
            "exports: exported",
            &exports::exported_toast("rec_1", "My take", Some("burned")),
            false,
        );
        c.toast(
            "uploads: reconnect",
            &uploads::reconnect_toast("My Drive"),
            false,
        );
        c.toast(
            "uploads: failed",
            &uploads::upload_failed_toast("job_1", Some("My take"), "My Drive", "quota"),
            true,
        );
        c.toast(
            "start error: permission",
            &toasts::start_error_toast(
                Some(cm_core::ErrorCode::PermissionMissing),
                None,
                "no permission",
            ),
            true,
        );
    }
    c.items
}

#[test]
fn every_label_accessor_goes_through_the_catalogue() {
    let en = with_language(Lang::En, corpus);
    let xa = with_language(Lang::EnXa, corpus);
    assert_eq!(
        en.iter().map(|(w, _)| w).collect::<Vec<_>>(),
        xa.iter().map(|(w, _)| w).collect::<Vec<_>>(),
        "the corpus must have the same entries in both languages"
    );
    assert!(en.len() > 400, "only {} labels looked at", en.len());
    let plain: Vec<String> = en
        .iter()
        .zip(&xa)
        .filter(|((_, e), (_, x))| e == x && !allowed(e))
        .map(|((what, e), _)| format!("{what}: {e:?}"))
        .collect();
    assert!(
        plain.is_empty(),
        "{} strings stay plain English in the pseudo-locale (wrap them in tr!, or add a product \
         name or format to ALLOWED):\n{}",
        plain.len(),
        plain.join("\n")
    );
}

/// The check itself works: a string that skips `tr!` is found, a wrapped one is not.
#[test]
fn the_net_finds_a_string_that_skips_the_catalogue() {
    use cm_i18n::tr;
    fn made(wrapped: bool) -> Vec<(String, String)> {
        let text = if wrapped {
            tr!("Copy link")
        } else {
            "Copy link".to_owned()
        };
        vec![("sample".to_owned(), text)]
    }
    let differs = |wrapped| {
        let en = with_language(Lang::En, || made(wrapped));
        let xa = with_language(Lang::EnXa, || made(wrapped));
        en[0].1 != xa[0].1
    };
    assert!(differs(true));
    assert!(!differs(false));
}
