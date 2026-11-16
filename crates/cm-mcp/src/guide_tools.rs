//! The Click Guide tools (GDE-4): `start_guide`, `stop_guide`, `get_guide`, `set_guide`,
//! `export_guide`. Registered by [`crate::tools::all`]; the behavior is in `cm_daemon::guide`.

use serde_json::{json, Value};

use crate::tools::{Tier, ToolSpec};

fn request_id() -> Value {
    json!({ "type": "string", "minLength": 1, "maxLength": 256,
            "description": "Idempotency key; a repeat within 24 h returns the first result." })
}

fn guide_id(required_desc: &str) -> Value {
    json!({ "type": "string", "minLength": 1, "description": required_desc })
}

fn click_schema() -> Value {
    json!({ "type": "object", "properties": {
        "x": { "type": "number" }, "y": { "type": "number" },
        "button": { "type": "string", "enum": ["left", "right", "middle"] },
        "count": { "type": "integer", "description": "2 for a double click." } },
        "required": ["x", "y", "button", "count"],
        "description": "The click, in physical px of the step's original image." })
}

fn guide_output() -> Value {
    json!({ "type": "object", "properties": {
        "guide_id": { "type": "string" },
        "title": { "type": "string" },
        "intro": { "type": "string" },
        "numbering": { "type": "string", "enum": ["number", "upper", "lower"] },
        "state": { "type": "string", "enum": ["countdown", "recording", "paused", "stopped", "cancelled", "discarded"] },
        "step_count": { "type": "integer" },
        "window_closed": { "type": "boolean", "description": "While recording: the followed window has closed. The guide paused itself and cannot resume; its steps are kept, `stop_guide` saves it." },
        "steps": { "type": "array", "items": { "type": "object", "properties": {
            "id": { "type": "string" },
            "number": { "type": "integer", "description": "Position, from 1." },
            "label": { "type": "string", "description": "The badge text in the guide's numbering style (1, A, a)." },
            "type": { "type": "string", "enum": ["image", "note"] },
            "title": { "type": "string" },
            "title_source": { "type": "string", "enum": ["screen_text", "generic"], "description": "Only on a title CaptureMann drafted: `screen_text` (named from the text recognized next to the click, used only when it looks like a control's label) or `generic` (`Click here`). Absent once a person or an agent wrote the title: rewrite drafts from the step's picture with `set_guide`. The user can turn screen text out of titles with the setting `guide.titles_from_screen`." },
            "note": { "type": "string" },
            "image_id": { "type": "string", "description": "The step's screenshot as a capture id (`<guide_id>/<step_id>`): `annotate`, `get_capture`, `suggest_redactions`, `recognize_text` and `undo` take it. Absent for a note step." },
            "click": click_schema(),
            "focus_crop": { "type": "boolean" } },
            "required": ["id", "number", "type", "title", "note"] } },
        "can_undo": { "type": "boolean" },
        "can_redo": { "type": "boolean" },
        "discarded": { "type": "boolean", "description": "`stop_guide` with no step taken: the empty guide was removed." } },
        "required": ["guide_id", "state", "step_count"] })
}

