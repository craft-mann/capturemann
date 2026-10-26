//! The Publish page in the headless GPUI platform: a real `PublishPage` over a real daemon and a
//! hand-made recording, driven with the keyboard. Typing must reach the title, description, tags
//! and playlist fields, and what is typed must be what the form holds.

use std::sync::Arc;

use super::*;
use crate::backend::{Backend, StartConfig};
use crate::library::LocalClock;
use crate::recording::preview::PreviewSource;
use crate::settings::FileSettings;
use crate::shortcuts::{Platform, ShortcutMap};
use crate::ui::app::{AppHandle, AppModel};
use crate::ui::recording::RecState;
use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_daemon::testing::RecordingClipboard;
use cm_daemon::DenyAll;
use gpui::{Focusable, TestAppContext, VisualTestContext};

/// An empty config directory for this test process, made once. Without it the page would list the
/// publish accounts of whoever runs the tests and the daemon would read their keychain (a prompt,
/// and a real secret in a test). Tests must never touch the real keychain or account.
fn isolated_config_dir() {
    static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let dir = DIR.get_or_init(|| tempfile::tempdir().unwrap());
    std::env::set_var("CAPTUREMANN_CONFIG_DIR", dir.path());
    assert_eq!(crate::settings::config_dir(), dir.path());
}

struct World {
    _rt: tokio::runtime::Runtime,
    _dir: tempfile::TempDir,
}

/// A page opened on a recording, with the daemon, the app model and the key bindings in place.
fn page(cx: &mut TestAppContext) -> (World, Entity<PublishPage>, &mut VisualTestContext) {
    // The daemon runs on tokio threads; the test scheduler must accept activity from them.
    cx.executor().allow_parking();
    isolated_config_dir();
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    let bundle = lib.join("Demo.cmrec");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(
        bundle.join("manifest.json"),
        serde_json::json!({
            "schema": 1, "id": "rec_demo", "title": "Demo recording",
            "created": "2026-10-26T10:00:00Z", "tick_rate": 120000,
            "tracks": {}, "edits": [], "revisions": [], "gaps": [],
            "duration": 600000,
        })
        .to_string(),
    )
    .unwrap();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let backend = Arc::new(Backend::start(
        &rt,
        StartConfig {
            library_path: Some(lib),
            fallback_path: dir.path().join("fb"),
            settings: Arc::new(FileSettings::load(dir.path().join("s.json"))),
            endpoint: None,
            overlay_binary: None,
            recording: crate::backend::RecordingMode::Off,
        },
        Capturer::new(FakeBackend::new().with_display(1, 0, 0, 120, 80, 1.0)).with_own_pid(None),
        Arc::new(Arc::new(RecordingClipboard::default())),
        DenyAll,
    ));
    cx.update(|cx| {
        cx.set_global(ThemeGlobal(crate::theme::Theme::DARK));
        crate::ui::studio::bind_keys(cx);
        let platform = Platform::current();
        let rec = cx.new(|_| RecState::new(PreviewSource::Fake));
        let model = cx.new(|_| {
            AppModel::new(
                backend.clone(),
                ShortcutMap::defaults(platform),
                platform,
                LocalClock {
                    offset: time::UtcOffset::UTC,
                },
                rec,
            )
        });
        cx.set_global(AppHandle(model));
    });
    let app = cx.update(|cx| cx.global::<AppHandle>().0.clone());
    let window = cx.add_window(|_, cx| PublishPage::new(app, cx));
    let entity = window.root(cx).unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx).into_mut();
    entity.update(vcx, |p, cx| p.set_capture("rec_demo", cx));
    // The details are read on the background executor.
    for _ in 0..50 {
        vcx.run_until_parked();
        if entity.read_with(vcx, |p, _| matches!(p.load, Load::Ready(_))) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    vcx.run_until_parked();
    (World { _rt: rt, _dir: dir }, entity, vcx)
}

