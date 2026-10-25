use super::*;
use cm_core::message::{render, TEMPLATES};
use cm_i18n::{with_language, Lang};
use serde_json::json;

/// A request as the daemon sent it in its tests (copied from the real output): the English
/// `summary` and `details`, the `kind` and the typed `params`.
struct Sample {
    kind: &'static str,
    summary: String,
    details: Value,
    params: Value,
    /// Rows that must be worded from the parameters (the others are shown as sent).
    worded: &'static [&'static str],
}

const SAVE: &str = "/Users/me/Movies/CaptureMann";

fn start_params(restart: bool) -> Value {
    json!({
        "restart": restart,
        "target": {"type": "display", "id": 1, "name": "Fake display 1", "width": 400, "height": 300},
        "width": 400, "height": 300, "fps": 30, "codec": "H.264", "mb_per_minute": 7.8125025,
        "microphone": null, "camera": null, "system_audio": false, "cursor": true,
        "save_to": SAVE, "title": null, "max_duration_s": null, "countdown_s": 0,
        "disk": {"free_gb": 500.0, "minutes_left": 36994.21109960437},
    })
}

fn start_details() -> Value {
    json!({
        "target": "Display 1 \"Fake display 1\", 400 x 300 px",
        "output": "400 x 300 px, 30 fps, H.264, about 8 MB per minute (an estimate: the real size varies with what is on screen)",
        "screen": "On (always recorded)",
        "microphone": "Off", "camera": "Off", "system_audio": "Off",
        "cursor": "Position, clicks and scroll direction (never keystrokes)",
        "save_to": SAVE, "max_duration": "No limit", "countdown": "None",
        "excluded_windows": "CaptureMann's own windows and your password managers never appear",
        "disk_space": "500.0 GB free, room for about 617 h at this setting (an estimate)",
    })
}

const START_WORDED: &[&str] = &[
    "target",
    "output",
    "screen",
    "microphone",
    "camera",
    "system_audio",
    "cursor",
    "excluded_windows",
    "disk_space",
];

