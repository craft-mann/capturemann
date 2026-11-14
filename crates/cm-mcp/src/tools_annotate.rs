//! 0.3.4 annotation tools: the schema of the new ops (Zoom Lens `magnify`, base edits `retype`,
//! `move_area` and `blueprint`) and the `suggest_blueprint` tool (ANN-8 to ANN-11). In its own
//! module so the annotation work does not collide with other edits of `tools.rs`.

use serde_json::{json, Value};

use crate::tools::{Tier, ToolSpec};

fn rgb() -> Value {
    json!({ "type": "string", "pattern": "^#[0-9A-Fa-f]{6}$", "description": "#RRGGBB" })
}

fn rect() -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4,
            "description": "[x, y, w, h] in physical px of the original image" })
}

/// A fill as stored in `retype`, `move_area` and `blueprint` (ANN-9): measured from the ring of
/// pixels around the box when the op is made. `suggest_blueprint` returns one per box.
fn fill() -> Value {
    json!({ "type": "object", "properties": {
        "kind": { "enum": ["solid", "linear"] },
        "colors": { "type": "array", "items": rgb(), "minItems": 1, "maxItems": 8,
                    "description": "`solid`: exactly 1 colour. `linear`: 2 to 8 evenly spaced stops." },
        "angle": { "type": "number", "default": 90,
                   "description": "Degrees for `linear`: 0 runs left to right, 90 top to bottom." } },
        "required": ["kind", "colors"], "additionalProperties": false,
        "description": "Background that hides the old pixels. Stored in the op, so every client renders the same." })
}

/// The `oneOf` variants of the 0.3.4 ops, appended to the `annotate` op schema.
pub fn op_variants() -> Vec<Value> {
    let id = json!({ "type": "string", "description": "Optional unique op id." });
    let variant = |name: &str, props: Value, required: &[&str], description: &str| {
        let mut p = props;
        if let Some(m) = p.as_object_mut() {
            m.insert("op".into(), json!({ "const": name }));
            m.insert("id".into(), id.clone());
        }
        let mut req = vec!["op"];
        req.extend_from_slice(required);
        json!({ "type": "object", "properties": p, "required": req, "description": description })
    };
    vec![
        variant(
            "magnify",
            json!({
                "source": rect(),
                "at": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2,
                        "description": "Centre of the loupe, [x, y]. The loupe is `source` scaled by `zoom`." },
                "zoom": { "type": "number", "minimum": 2, "maximum": 8, "default": 3 },
                "shape": { "enum": ["circle", "rounded", "rect"], "default": "rounded" },
                "border": rgb(),
                "border_width": { "type": "number", "minimum": 0, "maximum": 32, "default": 3 },
                "connector": { "type": "boolean", "default": true, "description": "Two thin lines from the source to the loupe." },
                "badge": { "type": "boolean", "default": true, "description": "A pill such as x3 on the loupe." } }),
            &["source", "at"],
            "Zoom Lens (at most 4 per image; source at least 8x8 px; the loupe at most 16384 px a side and 32 megapixels). Rendered again from the original, with the ops listed BEFORE it drawn inside; blur and pixelate hide every copy of the pixels wherever they are listed.",
        ),
        variant(
            "retype",
            json!({
                "rect": rect(),
                "fill": fill(),
                "text": { "type": "string", "maxLength": 2000 },
                "size": { "type": "number", "exclusiveMinimum": 0, "maximum": 2000, "default": 24 },
                "color": rgb(),
                "weight": { "type": "integer", "minimum": 100, "maximum": 900, "default": 400, "description": "600 and above draw bold." },
                "align": { "enum": ["left", "center", "right"], "default": "left" } }),
            &["rect", "fill", "text"],
            "BASE EDIT: replaces the text in `rect` (fill, then the new text in the UI font, clipped to `rect`). Base edits must come before every other op, or the call is invalid_input naming ops[i]; to add one to an image that has ops, send the whole list with `replace: true`. Not redaction: use blur or pixelate for secrets.",
        ),
        variant(
            "move_area",
            json!({
                "source": rect(),
                "to": { "type": ["array", "null"], "items": { "type": "number" }, "minItems": 2, "maxItems": 2,
                        "description": "New top-left corner [x, y]; null removes the contents." },
                "fill": fill() }),
            &["source", "fill"],
            "BASE EDIT: moves (or, with `to: null`, removes) the pixels in `source`; the hole is painted with `fill`. Order: copy, fill, paste.",
        ),
        variant(
            "blueprint",
            json!({
                "boxes": { "type": "array", "items": rect(), "minItems": 1, "maxItems": 2000,
                           "description": "Text lines to replace (from suggest_blueprint); leave a line out to keep its text." },
                "style": { "enum": ["bar", "box"], "default": "bar" },
                "color": { "type": "string", "default": "auto", "description": "`auto` or #RRGGBB." },
                "radius": { "type": "number", "minimum": 0, "maximum": 2000, "default": 3 },
                "colors": { "type": "array", "items": rgb(), "description": "Bar colour per box for `auto` (suggest_blueprint's `color`); empty: measured at render time." },
                "fills": { "type": "array", "items": fill(), "description": "Fill per box (suggest_blueprint's `fill`); empty: measured at render time." } }),
            &["boxes"],
            "BASE EDIT: replaces text lines by neutral bars so the layout shows and the words do not. Same ordering rule as retype.",
        ),
    ]
}

/// The tool specs this module adds.
pub fn tools() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "suggest_blueprint",
        tier: Tier::Free,
        mutating: false,
        destructive: false,
        spec: "ANN-11",
        summary: "Find the text lines of a screenshot with on-device text recognition and return them as boxes for a `blueprint` op (nothing is stored, nothing leaves the computer). It reads the original picture with the capture's own base edits (retype, move_area, blueprint) applied. Each box has the measured `fill` and bar `color` so the op can store them; pass the boxes you want replaced (leave out the lines whose text should stay) to `annotate` as a `blueprint` op, listed before every non-base op. Fails with `not_implemented` when this computer has no text recognizer.",
        input: json!({ "type": "object", "properties": { "capture_id": { "type": "string", "minLength": 1, "description": "Capture id of a screenshot." } },
            "required": ["capture_id"], "additionalProperties": false }),
        output: json!({ "type": "object", "properties": {
            "capture_id": { "type": "string" },
            "recognizer": { "type": "string" },
            "image_size": { "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 },
            "boxes": { "type": "array", "description": "In reading order.", "items": { "type": "object", "properties": {
                "rect": rect(),
                "text_preview": { "type": "string" },
                "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                "fill": fill(),
                "color": rgb(),
                "busy": { "type": "boolean", "description": "The background around this line is too busy to fill cleanly." } },
                "required": ["rect", "text_preview", "confidence"] } } },
            "required": ["capture_id", "recognizer", "image_size", "boxes"] }),
    }]
}