/// Gives `which` the focus (what a click on it does) and draws a frame.
fn focus(
    entity: &Entity<PublishPage>,
    vcx: &mut VisualTestContext,
    which: impl Fn(&PublishPage) -> Entity<TextField>,
) {
    entity.update_in(vcx, |p, window, cx| {
        let handle = which(p).focus_handle(cx);
        window.focus(&handle, cx);
    });
    vcx.run_until_parked();
    let focused = entity.update_in(vcx, |p, window, cx| {
        which(p).focus_handle(cx).is_focused(window)
    });
    assert!(focused, "the field did not take the focus");
}

#[gpui::test]
fn the_page_loads_the_recording_and_shows_its_form(cx: &mut TestAppContext) {
    let (_w, entity, vcx) = page(cx);
    assert!(
        entity.read_with(vcx, |p, _| matches!(p.load, Load::Ready(_))),
        "the page did not reach Ready"
    );
    // No publish account: the test runs on an empty config, never on the real one.
    assert!(entity.read_with(vcx, |p, _| p.accounts.is_empty()
        && p.accounts_error.is_none()));
    let title = entity.read_with(vcx, |p, cx| p.title_input.read(cx).text().to_owned());
    assert_eq!(
        title, "Demo recording",
        "the title field gets the recording's title"
    );
}

#[gpui::test]
fn typing_reaches_every_field_of_the_form(cx: &mut TestAppContext) {
    let (_w, entity, vcx) = page(cx);
    type Pick = fn(&PublishPage) -> Entity<TextField>;
    let fields: [(&str, Pick); 4] = [
        ("title", |p| p.title_input.clone()),
        ("description", |p| p.description_input.clone()),
        ("tags", |p| p.tags_input.clone()),
        ("playlist", |p| p.playlist_input.clone()),
    ];
    for (name, pick) in fields {
        focus(&entity, vcx, pick);
        // Select what is there (the title has a default), then type over it.
        vcx.simulate_keystrokes("cmd-a");
        vcx.simulate_input("typed text");
        let got = entity.read_with(vcx, |p, cx| pick(p).read(cx).text().to_owned());
        assert_eq!(got, "typed text", "typing in the {name} field");
    }
    // The form (what is checked and sent) follows the fields at the next frame.
    vcx.run_until_parked();
    entity.update(vcx, |p, cx| p.sync_form(cx));
    let (title, desc, tags, playlist) = entity.read_with(vcx, |p, _| {
        (
            p.form.title.clone(),
            p.form.description.clone(),
            p.form.tags_text.clone(),
            p.form.playlist_text.clone(),
        )
    });
    assert_eq!(
        (
            title.as_str(),
            desc.as_str(),
            tags.as_str(),
            playlist.as_str()
        ),
        ("typed text", "typed text", "typed text", "typed text")
    );
}

/// A click on each field gives it the focus and typing then lands in it: the path a person takes.
#[gpui::test]
fn clicking_a_field_then_typing_fills_it(cx: &mut TestAppContext) {
    let (_w, entity, vcx) = page(cx);
    // Draw the page once so every field has bounds.
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    let labels = FieldLabels::current();
    type Pick = fn(&PublishPage) -> Entity<TextField>;
    let fields: [(String, Pick); 4] = [
        (labels.title.0, |p| p.title_input.clone()),
        (labels.description.0, |p| p.description_input.clone()),
        (labels.tags.0, |p| p.tags_input.clone()),
        (labels.playlist.0, |p| p.playlist_input.clone()),
    ];
    for (label, pick) in fields {
        let selector: &'static str = Box::leak(format!("text-field:{label}").into_boxed_str());
        let bounds = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("the {label} field was not drawn"));
        vcx.simulate_click(bounds.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        let focused = entity.update_in(vcx, |p, window, cx| {
            pick(p).focus_handle(cx).is_focused(window)
        });
        assert!(focused, "a click on the {label} field did not focus it");
        vcx.simulate_keystrokes("cmd-a");
        vcx.simulate_input("typed text");
        let got = entity.read_with(vcx, |p, cx| pick(p).read(cx).text().to_owned());
        assert_eq!(
            got, "typed text",
            "typing after a click on the {label} field"
        );
    }
}