fn samples() -> Vec<Sample> {
    let mut restart_details = start_details();
    restart_details["countdown"] = json!("3 s");
    restart_details["discarded"] = json!("The current take is deleted and cannot be recovered");
    let mut restart_params = start_params(true);
    restart_params["countdown_s"] = json!(3);
    // Every sensor on, a window target, a title, a limit.
    let mut full_params = start_params(false);
    full_params["target"] = json!({"type": "window", "id": 7, "title": "Docs", "app": "Safari", "width": 1200, "height": 800});
    full_params["microphone"] = json!({"device": "Studio Mic", "denoise": true});
    full_params["camera"] = json!({"device": null, "resolution": "720p"});
    full_params["system_audio"] = json!(true);
    full_params["title"] = json!("Weekly");
    full_params["max_duration_s"] = json!(90.0);
    full_params["countdown_s"] = json!(5);
    let mut full_details = start_details();
    full_details["output"] = json!("1200 x 800 px, 30 fps, H.264, about 8 MB per minute (an estimate: the real size varies with what is on screen)");
    full_details["target"] = json!("Window \"Docs\" of Safari, 1200 x 800 px (follows the window)");
    full_details["microphone"] = json!("On: Studio Mic, noise suppression on");
    full_details["camera"] = json!("On: the default camera, 720p (the camera light turns on)");
    full_details["system_audio"] = json!("On");
    full_details["save_to"] = json!(format!("{SAVE} as \"Weekly\""));
    full_details["max_duration"] = json!("1 min 30 s");
    full_details["countdown"] = json!("5 s");
    vec![
        Sample {
            kind: "start_recording",
            summary: format!("Record display 1 at 400 x 300, 30 fps, and save it to {SAVE}"),
            details: start_details(),
            params: start_params(false),
            // "No limit" and "None" are messages; a number of seconds is not.
            worded: &[
                "target", "output", "screen", "microphone", "camera", "system_audio", "cursor",
                "max_duration", "countdown", "excluded_windows", "disk_space",
            ],
        },
        Sample {
            kind: "start_recording",
            summary: format!(
                "Record the window \"Docs\" at 1200 x 800, 30 fps with the microphone, the camera and system audio, and save it to {SAVE}"
            ),
            details: full_details,
            params: {
                let mut p = full_params;
                p["width"] = json!(1200);
                p["height"] = json!(800);
                p
            },
            worded: START_WORDED,
        },
        Sample {
            kind: "restart_recording",
            summary: format!(
                "Discard the current take and start over: record display 1 at 400 x 300, 30 fps, and save it to {SAVE}"
            ),
            details: restart_details,
            params: restart_params,
            worded: START_WORDED,
        },
        Sample {
            kind: "run_fps_selftest",
            summary: "Run the 60 fps self-test: record Display 1 \"Fake display 1\" at 400 x 300, 60 fps, for 10 s to check that this machine keeps up. Nothing is saved: the test video is deleted when the test ends".into(),
            details: json!({
                "target": "Display 1 \"Fake display 1\", 400 x 300 px",
                "output": "400 x 300 px, 60 fps, H.264, about 12 MB per minute (an estimate: the real size varies with what is on screen)",
                "screen": "On (always recorded)", "microphone": "Off", "camera": "Off",
                "system_audio": "Off", "cursor": "Off",
                "save_to": "Nothing is saved: the test video is deleted when the test ends",
                "max_duration": "10 s",
                "excluded_windows": "CaptureMann's own windows and your password managers never appear",
                "purpose": "Measures late and dropped frames at 60 fps; only the result (pass or fail) is kept",
            }),
            params: {
                let mut p = start_params(false);
                p["fps"] = json!(60);
                p["mb_per_minute"] = json!(11.71875);
                p["cursor"] = json!(false);
                p["duration_s"] = json!(10.0);
                p
            },
            worded: &[
                "target", "output", "screen", "microphone", "camera", "system_audio", "cursor",
                "save_to", "excluded_windows", "purpose",
            ],
        },
        Sample {
            kind: "discard_recording",
            summary: "Discard the current recording and delete its files".into(),
            details: json!({
                "recording": "rec_01", "title": "Recording 2026-10-23 14.48", "recorded": "00:42",
                "this_cannot_be_undone": "The take is deleted and cannot be recovered",
            }),
            params: json!({"recording": "rec_01", "title": "Recording 2026-10-23 14.48", "recorded_ms": 42000}),
            worded: &["this_cannot_be_undone"],
        },
        Sample {
            kind: "publish_video",
            summary: "Publish \"Probes in 5 minutes\" to YouTube as Private".into(),
            details: json!({
                "capture": "Take", "platform": "youtube", "account": "Teacher Channel",
                "title": "Probes in 5 minutes", "privacy": "private", "made_for_kids": false,
                "file": "Take - youtube-1080p.mp4", "size": "716.8 KB",
                "description": "00:00 Intro", "tags": "k8s, tutorial",
                "sends": "Uploads this video over the internet to YouTube, channel \"Teacher Channel\"",
                "uploads_today": "0 of 100 used today",
            }),
            params: json!({
                "title": "Probes in 5 minutes", "platform": "youtube", "platform_name": "YouTube",
                "account": "Teacher Channel", "privacy": "private", "privacy_defaulted": false,
                "preset": null, "size_bytes": 716_800, "captions": false,
                "uploads_used": 0, "uploads_limit": 100, "kept_private_note": false,
            }),
            worded: &["privacy", "made_for_kids", "sends", "uploads_today"],
        },
        Sample {
            kind: "publish_video",
            summary: "Publish \"Hi\" to YouTube as Unlisted".into(),
            details: json!({
                "capture": "Take", "platform": "youtube", "account": "Teacher Channel",
                "title": "Hi", "privacy": "unlisted",
                "privacy_source": "not given: the account's default", "made_for_kids": true,
                "file": "Exported first with the `youtube-1080p` preset",
                "captions": "Uploads the captions as an SRT track",
                "sends": "Uploads this video over the internet to YouTube, channel \"Teacher Channel\"",
                "uploads_today": "3 of 100 used today",
                "note": "YouTube keeps videos from an unverified API project private; the result says so if it happens",
            }),
            params: json!({
                "title": "Hi", "platform": "youtube", "platform_name": "YouTube",
                "account": "Teacher Channel", "privacy": "unlisted", "privacy_defaulted": true,
                "preset": "youtube-1080p", "size_bytes": null, "captions": true,
                "uploads_used": 3, "uploads_limit": 100, "kept_private_note": true,
            }),
            worded: &[
                "privacy", "privacy_source", "made_for_kids", "file", "captions", "sends",
                "uploads_today", "note",
            ],
        },
        Sample {
            kind: "delete_capture",
            summary: "Delete \"Quarterly numbers\" (13.2 KB)".into(),
            details: json!({
                "capture": "Quarterly numbers", "id": "shot_1", "kind": "screenshot",
                "created": "2026-10-23T14:49:18.337402Z", "size": "13.2 KB",
                "result": "Moved to the Trash",
            }),
            params: json!({"title": "Quarterly numbers", "id": "shot_1", "kind": "screenshot", "size_bytes": 13212, "remote_copies": 0}),
            worded: &["result"],
        },
        Sample {
            kind: "delete_capture",
            summary: "Delete \"Take\" (2.4 MB) and permanently delete 2 remote copies".into(),
            details: json!({
                "capture": "Take", "id": "rec_1", "kind": "recording",
                "created": "2026-10-23T14:49:18.337402Z", "size": "2.4 MB",
                "result": "The remote copies are deleted, then the capture is moved to the Trash",
            }),
            params: json!({"title": "Take", "id": "rec_1", "kind": "recording", "size_bytes": 2_400_000, "remote_copies": 2}),
            worded: &["result"],
        },
        Sample {
            kind: "delete_capture",
            summary: "Delete \"Take\" (2.4 MB) and permanently delete 1 remote copy".into(),
            details: json!({"size": "2.4 MB"}),
            params: json!({"title": "Take", "id": "rec_1", "kind": "recording", "size_bytes": 2_400_000, "remote_copies": 1}),
            worded: &[],
        },
        Sample {
            kind: "delete_capture",
            summary: "Delete \"Take\" (2.4 MB) and permanently delete 1 remote copy".into(),
            details: json!({
                "capture": "Take", "id": "rec_1", "kind": "recording", "size": "2.4 MB",
                "remote_delete_1": "Google Drive: the file a.mp4 (Google Drive (me), destination \"Drive\")",
                "remote_gone_1": "Folder: the file /nas/a.mp4 (already gone, nothing to delete)",
                "remote_left_1": "Left in place: export b.mp4 at \"Old disk\" (that destination was removed from Settings)",
                "permanent": "Deleting remote copies is permanent and cannot be undone: Google Drive files skip the Drive Trash and a deleted YouTube video cannot be restored",
                "order": "The remote copies are deleted first; if one fails the capture stays here and the result says what was and was not deleted",
                "result": "The remote copies are deleted, then the capture is moved to the Trash",
            }),
            params: json!({
                "title": "Take", "id": "rec_1", "kind": "recording", "size_bytes": 2_400_000,
                "remote_copies": 1,
                "remote_items": [
                    { "state": "delete", "n": 1, "form": "drive_file", "path": "a.mp4",
                      "place": { "type": "service", "service": "Google Drive", "account": "me", "name": "Drive" } },
                    { "state": "gone", "n": 1, "form": "local_file", "path": "/nas/a.mp4" },
                    { "state": "left", "n": 1, "form": "copy", "what": "export", "path": "b.mp4",
                      "destination_name": "Old disk", "reason": { "id": "destination_removed" } },
                ],
            }),
            worded: &[
                "remote_delete_1", "remote_gone_1", "remote_left_1", "permanent", "order", "result",
            ],
        },
        Sample {
            kind: "delete_capture",
            summary: "Delete \"Take\" (2.4 MB)".into(),
            details: json!({
                "capture": "Take", "size": "2.4 MB",
                "remote": "No remote copy is recorded for this capture, so nothing is deleted remotely",
                "result": "Moved to the Trash",
            }),
            params: json!({"title": "Take", "id": "rec_1", "kind": "recording", "size_bytes": 2_400_000, "remote_copies": 0, "remote_items": []}),
            worded: &["remote", "result"],
        },
        Sample {
            kind: "download_model",
            summary: "Download the speech model \"Small\" (about 466 MB) from the internet".into(),
            details: json!({
                "model": "small", "name": "Small", "size_mb": 466, "destination": "/models",
                "network": "Downloads the model from the internet",
            }),
            params: json!({"model": "small", "name": "Small", "size_mb": 466, "destination": "/models"}),
            worded: &["network"],
        },
        Sample {
            kind: "remove_model",
            summary: "Delete the speech model \"Small\" (466 MB) from this computer".into(),
            details: json!({"model": "small", "name": "Small", "size_mb": 466, "file": "/models/small.bin"}),
            params: json!({"model": "small", "name": "Small", "size_mb": 466, "file": "/models/small.bin"}),
            worded: &[],
        },
        Sample {
            kind: "publish_video",
            summary: "Publish capture rec_1 to youtube".into(),
            details: json!({"capture_id": "rec_1", "platform": "youtube"}),
            params: json!({"capture_id": "rec_1", "platform": "youtube"}),
            worded: &[],
        },
        Sample {
            kind: "delete_capture",
            summary: "Delete capture rec_1".into(),
            details: json!({"capture_id": "rec_1"}),
            params: json!({"capture_id": "rec_1"}),
            worded: &[],
        },
    ]
}

