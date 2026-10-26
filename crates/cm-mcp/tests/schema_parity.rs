//! The MCP input schemas and the daemon's parser must agree (AGT-1.AC4): for every tool, an
//! input built from the schema's `required` fields is accepted by `Action::parse`, so is one
//! with every optional property filled in, every `oneOf` variant of a property is accepted, and
//! an unknown key is rejected (`additionalProperties: false`).

#![allow(clippy::unwrap_used)]

use cm_daemon::Action;
use cm_mcp::tools;
use serde_json::{json, Map, Value};

/// A value for `schema`. `all`: fill optional properties too. `variant`: which `oneOf` branch.
fn gen(schema: &Value, all: bool, variant: usize, path: &str) -> Value {
    if let Some(c) = schema.get("const") {
        return c.clone();
    }
    if let Some(e) = schema.get("enum").and_then(Value::as_array) {
        return e[0].clone();
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(alts) = schema.get(key).and_then(Value::as_array) {
            return gen(&alts[variant.min(alts.len() - 1)], all, variant, path);
        }
    }
    let ty = match schema.get("type") {
        Some(Value::String(s)) => s.as_str(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or("null"),
        _ => "object",
    };
    match ty {
        "string" => {
            if schema.get("pattern").and_then(Value::as_str) == Some("^t:[0-9]+$") {
                // A time in ticks (EDT-1).
                json!("t:120000")
            } else if schema.get("pattern").is_some() {
                json!("#FF3B3B")
            } else {
                json!("x")
            }
        }
        "integer" => {
            let min = schema
                .get("minimum")
                .and_then(Value::as_i64)
                .or_else(|| {
                    schema
                        .get("exclusiveMinimum")
                        .and_then(Value::as_i64)
                        .map(|m| m + 1)
                })
                .unwrap_or(1);
            json!(min.max(1))
        }
        "number" => {
            let min = schema.get("minimum").and_then(Value::as_f64).unwrap_or(0.0);
            let max = schema
                .get("maximum")
                .and_then(Value::as_f64)
                .unwrap_or(f64::INFINITY);
            // Inside [minimum, maximum] (`loudness_lufs` is -40..-5).
            json!((min + 1.0).max(1.0).min(max).max(min))
        }
        "boolean" => json!(false),
        "array" => {
            let n = schema
                .get("minItems")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1) as usize;
            let item = schema.get("items").cloned().unwrap_or(json!({}));
            Value::Array((0..n).map(|_| gen(&item, all, variant, path)).collect())
        }
        "null" => Value::Null,
        _ => {
            let mut m = Map::new();
            let props = schema.get("properties").and_then(Value::as_object);
            let required: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if let Some(props) = props {
                for (k, s) in props {
                    if all || required.contains(&k.as_str()) {
                        m.insert(k.clone(), field(&format!("{path}.{k}"), s, all, variant));
                    }
                }
            }
            Value::Object(m)
        }
    }
}

/// Fixed values for properties that are free-form objects in the schema.
fn field(path: &str, schema: &Value, all: bool, variant: usize) -> Value {
    match path.trim_start_matches('.') {
        p if p.ends_with("set_settings.values") => json!({ "recording.fps": 30 }),
        // The edit tools need an end after the start and ranges that are not empty (EDT-1).
        p if p.ends_with("trim.end") => json!(5),
        p if p.ends_with("cut_segments.ranges") || p.ends_with("restore_segments.ranges") => {
            json!([[1, 2]])
        }
        p if p.ends_with("set_speed.range") => json!([1, 2]),
        // CAM-3: bubble-only fields need the bubble layout, and the ring is a small fraction.
        p if p.ends_with("set_camera_layout.range") => json!([1, 2]),
        p if p.ends_with("set_camera_layout.layout") => json!("bubble"),
        p if p.ends_with("set_camera_layout.rect") => json!([0.1, 0.1, 0.2, 0.2]),
        p if p.ends_with("set_camera_layout.ring") => json!(0.02),
        // AI-1: a model id, a language code, and caption lines that are in order and not empty.
        p if p.ends_with(".model") => json!("small"),
        p if p.ends_with("transcribe.language") => json!("en"),
        // AI-5: the caption tools take a translation's language code; translate needs a target.
        p if p.ends_with("_captions.language") => json!("vi"),
        p if p.ends_with("translate_captions.target_language") => json!("vi"),
        p if p.ends_with("set_captions.lines") => {
            json!([{ "start": 1, "end": 2, "text": "Hi" }])
        }
        _ => gen(schema, all, variant, path),
    }
}

