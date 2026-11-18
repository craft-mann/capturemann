//! What the Quick Capture panel holds (QC-1): the kind, the target and the option tiles of each
//! kind. Pure data and rules; the window in `ui::quick_window` only draws it.
//!
//! The tiles are remembered per kind in `quick_capture.json` in the settings folder and never
//! written to `settings.json` (QC-1: panel options are separate from Settings).

use std::path::{Path, PathBuf};

use cm_core::preset::{AfterCapture, Preset, PresetKind, PresetOptions, PresetTarget};
use cm_i18n::{tr, tr_ctx};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Seconds the Delay tile offers (screenshots).
pub const DELAY_CHOICES: [u32; 4] = [0, 3, 5, 10];
/// Seconds the Countdown tile offers (video).
pub const COUNTDOWN_CHOICES: [u32; 4] = [0, 3, 5, 10];

/// The file the panel's choices are remembered in, inside the settings folder.
pub const STATE_FILE: &str = "quick_capture.json";

/// One option tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tile {
    Editor,
    Copy,
    Redact,
    Delay,
    Mic,
    SystemAudio,
    Webcam,
    Countdown,
    Transcribe,
    Markers,
}

impl Tile {
    /// The four tiles of `kind`, in grid order (row by row).
    pub fn of(kind: PresetKind) -> [Tile; 4] {
        match kind {
            PresetKind::Screenshot => [Tile::Editor, Tile::Copy, Tile::Redact, Tile::Delay],
            PresetKind::Video => [Tile::Mic, Tile::SystemAudio, Tile::Webcam, Tile::Countdown],
            PresetKind::Audio => [
                Tile::Mic,
                Tile::SystemAudio,
                Tile::Transcribe,
                Tile::Markers,
            ],
        }
    }

    /// A tile that opens a small menu of values instead of toggling.
    pub fn has_values(self) -> bool {
        matches!(self, Tile::Delay | Tile::Countdown)
    }

    pub fn label(self) -> String {
        match self {
            Tile::Editor => tr_ctx!("quick capture tile", "Editor"),
            Tile::Copy => tr_ctx!("quick capture tile", "Copy"),
            Tile::Redact => tr_ctx!("quick capture tile", "Auto-redact"),
            Tile::Delay => tr_ctx!("quick capture tile", "Delay"),
            Tile::Mic => tr_ctx!("quick capture tile", "Mic"),
            Tile::SystemAudio => tr_ctx!("quick capture tile", "System audio"),
            Tile::Webcam => tr_ctx!("quick capture tile", "Webcam"),
            Tile::Countdown => tr_ctx!("quick capture tile", "Countdown"),
            Tile::Transcribe => tr_ctx!("quick capture tile", "Transcribe"),
            Tile::Markers => tr_ctx!("quick capture tile", "Markers"),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tile::Editor => "pencil",
            Tile::Copy => "copy",
            Tile::Redact => "shield-check",
            Tile::Delay | Tile::Countdown => "timer",
            Tile::Mic => "mic",
            Tile::SystemAudio => "volume-2",
            Tile::Webcam => "camera",
            Tile::Transcribe => "file-text",
            Tile::Markers => "bookmark",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Tile::Editor => "qc-tile-editor",
            Tile::Copy => "qc-tile-copy",
            Tile::Redact => "qc-tile-redact",
            Tile::Delay => "qc-tile-delay",
            Tile::Mic => "qc-tile-mic",
            Tile::SystemAudio => "qc-tile-system",
            Tile::Webcam => "qc-tile-webcam",
            Tile::Countdown => "qc-tile-countdown",
            Tile::Transcribe => "qc-tile-transcribe",
            Tile::Markers => "qc-tile-markers",
        }
    }
}

