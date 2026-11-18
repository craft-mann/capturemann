//! Quick Capture presets (QC-4, QC-5): the model, the `presets.json` file format and how a preset
//! fills the parameters of a call.
//!
//! Pure data and logic, no file I/O (the daemon reads and writes the file). A preset is a named
//! setup: what to capture (kind and target, never the area itself), the option tiles of that
//! kind, where to save, what happens afterwards and an optional shortcut.
//!
//! Unknown fields are kept everywhere (a team may share one file, and a newer version may add
//! keys): every struct carries the keys it does not know and writes them back.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::CmError;

/// The file name inside the app's settings folder.
pub const PRESETS_FILE: &str = "presets.json";
/// The version written to the file.
pub const PRESETS_VERSION: u32 = 1;
/// Most presets one file holds.
pub const MAX_PRESETS: usize = 64;
/// Longest preset name, in chars.
pub const MAX_NAME_CHARS: usize = 60;
/// Longest delay a screenshot preset may set, in seconds (CAP-5).
pub const MAX_DELAY_S: f64 = 30.0;
/// Longest countdown a recording preset may set, in seconds (REC-4).
pub const MAX_COUNTDOWN_S: u32 = 10;

/// What a preset makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    Screenshot,
    Video,
    Audio,
}

impl PresetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PresetKind::Screenshot => "screenshot",
            PresetKind::Video => "video",
            PresetKind::Audio => "audio",
        }
    }
}

/// What a screenshot or video preset captures (the kind of target, never a rectangle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetTarget {
    Region,
    Window,
    Display,
    /// A Long Shot (screenshots only).
    LongShot,
}

impl PresetTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            PresetTarget::Region => "region",
            PresetTarget::Window => "window",
            PresetTarget::Display => "display",
            PresetTarget::LongShot => "long_shot",
        }
    }
}

/// The picture format of a screenshot preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetImageFormat {
    Png,
    Jpeg,
}

