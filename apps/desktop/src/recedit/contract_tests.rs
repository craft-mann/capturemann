//! The editor's requests against the real daemon: what `EditorModel` sends must be accepted by
//! `trim`, `cut_segments`, `restore_segments`, `split`, `set_speed`, `set_background`, `undo` and
//! `redo`, and the answer must round-trip through `adopt`. This is the contract the GUI relies on
//! (the GUI never calls an engine crate). Tested with an in-process daemon and a recording bundle
//! with fake media, no display.

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_core::{Actor, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::Daemon;
use cm_library::rec::{RecBundle, SessionInfo};
use cm_library::Library;
use serde_json::json;

use super::inspector::{BgSpec, Preset};
use super::keys::Cmd;
use super::model::{Basis, EditRequest, EditorModel, Selection};
use super::snap::FrameGrid;

fn secs(s: i64) -> MediaTime {
    MediaTime::from_secs(s)
}

struct Rig {
    _dir: tempfile::TempDir,
    daemon: Daemon,
    id: String,
    model: EditorModel,
    calls: u64,
}

async fn rig() -> Rig {
    rig_with(false).await
}

/// The same with a camera master when `camera` is set (CAM-3).
async fn rig_with(camera: bool) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let daemon = Daemon::builder(
        Capturer::new(FakeBackend::new().with_display(1, 0, 0, 40, 30, 1.0)).with_own_pid(None),
        lib,
    )
    .build();
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Default::default(),
    };
    let now = time::OffsetDateTime::UNIX_EPOCH;
    let b = RecBundle::create(&dir.path().join("lib"), "Lesson", now, json!({}), &session).unwrap();
    let bundle_dir = b.dir().to_path_buf();
    std::fs::write(bundle_dir.join("screen.mp4"), b"master").unwrap();
    let info = TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::ZERO,
        end: secs(60),
        details: json!({"fps": 30.0}),
        stats: TrackStats::default(),
    };
    let mut tracks = vec![info];
    if camera {
        std::fs::write(bundle_dir.join("camera.mp4"), b"camera").unwrap();
        tracks.push(TrackInfo {
            kind: TrackKind::Camera,
            file: "camera.mp4".into(),
            start: MediaTime::ZERO,
            end: secs(60),
            details: json!({"fps": 30.0}),
            stats: TrackStats::default(),
        });
    }
    let m = b.finalize(&tracks, now).unwrap();
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let manifest = cm_library::read_manifest(&bundle_dir).unwrap();
    let basis = Basis {
        end: cm_library::rec_edit::session_end(&manifest),
        live_at: cm_library::rec_edit::live_at(&manifest),
        gaps: manifest.gaps.clone(),
    };
    let list = cm_library::rec_edit::load_edits(&manifest);
    let model = EditorModel::new(
        &m.id,
        basis,
        list,
        FrameGrid::new(MediaTime::ZERO, Some(30.0)),
        cm_library::rec_edit::history_flags(&manifest),
    );
    Rig {
        _dir: dir,
        daemon,
        id: m.id,
        model,
        calls: 0,
    }
}

impl Rig {
    /// Sends the requests like the view does (in order) and adopts each answer.
    async fn send(&mut self, reqs: Vec<EditRequest>) {
        assert!(!reqs.is_empty(), "the command produced no request");
        for mut r in reqs {
            // A fresh key per call, like the view's counter: a repeated key replays the answer.
            self.calls += 1;
            r.params["request_id"] = json!(format!("t-{}", self.calls));
            let v = self
                .daemon
                .execute_method(Actor::User, 0, r.method, &r.params)
                .await
                .unwrap_or_else(|e| panic!("{} was refused: {e}", r.method));
            self.model.adopt(&v);
        }
    }

    async fn cmd(&mut self, c: Cmd) {
        let reqs = self.model.command(c);
        self.send(reqs).await;
    }
}

