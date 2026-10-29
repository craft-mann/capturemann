//! The tool table: one row per daemon action exposed over MCP, with JSON Schema for input and
//! output (specs/02-agent-interface.md, "MCP tools (v0.1)").
//!
//! Names equal the daemon's JSON-RPC methods, which equal the CLI subcommands. The input
//! schemas mirror what `cm-daemon` accepts (`additionalProperties: false` everywhere it
//! rejects unknown keys), so a client that follows the schema is never refused for shape. The
//! daemon remains the only validator: this crate does not duplicate its checks.

use std::sync::Arc;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{json, Value};

/// Permission tier shown to the agent (mirrors `cm_daemon::Tier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Free,
    Confirm,
}

/// One exposed tool.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub tier: Tier,
    pub mutating: bool,
    /// The tool changes or destroys user data (`delete_capture`).
    pub destructive: bool,
    /// Spec row (`CAP-1`, `ANN-3`, ...).
    pub spec: &'static str,
    pub summary: &'static str,
    pub input: Value,
    pub output: Value,
}

fn obj(v: Value) -> Arc<JsonObject> {
    match v {
        Value::Object(m) => Arc::new(m),
        _ => Arc::new(JsonObject::new()),
    }
}

impl ToolSpec {
    /// The MCP `Tool` definition: description with the tier, schemas, hints.
    pub fn to_tool(&self) -> Tool {
        let tier = match self.tier {
            Tier::Free if self.mutating => "Tier: Free (runs immediately; undoable where it edits a capture). The user can set this tool to Ask first in CaptureMann\u{2019}s settings: the call then waits up to 5 minutes for an on-screen confirmation and fails with `user_declined` (`data.reason` is `ask_first`) or `confirmation_timeout`; the daemon\u{2019}s `actions.list` reports the `effective_tier`.".to_owned(),
            Tier::Free => "Tier: Free (runs immediately; read-only, never asks).".to_owned(),
            Tier::Confirm => "Tier: Confirm (the user must approve on screen; the call waits up to 5 minutes, then fails with `user_declined` or `confirmation_timeout`; the app may also not support it, then it fails with `not_implemented`).".to_owned(),
        };
        let idem = if self.mutating {
            " Accepts an optional `request_id`: repeating it returns the first result instead of acting twice."
        } else {
            ""
        };
        let description = format!("{} {tier}{idem} Spec: {}.", self.summary, self.spec);
        let mut ann = ToolAnnotations::new()
            .read_only(!self.mutating)
            .open_world(false);
        if self.mutating {
            ann = ann.destructive(self.destructive).idempotent(false);
        }
        Tool::new(self.name, description, obj(self.input.clone()))
            .with_raw_output_schema(obj(self.output.clone()))
            .with_title(self.name.replace('_', " "))
            .with_annotations(ann)
    }
}

fn request_id() -> Value {
    json!({ "type": "string", "minLength": 1, "maxLength": 256,
            "description": "Idempotency key; a repeat within 24 h returns the first result." })
}

fn rgb() -> Value {
    json!({ "type": "string", "pattern": "^#[0-9A-Fa-f]{6}$", "description": "#RRGGBB" })
}

fn point() -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2,
            "description": "[x, y] in physical px of the original image" })
}

fn rect() -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4,
            "description": "[x, y, w, h] in physical px of the original image" })
}

/// The annotation op schema of ANN-1/ANN-2 (`op` is the tag; `id` is optional and assigned
/// by the daemon when missing).
pub fn op_schema() -> Value {
    let id = json!({ "type": "string", "description": "Optional unique op id." });
    let width = json!({ "type": "number", "exclusiveMinimum": 0, "default": 4 });
    let color = rgb();
    let variant = |name: &str, props: Value, required: &[&str]| {
        let mut p = props;
        if let Some(m) = p.as_object_mut() {
            m.insert("op".into(), json!({ "const": name }));
            m.insert("id".into(), id.clone());
        }
        let mut req = vec!["op"];
        req.extend_from_slice(required);
        json!({ "type": "object", "properties": p, "required": req })
    };
    json!({
        "oneOf": [
            variant("arrow", json!({ "from": point(), "to": point(), "color": color, "width": width }), &["from", "to"]),
            variant("line", json!({ "from": point(), "to": point(), "color": color, "width": width }), &["from", "to"]),
            variant("rect", json!({ "rect": rect(), "color": color, "width": width,
                                    "fill": rgb(), "radius": { "type": "number", "minimum": 0 } }), &["rect"]),
            variant("ellipse", json!({ "rect": rect(), "color": color, "width": width }), &["rect"]),
            variant("text", json!({ "at": point(), "text": { "type": "string" },
                                    "size": { "type": "number", "exclusiveMinimum": 0, "maximum": 2000, "default": 24 },
                                    "color": color, "outline": rgb(), "bg": rgb() }), &["at", "text"]),
            variant("highlight", json!({ "rect": rect(), "color": color,
                                         "opacity": { "type": "number", "minimum": 0, "maximum": 1, "default": 0.35 } }), &["rect"]),
            variant("step", json!({ "at": point(), "n": { "type": "integer", "minimum": 1 },
                                    "size": { "type": "number", "exclusiveMinimum": 0, "maximum": 2000, "default": 28 },
                                    "color": color }), &["at", "n"]),
            variant("blur", json!({ "rect": rect(), "strength": { "type": "integer", "minimum": 4, "maximum": 32, "default": 12 } }), &["rect"]),
            variant("pixelate", json!({ "rect": rect(), "block": { "type": "integer", "minimum": 6, "maximum": 32, "default": 12 } }), &["rect"]),
            variant("crop", json!({ "rect": rect() }), &["rect"]),
        ]
    })
}

fn image_ref() -> Value {
    json!({
        "type": ["object", "null"],
        "description": "The image itself is returned as an MCP image content block; this says which one.",
        "properties": {
            "mime_type": { "type": "string" },
            "content_index": { "type": "integer", "description": "Index in the result's `content` array." }
        },
        "required": ["mime_type", "content_index"]
    })
}

fn capture_id() -> Value {
    json!({ "type": "string", "minLength": 1, "description": "Capture id, e.g. from capture_screenshot or list_library." })
}

fn undo_redo_output() -> Value {
    json!({
        "type": "object",
        "properties": {
            "capture_id": { "type": "string" },
            "revision": { "type": ["integer", "null"], "description": "The new revision, or null when there was nothing to do." },
            "changed": { "type": "boolean" },
            "can_undo": { "type": "boolean" },
            "can_redo": { "type": "boolean" },
            "image": image_ref()
        },
        "required": ["capture_id", "revision", "changed", "can_undo", "can_redo"]
    })
}

fn time_schema(what: &str) -> Value {
    json!({ "description": format!("{what}, in session time: seconds (a number) or \"t:<ticks>\" (120,000 ticks per second)."),
            "anyOf": [ { "type": "number", "minimum": 0 }, { "type": "string", "pattern": "^t:[0-9]+$" } ] })
}

fn range_schema(what: &str) -> Value {
    json!({ "type": "array", "minItems": 2, "maxItems": 2, "items": time_schema("A time"),
            "description": format!("{what}: [start, end], end after start.") })
}

fn ranges_schema() -> Value {
    json!({ "type": "array", "minItems": 1, "items": range_schema("A range") })
}

fn edit_output() -> Value {
    json!({ "type": "object", "properties": {
        "capture_id": { "type": "string" },
        "revision": { "type": "integer", "description": "The new revision (one undo step)." },
        "action": { "type": "string" },
        "by": { "type": "string", "description": "`user` or `agent:<name>`." },
        "changed": { "type": "boolean" },
        "duration": { "type": "integer", "description": "Length of the export in ticks." },
        "duration_s": { "type": "number", "description": "Length of the export in seconds." },
        "edits": { "type": "object", "description": "The edit list now stored in the manifest (`in`, `out`, `cuts`, `splits`, `speeds`, `background`, `audio`, ticks)." },
        "can_undo": { "type": "boolean" },
        "can_redo": { "type": "boolean" } },
        "required": ["capture_id", "revision", "duration_s", "edits", "can_undo", "can_redo"] })
}

fn edit_tool(
    name: &'static str,
    summary: &'static str,
    properties: Value,
    required: &[&str],
) -> ToolSpec {
    let mut props = properties;
    if let Some(m) = props.as_object_mut() {
        m.insert("capture_id".into(), capture_id());
        m.insert("request_id".into(), request_id());
    }
    let mut req = vec!["capture_id"];
    req.extend_from_slice(required);
    ToolSpec {
        name,
        tier: Tier::Free,
        mutating: true,
        destructive: false,
        spec: "EDT-1, EDT-2, EDT-3",
        summary,
        input: json!({ "type": "object", "properties": props, "required": req, "additionalProperties": false }),
        output: edit_output(),
    }
}

/// Adds `anyOf: [{required: [a]}, ...]` so the schema says the call needs at least one of `keys`.
fn at_least_one_of(mut t: ToolSpec, keys: &[&str]) -> ToolSpec {
    t.input["anyOf"] = Value::Array(keys.iter().map(|k| json!({ "required": [k] })).collect());
    t
}

fn edit_tools() -> Vec<ToolSpec> {
    vec![
        edit_tool(
            "trim",
            "Keep only start..end of a recording (session time). Edits the edit list only; the recorded files are never changed. One undo step.",
            json!({ "start": time_schema("Start of the kept part"), "end": time_schema("End of the kept part") }),
            &["start", "end"],
        ),
        edit_tool(
            "cut_segments",
            "Cut ranges out of a recording; they are skipped in preview and export. All ranges in one call are one undo step; overlapping ranges merge and ranges outside the kept part are clamped. A cut leaving under 0.5 s fails with `invalid_input` (\"Nothing left to export\").",
            json!({ "ranges": ranges_schema() }),
            &["ranges"],
        ),
        edit_tool(
            "restore_segments",
            "Bring cut ranges back (a range may restore only part of a cut). One undo step.",
            json!({ "ranges": ranges_schema() }),
            &["ranges"],
        ),
        edit_tool(
            "split",
            "Add a segment boundary at a time; speed and layout settings apply per segment. One undo step.",
            json!({ "at": time_schema("Where to split") }),
            &["at"],
        ),
        edit_tool(
            "set_speed",
            "Play a range at 0.25x to 8x. The range edges become segment boundaries; factor 1 clears the speed there. Audio is muted above 2x unless `keep_audio` is true. One undo step.",
            json!({ "range": range_schema("The range"),
                    "factor": { "type": "number", "minimum": 0.25, "maximum": 8 },
                    "keep_audio": { "type": "boolean", "default": false } }),
            &["range", "factor"],
        ),
        edit_tool(
            "set_background",
            "Frame the screen on a background: none, a color, a two-stop gradient or an image, with padding, corner radius and shadow. Fields you leave out keep their value. One undo step.",
            json!({ "kind": { "type": "string", "enum": ["none", "color", "gradient", "image"] },
                    "value": { "description": "`#rrggbb` for color, [\"#rrggbb\", \"#rrggbb\"] for gradient, a file path for image." },
                    "padding_pct": { "type": "number", "minimum": 0, "maximum": 20, "description": "Percent of the frame width." },
                    "radius_px": { "type": "integer", "minimum": 0, "maximum": 24 },
                    "shadow": { "type": "boolean" } }),
            &["kind"],
        ),
        at_least_one_of(edit_tool(
            "set_audio_mix",
            "Balance the separate mic and system-audio tracks: gain in dB (-96 to +12), mute, and the denoised or raw mic file. Fields you leave out keep their value; give at least one. The export applies the mix with a -1 dBTP limiter. One undo step.",
            json!({ "mic_gain_db": { "type": "number", "minimum": -96, "maximum": 12 },
                    "system_gain_db": { "type": "number", "minimum": -96, "maximum": 12 },
                    "mic_mute": { "type": "boolean" },
                    "system_mute": { "type": "boolean" },
                    "denoise": { "type": "boolean", "description": "true: the denoised mic file; false: the raw one." } }),
            &[],
        ), &["mic_gain_db", "system_gain_db", "mic_mute", "system_mute", "denoise"]),
        edit_tool(
            "set_camera_layout",
            "Set how the screen and the camera share the frame over a range: `bubble` (screen with the camera bubble; the default), `screen` (camera hidden), `camera` (camera full frame) or `side_by_side` (screen 2/3, camera 1/3). For `bubble` you can also set `shape`, `rect` ([x, y, w, h] as fractions of the frame; w is the bubble width, the bubble is kept inside the frame), `ring` (width as a fraction of the bubble, or false for none) and `mirror`. `transition: smooth` moves from the layout before it over 300 ms; `cut` switches at once. A range overwrites the layouts it overlaps. A recording without a camera track accepts only `screen`. One undo step.",
            json!({ "range": range_schema("The range"),
                    "layout": { "type": "string", "enum": ["bubble", "screen", "camera", "side_by_side"] },
                    "shape": { "type": "string", "enum": ["circle", "rounded"], "description": "`bubble` only." },
                    "rect": { "type": "array", "minItems": 4, "maxItems": 4, "items": { "type": "number", "minimum": 0, "maximum": 1 },
                              "description": "`bubble` only: [x, y, w, h] as fractions of the frame, w from 0.05 to 1." },
                    "ring": { "description": "`bubble` only: a fraction of the bubble width (0 to 0.5), true for the default ring, false for none." },
                    "mirror": { "type": "boolean" },
                    "transition": { "type": "string", "enum": ["cut", "smooth"], "default": "cut" } }),
            &["range", "layout"],
        ),
        with_spec(edit_tool(
            "set_zooms",
            "Add zoom regions to a recording: each starts at `at`, lasts `length`, keeps `focus` (fractions of the frame) in the middle at `scale` 1.2 to 3, easing in and out over 400 ms; `follow_cursor` keeps the cursor in view. A new region shortens the ones it overlaps, and regions are clipped to the kept part and out of the cuts. `replace: true` swaps every existing zoom for the given list (an empty list clears them). One undo step.",
            json!({ "zooms": { "type": "array", "items": zoom_schema() },
                    "replace": { "type": "boolean", "default": false } }),
            &["zooms"],
        ), "EDT-4"),
        with_spec(at_least_one_of(edit_tool(
            "set_cursor_style",
            "Style the telemetry cursor of a recording made with \"Record cursor separately\": size 1x to 3x, smoothing, click effect and hide-when-idle (2 s). Has no effect on a recording whose cursor is baked into the video. Fields you leave out keep their value; give at least one. One undo step.",
            json!({ "size": { "type": "number", "minimum": 1, "maximum": 3 },
                    "smoothing": { "type": "string", "enum": ["off", "light", "strong"] },
                    "click_effect": { "type": "string", "enum": ["none", "ring", "ripple"] },
                    "hide_idle": { "type": "boolean" } }),
            &[],
        ), &["size", "smoothing", "click_effect", "hide_idle"]), "EDT-4"),
        with_spec(at_least_one_of(edit_tool(
            "set_reframe",
            "Reframe a 16:9 recording for a vertical (9:16) or square (1:1) export: `fit` letterboxes the whole screen, `fixed` crops a window centered on `focus` (per segment: `at` picks the segment), `follow` keeps the window on the cursor. The setting is stored per aspect, so the 16:9 export is unchanged. Give at least one of mode and focus. One undo step.",
            json!({ "aspect": { "type": "string", "enum": ["9:16", "1:1"] },
                    "mode": { "type": "string", "enum": ["fit", "fixed", "follow", "follow_cursor"], "description": "`follow_cursor` is the same as `follow`." },
                    "focus": point_schema("The window centre"),
                    "at": time_schema("A time inside the segment the focus is for (default: the first segment)") }),
            &["aspect"],
        ), &["mode", "focus"]), "EDT-5"),
        caption_options_tool(),
    ]
}

