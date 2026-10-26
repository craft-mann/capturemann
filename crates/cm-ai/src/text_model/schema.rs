//! A small JSON Schema checker for model replies (AI-7). It covers the subset the report,
//! translation and summary schemas use: `type`, `enum`, `const`, `properties`, `required`,
//! `additionalProperties` (false or a schema), `items`, `minItems`, `maxItems`, `minLength`,
//! `maxLength`, `minimum`, `maximum`, `anyOf` and `oneOf`. Unknown keywords are ignored. The
//! error names the path of the first mismatch so it can be sent back to the model for the retry.

use serde_json::Value;

/// Checks `value` against `schema`; `Err` is a one-line description of the first mismatch.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    check(schema, value, "$")
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "number" => v.is_number(),
        "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
        _ => true,
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn check(schema: &Value, v: &Value, path: &str) -> Result<(), String> {
    let Some(s) = schema.as_object() else {
        return Ok(()); // `true` or a malformed schema accepts anything
    };
    if let Some(t) = s.get("type") {
        let ok = match t {
            Value::String(t) => type_ok(t, v),
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).any(|t| type_ok(t, v)),
            _ => true,
        };
        if !ok {
            return Err(format!("{path}: expected {t}, got {}", kind(v)));
        }
    }
    if let Some(Value::Array(options)) = s.get("enum") {
        if !options.contains(v) {
            return Err(format!("{path}: not one of the allowed values"));
        }
    }
    if let Some(c) = s.get("const") {
        if c != v {
            return Err(format!("{path}: must equal {c}"));
        }
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(alts)) = s.get(key) {
            if !alts.iter().any(|a| check(a, v, path).is_ok()) {
                return Err(format!("{path}: matches none of the {key} alternatives"));
            }
        }
    }
    if let (Some(n), Some(min)) = (v.as_f64(), s.get("minimum").and_then(Value::as_f64)) {
        if n < min {
            return Err(format!("{path}: {n} is below the minimum {min}"));
        }
    }
    if let (Some(n), Some(max)) = (v.as_f64(), s.get("maximum").and_then(Value::as_f64)) {
        if n > max {
            return Err(format!("{path}: {n} is above the maximum {max}"));
        }
    }
    if let Some(text) = v.as_str() {
        let len = text.chars().count() as u64;
        if let Some(min) = s.get("minLength").and_then(Value::as_u64) {
            if len < min {
                return Err(format!("{path}: shorter than {min} characters"));
            }
        }
        if let Some(max) = s.get("maxLength").and_then(Value::as_u64) {
            if len > max {
                return Err(format!("{path}: longer than {max} characters"));
            }
        }
    }
    if let Some(items) = v.as_array() {
        let n = items.len() as u64;
        if let Some(min) = s.get("minItems").and_then(Value::as_u64) {
            if n < min {
                return Err(format!("{path}: fewer than {min} items"));
            }
        }
        if let Some(max) = s.get("maxItems").and_then(Value::as_u64) {
            if n > max {
                return Err(format!("{path}: more than {max} items"));
            }
        }
        if let Some(item_schema) = s.get("items") {
            for (i, item) in items.iter().enumerate() {
                check(item_schema, item, &format!("{path}[{i}]"))?;
            }
        }
    }
    if let Some(obj) = v.as_object() {
        if let Some(Value::Array(req)) = s.get("required") {
            for k in req.iter().filter_map(Value::as_str) {
                if !obj.contains_key(k) {
                    return Err(format!("{path}: missing required property `{k}`"));
                }
            }
        }
        let props = s.get("properties").and_then(Value::as_object);
        for (k, val) in obj {
            match props.and_then(|p| p.get(k)) {
                Some(ps) => check(ps, val, &format!("{path}.{k}"))?,
                None => match s.get("additionalProperties") {
                    Some(Value::Bool(false)) => {
                        return Err(format!("{path}: unexpected property `{k}`"));
                    }
                    Some(extra @ Value::Object(_)) => {
                        check(extra, val, &format!("{path}.{k}"))?;
                    }
                    _ => {}
                },
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_and_rejects() {
        let schema = json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "minLength": 1 },
                "n": { "type": "integer", "minimum": 0 },
                "tags": { "type": "array", "items": { "type": "string" }, "maxItems": 2 },
                "kind": { "enum": ["a", "b"] }
            },
            "required": ["title"],
            "additionalProperties": false
        });
        assert!(validate(
            &schema,
            &json!({"title": "x", "n": 3, "tags": ["a"], "kind": "a"})
        )
        .is_ok());
        let e = validate(&schema, &json!({"n": 1})).unwrap_err();
        assert!(e.contains("missing required property `title`"), "{e}");
        let e = validate(&schema, &json!({"title": "x", "extra": 1})).unwrap_err();
        assert!(e.contains("unexpected property `extra`"), "{e}");
        let e = validate(&schema, &json!({"title": "x", "tags": [1]})).unwrap_err();
        assert!(e.starts_with("$.tags[0]"), "{e}");
        assert!(validate(&schema, &json!({"title": "x", "n": 1.5})).is_err());
        assert!(validate(&schema, &json!({"title": "x", "kind": "c"})).is_err());
        assert!(validate(&schema, &json!([])).is_err());
    }
}
