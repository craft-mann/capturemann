//! The audio editor in the headless GPUI platform (VOC-2): a real `RecordingEditor` over a real
//! daemon and a hand-made audio recording (two tracks, two speakers, a marker, a transcript and a
//! translation). It checks what loads, that the page draws, that M adds a marker through the
//! daemon and that the Export audio sheet opens with the recording's tracks. Nothing here uses the
//! screen, the camera or a microphone.

use std::sync::Arc;

use super::tabs::Inspector;
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
use gpui::{TestAppContext, VisualTestContext};

/// An empty config directory for this test process (never the real keychain or account).
fn isolated_config_dir() {
    static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let dir = DIR.get_or_init(|| tempfile::tempdir().unwrap());
    std::env::set_var("CAPTUREMANN_CONFIG_DIR", dir.path());
}

struct World {
    _rt: tokio::runtime::Runtime,
    _dir: tempfile::TempDir,
    bundle: PathBuf,
}

const SECS: i64 = 6;

/// A mono 16-bit 48 kHz WAV of a quiet tone, `SECS` long.
fn wav() -> Vec<u8> {
    let n = 48_000 * SECS as usize;
    let mut data = Vec::with_capacity(n * 2);
    for i in 0..n {
        let v = ((i as f32 / 48.0).sin() * 8_000.0) as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&96_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// One stored report (meeting minutes) and its entry in the manifest's index.
fn write_report(bundle: &std::path::Path) {
    use cm_ai::report::store;
    let r = store::Report {
        version: 1,
        id: "rpt_00000001".into(),
        v: 1,
        template: "minutes".into(),
        template_name: "Meeting minutes".into(),
        template_version: 1,
        model: "test-model".into(),
        provider: "local".into(),
        created: "2026-10-26T10:51:00Z".into(),
        language: "en".into(),
        context: Default::default(),
        content: serde_json::json!({
            "summary": [{"text": "Pricing goes live after the beta.", "at": [1.0]}],
            "decisions": [{"text": "Launch after the beta", "at": [2.0, 4.5]}],
            "action_items": [
                {"task": "Draft the post", "owner": "Linh", "due": "Thu", "at": [4.0]}
            ]
        }),
        dropped_uncited: 2,
        reassigned_names: 0,
        chunks: 1,
        edited: false,
        markdown: None,
    };
    store::write(bundle, &r).unwrap();
    let path = bundle.join("manifest.json");
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    m["reports"] = store::index_value(&[store::entry_of(&r)]);
    std::fs::write(&path, m.to_string()).unwrap();
}

/// Waits (the daemon runs on its own runtime) until `done` is true, drawing frames meanwhile.
fn wait_for(
    vcx: &mut VisualTestContext,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) -> bool {
    for _ in 0..150 {
        vcx.run_until_parked();
        vcx.update(|window, _| window.refresh());
        if done(vcx) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

fn word(text: &str, a: i64, b: i64) -> serde_json::Value {
    serde_json::json!({"text": text, "start": a * 120, "end": b * 120, "confidence": 0.9})
}

/// The editor opened on a hand-made audio recording.
fn editor(cx: &mut TestAppContext) -> (World, Entity<RecordingEditor>, &mut VisualTestContext) {
    cx.executor().allow_parking();
    isolated_config_dir();
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    let bundle = lib.join("Weekly sync.cmrec");
    std::fs::create_dir_all(bundle.join("translations")).unwrap();
    let tps = 120_000_i64;
    std::fs::write(
        bundle.join("manifest.json"),
        serde_json::json!({
            "schema": 1, "id": "rec_audio", "title": "Weekly sync", "kind": "audio",
            "created": "2026-10-26T10:00:00Z", "tick_rate": tps,
            "tracks": {
                "mic": {"file": "mic.wav", "start": 0, "end": SECS * tps, "channels": 1},
                "system": {"file": "system.wav", "start": 0, "end": SECS * tps, "channels": 1}
            },
            "edits": [], "revisions": [], "gaps": [], "duration": SECS * tps,
            "markers": [{"at": 2 * tps, "label": "Pricing decision"}],
            "speakers": [{"id": 1, "name": "Trung (me)"}, {"id": 2, "name": "Linh"}],
            "speaker_segments": [
                {"start": 0, "end": 3 * tps, "speaker": 1},
                {"start": 3 * tps, "end": SECS * tps, "speaker": 2}
            ],
        })
        .to_string(),
    )
    .unwrap();
    write_report(&bundle);
    std::fs::write(bundle.join("mic.wav"), wav()).unwrap();
    std::fs::write(bundle.join("system.wav"), wav()).unwrap();
    // Words every half second: the first three seconds are Trung's, the rest Linh's.
    let words: Vec<_> = [
        ("Let's", 0, 500),
        ("start", 500, 1_000),
        ("with", 1_000, 1_500),
        ("pricing.", 1_500, 3_000),
        ("Sounds", 3_000, 4_000),
        ("good", 4_000, 5_000),
        ("to", 5_000, 5_500),
        ("me.", 5_500, 6_000),
    ]
    .iter()
    .map(|(t, a, b)| word(t, *a, *b))
    .collect();
    std::fs::write(
        bundle.join("transcript.json"),
        serde_json::json!({"version": 1, "language": "en", "model": "base",
                           "words": words, "note": null})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        bundle.join("translations").join("vi.json"),
        serde_json::json!({
            "version": 1, "language": "vi", "engine": "text_model", "model": "test",
            "created": "2026-10-26T10:05:00Z", "edited": false,
            "lines": [
                {"start": 0, "end": 3 * tps, "text": "Hay bat dau voi gia."},
                {"start": 3 * tps, "end": SECS * tps, "text": "Nghe on do."}
            ]
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
    // The library knows the folder (the editor finds the bundle by id).
    let daemon = backend.daemon.clone().unwrap();
    rt.block_on(async { Backend::rescan_library(&daemon).await.unwrap() });
    let window = cx.add_window(|_, cx| RecordingEditor::new(app, cx));
    let entity = window.root(cx).unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx).into_mut();
    entity.update(vcx, |e, cx| e.set_capture("rec_audio", cx));
    for _ in 0..100 {
        vcx.run_until_parked();
        let ready = entity.read_with(vcx, |e, _| matches!(e.load, Load::Ready(_)));
        let ai = entity.read_with(vcx, |e, _| e.ai.has_transcript);
        if ready && ai {
            break;
        }
        // A frame: the editor reads the transcript while it draws.
        vcx.update(|window, _| window.refresh());
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    vcx.run_until_parked();
    (
        World {
            _rt: rt,
            _dir: dir,
            bundle,
        },
        entity,
        vcx,
    )
}

#[gpui::test]
fn an_audio_recording_opens_with_its_lanes_and_the_transcript_tab(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    e.read_with(vcx, |e, _| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        assert!(s.voice.audio, "the manifest says audio");
        // No screen row: the mic and the system track.
        assert_eq!(
            s.rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
            vec![RowKind::Mic, RowKind::System]
        );
        // One waveform per track when FFmpeg can decode them (CI installs it; a Mac without it
        // shows lanes without waves and says so in the card).
        if cm_media::tools::locate().is_ok() {
            assert_eq!(
                s.waves.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                vec![RowKind::Mic, RowKind::System]
            );
            let peak = s.waves[0].1.peaks.iter().fold(0.0_f32, |m, p| m.max(*p));
            assert!(peak > 0.1, "the tone shows in the waveform: {peak}");
        } else {
            eprintln!("SKIPPED the waveform check: no FFmpeg");
        }
        assert_eq!(s.voice.markers.len(), 1);
        assert_eq!(s.voice.markers[0].label, "Pricing decision");
        assert_eq!(s.voice.speakers.len(), 2);
        assert!(s.voice.lines_for("vi").is_some(), "the translation is read");
        // An audio recording opens on its transcript, in the audio rail.
        assert_eq!(e.tab, Inspector::Transcript);
        assert!(Inspector::set(true).contains(&e.tab));
        assert!(e.ai.has_transcript, "the transcript was read");
    });
}

#[gpui::test]
fn the_transcript_groups_lines_by_speaker_and_puts_the_marker_between(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    e.read_with(vcx, |e, _| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        let (lines, turns, rows) = e.voice_turns(s);
        assert!(!lines.is_empty());
        let speakers: Vec<_> = turns.iter().map(|t| t.speaker).collect();
        assert_eq!(speakers, vec![Some(1), Some(2)], "{lines:?}");
        // The marker at 2 s falls inside the first turn: it goes before the second.
        assert_eq!(
            rows.as_slice(),
            &[
                crate::recedit::voice::Row::Turn(0),
                crate::recedit::voice::Row::Marker(0),
                crate::recedit::voice::Row::Turn(1)
            ]
        );
    });
    // The translation is a language of the switcher and builds its own turns.
    e.update(vcx, |e, cx| {
        e.voice_ui.lang = Some("vi".to_owned());
        cx.notify();
    });
    e.read_with(vcx, |e, _| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        let (lines, turns, _) = e.voice_turns(s);
        assert_eq!(lines.len(), 2);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].speaker, Some(1));
    });
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
}

#[gpui::test]
fn m_sets_a_marker_through_the_daemon(cx: &mut TestAppContext) {
    let (w, e, vcx) = editor(cx);
    e.update(vcx, |e, cx| {
        if let Some(s) = e.session() {
            s.model.seek(MediaTime::from_secs(4));
        }
        e.add_marker_here(cx);
    });
    // The call runs on the daemon's runtime; wait for it to land in the bundle.
    let mut count = 0;
    for _ in 0..100 {
        vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let m = cm_library::read_manifest(&w.bundle).unwrap();
        count = m.markers.len();
        if count == 2 {
            break;
        }
    }
    assert_eq!(count, 2, "add_marker reached the manifest");
    let m = cm_library::read_manifest(&w.bundle).unwrap();
    assert_eq!(m.markers[1].at, MediaTime::from_secs(4));
    // The view reads the bundle again and shows it.
    for _ in 0..100 {
        vcx.run_until_parked();
        vcx.update(|window, _| window.refresh());
        let n = e.read_with(vcx, |e, _| e.session_voice_markers());
        if n == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(e.read_with(vcx, |e, _| e.session_voice_markers()), 2);
}

impl RecordingEditor {
    fn session_voice_markers(&self) -> usize {
        match &self.load {
            Load::Ready(s) => s.voice.markers.len(),
            _ => 0,
        }
    }
}

#[gpui::test]
fn the_export_sheet_opens_with_the_tracks_of_the_recording(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    e.update(vcx, |e, cx| e.open_export_sheet(cx));
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    e.read_with(vcx, |e, _| {
        let sheet = e.voice_ui.sheet.as_ref().expect("the sheet is open");
        assert!(sheet.avail.mic && sheet.avail.system);
        assert_eq!(sheet.form.format, crate::recedit::audio_export::Format::M4a);
        assert!(sheet.form.loudness, "on by default for M4A");
        assert_eq!(
            sheet.form.params("rec_audio")["tracks"],
            serde_json::json!("mix")
        );
    });
    // Esc closes it.
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    // (The key goes to the focused editor; without focus the sheet is closed the same way.)
    e.update(vcx, |e, cx| {
        e.voice_ui.sheet = None;
        cx.notify();
    });
    assert!(e.read_with(vcx, |e, _| e.voice_ui.sheet.is_none()));
}

#[gpui::test]
fn export_audio_only_from_another_page_opens_the_sheet(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    let app = e.read_with(vcx, |e, _| e.app.clone());
    app.update(vcx, |m, _| {
        m.transcribe.export_audio_focus = Some("rec_audio".to_owned());
    });
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    assert!(e.read_with(vcx, |e, _| e.voice_ui.sheet.is_some()));
    // The request is used up: it does not reopen the sheet later.
    vcx.run_until_parked();
    assert!(app.read_with(vcx, |m, _| m.transcribe.export_audio_focus.is_none()));
}

// ---- the Speakers and Report tabs (V5, V6) ---------------------------------------------------

fn open_tab(e: &Entity<RecordingEditor>, vcx: &mut VisualTestContext, tab: Inspector) {
    e.update(vcx, |e, cx| {
        e.tab = tab;
        cx.notify();
    });
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
}

fn speaker_names(bundle: &std::path::Path) -> Vec<String> {
    let m = cm_library::read_manifest(bundle).unwrap();
    cm_ai::speakers::SpeakerState::from_manifest(&m)
        .map(|s| s.speakers.into_iter().map(|x| x.name).collect())
        .unwrap_or_default()
}

#[gpui::test]
fn the_speakers_tab_draws_and_renames_through_set_speakers(cx: &mut TestAppContext) {
    let (w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Speakers);
    e.read_with(vcx, |e, _| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        let shares =
            crate::recedit::voice_panels::talk_shares(&s.voice.speakers, &s.voice.segments);
        assert_eq!(shares, vec![(1, 0.5), (2, 0.5)]);
    });
    // The pencil turns the row into a field holding the name; a name another speaker has is
    // refused without a call.
    e.update(vcx, |e, cx| {
        e.speaker_start_rename(2, cx);
        e.vp.rename.update(cx, |f, cx| f.set_text("trung (me)", cx));
    });
    // (The field's change event is delivered first, as when a person types and then presses Enter.)
    vcx.run_until_parked();
    e.update(vcx, |e, cx| e.speaker_commit_rename(2, cx));
    e.read_with(vcx, |e, _| {
        assert_eq!(
            e.vp.rename_error,
            Some(crate::recedit::voice_panels::NameError::Taken)
        );
        assert_eq!(e.vp.renaming, Some(2));
    });
    assert_eq!(speaker_names(&w.bundle), ["Trung (me)", "Linh"]);
    e.update(vcx, |e, cx| {
        e.vp.rename.update(cx, |f, cx| f.set_text("Minh", cx));
    });
    vcx.run_until_parked();
    e.update(vcx, |e, cx| e.speaker_commit_rename(2, cx));
    assert!(wait_for(vcx, |_| speaker_names(&w.bundle) == ["Trung (me)", "Minh"]));
    // The tab reads the new name from the bundle.
    assert!(wait_for(vcx, |vcx| e.read_with(vcx, |e, _| {
        matches!(&e.load, Load::Ready(s) if s.voice.name_of(2) == "Minh")
    })));
    e.read_with(vcx, |e, _| assert_eq!(e.vp.renaming, None));
}

#[gpui::test]
fn merging_a_speaker_goes_through_set_speakers(cx: &mut TestAppContext) {
    let (w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Speakers);
    e.update(vcx, |e, cx| {
        e.vp.merging = Some((2, None));
        // No target chosen: nothing happens.
        e.speaker_commit_merge(cx);
    });
    assert_eq!(speaker_names(&w.bundle).len(), 2);
    e.update(vcx, |e, cx| {
        e.vp.merging = Some((2, Some(1)));
        e.speaker_commit_merge(cx);
    });
    assert!(wait_for(vcx, |_| speaker_names(&w.bundle) == ["Trung (me)"]));
    e.read_with(vcx, |e, _| assert_eq!(e.vp.merging, None));
}

#[gpui::test]
fn find_speakers_without_a_model_says_not_available_and_fakes_nothing(cx: &mut TestAppContext) {
    let (w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Speakers);
    e.update(vcx, |e, cx| {
        e.vp.people = Some(3);
        e.speakers_find(cx)
    });
    assert!(wait_for(vcx, |vcx| e
        .read_with(vcx, |e, _| e.vp.speaker_note.is_some())));
    e.read_with(vcx, |e, cx| {
        assert_eq!(
            e.vp.speaker_note,
            Some(super::voice_fields::SpeakerNote::NoModel)
        );
        let id = e.id.clone().unwrap();
        assert!(
            e.app
                .read(cx)
                .transcribe
                .voice_job(&id, crate::ai::jobs::VoiceJobKind::Speakers)
                .is_none(),
            "a refused call is not running"
        );
        // The speaker model card of the AI page says there is none to install.
        assert_eq!(
            e.app.read(cx).ai.voice.speakers.card(),
            crate::ai::voice::SpeakerCard::Unavailable
        );
    });
    // Nothing changed in the bundle.
    assert_eq!(speaker_names(&w.bundle), ["Trung (me)", "Linh"]);
    vcx.update(|window, _| window.refresh());
}

#[gpui::test]
fn play_sample_plays_the_speakers_longest_turn_and_stops_at_its_end(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    e.update(vcx, |e, cx| e.speaker_play_sample(2, cx));
    e.read_with(vcx, |e, _| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        assert_eq!(s.model.playhead, MediaTime::from_secs(3));
        assert!(s.model.playing);
        let lp = e.ai.line_play.expect("it stops by itself");
        assert_eq!(lp.end, MediaTime::from_secs(SECS));
    });
    // A speaker with no turn plays nothing.
    e.update(vcx, |e, cx| {
        e.ai.line_play = None;
        e.speaker_play_sample(9, cx)
    });
    e.read_with(vcx, |e, _| assert!(e.ai.line_play.is_none()));
}

#[gpui::test]
fn the_report_tab_shows_the_stored_report_and_its_times_seek(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Report);
    e.read_with(vcx, |e, cx| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        assert_eq!(
            s.voice.reports.len(),
            1,
            "the report is read from the bundle"
        );
        assert_eq!(s.voice.preset, None);
        assert!(
            e.report_view_open(s, cx),
            "a report shows in the wide inspector"
        );
        let sections = crate::recedit::voice_panels::sections(&s.voice.reports[0]);
        let decisions = sections.iter().find(|x| x.id == "decisions").unwrap();
        assert_eq!(decisions.items[0].at, [2.0, 4.5]);
    });
    // A time chip seeks.
    e.update(vcx, |e, cx| {
        e.voice_seek(crate::recedit::voice_panels::at_time(4.5), cx)
    });
    e.read_with(vcx, |e, _| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        assert_eq!(s.model.playhead.as_secs_f64(), 4.5);
    });
    // "New" opens the setup (the narrow inspector); "Back to the report" returns.
    e.update(vcx, |e, cx| {
        e.vp.setup = true;
        cx.notify();
    });
    vcx.update(|window, _| window.refresh());
    e.read_with(vcx, |e, cx| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        assert!(!e.report_view_open(s, cx));
    });
}

