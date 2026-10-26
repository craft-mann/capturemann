use super::*;

// ---- the indicator facts and the dialog text ------------------------------------------------

fn rect4(r: &cm_core::PhysicalRect) -> [i64; 4] {
    [
        i64::from(r.origin.x),
        i64::from(r.origin.y),
        i64::from(r.size.width),
        i64::from(r.size.height),
    ]
}

pub(super) fn indicator_json(
    r: &ResolvedStart,
    plan: &CapturePlan,
    targets: &Targets,
    actor: &Actor,
) -> Value {
    let display = plan
        .display_id
        .and_then(|id| targets.displays.iter().find(|d| d.id == id));
    let (kind, window_id, bounds) = match plan.target {
        CaptureTarget::Display { .. } => ("display", None, display.map(|d| rect4(&d.bounds))),
        CaptureTarget::Region { .. } => (
            "region",
            None,
            display.zip(plan.crop).map(|(d, c)| {
                [
                    i64::from(d.bounds.origin.x) + i64::from(c.origin.x),
                    i64::from(d.bounds.origin.y) + i64::from(c.origin.y),
                    i64::from(c.size.width),
                    i64::from(c.size.height),
                ]
            }),
        ),
        CaptureTarget::Window { window_id } => (
            "window",
            Some(window_id),
            targets
                .windows
                .iter()
                .find(|w| w.id == window_id)
                .map(|w| rect4(&w.bounds)),
        ),
    };
    let started_by = match actor {
        Actor::User => json!({ "kind": "user", "client": null, "tool": null }),
        Actor::Agent(name) => json!({ "kind": "agent", "client": name, "tool": "start_recording" }),
    };
    json!({
        "area": {
            "kind": kind,
            "display_id": plan.display_id,
            "window_id": window_id,
            "bounds": bounds,
            "source_size": [plan.source_size.width, plan.source_size.height],
            "scale": plan.scale.get(),
        },
        "output": {
            "width": plan.video.width, "height": plan.video.height,
            "fps": plan.video.fps, "codec": plan.video.codec.as_str(),
        },
        "camera": r.camera.as_ref().map(crate::recording::params::CameraParams::bubble),
        "mic": r.mic.is_some(),
        "system_audio": r.system_audio,
        "cursor": r.cursor,
        "started_by": started_by,
    })
}

/// Longest agent- or app-supplied text shown in a dialog row (R2-10).
pub(super) const DIALOG_TEXT_MAX: usize = 120;

/// Makes text an agent or another app controls safe to show in a confirmation row: control
/// characters (newlines included) become spaces, bidirectional and invisible formatting marks
/// are dropped (they can reorder or hide what follows), and the text is cut at 120 characters.
pub(crate) fn dialog_text(s: &str) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in s.chars() {
        let c = if c.is_control() { ' ' } else { c };
        if matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{061C}'
                | '\u{FEFF}'
                | '\u{00AD}'
        ) {
            continue;
        }
        if n == DIALOG_TEXT_MAX {
            out.push('\u{2026}');
            break;
        }
        out.push(c);
        n += 1;
    }
    out
}