/// `set_caption_options` (AI-1): the show and burn-in switches of a recording's captions.
fn caption_options_tool() -> ToolSpec {
    let mut t = at_least_one_of(
        edit_tool(
            "set_caption_options",
            "Turn a recording's captions on or off. `enabled` shows the captions on the video in the editor's preview; `burn_in` burns them into exported videos by default (`export_video` can override it with `captions`). Give at least one; a value that is already set changes nothing and records no revision. The caption lines are untouched. One undo step.",
            json!({ "enabled": { "type": "boolean", "description": "Show the captions in the editor's preview." },
                    "burn_in": { "type": "boolean", "description": "Burn the captions into exported videos by default." } }),
            &[],
        ),
        &["enabled", "burn_in"],
    );
    t.spec = "AI-1";
    t.output = json!({ "type": "object", "properties": {
        "capture_id": { "type": "string" },
        "enabled": { "type": "boolean" }, "burn_in": { "type": "boolean" },
        "changed": { "type": "boolean" },
        "revision": { "type": "integer" }, "action": { "type": "string" }, "by": { "type": "string" },
        "can_undo": { "type": "boolean" }, "can_redo": { "type": "boolean" } },
        "required": ["capture_id", "enabled", "burn_in", "changed"] });
    t
}

fn with_spec(mut t: ToolSpec, spec: &'static str) -> ToolSpec {
    t.spec = spec;
    t
}

fn point_schema(what: &str) -> Value {
    json!({ "type": "array", "minItems": 2, "maxItems": 2,
            "items": { "type": "number", "minimum": 0, "maximum": 1 },
            "description": format!("{what}: [x, y] as fractions of the frame, y down.") })
}

fn zoom_schema() -> Value {
    json!({ "type": "object", "properties": {
        "at": time_schema("When the zoom starts"),
        "length": time_schema("How long it lasts (at least 0.8 s)"),
        "focus": point_schema("The point kept in the middle"),
        "scale": { "type": "number", "minimum": 1.2, "maximum": 3, "default": 1.8 },
        "follow_cursor": { "type": "boolean", "default": false } },
        "required": ["at", "length", "focus"], "additionalProperties": false })
}

fn job_id() -> Value {
    json!({ "type": "string", "minLength": 1, "description": "Job id, e.g. from export_video or upload." })
}

/// The job object returned by `get_job`, `cancel_job` and (in a list) `list_jobs`. The
/// resumable checkpoint is never part of it.
fn job_schema() -> Value {
    json!({ "type": "object", "properties": {
        "job_id": { "type": "string" },
        "type": { "type": "string", "description": "`export`, `upload`, `transcribe`, `publish`, ..." },
        "state": { "type": "string", "enum": ["queued", "running", "paused", "failed", "done", "cancelled"] },
        "progress": { "type": "number", "minimum": 0, "maximum": 1 },
        "message": { "type": ["string", "null"] },
        "attempts": { "type": "integer" },
        "depends_on": { "type": "array", "items": { "type": "string" } },
        "inputs": {},
        "result": { "description": "Set once `done` (e.g. `{ path, size_bytes }`)." },
        "error": { "type": ["object", "null"], "description": "Last error: `{ code, reason }`.",
                   "properties": { "code": { "type": "string" }, "reason": { "type": "string" } } },
        "pause_reason": { "type": ["string", "null"], "description": "`user`, or `hold:*` while a recording runs." },
        "created_at": { "type": "string" },
        "started_at": { "type": ["string", "null"] },
        "finished_at": { "type": ["string", "null"] },
        "elapsed_ms": { "type": ["integer", "null"] },
        "eta_ms": { "type": ["integer", "null"] } },
        "required": ["job_id", "type", "state", "progress"] })
}

fn recording_ref_input() -> Value {
    json!({ "type": "object", "properties": {
        "recording_id": { "type": "string", "minLength": 1,
                          "description": "From start_recording; optional while only one recording exists." },
        "request_id": request_id() },
        "additionalProperties": false })
}

