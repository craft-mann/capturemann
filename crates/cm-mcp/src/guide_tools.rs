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
        "steps": { "type": "array", "items": { "type": "object", "properties": {
            "id": { "type": "string" },
            "number": { "type": "integer", "description": "Position, from 1." },
            "label": { "type": "string", "description": "The badge text in the guide's numbering style (1, A, a)." },
            "type": { "type": "string", "enum": ["image", "note"] },
            "title": { "type": "string" },
            "note": { "type": "string" },
            "image_id": { "type": "string", "description": "The step's screenshot as a capture id (`<guide_id>/<step_id>`): `annotate`, `get_capture`, `suggest_redactions`, `recognize_text` and `undo` take it. Absent for a note step." },
            "click": click_schema(),
            "focus_crop": { "type": "boolean" },
            "has_proposals": { "type": "boolean", "description": "Auto-redact found something to review; nothing was blurred." } },
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
            summary: "Read a guide: title, intro, numbering, and the ordered steps with title, note, click point, `has_proposals` and `image_id` (a capture id for the step's screenshot, so `get_capture` returns the image and `recognize_text` reads the step's text). Works while the guide records (`state`, `recorded_steps`).",
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
            summary: "Export a stopped guide. `markdown`: a folder `dest` with `guide.md` and `images/step-NN.png` (relative links, no absolute paths; pastes into a GitHub wiki). `html`: one self-contained file (images embedded, light and dark styles, table of contents, print styles; `dest` is a file or a folder). `rich_text`: puts formatted text with the pictures on the clipboard (no `dest`). `pdf` answers `not_implemented` in this version: export HTML and print it. The pictures are rendered, so annotations and blurs are in every export; steps that still have redaction proposals are listed in `steps_to_review` and exported without those blurs (review them with `suggest_redactions` and `annotate` first). Options: `headings` (`numbered` `1. Title`, `step` `Step 1 · Title`, `title_only`), `image_scale` 1 or 2 (images are at most 800 px wide, or 1600 at 2, never larger than the frame), and `intro`, `notes`, `note_steps` switches (default true).",
            input: json!({ "type": "object", "properties": {
                "guide_id": guide_id("The guide to export."),
                "format": { "type": "string", "enum": ["markdown", "html", "pdf", "rich_text"] },
                "dest": { "type": "string", "description": "Absolute path: a folder (markdown), a file or folder (html). Required; with `rich_text` it is ignored (pass any path)." },
                "headings": { "type": "string", "enum": ["numbered", "step", "title_only"], "default": "numbered" },
                "image_scale": { "type": "integer", "enum": [1, 2], "default": 1 },
                "intro": { "type": "boolean", "default": true },
                "notes": { "type": "boolean", "default": true },
                "note_steps": { "type": "boolean", "default": true },
                "request_id": request_id() },
                "required": ["guide_id", "format", "dest"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "guide_id": { "type": "string" },
                "format": { "type": "string" },
                "steps": { "type": "integer" },
                "path": { "type": "string" },
                "files": { "type": "array", "items": { "type": "string" } },
                "steps_to_review": { "type": "array", "items": { "type": "object", "properties": {
                    "step_id": { "type": "string" }, "number": { "type": "integer" } } } },
                "copied_to_clipboard": { "type": "boolean", "description": "Only with rich_text." },
                "html": { "type": "string", "description": "Only with rich_text." },
                "warnings": { "type": "array", "items": { "type": "string" } } },
                "required": ["guide_id", "format", "steps"] }),
        },
    ]
}