/// The option tiles. Every field is optional: absent means "use the Settings value". A tile only
/// belongs to some kinds (listed per field); a tile of another kind makes the preset invalid.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetOptions {
    /// Screenshot: seconds before the capture, 0 to 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_s: Option<f64>,
    /// Screenshot: keep the pointer in the picture. Video: show the cursor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<bool>,
    /// Screenshot: the file format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<PresetImageFormat>,
    /// Screenshot: 1 or 2 (a 2x file keeps the display's physical pixels).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u8>,
    /// Video and audio: record the microphone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mic: Option<bool>,
    /// Video and audio: record the system sound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_audio: Option<bool>,
    /// Video: show the webcam bubble.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<bool>,
    /// Video: frames per second, 30 or 60.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    /// Video and audio: seconds of countdown, 0 to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub countdown_s: Option<u32>,
    /// Keys this version does not know, kept when the file is written again.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What happens after the capture.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AfterCapture {
    /// Screenshot: copy the picture to the clipboard.
    #[serde(default, skip_serializing_if = "is_false")]
    pub copy_to_clipboard: bool,
    /// Screenshot: run Auto-redact and apply the blur it proposes.
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_redact: bool,
    /// Open the capture in the editor.
    #[serde(default, skip_serializing_if = "is_false")]
    pub open_in_editor: bool,
    /// Screenshot: the editor tool that is selected when it opens (an annotation op name such as
    /// `step`). Needs `open_in_editor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Video and audio: transcribe when the recording ends.
    #[serde(default, skip_serializing_if = "is_false")]
    pub transcribe: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// One preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Stable id (the name may change). Lowercase letters, digits, `_` and `-`.
    pub id: String,
    pub name: String,
    pub kind: PresetKind,
    /// Absent for audio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<PresetTarget>,
    #[serde(default)]
    pub options: PresetOptions,
    /// The folder to save in; `{date}` becomes the local date (`2026-11-18`). Absent = the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_to: Option<String>,
    #[serde(default)]
    pub after: AfterCapture,
    /// A global shortcut in the app's notation (`Ctrl+Shift+B`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Preset {
    /// Checks the preset; the error names the first problem.
    pub fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
        if !id_ok {
            return Err("the id must be lowercase letters, digits, `_` or `-`".into());
        }
        let name = self.name.trim();
        if name.is_empty() {
            return Err("the name is empty".into());
        }
        if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
            return Err(format!(
                "the name must be one line of at most {MAX_NAME_CHARS} characters"
            ));
        }
        match (self.kind, self.target) {
            (PresetKind::Audio, Some(_)) => return Err("an audio preset has no target".into()),
            (PresetKind::Audio, None) => {}
            (_, None) => return Err("the target is missing".into()),
            (PresetKind::Video, Some(PresetTarget::LongShot)) => {
                return Err("a video preset cannot use a Long Shot".into())
            }
            _ => {}
        }
        let o = &self.options;
        let a = &self.after;
        let screenshot = self.kind == PresetKind::Screenshot;
        let video = self.kind == PresetKind::Video;
        let sound = !screenshot;
        let misplaced = [
            ("delay_s", o.delay_s.is_some() && !screenshot),
            ("format", o.format.is_some() && !screenshot),
            ("scale", o.scale.is_some() && !screenshot),
            ("cursor", o.cursor.is_some() && !screenshot && !video),
            ("camera", o.camera.is_some() && !video),
            ("fps", o.fps.is_some() && !video),
            ("mic", o.mic.is_some() && !sound),
            ("system_audio", o.system_audio.is_some() && !sound),
            ("countdown_s", o.countdown_s.is_some() && !sound),
            ("copy_to_clipboard", a.copy_to_clipboard && !screenshot),
            ("auto_redact", a.auto_redact && !screenshot),
            ("tool", a.tool.is_some() && !screenshot),
            ("transcribe", a.transcribe && !sound),
            (
                "open_in_editor",
                a.open_in_editor && self.kind == PresetKind::Audio,
            ),
        ];
        if let Some((k, _)) = misplaced.iter().find(|(_, bad)| *bad) {
            return Err(format!(
                "`{k}` does not belong to a {} preset",
                self.kind.as_str()
            ));
        }
        if let Some(d) = o.delay_s {
            if !(0.0..=MAX_DELAY_S).contains(&d) {
                return Err(format!("`delay_s` must be 0 to {MAX_DELAY_S}"));
            }
        }
        if o.scale.is_some_and(|s| !matches!(s, 1 | 2)) {
            return Err("`scale` must be 1 or 2".into());
        }
        if o.fps.is_some_and(|f| !matches!(f, 30 | 60)) {
            return Err("`fps` must be 30 or 60".into());
        }
        if o.countdown_s.is_some_and(|c| c > MAX_COUNTDOWN_S) {
            return Err(format!("`countdown_s` must be 0 to {MAX_COUNTDOWN_S}"));
        }
        if let Some(t) = &a.tool {
            if !a.open_in_editor {
                return Err("`tool` needs `open_in_editor`".into());
            }
            if t.is_empty() || !t.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                return Err("`tool` must be an editor tool name such as `step`".into());
            }
        }
        if let Some(s) = &self.save_to {
            if s.trim().is_empty() || s.contains('\0') {
                return Err("`save_to` is empty".into());
            }
        }
        if let Some(s) = &self.shortcut {
            if s.trim().is_empty() {
                return Err("`shortcut` is empty".into());
            }
        }
        Ok(())
    }

    /// A one-line English summary (the UI translates its own): `Screenshot · Region · copy, redact`.
    pub fn summary(&self) -> String {
        let mut parts = vec![match self.kind {
            PresetKind::Screenshot => "Screenshot".to_owned(),
            PresetKind::Video => "Video".to_owned(),
            PresetKind::Audio => "Audio".to_owned(),
        }];
        if let Some(t) = self.target {
            parts.push(
                match t {
                    PresetTarget::Region => "Region",
                    PresetTarget::Window => "Window",
                    PresetTarget::Display => "Display",
                    PresetTarget::LongShot => "Long Shot",
                }
                .to_owned(),
            );
        }
        let (o, a) = (&self.options, &self.after);
        let mut tiles: Vec<String> = Vec::new();
        let on = |v: Option<bool>| v == Some(true);
        if on(o.mic) {
            tiles.push("mic".into());
        }
        if on(o.system_audio) {
            tiles.push("system sound".into());
        }
        if on(o.camera) {
            tiles.push("webcam".into());
        }
        if on(o.cursor) {
            tiles.push("cursor".into());
        }
        if a.copy_to_clipboard {
            tiles.push("copy".into());
        }
        if a.auto_redact {
            tiles.push("redact".into());
        }
        if a.open_in_editor {
            tiles.push("editor".into());
        }
        if a.transcribe {
            tiles.push("transcribe".into());
        }
        if !tiles.is_empty() {
            parts.push(tiles.join(", "));
        }
        parts.join(" · ")
    }

    /// The `save_to` folder with `{date}` replaced by `date` (`YYYY-MM-DD`). `None` without one.
    pub fn save_folder(&self, date: time::Date) -> Option<String> {
        self.save_to.as_deref().map(|t| expand_save_to(t, date))
    }
}