/// Every tool this build exposes, in the order of the spec's table.
pub fn all() -> Vec<ToolSpec> {
    let displays_windows = json!({
        "type": "object",
        "properties": {
            "displays": { "type": "array", "items": { "type": "object", "properties": {
                "id": { "type": "integer" }, "name": { "type": "string" },
                "bounds": { "type": "object" }, "scale": {}, "is_primary": { "type": "boolean" },
                "is_builtin": { "type": "boolean" } },
                "required": ["id", "bounds", "scale"] } },
            "windows": { "type": "array", "items": { "type": "object", "properties": {
                "id": { "type": "integer" }, "title": { "type": "string" }, "app": { "type": "string" },
                "pid": { "type": "integer" }, "bounds": { "type": "object" }, "scale": {},
                "display_id": { "type": ["integer", "null"] }, "minimized": { "type": "boolean" } },
                "required": ["id", "title", "app", "bounds", "scale"] } }
        },
        "required": ["displays", "windows"]
    });
    let settings_out = json!({
        "type": "object",
        "properties": { "settings": { "type": "object", "description": "Effective settings by dotted key." } },
        "required": ["settings"]
    });
    let mut tools = vec![
        ToolSpec {
            name: "list_targets",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "CAP-1",
            summary: "List the displays and windows that can be captured (ids, titles, apps, bounds in physical px, scale). Pass windows=false for displays only (much faster).",
            input: json!({
                "type": "object",
                "properties": { "windows": { "type": "boolean", "default": true,
                    "description": "Include the window list. false returns displays only and an empty `windows` array, without enumerating windows." } },
                "additionalProperties": false
            }),
            output: displays_windows,
        },
        ToolSpec {
            name: "get_settings",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "settings",
            summary: "Read effective settings (dotted keys such as `recording.fps`). Secrets are never available.",
            input: json!({ "type": "object", "properties": {
                "keys": { "type": "array", "items": { "type": "string" }, "description": "Keys to read; omit for all." } },
                "additionalProperties": false }),
            output: settings_out.clone(),
        },
        ToolSpec {
            name: "set_settings",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "settings",
            summary: "Change settings by dotted key; returns the effective settings. Keys that move or weaken protected state (library.path, privacy.*, shortcuts.*, permissions.*, agents.*, storage.*, export.*) can only be changed in the app and are refused with user_declined (data.reason app_only_setting); keys containing token, secret, password or api_key are never available. At most 64 keys and 16 KiB per value.",
            input: json!({ "type": "object", "properties": {
                "values": { "type": "object", "minProperties": 1, "description": "Map of dotted key to new value." },
                "request_id": request_id() },
                "required": ["values"], "additionalProperties": false }),
            output: settings_out,
        },
        ToolSpec {
            name: "capture_screenshot",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "CAP-1, CAP-4, CAP-5",
            summary: "Take a screenshot of a display, a window, or a region of a display and store it in the library. Returns the capture id, file path and the image. A region `rect` is [x, y, w, h] in physical px relative to the display. No overlay is shown.",
            input: json!({ "type": "object", "properties": {
                "target": { "oneOf": [
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 } },
                      "required": ["display_id"], "additionalProperties": false },
                    { "type": "object", "properties": { "window_id": { "type": "integer", "minimum": 0 } },
                      "required": ["window_id"], "additionalProperties": false },
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 },
                        "rect": { "type": "array", "items": { "type": "integer" }, "minItems": 4, "maxItems": 4,
                                  "description": "[x, y, w, h] physical px relative to the display" } },
                      "required": ["display_id", "rect"], "additionalProperties": false } ] },
                "delay_s": { "type": "number", "minimum": 0, "maximum": 30, "description": "Seconds to wait first." },
                "copy_to_clipboard": { "type": "boolean", "default": false },
                "title": { "type": "string", "description": "Library title; defaults to `Screenshot <date> <time>`." },
                "request_id": request_id() },
                "required": ["target"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "path": { "type": "string" },
                "kind": { "type": "string", "enum": ["display", "window", "region"] },
                "width": { "type": "integer" }, "height": { "type": "integer" },
                "copied_to_clipboard": { "type": "boolean" },
                "warnings": { "type": "array", "items": { "type": "string" } },
                "image": image_ref() },
                "required": ["capture_id", "path", "width", "height", "image"] }),
        },
        ToolSpec {
            name: "annotate",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "ANN-1, ANN-2",
            summary: "Add annotation ops (arrow, line, rect, ellipse, text, highlight, step, blur, pixelate, crop) to a screenshot. One call is one revision and one undo step. Coordinates are physical px of the original image. Returns the revision and the rendered image.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "ops": { "type": "array", "items": op_schema(), "description": "The ops to add. Empty is allowed only with `replace: true`, which clears every annotation (revert to the original)." },
                "replace": { "type": "boolean", "default": false, "description": "Clear the existing ops first (still one undo step)." },
                "copy_to_clipboard": { "type": "boolean", "default": false, "description": "Copy the rendered image (annotations and redactions applied, never the original) to the clipboard once the revision is saved." },
                "return_image": { "type": "boolean", "default": true, "description": "Return the rendered image. Set false when you do not need it (the result then has `image: null`)." },
                "request_id": request_id() },
                "required": ["capture_id", "ops"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "revision": { "type": "integer" },
                "by": { "type": "string", "description": "`agent:<client>`" },
                "copied_to_clipboard": { "type": "boolean" },
                "warnings": { "type": "array", "items": { "type": "string" } }, "image": image_ref() },
                "required": ["capture_id", "revision", "by"] }),
        },
        ToolSpec {
            name: "get_capture",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "ANN-3",
            summary: "Read a capture: metadata, tracks, uploads, current annotation ops, revisions, and the current image (annotations rendered).",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "revision": { "type": "integer", "minimum": 0, "description": "Only the current revision is supported." },
                "include_image": { "type": "boolean", "default": true } },
                "required": ["capture_id"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "title": { "type": "string" }, "kind": { "type": "string" },
                "created": { "type": "string" }, "path": { "type": "string" },
                "tracks": { "type": "array", "items": { "type": "object" } },
                "uploads": { "type": "array", "items": { "type": "object" } },
                "ops": { "type": "array", "items": { "type": "object" } },
                "revision": { "type": "integer" },
                "revisions": { "type": "array", "items": { "type": "object" } },
                "can_undo": { "type": "boolean" }, "can_redo": { "type": "boolean" },
                "warnings": { "type": "array", "items": { "type": "string" } },
                "image": image_ref() },
                "required": ["capture_id", "revision", "ops", "can_undo", "can_redo"] }),
        },
        ToolSpec {
            name: "suggest_redactions",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "ANN-2",
            summary: "Find emails, API keys and other long tokens (and, with lower confidence, phone numbers and IPv4 addresses) in a screenshot with on-device text recognition and propose blur rectangles. Nothing is applied and nothing is sent anywhere: pass the rectangles you accept to `annotate` as `blur` ops (one call, one undo step). Rectangles are physical px of the original image, padded around the text. `text_preview` is masked (first and last two characters) so the secret is not repeated. Fails with `not_implemented` when this computer has no text recognizer.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "kinds": { "type": "array", "uniqueItems": true,
                    "items": { "type": "string", "enum": ["email", "token", "ip_address", "phone"] },
                    "description": "Only look for these kinds. Default: all." } },
                "required": ["capture_id"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" },
                "recognizer": { "type": "string", "description": "`apple_vision` on macOS." },
                "image_size": { "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 },
                "rects": { "type": "array", "items": { "type": "object", "properties": {
                    "rect": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4,
                              "description": "[x, y, w, h] in physical px of the original image." },
                    "kind": { "type": "string", "enum": ["email", "token", "ip_address", "phone"] },
                    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                    "text_preview": { "type": "string" } },
                    "required": ["rect", "kind", "confidence"] } },
                "applied": { "type": "boolean", "description": "Always false: nothing is applied." } },
                "required": ["capture_id", "rects", "applied"] }),
        },
        ToolSpec {
            name: "undo",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "ANN-3, AGT-3",
            summary: "Undo the last revision of a capture (one step reverts a whole earlier call). With nothing to undo it returns `changed: false`.",
            input: json!({ "type": "object", "properties": { "capture_id": capture_id(), "request_id": request_id() },
                "required": ["capture_id"], "additionalProperties": false }),
            output: undo_redo_output(),
        },
        ToolSpec {
            name: "redo",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "ANN-3, AGT-3",
            summary: "Redo the last undone revision of a capture. With nothing to redo it returns `changed: false`.",
            input: json!({ "type": "object", "properties": { "capture_id": capture_id(), "request_id": request_id() },
                "required": ["capture_id"], "additionalProperties": false }),
            output: undo_redo_output(),
        },
        ToolSpec {
            name: "list_library",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "LIB-1",
            summary: "List or search captures in the library, newest first, with paging.",
            input: json!({ "type": "object", "properties": {
                "query": { "type": "string", "description": "Full-text search." },
                "kind": { "type": "string", "enum": ["screenshot", "recording", "audio"], "description": "`audio` lists audio-only recordings (VOC-1); `recording` lists screen recordings." },
                "status": { "type": "string", "enum": ["uploaded", "not_uploaded", "published", "not_published"], "description": "Only captures with (or without) an upload record (`uploaded`: sent to a storage destination) or a publish record (`published`: on YouTube and the like). Combines with `kind` and `query`; paging stays correct." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 },
                "cursor": { "type": "string", "description": "`next_cursor` of the previous page." } },
                "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "captures": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" }, "title": { "type": "string" }, "kind": { "type": "string" },
                    "created": { "type": "string" }, "duration": {}, "width": {}, "height": {},
                    "size_bytes": {}, "broken": { "type": "boolean" } },
                    "required": ["id", "title", "kind", "created"] } },
                "next_cursor": { "type": ["string", "null"] } },
                "required": ["captures", "next_cursor"] }),
        },
        ToolSpec {
            name: "rename_capture",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "LIB-1",
            summary: "Rename a capture: sets its title and renames its library folder to match. Not an undo step. Refused with `busy` while the capture is being recorded or exported.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "title": { "type": "string", "minLength": 1, "description": "The new title." },
                "request_id": request_id() },
                "required": ["capture_id", "title"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "title": { "type": "string" },
                "path": { "type": "string", "description": "The capture's new folder." },
                "kind": { "type": "string" } },
                "required": ["capture_id", "title", "path"] }),
        },
        ToolSpec {
            name: "list_destinations",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "STO-1",
            summary: "List the destinations the user set up in Settings (a folder on this computer, a drive or a network share, or a connected Google Drive, OneDrive or Dropbox account): id, name, kind (`local`, `google_drive`, `onedrive` or `dropbox`), path (empty for a cloud account), folder pattern, what is sent, `auto_upload` (`off`, `on_stop` or `on_export`), `default` and `online` (whether the folder is reachable now; a cloud account counts as online, and a lost sign-in shows as an `upload` job paused with `auth_expired`). A cloud destination also has `account` with its `label` (the email), `folder` and `link_mode` (`anyone`, `organization` or `none`); never a token or key. Agents cannot add or change destinations.",
            input: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "destinations": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" }, "name": { "type": "string" }, "kind": { "type": "string" },
                    "path": { "type": "string" }, "folder_pattern": { "type": "string" },
                    "what": { "type": "string", "enum": ["export", "original", "both"] },
                    "auto_upload": { "type": "string", "enum": ["off", "on_stop", "on_export"] }, "default": { "type": "boolean" },
                    "online": { "type": "boolean" } },
                    "required": ["id", "name", "kind", "path", "online"] } } },
                "required": ["destinations"] }),
        },
        ToolSpec {
            name: "upload",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "STO-1, STO-3, STO-5",
            summary: "Copy a capture (or one file) to a destination in the background. Give `capture_id` or `file`. Returns a `job_id` at once; watch it with get_job or the `job.*` events (job type `upload`, progress by bytes, resumable after a restart). If the destination is offline the job waits and does not fail. Nothing is deleted or published. `destination_id` defaults to the default destination; `folder` replaces its folder pattern for this upload; `what` (`export`, `original` or `both`) applies to a capture only, and a screenshot only has `original`.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "file": { "type": "string", "minLength": 1, "description": "Absolute path of a file to copy instead of a capture." },
                "destination_id": { "type": "string", "description": "A destination id from list_destinations; the default one when absent." },
                "folder": { "type": "string", "minLength": 1, "description": "Folder inside the destination; replaces its folder pattern." },
                "what": { "type": "string", "enum": ["export", "original", "both"], "description": "What to send of a capture; the destination's own choice when absent." },
                "request_id": request_id() },
                "anyOf": [{ "required": ["capture_id"] }, { "required": ["file"] }],
                "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "job_id": { "type": "string", "description": "Watch it with get_job or the `job.*` events." },
                "destination_id": { "type": ["string", "null"] },
                "what": { "type": ["string", "null"] },
                "state": { "type": "string" } },
                "required": ["job_id", "state"] }),
        },
        ToolSpec {
            name: "get_job",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "EXP-4, STO-3",
            summary: "Read one background job (export, upload, transcribe, publish): state, progress 0..1, elapsed, ETA, result or error. Progress also arrives as `job.progress`, `job.done` and `job.failed` events.",
            input: json!({ "type": "object", "properties": { "job_id": job_id() },
                "required": ["job_id"], "additionalProperties": false }),
            output: job_schema(),
        },
        ToolSpec {
            name: "list_jobs",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "EXP-4, STO-3",
            summary: "List background jobs, newest first, optionally filtered by state and type.",
            input: json!({ "type": "object", "properties": {
                "state": { "type": "string", "enum": ["queued", "running", "paused", "failed", "done", "cancelled"] },
                "type": { "type": "string", "description": "Job type, e.g. `upload`." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 } },
                "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "jobs": { "type": "array", "items": job_schema() } },
                "required": ["jobs"] }),
        },
        ToolSpec {
            name: "cancel_job",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "EXP-4, STO-3",
            summary: "Cancel a job. A queued or paused job is cancelled at once; a running one is told to stop (`cancelling: true`) and reaches `cancelled` shortly after, with its partial output deleted. Cancelling a cancelled job changes nothing; a done or failed job cannot be cancelled. No `job.*` event is emitted for cancellation: read `get_job`.",
            input: json!({ "type": "object", "properties": { "job_id": job_id(), "request_id": request_id() },
                "required": ["job_id"], "additionalProperties": false }),
            output: json!({ "type": "object", "description": "The job, plus `cancelling`.",
                "properties": { "job_id": { "type": "string" }, "state": { "type": "string" },
                                "cancelling": { "type": "boolean", "description": "true while a running job is still stopping." } },
                "required": ["job_id", "state", "cancelling"] }),
        },
        ToolSpec {
            name: "retry_job",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "STO-3, EXP-4",
            summary: "Try a job again. A failed job is queued again with its attempts reset and its saved progress kept (an upload continues from the last confirmed chunk); a cancelled job starts over; a job waiting with `auth_expired` continues (the account must be connected again first, in the app, otherwise it pauses again). A job that is already queued or running is returned unchanged, so a repeated call is harmless; a done job, or one the user paused, fails with `invalid_input`. Returns the job.",
            input: json!({ "type": "object", "properties": { "job_id": job_id(), "request_id": request_id() },
                "required": ["job_id"], "additionalProperties": false }),
            output: json!({ "type": "object", "description": "The job after the retry.",
                "properties": { "job_id": { "type": "string" }, "state": { "type": "string" } },
                "required": ["job_id", "state"] }),
        },
        ToolSpec {
            name: "export_video",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "EXP-1, EXP-4, REC-1",
            summary: "Render a recording to one combined MP4 (screen, round camera bubble with the rec ring, microphone and system audio) in the capture's `exports/` folder, in the background. Returns a `job_id` at once; watch it with get_job or the `job.progress` / `job.done` events (`export.done` carries the file). The result of the finished job is `{ path, size_bytes, duration_s, file, encoder }`. The masters are never changed. The export applies the edits (trim, cuts, speed, background) and the audio mix (`set_audio_mix` gains and mutes), and can normalise the loudness (`loudness_lufs`). `preset` is `quick` (the default: the recording's size, H.264/AAC), a platform preset (`youtube-4k`, `youtube-1440p`, `youtube-1080p`, `vertical-1080` for Shorts/TikTok/Reels, `square-1080`, `source`; fixed frame, platform bitrate, audio at -14 LUFS; `youtube-4k` is refused for a recording smaller than 2160p) or `gif` / `webm` (`format`, `width`, `fps`, `max_mb`; a GIF is at most 30 s). The custom size/codec/quality/captions form fails with `invalid_input` (\"not available yet\"). The bubble uses its layout at the end of the recording. A recording that stopped normally is exported automatically unless the setting `export.auto_on_stop` is off. Needs FFmpeg (`brew install ffmpeg`); without it the job fails with `not_found`. Fails with `not_implemented` if the app has no exporter.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "preset": { "type": "string", "default": "quick",
                            "description": "`quick`, `youtube-4k`, `youtube-1440p`, `youtube-1080p`, `vertical-1080`, `square-1080`, `source`, `gif` or `webm` (the preset table can be extended with a rules file). Unknown ids fail with `invalid_input`." },
                "format": { "type": "string", "enum": ["mp4", "gif", "webm"],
                            "description": "`gif` or `webm` is the same as that preset. Default: MP4." },
                "width": { "type": "integer", "minimum": 16, "maximum": 7680,
                           "description": "GIF (480, 640, 800 or 1024; default 800) and WebM (even width; default 1280) only." },
                "fps": { "type": "number", "minimum": 1, "maximum": 60,
                         "description": "GIF (10, 15 or 24; default 15) and WebM (default 30) only." },
                "max_mb": { "type": "number", "exclusiveMinimum": 0, "maximum": 1000,
                            "description": "GIF and WebM only: \"fit under N MB\" (10^6 bytes). A GIF steps the width and then the frame rate down, a WebM lowers the bitrate; fails if it cannot fit." },
                "loudness_lufs": { "type": "number", "minimum": -40, "maximum": -5,
                                   "description": "Normalise the integrated loudness to this level in LUFS (AUD-4; -14 is the usual platform target) with a -1 dBTP true-peak limiter. Omit to keep the recorded level." },
                "captions": { "type": "string", "enum": ["burn", "none"],
                              "description": "`burn` burns the captions into the picture (white text on a dark box inside the platform's safe area; the user's edited lines, else the transcript, with the edits applied) and fails with `not_found` before `transcribe` has run; `none` leaves them out. Omit to follow the recording's `set_caption_options` burn_in." },
                "request_id": request_id() },
                "required": ["capture_id"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "job_id": { "type": "string" },
                "capture_id": { "type": "string" },
                "preset": { "type": "string" },
                "loudness_lufs": { "type": ["number", "null"] },
                "captions": { "type": ["string", "null"], "enum": ["burn", "none", null] },
                "state": { "type": "string" },
                "reused": { "type": "boolean", "description": "true when an export of this capture, preset, loudness target and caption choice was already queued or running." } },
                "required": ["job_id"] }),
        },
        ToolSpec {
            name: "export_audio",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "VOC-2",
            summary: "Export the audio of a capture (an audio recording, or \"Export audio only\" of a screen recording) as M4A (AAC-LC, 48 kHz, 128 kbps mono or 192 kbps stereo; the default) or WAV (48 kHz, 24-bit), in the background. Returns a `job_id` at once; watch it with get_job or the `job.progress` / `job.done` events (`export.done` carries the file). The result of the finished job is `{ path, size_bytes, duration_s, file, format, tracks, loudness_lufs, copy_path }`. The edits apply to all audio tracks together (trim, cuts, splits, speed, pauses removed, and the `set_audio_mix` gains and mutes); no picture is read or encoded. `tracks` is `mix` (the default), `mic` or `system`. M4A is normalised to -16 LUFS by default (two-pass loudnorm, -1.5 dBTP); WAV is left as recorded; `loudness_lufs` sets another target and `null` turns the normalisation off. The title and date are written as file tags. With `out_dir` (an existing absolute folder) a copy is also made there, under a name that never overwrites a file. Runs as an `export` job, one at a time, held while a recording runs. Fails with `invalid_input` when the capture has no such audio track or nothing is left after the cuts. Needs FFmpeg with the `wav` muxer and `pcm_s24le` encoder for WAV and the `loudnorm` filter for normalisation.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "format": { "type": "string", "enum": ["m4a", "wav"], "default": "m4a" },
                "tracks": { "type": "string", "enum": ["mix", "mic", "system"], "default": "mix",
                            "description": "`mix` sums the microphone and the system audio, `mic` and `system` export one of them." },
                "loudness_lufs": { "type": ["number", "null"], "minimum": -40, "maximum": -5,
                                   "description": "Integrated loudness target in LUFS. Default: -16 for M4A, none for WAV. `null` turns it off." },
                "out_dir": { "type": "string", "description": "Absolute path of an existing folder to also copy the file to." },
                "request_id": request_id() },
                "required": ["capture_id"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "job_id": { "type": "string" },
                "capture_id": { "type": "string" },
                "kind": { "type": "string", "enum": ["audio"] },
                "format": { "type": "string", "enum": ["m4a", "wav"] },
                "tracks": { "type": "string", "enum": ["mix", "mic", "system"] },
                "loudness_lufs": { "type": ["number", "null"] },
                "out_dir": { "type": ["string", "null"] },
                "state": { "type": "string" },
                "reused": { "type": "boolean", "description": "true when an equal audio export of this capture was already queued or running." } },
                "required": ["job_id"] }),
        },
        ToolSpec {
            name: "start_recording",
            tier: Tier::Confirm,
            mutating: true,
            destructive: false,
            spec: "REC-1-4, CAM-1, AUD-1, AUD-2, AGT-2, VOC-1",
            summary: "Start a screen recording of a display, a window or a region (physical px relative to the display, like capture_screenshot), optionally with the webcam, the microphone and system audio. With `mode: \"audio\"` it records the microphone and/or system audio only (no screen, camera or cursor; `kind: \"audio\"` in the library): `target`, `size`, `fps`, `codec`, `quality`, `camera`, `cursor`, `cursor_separate` and `exclude` are then refused with `invalid_input` naming the field. The microphone is on unless `mic: false`; `preset` sets the default sources (`online` and `product` add system audio). The dialog says \"Record audio\", names the microphone and carries the line \"Let everyone know they're being recorded.\". A microphone-only recording never needs Screen Recording access; system audio without it fails with `permission_missing` (data.permission: screen). It stops after `max_duration_s` (default 4 h, at most 12 h), or with reason `system_sleep` when the computer sleeps. The user sees a confirmation that says exactly what will be recorded (unless they turned that off for AI agents in CaptureMann\u{2019}s settings; the recording indicator and a Stop button on the screen always show), then a countdown (default 3 s), and the recording shows the usual border, timer and stop button. Returns when recording has started, with the recording id, the capture id it will have in the library and the indicator facts. Fails with `busy` if a recording is running and `permission_missing` (data.permission: screen, microphone or camera) before anything is asked. Stop it with stop_recording.",
            input: json!({ "type": "object", "properties": {
                "mode": { "type": "string", "enum": ["screen", "audio"], "default": "screen", "description": "`audio`: microphone and/or system audio only (see above)." },
                "preset": { "type": "string", "enum": ["room", "online", "lecture", "product", "note"], "description": "Only with `mode: \"audio\"`: the use case. `room` (interview, 1:1, meeting around a laptop), `lecture` and `note` record the microphone; `online` (call) and `product` (demo call) add system audio. Every source can still be set." },
                "people": { "type": "integer", "minimum": 1, "maximum": 8, "description": "Only with `mode: \"audio\"`: how many people speak, when known. Stored with the recording; the automatic speaker step (setting `ai.auto_speakers`, on by default) uses it as the speaker count. Absent = not known (the preset decides: 2 for `room`, 3 otherwise; `note` skips the speaker step)." },
                "target": { "description": "Required for a screen recording; refused with `mode: \"audio\"`.", "oneOf": [
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 } },
                      "required": ["display_id"], "additionalProperties": false },
                    { "type": "object", "properties": { "window_id": { "type": "integer", "minimum": 0 } },
                      "required": ["window_id"], "additionalProperties": false, "description": "Follows the window; its size is fixed at start." },
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 },
                        "rect": { "type": "array", "items": { "type": "integer" }, "minItems": 4, "maxItems": 4,
                                  "description": "[x, y, w, h] physical px relative to the display" } },
                      "required": ["display_id", "rect"], "additionalProperties": false } ] },
                "size": { "type": "string", "enum": ["source", "2160p", "1440p", "1080p", "720p"], "description": "Output size; default `source` (native, no cap) or the `recording.size` setting." },
                "fps": { "type": "integer", "enum": [30, 60], "description": "Default 30 or the `recording.fps` setting. 60 is accepted only after a passing `run_fps_selftest` for this display, size and codec (valid 30 days); otherwise `invalid_input` with `data.suggestion` (30 fps, a lower size, or run the self-test). A default of 60 from the setting falls back to 30 with a warning." },
                "codec": { "type": "string", "enum": ["auto", "hevc", "h264"], "description": "`auto`: HEVC above 1920 px wide, else H.264." },
                "quality": { "type": "string", "enum": ["high", "balanced", "small"] },
                "camera": { "description": "Record the webcam: false (default), true (the camera set in Settings > Webcam & audio (`camera.device`) when it is connected, else the system default camera), a device id, or an object. The bubble facts (shape, size, position) are stored and shown by the app; the camera is its own track.",
                    "oneOf": [
                        { "type": "boolean" },
                        { "type": "string", "description": "A camera id from list_devices, `default` (same as true) or `off`." },
                        { "type": "object", "properties": {
                            "on": { "type": "boolean", "default": true },
                            "device_id": { "type": "string" },
                            "resolution": { "type": "string", "enum": ["720p", "1080p"], "default": "1080p" },
                            "mirror": { "type": "boolean", "default": true, "description": "Display flag only; the stored pixels are never flipped." },
                            "shape": { "type": "string", "enum": ["circle", "rounded"], "default": "circle" },
                            "size": { "type": "string", "enum": ["s", "m", "l"], "default": "m" },
                            "position": { "oneOf": [
                                { "type": "string", "enum": ["top-left", "top", "top-right", "left", "right", "bottom-left", "bottom", "bottom-right"] },
                                { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 1 }, "minItems": 2, "maxItems": 2 } ],
                                "default": "bottom-left" } },
                          "additionalProperties": false } ] },
                "mic": { "description": "Record the microphone: false (default), true or `default` (the microphone set in Settings > Webcam & audio (`audio.mic_device`) when it is connected, else the system default input), a device id from list_devices, or an object.",
                    "oneOf": [
                        { "type": "boolean" },
                        { "type": "string" },
                        { "type": "object", "properties": {
                            "on": { "type": "boolean", "default": true },
                            "device_id": { "type": "string" },
                            "denoise": { "type": "boolean", "description": "Also write the noise-suppressed track (default: the `audio.noise_suppression` setting, on)." } },
                          "additionalProperties": false } ] },
                "system_audio": { "type": "boolean", "default": false },
                "cursor": { "type": "boolean", "default": true, "description": "Record cursor telemetry (positions and clicks, never keystrokes). The pointer stays in the video unless `cursor_separate` is true." },
                "cursor_separate": { "type": "boolean", "default": false, "description": "Record the cursor separately (EDT-4): the pointer is left out of the screen video and drawn from the telemetry in the editor and the export, so it can be made bigger, smoother and given click effects. Needs `cursor` on; default the `recording.cursor_separate` setting." },
                "max_duration_s": { "type": "number", "exclusiveMinimum": 0, "maximum": 86400, "description": "Stop automatically after this much recorded time (pauses do not count)." },
                "countdown_s": { "type": "integer", "minimum": 0, "maximum": 10, "description": "Countdown before recording; default the `recording.countdown_s` setting (3). stop_recording during it cancels the start." },
                "countdown": { "type": "integer", "minimum": 0, "maximum": 10, "description": "Alias of `countdown_s`." },
                "title": { "type": "string", "description": "Library title; defaults to `Recording <date> <time>`." },
                "exclude": { "type": "object", "properties": {
                    "bundle_ids": { "type": "array", "items": { "type": "string" }, "description": "Apps kept out of the recording, in addition to CaptureMann's own windows and the default password managers (which can never be removed)." },
                    "window_ids": { "type": "array", "items": { "type": "integer", "minimum": 0 } } },
                    "additionalProperties": false },
                "request_id": request_id() },
                "anyOf": [ { "required": ["target"] }, { "required": ["mode"] } ],
                "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string", "description": "Identifies this run; pass it to the other recording tools (optional while only one runs)." },
                "capture_id": { "type": "string", "description": "The library id the recording has once stopped." },
                "title": { "type": "string" },
                "state": { "type": "string", "enum": ["recording"] },
                "live_at_ms": { "type": "integer", "description": "Session time at which every track was delivering; the timer starts here." },
                "started_at": { "type": "string" },
                "by": { "type": "string" },
                "countdown_s": { "type": "integer" },
                "mode": { "type": "string", "enum": ["screen", "audio"] },
                "tracks": { "type": "array", "items": { "type": "string", "enum": ["screen", "cursor", "mic", "system", "camera"] }, "description": "Tracks that actually started; an optional track that failed is missing and listed in `warnings`. An audio recording lists only `mic` and/or `system`." },
                "warnings": { "type": "array", "items": { "type": "object", "properties": {
                    "track": { "type": ["string", "null"] }, "code": { "type": "string" }, "message": { "type": "string" } } } },
                "indicator": { "type": "object", "description": "What the on-screen indicator shows: area (kind, display_id, bounds), output size/fps/codec, camera bubble, sensors, who started it." },
                "max_duration_s": { "type": ["number", "null"] },
                "restart_of": { "type": ["string", "null"] } },
                "required": ["recording_id", "capture_id", "state", "tracks"] }),
        },
        ToolSpec {
            name: "pause_recording",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "REC-4",
            summary: "Pause the running recording: every track stops writing and the timer stops; resume continues in the same files with no gap. Pausing during the countdown cancels the start. Pausing a paused recording changes nothing.",
            input: recording_ref_input(),
            output: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string" },
                "state": { "type": "string", "enum": ["paused"] },
                "changed": { "type": "boolean" },
                "recorded_ms": { "type": "integer", "description": "What the timer shows." },
                "cancelled": { "type": "boolean", "description": "True when it cancelled a countdown." } },
                "required": ["recording_id"] }),
        },
        ToolSpec {
            name: "resume_recording",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "REC-4",
            summary: "Resume a paused recording after the resume countdown (default 1 s, the `recording.resume_countdown_s` setting). Resuming a running recording changes nothing.",
            input: recording_ref_input(),
            output: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string" },
                "state": { "type": "string", "enum": ["recording", "paused"] },
                "changed": { "type": "boolean" },
                "paused_ms": { "type": "integer" } },
                "required": ["recording_id"] }),
        },
        ToolSpec {
            name: "set_mic_muted",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "AUD-6",
            summary: "Mute or unmute the microphone of the running recording (also while paused). The mic track keeps running, so sync and the clock are untouched; while muted the mic files hold digital silence, and the muted spans are listed in the manifest (`mic_muted`, session time). Works only on a recording that has a microphone track. Muting a muted mic changes nothing (`changed: false`).",
            input: json!({ "type": "object", "properties": {
                "muted": { "type": "boolean", "description": "true mutes, false unmutes." },
                "recording_id": { "type": "string", "minLength": 1,
                                  "description": "From start_recording; optional while only one recording exists." },
                "request_id": request_id() },
                "required": ["muted"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string" },
                "muted": { "type": "boolean", "description": "The state now." },
                "changed": { "type": "boolean" },
                "at_ms": { "type": "integer", "description": "Session time of the change (of the last one when nothing changed)." } },
                "required": ["recording_id", "muted", "changed"] }),
        },
        ToolSpec {
            name: "add_marker",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "VOC-1",
            summary: "Add a marker (a moment worth coming back to, with an optional label) to the running recording at the current recorded time, or to a finished recording at `at`. Markers are stored in `manifest.markers` in recorded time (the timer: pauses do not count), so a marker set at 02:00 stays at 120 s after a pause. On a finished recording it is one revision and one undo step. Give nothing but `label` (and optionally `recording_id`) for the running recording; give `capture_id` and `at` (seconds) for a finished one.",
            input: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string", "minLength": 1,
                                  "description": "From start_recording; optional while only one recording exists. Not with `capture_id`." },
                "capture_id": { "type": "string", "minLength": 1, "description": "A finished recording; needs `at`." },
                "at": { "type": "number", "minimum": 0, "description": "Seconds of recorded time (pauses removed). With `capture_id`: where to add the marker to a finished recording. Without it, and with `label`: the running recording's marker set at that time gets the label (nothing is added)." },
                "label": { "type": "string", "maxLength": 200, "description": "Optional short label." },
                "request_id": request_id() },
                "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string" },
                "capture_id": { "type": "string" },
                "marker": { "type": "object", "properties": {
                    "at": { "type": "number", "description": "Seconds of recorded time." },
                    "at_ms": { "type": "integer" },
                    "label": { "type": ["string", "null"] },
                    "by": { "type": ["string", "null"], "description": "`user` or `agent:<name>`." } },
                    "required": ["at", "at_ms"] },
                "count": { "type": "integer", "description": "Markers on the running recording." },
                "revision": { "type": "integer", "description": "Finished recording only." } },
                "required": ["marker"] }),
        },
        ToolSpec {
            name: "stop_recording",
            tier: Tier::Free,
            mutating: true,
            destructive: false,
            spec: "REC-1, REC-4",
            summary: "Stop and save the recording (also valid while paused). Returns the capture id (use it with get_capture, annotate, export...), the duration, the tracks and the reason it ended. During a countdown it cancels the start and nothing is recorded. If the recording already ended on its own (max duration, disk full, window closed) the same result comes back with `already_stopped: true` when `recording_id` is given. While a 60 fps self-test runs, it cancels the test (`state: \"cancelled\"`) and deletes its output.",
            input: recording_ref_input(),
            output: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string" },
                "capture_id": { "type": ["string", "null"] },
                "title": { "type": "string" },
                "path": { "type": "string" },
                "reason": { "type": "string", "description": "`user`/`agent` (a stop call), `max_duration`, `disk_full`, `screen_halted`, `shutdown`, ..." },
                "duration_s": { "type": "number" },
                "duration_ms": { "type": "integer" },
                "paused_ms": { "type": "integer" },
                "tracks": { "type": "array", "items": { "type": "object", "properties": {
                    "kind": { "type": "string" }, "file": { "type": "string" },
                    "start_ms": { "type": "integer" }, "end_ms": { "type": "integer" }, "stats": { "type": "object" } } } },
                "warnings": { "type": "array", "items": { "type": "object" } },
                "already_stopped": { "type": "boolean" },
                "cancelled": { "type": "boolean" } },
                "required": ["recording_id"] }),
        },
        ToolSpec {
            name: "get_recording_status",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "REC-3, REC-8, AUD-1",
            summary: "The state of the recording: idle, countdown, recording, paused, resuming or stopping, with elapsed recorded time, per-track state and counters, late-frame percentage, microphone level (dBFS), minutes of disk left and warnings. Without a running recording it returns `state: idle` and the last one.",
            input: json!({ "type": "object", "properties": {
                "recording_id": { "type": "string", "minLength": 1 } },
                "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "state": { "type": "string", "enum": ["idle", "countdown", "starting", "recording", "paused", "resuming", "stopping", "stopped"] },
                "recording_id": { "type": ["string", "null"] },
                "capture_id": { "type": ["string", "null"] },
                "countdown_remaining": { "type": ["integer", "null"] },
                "recorded_ms": { "type": "integer", "description": "What the timer shows (no pauses)." },
                "elapsed_ms": { "type": "integer" },
                "paused_ms": { "type": "integer" },
                "pauses": { "type": "integer" },
                "tracks": { "type": "array", "items": { "type": "object" } },
                "late_frame_pct": { "type": ["number", "null"] },
                "mic_level_dbfs": { "type": ["number", "null"] },
                "disk_left_minutes": { "type": ["number", "null"] },
                "warnings": { "type": "array", "items": { "type": "object" } },
                "indicator": { "type": "object" },
                "last": { "type": ["object", "null"], "description": "When idle: the last recording's outcome." } },
                "required": ["state"] }),
        },
        ToolSpec {
            name: "list_devices",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "CAM-4, AUD-1, AUD-2",
            summary: "List the cameras, microphones and system-audio support, with the permission state of the camera, microphone and screen recording. Never asks for a permission and never opens a device.",
            input: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "cameras": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" }, "name": { "type": "string" },
                    "is_default": { "type": "boolean" },
                    "max_size": { "type": "array", "items": { "type": "integer" } } },
                    "required": ["id", "name"] } },
                "cameras_error": { "type": ["string", "null"] },
                "camera_permission": { "type": "string" },
                "mics": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" }, "name": { "type": "string" },
                    "default": { "type": "boolean" }, "channels": { "type": "integer" },
                    "sample_rate": { "type": "integer" } },
                    "required": ["id", "name"] } },
                "microphone_permission": { "type": "string" },
                "system_audio": { "type": "object", "properties": {
                    "supported": { "type": "boolean" }, "backend": { "type": "string" },
                    "reason": { "type": ["string", "null"] } } },
                "screen_recording_permission": { "type": "string" },
                "cursor": { "type": "object" } },
                "required": ["cameras", "mics", "system_audio"] }),
        },
        ToolSpec {
            name: "run_fps_selftest",
            tier: Tier::Confirm,
            mutating: true,
            destructive: false,
            spec: "REC-3, AGT-2",
            summary: "Test whether this machine records 60 fps without late frames: records about 10 s of the screen at 60 fps (no microphone, camera or system audio), measures late and dropped frames (pass: at most 1% late and 1% dropped), deletes the test video and keeps only the result (setting `recording.fps60_selftest`, valid 30 days for this display, size and codec). The user sees a confirmation that says exactly what happens, and the usual recording indicator is shown while it runs. Returns when it ends. Fails with `busy` while a recording runs, `permission_missing` (data.permission: screen) before anything is asked. Cancel it with stop_recording. After a pass, start_recording accepts fps 60 for the same display, size and codec.",
            input: json!({ "type": "object", "properties": {
                "target": { "oneOf": [
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 } },
                      "required": ["display_id"], "additionalProperties": false },
                    { "type": "object", "properties": { "window_id": { "type": "integer", "minimum": 0 } },
                      "required": ["window_id"], "additionalProperties": false },
                    { "type": "object", "properties": { "display_id": { "type": "integer", "minimum": 0 },
                        "rect": { "type": "array", "items": { "type": "integer" }, "minItems": 4, "maxItems": 4,
                                  "description": "[x, y, w, h] physical px relative to the display" } },
                      "required": ["display_id", "rect"], "additionalProperties": false } ],
                    "description": "What to test; default the primary display." },
                "size": { "type": "string", "enum": ["source", "2160p", "1440p", "1080p", "720p"], "description": "Output size to test; default the `recording.size` setting, else `source`." },
                "codec": { "type": "string", "enum": ["auto", "hevc", "h264"], "description": "Codec to test; default the `recording.codec` setting, else `auto`." },
                "request_id": request_id() },
                "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "selftest_id": { "type": "string" },
                "state": { "type": "string", "enum": ["passed", "failed"] },
                "passed": { "type": "boolean" },
                "fps": { "type": "integer" },
                "late_pct": { "type": "number", "description": "Late frames over the run, percent (NFR-2: at most 1)." },
                "dropped": { "type": "integer" },
                "dropped_pct": { "type": "number" },
                "frames": { "type": "integer" },
                "duration_s": { "type": "number" },
                "size": { "type": "array", "items": { "type": "integer" }, "description": "Output [width, height] that was tested." },
                "size_label": { "type": "string" },
                "codec": { "type": "string" },
                "display_id": { "type": ["integer", "null"] },
                "at": { "type": "string" },
                "stored": { "type": "boolean", "description": "False if the result could not be saved (then 60 fps stays off)." },
                "message": { "type": "string" },
                "suggestion": { "type": "object", "description": "On a fail: `fps: 30` and, when there is one, a lower `size`." } },
                "required": ["selftest_id", "state", "passed", "late_pct"] }),
        },
        ToolSpec {
            name: "list_publish_accounts",
            tier: Tier::Free,
            mutating: false,
            destructive: false,
            spec: "PUB-1, PUB-2",
            summary: "List the accounts the user connected for publishing (YouTube channels today): `account_id` (pass it to publish_video), `platform`, `name` (the channel), `status` (`connected`, or `needs_reconnect` when a publish is waiting for the user to sign in again), `default`, `default_privacy` (never `public`), `made_for_kids_default` (may be null), `uploads_today` and `uploads_limit` (YouTube allows 100 uploads a day per API project; the count resets at midnight Pacific time, `resets_at`). `platforms` has the field limits (title, description, tags, thumbnail). Never a key or token. Agents cannot connect or change accounts.",
            input: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "accounts": { "type": "array", "items": { "type": "object", "properties": {
                    "account_id": { "type": "string" }, "platform": { "type": "string" },
                    "name": { "type": "string" },
                    "status": { "type": "string", "enum": ["connected", "needs_reconnect"] },
                    "default": { "type": "boolean" },
                    "default_privacy": { "type": "string", "enum": ["private", "unlisted"] },
                    "made_for_kids_default": { "type": ["boolean", "null"] },
                    "uploads_today": { "type": "integer" }, "uploads_limit": { "type": "integer" },
                    "resets_at": { "type": "string" } },
                    "required": ["account_id", "platform", "name", "status"] } },
                "platforms": { "type": "object" } },
                "required": ["accounts"] }),
        },
        ai_tool(
            "get_text_model",
            Tier::Free,
            false,
            "AI-7",
            "Read how the text model (summaries, translation to any language, reports) is set up: `provider` (`none`, or `embedded` for the text engine built into CaptureMann), `model`, `embedded_model`, `engine` (the built-in engine's state), `context_tokens`, `timeout_s`, `structured_output` (the last Test's result, null if untested). The text never leaves this computer: the built-in engine listens on 127.0.0.1 only. These settings (`ai.text.*`) can only be changed in the app.",
            json!({}),
            &[],
            json!({ "type": "object", "properties": {
                "provider": { "type": "string", "enum": ["none", "embedded"] },
                "model": { "type": "string" },
                "embedded_model": { "type": "string", "description": "The built-in model chosen in the settings (empty: the active one)." },
                "engine": { "type": "object", "description": "The built-in engine: `state` (`stopped`, `starting`, `running`, `failed`), `model`, `port`, `in_flight`, `restarts`, `helper`, `reason`, `error`, `idle_stop_s`." },
                "context_tokens": { "type": "integer" },
                "timeout_s": { "type": "integer" },
                "structured_output": { "type": ["boolean", "null"] } },
                "required": ["provider", "model", "context_tokens", "structured_output"] }),
        ),
        ai_tool(
            "test_text_model",
            Tier::Free,
            false,
            "AI-7",
            "Send one short fixed prompt to the configured text model and report `ok`, `latency_ms`, `model` and whether structured output worked on the first try (`structured_output`). It sends no recording text and talks only to the configured provider. A server that is not running or a model that fails is an answer with `ok: false`, a `reason` (`text_model_unreachable`, `timeout`, `invalid_model_output`, `text_model_error`, and for the built-in engine `engine_missing`, `engine_failed`, `not_enough_memory`) and a `message`. With provider `none` it fails with `invalid_input` and `data.reason = \"no_text_model\"`.",
            json!({}),
            &[],
            json!({ "type": "object", "properties": {
                "ok": { "type": "boolean" }, "latency_ms": { "type": "integer" },
                "model": { "type": "string" }, "structured_output": { "type": "boolean" },
                "reason": { "type": "string" }, "message": { "type": "string" } },
                "required": ["ok", "latency_ms", "model"] }),
        ),
        ToolSpec {
            name: "publish_video",
            tier: Tier::Confirm,
            mutating: true,
            destructive: false,
            spec: "PUB-2",
            summary: "Publish a recording to YouTube. The user always has to approve on screen: the dialog shows the account, title, privacy, made for kids, the file and its size, the playlist id when `playlist_id` is given (the id, not the playlist title), and that the video is uploaded over the internet. Everything is checked first (the recording and its export exist, the account exists, the title, description, tags and thumbnail fit YouTube's limits, `made_for_kids` is answered), so a bad request fails with `invalid_input` naming the field before any dialog. `made_for_kids` has no default: ask the user. A missing `privacy` is the account's default (`unlisted` unless the user chose `private`), never `public`. The newest export is sent; give `export_preset` (for example `youtube-1080p`) to export first. `account_id` comes from list_publish_accounts (the default account when absent). Returns a `job_id` once approved; watch it with get_job or `job.*` events (job type `publish`, resumable, retried on network errors, paused when the account must sign in again). The finished job's result has `url`, `video_id`, `status`, `privacy` (as YouTube reports it, `privacy_locked` when YouTube kept an unverified project's video private) and `warnings`. There is no tool that changes the privacy of a video that is already on YouTube.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "platform": { "type": "string", "enum": ["youtube"], "description": "Only `youtube` is built; `tiktok` and `facebook` fail with `not_implemented`." },
                "account_id": { "type": "string", "description": "From list_publish_accounts; the default account when absent." },
                "title": { "type": "string", "maxLength": 100, "description": "Required, at most 100 characters, no < or >." },
                "description": { "type": "string", "description": "At most 5,000 bytes, no < or >." },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "At most 500 characters in total." },
                "privacy": { "type": "string", "enum": ["private", "unlisted", "public"], "description": "The account's default (never public) when absent." },
                "made_for_kids": { "type": "boolean", "description": "Required by YouTube; there is no default, ask the user." },
                "thumbnail": { "type": "string", "description": "Absolute path of a JPG or PNG up to 2 MB." },
                "export_preset": { "type": "string", "description": "Export the recording with this preset first (for example `youtube-1080p`); the newest existing export otherwise." },
                "export_path": { "type": "string", "description": "Absolute path of an export of this capture to send instead of the newest one; not together with `export_preset`." },
                "captions": { "type": "boolean", "default": false, "description": "Also upload the recording's captions as an SRT track (needs captions or a transcript)." },
                "category": { "type": "string", "description": "YouTube category id; 28 (Science & Technology) when absent." },
                "playlist_id": { "type": "string", "description": "Add the video to this playlist (a playlist id). The confirmation dialog shows the id, not the title." },
                "metadata": { "type": "object", "description": "The same fields as above in one object (title, description, tags, privacy, made_for_kids, thumbnail, captions, category, playlist_id); top-level fields win." },
                "request_id": request_id() },
                "required": ["capture_id", "platform"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "job_id": { "type": "string", "description": "The publish job; its result has the URL." },
                "capture_id": { "type": "string" }, "platform": { "type": "string" },
                "account_id": { "type": "string" }, "account": { "type": "string" },
                "title": { "type": "string" },
                "privacy": { "type": "string", "enum": ["private", "unlisted", "public"] },
                "made_for_kids": { "type": ["boolean", "null"] },
                "file": { "type": ["string", "null"], "description": "The export that is sent, relative to the capture." },
                "export_preset": { "type": ["string", "null"] },
                "export_job_id": { "type": ["string", "null"] },
                "captions": { "type": "boolean" }, "state": { "type": "string" } },
                "required": ["job_id", "state"] }),
        },
        ToolSpec {
            name: "delete_capture",
            tier: Tier::Confirm,
            mutating: true,
            destructive: true,
            spec: "LIB-1",
            summary: "Delete a capture: its folder is moved to the Trash (the user can restore it) and it leaves the library. The dialog names the capture and its size. Refused with `busy` while the capture is being recorded, or while an export, upload or publish job for it is queued, running or paused (the message names the job). `also_delete_remote: true` also deletes the remote copies the capture records, PERMANENTLY, before the capture goes to the Trash: Google Drive files and bundle folders, copies in local destination folders, and YouTube videos (never changing the privacy of any video). The dialog lists each remote copy (destination and path, or video title, URL and channel) and each copy that is left in place (OneDrive, Dropbox, removed destinations or accounts, a Drive bundle copy from before folder ids were recorded). If a remote step fails (`auth_expired`, `network`, `platform_rejected`), the capture is KEPT and the error says which copies were deleted and which were not (`error.data.remote`); the manifest records the outcome, so calling again only meets what is left. Without remote copies the flag changes nothing.",
            input: json!({ "type": "object", "properties": {
                "capture_id": capture_id(),
                "also_delete_remote": { "type": "boolean", "default": false, "description": "Also permanently delete the remote copies recorded in the capture (Google Drive, local destinations, YouTube), listed in the confirmation dialog. A failed remote step keeps the capture." },
                "request_id": request_id() },
                "required": ["capture_id"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "title": { "type": "string" },
                "kind": { "type": "string" }, "size_bytes": { "type": "integer" },
                "moved_to_trash": { "type": "boolean" },
                "deleted_remote": { "type": "boolean", "description": "At least one remote copy was deleted." },
                "remote": { "type": "object", "description": "Only with also_delete_remote: what happened to each recorded remote copy (text of the copy).",
                    "properties": {
                        "deleted": { "type": "array", "items": { "type": "string" } },
                        "already_gone": { "type": "array", "items": { "type": "string" } },
                        "left_in_place": { "type": "array", "items": { "type": "object" } },
                        "failed": { "type": "array", "items": { "type": "object" } },
                        "not_tried": { "type": "array", "items": { "type": "string" } } } } },
                "required": ["capture_id", "moved_to_trash"] }),
        },
        ToolSpec {
            name: "move_library",
            tier: Tier::Confirm,
            mutating: true,
            destructive: false,
            spec: "LIB-2",
            summary: "Change the folder the library lives in (for example to an external drive). The user always has to approve on screen, and this cannot be switched off: the dialog states the old path, the new path, the size and whether it is a move, a fresh start or a switch to a library that is already there. `mode: move` (default) copies every capture to the new folder as the background job `library_move`: each capture is copied beside its final name, checked against the original (same files and byte sizes, `manifest.json` identical) and only then renamed, so a name in the new folder always holds a complete capture; nothing is ever deleted from the old folder, an interrupted or cancelled job leaves the old library active and can be retried (`retry_job`) to resume, and captures that are being recorded are skipped and reported. When every capture is verified the library setting changes (an agent cannot write it with `set_settings`). `mode: fresh` switches to an empty library there and copies nothing (the old files stay). `mode: use_existing` switches to a folder that already holds captures. Everything is checked before any dialog or copy and fails with a typed `error.data.reason`: `recording_running`, `selftest_running`, `job_running` (names the job; nothing is cancelled for you) and `move_running` (`busy`); `target_not_absolute`, `target_is_file`, `target_not_writable`, `target_inside_library`, `target_contains_library`, `same_folder`, `target_has_library` (use `use_existing`), `target_has_no_library` (`invalid_input`); `not_enough_space` (`disk_full`, needs 1.1 times the library size free) and `source_unreadable` (`not_found`). The running app keeps using the old library until it restarts: the result and the `library.moved` event say `restart_required: true`. Moving the old copies to the Trash is not available to agents.",
            input: json!({ "type": "object", "properties": {
                "path": { "type": "string", "minLength": 1, "description": "Absolute path of the new library folder (created if it does not exist)." },
                "mode": { "type": "string", "enum": ["move", "fresh", "use_existing"], "default": "move", "description": "`move` copies the captures, `fresh` starts empty, `use_existing` switches to a library already in the folder." },
                "request_id": request_id() },
                "required": ["path"], "additionalProperties": false }),
            output: json!({ "type": "object", "properties": {
                "job_id": { "type": ["string", "null"], "description": "The `library_move` job (mode `move`); null for `fresh` and `use_existing`, which are done at once." },
                "state": { "type": "string", "description": "`queued` for a move, `done` otherwise." },
                "mode": { "type": "string", "enum": ["move", "fresh", "use_existing"] },
                "from": { "type": "string" }, "to": { "type": "string" },
                "bytes": { "type": "integer", "description": "Bytes to copy (move) or already in the new folder (use_existing)." },
                "bundles": { "type": "integer", "description": "Captures to copy (move) or already in the new folder (use_existing)." },
                "restart_required": { "type": "boolean", "description": "Always true: CaptureMann uses the new folder after it restarts." },
                "warnings": { "type": "array", "items": { "type": "string" } } },
                "required": ["job_id", "state", "mode", "restart_required"] }),
        },
    ];
    // Recording edits (EDT-1 to EDT-3).
    tools.extend(edit_tools());
    // Local AI (M5).
    tools.extend(ai_tools());
    tools
}