/// EDT-1.AC1: I at 2 s and O at 58 s of a 60 s recording export 56 s.
#[tokio::test]
async fn edt1_ac1_in_and_out_trim_to_56_seconds() {
    let mut r = rig().await;
    r.model.seek(secs(2));
    r.cmd(Cmd::SetIn).await;
    r.model.seek(secs(58));
    r.cmd(Cmd::SetOut).await;
    assert_eq!((r.model.list.start, r.model.list.end), (secs(2), secs(58)));
    assert_eq!(r.model.output_duration(), secs(56));
    assert!(r.model.can_undo);
}

/// EDT-1.AC2/AC3: cut a selected range, then restore it with the cut selected; each one step.
#[tokio::test]
async fn edt1_ac3_cut_and_restore_round_trip() {
    let mut r = rig().await;
    r.model.select_range(secs(10), secs(15));
    r.cmd(Cmd::Delete).await;
    assert_eq!(r.model.output_duration(), secs(55));
    assert_eq!(r.model.list.cuts.len(), 1);
    r.model.selection = Selection::Cut(0);
    r.cmd(Cmd::Delete).await;
    assert_eq!(r.model.output_duration(), secs(60));
    assert!(r.model.list.cuts.is_empty());
}

/// EDT-1: split, then speed for the selected segment (EDT-2.AC1: 30 s at 2x lasts 15 s).
#[tokio::test]
async fn edt2_ac1_split_then_speed_for_a_segment() {
    let mut r = rig().await;
    r.model.seek(secs(30));
    r.cmd(Cmd::Split).await;
    assert_eq!(r.model.list.splits, vec![secs(30)]);
    r.model.selection = Selection::Segment(1);
    let reqs = r.model.set_speed(2.0, false);
    r.send(reqs).await;
    assert_eq!(r.model.output_duration(), secs(45));
    assert_eq!(r.model.selected_speed(), Some((2.0, false)));
    // EDT-2.AC3: above 2x the audio is muted unless kept.
    let reqs = r.model.set_speed(4.0, false);
    r.send(reqs).await;
    assert_eq!(r.model.selected_speed(), Some((4.0, false)));
    let reqs = r.model.set_speed(4.0, true);
    r.send(reqs).await;
    assert_eq!(r.model.selected_speed(), Some((4.0, true)));
}

/// EDT-3: each preset is accepted and read back as the same preset; None turns it off.
#[tokio::test]
async fn edt3_presets_round_trip_through_the_daemon() {
    let mut r = rig().await;
    for p in [Preset::SoftGray, Preset::LensViolet, Preset::None] {
        let spec = BgSpec::preset(p, &r.model.background());
        let reqs = r.model.set_background(&spec);
        r.send(reqs).await;
        assert_eq!(r.model.background().which(), p, "{p:?}");
    }
    // Custom: a swatch and a tweaked padding.
    let custom = BgSpec::preset(Preset::Custom, &r.model.background()).with_color("#ffd9c7");
    let reqs = r.model.set_background(&custom);
    r.send(reqs).await;
    let bg = r.model.background();
    assert_eq!(bg.which(), Preset::Custom);
    assert_eq!(bg.value, json!("#ffd9c7"));
    let reqs = r
        .model
        .set_background(&bg.with_padding(12.0).with_radius(20));
    r.send(reqs).await;
    let bg = r.model.background();
    assert_eq!((bg.padding_pct, bg.radius_px), (12.0, 20));
}

/// Undo and redo step the same history the actions wrote, and the flags follow.
#[tokio::test]
async fn undo_and_redo_walk_the_daemons_history() {
    let mut r = rig().await;
    r.model.seek(secs(20));
    r.cmd(Cmd::Split).await;
    r.model.seek(secs(40));
    r.cmd(Cmd::Split).await;
    assert_eq!(r.model.list.splits.len(), 2);
    r.cmd(Cmd::Undo).await;
    assert_eq!(r.model.list.splits, vec![secs(20)]);
    assert!(r.model.can_redo);
    r.cmd(Cmd::Redo).await;
    assert_eq!(r.model.list.splits.len(), 2);
    r.cmd(Cmd::Undo).await;
    r.cmd(Cmd::Undo).await;
    assert!(r.model.list.splits.is_empty());
    assert!(!r.model.can_undo, "nothing left to undo");
}