/// The text of a seconds value on a tile: "Off" for none, else "3 s".
pub fn seconds_text(secs: u32) -> String {
    if secs == 0 {
        tr_ctx!("quick capture value", "Off")
    } else {
        tr!("{secs} s", secs = secs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShotTiles {
    pub editor: bool,
    pub copy: bool,
    pub redact: bool,
    pub delay_s: u32,
}

impl Default for ShotTiles {
    fn default() -> Self {
        // Copy on is what the dock tiles do out of the box (CAP-4 default).
        ShotTiles {
            editor: false,
            copy: true,
            redact: false,
            delay_s: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoTiles {
    pub mic: bool,
    pub system_audio: bool,
    pub webcam: bool,
    pub countdown_s: u32,
}

impl Default for VideoTiles {
    fn default() -> Self {
        VideoTiles {
            mic: true,
            system_audio: false,
            webcam: false,
            countdown_s: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioTiles {
    pub mic: bool,
    pub system_audio: bool,
    pub transcribe: bool,
    pub markers: bool,
}

impl Default for AudioTiles {
    fn default() -> Self {
        AudioTiles {
            mic: true,
            system_audio: false,
            transcribe: false,
            markers: false,
        }
    }
}

/// The remembered part of the panel (what `quick_capture.json` holds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Remembered {
    pub kind: PresetKind,
    pub shot_target: PresetTarget,
    pub video_target: PresetTarget,
    pub shot: ShotTiles,
    pub video: VideoTiles,
    pub audio: AudioTiles,
}

impl Default for Remembered {
    fn default() -> Self {
        Remembered {
            kind: PresetKind::Screenshot,
            shot_target: PresetTarget::Region,
            video_target: PresetTarget::Display,
            shot: ShotTiles::default(),
            video: VideoTiles::default(),
            audio: AudioTiles::default(),
        }
    }
}

/// The panel: the remembered choices plus the preset that is picked (QC-4).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelState {
    pub mem: Remembered,
    /// The picked preset with the panel's own tile changes applied on top; its hidden fields
    /// (tool, folder, format, scale, cursor, frame rate) travel with the run. `None`: no preset.
    pub preset: Option<Preset>,
}

/// The screenshot targets, in tab order.
pub const SHOT_TARGETS: [PresetTarget; 4] = [
    PresetTarget::Region,
    PresetTarget::Window,
    PresetTarget::Display,
    PresetTarget::LongShot,
];
/// The video targets.
pub const VIDEO_TARGETS: [PresetTarget; 3] = [
    PresetTarget::Region,
    PresetTarget::Window,
    PresetTarget::Display,
];

pub fn target_label(t: PresetTarget) -> String {
    match t {
        PresetTarget::Region => tr_ctx!("quick capture target", "Region"),
        PresetTarget::Window => tr_ctx!("quick capture target", "Window"),
        PresetTarget::Display => tr_ctx!("quick capture target", "Display"),
        PresetTarget::LongShot => tr_ctx!("quick capture target", "Long Shot"),
    }
}

pub fn kind_label(k: PresetKind) -> String {
    match k {
        PresetKind::Screenshot => tr_ctx!("quick capture kind", "Screenshot"),
        PresetKind::Video => tr_ctx!("quick capture kind", "Video"),
        PresetKind::Audio => tr_ctx!("quick capture kind", "Audio"),
    }
}

pub fn kind_icon(k: PresetKind) -> &'static str {
    match k {
        PresetKind::Screenshot => "cm-capture-region",
        PresetKind::Video => "cm-record",
        PresetKind::Audio => "mic",
    }
}

/// The kind a number key picks (1, 2, 3).
pub fn kind_of_key(key: &str) -> Option<PresetKind> {
    match key {
        "1" => Some(PresetKind::Screenshot),
        "2" => Some(PresetKind::Video),
        "3" => Some(PresetKind::Audio),
        _ => None,
    }
}

fn nearest(choices: &[u32], secs: f64) -> u32 {
    let secs = secs.max(0.0);
    choices
        .iter()
        .copied()
        .min_by(|a, b| {
            let (da, db) = ((f64::from(*a) - secs).abs(), (f64::from(*b) - secs).abs());
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0)
}

impl PanelState {
    pub fn kind(&self) -> PresetKind {
        self.mem.kind
    }

    pub fn set_kind(&mut self, k: PresetKind) {
        if self.mem.kind != k {
            self.mem.kind = k;
            // A preset belongs to one kind; switching the tab leaves it.
            if self.preset.as_ref().is_some_and(|p| p.kind != k) {
                self.preset = None;
            }
        }
    }

    /// The targets of the current kind (audio has none).
    pub fn targets(&self) -> &'static [PresetTarget] {
        match self.mem.kind {
            PresetKind::Screenshot => &SHOT_TARGETS,
            PresetKind::Video => &VIDEO_TARGETS,
            PresetKind::Audio => &[],
        }
    }

    /// The target of the current kind, `None` for audio.
    pub fn target(&self) -> Option<PresetTarget> {
        match self.mem.kind {
            PresetKind::Screenshot => Some(self.mem.shot_target),
            PresetKind::Video => Some(self.mem.video_target),
            PresetKind::Audio => None,
        }
    }

    pub fn set_target(&mut self, t: PresetTarget) {
        match self.mem.kind {
            PresetKind::Screenshot => self.mem.shot_target = t,
            PresetKind::Video if t != PresetTarget::LongShot => self.mem.video_target = t,
            _ => {}
        }
    }

    /// Whether a tile is on (value tiles: a non-zero value).
    pub fn is_on(&self, t: Tile) -> bool {
        let (s, v, a) = (&self.mem.shot, &self.mem.video, &self.mem.audio);
        match (self.mem.kind, t) {
            (PresetKind::Screenshot, Tile::Editor) => s.editor,
            (PresetKind::Screenshot, Tile::Copy) => s.copy,
            (PresetKind::Screenshot, Tile::Redact) => s.redact,
            (PresetKind::Screenshot, Tile::Delay) => s.delay_s > 0,
            (PresetKind::Video, Tile::Mic) => v.mic,
            (PresetKind::Video, Tile::SystemAudio) => v.system_audio,
            (PresetKind::Video, Tile::Webcam) => v.webcam,
            (PresetKind::Video, Tile::Countdown) => v.countdown_s > 0,
            (PresetKind::Audio, Tile::Mic) => a.mic,
            (PresetKind::Audio, Tile::SystemAudio) => a.system_audio,
            (PresetKind::Audio, Tile::Transcribe) => a.transcribe,
            (PresetKind::Audio, Tile::Markers) => a.markers,
            _ => false,
        }
    }

    /// The value a value tile shows ("5 s"), `None` for a toggle tile.
    pub fn value_text(&self, t: Tile) -> Option<String> {
        match (self.mem.kind, t) {
            (PresetKind::Screenshot, Tile::Delay) => Some(seconds_text(self.mem.shot.delay_s)),
            (PresetKind::Video, Tile::Countdown) => Some(seconds_text(self.mem.video.countdown_s)),
            _ => None,
        }
    }

    /// The choices of a value tile.
    pub fn value_choices(&self, t: Tile) -> &'static [u32] {
        match t {
            Tile::Delay => &DELAY_CHOICES,
            Tile::Countdown => &COUNTDOWN_CHOICES,
            _ => &[],
        }
    }

    /// The current value of a value tile, in seconds.
    pub fn value_of(&self, t: Tile) -> u32 {
        match t {
            Tile::Delay => self.mem.shot.delay_s,
            Tile::Countdown => self.mem.video.countdown_s,
            _ => 0,
        }
    }

    /// Clicks a toggle tile.
    pub fn toggle(&mut self, t: Tile) {
        let on = !self.is_on(t);
        let (s, v, a) = (&mut self.mem.shot, &mut self.mem.video, &mut self.mem.audio);
        match (self.mem.kind, t) {
            (PresetKind::Screenshot, Tile::Editor) => s.editor = on,
            (PresetKind::Screenshot, Tile::Copy) => s.copy = on,
            (PresetKind::Screenshot, Tile::Redact) => s.redact = on,
            (PresetKind::Video, Tile::Mic) => v.mic = on,
            (PresetKind::Video, Tile::SystemAudio) => v.system_audio = on,
            (PresetKind::Video, Tile::Webcam) => v.webcam = on,
            (PresetKind::Audio, Tile::Mic) => a.mic = on,
            (PresetKind::Audio, Tile::SystemAudio) => a.system_audio = on,
            (PresetKind::Audio, Tile::Transcribe) => a.transcribe = on,
            (PresetKind::Audio, Tile::Markers) => a.markers = on,
            _ => {}
        }
        self.sync_preset_after();
    }

    /// Picks a value of a value tile.
    pub fn set_value(&mut self, t: Tile, secs: u32) {
        match (self.mem.kind, t) {
            (PresetKind::Screenshot, Tile::Delay) if DELAY_CHOICES.contains(&secs) => {
                self.mem.shot.delay_s = secs
            }
            (PresetKind::Video, Tile::Countdown) if COUNTDOWN_CHOICES.contains(&secs) => {
                self.mem.video.countdown_s = secs
            }
            _ => {}
        }
        self.sync_preset_after();
    }

    /// The tiles stay the truth for the run; the carried preset only keeps its hidden fields.
    fn sync_preset_after(&mut self) {}

    /// What the action bar says: "Capture" or "Record", with the preset's name when one is
    /// picked. The second value is true for the red Record bar.
    pub fn action_text(&self) -> (String, bool) {
        let record = self.mem.kind != PresetKind::Screenshot;
        let name = self.preset.as_ref().map(|p| p.name.clone());
        let text = match (record, name) {
            (false, None) => tr_ctx!("quick capture action", "Capture"),
            (true, None) => tr_ctx!("quick capture action", "Record"),
            (false, Some(n)) => {
                tr_ctx!("quick capture action", "Capture \u{00B7} {name}", name = n)
            }
            (true, Some(n)) => tr_ctx!("quick capture action", "Record \u{00B7} {name}", name = n),
        };
        (text, record)
    }

    /// QC-4.AC1: picks `p`: kind, target and the tiles are filled from it.
    pub fn apply_preset(&mut self, p: &Preset) {
        self.mem.kind = p.kind;
        let (o, a) = (&p.options, &p.after);
        match p.kind {
            PresetKind::Screenshot => {
                if let Some(t) = p.target {
                    self.mem.shot_target = t;
                }
                self.mem.shot = ShotTiles {
                    editor: a.open_in_editor,
                    copy: a.copy_to_clipboard,
                    redact: a.auto_redact,
                    delay_s: o.delay_s.map_or(0, |d| nearest(&DELAY_CHOICES, d)),
                };
            }
            PresetKind::Video => {
                if let Some(t) = p.target.filter(|t| *t != PresetTarget::LongShot) {
                    self.mem.video_target = t;
                }
                let v = self.mem.video;
                self.mem.video = VideoTiles {
                    mic: o.mic.unwrap_or(v.mic),
                    system_audio: o.system_audio.unwrap_or(false),
                    webcam: o.camera.unwrap_or(false),
                    countdown_s: o
                        .countdown_s
                        .map_or(v.countdown_s, |c| nearest(&COUNTDOWN_CHOICES, f64::from(c))),
                };
            }
            PresetKind::Audio => {
                let au = self.mem.audio;
                self.mem.audio = AudioTiles {
                    mic: o.mic.unwrap_or(au.mic),
                    system_audio: o.system_audio.unwrap_or(false),
                    transcribe: a.transcribe,
                    markers: o
                        .extra
                        .get("markers")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                };
            }
        }
        self.preset = Some(p.clone());
    }

    /// Leaves the preset ("No preset"); the tiles stay as they are.
    pub fn clear_preset(&mut self) {
        self.preset = None;
    }

    /// QC-4.AC2: the panel's current setup as a new preset (JSON without an `id`, as
    /// `save_preset` takes it). The hidden fields of a picked preset are kept; its shortcut is
    /// not (a shortcut belongs to one preset).
    pub fn to_preset_json(&self, name: &str) -> Value {
        let carried = self.preset.as_ref().filter(|p| p.kind == self.mem.kind);
        let mut options = PresetOptions::default();
        let mut after = AfterCapture::default();
        let mut extra = Map::new();
        let (s, v, a) = (&self.mem.shot, &self.mem.video, &self.mem.audio);
        match self.mem.kind {
            PresetKind::Screenshot => {
                options.delay_s = (s.delay_s > 0).then(|| f64::from(s.delay_s));
                if let Some(c) = carried {
                    options.cursor = c.options.cursor;
                    options.format = c.options.format;
                    options.scale = c.options.scale;
                }
                after.copy_to_clipboard = s.copy;
                after.auto_redact = s.redact;
                after.open_in_editor = s.editor;
                if s.editor {
                    after.tool = carried.and_then(|c| c.after.tool.clone());
                }
            }
            PresetKind::Video => {
                options.mic = Some(v.mic);
                options.system_audio = Some(v.system_audio);
                options.camera = Some(v.webcam);
                options.countdown_s = Some(v.countdown_s);
                if let Some(c) = carried {
                    options.cursor = c.options.cursor;
                    options.fps = c.options.fps;
                }
            }
            PresetKind::Audio => {
                options.mic = Some(a.mic);
                options.system_audio = Some(a.system_audio);
                options.extra.insert("markers".into(), json!(a.markers));
                after.transcribe = a.transcribe;
            }
        }
        if let Some(c) = carried {
            extra.extend(c.extra.clone());
        }
        let p = Preset {
            id: String::new(),
            name: name.trim().to_owned(),
            kind: self.mem.kind,
            target: self.target(),
            options,
            save_to: carried.and_then(|c| c.save_to.clone()),
            after,
            shortcut: None,
            extra,
        };
        let mut v = serde_json::to_value(&p).unwrap_or(Value::Null);
        if let Some(o) = v.as_object_mut() {
            o.remove("id");
        }
        v
    }

    /// The accessible name of a tile: the label and its state ("Copy, on", "Delay, 5 s").
    pub fn tile_aria(&self, t: Tile) -> String {
        match self.value_text(t) {
            Some(v) => tr_ctx!(
                "quick capture tile",
                "{tile}, {value}",
                tile = t.label(),
                value = v
            ),
            None if self.is_on(t) => {
                tr_ctx!("quick capture tile", "{tile}, on", tile = t.label())
            }
            None => tr_ctx!("quick capture tile", "{tile}, off", tile = t.label()),
        }
    }
}

/// Where the choices are remembered.
pub fn state_path(config_dir: &Path) -> PathBuf {
    config_dir.join(STATE_FILE)
}

/// Reads the remembered choices; a missing or damaged file gives the defaults.
pub fn load(config_dir: &Path) -> Remembered {
    std::fs::read_to_string(state_path(config_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Writes the remembered choices. A failure is only logged: the panel works without it.
pub fn save(config_dir: &Path, mem: &Remembered) {
    let path = state_path(config_dir);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(mem).unwrap_or_default())?;
        std::fs::rename(&tmp, &path)
    };
    if let Err(e) = write() {
        log::warn!("quick capture: cannot remember the panel choices: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::preset::starter_presets;

    fn starter(name: &str) -> Preset {
        starter_presets()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
    }

    /// QC-1.AC1: four tiles per kind, in the order the spec names.
    #[test]
    fn qc1_ac1_each_kind_has_four_tiles_in_order() {
        use Tile::*;
        assert_eq!(
            Tile::of(PresetKind::Screenshot),
            [Editor, Copy, Redact, Delay]
        );
        assert_eq!(
            Tile::of(PresetKind::Video),
            [Mic, SystemAudio, Webcam, Countdown]
        );
        assert_eq!(
            Tile::of(PresetKind::Audio),
            [Mic, SystemAudio, Transcribe, Markers]
        );
    }

    /// QC-1.AC3: the bar says Capture (neutral) for a screenshot, Record (red) for video and audio.
    #[test]
    fn qc1_ac3_action_bar_text_and_colour() {
        let mut s = PanelState::default();
        assert_eq!(s.action_text(), ("Capture".to_owned(), false));
        s.set_kind(PresetKind::Video);
        assert_eq!(s.action_text(), ("Record".to_owned(), true));
        s.set_kind(PresetKind::Audio);
        assert_eq!(s.action_text(), ("Record".to_owned(), true));
        assert!(s.targets().is_empty() && s.target().is_none());
    }

    /// QC-4.AC1: picking "Bug report" fills kind, target and tiles, and the bar names it.
    #[test]
    fn qc4_ac1_a_picked_preset_fills_the_panel() {
        let mut s = PanelState::default();
        s.set_kind(PresetKind::Audio);
        s.apply_preset(&starter("Bug report"));
        assert_eq!(s.kind(), PresetKind::Screenshot);
        assert_eq!(s.target(), Some(PresetTarget::Region));
        assert!(s.is_on(Tile::Copy) && s.is_on(Tile::Redact) && s.is_on(Tile::Editor));
        assert_eq!(s.action_text().0, "Capture \u{00B7} Bug report");
        s.apply_preset(&starter("Demo video"));
        assert_eq!(s.kind(), PresetKind::Video);
        assert_eq!(s.target(), Some(PresetTarget::Display));
        assert!(s.is_on(Tile::Mic) && s.is_on(Tile::Webcam) && !s.is_on(Tile::SystemAudio));
        assert_eq!(s.value_of(Tile::Countdown), 3);
        assert_eq!(
            s.action_text(),
            ("Record \u{00B7} Demo video".to_owned(), true)
        );
        s.apply_preset(&starter("Meeting"));
        assert_eq!(s.kind(), PresetKind::Audio);
        assert!(s.is_on(Tile::Mic) && s.is_on(Tile::SystemAudio) && s.is_on(Tile::Transcribe));
    }

    /// QC-1: tiles are remembered per kind and a toggle in one kind does not touch another.
    #[test]
    fn tiles_are_per_kind() {
        let mut s = PanelState::default();
        s.toggle(Tile::Copy);
        assert!(!s.is_on(Tile::Copy));
        s.set_kind(PresetKind::Video);
        s.toggle(Tile::Mic);
        assert!(!s.is_on(Tile::Mic));
        s.set_kind(PresetKind::Screenshot);
        assert!(!s.is_on(Tile::Copy));
        s.set_kind(PresetKind::Audio);
        assert!(s.is_on(Tile::Mic), "the audio mic is its own tile");
        s.set_kind(PresetKind::Screenshot);
        s.set_value(Tile::Delay, 5);
        assert_eq!(s.value_text(Tile::Delay).as_deref(), Some("5 s"));
        s.set_value(Tile::Delay, 7); // not a choice
        assert_eq!(s.value_of(Tile::Delay), 5);
    }

    /// QC-1.AC6: the panel's choices live in their own file; reading it back gives the same.
    #[test]
    fn qc1_ac6_choices_are_remembered_in_their_own_file() {
        let dir = std::env::temp_dir().join(format!("cm-qc-state-{}", std::process::id()));
        let mut s = PanelState::default();
        s.set_kind(PresetKind::Video);
        s.toggle(Tile::Webcam);
        s.set_value(Tile::Countdown, 10);
        save(&dir, &s.mem);
        assert!(!dir.join("settings.json").exists());
        let back = load(&dir);
        assert_eq!(back, s.mem);
        assert_eq!(back.kind, PresetKind::Video, "opens on the last-used kind");
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            load(&dir),
            Remembered::default(),
            "a missing file is the defaults"
        );
    }

    /// QC-4.AC2: save current as preset gives a valid preset that reads back as the same setup.
    #[test]
    fn qc4_ac2_current_setup_becomes_a_valid_preset() {
        let mut s = PanelState::default();
        s.apply_preset(&starter("Docs shot"));
        s.toggle(Tile::Redact);
        let v = s.to_preset_json("My docs");
        assert!(v.get("id").is_none());
        let mut raw = v.clone();
        raw["id"] = json!("my-docs");
        let p: Preset = serde_json::from_value(raw).unwrap();
        p.validate().unwrap();
        assert_eq!(p.name, "My docs");
        assert!(p.after.auto_redact && p.after.open_in_editor);
        assert_eq!(p.options.scale, Some(2), "hidden fields travel");
        assert!(p.shortcut.is_none());
        let mut t = PanelState::default();
        t.apply_preset(&p);
        assert_eq!(t.mem.shot, s.mem.shot);
        assert_eq!(t.target(), s.target());

        let mut a = PanelState::default();
        a.set_kind(PresetKind::Audio);
        a.toggle(Tile::Markers);
        let mut raw = a.to_preset_json("Calls");
        raw["id"] = json!("calls");
        let p: Preset = serde_json::from_value(raw).unwrap();
        p.validate().unwrap();
        let mut b = PanelState::default();
        b.apply_preset(&p);
        assert_eq!(b.mem.audio, a.mem.audio);
    }

    #[test]
    fn number_keys_pick_the_kind() {
        assert_eq!(kind_of_key("1"), Some(PresetKind::Screenshot));
        assert_eq!(kind_of_key("2"), Some(PresetKind::Video));
        assert_eq!(kind_of_key("3"), Some(PresetKind::Audio));
        assert_eq!(kind_of_key("4"), None);
    }
}
