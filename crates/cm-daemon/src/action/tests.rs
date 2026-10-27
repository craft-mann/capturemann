use super::*;
use serde_json::json;
use std::collections::BTreeSet;

fn samples() -> Vec<(&'static str, Value)> {
    vec![
        ("list_targets", json!({})),
        ("capture_screenshot", json!({"target": {"display_id": 1}})),
        (
            "annotate",
            json!({"capture_id": "shot_1", "ops": [{"op":"rect","rect":[1,2,3,4]}]}),
        ),
        ("get_capture", json!({"capture_id": "shot_1"})),
        ("suggest_redactions", json!({"capture_id": "shot_1"})),
        (
            "suggest_redactions",
            json!({"capture_id": "shot_1", "kinds": ["email", "token"]}),
        ),
        ("get_settings", json!({})),
        ("set_settings", json!({"values": {"recording.fps": 60}})),
        ("undo", json!({"capture_id": "shot_1"})),
        ("redo", json!({"capture_id": "shot_1"})),
        ("list_library", json!({})),
        (
            "rename_capture",
            json!({"capture_id": "shot_1", "title": "New", "request_id": "r"}),
        ),
        ("rescan_library", json!({})),
        (
            "trim",
            json!({"capture_id": "rec_1", "start": 2, "end": 58}),
        ),
        (
            "cut_segments",
            json!({"capture_id": "rec_1", "ranges": [[10, 15]]}),
        ),
        (
            "restore_segments",
            json!({"capture_id": "rec_1", "ranges": [[10, 15]]}),
        ),
        ("split", json!({"capture_id": "rec_1", "at": 30})),
        (
            "set_speed",
            json!({"capture_id": "rec_1", "range": [10, 30], "factor": 2}),
        ),
        (
            "set_background",
            json!({"capture_id": "rec_1", "kind": "color", "value": "#112233"}),
        ),
        (
            "set_audio_mix",
            json!({"capture_id": "rec_1", "system_gain_db": -6}),
        ),
        (
            "set_camera_layout",
            json!({"capture_id": "rec_1", "range": [0, 15], "layout": "camera"}),
        ),
        (
            "set_zooms",
            json!({"capture_id": "rec_1", "zooms": [{"at": 15, "length": 3, "focus": [0.5, 0.4]}]}),
        ),
        (
            "set_cursor_style",
            json!({"capture_id": "rec_1", "size": 2, "click_effect": "ripple"}),
        ),
        (
            "set_reframe",
            json!({"capture_id": "rec_1", "aspect": "9:16", "mode": "follow"}),
        ),
        ("freeze_display", json!({"display_id": 1})),
        ("release_frame", json!({"frame_id": "frame_1"})),
        ("window_thumbnail", json!({"window_id": 5, "max_px": 200})),
        ("get_job", json!({"job_id": "job_1"})),
        (
            "list_jobs",
            json!({"state": "running", "type": "export", "limit": 5}),
        ),
        ("cancel_job", json!({"job_id": "job_1"})),
        ("retry_job", json!({"job_id": "job_1"})),
        (
            "export_video",
            json!({"capture_id": "rec_1", "preset": "quick", "request_id": "r"}),
        ),
        (
            "export_audio",
            json!({"capture_id": "rec_1", "format": "wav", "tracks": "mic", "request_id": "r"}),
        ),
        ("start_recording", json!({"target": {"display_id": 1}})),
        ("pause_recording", json!({})),
        ("resume_recording", json!({"recording_id": "rec_1"})),
        ("set_mic_muted", json!({"muted": true, "request_id": "r"})),
        (
            "add_marker",
            json!({"label": "decision", "request_id": "r"}),
        ),
        ("stop_recording", json!({"request_id": "r"})),
        ("get_recording_status", json!({})),
        ("list_devices", json!({})),
        ("discard_recording", json!({})),
        ("restart_recording", json!({})),
        ("skip_countdown", json!({})),
        ("run_fps_selftest", json!({"target": {"display_id": 1}})),
        (
            "publish_video",
            json!({"capture_id": "rec_1", "platform": "youtube"}),
        ),
        ("delete_capture", json!({"capture_id": "rec_1"})),
        (
            "move_library",
            json!({"path": "/Volumes/X/CaptureMann", "mode": "fresh", "request_id": "r"}),
        ),
        ("list_models", json!({})),
        (
            "download_model",
            json!({"model": "small", "request_id": "r"}),
        ),
        (
            "import_model",
            json!({"model": "small", "path": "/tmp/ggml-small.bin"}),
        ),
        ("select_model", json!({"model": "small"})),
        ("remove_model", json!({"model": "small"})),
        (
            "transcribe",
            json!({"capture_id": "rec_1", "language": "en", "model": "small"}),
        ),
        ("get_transcript", json!({"capture_id": "rec_1"})),
        ("get_captions", json!({"capture_id": "rec_1"})),
        (
            "set_captions",
            json!({"capture_id": "rec_1", "lines": [{"start": 1.0, "end": 2.5, "text": "Hi"}]}),
        ),
        (
            "replace_in_captions",
            json!({"capture_id": "rec_1", "replacements": [{"find": "a", "replace": "b"}]}),
        ),
        (
            "export_captions",
            json!({"capture_id": "rec_1", "format": "vtt"}),
        ),
        (
            "translate_captions",
            json!({"capture_id": "rec_1", "target_language": "vi", "engine": "text_model",
                   "request_id": "r"}),
        ),
        ("suggest_zooms", json!({"capture_id": "rec_1"})),
        ("suggest_cuts", json!({"capture_id": "rec_1"})),
        ("diarize", json!({"capture_id": "rec_1", "num_speakers": 2})),
        (
            "set_speakers",
            json!({"capture_id": "rec_1", "rename": {"2": "Linh"}}),
        ),
        (
            "set_caption_options",
            json!({"capture_id": "rec_1", "enabled": true, "burn_in": false}),
        ),
        ("list_destinations", json!({})),
        ("list_publish_accounts", json!({})),
        ("get_text_model", json!({})),
        ("test_text_model", json!({})),
        ("list_report_templates", json!({})),
        (
            "generate_report",
            json!({"capture_id": "rec_1", "template": "minutes", "language": "vi",
                   "context": {"title": "Weekly", "attendees": ["Linh"]},
                   "report_id": "rpt_00000001", "request_id": "r"}),
        ),
        (
            "get_report",
            json!({"capture_id": "rec_1", "report_id": "rpt_00000001"}),
        ),
        (
            "export_report",
            json!({"capture_id": "rec_1", "report_id": "rpt_00000001", "format": "txt"}),
        ),
        (
            "delete_report",
            json!({"capture_id": "rec_1", "report_id": "rpt_00000001"}),
        ),
        (
            "edit_report",
            json!({"capture_id": "rec_1", "report_id": "rpt_00000001", "markdown": "# Mine"}),
        ),
        (
            "check_report_item",
            json!({"capture_id": "rec_1", "report_id": "rpt_00000001",
                   "section": "action_items", "index": 0}),
        ),
        (
            "upload",
            json!({"capture_id": "rec_1", "destination_id": "nas", "what": "both",
                   "folder": "Lessons", "request_id": "r"}),
        ),
    ]
}