fn text_model_id_schema() -> Value {
    json!({ "type": "string", "pattern": "^[a-z0-9][a-z0-9._-]{0,39}$",
            "description": "A built-in text model id from list_text_models, e.g. `gemma-4-e2b`." })
}

fn model_id_schema() -> Value {
    json!({ "type": "string", "pattern": "^[a-z0-9][a-z0-9._-]{0,39}$",
            "description": "A speech model id from list_models, e.g. `base`, `small` or `medium`." })
}

#[allow(clippy::too_many_arguments)]
fn ai_tool(
    name: &'static str,
    tier: Tier,
    mutating: bool,
    spec: &'static str,
    summary: &'static str,
    properties: Value,
    required: &[&str],
    output: Value,
) -> ToolSpec {
    let mut props = properties;
    if mutating {
        if let Some(m) = props.as_object_mut() {
            m.insert("request_id".into(), request_id());
        }
    }
    ToolSpec {
        name,
        tier,
        mutating,
        destructive: matches!(name, "remove_model" | "remove_text_model"),
        spec,
        summary,
        input: json!({ "type": "object", "properties": props, "required": required,
                       "additionalProperties": false }),
        output,
    }
}

fn report_revision_output() -> Value {
    json!({ "type": "object", "properties": {
        "capture_id": { "type": "string" }, "report_id": { "type": "string" },
        "revision": { "type": "integer" }, "action": { "type": "string" }, "by": { "type": "string" },
        "can_undo": { "type": "boolean" }, "can_redo": { "type": "boolean" } },
        "required": ["capture_id", "report_id", "revision"] })
}

