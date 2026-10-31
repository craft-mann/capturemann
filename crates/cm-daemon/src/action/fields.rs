use super::*;

pub(crate) struct Fields<'a> {
    pub(crate) obj: &'a Map<String, Value>,
}

impl<'a> Fields<'a> {
    pub(crate) fn new(params: &'a Value, empty: &'a Map<String, Value>) -> Result<Self, CmError> {
        match params {
            Value::Null => Ok(Fields { obj: empty }),
            Value::Object(m) => Ok(Fields { obj: m }),
            _ => Err(CmError::invalid_field("params", "must be an object")),
        }
    }

    /// Rejects keys outside `allowed` (typos would otherwise be silently ignored).
    pub(crate) fn only(&self, allowed: &[&str]) -> Result<(), CmError> {
        for k in self.obj.keys() {
            if !allowed.contains(&k.as_str()) {
                return Err(CmError::invalid_field(k, "unknown parameter"));
            }
        }
        Ok(())
    }

    pub(crate) fn str_opt(&self, key: &str) -> Result<Option<String>, CmError> {
        match self.obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(CmError::invalid_field(key, "must be a string")),
        }
    }

    pub(crate) fn str_req(&self, key: &str) -> Result<String, CmError> {
        match self.str_opt(key)? {
            Some(s) if !s.trim().is_empty() => Ok(s),
            Some(_) => Err(CmError::invalid_field(key, "must not be empty")),
            None => Err(CmError::invalid_field(key, "is required")),
        }
    }

    pub(crate) fn bool_or(&self, key: &str, default: bool) -> Result<bool, CmError> {
        match self.obj.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(CmError::invalid_field(key, "must be a boolean")),
        }
    }

    pub(crate) fn request_id(&self) -> Result<Option<String>, CmError> {
        match self.str_opt("request_id")? {
            Some(s) if s.is_empty() || s.len() > MAX_REQUEST_ID_LEN => Err(CmError::invalid_field(
                "request_id",
                format!("must be 1..={MAX_REQUEST_ID_LEN} characters"),
            )),
            other => Ok(other),
        }
    }
}

/// The preset id of an `export_video`: `preset`, or the one a `format` of `gif` or `webm` stands
/// for; `quick` when neither is given.
pub(super) fn parse_export_preset(f: &Fields<'_>) -> Result<String, CmError> {
    let preset = f.str_opt("preset")?;
    if let Some(p) = &preset {
        let ok = !p.is_empty()
            && p.len() <= 40
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !ok {
            return Err(CmError::invalid_field(
                "preset",
                "must be a preset id such as `youtube-1080p`, `vertical-1080`, `gif` or `webm`",
            ));
        }
    }
    let format = f.str_opt("format")?;
    match (format.as_deref(), preset) {
        (None, p) => Ok(p.unwrap_or_else(|| "quick".to_owned())),
        (Some("mp4"), Some(p)) if p == "gif" || p == "webm" => Err(CmError::invalid_field(
            "format",
            format!("`mp4` does not match the preset `{p}`"),
        )),
        (Some("mp4"), p) => Ok(p.unwrap_or_else(|| "quick".to_owned())),
        (Some(fmt @ ("gif" | "webm")), None) => Ok(fmt.to_owned()),
        (Some(fmt @ ("gif" | "webm")), Some(p)) if p == fmt => Ok(p),
        (Some(fmt @ ("gif" | "webm")), Some(p)) => Err(CmError::invalid_field(
            "format",
            format!("`{fmt}` does not match the preset `{p}`"),
        )),
        (Some(_), _) => Err(CmError::invalid_field(
            "format",
            "must be `mp4`, `gif` or `webm`",
        )),
    }
}

/// `width`, `fps` and `max_mb` of an `export_video` (EXP-3). Which presets take them is checked
/// where the preset is resolved.
#[allow(clippy::type_complexity)]
pub(super) fn parse_export_options(
    f: &Fields<'_>,
) -> Result<(Option<u32>, Option<f64>, Option<f64>), CmError> {
    let number = |key: &str| -> Result<Option<f64>, CmError> {
        match f.obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|n| n.is_finite() && *n > 0.0)
                .map(Some)
                .ok_or_else(|| CmError::invalid_field(key, "must be a positive number")),
        }
    };
    let width = match number("width")? {
        None => None,
        Some(w) if w.fract() == 0.0 && (16.0..=7680.0).contains(&w) => Some(w as u32),
        Some(_) => {
            return Err(CmError::invalid_field(
                "width",
                "must be a whole number of pixels from 16 to 7680",
            ))
        }
    };
    let fps = number("fps")?;
    if fps.is_some_and(|f| f > 60.0) {
        return Err(CmError::invalid_field("fps", "must be from 1 to 60"));
    }
    Ok((width, fps, number("max_mb")?))
}