/// The daemon is the source of truth: an edit it refuses leaves the model as it was (the view
/// shows the error; EDT-1: a cut leaving under 0.5 s is "Nothing left to export").
#[tokio::test]
async fn a_refused_edit_leaves_the_model_unchanged() {
    let mut r = rig().await;
    r.model.select_range(MediaTime::ZERO, secs(60));
    let reqs = r.model.command(Cmd::Delete);
    let before = r.model.list.clone();
    let mut req = reqs.into_iter().next().unwrap();
    req.params["request_id"] = json!("t-refused");
    let err = r
        .daemon
        .execute_method(Actor::User, 0, req.method, &req.params)
        .await
        .expect_err("cutting everything is refused");
    assert!(err.to_string().contains("Nothing left"), "{err}");
    assert_eq!(r.model.list, before);
    // The id is the recording's, so the view's request really targets it.
    assert_eq!(req.params["capture_id"], json!(r.id));
}

/// CAM-3: the camera controls' requests are accepted by `set_camera_layout`, each is one undo
/// step, the answer round-trips through `adopt`, and a camera-full range reads back by time.
#[tokio::test]
async fn cam3_camera_layout_requests_round_trip_through_the_daemon() {
    use super::camera::{self, Corner};
    use cm_core::{CameraMode, LayoutTransition};
    let mut r = rig_with(true).await;
    // Split at 15 s; the playhead in the first segment targets it (no selection needed).
    r.model.seek(secs(15));
    r.cmd(Cmd::Split).await;
    r.model.seek(secs(5));
    let first = r.model.layout_target().unwrap();
    assert_eq!(first.to, secs(15));
    let cur = r.model.camera_spec().unwrap();
    assert_eq!(cur.mode, CameraMode::Bubble);

    // Camera full for 0-15 s (CAM-3.AC2), smooth (AC3).
    let next = camera::with_smooth(&camera::with_mode(&cur, CameraMode::Camera), true);
    let reqs = r.model.set_camera_layout(next);
    assert_eq!(reqs[0].method, "set_camera_layout");
    r.send(reqs).await;
    let segs = r.model.list.camera_segments();
    assert_eq!(segs.len(), 1);
    assert_eq!(
        (segs[0].span, segs[0].spec.mode),
        (first, CameraMode::Camera)
    );
    assert_eq!(segs[0].spec.transition, LayoutTransition::Smooth);
    assert_eq!(
        r.model.list.layout_at(secs(5)).current.mode,
        CameraMode::Camera
    );
    assert_eq!(
        r.model.list.layout_at(secs(20)).current.mode,
        CameraMode::Bubble
    );

    // The same layout again is not sent twice.
    let again = camera::with_smooth(&camera::with_mode(&cur, CameraMode::Camera), true);
    assert!(r.model.set_camera_layout(again).is_empty());

    // A bubble in the top left for the second segment (CAM-3.AC1).
    r.model.seek(secs(30));
    let cur2 = r.model.camera_spec().unwrap();
    let base = r.model.camera_base();
    let moved = camera::with_corner(&cur2, base.as_ref(), Corner::TopLeft, 16.0 / 9.0);
    let reqs = r.model.set_camera_layout(moved);
    r.send(reqs).await;
    let segs = r.model.list.camera_segments();
    assert_eq!(segs.len(), 2);
    let rect = segs[1].spec.rect.unwrap();
    assert!(rect[0] < 0.1 && rect[1] < 0.1);

    // AC5: each change is one undo step.
    r.cmd(Cmd::Undo).await;
    assert_eq!(r.model.list.camera_segments().len(), 1);
    r.cmd(Cmd::Undo).await;
    assert!(r.model.list.camera_segments().is_empty());

    // Nowhere to put a layout: no request, and a note.
    r.model.selection = Selection::None;
    r.model.seek(MediaTime::from_secs(70));
    assert!(r
        .model
        .set_camera_layout(camera::with_mode(&cur, CameraMode::Screen))
        .is_empty());
    assert!(r.model.note.is_some());
}