/// The English a row shows with no wording of its own.
fn plain_english(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(if *b { "Yes" } else { "No" }.to_owned()),
        other => Some(other.to_string()),
    }
}

/// The English dialog text is what the daemon sent, byte for byte, for every kind that asks.
#[test]
fn english_is_byte_identical_to_the_daemons_text() {
    for s in samples() {
        let built = confirm_summary(s.kind, &s.params, &s.summary);
        assert_eq!(built, s.summary, "{} summary", s.kind);
        let rows = confirm_rows(s.kind, &s.details, &s.params);
        let sent: Vec<(String, String)> = s
            .details
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(k, v)| Some((humanize_key(k), plain_english(v)?)))
            .collect();
        assert_eq!(rows, sent, "{} rows", s.kind);
    }
}

/// The sentence and the named rows really come from the parameters: in the pseudo-locale
/// (every wrapped message accented) they differ from the English; the other rows stay as sent.
#[test]
fn summaries_and_rows_are_worded_from_the_parameters() {
    with_language(Lang::EnXa, || {
        for s in samples() {
            let built = confirm_summary(s.kind, &s.params, &s.summary);
            let has_plan = s.params.get("target").is_some()
                || s.params.get("title").is_some()
                || s.params.get("name").is_some()
                || s.kind == "discard_recording";
            if has_plan {
                assert_ne!(built, s.summary, "{} summary is not worded", s.kind);
            }
            let rows = confirm_rows(s.kind, &s.details, &s.params);
            for (key, (label, value)) in s.details.as_object().unwrap().keys().zip(&rows) {
                assert_ne!(label, &humanize_key(key), "{} label {key}", s.kind);
                let english = plain_english(&s.details[key]).unwrap();
                if s.worded.contains(&key.as_str()) {
                    assert_ne!(value, &english, "{} row {key}", s.kind);
                }
            }
        }
    });
}