pub(super) fn recording_id(f: &Fields<'_>) -> Result<Option<String>, CmError> {
    match f.str_opt("recording_id")? {
        Some(s) if s.trim().is_empty() => {
            Err(CmError::invalid_field("recording_id", "must not be empty"))
        }
        other => Ok(other),
    }
}

/// `capture_screenshot` whose `target` is `{ frame_id, rect? }`.
pub(super) fn parse_from_frame(
    f: &Fields<'_>,
    target: &Value,
) -> Result<CaptureFromFrameParams, CmError> {
    let obj = target
        .as_object()
        .ok_or_else(|| CmError::invalid_field("target", "must be an object"))?;
    for k in obj.keys() {
        if !matches!(k.as_str(), "frame_id" | "rect") {
            return Err(CmError::invalid_field(
                "target",
                format!("unknown key `{k}` (with `frame_id` only `rect` is allowed)"),
            ));
        }
    }
    let frame_id = match obj.get("frame_id") {
        Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
        _ => {
            return Err(CmError::invalid_field(
                "target.frame_id",
                "must be a non-empty string",
            ))
        }
    };
    let rect = match obj.get("rect") {
        None | Some(Value::Null) => None,
        Some(r) => Some(parse_rect(r)?),
    };
    if f.obj.get("delay_s").is_some_and(|v| !v.is_null()) {
        return Err(CmError::invalid_field(
            "delay_s",
            "is not allowed with `frame_id` (the frame is already captured)",
        ));
    }
    Ok(CaptureFromFrameParams {
        frame_id,
        rect,
        copy_to_clipboard: f.bool_or("copy_to_clipboard", false)?,
        title: f.str_opt("title")?,
        request_id: f.request_id()?,
    })
}

/// `target: { pick: true }`: only `pick` is allowed in the target, and no `delay_s`.
pub(super) fn parse_pick(f: &Fields<'_>, target: &Value) -> Result<CapturePickParams, CmError> {
    let obj = target
        .as_object()
        .ok_or_else(|| CmError::invalid_field("target", "must be an object"))?;
    if obj.len() != 1 {
        return Err(CmError::invalid_field(
            "target",
            "with `pick` no other key is allowed",
        ));
    }
    if obj.get("pick") != Some(&Value::Bool(true)) {
        return Err(CmError::invalid_field("target.pick", "must be `true`"));
    }
    if f.obj.get("delay_s").is_some_and(|v| !v.is_null()) {
        return Err(CmError::invalid_field(
            "delay_s",
            "is not allowed with `pick` (the person chooses when)",
        ));
    }
    Ok(CapturePickParams {
        copy_to_clipboard: f.bool_or("copy_to_clipboard", false)?,
        title: f.str_opt("title")?,
        request_id: f.request_id()?,
    })
}

/// `[x, y, w, h]`, whole physical px relative to the display, positive size.
pub(super) fn parse_rect(r: &Value) -> Result<[i64; 4], CmError> {
    let bad = || {
        CmError::invalid_field(
            "target.rect",
            "must be [x, y, w, h] in whole physical px relative to the display",
        )
    };
    let arr = r.as_array().filter(|a| a.len() == 4).ok_or_else(bad)?;
    let mut out = [0i64; 4];
    for (slot, n) in out.iter_mut().zip(arr) {
        *slot = n.as_i64().ok_or_else(bad)?;
    }
    if out[2] <= 0 || out[3] <= 0 {
        return Err(CmError::invalid_field(
            "target.rect",
            "width and height must be positive",
        ));
    }
    if out[0] < i64::from(i32::MIN)
        || out[0] > i64::from(i32::MAX)
        || out[1] < i64::from(i32::MIN)
        || out[1] > i64::from(i32::MAX)
        || out[2] > i64::from(u32::MAX)
        || out[3] > i64::from(u32::MAX)
    {
        return Err(bad());
    }
    Ok(out)
}