/// Replaces every `{date}` in `template` with `YYYY-MM-DD`. The caller passes the local date.
pub fn expand_save_to(template: &str, date: time::Date) -> String {
    let stamp = format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    );
    template.replace("{date}", &stamp)
}

/// The four starter presets, in order.
pub fn starter_presets() -> Vec<Preset> {
    let base = |id: &str, name: &str, kind, target| Preset {
        id: id.to_owned(),
        name: name.to_owned(),
        kind,
        target,
        options: PresetOptions::default(),
        save_to: None,
        after: AfterCapture::default(),
        shortcut: None,
        extra: Map::new(),
    };
    let mut bug = base(
        "bug-report",
        "Bug report",
        PresetKind::Screenshot,
        Some(PresetTarget::Region),
    );
    bug.after = AfterCapture {
        copy_to_clipboard: true,
        auto_redact: true,
        open_in_editor: true,
        tool: Some("step".into()),
        ..AfterCapture::default()
    };
    let mut docs = base(
        "docs-shot",
        "Docs shot",
        PresetKind::Screenshot,
        Some(PresetTarget::Window),
    );
    docs.options = PresetOptions {
        cursor: Some(true),
        format: Some(PresetImageFormat::Png),
        scale: Some(2),
        ..PresetOptions::default()
    };
    docs.after.open_in_editor = true;
    let mut demo = base(
        "demo-video",
        "Demo video",
        PresetKind::Video,
        Some(PresetTarget::Display),
    );
    demo.options = PresetOptions {
        mic: Some(true),
        camera: Some(true),
        countdown_s: Some(3),
        ..PresetOptions::default()
    };
    let mut meeting = base("meeting", "Meeting", PresetKind::Audio, None);
    meeting.options = PresetOptions {
        mic: Some(true),
        system_audio: Some(true),
        ..PresetOptions::default()
    };
    meeting.after.transcribe = true;
    vec![bug, docs, demo, meeting]
}

/// The content of `presets.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetFile {
    pub presets: Vec<Preset>,
    /// The starter presets were written once; they never come back.
    pub starters_seeded: bool,
    /// Entries of the file that were not valid presets, kept as they are so a save does not
    /// destroy a hand edit.
    pub skipped: Vec<Value>,
    /// Top-level keys this version does not know.
    pub extra: Map<String, Value>,
}

impl PresetFile {
    /// A new file with the four starter presets.
    pub fn with_starters() -> Self {
        PresetFile {
            presets: starter_presets(),
            starters_seeded: true,
            skipped: Vec::new(),
            extra: Map::new(),
        }
    }