// ---- EDT-4 and EDT-5 ----------------------------------------------------------------------

use super::viewedit::{PreviewAspect, ZoomAdjust};
use cm_core::{Aspect, ClickEffect, ReframeMode, Smoothing};

/// EDT-4.AC2: Z adds a zoom at the playhead, the steppers change it, removing it empties the lane;
/// the daemon accepts every request and the model reads the answer back.
#[tokio::test]
async fn edt4_zoom_requests_round_trip_through_the_daemon() {
    let mut r = rig().await;
    r.model.seek(secs(15));
    r.cmd(Cmd::AddZoom).await;
    assert_eq!(r.model.zooms().len(), 1);
    let z = r.model.selected_zoom().unwrap();
    assert_eq!((z.at, z.length, z.scale), (secs(15), secs(3), 1.8));

    let reqs = r.model.adjust_zoom(ZoomAdjust::Scale(0.2));
    r.send(reqs).await;
    let reqs = r.model.adjust_zoom(ZoomAdjust::Length(0.5));
    r.send(reqs).await;
    let reqs = r.model.adjust_zoom(ZoomAdjust::Focus(0.1, -0.1));
    r.send(reqs).await;
    let reqs = r.model.adjust_zoom(ZoomAdjust::FollowCursor(true));
    r.send(reqs).await;
    let z = r.model.selected_zoom().unwrap();
    assert_eq!(
        r.model.zooms().len(),
        1,
        "an adjustment swaps the zoom, it does not add one"
    );
    assert!((z.scale - 2.0).abs() < 1e-9);
    assert_eq!(z.length.ticks(), 3_500 * 120);
    assert_eq!(z.focus, (0.6, 0.4));
    assert!(z.follow_cursor);

    // A second zoom over the first shortens it (EDT-4 edge case) and Z over a range uses the range.
    r.model.select_range(secs(17), secs(21));
    r.cmd(Cmd::AddZoom).await;
    assert_eq!(r.model.zooms().len(), 2);
    assert_eq!(
        r.model.zooms()[0].length,
        secs(2),
        "the first now ends where the new one starts"
    );

    // Undo walks back one step per request; remove empties the lane.
    r.cmd(Cmd::Undo).await;
    assert_eq!(r.model.zooms().len(), 1);
    r.model.select_zoom(Some(secs(15)));
    let reqs = r.model.remove_zoom();
    r.send(reqs).await;
    assert!(r.model.zooms().is_empty());
    assert_eq!(r.model.zoom_selected, None);
}

/// EDT-4: the cursor card's requests are accepted and read back.
#[tokio::test]
async fn edt4_cursor_style_requests_round_trip_through_the_daemon() {
    let mut r = rig().await;
    let reqs = r.model.nudge_cursor_size(4);
    r.send(reqs).await;
    let reqs = r.model.set_smoothing(Smoothing::Strong);
    r.send(reqs).await;
    let reqs = r.model.set_click_effect(ClickEffect::Ripple);
    r.send(reqs).await;
    let reqs = r.model.set_hide_idle(true);
    r.send(reqs).await;
    let st = r.model.cursor_style();
    assert_eq!(st.size, 2.0);
    assert_eq!(st.smoothing, Smoothing::Strong);
    assert_eq!(st.click_effect, ClickEffect::Ripple);
    assert!(st.hide_idle);
}