fn job_started_output(extra: Value) -> Value {
    let mut props = json!({
        "job_id": { "type": "string", "description": "Watch it with get_job or the `job.*` events." },
        "state": { "type": "string" },
        "reused": { "type": "boolean", "description": "true when an equal job was already queued or running." } });
    if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            p.insert(k.clone(), v.clone());
        }
    }
    json!({ "type": "object", "properties": props, "required": ["job_id", "state"] })
}

fn suggestions_output(item: Value) -> Value {
    json!({ "type": "object", "properties": {
        "capture_id": { "type": "string" },
        "count": { "type": "integer" },
        "suggestions": { "type": "array", "items": item },
        "apply_with": { "type": "string", "description": "The tool that applies an accepted suggestion." } },
        "required": ["capture_id", "count", "suggestions", "apply_with"] })
}

/// The local-AI tools (AI-1, AI-3, AI-4): speech models, transcripts, captions, suggestions.
fn ai_tools() -> Vec<ToolSpec> {
    let time = |what: &str| time_schema(what);
    vec![
        ai_tool(
            "list_models",
            Tier::Free,
            false,
            "AI-1",
            "List the speech models: which are installed on this computer, which one transcribe uses by default (`active`), their download size, and the folder they are kept in. Nothing is downloaded by listing.",
            json!({}),
            &[],
            json!({ "type": "object", "properties": {
                "models": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" }, "label": { "type": "string" },
                    "size_mb": { "type": "integer", "description": "Download size in MB (10^6 bytes)." },
                    "hint": { "type": "string", "description": "What the model is good for (speed against accuracy)." },
                    "installed": { "type": "boolean" }, "active": { "type": "boolean" },
                    "path": { "type": ["string", "null"] } },
                    "required": ["id", "label", "size_mb", "installed", "active"] } },
                "active": { "type": ["string", "null"] },
                "dir": { "type": "string" } },
                "required": ["models", "active", "dir"] }),
        ),
        ai_tool(
            "download_model",
            Tier::Confirm,
            true,
            "AI-1",
            "Download a speech model from the internet, in the background. This is the only thing in the AI features that uses the network, and it only runs when asked: the user sees a confirmation with the model, its size and where it is saved. Returns a `job_id`; progress arrives as `job.progress` and `model.download.progress` events, an interrupted download resumes, and `cancel_job` stops it. Fails with `invalid_input` for an unknown or already installed model, and `not_implemented` when the app has no speech engine.",
            json!({ "model": model_id_schema() }),
            &["model"],
            job_started_output(json!({ "model": { "type": "string" } })),
        ),
        ai_tool(
            "import_model",
            Tier::Free,
            true,
            "AI-1",
            "Install a speech model from a file the user already has (for example a downloaded `ggml-small.bin`), without using the network. The file is checked before it is accepted.",
            json!({ "model": model_id_schema(),
                    "path": { "type": "string", "minLength": 1, "description": "Path of the model file on this computer." } }),
            &["model", "path"],
            json!({ "type": "object", "properties": {
                "model": { "type": "string" }, "installed": { "type": "boolean" } },
                "required": ["model", "installed"] }),
        ),
        ai_tool(
            "select_model",
            Tier::Free,
            true,
            "AI-1",
            "Choose the installed speech model that `transcribe` uses when no model is named.",
            json!({ "model": model_id_schema() }),
            &["model"],
            json!({ "type": "object", "properties": { "active": { "type": "string" } },
                    "required": ["active"] }),
        ),
        ai_tool(
            "remove_model",
            Tier::Confirm,
            true,
            "AI-1",
            "Delete an installed speech model from this computer. The user sees a confirmation with the model and its size. Refused with `busy` while a transcription or download uses it.",
            json!({ "model": model_id_schema() }),
            &["model"],
            json!({ "type": "object", "properties": {
                "model": { "type": "string" }, "removed": { "type": "boolean" } },
                "required": ["model", "removed"] }),
        ),
        ai_tool(
            "list_text_models",
            Tier::Free,
            false,
            "AI-7",
            "List the text models of the engine built into CaptureMann (provider `embedded`): which are downloaded, which one runs by default (`active`), the download size, the memory each needs (`ram_mb`), whether it is the recommended default (`recommended`), its licence and the folder they are kept in. Models whose checksum is not pinned yet are not listed. Nothing is downloaded by listing.",
            json!({}),
            &[],
            json!({ "type": "object", "properties": {
                "models": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" }, "label": { "type": "string" },
                    "size_mb": { "type": "integer", "description": "Download size in MB (10^6 bytes)." },
                    "ram_mb": { "type": "integer", "description": "Memory it needs while it runs, in MB." },
                    "recommended": { "type": "boolean", "description": "The default model, recommended on every Mac." },
                    "min_memory_gb": { "type": "integer", "description": "The app offers this model only on a Mac with at least this much memory, in GB (0: every Mac)." },
                    "license": { "type": "string", "description": "Apache-2.0 or MIT." },
                    "context_tokens": { "type": "integer" },
                    "hint": { "type": "string" },
                    "installed": { "type": "boolean" }, "active": { "type": "boolean" },
                    "path": { "type": ["string", "null"] } },
                    "required": ["id", "label", "size_mb", "recommended", "license", "installed", "active"] } },
                "active": { "type": ["string", "null"] },
                "dir": { "type": "string" } },
                "required": ["models", "active", "dir"] }),
        ),
        ai_tool(
            "download_text_model",
            Tier::Confirm,
            true,
            "AI-7",
            "Download a text model for the built-in engine from the internet, in the background. Like `download_model` it only runs when asked: the user sees a confirmation with the model, its size, its licence and where it is saved. Returns a `job_id`; progress arrives as `job.progress` and `model.download.progress` events (with `kind: \"text\"`), an interrupted download resumes, the file is checked against its pinned SHA-256, and `cancel_job` stops it. Fails with `invalid_input` for an unknown or already downloaded model.",
            json!({ "model": text_model_id_schema() }),
            &["model"],
            job_started_output(json!({ "model": { "type": "string" } })),
        ),
        ai_tool(
            "select_text_model",
            Tier::Free,
            true,
            "AI-7",
            "Choose the downloaded text model the built-in engine runs, the same as the Use button on the AI page: it turns the built-in engine on (provider `embedded`, with this model) and loads nothing: the engine starts on the first request.",
            json!({ "model": text_model_id_schema() }),
            &["model"],
            json!({ "type": "object", "properties": { "active": { "type": "string" }, "provider": { "type": "string" } },
                    "required": ["active"] }),
        ),
        ai_tool(
            "remove_text_model",
            Tier::Confirm,
            true,
            "AI-7",
            "Delete a downloaded text model from this computer. The user sees a confirmation with the model and its size. Refused with `busy` while it is being downloaded or a job is using it; an idle engine is stopped first.",
            json!({ "model": text_model_id_schema() }),
            &["model"],
            json!({ "type": "object", "properties": {
                "model": { "type": "string" }, "removed": { "type": "boolean" } },
                "required": ["model", "removed"] }),
        ),
        ai_tool(
            "transcribe",
            Tier::Free,
            true,
            "AI-1",
            "Transcribe a recording on this computer with an installed speech model (no network). Runs in the background and returns a `job_id`; the result is `transcript.json` in the capture, read with get_transcript; `transcript.ready` is published when it is done. Without an installed model it fails with `invalid_input` and `data.reason = \"no_model\"`: tell the user to open the AI page in CaptureMann or run `capturemann ai models` to download or import one (nothing is downloaded automatically). A named model that is not installed fails with `data.reason = \"model_not_installed\"`. The recording needs a microphone or system-audio track.",
            json!({ "capture_id": capture_id(),
                    "language": { "type": "string", "default": "auto",
                                  "description": "`auto` to detect, a language code such as `en`, `vi` or `es`, or codes separated by commas to choose among, such as `vi,en` (default: the languages in the `ai.languages` setting, else detect among all)." },
                    "model": model_id_schema(),
                    "original_audio": { "type": "boolean", "default": false,
                                        "description": "Read the microphone as it was recorded instead of the noise-reduced copy a recording stores by default. Noise reduction can remove soft sounds that speech recognition needs, so try this when words are missing; it makes no difference to a recording made without noise reduction." } }),
            &["capture_id"],
            job_started_output(json!({
                "capture_id": { "type": "string" }, "model": { "type": "string" },
                "language": { "type": ["string", "null"] },
                "original_audio": { "type": "boolean" } })),
        ),
        ai_tool(
            "get_transcript",
            Tier::Free,
            false,
            "AI-1",
            "Read a recording's transcript (words with start and end in session time, ticks of 1/120000 s, and confidence) and its caption lines: the user's edited lines if there are any (`captions_source: \"edited\"`), else lines generated from the transcript (`\"generated\"`). Fails with `not_found` until transcribe has finished.",
            json!({ "capture_id": capture_id() }),
            &["capture_id"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" },
                "transcript": { "type": "object", "properties": {
                    "version": { "type": "integer" }, "language": { "type": "string" },
                    "model": { "type": "string" },
                    "words": { "type": "array", "items": { "type": "object", "properties": {
                        "text": { "type": "string" }, "start": { "type": "integer" },
                        "end": { "type": "integer" }, "confidence": { "type": "number" },
                        "speaker": { "type": "integer", "description": "AI-8: the speaker's id (see `speakers`); present once `diarize` has run." },
                        "overlap": { "type": "boolean", "description": "AI-8: other speakers talked at the same time." } },
                        "required": ["text", "start", "end"] } },
                    "note": { "type": ["string", "null"] } },
                    "required": ["language", "model", "words"] },
                "speakers": { "type": "array", "description": "AI-8: present once `diarize` has run.",
                    "items": { "type": "object", "properties": {
                        "id": { "type": "integer" }, "name": { "type": "string" } },
                        "required": ["id", "name"] } },
                "captions": { "type": ["array", "null"], "items": { "type": "object", "properties": {
                    "start": { "type": "integer" }, "end": { "type": "integer" },
                    "text": { "type": "string" } } } },
                "captions_source": { "type": ["string", "null"], "enum": ["edited", "generated", null] } },
                "required": ["capture_id", "transcript"] }),
        ),
        ai_tool(
            "get_captions",
            Tier::Free,
            false,
            "AI-1",
            "Read a recording's caption lines (start and end in session time, ticks of 1/120000 s, and text of at most 2 rows of 42 characters): the user's edited lines if there are any (`source: \"edited\"`), else lines generated from the transcript (`\"generated\"`). Fails with `not_found` until transcribe has finished and nothing was edited.",
            json!({ "capture_id": capture_id(), "language": { "type": "string", "description": "A translation's language code (`vi`, `pt-br`) made by translate_captions; omit for the original captions." } }),
            &["capture_id"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" },
                "lines": { "type": "array", "items": { "type": "object", "properties": {
                    "start": { "type": "integer" }, "end": { "type": "integer" },
                    "text": { "type": "string" } },
                    "required": ["start", "end", "text"] } },
                "source": { "type": "string", "enum": ["edited", "generated", "translation"] },
                "language": { "type": "string", "description": "Only for a translation." },
                "engine": { "type": "string", "description": "Only for a translation: `whisper` or `text_model`." },
                "model": { "type": "string" }, "edited": { "type": "boolean" },
                "edited_lines": { "type": "array", "items": { "type": "integer" },
                    "description": "Only for a translation: indexes of lines changed by hand." },
                "untranslated_lines": { "type": "array", "items": { "type": "integer" },
                    "description": "Only for a translation: indexes of lines the text model did not return." } },
                "required": ["capture_id", "lines", "source"] }),
        ),
        ai_tool(
            "set_captions",
            Tier::Free,
            true,
            "AI-1",
            "Replace a recording's caption lines with edited ones (with `language`, the lines of that translation: written to its file, not an undo step; a line you leave as it was keeps its record, a changed one counts as edited and survives a re-translate while its source line is unchanged). Each line has `start`, `end` (session time: seconds or \"t:<ticks>\") and `text` of at most 2 rows of 42 characters (rows are separated by a newline), lasts at most 7 s, and lines must be in order and not overlap. An empty list removes the edited lines (the generated ones come back). One undo step, tagged with the caller; undo restores the lines before.",
            json!({ "lines": { "type": "array", "maxItems": 5000, "items": { "type": "object", "properties": {
                        "start": time("Line start"), "end": time("Line end"),
                        "text": { "type": "string", "minLength": 1,
                                  "description": "At most 2 rows of 42 characters; separate rows with a newline." } },
                    "required": ["start", "end", "text"], "additionalProperties": false } },
                    "language": { "type": "string", "description": "A translation's language code (`vi`, `pt-br`) made by translate_captions; omit for the original captions." } }),
            &["lines"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" },
                "revision": { "type": "integer" }, "action": { "type": "string" }, "by": { "type": "string" },
                "lines": { "type": "integer" },
                "can_undo": { "type": "boolean" }, "can_redo": { "type": "boolean" } },
                "required": ["capture_id", "revision", "lines"] }),
        ),
        ai_tool(
            "replace_in_captions",
            Tier::Free,
            true,
            "AI-6a",
            "Find and replace across all caption lines of a recording (the edited lines, else the lines generated from the transcript). The pairs apply in order. Matching is by Unicode text with Vietnamese diacritics exact (`dê` never matches `đê`), NFC and NFD forms are equal, case-insensitive unless `match_case`, and `whole_word` needs non-letters around the match; a match may span the line's row break. Touched lines are wrapped again; a result over 2 rows of 42 characters is refused and nothing changes. Nothing found changes nothing (`changed: false`, no revision). One undo step, tagged with the caller, for all pairs together. The original transcript (`transcript.json`) is not modified: the caption lines are the corrected text layer that SRT, VTT, TXT and burned-in captions use.",
            json!({ "replacements": { "type": "array", "minItems": 1, "maxItems": 100, "items": { "type": "object", "properties": {
                        "find": { "type": "string", "minLength": 1, "maxLength": 200, "description": "Text to find." },
                        "replace": { "type": "string", "maxLength": 200, "description": "Replacement; may be empty to delete the match." },
                        "match_case": { "type": "boolean", "description": "Default false." },
                        "whole_word": { "type": "boolean", "description": "Default false." } },
                    "required": ["find", "replace"], "additionalProperties": false } },
                    "language": { "type": "string", "description": "A translation's language code (`vi`, `pt-br`) made by translate_captions; omit for the original captions." } }),
            &["replacements"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" },
                "replacements": { "type": "integer", "description": "How many matches were replaced." },
                "lines_touched": { "type": "integer" },
                "changed": { "type": "boolean" },
                "revision": { "type": ["integer", "null"] }, "action": { "type": "string" }, "by": { "type": "string" },
                "can_undo": { "type": "boolean" }, "can_redo": { "type": "boolean" } },
                "required": ["capture_id", "replacements", "lines_touched", "changed"] }),
        ),
        ai_tool(
            "export_captions",
            Tier::Free,
            true,
            "AI-1",
            "Write the captions as an SRT, VTT or plain-text (`txt`) file in the capture's `exports/` folder (with `language`, a translation: `<title> - captions.<lang>.srt`, `<title>.<lang>.txt`; `txt` is `<title>.txt`: one caption line per row, add `timestamps: true` for a `[mm:ss]` prefix on every line). The lines are the edited ones if there are any, else generated from the transcript, and the recording's edits apply: cut ranges remove their lines and shift the later ones, and speed scales the times, so no caption refers to cut content. Fails with `not_found` before transcribe has run.",
            json!({ "format": { "type": "string", "enum": ["srt", "vtt", "txt", "md"] },
                    "language": { "type": "string", "description": "A translation's language code (`vi`, `pt-br`) made by translate_captions (srt, vtt or txt); omit for the original captions." },
                    "timestamps": { "type": "boolean",
                        "description": "Only with `txt`: prefix every line with [mm:ss] (or [hh:mm:ss] for long recordings). Default false." },
                    "speaker_names": { "type": "boolean",
                        "description": "Only with `srt`: start every cue with `NAME:` (AI-8). Default false. Once the recording has speakers, VTT always uses voice tags (`<v Name>`), and `txt` and `md` write `Name: text` per turn." } }),
            &["format"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "format": { "type": "string" },
                "path": { "type": "string" }, "lines": { "type": "integer" },
                "speaker_names": { "type": "boolean", "description": "The file names the speakers." } },
                "required": ["capture_id", "format", "path", "lines"] }),
        ),
        ai_tool(
            "translate_captions",
            Tier::Free,
            true,
            "AI-5",
            "Translate a recording's captions into another language. Runs in the background and returns a `job_id`; the result is `translations/<lang>.json` in the capture (read it with get_captions `language`, edit it with set_captions or replace_in_captions `language`, export it with export_captions `language`); `translation.ready` is published when it is done. Engines: `whisper` re-runs the audio with Whisper's translate task, into English only, offline; it uses the active model if that can translate, else large-v3, else medium (large-v3-turbo cannot translate and is never used), and fails with `invalid_input` and `data.reason = \"translation_model_missing\"` when neither is installed (nothing is downloaded). `text_model` uses the text model provider (not available yet: `invalid_input` with `data.reason = \"no_text_model\"`). `auto` (default) picks whisper for English and the text model for every other language. A target equal to the recording's language fails with `data.reason = \"same_language\"`. Hand edits to an earlier translation are kept when their source caption line is unchanged. The recording needs a transcript (`not_found` otherwise) and a microphone or system-audio track.",
            json!({ "target_language": { "type": "string",
                        "description": "The language to translate into: a code such as `en`, `vi`, `es` or `pt-br`." },
                    "engine": { "type": "string", "enum": ["auto", "whisper", "text_model"], "default": "auto" } }),
            &["capture_id", "target_language"],
            job_started_output(json!({
                "capture_id": { "type": "string" }, "language": { "type": "string" },
                "engine": { "type": "string", "enum": ["whisper", "text_model"] },
                "model": { "type": "string", "description": "The Whisper model that will run (empty for the text model)." },
                "source_language": { "type": "string" } })),
        ),
        ai_tool(
            "diarize",
            Tier::Free,
            true,
            "AI-8",
            "Find who spoke when in a recording, on this computer (no network), so the transcript can show speaker names. Runs in the background and returns a `job_id`; the job result is `{ speakers, segments }` and `speakers.ready` is published when it is done. Speakers are numbered and named `Speaker 1..N` by first speech; `get_transcript` then adds `speaker` to each word and `speakers` to the result, and the caption exports use the names. Give `num_speakers` when you know how many people talk, or `min_speakers` / `max_speakers`. `headset: true` means the microphone only hears the user: every microphone word is labelled with the setting `profile.display_name` (default `Me`) and only the system-audio track is diarized. A recording shorter than 10 s is skipped with a note, and one detected speaker gives a note too. Without an installed speaker model it fails with `invalid_input` and `data.reason = \"no_speaker_model\"`: tell the user to open the AI page in CaptureMann (nothing is downloaded automatically). Running it again replaces the previous result and the renames made to it.",
            json!({ "capture_id": capture_id(),
                    "num_speakers": { "type": "integer", "minimum": 1, "maximum": 32, "description": "Exactly this many speakers; wins over min and max." },
                    "min_speakers": { "type": "integer", "minimum": 1, "maximum": 32 },
                    "max_speakers": { "type": "integer", "minimum": 1, "maximum": 32 },
                    "headset": { "type": "boolean", "description": "My microphone only hears me. Default false." } }),
            &["capture_id"],
            job_started_output(json!({
                "capture_id": { "type": "string" }, "headset": { "type": "boolean" } })),
        ),
        ai_tool(
            "set_speakers",
            Tier::Free,
            true,
            "AI-8",
            "Rename, merge and reassign the speakers of a diarized recording (run `diarize` first). `rename` maps a speaker id to a new name (`{ \"2\": \"Linh\" }`; names are unique, 1 to 60 characters), `merge` folds `from` into `into` (the `from` speaker disappears and every word it had moves to `into`), `assign` gives the time range `start` to `end` (session time) to a speaker. They apply in that order and all together are ONE undo step, tagged with the caller; undo restores the earlier names and labels. A call that changes nothing makes no revision (`changed: false`). The words and exports show the new names at once.",
            json!({ "rename": { "type": "object", "maxProperties": 500,
                        "description": "Speaker id (as text) to the new name.",
                        "additionalProperties": { "type": "string", "minLength": 1, "maxLength": 60 } },
                    "merge": { "type": "array", "maxItems": 500, "items": { "type": "object", "properties": {
                        "from": { "type": "integer" }, "into": { "type": "integer" } },
                        "required": ["from", "into"], "additionalProperties": false } },
                    "assign": { "type": "array", "maxItems": 500, "items": { "type": "object", "properties": {
                        "start": time("Range start"), "end": time("Range end"),
                        "speaker": { "type": "integer" } },
                        "required": ["start", "end", "speaker"], "additionalProperties": false } } }),
            &["capture_id"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" },
                "changed": { "type": "boolean" },
                "revision": { "type": ["integer", "null"] }, "action": { "type": "string" }, "by": { "type": "string" },
                "speakers": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "integer" }, "name": { "type": "string" } },
                    "required": ["id", "name"] } },
                "can_undo": { "type": "boolean" }, "can_redo": { "type": "boolean" } },
                "required": ["capture_id", "changed", "speakers"] }),
        ),
        ai_tool(
            "list_report_templates",
            Tier::Free,
            false,
            "AI-9",
            "List the built-in report templates (Summary, Meeting minutes, One-on-one notes, Interview notes, Product presentation recap, Lecture notes): their sections, the context fields each one uses and which recording presets suggest it. Interview notes only collect evidence; they never score a candidate.",
            json!({}),
            &[],
            json!({ "type": "object", "properties": {
                "templates": { "type": "array", "items": { "type": "object", "properties": {
                    "id": { "type": "string" }, "name": { "type": "string" },
                    "version": { "type": "integer" }, "description": { "type": "string" },
                    "sections": { "type": "array", "items": { "type": "object", "properties": {
                        "id": { "type": "string" }, "title": { "type": "string" } },
                        "required": ["id", "title"] } },
                    "context_fields": { "type": "array", "items": { "type": "string" } },
                    "suggested_for": { "type": "array", "items": { "type": "string" } },
                    "notes_only": { "type": "string" } },
                    "required": ["id", "name", "sections", "context_fields"] } } },
                "required": ["templates"] }),
        ),
        ai_tool(
            "generate_report",
            Tier::Free,
            true,
            "AI-9",
            "Write a structured report (meeting minutes, notes, recap, summary) from a recording's transcript with the built-in text model (any other provider gives `no_text_model`). Runs in the background and returns a `job_id`; the job result is `{ report_id, dropped_uncited }` and `report.ready` is published. Every item cites the recording with `at` times; items whose citations do not match the transcript are dropped and counted in `dropped_uncited`. The transcript must exist (run `transcribe`) and have at least 30 words. Pass `report_id` to regenerate a report: the old one is replaced only when the new one succeeds. One undo step.",
            json!({ "template": { "type": "string", "enum": ["summary", "minutes", "one_on_one", "interview", "product_recap", "lecture"],
                        "description": "A template id from list_report_templates." },
                    "language": { "type": "string", "description": "Language of the note text (`vi`, `es`, `pt-br`); default the transcript's. Quotes stay as spoken." },
                    "context": { "type": "object", "additionalProperties": false,
                        "description": "Optional facts the notes may use. Only the fields the template lists in `context_fields` are accepted.",
                        "properties": {
                            "title": { "type": "string", "maxLength": 200 },
                            "attendees": { "type": "array", "maxItems": 50, "items": { "type": "string", "maxLength": 80 },
                                "description": "Names an owner or attendee may be." },
                            "agenda": { "type": "string", "maxLength": 4000 },
                            "role": { "type": "string", "maxLength": 200, "description": "Interview notes: the role." },
                            "competencies": { "type": "array", "maxItems": 30, "items": { "type": "string", "maxLength": 120 },
                                "description": "Interview notes: competencies to collect evidence for." } } },
                    "report_id": { "type": "string", "pattern": "^rpt_[0-9a-f]{8}$", "description": "Regenerate this report." } }),
            &["capture_id", "template"],
            job_started_output(json!({
                "capture_id": { "type": "string" }, "template": { "type": "string" },
                "report_id": { "type": ["string", "null"] } })),
        ),
        ai_tool(
            "get_report",
            Tier::Free,
            false,
            "AI-9",
            "Read a report as JSON (`report.content`: sections of items with `at` times in seconds) and as Markdown. Without `report_id` it lists the recording's reports (`reports`) and returns the newest one in full (`latest`).",
            json!({ "report_id": { "type": "string", "pattern": "^rpt_[0-9a-f]{8}$" } }),
            &["capture_id"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "report_id": { "type": "string" },
                "report": { "type": "object", "description": "The stored report: template, model, provider, created, language, context, content, dropped_uncited, edited." },
                "markdown": { "type": "string" },
                "reports": { "type": "array", "items": { "type": "object" } },
                "latest": { "type": ["object", "null"] } },
                "required": ["capture_id"] }),
        ),
        ai_tool(
            "export_report",
            Tier::Free,
            true,
            "AI-9",
            "Write a report as a Markdown (`md`) or plain text (`txt`) file, into `out_dir` (an existing absolute folder) or the recording's `exports/` folder. Never overwrites a file.",
            json!({ "report_id": { "type": "string", "pattern": "^rpt_[0-9a-f]{8}$" },
                    "format": { "type": "string", "enum": ["md", "txt"] },
                    "out_dir": { "type": "string", "description": "An existing absolute folder." },
                    "include_times": { "type": "boolean", "description": "Print the citation times ([mm:ss]) after each item. Default false: the text is cleaner without them; the stored report keeps them." } }),
            &["capture_id", "report_id", "format"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "report_id": { "type": "string" },
                "format": { "type": "string" }, "path": { "type": "string" } },
                "required": ["capture_id", "report_id", "format", "path"] }),
        ),
        ai_tool(
            "delete_report",
            Tier::Free,
            true,
            "AI-9",
            "Delete one report of a recording. One revision; `undo` brings it back.",
            json!({ "report_id": { "type": "string", "pattern": "^rpt_[0-9a-f]{8}$" } }),
            &["capture_id", "report_id"],
            report_revision_output(),
        ),
        ai_tool(
            "edit_report",
            Tier::Free,
            true,
            "AI-9",
            "Replace a report's Markdown with your own text (the Report panel's edit). The report is marked `edited`; its structured `content` stays as generated. One revision; `undo` brings the previous text back.",
            json!({ "report_id": { "type": "string", "pattern": "^rpt_[0-9a-f]{8}$" },
                    "markdown": { "type": "string", "minLength": 1, "maxLength": 400000 } }),
            &["capture_id", "report_id", "markdown"],
            report_revision_output(),
        ),
        ai_tool(
            "check_report_item",
            Tier::Free,
            true,
            "AI-9",
            "Tick or untick one task of a report (an item of `action_items`, `agreed_actions`, `follow_ups` or `next_steps`; `index` counts from 0 in the section of `get_report`'s `report.content`). Ticks are in `report.checked` as `<section>:<index>`. One revision; `undo` brings the earlier state back. Ticking an item that is already ticked changes nothing (`changed: false`).",
            json!({ "report_id": { "type": "string", "pattern": "^rpt_[0-9a-f]{8}$" },
                    "section": { "type": "string", "enum": ["action_items", "agreed_actions", "follow_ups", "next_steps"] },
                    "index": { "type": "integer", "minimum": 0, "maximum": 999 },
                    "checked": { "type": "boolean", "default": true } }),
            &["capture_id", "report_id", "section", "index"],
            json!({ "type": "object", "properties": {
                "capture_id": { "type": "string" }, "report_id": { "type": "string" },
                "section": { "type": "string" }, "index": { "type": "integer" },
                "checked": { "type": "boolean" }, "changed": { "type": "boolean" },
                "revision": { "type": "integer" }, "action": { "type": "string" },
                "by": { "type": "string" },
                "can_undo": { "type": "boolean" }, "can_redo": { "type": "boolean" } },
                "required": ["capture_id", "report_id", "section", "index", "checked", "changed"] }),
        ),
        ai_tool(
            "suggest_zooms",
            Tier::Free,
            false,
            "AI-3",
            "Suggest zoom regions from the recording's clicks and cursor activity (deterministic, no network). Nothing is applied: pass the ones you accept to `set_zooms` (each has `start_s`, `end_s`, `focus` and `scale`).",
            json!({}),
            &["capture_id"],
            suggestions_output(json!({ "type": "object", "properties": {
                "start": { "type": "integer" }, "end": { "type": "integer" },
                "start_s": { "type": "number" }, "end_s": { "type": "number" },
                "focus": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 },
                "scale": { "type": "number" },
                "reason": { "type": "string", "description": "`click`, `typing` or `activity`." } },
                "required": ["start_s", "end_s", "focus", "scale", "reason"] })),
        ),
        ai_tool(
            "suggest_cuts",
            Tier::Free,
            false,
            "AI-4",
            "Suggest cuts for silences (from the audio) and filler words (from the transcript, when one exists). Nothing is applied: pass the ranges you accept to `cut_segments` (use `start_s` and `end_s`).",
            json!({}),
            &["capture_id"],
            suggestions_output(json!({ "type": "object", "properties": {
                "start": { "type": "integer" }, "end": { "type": "integer" },
                "start_s": { "type": "number" }, "end_s": { "type": "number" },
                "kind": { "type": "string", "description": "`silence` or `filler`." },
                "detail": { "type": "string" } },
                "required": ["start_s", "end_s", "kind"] })),
        ),
    ]
    .into_iter()
    .map(with_capture_input)
    .collect()
}