#[gpui::test]
fn deleting_a_report_goes_through_delete_report_and_undo_brings_it_back(cx: &mut TestAppContext) {
    let (w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Report);
    e.update(vcx, |e, cx| e.report_delete("rpt_00000001".into(), cx));
    let reports = |b: &std::path::Path| {
        cm_ai::report::store::index_of(&cm_library::read_manifest(b).unwrap()).len()
    };
    assert!(wait_for(vcx, |_| reports(&w.bundle) == 0));
    assert!(wait_for(vcx, |vcx| e.read_with(vcx, |e, _| {
        matches!(&e.load, Load::Ready(s) if s.voice.reports.is_empty())
    })));
    // With no report the tab is the setup form.
    e.read_with(vcx, |e, cx| {
        let Load::Ready(s) = &e.load else {
            panic!("not ready")
        };
        assert!(!e.report_view_open(s, cx));
    });
}

#[gpui::test]
fn writing_a_report_that_the_daemon_refuses_says_why_and_is_not_left_running(
    cx: &mut TestAppContext,
) {
    let (_w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Report);
    e.update(vcx, |e, cx| {
        e.vp.setup = true;
        e.vp.template = Some("summary".into());
        e.report_generate(cx)
    });
    assert!(wait_for(vcx, |vcx| e
        .read_with(vcx, |e, _| e.vp.report_note.is_some())));
    e.read_with(vcx, |e, cx| {
        // The fixture's transcript is far shorter than the 30 words a report needs, so the daemon
        // refuses before it looks at the text model.
        assert!(
            matches!(
                e.vp.report_note,
                Some(super::voice_fields::ReportNote::Failed(_))
            ),
            "{:?}",
            e.vp.report_note
        );
        let id = e.id.clone().unwrap();
        assert!(e
            .app
            .read(cx)
            .transcribe
            .voice_job(&id, crate::ai::jobs::VoiceJobKind::Report)
            .is_none());
    });
}