/// The five Click Guide tools.
pub fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "start_guide",
            tier: Tier::Confirm,
            mutating: true,
            destructive: false,
            spec: "GDE-1, GDE-4",
            summary: "Start a Click Guide: the user clicks through a task once and CaptureMann takes a screenshot of each mouse-down in the followed target, with a numbered badge and a ring at the click point and a draft title from the text nearest the click (`Click \"Save\"`). **Confirm tier** because it listens to the user's mouse clicks (never keystrokes): the user must approve on screen, the dialog says clicks only, and the call waits up to 5 minutes for the answer. After the approval there is a countdown (`countdown_s`, default 3) and the guide records until `stop_guide`. Clicks on CaptureMann's own HUD and outside the target are not steps. Auto-redact runs on each step and only marks the steps that have proposals (`has_proposals`); nothing is blurred without `annotate`. Needs Input Monitoring on macOS (`permission_missing` with `data.permission: \"input_monitoring\"`) and a session that delivers click events (a Wayland session may not: `not_implemented` with the reason). Only one guide records at a time (`busy`). While it records, `set_guide` accepts only `paused`, `note_step` and `remove_last`.",
            input: json!({ "type": "object", "properties": {
                "target": { "description": "What to follow, as in capture_screenshot (`pick` is not supported).", "oneOf": [
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 } },
                      "required": ["display_id"], "additionalProperties": false },
                    { "type": "object", "properties": { "window_id": { "type": "integer", "minimum": 0 } },
                      "required": ["window_id"], "additionalProperties": false },
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 },
                        "rect": { "type": "array", "items": { "type": "integer" }, "minItems": 4, "maxItems": 4,
                                  "description": "[x, y, w, h] physical px relative to the display" } },
                      "required": ["display_id", "rect"], "additionalProperties": false } ] },
                "title": { "type": "string", "description": "Default: `Click Guide <date time>`." },
                "countdown_s": { "type": "number", "minimum": 0, "maximum": 10, "default": 3 },
                "request_id": request_id() },
                "required": ["target"], "additionalProperties": false }),
            output: guide_output(),
        },
        ToolSpec {
            name: "stop_guide",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "GDE-1, GDE-4",
            summary: "Stop the Click Guide that is recording and return the finished guide (`get_guide` shape). A guide with no step is not kept (`discarded: true`). Stopping during the countdown cancels the guide. Next: rewrite the draft titles from each step's text with `set_guide`, then `export_guide`.",
            input: json!({ "type": "object", "properties": {
                "guide_id": guide_id("The guide to stop; default: the one recording."),
                "request_id": request_id() },
                "additionalProperties": false }),
            output: guide_output(),
        },
        ToolSpec {
            name: "get_guide",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "GDE-4",
            summary: "Read a guide: title, intro, numbering, and the ordered steps with title, note, click point and `image_id` (a capture id for the step's screenshot, so `get_capture` returns the image and `recognize_text` reads the step's text). Works while the guide records (`state`, `recorded_steps`).",
            input: json!({ "type": "object", "properties": {
                "guide_id": guide_id("Default: the guide that is recording.") },
                "additionalProperties": false }),
            output: guide_output(),
        },
        ToolSpec {
            name: "set_guide",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "GDE-2, GDE-4",
            summary: "Edit a guide. One call is ONE undo step (`undo` and `redo` take the guide id): `title`, `intro`, `numbering`, and `steps`, the new order as a list of `{ id, title?, note?, delete?, duplicate? }` (a step with no `id` and a `title` is a new note step; steps not listed keep their order after the listed ones; badges follow the order). `focus_crop` with `crop_steps` turns the focus crop of those steps on or off. `retake` takes a fresh frame of the followed target for those steps. While the guide records only `paused` (Pause and Resume), `note_step` (`{ title, note? }`, a step with no image) and `remove_last` (drop the newest step) are allowed. Edit the step pictures with `annotate` on the `image_id`.",
            input: json!({ "type": "object", "properties": {
                "guide_id": guide_id("Default: the guide that is recording."),
                "title": { "type": "string" },
                "intro": { "type": "string" },
                "numbering": { "type": "string", "enum": ["number", "upper", "lower"] },
                "steps": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" },
                    "title": { "type": "string" },
                    "note": { "type": "string" },
                    "delete": { "type": "boolean" },
                    "duplicate": { "type": "boolean" } },
                    "additionalProperties": false } },
                "focus_crop": { "type": "boolean" },
                "crop_steps": { "type": "array", "items": { "type": "string" } },
                "retake": { "type": "array", "items": { "type": "string" } },
                "paused": { "type": "boolean" },
                "note_step": { "type": "object", "properties": {
                    "title": { "type": "string" }, "note": { "type": "string" } }, "additionalProperties": false },
                "remove_last": { "type": "boolean" },
                "request_id": request_id() },
                "additionalProperties": false }),
            output: guide_output(),
        },
        ToolSpec {
            name: "export_guide",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "GDE-3, GDE-4",
            summary: "Export a stopped guide. `markdown`: `guide.md` and `images/step-NN.png` (relative links, no absolute paths; pastes into a GitHub wiki) in the folder `dest` when it is new or empty, else in a new folder inside it named after the guide (`Title (2)` when taken); titles and notes are written as plain text, never as links or markup. Nothing is ever overwritten: an HTML or PDF file that exists gets `name (2).html`; `path` is what was written and `renamed` says whether the name differs from `dest`. `html`: one self-contained file (images embedded, light and dark styles, table of contents, print styles; `dest` is a file or a folder). `rich_text`: puts formatted text with the pictures on the clipboard (no `dest`). `pdf`: one file (`dest` is a file or a folder), A4 or `paper` `letter`, one or two steps per page, the fonts embedded (Vietnamese text works). The pictures are rendered, so annotations and blurs are in every export; Click Guide does not mark or blur text: a step can show private text, so look at the pictures first (`suggest_redactions` and `annotate` work on a step's `image_id`). Options: `headings` (`numbered` `1. Title`, `step` `Step 1 · Title`, `title_only`), `image_scale` 1 or 2 (images are at most 800 px wide, or 1600 at 2, never larger than the frame), and `intro`, `notes`, `note_steps` switches (default true).",
            input: json!({ "type": "object", "properties": {
                "guide_id": guide_id("The guide to export."),
                "format": { "type": "string", "enum": ["markdown", "html", "pdf", "rich_text"] },
                "dest": { "type": "string", "description": "Absolute path: a folder (markdown), a file or folder (html, pdf). Required; with `rich_text` it is ignored (pass any path)." },
                "headings": { "type": "string", "enum": ["numbered", "step", "title_only"], "default": "numbered" },
                "image_scale": { "type": "integer", "enum": [1, 2], "default": 1 },
                "paper": { "type": "string", "enum": ["a4", "letter"], "default": "a4", "description": "Only for pdf." },
                "intro": { "type": "boolean", "default": true },
                "notes": { "type": "boolean", "default": true },
                "note_steps": { "type": "boolean", "default": true },
                "request_id": request_id() },
                "required": ["guide_id", "format", "dest"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "guide_id": { "type": "string" },
                "format": { "type": "string" },
                "steps": { "type": "integer" },
                "path": { "type": "string", "description": "The folder or file written; differs from `dest` when `renamed`." },
                "renamed": { "type": "boolean", "description": "A free name was chosen because `dest` was taken (never overwritten)." },
                "requested": { "type": "string", "description": "markdown: `dest` as asked." },
                "files": { "type": "array", "items": { "type": "string" } },
                "copied_to_clipboard": { "type": "boolean", "description": "Only with rich_text." },
                "html": { "type": "string", "description": "Only with rich_text." },
                "warnings": { "type": "array", "items": { "type": "string" } } },
                "required": ["guide_id", "format", "steps"] }),
        },
    ]
}