/// The tools that work on one capture take `capture_id`; the rest do not.
fn with_capture_input(mut t: ToolSpec) -> ToolSpec {
    let on_capture = matches!(
        t.name,
        "transcribe"
            | "get_transcript"
            | "get_captions"
            | "set_captions"
            | "replace_in_captions"
            | "export_captions"
            | "translate_captions"
            | "suggest_zooms"
            | "suggest_cuts"
            | "diarize"
            | "set_speakers"
            | "generate_report"
            | "get_report"
            | "export_report"
            | "delete_report"
            | "edit_report"
            | "check_report_item"
    );
    if on_capture {
        if let Some(p) = t.input["properties"].as_object_mut() {
            p.insert("capture_id".into(), capture_id());
        }
        if let Some(r) = t.input["required"].as_array_mut() {
            if !r.iter().any(|v| v == "capture_id") {
                r.insert(0, json!("capture_id"));
            }
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_are_objects_and_names_unique() {
        let all = all();
        let mut names = std::collections::HashSet::new();
        for t in &all {
            assert!(names.insert(t.name));
            assert_eq!(t.input["type"], "object", "{}", t.name);
            assert_eq!(t.output["type"], "object", "{}", t.name);
            let tool = t.to_tool();
            assert!(tool.description.as_deref().unwrap_or("").contains("Tier:"));
        }
    }
}