#[test]
fn unknown_kinds_and_missing_parameters_fall_back_to_the_daemons_english() {
    let english = "Do the new thing with \"x\"";
    assert_eq!(
        confirm_summary("new_action", &json!({"x": 1}), english),
        english
    );
    assert_eq!(
        confirm_summary("start_recording", &json!({}), english),
        english
    );
    assert_eq!(
        confirm_summary(
            "publish_video",
            &json!({"platform_name": "YouTube"}),
            "Publish capture ? to a platform"
        ),
        "Publish capture ? to a platform"
    );
    // A row this version cannot word keeps the daemon's text and a humanized label.
    let rows = confirm_rows(
        "new_action",
        &json!({"future_field": "kept", "request_id": "r", "nothing": null}),
        &json!({}),
    );
    assert_eq!(rows, vec![("Future field".to_owned(), "kept".to_owned())]);
    // A word for a row whose parameters are missing keeps the daemon's value.
    let rows = confirm_rows(
        "start_recording",
        &json!({"output": "400 x 300 px"}),
        &json!({}),
    );
    assert_eq!(rows, vec![("Output".to_owned(), "400 x 300 px".to_owned())]);
    // Parameters that are not an object are tolerated.
    assert_eq!(params_of(&json!(3)), json!({}));
}

#[test]
fn a_comma_in_a_self_test_target_keeps_the_daemons_sentence() {
    let mut s = samples()
        .into_iter()
        .find(|s| s.kind == "run_fps_selftest")
        .unwrap();
    s.params["target"]["name"] = json!("Dell, left");
    assert_eq!(confirm_summary(s.kind, &s.params, "english"), "english");
}