/// The dialog sentence and rows (AGT-2: "shows exactly what will happen").
pub(super) fn describe(
    r: &ResolvedStart,
    plan: &CapturePlan,
    targets: &Targets,
    names: &DeviceNames,
    root: &Path,
    restart: bool,
) -> (String, Value, Value) {
    let display = plan
        .display_id
        .and_then(|id| targets.displays.iter().find(|d| d.id == id));
    let (short, long) = match plan.target {
        CaptureTarget::Display { display_id } => (
            format!("display {display_id}"),
            format!(
                "Display {display_id}{}, {} x {} px",
                display.map_or(String::new(), |d| format!(" \"{}\"", d.name)),
                plan.source_size.width,
                plan.source_size.height
            ),
        ),
        CaptureTarget::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id);
            (
                w.map_or_else(
                    || format!("window {window_id}"),
                    |w| format!("the window \"{}\"", dialog_text(&w.title)),
                ),
                w.map_or_else(
                    || format!("Window {window_id}"),
                    |w| {
                        format!(
                            "Window \"{}\" of {}, {} x {} px (follows the window)",
                            dialog_text(&w.title),
                            dialog_text(&w.app),
                            plan.source_size.width,
                            plan.source_size.height
                        )
                    },
                ),
            )
        }
        CaptureTarget::Region { display_id, .. } => {
            let c = plan.crop;
            let (w, h) = (plan.source_size.width, plan.source_size.height);
            (
                format!("a {w} x {h} region of display {display_id}"),
                format!(
                    "Region {w} x {h} px at ({}, {}) on display {display_id}",
                    c.map_or(0, |c| c.origin.x),
                    c.map_or(0, |c| c.origin.y)
                ),
            )
        }
    };
    let codec = match plan.video.codec {
        cm_encode::Codec::Hevc => "HEVC",
        cm_encode::Codec::H264 => "H.264",
    };
    let output = format!(
        "{} x {} px, {} fps, {codec}, about {:.0} MB per minute (an estimate: the real size varies with what is on screen)",
        plan.video.width,
        plan.video.height,
        plan.video.fps,
        plan.video.disk_mb_per_minute()
    );
    let mic = match (&r.mic, &names.mic) {
        (None, _) => "Off".to_owned(),
        (Some(_), n) => format!(
            "On: {}{}",
            n.as_deref().unwrap_or("the default microphone"),
            if r.mic_denoise() {
                ", noise suppression on"
            } else {
                ""
            }
        ),
    };
    let camera = match (&r.camera, &names.camera) {
        (None, _) => "Off".to_owned(),
        (Some(c), n) => format!(
            "On: {}, {} (the camera light turns on)",
            n.as_deref().unwrap_or("the default camera"),
            c.resolution.as_str()
        ),
    };
    let mut sensors = Vec::new();
    if r.mic.is_some() {
        sensors.push("the microphone");
    }
    if r.camera.is_some() {
        sensors.push("the camera");
    }
    if r.system_audio {
        sensors.push("system audio");
    }
    let with = match sensors.as_slice() {
        [] => String::new(),
        [a] => format!(" with {a}"),
        [a, b] => format!(" with {a} and {b}"),
        [a, b, c] => format!(" with {a}, {b} and {c}"),
        _ => String::new(),
    };
    let lead = if restart {
        "Discard the current take and start over: record"
    } else {
        "Record"
    };
    let summary = format!(
        "{lead} {short} at {} x {}, {} fps{with}, and save it to {}",
        plan.video.width,
        plan.video.height,
        plan.video.fps,
        root.display()
    );
    let mut details = Map::new();
    details.insert("target".into(), json!(long));
    details.insert("output".into(), json!(output));
    details.insert("screen".into(), json!("On (always recorded)"));
    details.insert("microphone".into(), json!(mic));
    details.insert("camera".into(), json!(camera));
    details.insert(
        "system_audio".into(),
        json!(if r.system_audio { "On" } else { "Off" }),
    );
    details.insert(
        "cursor".into(),
        json!(if r.cursor {
            "Position, clicks and scroll direction (never keystrokes)"
        } else {
            "Off"
        }),
    );
    details.insert(
        "save_to".into(),
        json!(format!(
            "{}{}",
            root.display(),
            r.title
                .as_deref()
                .map_or(String::new(), |t| format!(" as \"{}\"", dialog_text(t)))
        )),
    );
    details.insert(
        "max_duration".into(),
        json!(r
            .max_duration_s
            .map_or_else(|| "No limit".to_owned(), duration_words)),
    );
    details.insert(
        "countdown".into(),
        json!(if r.countdown_s == 0 {
            "None".to_owned()
        } else {
            format!("{} s", r.countdown_s)
        }),
    );
    details.insert(
        "excluded_windows".into(),
        json!("CaptureMann's own windows and your password managers never appear"),
    );
    if restart {
        details.insert(
            "discarded".into(),
            json!("The current take is deleted and cannot be recovered"),
        );
    }
    let params = json!({
        "restart": restart,
        "target": target_params(plan, targets),
        "width": plan.video.width,
        "height": plan.video.height,
        "fps": plan.video.fps,
        "codec": codec,
        "mb_per_minute": plan.video.disk_mb_per_minute(),
        "microphone": r.mic.as_ref().map(|_| json!({
            "device": names.mic,
            "denoise": r.mic_denoise(),
        })),
        "camera": r.camera.as_ref().map(|c| json!({
            "device": names.camera,
            "resolution": c.resolution.as_str(),
        })),
        "system_audio": r.system_audio,
        "cursor": r.cursor,
        "save_to": root.display().to_string(),
        "title": r.title.as_deref().map(dialog_text),
        "max_duration_s": r.max_duration_s,
        "countdown_s": r.countdown_s,
    });
    (summary, Value::Object(details), params)
}