/// CAP-1.AC1: `list_targets` stays the same action; `windows: false` selects the
/// displays-only listing the overlay uses to freeze without waiting for window enumeration.
#[test]
fn cap1_ac1_list_targets_windows_flag() {
    for p in [json!({}), json!(null), json!({"windows": true})] {
        assert_eq!(
            Action::parse("list_targets", &p).unwrap(),
            Action::ListTargets
        );
    }
    let a = Action::parse("list_targets", &json!({"windows": false})).unwrap();
    assert_eq!(a, Action::ListDisplays);
    assert_eq!(a.method(), "list_targets");
    assert_eq!(a.spec().name, "list_targets");
    assert!(!a.spec().mutating);
}

#[test]
fn registry_and_enum_agree() {
    let table: BTreeSet<_> = ACTIONS.iter().map(|s| s.name).collect();
    let sampled: BTreeSet<_> = samples().iter().map(|(n, _)| *n).collect();
    assert_eq!(
        table, sampled,
        "every action needs a sample and a table row"
    );
    for (name, params) in samples() {
        let a = Action::parse(name, &params).unwrap();
        assert_eq!(a.method(), name);
        assert_eq!(a.spec().name, name);
    }
}

#[test]
fn agt_2_tier_table() {
    for s in ACTIONS {
        let confirm = matches!(
            s.name,
            "start_recording"
                | "run_fps_selftest"
                | "publish_video"
                | "delete_capture"
                | "move_library"
                | "discard_recording"
                | "restart_recording"
                | "download_model"
                | "remove_model"
        );
        assert_eq!(s.tier == Tier::Confirm, confirm, "{}", s.name);
        assert_ne!(s.tier, Tier::Never, "{} must not be exposed", s.name);
    }
}