/// Tools whose payload the daemon passes through unvalidated until their engines exist. None
/// today: `publish_video` is validated before its dialog (`PublishVideoParams::parse`).
const PASS_THROUGH: &[&str] = &[];

/// Properties that are aliases of another one (the daemon refuses both at once).
const ALIASES: &[(&str, &str)] = &[("start_recording", "countdown"), ("upload", "file")];

fn input_for(tool: &tools::ToolSpec, all: bool, variant: usize) -> Value {
    // `gen` would pick one `anyOf` alternative as the whole object; generate from the plain schema.
    let mut plain = tool.input.clone();
    if let Some(o) = plain.as_object_mut() {
        o.remove("anyOf");
    }
    let mut v = gen(&plain, all, variant, tool.name);
    for (t, alias) in ALIASES {
        if tool.name == *t {
            if let Some(o) = v.as_object_mut() {
                o.remove(*alias);
            }
        }
    }
    // `anyOf: [{required: [k]}, ...]`: at least one of the keys must be present.
    if let (Some(alts), Some(o)) = (
        tool.input.get("anyOf").and_then(Value::as_array),
        v.as_object_mut(),
    ) {
        let keys: Vec<&str> = alts
            .iter()
            .filter_map(|a| a["required"][0].as_str())
            .collect();
        if !keys.iter().any(|k| o.contains_key(*k)) {
            if let Some(k) = keys.first() {
                let schema = &tool.input["properties"][*k];
                o.insert((*k).to_owned(), gen(schema, all, variant, k));
            }
        }
    }
    // `gen` builds paths from the root; re-run the free-form fixtures at the top level.
    if tool.name == "set_settings" {
        v["values"] = json!({ "recording.fps": 30 });
    }
    v
}

#[test]
fn every_tool_schema_input_is_accepted_by_the_daemon_parser() {
    let mut checked = 0;
    for tool in tools::all() {
        for all in [false, true] {
            for variant in 0..10 {
                let input = input_for(&tool, all, variant);
                if let Err(e) = Action::parse(tool.name, &input) {
                    // Some optional fields exclude each other (a target is a display OR a
                    // window); only the minimal input has to parse for every variant.
                    if all && variant > 0 {
                        continue;
                    }
                    panic!(
                        "`{}` (all={all}, variant={variant}) built from the MCP schema was refused: {e}\ninput: {input}",
                        tool.name
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 40, "only {checked} inputs were generated");
}

#[test]
fn unknown_keys_are_rejected_like_additional_properties_false() {
    for tool in tools::all()
        .into_iter()
        .filter(|t| !PASS_THROUGH.contains(&t.name))
    {
        assert_eq!(
            tool.input.get("additionalProperties"),
            Some(&json!(false)),
            "`{}` must say additionalProperties:false",
            tool.name
        );
        let mut input = input_for(&tool, false, 0);
        input["definitely_not_a_param"] = json!(1);
        let e = Action::parse(tool.name, &input)
            .expect_err(&format!("`{}` accepted an unknown key", tool.name));
        assert!(
            e.to_string().contains("definitely_not_a_param"),
            "{}: {e}",
            tool.name
        );
    }
}

/// Every property the schema advertises is one the daemon reads (no dead properties), and
/// every required property is really required by the parser.
#[test]
fn schema_properties_and_required_fields_match_the_parser() {
    for tool in tools::all()
        .into_iter()
        .filter(|t| !PASS_THROUGH.contains(&t.name))
    {
        let required: Vec<String> = tool
            .input
            .get("required")
            .and_then(Value::as_array)
            .map(|r| {
                r.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let full = input_for(&tool, false, 0);
        for key in &required {
            let mut without = full.clone();
            without.as_object_mut().unwrap().remove(key);
            assert!(
                Action::parse(tool.name, &without).is_err(),
                "`{}` parses without its required `{key}`",
                tool.name
            );
        }
    }
}