#[gpui::test]
fn a_report_tab_without_a_text_model_says_so(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Report);
    // The tab asked the daemon for the text model setting; the default provider is None.
    assert!(wait_for(vcx, |vcx| e
        .read_with(vcx, |e, cx| e.text_state(cx)
            == super::report_tab::TextState::Missing)));
    e.update(vcx, |e, cx| {
        e.vp.setup = true;
        cx.notify();
    });
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
}

#[gpui::test]
fn the_translate_panel_blocks_a_language_the_text_model_must_do(cx: &mut TestAppContext) {
    let (_w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Transcript);
    e.update(vcx, |e, cx| {
        e.voice_ui.translate_open = true;
        e.vp.translate_pick = Some("fr");
        cx.notify();
    });
    assert!(wait_for(vcx, |vcx| e
        .read_with(vcx, |e, cx| e.text_state(cx)
            != super::report_tab::TextState::Unknown)));
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    e.read_with(vcx, |e, cx| {
        assert_eq!(e.text_state(cx), super::report_tab::TextState::Missing);
    });
}

/// AI-5: with only the turbo model the Translate panel to English draws the model-missing state
/// (known from the model list, before any click); with Large v3 installed it does not.
#[gpui::test]
fn the_translate_panel_knows_when_whisper_cannot_translate(cx: &mut TestAppContext) {
    use crate::ai::models::WhisperTranslate;
    let (_w, e, vcx) = editor(cx);
    open_tab(&e, vcx, Inspector::Transcript);
    let app = e.read_with(vcx, |e, _| e.app.clone());
    let model = |id: &str, installed: bool, active: bool| cm_ai::model::ModelInfo {
        id: id.into(),
        label: id.into(),
        size_mb: 1000,
        ram_mb: 0,
        hint: String::new(),
        installed,
        active,
        path: None,
    };
    let set = |vcx: &mut VisualTestContext, models: Vec<cm_ai::model::ModelInfo>| {
        let active = models.iter().find(|m| m.active).map(|m| m.id.clone());
        app.update(vcx, |m, _| {
            m.ai.set_list(cm_ai::model::ModelList {
                models,
                active,
                dir: "/m".into(),
            })
        });
    };
    set(
        vcx,
        vec![
            model("large-v3-turbo", true, true),
            model("large", false, false),
        ],
    );
    e.update(vcx, |e, cx| {
        e.voice_ui.translate_open = true;
        e.vp.translate_pick = Some("en");
        cx.notify();
    });
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    assert!(matches!(
        app.read_with(vcx, |m, _| m.ai.whisper_translate()),
        WhisperTranslate::Missing { .. }
    ));
    set(
        vcx,
        vec![
            model("large-v3-turbo", true, true),
            model("large", true, false),
        ],
    );
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    assert_eq!(
        app.read_with(vcx, |m, _| m.ai.whisper_translate()),
        WhisperTranslate::Ready("large".into())
    );
}