#[test]
fn agt_2_ac5_never_tier_has_no_action() {
    for n in NEVER_TIER {
        assert!(spec_for(n).is_none());
        assert_eq!(tier_of(n), Some(Tier::Never));
        assert!(Action::parse(n, &json!({})).is_err());
    }
}

/// CAP-1.AC2: `target: {frame_id, rect}` selects the frame crop; it is still the
/// `capture_screenshot` action, with the same tier and the display-relative physical rect.
#[test]
fn cap1_ac2_frame_target_parses_to_a_crop_of_the_frame() {
    let a = Action::parse(
        "capture_screenshot",
        &json!({"target": {"frame_id": "frame_1", "rect": [2207, 669, 420, 649]},
                "title": "t", "request_id": "r"}),
    )
    .unwrap();
    assert_eq!(a.method(), "capture_screenshot");
    assert_eq!(a.request_id(), Some("r"));
    assert_eq!(a.spec().tier, Tier::Free);
    let Action::CaptureFromFrame(p) = a else {
        panic!()
    };
    assert_eq!(p.frame_id, "frame_1");
    assert_eq!(p.rect, Some([2207, 669, 420, 649]));
    let whole = Action::parse("capture_screenshot", &json!({"target": {"frame_id": "f"}})).unwrap();
    assert!(matches!(whole, Action::CaptureFromFrame(p) if p.rect.is_none()));
    for (params, field) in [
        (
            json!({"target": {"frame_id": "f"}, "delay_s": 1}),
            "delay_s",
        ),
        (
            json!({"target": {"frame_id": "f", "display_id": 1}}),
            "target",
        ),
        (json!({"target": {"frame_id": ""}}), "target.frame_id"),
        (
            json!({"target": {"frame_id": "f", "rect": [0, 0, 0, 1]}}),
            "target.rect",
        ),
    ] {
        let e = Action::parse("capture_screenshot", &params).unwrap_err();
        assert!(e.to_string().starts_with(field), "{params}: {e}");
    }
}

#[test]
fn window_thumbnail_params_are_validated() {
    let parse = |p: Value| Action::parse("window_thumbnail", &p);
    assert!(matches!(
        parse(json!({"window_id": 7})).unwrap(),
        Action::WindowThumbnail(WindowThumbnailParams {
            window_id: 7,
            max_px: 360
        })
    ));
    // max_px is clamped to 64..=1024, not refused.
    for (given, want) in [(10, 64), (64, 64), (500, 500), (1024, 1024), (99_999, 1024)] {
        match parse(json!({"window_id": 1, "max_px": given})).unwrap() {
            Action::WindowThumbnail(p) => assert_eq!(p.max_px, want, "{given}"),
            other => panic!("{other:?}"),
        }
    }
    for (p, field) in [
        (json!({}), "window_id"),
        (json!({"window_id": -1}), "window_id"),
        (json!({"window_id": "7"}), "window_id"),
        (json!({"window_id": 4_294_967_296u64}), "window_id"),
        (json!({"window_id": 1, "max_px": "big"}), "max_px"),
        (json!({"window_id": 1, "max_px": -5}), "max_px"),
        (json!({"window_id": 1, "max_px": 1.5}), "max_px"),
        (json!({"window_id": 1, "request_id": "r"}), "request_id"),
    ] {
        let e = parse(p.clone()).unwrap_err();
        assert!(e.to_string().starts_with(field), "{p}: {e}");
    }
    assert_eq!(
        spec_for("window_thumbnail").map(|s| (s.tier, s.mutating)),
        Some((Tier::Free, true))
    );
    assert!(is_internal("window_thumbnail"));
}

#[test]
fn freeze_and_release_params_are_validated() {
    let a = Action::parse(
        "freeze_display",
        &json!({"display_id": 2, "transport": "inline"}),
    )
    .unwrap();
    assert!(matches!(
        a,
        Action::FreezeDisplay(FreezeDisplayParams {
            display_id: 2,
            transport: FrameTransport::Inline
        })
    ));
    for (m, p, field) in [
        ("freeze_display", json!({}), "display_id"),
        (
            "freeze_display",
            json!({"display_id": 1, "transport": "x"}),
            "transport",
        ),
        (
            "freeze_display",
            json!({"display_id": 1, "request_id": "r"}),
            "request_id",
        ),
        ("release_frame", json!({}), "frame_id"),
    ] {
        let e = Action::parse(m, &p).unwrap_err();
        assert!(e.to_string().starts_with(field), "{m} {p}: {e}");
    }
}