    /// Reads the file text. An invalid preset is skipped; the second value is one notice that
    /// names every skipped preset. An error means the text is not a presets file at all.
    pub fn parse(text: &str) -> Result<(PresetFile, Option<String>), CmError> {
        let root: Value = serde_json::from_str(text).map_err(|e| {
            CmError::invalid_input(format!("{PRESETS_FILE} is not valid JSON: {e}"))
        })?;
        let Value::Object(mut obj) = root else {
            return Err(CmError::invalid_input(format!(
                "{PRESETS_FILE} must hold a JSON object"
            )));
        };
        let list = match obj.remove("presets") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a,
            Some(_) => {
                return Err(CmError::invalid_input(format!(
                    "`presets` in {PRESETS_FILE} must be a list"
                )))
            }
        };
        // A file that was not made by this app (a team's shared file) has no flag: its owner
        // chose the list, so the starters are not added to it.
        let starters_seeded = obj
            .remove("starters_seeded")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        obj.remove("version");
        let mut presets: Vec<Preset> = Vec::new();
        let mut skipped = Vec::new();
        let mut problems: Vec<String> = Vec::new();
        for (i, raw) in list.into_iter().enumerate() {
            let label = raw
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map_or_else(|| format!("#{}", i + 1), |s| format!("\"{s}\""));
            let parsed = serde_json::from_value::<Preset>(raw.clone())
                .map_err(|e| e.to_string())
                .and_then(|p| p.validate().map(|()| p));
            let parsed = parsed.and_then(|p| {
                if presets.iter().any(|q| q.id == p.id) {
                    Err(format!("the id `{}` is used twice", p.id))
                } else if presets
                    .iter()
                    .any(|q| q.name.trim().eq_ignore_ascii_case(p.name.trim()))
                {
                    Err("the name is used twice".to_owned())
                } else {
                    Ok(p)
                }
            });
            match parsed {
                Ok(p) => presets.push(p),
                Err(why) => {
                    problems.push(format!("{label} ({why})"));
                    skipped.push(raw);
                }
            }
        }
        let notice = (!problems.is_empty()).then(|| {
            format!(
                "{PRESETS_FILE}: skipped {} that {} not valid: {}",
                if problems.len() == 1 {
                    "1 preset"
                } else {
                    "presets"
                },
                if problems.len() == 1 { "is" } else { "are" },
                problems.join("; ")
            )
        });
        Ok((
            PresetFile {
                presets,
                starters_seeded,
                skipped,
                extra: obj,
            },
            notice,
        ))
    }

    /// The file text (pretty JSON, a trailing newline).
    pub fn to_text(&self) -> String {
        let mut obj = self.extra.clone();
        obj.insert("version".into(), json!(PRESETS_VERSION));
        obj.insert("starters_seeded".into(), json!(self.starters_seeded));
        let mut list: Vec<Value> = self
            .presets
            .iter()
            .map(|p| serde_json::to_value(p).unwrap_or(Value::Null))
            .collect();
        list.extend(self.skipped.iter().cloned());
        obj.insert("presets".into(), Value::Array(list));
        let mut s = serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default();
        s.push('\n');
        s
    }

    /// The preset with this id, or else this name (case does not matter).
    pub fn find(&self, id_or_name: &str) -> Option<&Preset> {
        let q = id_or_name.trim();
        self.presets.iter().find(|p| p.id == q).or_else(|| {
            self.presets
                .iter()
                .find(|p| p.name.trim().eq_ignore_ascii_case(q))
        })
    }

    /// Adds `p`, or replaces the preset with the same id (keeping its place). Checks the preset,
    /// the name (unique) and the shortcut (unique among presets; the error names the other).
    pub fn upsert(&mut self, p: Preset) -> Result<(), CmError> {
        p.validate()
            .map_err(|why| CmError::invalid_input(format!("preset \"{}\": {why}", p.name)))?;
        if let Some(o) = self
            .presets
            .iter()
            .find(|q| q.id != p.id && q.name.trim().eq_ignore_ascii_case(p.name.trim()))
        {
            return Err(CmError::invalid_input(format!(
                "a preset named \"{}\" exists already",
                o.name
            )));
        }
        if let Some(s) = &p.shortcut {
            if let Some(o) = self.presets.iter().find(|q| {
                q.id != p.id
                    && q.shortcut
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(s))
            }) {
                return Err(CmError::invalid_input(format!(
                    "the shortcut {s} is used by the preset \"{}\"",
                    o.name
                )));
            }
        }
        match self.presets.iter_mut().find(|q| q.id == p.id) {
            Some(slot) => *slot = p,
            None => {
                if self.presets.len() >= MAX_PRESETS {
                    return Err(CmError::invalid_input(format!(
                        "at most {MAX_PRESETS} presets"
                    )));
                }
                self.presets.push(p);
            }
        }
        Ok(())
    }

    /// Removes the preset with this id or name and returns it.
    pub fn remove(&mut self, id_or_name: &str) -> Result<Preset, CmError> {
        let id = self
            .find(id_or_name)
            .map(|p| p.id.clone())
            .ok_or_else(|| unknown_preset(id_or_name))?;
        let at = self.presets.iter().position(|p| p.id == id).unwrap_or(0);
        Ok(self.presets.remove(at))
    }

    /// Puts the presets in the order of `ids`; presets not named keep their order after them.
    pub fn reorder(&mut self, ids: &[String]) -> Result<(), CmError> {
        for id in ids {
            if self.find(id).is_none() {
                return Err(unknown_preset(id));
            }
        }
        let mut rest = std::mem::take(&mut self.presets);
        let mut out = Vec::with_capacity(rest.len());
        for id in ids {
            if let Some(i) = rest
                .iter()
                .position(|p| p.id == *id || p.name.eq_ignore_ascii_case(id))
            {
                out.push(rest.remove(i));
            }
        }
        out.extend(rest);
        self.presets = out;
        Ok(())
    }
}

