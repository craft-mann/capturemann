//! MCP prompts (AGT-1, "MCP resources, prompts and events"): short task templates that name
//! the real tools. A prompt only returns text for the model; it never calls the daemon and
//! never runs anything. Text is English (MCP is not translated).

use rmcp::model::{GetPromptResult, Prompt, PromptArgument, PromptMessage, Role};
use serde_json::{Map, Value};

/// One prompt: its arguments and the tools its text names (a test checks both).
pub struct PromptSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// `(name, description, required)`.
    pub args: &'static [(&'static str, &'static str, bool)],
    /// Every tool the template mentions; each must exist in [`crate::tools::all`].
    pub tools: &'static [&'static str],
    build: fn(&Map<String, Value>) -> String,
}

/// Why a prompt could not be built.
#[derive(Debug, PartialEq, Eq)]
pub enum PromptError {
    Unknown(String),
    MissingArgument(&'static str),
}

fn arg<'a>(a: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    a.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The capture the user named, or a reminder to find one.
fn capture_ref(a: &Map<String, Value>) -> String {
    match arg(a, "capture_id") {
        Some(id) => format!("capture `{id}`"),
        None => {
            "the capture the user means (find it with `list_library` if no id was given)".into()
        }
    }
}

fn record_tutorial(a: &Map<String, Value>) -> String {
    let topic = arg(a, "topic").unwrap_or("the topic the user names");
    format!(
        "Record a tutorial about {topic}.\n\
1. `list_devices`: check the camera, microphone and permissions. If a permission is missing, tell the user and stop; do not try to work around it.\n\
2. `start_recording` with the screen, microphone and webcam tracks. The user approves on screen and sees a countdown; if they decline, stop and say so.\n\
3. When the user says they are done, `stop_recording` and keep the returned capture id.\n\
4. `transcribe` the capture (it needs an installed speech model; if the error says there is none, tell the user to download one on the AI page and stop). Wait with `get_job` until it is done.\n\
5. `suggest_cuts` and `suggest_zooms`. These only propose; apply the ones that are clearly right with `cut_segments` and `set_zooms`, and list the rest for the user.\n\
6. Reply with a short summary: duration, what you cut, what you zoomed, and what is left for the user to decide."
    )
}

fn publish_everywhere(a: &Map<String, Value>) -> String {
    let c = capture_ref(a);
    format!(
        "Publish {c}. Only YouTube is available in this version; other platforms answer an error.\n\
1. `get_capture` to read the title and length, then `get_transcript` if it exists, so the text you write matches what is said.\n\
2. `list_publish_accounts` for the connected accounts, the platform limits and today's upload quota. If no account is connected, tell the user to connect one in Settings and stop.\n\
3. `export_video` with the youtube-1080p preset (or youtube-1440p or youtube-4k if the recording is that large); for a vertical cut also vertical-1080. Wait with `get_job`.\n\
4. Write the title and description within the limits from step 2.\n\
5. `publish_video` for YouTube. The user must approve the exact plan on screen. Never choose public on your own: leave the privacy at its default (unlisted) unless the user asked for another.\n\
6. Report the video URL from the job result (`get_job`) or the publish.done event."
    )
}

fn bug_report_screenshot(a: &Map<String, Value>) -> String {
    let what = arg(a, "window").unwrap_or("the window the user names");
    format!(
        "Make a screenshot for a bug report of {what}.\n\
1. `list_targets` to find the window or display.\n\
2. `capture_screenshot` of it.\n\
3. `suggest_redactions` on the capture. It only proposes blur rectangles for emails, keys, tokens, phone numbers and IP addresses. Check the list yourself; do not redact anything you are unsure about without asking.\n\
4. `annotate` in one call: the accepted redactions and numbered step markers (one per step the user described). One call is one undo step.\n\
5. Set copy_to_clipboard to true on that `annotate` call so the rendered image (never the original) is ready to paste.\n\
6. Say what was redacted and numbered."
    )
}

fn lesson_to_drive(a: &Map<String, Value>) -> String {
    let subject = arg(a, "subject").unwrap_or("the lesson the user names");
    format!(
        "Record {subject} and put it on Google Drive.\n\
1. `list_devices`, then `start_recording` with the screen, microphone and webcam (the user approves on screen).\n\
2. `stop_recording`; keep the capture id.\n\
3. `transcribe`, then `suggest_cuts`. Trim silence only at the start and the end: `trim` to the first and last spoken word. Do not cut the middle unless the user asks.\n\
4. `export_video` with the youtube-1080p preset; wait with `get_job` until it is done.\n\
5. `list_destinations`: use the Google Drive destination. If none is connected, tell the user to connect one in Settings and stop.\n\
6. `upload` the capture to it and wait with `get_job`.\n\
7. Return the share link from the job result. If the result has no link, say so instead of guessing one."
    )
}

fn caption_cleanup(a: &Map<String, Value>) -> String {
    let c = capture_ref(a);
    let words = arg(a, "glossary").map_or_else(String::new, |g| {
        format!(" Names and terms to keep exactly as written: {g}.")
    });
    format!(
        "Clean up the captions of {c}.{words}\n\
1. `get_transcript`. If it fails with not_found, run `transcribe` first and wait with `get_job`.\n\
2. Read the caption lines. Look for misheard names and terms, filler words, and lines over the length limits.\n\
3. Fix wrong words with `replace_in_captions` (one find and replace per mistake, so each is easy to review); rewrite or re-split whole lines with `set_captions`. Keep the timing; change text only. The user's own edits stay: do not undo them.\n\
4. `get_captions` to check the result.\n\
5. Reply with the list of changes. Export a file with `export_captions` (SRT, VTT or TXT) only if the user asks."
    )
}

fn make_short_clip(a: &Map<String, Value>) -> String {
    let c = capture_ref(a);
    let len = arg(a, "seconds").unwrap_or("30");
    format!(
        "Make a vertical clip of about {len} seconds from {c}.\n\
1. `get_transcript` (run `transcribe` first if there is none) and pick one self-contained passage about that long.\n\
2. `trim` to that passage. This edits the edit list only; the recording is never changed.\n\
3. `suggest_zooms`, then apply the good ones with `set_zooms`. `set_reframe` keeps the subject in a 9:16 frame.\n\
4. `set_caption_options` to show burned-in captions.\n\
5. `export_video` with the vertical-1080 preset; wait with `get_job` until it is done.\n\
6. Report the file path. Do not publish it; if the user wants that, use the publish-everywhere prompt."
    )
}

const CAPTURE_ID: (&str, &str, bool) = (
    "capture_id",
    "The capture to work on (found with list_library when missing).",
    false,
);

/// All prompts, in listing order.
pub const PROMPTS: &[PromptSpec] = &[
    PromptSpec {
        name: "record-tutorial",
        description: "Check devices, record with the webcam (the user confirms), transcribe, suggest cuts and zooms, and summarize.",
        args: &[("topic", "What the tutorial is about.", false)],
        tools: &[
            "list_devices",
            "start_recording",
            "stop_recording",
            "transcribe",
            "get_job",
            "suggest_cuts",
            "suggest_zooms",
            "cut_segments",
            "set_zooms",
        ],
        build: record_tutorial,
    },
    PromptSpec {
        name: "publish-everywhere",
        description: "Export a recording, write the metadata within the platform limits, publish after the user confirms, and return the URL.",
        args: &[CAPTURE_ID],
        tools: &[
            "get_capture",
            "get_transcript",
            "list_publish_accounts",
            "export_video",
            "get_job",
            "publish_video",
        ],
        build: publish_everywhere,
    },
    PromptSpec {
        name: "bug-report-screenshot",
        description: "Capture a window, blur emails and tokens, number the steps, and copy the result to the clipboard.",
        args: &[("window", "Which window or display to capture.", false)],
        tools: &[
            "list_targets",
            "capture_screenshot",
            "suggest_redactions",
            "annotate",
        ],
        build: bug_report_screenshot,
    },
    PromptSpec {
        name: "lesson-to-drive",
        description: "Record a lesson with the webcam, trim the silence at both ends, export 1080p, upload to Google Drive, and return the link.",
        args: &[("subject", "What the lesson is about.", false)],
        tools: &[
            "list_devices",
            "start_recording",
            "stop_recording",
            "transcribe",
            "suggest_cuts",
            "trim",
            "export_video",
            "get_job",
            "list_destinations",
            "upload",
        ],
        build: lesson_to_drive,
    },
    PromptSpec {
        name: "caption-cleanup",
        description: "Fix misheard words and filler in a recording's captions without touching the timing or the user's own edits.",
        args: &[
            CAPTURE_ID,
            (
                "glossary",
                "Names and terms to keep exactly as written, comma separated.",
                false,
            ),
        ],
        tools: &[
            "get_transcript",
            "transcribe",
            "get_job",
            "replace_in_captions",
            "set_captions",
            "get_captions",
            "export_captions",
        ],
        build: caption_cleanup,
    },
    PromptSpec {
        name: "make-short-clip",
        description: "Turn one passage of a recording into a vertical clip with zoom and burned-in captions.",
        args: &[
            CAPTURE_ID,
            ("seconds", "Target length in seconds (default 30).", false),
        ],
        tools: &[
            "get_transcript",
            "transcribe",
            "trim",
            "suggest_zooms",
            "set_zooms",
            "set_reframe",
            "set_caption_options",
            "export_video",
            "get_job",
        ],
        build: make_short_clip,
    },
];

/// The `prompts/list` entries.
pub fn list() -> Vec<Prompt> {
    PROMPTS
        .iter()
        .map(|p| {
            let args = p
                .args
                .iter()
                .map(|(n, d, req)| {
                    PromptArgument::new(*n)
                        .with_description(*d)
                        .with_required(*req)
                })
                .collect();
            Prompt::new(p.name, Some(p.description), Some(args))
        })
        .collect()
}

/// The `prompts/get` result. Only declared arguments are used; each is flattened to one
/// line of at most 200 characters, so an argument is a name or a number and not a place to
/// put instructions.
pub fn get(name: &str, args: &Map<String, Value>) -> Result<GetPromptResult, PromptError> {
    let spec = PROMPTS
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| PromptError::Unknown(name.to_owned()))?;
    let mut clean = Map::new();
    for (n, _, required) in spec.args {
        match arg(args, n) {
            Some(v) => {
                let one_line: String = v
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .take(200)
                    .collect();
                clean.insert((*n).to_owned(), Value::String(one_line));
            }
            None if *required => return Err(PromptError::MissingArgument(n)),
            None => {}
        }
    }
    let text = (spec.build)(&clean);
    Ok(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
            .with_description(spec.description),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_named_tool_exists_and_is_mentioned() {
        let tools: Vec<_> = crate::tools::all().iter().map(|t| t.name).collect();
        for p in PROMPTS {
            let text = (p.build)(&Map::new());
            assert!(text.len() < 3000, "{}: templates are short", p.name);
            for t in p.tools {
                assert!(tools.contains(t), "{}: unknown tool {t}", p.name);
                assert!(
                    text.contains(&format!("`{t}`")),
                    "{}: {t} not in text",
                    p.name
                );
            }
            // The reverse: every backticked snake_case word is a real tool.
            for w in text.split('`').skip(1).step_by(2) {
                if w.contains('_') && !w.contains(' ') {
                    assert!(tools.contains(&w), "{}: `{w}` is not a tool", p.name);
                }
            }
        }
    }

    #[test]
    fn arguments_are_placed_and_sanitized() {
        let a = json!({"topic": "git\nrebase", "extra": "x"});
        let r = get("record-tutorial", a.as_object().unwrap()).unwrap();
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("git rebase"));
        assert!(!s.contains("extra"));
        assert_eq!(
            get("nope", &Map::new()),
            Err(PromptError::Unknown("nope".into()))
        );
    }
}