/// AGT-1.AC4: the internal list is explicit, short, and every entry is a real action.
#[test]
fn agt_1_ac4_internal_actions_are_explicit() {
    for n in INTERNAL_ACTIONS {
        assert!(spec_for(n).is_some(), "{n}");
        assert!(is_internal(n));
    }
    assert!(!is_internal("capture_screenshot"));
    // The recording actions the spec exposes are public; only the HUD's are internal.
    for public in [
        "start_recording",
        "pause_recording",
        "resume_recording",
        "stop_recording",
        "get_recording_status",
        "list_devices",
    ] {
        assert!(!is_internal(public), "{public}");
    }
    assert_eq!(INTERNAL_ACTIONS.len(), 7);
}

#[test]
fn region_rect_is_physical_xywh() {
    let a = Action::parse(
        "capture_screenshot",
        &json!({"target": {"display_id": 1, "rect": [2207, 669, 420, 649]}, "delay_s": 2}),
    )
    .unwrap();
    let Action::CaptureScreenshot(p) = a else {
        panic!()
    };
    assert_eq!(
        p.target,
        ScreenshotTarget::Region {
            display_id: 1,
            rect: [2207, 669, 420, 649]
        }
    );
    assert_eq!(p.delay_s, Some(2.0));
}

#[test]
fn bad_inputs_name_the_field() {
    let cases = [
        ("capture_screenshot", json!({}), "target"),
        (
            "capture_screenshot",
            json!({"target": {"display_id": 1, "window_id": 2}}),
            "target",
        ),
        (
            "capture_screenshot",
            json!({"target": {"display_id": 1, "rect": [0, 0, 0, 5]}}),
            "target.rect",
        ),
        (
            "capture_screenshot",
            json!({"target": {"display_id": 1, "rect": [0.5, 0, 5, 5]}}),
            "target.rect",
        ),
        (
            "capture_screenshot",
            json!({"target": {"display_id": 1}, "bogus": 1}),
            "bogus",
        ),
        ("annotate", json!({"capture_id": "x", "ops": []}), "ops"),
        (
            "annotate",
            json!({"capture_id": "x", "ops": [{"op": "zig"}]}),
            "ops[0]",
        ),
        ("undo", json!({}), "capture_id"),
        ("suggest_redactions", json!({}), "capture_id"),
        (
            "suggest_redactions",
            json!({"capture_id": "x", "kinds": ["face"]}),
            "kinds[0]",
        ),
        (
            "suggest_redactions",
            json!({"capture_id": "x", "kinds": "email"}),
            "kinds",
        ),
        (
            "suggest_redactions",
            json!({"capture_id": "x", "request_id": "r"}),
            "request_id",
        ),
        (
            "undo",
            json!({"capture_id": "x", "request_id": ""}),
            "request_id",
        ),
        ("list_library", json!({"limit": 0}), "limit"),
        ("set_settings", json!({"values": {}}), "values"),
        ("delete_capture", json!({}), "capture_id"),
        (
            "delete_capture",
            json!({"capture_id": "x", "also_delete_remote": "yes"}),
            "also_delete_remote",
        ),
        ("rename_capture", json!({"capture_id": "x"}), "title"),
        (
            "rename_capture",
            json!({"capture_id": "x", "title": " "}),
            "title",
        ),
        ("rescan_library", json!({"x": 1}), "x"),
        ("list_destinations", json!({"x": 1}), "x"),
        ("list_publish_accounts", json!({"x": 1}), "x"),
        ("upload", json!({}), "capture_id"),
        (
            "upload",
            json!({"capture_id": "a", "file": "b"}),
            "capture_id",
        ),
        ("upload", json!({"capture_id": "a", "what": "all"}), "what"),
        ("upload", json!({"file": "b", "what": "export"}), "what"),
        (
            "upload",
            json!({"capture_id": "a", "folder": " "}),
            "folder",
        ),
        ("upload", json!({"capture_id": "a", "path": "/x"}), "path"),
        ("get_job", json!({}), "job_id"),
        (
            "cancel_job",
            json!({"job_id": "j", "request_id": ""}),
            "request_id",
        ),
        ("retry_job", json!({}), "job_id"),
        ("retry_job", json!({"job_id": "j", "force": true}), "force"),
        ("list_jobs", json!({"state": "stuck"}), "state"),
        ("list_jobs", json!({"limit": 0}), "limit"),
        ("list_jobs", json!({"checkpoint": true}), "checkpoint"),
        ("list_targets", json!({"x": 1}), "x"),
        ("list_targets", json!({"windows": "no"}), "windows"),
        ("export_video", json!({}), "capture_id"),
        (
            "export_video",
            json!({"capture_id": "r", "preset": "YouTube 1080p"}),
            "preset",
        ),
        (
            "export_video",
            json!({"capture_id": "r", "size": "1080p"}),
            "size",
        ),
        (
            "export_video",
            json!({"capture_id": "r", "bogus": 1}),
            "bogus",
        ),
        (
            "export_video",
            json!({"capture_id": "r", "request_id": ""}),
            "request_id",
        ),
    ];
    for (m, p, field) in cases {
        let e = Action::parse(m, &p).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
        assert!(e.to_string().starts_with(field), "{m} {p}: {e}");
    }
    assert!(Action::parse("undo", &json!([1])).is_err());
}