/// EDT-5.AC3: a vertical reframe is stored on its own; the square one and the 16:9 edit are
/// untouched, and a nudge in a split segment moves that segment's window only.
#[tokio::test]
async fn edt5_reframe_requests_round_trip_through_the_daemon() {
    let mut r = rig().await;
    r.model.seek(secs(30));
    r.cmd(Cmd::Split).await;
    r.model.set_preview_aspect(PreviewAspect::Vertical);
    r.model.seek(secs(10));
    let reqs = r.model.nudge_reframe(Aspect::Vertical, -0.2, 0.0);
    r.send(reqs).await;
    r.model.seek(secs(45));
    let reqs = r.model.nudge_reframe(Aspect::Vertical, 0.3, 0.0);
    r.send(reqs).await;
    let v = r.model.reframe_of(Aspect::Vertical);
    assert_eq!(v.mode, ReframeMode::Fixed);
    assert_eq!(v.windows.len(), 2, "one window per segment");
    assert_eq!(v.focus_at(secs(10)).0, 0.3);
    // The new segment starts from the previous window (0.3), so +0.3 is 0.6.
    assert_eq!(v.focus_at(secs(45)).0, 0.6);
    assert!(r.model.preview_reframes());
    assert_eq!(r.model.reframe_of(Aspect::Square).mode, ReframeMode::Fit);
    let reqs = r
        .model
        .set_reframe_mode(Aspect::Vertical, ReframeMode::Follow);
    r.send(reqs).await;
    assert_eq!(
        r.model.reframe_of(Aspect::Vertical).mode,
        ReframeMode::Follow
    );
    // The recording's own aspect never reframes.
    r.model.set_preview_aspect(PreviewAspect::Wide);
    assert!(!r.model.preview_reframes());
}

/// AI-3: accepting a zoom suggestion sends a `set_zooms` the daemon takes; it is added to the
/// zooms already there, not substituted for them.
#[tokio::test]
async fn ai3_accepting_zoom_suggestions_adds_zooms() {
    use cm_ai::suggest::ZoomSuggestion;
    let mut r = rig().await;
    let zoom = |a: i64, b: i64, fx: f32| ZoomSuggestion {
        start: secs(a),
        end: secs(b),
        focus: (fx, 0.6),
        scale: 2.0,
        reason: "click".into(),
    };
    let req = crate::ai::suggest::zoom_request(&r.id, &[zoom(10, 14, 0.3)]).unwrap();
    r.send(vec![req]).await;
    let zooms = r.model.list.view.zooms.clone();
    assert_eq!(zooms.len(), 1);
    assert_eq!((zooms[0].at, zooms[0].length), (secs(10), secs(4)));
    assert!((zooms[0].focus.0 - 0.3).abs() < 1e-6);
    // Accept all of a second batch: the first zoom is still there.
    let req =
        crate::ai::suggest::zoom_request(&r.id, &[zoom(30, 33, 0.7), zoom(40, 44, 0.5)]).unwrap();
    r.send(vec![req]).await;
    assert_eq!(r.model.list.view.zooms.len(), 3);
}

/// AI-4: accepting every cut suggestion is one `cut_segments` (overlaps merged), so one undo
/// brings everything back.
#[tokio::test]
async fn ai4_accepting_all_cut_suggestions_is_one_undo_step() {
    use cm_ai::suggest::CutSuggestion;
    let mut r = rig().await;
    let cut = |a: f64, b: f64, kind: &str| CutSuggestion {
        start: MediaTime::from_secs_f64(a).unwrap(),
        end: MediaTime::from_secs_f64(b).unwrap(),
        kind: kind.into(),
        detail: String::new(),
    };
    let req = crate::ai::suggest::cut_request(
        &r.id,
        &[
            cut(10.0, 12.0, "silence"),
            cut(11.5, 13.0, "silence"),
            cut(20.0, 20.4, "filler"),
        ],
    )
    .unwrap();
    r.send(vec![req]).await;
    assert_eq!(r.model.list.cuts.len(), 2, "overlapping suggestions merged");
    assert!((r.model.output_duration().as_secs_f64() - 56.6).abs() < 0.02);
    r.cmd(Cmd::Undo).await;
    assert!(
        r.model.list.cuts.is_empty(),
        "one undo restores all of them"
    );
    assert_eq!(r.model.output_duration(), secs(60));
}