/// `invalid_input` naming an unknown preset (QC-5.AC5).
pub fn unknown_preset(name: &str) -> CmError {
    CmError::invalid_input(format!("unknown preset \"{name}\""))
}

/// A new id for a preset called `name`: its slug, made unique among `taken`.
pub fn new_id(name: &str, taken: &[String]) -> String {
    let mut slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let base = if slug.is_empty() { "preset" } else { slug };
    let base: String = base.chars().take(40).collect();
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !taken.contains(c))
        .unwrap_or(base)
}

/// The audio profile names `start_recording` already knows under `preset` (VOC-1).
pub const AUDIO_PROFILE_NAMES: [&str; 5] = ["room", "online", "lecture", "product", "note"];

fn unset(params: &Map<String, Value>, key: &str) -> bool {
    params.get(key).is_none_or(Value::is_null)
}

/// Fills the parameters of `capture_screenshot` from `preset`; a key already in `params` wins
/// (QC-5.AC1, AC3). The target is not filled (a preset holds no area): the call still names it.
pub fn apply_to_screenshot(
    preset: &Preset,
    params: &mut Map<String, Value>,
) -> Result<(), CmError> {
    if preset.kind != PresetKind::Screenshot {
        return Err(CmError::invalid_input(format!(
            "the preset \"{}\" makes {} captures; `capture_screenshot` needs a screenshot preset",
            preset.name,
            preset.kind.as_str()
        )));
    }
    if preset.target == Some(PresetTarget::LongShot) && unset(params, "mode") {
        params.insert("mode".into(), json!("long"));
    }
    if let (true, Some(d)) = (unset(params, "delay_s"), preset.options.delay_s) {
        params.insert("delay_s".into(), json!(d));
    }
    if unset(params, "copy_to_clipboard") && preset.after.copy_to_clipboard {
        params.insert("copy_to_clipboard".into(), json!(true));
    }
    Ok(())
}