/// LIB-1: `status` takes the four filter values; anything else is `invalid_input`.
#[test]
fn lib_1_list_library_status_values() {
    for s in ["uploaded", "not_uploaded", "published", "not_published"] {
        assert!(
            Action::parse("list_library", &json!({"status": s})).is_ok(),
            "{s}"
        );
    }
    for bad in [json!("all"), json!(3), json!("")] {
        let e = Action::parse("list_library", &json!({"status": bad})).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput, "{bad}");
    }
    assert!(Action::parse("list_library", &json!({"status": null})).is_ok());
}

/// R1-1: `annotate` takes `copy_to_clipboard`.
#[test]
fn r1_1_annotate_accepts_copy_to_clipboard() {
    let a = Action::parse(
        "annotate",
        &json!({"capture_id": "x", "ops": [{"op": "rect", "rect": [20, 20, 50, 30], "color": "#2E8BFF", "width": 3}], "copy_to_clipboard": true}),
    );
    match a {
        Ok(Action::Annotate(p)) => assert!(p.copy_to_clipboard),
        other => panic!("{other:?}"),
    }
    let e = Action::parse(
        "annotate",
        &json!({"capture_id": "x", "ops": [], "replace": true, "copy_to_clipboard": "yes"}),
    )
    .unwrap_err();
    assert!(e.to_string().starts_with("copy_to_clipboard"), "{e}");
}

/// REC-4 / REC-5 / AGT-1: the recording actions parse `recording_id` and `request_id` only,
/// `list_devices` takes nothing, and the typed `start_recording` input is validated at parse
/// time (before any dialog).
#[test]
fn recording_actions_parse_and_validate() {
    for m in [
        "pause_recording",
        "resume_recording",
        "stop_recording",
        "discard_recording",
        "restart_recording",
        "skip_countdown",
    ] {
        let a = Action::parse(m, &json!({"recording_id": "rec_1", "request_id": "r"})).unwrap();
        assert_eq!(a.method(), m);
        assert_eq!(a.request_id(), Some("r"));
        assert!(a.spec().mutating, "{m}");
        for (p, field) in [
            (json!({"recording_id": ""}), "recording_id"),
            (json!({"recording_id": 5}), "recording_id"),
            (json!({"x": 1}), "x"),
        ] {
            let e = Action::parse(m, &p).unwrap_err();
            assert!(e.to_string().starts_with(field), "{m} {p}: {e}");
        }
    }
    let s = Action::parse("get_recording_status", &json!({"recording_id": "rec_1"})).unwrap();
    assert!(!s.spec().mutating);
    assert!(Action::parse("get_recording_status", &json!({"request_id": "r"})).is_err());
    assert_eq!(
        Action::parse("list_devices", &json!({})).unwrap(),
        Action::ListDevices
    );
    assert!(Action::parse("list_devices", &json!({"x": 1})).is_err());

    // start_recording: typed validation, field-named errors, still pass-through payload.
    let a = Action::parse(
        "start_recording",
        &json!({"target": {"display_id": 1}, "fps": 60, "request_id": "r",
                "camera": {"shape": "circle"}, "mic": "default"}),
    )
    .unwrap();
    assert_eq!(a.request_id(), Some("r"));
    assert_eq!(a.spec().tier, Tier::Confirm);
    let Action::StartRecording(p) = &a else {
        panic!()
    };
    assert_eq!(p.rest["fps"], 60);
    for (p, field) in [
        (json!({}), "target"),
        (json!({"target": {"display_id": 1}, "fps": 50}), "fps"),
        (
            json!({"target": {"display_id": 1}, "capture_id": "x"}),
            "capture_id",
        ),
        (json!({"target": {"frame_id": "f"}}), "target"),
    ] {
        let e = Action::parse("start_recording", &p).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
        assert!(e.to_string().starts_with(field), "{p}: {e}");
    }
}