/// The bundle folder of the rig's recording (the one `.cmrec` in its library).
fn bundle_of(r: &Rig) -> std::path::PathBuf {
    std::fs::read_dir(r._dir.path().join("lib"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "cmrec"))
        .expect("the recording's bundle")
}

/// A stored transcript the way the daemon writes it: "Hello there" at 1 to 2.5 s.
fn write_transcript(dir: &std::path::Path) {
    let t = |s: f64| (s * 120_000.0) as i64;
    let words = json!([
        {"text": "Hello", "start": t(1.0), "end": t(1.6), "confidence": 0.9},
        {"text": "there", "start": t(1.7), "end": t(2.5), "confidence": 0.9},
    ]);
    std::fs::write(
        dir.join("transcript.json"),
        json!({"version": 1, "language": "en", "model": "small", "words": words, "note": null})
            .to_string(),
    )
    .unwrap();
}

/// AI-1: the UI client's `set_caption_options` is accepted by the real daemon, the transcript
/// answer reports the switches the client reads, and an undo puts the switch back.
#[tokio::test(flavor = "multi_thread")]
async fn ai1_caption_switches_from_the_ui_client() {
    use crate::ai::client::{AiClient, CaptionOptionsInfo, DaemonTransport};
    let r = rig().await;
    write_transcript(&bundle_of(&r));
    let client = AiClient::new(std::sync::Arc::new(DaemonTransport {
        daemon: r.daemon.clone(),
        rt: tokio::runtime::Handle::current(),
    }));
    let (_, now) = client
        .get_transcript_with_options(&r.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(now, CaptionOptionsInfo::default());
    let after = client
        .set_caption_options(&r.id, Some(true), None)
        .await
        .unwrap();
    assert_eq!(
        after,
        CaptionOptionsInfo {
            enabled: true,
            burn_in: false
        }
    );
    let after = client
        .set_caption_options(&r.id, None, Some(true))
        .await
        .unwrap();
    assert!(
        after.enabled && after.burn_in,
        "each call keeps the other switch"
    );
    let (_, read) = client
        .get_transcript_with_options(&r.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read, after);
    let info = client.transcript_info(&r.id).await.unwrap().unwrap();
    assert_eq!(
        info.options, after,
        "the details page reads the same switches"
    );
    // One undo undoes the last switch only.
    r.daemon
        .execute_method(Actor::User, 0, "undo", &json!({"capture_id": r.id}))
        .await
        .unwrap();
    let (_, read) = client
        .get_transcript_with_options(&r.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        read,
        CaptionOptionsInfo {
            enabled: true,
            burn_in: false
        }
    );
}

/// AI-1: the preview's caption track is built from the real bundle: a transcript gives captions
/// at the right times, a recording without one gives none, and a cut removes the words in it.
#[tokio::test(flavor = "multi_thread")]
async fn ai1_the_preview_track_is_built_from_the_bundle() {
    use super::captions::build_track;
    let mut r = rig().await;
    let dir = bundle_of(&r);
    assert!(
        build_track(&dir, &r.id, &r.model.played()).is_none(),
        "no transcript, no captions"
    );
    write_transcript(&dir);
    let track = build_track(&dir, &r.id, &r.model.played()).expect("a transcript gives captions");
    let at = |track: &cm_media::CaptionTrack, s: f64| {
        track
            .layer_at(MediaTime::from_secs_f64(s).unwrap(), (1280, 720))
            .is_some()
    };
    assert!(at(&track, 1.5), "inside the caption");
    assert!(!at(&track, 10.0), "after it");
    // Cut 0.5 s to 3 s: the words are gone from the preview too.
    r.model.seek(MediaTime::from_secs_f64(0.5).unwrap());
    r.cmd(Cmd::SetIn).await; // trim the start to 0.5 s ...
    let cut = vec![EditRequest {
        method: "cut_segments",
        params: json!({"capture_id": r.id, "ranges": [[1.0, 3.0]]}),
    }];
    r.send(cut).await;
    let track = build_track(&dir, &r.id, &r.model.played()).unwrap();
    assert!(!at(&track, 1.5), "cut section shows no caption");
}