/// The target as typed parameters for the confirmation request (P3): what the dialog names,
/// with texts already made safe for display.
fn target_params(plan: &CapturePlan, targets: &Targets) -> Value {
    let size = (plan.source_size.width, plan.source_size.height);
    match plan.target {
        CaptureTarget::Display { display_id } => {
            let name = plan
                .display_id
                .and_then(|id| targets.displays.iter().find(|d| d.id == id))
                .map(|d| d.name.clone());
            json!({ "type": "display", "id": display_id, "name": name,
                    "width": size.0, "height": size.1 })
        }
        CaptureTarget::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id);
            json!({ "type": "window", "id": window_id,
                    "title": w.map(|w| dialog_text(&w.title)),
                    "app": w.map(|w| dialog_text(&w.app)),
                    "width": size.0, "height": size.1 })
        }
        CaptureTarget::Region { display_id, .. } => {
            let c = plan.crop;
            json!({ "type": "region", "display_id": display_id,
                    "x": c.map_or(0, |c| c.origin.x), "y": c.map_or(0, |c| c.origin.y),
                    "width": size.0, "height": size.1 })
        }
    }
}

// ---- audio recordings (VOC-1) ---------------------------------------------------------------

/// The consent line of VOC-1.AC7: in the start sheet while `recording.consent_notice` is on, and
/// always in the confirmation dialog an agent's `start_recording { mode: "audio" }` opens.
pub const CONSENT_SENTENCE: &str = "Let everyone know they're being recorded.";

/// The indicator facts of an audio recording: no area, no output size, no camera, no cursor.
pub(super) fn indicator_json_audio(r: &ResolvedStart, actor: &Actor) -> Value {
    let started_by = match actor {
        Actor::User => json!({ "kind": "user", "client": null, "tool": null }),
        Actor::Agent(name) => json!({ "kind": "agent", "client": name, "tool": "start_recording" }),
    };
    json!({
        "mode": "audio",
        "preset": r.preset,
        "area": null,
        "output": null,
        "camera": null,
        "mic": r.mic.is_some(),
        "system_audio": r.system_audio,
        "cursor": false,
        "started_by": started_by,
    })
}

/// The dialog sentence and rows of an audio recording: "Record audio", the microphone's name,
/// no screen target (VOC-1.AC8), and the consent line (AC7).
pub(super) fn describe_audio(
    r: &ResolvedStart,
    names: &DeviceNames,
    root: &Path,
    restart: bool,
    consent_notice: bool,
) -> (String, Value, Value) {
    let mic_name = names.mic.as_deref().unwrap_or("the default microphone");
    let sources = match (&r.mic, r.system_audio) {
        (Some(_), false) => format!("the microphone \"{}\"", dialog_text(mic_name)),
        (Some(_), true) => format!(
            "the microphone \"{}\" and system audio",
            dialog_text(mic_name)
        ),
        (None, _) => "system audio".to_owned(),
    };
    let lead = if restart {
        "Discard the current take and start over: record audio"
    } else {
        "Record audio"
    };
    let summary = format!("{lead} from {sources}, and save it to {}", root.display());
    let mic = match &r.mic {
        None => "Off".to_owned(),
        Some(_) => format!(
            "On: {}{}",
            dialog_text(mic_name),
            if r.mic_denoise() {
                ", noise suppression on"
            } else {
                ""
            }
        ),
    };
    let mut details = Map::new();
    details.insert(
        "mode".into(),
        json!("Record audio (no screen, camera or cursor)"),
    );
    if let Some(p) = r.preset {
        details.insert("preset".into(), json!(p.as_str()));
    }
    details.insert("microphone".into(), json!(mic));
    details.insert(
        "system_audio".into(),
        json!(if r.system_audio { "On" } else { "Off" }),
    );
    details.insert(
        "save_to".into(),
        json!(format!(
            "{}{}",
            root.display(),
            r.title
                .as_deref()
                .map_or(String::new(), |t| format!(" as \"{}\"", dialog_text(t)))
        )),
    );
    details.insert(
        "max_duration".into(),
        json!(r
            .max_duration_s
            .map_or_else(|| "No limit".to_owned(), duration_words)),
    );
    details.insert(
        "countdown".into(),
        json!(if r.countdown_s == 0 {
            "None".to_owned()
        } else {
            format!("{} s", r.countdown_s)
        }),
    );
    details.insert("consent".into(), json!(CONSENT_SENTENCE));
    if restart {
        details.insert(
            "discarded".into(),
            json!("The current take is deleted and cannot be recovered"),
        );
    }
    let params = json!({
        "mode": "audio",
        "restart": restart,
        "preset": r.preset,
        "microphone": r.mic.as_ref().map(|_| json!({
            "device": names.mic,
            "denoise": r.mic_denoise(),
        })),
        "system_audio": r.system_audio,
        "save_to": root.display().to_string(),
        "title": r.title.as_deref().map(dialog_text),
        "max_duration_s": r.max_duration_s,
        "countdown_s": r.countdown_s,
        "consent": CONSENT_SENTENCE,
        "consent_notice": consent_notice,
    });
    (summary, Value::Object(details), params)
}