/// EXP-1 / EXP-4: `export_video` defaults to the `quick` preset, is Free and mutating, and
/// refuses presets and options that do not exist yet by name.
#[test]
fn export_video_parses_the_quick_preset_only() {
    let a = Action::parse("export_video", &json!({"capture_id": "rec_1"})).unwrap();
    let Action::ExportVideo(p) = &a else { panic!() };
    assert_eq!(
        (p.capture_id.as_str(), p.preset.as_str()),
        ("rec_1", "quick")
    );
    assert_eq!(a.spec().tier, Tier::Free);
    assert!(a.spec().mutating);
    assert_eq!(a.capture_id(), Some("rec_1"));
    assert!(!is_internal("export_video"));
    // EXP-2 / EXP-3: any well-formed preset id parses (the table decides if it exists), and
    // `format` is the same as the gif and webm presets.
    let Action::ExportVideo(p) = Action::parse(
        "export_video",
        &json!({"capture_id": "r", "preset": "youtube-4k"}),
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(p.preset, "youtube-4k");
    let Action::ExportVideo(p) = Action::parse(
        "export_video",
        &json!({"capture_id": "r", "format": "gif", "width": 640, "fps": 10, "max_mb": 10}),
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(
        (p.preset.as_str(), p.width, p.fps, p.max_mb),
        ("gif", Some(640), Some(10.0), Some(10.0))
    );
    for bad in [
        json!({"capture_id": "r", "preset": "YouTube 4K"}),
        json!({"capture_id": "r", "format": "avi"}),
        json!({"capture_id": "r", "format": "gif", "preset": "webm"}),
        json!({"capture_id": "r", "format": "mp4", "preset": "gif"}),
        json!({"capture_id": "r", "width": 15}),
        json!({"capture_id": "r", "width": 640.5}),
        json!({"capture_id": "r", "fps": 61}),
        json!({"capture_id": "r", "max_mb": -1}),
        json!({"capture_id": "r", "codec": "hevc"}),
    ] {
        let e = Action::parse("export_video", &bad).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput, "{bad}");
    }
    assert!(Action::parse("export_video", &json!({"capture_id": "r", "size": null})).is_ok());
    // AUD-4: the loudness target parses and is range checked.
    let a = Action::parse(
        "export_video",
        &json!({"capture_id": "r", "loudness_lufs": -14}),
    )
    .unwrap();
    let Action::ExportVideo(p) = &a else { panic!() };
    assert_eq!(p.loudness_lufs, Some(-14.0));
    for bad in [json!(-3), json!(-41), json!("x"), json!(true)] {
        let e = Action::parse(
            "export_video",
            &json!({"capture_id": "r", "loudness_lufs": bad}),
        )
        .unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
    }
}

/// REC-5: Discard and Restart are Confirm tier (for agents) and internal; their summaries say
/// what they do.
#[test]
fn rec5_discard_and_restart_are_internal_confirm_actions() {
    for m in ["discard_recording", "restart_recording"] {
        assert_eq!(spec_for(m).unwrap().tier, Tier::Confirm);
        assert!(is_internal(m));
    }
    let d = Action::parse("discard_recording", &json!({})).unwrap();
    assert!(d.summary().contains("delete its files"));
    let r = Action::parse("restart_recording", &json!({})).unwrap();
    assert!(r.summary().contains("start over"));
}

#[test]
fn external_params_pass_through() {
    let a = Action::parse(
        "publish_video",
        &json!({"capture_id":"rec_1","platform":"youtube","privacy":"unlisted","request_id":"r"}),
    )
    .unwrap();
    assert_eq!(a.request_id(), Some("r"));
    assert_eq!(a.capture_id(), Some("rec_1"));
    assert_eq!(a.summary(), "Publish capture rec_1 to youtube");
    assert_eq!(
        a.details(),
        json!({"capture_id":"rec_1","rest":{"platform":"youtube","privacy":"unlisted"}})
    );
}