#[test]
fn sizes_and_durations_keep_the_daemons_shapes() {
    assert_eq!(size_text(512), "512 B");
    assert_eq!(size_text(13_212), "13.2 KB");
    assert_eq!(size_text(2_400_000), "2.4 MB");
    assert_eq!(size_text(1_230_000_000), "1.2 GB");
    assert_eq!(duration_text(45.0), "45 s");
    assert_eq!(duration_text(120.0), "2 min");
    assert_eq!(duration_text(90.0), "1 min 30 s");
    assert_eq!(duration_text(3600.0), "1 h");
    assert_eq!(duration_text(3900.0), "1 h 5 min");
    // The decimal mark follows the language.
    with_language(Lang::Vi, || assert_eq!(size_text(2_400_000), "2,4 MB"));
}

// -- errors --

fn sample_params(english: &'static str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = english;
    while let Some(open) = rest.find('{') {
        let close = open + rest[open..].find('}').unwrap();
        out.push((rest[open + 1..close].to_owned(), "delete".to_owned()));
        rest = &rest[close + 1..];
    }
    out
}

/// Every message with a stable id has an arm here, and in English it reads exactly like the
/// daemon's message (pseudo-locale: it really goes through `tr!`).
#[test]
fn every_stable_message_is_translatable_and_english_is_unchanged() {
    for t in TEMPLATES {
        let params = sample_params(t.english);
        let refs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let english = render(t.id, &refs).unwrap();
        assert_eq!(error_text(Some(t.code), &english), english, "{}", t.id);
        with_language(Lang::EnXa, || {
            assert_ne!(
                error_text(Some(t.code), &english),
                english,
                "{} has no arm",
                t.id
            );
        });
    }
}

#[test]
fn unknown_errors_show_the_english_message() {
    with_language(Lang::EnXa, || {
        assert_eq!(
            error_text(Some(ErrorCode::Internal), "sqlite: database is locked"),
            "sqlite: database is locked"
        );
        // Right words, wrong code: not ours.
        assert_eq!(
            error_text(Some(ErrorCode::Internal), "A recording is already running"),
            "A recording is already running"
        );
        assert_eq!(error_text(None, "x"), "x");
    });
}

#[test]
fn errors_carry_their_parameters_into_the_sentence() {
    let e = error_text(
        Some(ErrorCode::DiskFull),
        "Only 312 MB free; recording needs more than 1 GB free to start",
    );
    assert_eq!(
        e,
        "Only 312 MB free; recording needs more than 1 GB free to start"
    );
    let coded = CallError {
        code: Some(ErrorCode::NotFound),
        permission: None,
        message: "Display 3 not found".into(),
        data: None,
    };
    assert_eq!(call_error_text(&coded), "Display 3 not found");
    with_language(Lang::EnXa, || {
        let t = call_error_text(&coded);
        assert!(t.contains('3') && t != "Display 3 not found", "{t}");
    });
}