/// Fills the parameters of `start_recording` from `preset`; a key already in `params` wins.
/// An audio preset sets `mode: "audio"` and only the keys an audio recording accepts.
pub fn apply_to_recording(preset: &Preset, params: &mut Map<String, Value>) -> Result<(), CmError> {
    let audio = match preset.kind {
        PresetKind::Video => false,
        PresetKind::Audio => true,
        PresetKind::Screenshot => {
            return Err(CmError::invalid_input(format!(
            "the preset \"{}\" makes screenshots; `start_recording` needs a video or audio preset",
            preset.name
        )))
        }
    };
    if audio && unset(params, "mode") {
        params.insert("mode".into(), json!("audio"));
    }
    let o = &preset.options;
    let alias_free = unset(params, "countdown");
    let mut put = |key: &str, v: Option<Value>| {
        if let (true, Some(v)) = (unset(params, key), v) {
            params.insert(key.into(), v);
        }
    };
    put("mic", o.mic.map(Value::from));
    put("system_audio", o.system_audio.map(Value::from));
    if !audio {
        put("camera", o.camera.map(Value::from));
        put("cursor", o.cursor.map(Value::from));
        put("fps", o.fps.map(Value::from));
    }
    if alias_free {
        put("countdown_s", o.countdown_s.map(Value::from));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> time::Date {
        time::Date::from_calendar_date(2026, time::Month::November, 18).unwrap()
    }

    #[test]
    fn starters_are_valid_and_named() {
        let s = starter_presets();
        let names: Vec<_> = s.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Bug report", "Docs shot", "Demo video", "Meeting"]);
        for p in &s {
            p.validate().unwrap();
        }
        assert_eq!(s[0].after.tool.as_deref(), Some("step"));
        assert_eq!(s[1].options.scale, Some(2));
        assert_eq!(s[2].options.countdown_s, Some(3));
        assert_eq!(s[3].target, None);
    }

    /// QC-4.AC2: what is written reads back as the same setup.
    #[test]
    fn a_file_round_trips() {
        let f = PresetFile::with_starters();
        let (back, notice) = PresetFile::parse(&f.to_text()).unwrap();
        assert!(notice.is_none());
        assert_eq!(back, f);
    }

    /// QC-4.AC3: unknown fields survive a rewrite, at every level.
    #[test]
    fn unknown_fields_are_kept() {
        let text = r#"{
          "team": "docs", "version": 1, "starters_seeded": true,
          "presets": [{"id":"a","name":"A","kind":"screenshot","target":"region",
            "color":"red","options":{"glow":3,"scale":2},"after":{"sparkle":true}}]
        }"#;
        let (mut f, notice) = PresetFile::parse(text).unwrap();
        assert!(notice.is_none());
        let mut b = f.presets[0].clone();
        b.name = "A2".into();
        f.upsert(b).unwrap();
        let v: Value = serde_json::from_str(&f.to_text()).unwrap();
        assert_eq!(v["team"], "docs");
        assert_eq!(v["presets"][0]["name"], "A2");
        assert_eq!(v["presets"][0]["color"], "red");
        assert_eq!(v["presets"][0]["options"]["glow"], 3);
        assert_eq!(v["presets"][0]["options"]["scale"], 2);
        assert_eq!(v["presets"][0]["after"]["sparkle"], true);
    }

    /// QC-4.AC4: one invalid preset is skipped and one notice names it; the others load, and a
    /// save keeps the skipped entry in the file.
    #[test]
    fn an_invalid_preset_is_skipped_with_one_notice() {
        let text = r#"{"presets":[
          {"id":"ok","name":"Fine","kind":"screenshot","target":"window"},
          {"id":"bad","name":"Broken","kind":"video","target":"long_shot"},
          {"id":"worse","name":"Worse","kind":"hologram"}
        ]}"#;
        let (f, notice) = PresetFile::parse(text).unwrap();
        assert_eq!(f.presets.len(), 1);
        let n = notice.unwrap();
        assert!(n.contains("\"Broken\"") && n.contains("\"Worse\""), "{n}");
        assert!(!n.contains("Fine"));
        assert_eq!(f.skipped.len(), 2);
        let again = f.to_text();
        assert!(again.contains("Broken") && again.contains("hologram"));
        // A shared file without the flag does not get the starters.
        assert!(f.starters_seeded);
    }

    #[test]
    fn duplicate_names_and_ids_are_skipped() {
        let text = r#"{"presets":[
          {"id":"a","name":"Same","kind":"audio"},
          {"id":"b","name":"same","kind":"audio"},
          {"id":"a","name":"Other","kind":"audio"}
        ]}"#;
        let (f, notice) = PresetFile::parse(text).unwrap();
        assert_eq!(f.presets.len(), 1);
        assert!(notice.unwrap().contains("used twice"));
    }

    #[test]
    fn kind_rules() {
        let mut p = starter_presets().remove(3);
        p.options.fps = Some(60);
        assert!(p.validate().unwrap_err().contains("fps"));
        let mut p = starter_presets().remove(0);
        p.options.delay_s = Some(31.0);
        assert!(p.validate().is_err());
        let mut p = starter_presets().remove(0);
        p.after.open_in_editor = false;
        assert!(p.validate().unwrap_err().contains("open_in_editor"));
    }

    /// QC-4.AC7: `{date}` is the local date given.
    #[test]
    fn save_to_expands_the_date() {
        assert_eq!(expand_save_to("~/Bugs/{date}", date()), "~/Bugs/2026-11-18");
        let mut p = starter_presets().remove(0);
        assert_eq!(p.save_folder(date()), None);
        p.save_to = Some("/x/{date}/{date}".into());
        assert_eq!(p.save_folder(date()).unwrap(), "/x/2026-11-18/2026-11-18");
    }

    #[test]
    fn upsert_remove_reorder_and_conflicts() {
        let mut f = PresetFile::with_starters();
        let mut p = f.find("docs shot").unwrap().clone();
        p.shortcut = Some("Ctrl+Shift+D".into());
        f.upsert(p).unwrap();
        let mut q = f.find("bug-report").unwrap().clone();
        q.shortcut = Some("ctrl+shift+d".into());
        let e = f.upsert(q).unwrap_err();
        assert!(e.to_string().contains("Docs shot"), "{e}");
        let mut q = f.find("bug-report").unwrap().clone();
        q.name = "demo video".into();
        assert!(f.upsert(q).is_err());
        f.reorder(&["meeting".into(), "nope".into()]).unwrap_err();
        f.reorder(&["meeting".into(), "demo-video".into()]).unwrap();
        let ids: Vec<_> = f.presets.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["meeting", "demo-video", "bug-report", "docs-shot"]);
        assert_eq!(f.remove("Meeting").unwrap().id, "meeting");
        let e = f.remove("nope").unwrap_err();
        assert!(e.to_string().contains("\"nope\""));
    }

    #[test]
    fn new_ids_are_unique_slugs() {
        assert_eq!(new_id("My  Bug Shot!", &[]), "my-bug-shot");
        assert_eq!(new_id("Meeting", &["meeting".into()]), "meeting-2");
        assert_eq!(new_id("!!!", &[]), "preset");
    }

    /// QC-5.AC1 and AC3: the preset fills what the call leaves out; the call wins.
    #[test]
    fn screenshot_resolution() {
        let bug = starter_presets().remove(0);
        let mut p = Map::new();
        p.insert("target".into(), json!({"display_id": 1}));
        apply_to_screenshot(&bug, &mut p).unwrap();
        assert_eq!(p["copy_to_clipboard"], true);
        assert!(!p.contains_key("mode"));

        let mut timed = bug.clone();
        timed.options.delay_s = Some(5.0);
        let mut p = Map::new();
        p.insert("delay_s".into(), json!(3));
        apply_to_screenshot(&timed, &mut p).unwrap();
        assert_eq!(p["delay_s"], 3);
        let mut p = Map::new();
        p.insert("copy_to_clipboard".into(), json!(false));
        apply_to_screenshot(&timed, &mut p).unwrap();
        assert_eq!(p["copy_to_clipboard"], false);
        assert_eq!(p["delay_s"], 5.0);

        let mut long = bug;
        long.target = Some(PresetTarget::LongShot);
        let mut p = Map::new();
        apply_to_screenshot(&long, &mut p).unwrap();
        assert_eq!(p["mode"], "long");
    }

    #[test]
    fn recording_resolution() {
        let s = starter_presets();
        let mut p = Map::new();
        p.insert("mic".into(), json!(false));
        apply_to_recording(&s[2], &mut p).unwrap();
        assert_eq!(p["mic"], false);
        assert_eq!(p["camera"], true);
        assert_eq!(p["countdown_s"], 3);
        assert!(!p.contains_key("mode"));
        let mut p = Map::new();
        apply_to_recording(&s[3], &mut p).unwrap();
        assert_eq!(p["mode"], "audio");
        assert_eq!(p["system_audio"], true);
        assert!(!p.contains_key("camera"));
        assert!(apply_to_recording(&s[0], &mut Map::new()).is_err());
        assert!(apply_to_screenshot(&s[2], &mut Map::new()).is_err());
    }
}