/// REC-2: a start that fails because the Mac has no usable hardware H.264 encoder reads in the
/// chosen language (stable id `hardware_encoder_unavailable`); English is unchanged.
#[test]
fn the_missing_hardware_encoder_start_error_is_translated() {
    let english = message::render("hardware_encoder_unavailable", &[]).unwrap();
    let code = Some(ErrorCode::PlatformRejected);
    assert_eq!(error_text(code, &english), english);
    with_language(Lang::Vi, || {
        let t = error_text(code, &english);
        assert!(t != english && t.contains("H.264"), "{t}");
    });
    with_language(Lang::EnXa, || {
        assert_ne!(error_text(code, &english), english);
    });
}

/// The remote-copy rows of a delete dialog (an agent's `delete_capture` with the box) are worded
/// from `params.remote_items`, in Vietnamese too; the labels carry the row number.
#[test]
fn the_remote_copy_rows_of_a_delete_are_worded_in_the_chosen_language() {
    let s = samples()
        .into_iter()
        .find(|s| s.details.get("remote_delete_1").is_some())
        .unwrap();
    with_language(Lang::Vi, || {
        let rows = confirm_rows(s.kind, &s.details, &s.params);
        let get = |label: &str| {
            rows.iter()
                .find(|(l, _)| l == label)
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| panic!("{label} in {rows:?}"))
        };
        assert_eq!(
            get("Xoá bản sao trực tuyến 1"),
            "Google Drive: tệp a.mp4 (Google Drive (me), điểm lưu “Drive”)"
        );
        assert_eq!(
            get("Bản sao trực tuyến 1 đã không còn"),
            "Thư mục: tệp /nas/a.mp4 (đã không còn, không có gì để xoá)"
        );
        assert_eq!(
            get("Bản sao trực tuyến 1 được giữ lại"),
            "Giữ nguyên: bản xuất b.mp4 tại “Old disk” (điểm lưu đó đã bị gỡ khỏi Cài đặt)"
        );
    });
    with_language(Lang::Vi, || {
        let rows = confirm_rows(s.kind, &s.details, &s.params);
        assert!(
            rows.iter()
                .any(|(_, v)| v.starts_with("Việc xoá các bản sao")),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .any(|(_, v)| v.starts_with("Các bản sao trực tuyến được xoá trước")),
            "{rows:?}"
        );
    });
    // A newer daemon's item this version cannot word keeps the daemon's English row.
    let rows = confirm_rows(
        "delete_capture",
        &json!({"remote_delete_1": "Future: the thing"}),
        &json!({"remote_items": [{"state": "delete", "n": 1, "form": "from_the_future"}]}),
    );
    assert_eq!(rows[0].1, "Future: the thing");
}

/// The typed "FFmpeg was not found" ids: a job's English reason is recognised by id, worded in
/// the chosen language and unchanged in English.
#[test]
fn a_missing_ffmpeg_is_translated_by_id() {
    let cases = [
        (
            "ffmpeg_not_found",
            vec![],
            "Không tìm thấy FFmpeg: hãy cài bằng Homebrew",
        ),
        (
            "ffmpeg_bundle_missing",
            vec![],
            "Không tìm thấy FFmpeg: bản đi kèm bên trong CaptureMann đã mất",
        ),
        (
            "ffmpeg_env_invalid",
            vec![("path", "/x/ffmpeg")],
            "Không tìm thấy FFmpeg: CM_FFMPEG trỏ tới `/x/ffmpeg`",
        ),
        (
            "ffmpeg_bundle_cannot_run",
            vec![("path", "/A.app/ffmpeg"), ("error", "denied")],
            "Không tìm thấy FFmpeg: không chạy được bản nằm trong CaptureMann (`/A.app/ffmpeg`) (denied)",
        ),
        (
            "ffmpeg_permission_denied",
            vec![("path", "/b/ffmpeg")],
            "Không chạy được FFmpeg tại `/b/ffmpeg` (bị từ chối quyền)",
        ),
    ];
    for (id, params, vi) in cases {
        let english = render(id, &params).unwrap();
        let code = Some(ErrorCode::NotFound);
        assert_eq!(message::ffmpeg_missing_id(&english), Some(id));
        assert_eq!(error_text(code, &english), english, "{id}");
        with_language(Lang::Vi, || {
            let t = error_text(code, &english);
            assert!(t.starts_with(vi), "{id}: {t}");
        });
    }
}

// ---- AGT-2.AC11: the generic dialog of a tool set to Ask first ------------------------------

fn ask_first_params(tool: &str, title: &str) -> Value {
    json!({
        "tool": tool, "title": title, "client": "claude-desktop",
        "lines": [{"key": "target", "value": "{...}"}], "omitted": 0,
    })
}

const ASK_FIRST_ENGLISH: &str = "claude-desktop wants to run `capture_screenshot` (Capture screenshot). You set this tool to Ask first.";

#[test]
fn agt_2_ac11_the_ask_first_sentence_names_the_agent_and_the_tool() {
    let p = ask_first_params("capture_screenshot", "Capture screenshot");
    assert_eq!(
        confirm_summary("tool_ask_first", &p, ASK_FIRST_ENGLISH),
        "Claude Desktop is asking to run \u{201C}Take screenshots\u{201D}. You set this tool to Ask first in Settings \u{2192} Agents & CLI."
    );
    // A tool this version has no title for: the daemon's English title.
    let p = ask_first_params("brand_new_tool", "Brand new tool");
    assert!(confirm_summary("tool_ask_first", &p, ASK_FIRST_ENGLISH)
        .contains("\u{201C}Brand new tool\u{201D}"));
    // Parameters that are not understood: the daemon's English sentence.
    assert_eq!(
        confirm_summary("tool_ask_first", &json!({}), ASK_FIRST_ENGLISH),
        ASK_FIRST_ENGLISH
    );
    // The pseudo-locale changes the sentence (it is a wrapped message).
    with_language(Lang::EnXa, || {
        let p = ask_first_params("capture_screenshot", "Capture screenshot");
        assert_ne!(
            confirm_summary("tool_ask_first", &p, ASK_FIRST_ENGLISH),
            "Claude Desktop is asking to run \u{201C}Take screenshots\u{201D}. You set this tool to Ask first in Settings \u{2192} Agents & CLI."
        );
    });
}

#[test]
fn agt_2_ac11_the_ask_first_sentence_is_vietnamese() {
    let p = ask_first_params("capture_screenshot", "Capture screenshot");
    with_language(Lang::Vi, || {
        let s = confirm_summary("tool_ask_first", &p, ASK_FIRST_ENGLISH);
        assert!(s.starts_with("Claude Desktop muốn chạy \u{201C}"), "{s}");
        assert!(s.contains("Hỏi trước"), "{s}");
        assert!(!s.contains("{agent}") && !s.contains("{title}"), "{s}");
    });
}

#[test]
fn agt_2_ac11_the_ask_first_rows_are_the_parameter_lines() {
    let details = json!({"capture_id": "shot_1", "title": "A title", "values": "{...}"});
    let rows = confirm_rows("tool_ask_first", &details, &json!({}));
    assert_eq!(
        rows,
        [
            ("Capture id".to_owned(), "shot_1".to_owned()),
            ("Title".to_owned(), "A title".to_owned()),
            ("Values".to_owned(), "{...}".to_owned()),
        ]
    );
    // An English key this version has no label for is shown humanized, not blank.
    let rows = confirm_rows("tool_ask_first", &json!({"some_new_key": "x"}), &json!({}));
    assert_eq!(rows[0].0, "Some new key");
}
