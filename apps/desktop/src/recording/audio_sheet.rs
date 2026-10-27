//! The start sheet's Audio mode (VOC-1, board V1 `VoiceRecordSheet`): the five use-case presets,
//! the microphone and system audio rows, "How many people?", the "After you stop" chips, the
//! consent line and the disk, lid and duration footer line. Display-free; `ui::recording::sheet`
//! draws it and starts the recording through `start_recording { mode: "audio", ... }`.
//!
//! Presets only set defaults (`mic`, `system_audio`, the people guess and the report the AI-9
//! step suggests); every source can be changed after choosing one. The last preset and sources
//! are remembered (`recording.audio_*`, writable by agents like the rest of `recording.*`), so
//! the sheet, the Option+Shift+V shortcut and a launcher entry all start the same way.

use cm_i18n::{tr, tr_ctx};
use serde_json::{json, Map, Value};

use crate::recording::sheet::{Devices, Note, SheetHeights};
use crate::shortcuts::Platform;

/// The audio sheet is wider than the screen sheet (board V1: 760).
pub const AUDIO_SHEET_WIDTH: f64 = 760.0;
/// Segments of the live mic meter (board V1).
pub const METER_SEGMENTS: usize = 40;

/// The last preset and sources used (app settings, shared with the shortcut and the launcher).
pub const KEY_PRESET: &str = "recording.audio_preset";
pub const KEY_MIC: &str = "recording.audio_mic";
pub const KEY_SYSTEM: &str = "recording.audio_system_audio";
/// The first audio recording was started once: from then on only presets with other people show
/// the consent line again (the setting `recording.consent_notice` still switches it all off).
pub const KEY_SEEN: &str = "recording.audio_seen";
/// VOC-1: the consent line's switch (default on, app-only).
pub const KEY_CONSENT: &str = "recording.consent_notice";
/// VOC-1: how long an audio recording may run, in seconds (default 4 h, at most 12 h).
pub const KEY_MAX_DURATION: &str = "recording.audio_max_duration_s";
pub const DEFAULT_MAX_DURATION_S: f64 = 4.0 * 3600.0;
const KEY_NOISE: &str = "audio.noise_suppression";
const KEY_MIC_DEVICE: &str = "audio.mic_device";
const KEY_AUTO_SPEAKERS: &str = "ai.auto_speakers";

/// Disk use per hour (VOC-1): a 48 kHz 32-bit float mono master is about 0.7 GB, the denoised
/// copy (AUD-3) doubles it, and system audio (stereo) is about 1.4 GB.
pub const MIC_GB_PER_HOUR: f64 = 0.7;
pub const SYSTEM_GB_PER_HOUR: f64 = 1.4;

/// The use-case presets of VOC-1 (`start_recording.preset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    /// Interview, 1:1, a meeting around a laptop.
    #[default]
    Room,
    Online,
    Lecture,
    Product,
    Note,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Room,
        Preset::Online,
        Preset::Lecture,
        Preset::Product,
        Preset::Note,
    ];

    /// The `preset` value of `start_recording`.
    pub fn key(self) -> &'static str {
        match self {
            Preset::Room => "room",
            Preset::Online => "online",
            Preset::Lecture => "lecture",
            Preset::Product => "product",
            Preset::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.key() == s)
    }

    pub fn title(self) -> String {
        match self {
            Preset::Room => tr!("In the room"),
            Preset::Online => tr!("Online meeting"),
            Preset::Lecture => tr!("Lecture or talk"),
            Preset::Product => tr!("Product demo"),
            Preset::Note => tr!("Voice note"),
        }
    }

    pub fn blurb(self) -> String {
        match self {
            Preset::Room => tr!("Interview, 1:1, meeting around a laptop"),
            Preset::Online => tr!("Zoom, Meet, Teams, a browser call"),
            Preset::Lecture => tr!("One speaker, a room"),
            Preset::Product => tr!("Introduction or demo call"),
            Preset::Note => tr!("Just you"),
        }
    }

    /// The icon on the card (`design/icons/icon-map.md`).
    pub fn icon(self) -> &'static str {
        match self {
            Preset::Room => "users",
            Preset::Online => "laptop",
            Preset::Lecture => "graduation-cap",
            Preset::Product => "presentation",
            Preset::Note => "file-text",
        }
    }

    /// Microphone and system audio this preset starts with (the table of VOC-1).
    pub fn sources(self) -> (bool, bool) {
        match self {
            Preset::Room | Preset::Lecture | Preset::Note => (true, false),
            Preset::Online | Preset::Product => (true, true),
        }
    }

    /// The report the AI-9 step suggests after stop.
    pub fn report(self) -> String {
        match self {
            Preset::Room | Preset::Online => tr!("Meeting minutes"),
            Preset::Lecture => tr!("Lecture notes"),
            Preset::Product => tr!("Product presentation recap"),
            Preset::Note => tr!("Summary"),
        }
    }

    /// Other people are being recorded (the consent line shows every time).
    pub fn involves_others(self) -> bool {
        self != Preset::Note
    }

    /// "How many people?" is asked (a voice note is just the user).
    pub fn asks_people(self) -> bool {
        self != Preset::Note
    }

    fn default_people(self) -> People {
        match self {
            Preset::Room => People::Two,
            _ => People::NotSure,
        }
    }

    /// The preset shows "Keep the lid open" (a laptop on the table).
    pub fn laptop_on_table(self) -> bool {
        self == Preset::Room
    }
}

/// "How many people?" (optional): feeds the speaker step (AI-8 `num_speakers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum People {
    #[default]
    NotSure,
    Two,
    Three,
    Four,
    FivePlus,
}

impl People {
    pub const ALL: [People; 5] = [
        People::NotSure,
        People::Two,
        People::Three,
        People::Four,
        People::FivePlus,
    ];

    pub fn label(self) -> String {
        match self {
            People::NotSure => tr!("Not sure"),
            People::Two => "2".to_owned(),
            People::Three => "3".to_owned(),
            People::Four => "4".to_owned(),
            People::FivePlus => "5+".to_owned(),
        }
    }

    /// Exactly this many speakers (`num_speakers`); `5+` is a lower bound, not a count.
    pub fn count(self) -> Option<u32> {
        match self {
            People::Two => Some(2),
            People::Three => Some(3),
            People::Four => Some(4),
            People::NotSure | People::FivePlus => None,
        }
    }

    /// At least this many speakers (`min_speakers`).
    pub fn at_least(self) -> Option<u32> {
        (self == People::FivePlus).then_some(5)
    }

    /// The `people` the recording stores (`start_recording.people`, 1 to 8): the count, or 5 for
    /// "5+". "Not sure" stores nothing (unknown).
    pub fn stored(self) -> Option<u32> {
        self.count().or(self.at_least())
    }
}

/// What the sheet knows when it opens (settings and the AI page's state).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioDefaults {
    pub preset: Preset,
    pub mic: bool,
    pub system_audio: bool,
    pub mic_device: Option<String>,
    pub noise_suppression: bool,
    pub max_duration_s: f64,
    pub consent_notice: bool,
    /// An audio recording was started before (`recording.audio_seen`).
    pub seen: bool,
    pub auto_speakers: bool,
}

fn setting<T: serde::de::DeserializeOwned>(s: &Map<String, Value>, key: &str) -> Option<T> {
    s.get(key)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

impl AudioDefaults {
    pub fn from_settings(s: &Map<String, Value>) -> AudioDefaults {
        let preset = setting::<String>(s, KEY_PRESET)
            .and_then(|p| Preset::parse(&p))
            .unwrap_or_default();
        let (mic, system) = preset.sources();
        AudioDefaults {
            preset,
            mic: setting(s, KEY_MIC).unwrap_or(mic),
            system_audio: setting(s, KEY_SYSTEM).unwrap_or(system),
            mic_device: setting::<String>(s, KEY_MIC_DEVICE).filter(|d| !d.is_empty()),
            noise_suppression: setting(s, KEY_NOISE).unwrap_or(true),
            max_duration_s: setting::<f64>(s, KEY_MAX_DURATION)
                .filter(|n| *n > 0.0 && *n <= 12.0 * 3600.0)
                .unwrap_or(DEFAULT_MAX_DURATION_S),
            consent_notice: setting(s, KEY_CONSENT).unwrap_or(true),
            seen: setting(s, KEY_SEEN).unwrap_or(false),
            auto_speakers: setting(s, KEY_AUTO_SPEAKERS).unwrap_or(true),
        }
    }

    /// The `start_recording` parameters of a start with the remembered choices (the shortcut and
    /// the launcher: no sheet to ask). The daemon refuses a microphone that is not allowed
    /// (`permission_missing`) before anything starts, and the caller shows that error.
    pub fn quick_params(&self) -> Value {
        let mut p = start_params(
            self.preset,
            self.mic,
            self.mic_device.as_deref(),
            self.system_audio,
            None,
        );
        if !self.mic && !self.system_audio {
            // Nothing was left on: record the microphone, the one source every preset has.
            p["mic"] = json!(true);
        }
        p
    }
}

/// `start_recording { mode: "audio", ... }`: sources, preset and the "How many people?" answer
/// (`people`, left out when unknown). Title, countdown and the length limit come from the
/// daemon's settings, the same for the sheet, the shortcut and agents.
pub fn start_params(
    preset: Preset,
    mic: bool,
    mic_device: Option<&str>,
    system_audio: bool,
    people: Option<u32>,
) -> Value {
    let mic = match (mic, mic_device) {
        (false, _) => json!(false),
        (true, Some(d)) => json!({ "device_id": d }),
        (true, None) => json!(true),
    };
    let mut p = json!({
        "mode": "audio",
        "preset": preset.key(),
        "mic": mic,
        "system_audio": system_audio,
    });
    if let Some(n) = people {
        p["people"] = json!(n);
    }
    p
}

/// What the Audio mode of the sheet holds.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSheetModel {
    pub preset: Preset,
    pub mic: bool,
    pub mic_device: Option<String>,
    pub system_audio: bool,
    pub people: People,
    /// The microphone list is open under its row.
    pub picker_open: bool,
    pub devices: Devices,
    pub noise_suppression: bool,
    pub max_duration_s: f64,
    /// `recording.consent_notice`.
    pub consent_notice: bool,
    /// The user pressed "Don't show again": gone for this sheet at once, and saved.
    pub consent_dismissed: bool,
    pub seen: bool,
    /// `ai.auto_transcribe` is on and a model is installed: its label (for "Transcribe · Medium").
    pub transcribe_model: Option<String>,
    /// `ai.auto_speakers` (default on).
    pub auto_speakers: bool,
    /// Both speaker model files are usable (they ship inside the app); read from the AI page's
    /// state as the sheet draws.
    pub speakers_ready: bool,
    /// Free bytes on the library's volume, when known.
    pub free_bytes: Option<u64>,
}

impl AudioSheetModel {
    pub fn new(d: &AudioDefaults) -> AudioSheetModel {
        AudioSheetModel {
            preset: d.preset,
            mic: d.mic,
            mic_device: d.mic_device.clone(),
            system_audio: d.system_audio,
            people: d.preset.default_people(),
            picker_open: false,
            devices: Devices::default(),
            noise_suppression: d.noise_suppression,
            max_duration_s: d.max_duration_s,
            consent_notice: d.consent_notice,
            consent_dismissed: false,
            seen: d.seen,
            transcribe_model: None,
            auto_speakers: d.auto_speakers,
            speakers_ready: true,
            free_bytes: None,
        }
    }

    /// A card was chosen: its sources and people guess become the defaults. The user can still
    /// change every source after.
    pub fn choose(&mut self, preset: Preset) {
        self.preset = preset;
        (self.mic, self.system_audio) = preset.sources();
        self.people = preset.default_people();
    }

    /// The devices arrived: a remembered microphone that is gone is dropped (the default is used).
    pub fn set_devices(&mut self, devices: Devices) {
        if let Some(m) = &self.mic_device {
            if devices.loaded && !devices.mics.iter().any(|d| &d.id == m) {
                self.mic_device = None;
            }
        }
        self.devices = devices;
    }

    // ---- sources -------------------------------------------------------------------------

    pub fn mic_blocked(&self) -> bool {
        self.devices.mic_permission.blocked()
    }

    /// System audio comes from ScreenCaptureKit on macOS: it needs the capability and, on a Mac,
    /// Screen Recording. Microphone-only recordings never need either.
    pub fn system_supported(&self) -> bool {
        !self.devices.loaded || self.devices.system_audio_supported
    }

    pub fn screen_blocked(&self) -> bool {
        self.devices.screen_permission.blocked()
    }

    /// The name on the device picker: the chosen microphone, else the system default.
    pub fn mic_name(&self) -> String {
        let chosen = self
            .mic_device
            .as_ref()
            .and_then(|id| self.devices.mics.iter().find(|d| &d.id == id));
        chosen
            .or_else(|| self.devices.mics.iter().find(|d| d.is_default))
            .or_else(|| self.devices.mics.first())
            .map(|d| d.name.clone())
            .unwrap_or_else(|| tr!("Default microphone"))
    }

    /// The device id the meter and the recording use (`None`: the system default).
    pub fn effective_mic_device(&self) -> Option<String> {
        self.mic_device.clone()
    }

    pub fn mic_note(&self) -> Option<Note> {
        if self.mic && self.mic_blocked() {
            return Some(Note::Problem {
                text: tr!(
                    "Microphone access is off. Turn it on in System Settings to record your voice."
                ),
                settings: true,
            });
        }
        if self.devices.loaded && self.devices.mics.is_empty() {
            return Some(Note::Info(tr!("No microphone found")));
        }
        None
    }

    pub fn system_note(&self) -> Option<Note> {
        if !self.system_supported() {
            return Some(Note::Info(
                self.devices
                    .system_audio_reason
                    .clone()
                    .unwrap_or_else(|| tr!("System audio is not available on this computer")),
            ));
        }
        if self.system_audio && self.screen_blocked() {
            return Some(Note::Problem {
                text: tr!(
                    "System audio needs Screen Recording permission. Turn it on in System Settings, or record the microphone only."
                ),
                settings: true,
            });
        }
        None
    }

    /// The hint under "System audio" (the board's text; the macOS permission note only there).
    pub fn system_hint(platform: Platform) -> String {
        match platform {
            Platform::Mac => tr!(
                "For online calls: the other people\u{2019}s voices. Needs Screen Recording permission on macOS; microphone-only recordings never ask for it."
            ),
            Platform::Windows => tr!("For online calls: the other people\u{2019}s voices."),
        }
    }

    /// What will be recorded: microphone and system audio that can actually be used.
    pub fn effective_sources(&self) -> (bool, bool) {
        (
            self.mic && !self.mic_blocked(),
            self.system_audio && self.system_supported() && !self.screen_blocked(),
        )
    }

    /// Why Start is off, or `None` when it is allowed.
    pub fn blocker(&self) -> Option<String> {
        if !self.mic && !self.system_audio {
            return Some(tr!("Turn on the microphone or system audio to record."));
        }
        let (mic, system) = self.effective_sources();
        if mic || system {
            return None;
        }
        if self.system_audio && self.screen_blocked() && !self.mic {
            return Some(tr!(
                "System audio needs Screen Recording permission on macOS."
            ));
        }
        if self.mic && self.mic_blocked() {
            return Some(tr!("Microphone access is off for CaptureMann."));
        }
        Some(tr!("The selected sources are not available."))
    }

    /// The `start_recording` parameters, or `None` while a blocker stands. A source that cannot
    /// be used (blocked microphone with system audio on, unsupported system audio) is left out
    /// so the recording can still start with the other one.
    pub fn params(&self) -> Option<Value> {
        if self.blocker().is_some() {
            return None;
        }
        let (mic, system) = self.effective_sources();
        Some(start_params(
            self.preset,
            mic,
            self.mic_device.as_deref(),
            system,
            self.people_visible()
                .then(|| self.people.stored())
                .flatten(),
        ))
    }

    /// The choices to remember for the next sheet and the shortcut.
    pub fn remembered(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert(KEY_PRESET.into(), json!(self.preset.key()));
        m.insert(KEY_MIC.into(), json!(self.mic));
        m.insert(KEY_SYSTEM.into(), json!(self.system_audio));
        m.insert(KEY_SEEN.into(), json!(true));
        m
    }

    // ---- the lines under the cards ---------------------------------------------------------

    /// Whether "How many people?" is shown.
    pub fn people_visible(&self) -> bool {
        self.preset.asks_people()
    }

    /// The consent line shows while `recording.consent_notice` is on: every time for presets
    /// with other people, and for the first audio recording of any preset (VOC-1).
    pub fn consent_visible(&self) -> bool {
        self.consent_notice
            && !self.consent_dismissed
            && (self.preset.involves_others() || !self.seen)
    }

    pub fn consent_text() -> String {
        tr!("Let everyone know they\u{2019}re being recorded.")
    }

    /// The "After you stop" chips: (label, on). On means the step will run; off means "if it is
    /// turned on" (Settings, the AI page).
    pub fn after_stop(&self) -> [(String, bool); 3] {
        let transcribe = match &self.transcribe_model {
            Some(m) => (tr!("Transcribe \u{00B7} {model}", model = m), true),
            None => (tr!("Transcribe, if turned on"), false),
        };
        let speakers = if self.preset == Preset::Note {
            // One voice: there is nobody to tell apart.
            (tr!("Speaker labels, not needed for one voice"), false)
        } else if self.auto_speakers && self.speakers_ready {
            (tr!("Speaker labels"), true)
        } else {
            (tr!("Speaker labels, if turned on"), false)
        };
        let report = (
            tr!("Suggest: {report}", report = self.preset.report()),
            true,
        );
        [transcribe, speakers, report]
    }

    /// Disk use per hour for the chosen sources, in GB.
    pub fn gb_per_hour(&self) -> f64 {
        let (mic, system) = (self.mic, self.system_audio && self.system_supported());
        let mic_gb = if self.noise_suppression {
            2.0 * MIC_GB_PER_HOUR
        } else {
            MIC_GB_PER_HOUR
        };
        let mut gb = 0.0;
        if mic {
            gb += mic_gb;
        }
        if system {
            gb += SYSTEM_GB_PER_HOUR;
        }
        gb
    }

    /// "About 0.7 GB per hour · 412 GB free".
    pub fn disk_text(&self) -> String {
        let gb = format!("{:.1}", self.gb_per_hour());
        match self.free_bytes {
            Some(b) => tr!(
                "About {gb} GB per hour \u{00B7} {free} GB free",
                gb = gb,
                free = (b as f64 / 1e9).round() as u64
            ),
            None => tr!("About {gb} GB per hour", gb = gb),
        }
    }

    /// The disk would not hold the longest recording at this rate.
    pub fn disk_short(&self) -> bool {
        self.free_bytes
            .is_some_and(|b| (b as f64 / 1e9) < self.gb_per_hour() * self.max_duration_s / 3600.0)
    }

    pub fn lid_visible(&self) -> bool {
        self.preset.laptop_on_table()
    }

    pub fn lid_text() -> String {
        tr!("Keep the lid open: closing it stops the recording")
    }

    pub fn limit_text(&self) -> String {
        tr!(
            "Stops by itself after {duration}",
            duration = duration_text(self.max_duration_s)
        )
    }

    // ---- layout ----------------------------------------------------------------------------

    /// Heights of the rows the sheet shows (board V1): the header, the body and the footer. Only
    /// close, as with the screen sheet: the body scrolls and the footer never moves.
    pub fn heights(&self) -> SheetHeights {
        const HEADER: f64 = 18.0 + 40.0 + 6.0;
        const BODY_PAD: f64 = 14.0 + 18.0;
        const GAP: f64 = 16.0;
        // "What are you recording?" 16 + 8, cards: 12 + 18 + 8 + title 17 + 2 + 3 lines + 12.
        const PRESETS: f64 = 16.0 + 8.0 + 12.0 + 18.0 + 8.0 + 17.0 + 2.0 + 3.0 * 16.0 + 12.0;
        const ROW_MIC: f64 = 12.0 + 26.0 + 6.0 + 8.0 + 12.0;
        const ROW_SYSTEM: f64 = 12.0 + 20.0 + 2.0 * 16.0 + 12.0 + 1.0;
        const ROW_PEOPLE: f64 = 12.0 + 36.0 + 12.0 + 1.0;
        const CARD_BORDER: f64 = 2.0;
        const CHIPS: f64 = 26.0;
        const CONSENT: f64 = 11.0 + 18.0 + 11.0 + 2.0;
        const LINE: f64 = 16.0;
        const FOOTER: f64 = 14.0 + 40.0 + 14.0 + 1.0;
        const NOTE: f64 = 10.0 + 2.0 * 16.0;
        const PICKER_ROW: f64 = 32.0;
        let mut card = CARD_BORDER + ROW_MIC + ROW_SYSTEM;
        if self.people_visible() {
            card += ROW_PEOPLE;
        }
        if self.picker_open {
            card += PICKER_ROW * self.devices.mics.len().max(1) as f64 + 8.0;
        }
        if self.mic_note().is_some() {
            card += NOTE;
        }
        if self.system_note().is_some() {
            card += NOTE;
        }
        let mut body = BODY_PAD + PRESETS + GAP + card + GAP + CHIPS;
        if self.consent_visible() {
            body += GAP + CONSENT;
        }
        let lines = 1.0 + f64::from(u8::from(self.lid_visible() && self.disk_short()));
        body += GAP + LINE * lines;
        SheetHeights {
            header: HEADER,
            body,
            footer: FOOTER + if self.blocker().is_some() { 24.0 } else { 0.0 },
        }
    }
}

/// "4 h", "1 h 30 min", "45 min".
pub fn duration_text(secs: f64) -> String {
    let minutes = (secs / 60.0).round().max(1.0) as u64;
    let (h, m) = (minutes / 60, minutes % 60);
    match (h, m) {
        (0, m) => tr!("{m} min", m = m),
        (h, 0) => tr!("{h} h", h = h),
        (h, m) => tr!("{h} h {m} min", h = h, m = m),
    }
}

/// The text of the sheet's mode switch label with its shortcut: ("Screen", "Audio").
pub fn mode_labels() -> (String, String) {
    (
        tr_ctx!("record sheet mode", "Screen"),
        tr_ctx!("record sheet mode", "Audio"),
    )
}

/// The header's subtitle in Audio mode.
pub fn subtitle() -> String {
    tr!("Audio only: no screen, camera or cursor.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::sheet::{DeviceChoice, Perm};

    fn defaults() -> AudioDefaults {
        AudioDefaults::from_settings(&Map::new())
    }

    fn model() -> AudioSheetModel {
        AudioSheetModel::new(&defaults())
    }

    fn devices(mic: Perm, screen: Perm, system: bool) -> Devices {
        Devices {
            mics: vec![
                DeviceChoice {
                    id: "builtin".into(),
                    name: "MacBook Pro Microphone".into(),
                    is_default: true,
                },
                DeviceChoice {
                    id: "usb".into(),
                    name: "USB Mic".into(),
                    is_default: false,
                },
            ],
            mic_permission: mic,
            screen_permission: screen,
            system_audio_supported: system,
            system_audio_reason: (!system).then(|| "No system audio here".to_owned()),
            loaded: true,
            ..Devices::default()
        }
    }

    /// VOC-1 preset table: the sources each card starts with, the report it suggests, the
    /// `preset` key the daemon takes.
    #[test]
    fn voc1_presets_set_the_documented_defaults() {
        let keys: Vec<_> = Preset::ALL.iter().map(|p| p.key()).collect();
        assert_eq!(keys, ["room", "online", "lecture", "product", "note"]);
        let sources: Vec<_> = Preset::ALL.iter().map(|p| p.sources()).collect();
        assert_eq!(
            sources,
            [
                (true, false),
                (true, true),
                (true, false),
                (true, true),
                (true, false)
            ]
        );
        let reports: Vec<_> = Preset::ALL.iter().map(|p| p.report()).collect();
        assert_eq!(
            reports,
            [
                "Meeting minutes",
                "Meeting minutes",
                "Lecture notes",
                "Product presentation recap",
                "Summary"
            ]
        );
        for p in Preset::ALL {
            assert_eq!(Preset::parse(p.key()), Some(p));
            assert!(!p.title().is_empty() && !p.blurb().is_empty());
        }
        assert_eq!(Preset::parse("meeting"), None);
        // The daemon accepts exactly these keys (`AudioPreset::parse`).
        assert_eq!(Preset::Online.title(), "Online meeting");
    }

    /// Choosing a card changes the sources, and the user can still change them after.
    #[test]
    fn choosing_a_preset_sets_defaults_that_stay_editable() {
        let mut m = model();
        assert_eq!(
            (m.preset, m.mic, m.system_audio),
            (Preset::Room, true, false)
        );
        m.choose(Preset::Online);
        assert_eq!((m.mic, m.system_audio), (true, true));
        m.system_audio = false;
        assert_eq!(m.params().unwrap()["system_audio"], json!(false));
        assert_eq!(m.params().unwrap()["preset"], "online");
        m.choose(Preset::Room);
        assert_eq!(m.people, People::Two);
        m.choose(Preset::Note);
        assert!(!m.people_visible());
    }

    /// The request: mode audio, the preset, the sources; never a screen-only field (AC9).
    #[test]
    fn voc1_start_params_are_audio_only() {
        let m = model();
        let p = m.params().unwrap();
        assert_eq!(
            p,
            json!({"mode": "audio", "preset": "room", "mic": true, "system_audio": false,
                   "people": 2})
        );
        for banned in [
            "target", "camera", "fps", "size", "codec", "quality", "cursor",
        ] {
            assert!(p.get(banned).is_none(), "{banned}");
        }
        let mut m = model();
        m.mic_device = Some("usb".into());
        assert_eq!(m.params().unwrap()["mic"], json!({"device_id": "usb"}));
        m.mic = false;
        m.system_audio = true;
        assert_eq!(m.params().unwrap()["mic"], json!(false));
    }

    /// Nothing on, or the only chosen source unusable: Start is off with a reason, and nothing
    /// is sent.
    #[test]
    fn start_needs_a_usable_source() {
        let mut m = model();
        m.mic = false;
        assert!(m.blocker().unwrap().contains("microphone or system audio"));
        assert!(m.params().is_none());
        let mut m = model();
        m.set_devices(devices(Perm::Denied, Perm::Authorized, true));
        assert_eq!(
            m.blocker().as_deref(),
            Some("Microphone access is off for CaptureMann.")
        );
        assert!(matches!(
            m.mic_note(),
            Some(Note::Problem { settings: true, .. })
        ));
        // Microphone denied but system audio usable: start with system audio only.
        m.system_audio = true;
        let p = m.params().unwrap();
        assert_eq!(
            (p["mic"].clone(), p["system_audio"].clone()),
            (json!(false), json!(true))
        );
        // System audio only, Screen Recording denied: the sheet offers the microphone only.
        let mut m = model();
        m.mic = false;
        m.system_audio = true;
        m.set_devices(devices(Perm::Authorized, Perm::Denied, true));
        assert!(m.blocker().unwrap().contains("Screen Recording"));
        assert!(matches!(
            m.system_note(),
            Some(Note::Problem { settings: true, .. })
        ));
        // Mic and system, Screen Recording denied: the microphone still records.
        m.mic = true;
        let p = m.params().unwrap();
        assert_eq!(p["system_audio"], json!(false));
        // A microphone-only start never mentions Screen Recording (AC1).
        let mut m = model();
        m.set_devices(devices(Perm::Authorized, Perm::Denied, true));
        assert!(m.blocker().is_none() && m.system_note().is_none());
    }

    #[test]
    fn unsupported_system_audio_is_left_out() {
        let mut m = model();
        m.choose(Preset::Online);
        m.set_devices(devices(Perm::Authorized, Perm::Authorized, false));
        assert!(!m.system_supported());
        assert_eq!(
            m.system_note(),
            Some(Note::Info("No system audio here".into()))
        );
        assert_eq!(m.params().unwrap()["system_audio"], json!(false));
    }

    #[test]
    fn a_missing_remembered_microphone_falls_back_to_the_default() {
        let mut m = model();
        m.mic_device = Some("gone".into());
        m.set_devices(devices(Perm::Authorized, Perm::Authorized, true));
        assert_eq!(m.mic_device, None);
        assert_eq!(m.mic_name(), "MacBook Pro Microphone");
        m.mic_device = Some("usb".into());
        assert_eq!(m.mic_name(), "USB Mic");
        assert_eq!(model().mic_name(), "Default microphone");
    }

    /// VOC-1.AC7: the consent line shows while the setting is on, every time for presets with
    /// other people and once for a voice note; "Don't show again" hides it at once.
    #[test]
    fn voc1_ac7_consent_line_rules() {
        let mut m = model();
        assert!(m.consent_visible());
        m.consent_dismissed = true;
        assert!(!m.consent_visible());
        let off = AudioDefaults::from_settings(json!({ KEY_CONSENT: false }).as_object().unwrap());
        assert!(!AudioSheetModel::new(&off).consent_visible());
        // A voice note after the first recording: nobody else is recorded.
        let seen = AudioDefaults::from_settings(
            json!({ KEY_SEEN: true, KEY_PRESET: "note" })
                .as_object()
                .unwrap(),
        );
        assert!(!AudioSheetModel::new(&seen).consent_visible());
        let first =
            AudioDefaults::from_settings(json!({ KEY_PRESET: "note" }).as_object().unwrap());
        assert!(AudioSheetModel::new(&first).consent_visible());
        let seen_room = AudioDefaults::from_settings(
            json!({ KEY_SEEN: true, KEY_PRESET: "room" })
                .as_object()
                .unwrap(),
        );
        assert!(AudioSheetModel::new(&seen_room).consent_visible());
        assert_eq!(
            AudioSheetModel::consent_text(),
            "Let everyone know they\u{2019}re being recorded."
        );
    }

    /// The remembered choices come back (the sheet, the shortcut and the launcher agree), and a
    /// start marks the first audio recording as done.
    #[test]
    fn remembered_choices_round_trip() {
        let mut m = model();
        m.choose(Preset::Online);
        m.system_audio = false;
        let saved = m.remembered();
        assert_eq!(saved[KEY_SEEN], json!(true));
        let d = AudioDefaults::from_settings(&saved);
        assert_eq!(
            (d.preset, d.mic, d.system_audio),
            (Preset::Online, true, false)
        );
        assert!(d.seen);
        // The shortcut starts with them; with nothing left on it records the microphone.
        assert_eq!(
            d.quick_params(),
            json!({"mode": "audio", "preset": "online", "mic": true, "system_audio": false})
        );
        let none = AudioDefaults {
            mic: false,
            system_audio: false,
            ..d
        };
        assert_eq!(none.quick_params()["mic"], json!(true));
        // Without settings: the room preset, microphone only, 4 h.
        let d = defaults();
        assert_eq!(
            (d.preset, d.mic, d.system_audio),
            (Preset::Room, true, false)
        );
        assert_eq!(d.max_duration_s, 4.0 * 3600.0);
        assert!(
            d.consent_notice && !d.seen && d.auto_speakers,
            "speaker step on by default"
        );
        // A bad stored value falls back to the default.
        let bad = AudioDefaults::from_settings(
            json!({ KEY_PRESET: "banana", KEY_MAX_DURATION: -3 })
                .as_object()
                .unwrap(),
        );
        assert_eq!(
            (bad.preset, bad.max_duration_s),
            (Preset::Room, 4.0 * 3600.0)
        );
    }

    /// "After you stop": the chips say what runs; the speaker step follows `ai.auto_speakers`.
    #[test]
    fn after_stop_chips() {
        let mut m = model();
        let [t, s, r] = m.after_stop();
        assert_eq!(t, ("Transcribe, if turned on".to_owned(), false));
        // The speaker step is part of the default plan.
        assert_eq!(s, ("Speaker labels".to_owned(), true));
        assert_eq!(r, ("Suggest: Meeting minutes".to_owned(), true));
        // Turned off by the user, or the models are missing: "if turned on".
        m.auto_speakers = false;
        assert_eq!(
            m.after_stop()[1],
            ("Speaker labels, if turned on".to_owned(), false)
        );
        m.auto_speakers = true;
        m.speakers_ready = false;
        assert!(!m.after_stop()[1].1);
        m.speakers_ready = true;
        // A voice note has one voice.
        m.choose(Preset::Note);
        assert!(!m.after_stop()[1].1);
        m.transcribe_model = Some("Medium".into());
        m.choose(Preset::Lecture);
        let [t, s, r] = m.after_stop();
        assert_eq!(t, ("Transcribe \u{00B7} Medium".to_owned(), true));
        assert_eq!(s, ("Speaker labels".to_owned(), true));
        assert_eq!(r.0, "Suggest: Lecture notes");
    }

    /// The footer line: disk per hour with free space, the lid hint for the room preset only,
    /// and the length limit.
    #[test]
    fn footer_line_facts() {
        let mut m = model();
        m.noise_suppression = false;
        m.free_bytes = Some(412_000_000_000);
        assert_eq!(m.disk_text(), "About 0.7 GB per hour \u{00B7} 412 GB free");
        m.noise_suppression = true;
        assert_eq!(m.disk_text(), "About 1.4 GB per hour \u{00B7} 412 GB free");
        m.choose(Preset::Online);
        assert!((m.gb_per_hour() - 2.8).abs() < 1e-9);
        m.free_bytes = None;
        assert_eq!(m.disk_text(), "About 2.8 GB per hour");
        assert!(!m.lid_visible());
        m.choose(Preset::Room);
        assert!(m.lid_visible());
        assert_eq!(m.limit_text(), "Stops by itself after 4 h");
        m.max_duration_s = 90.0 * 60.0;
        assert_eq!(m.limit_text(), "Stops by itself after 1 h 30 min");
        m.max_duration_s = 45.0 * 60.0;
        assert_eq!(m.limit_text(), "Stops by itself after 45 min");
        // 4 h of microphone-only at 1.4 GB/h needs 5.6 GB.
        m.max_duration_s = 4.0 * 3600.0;
        m.free_bytes = Some(5_000_000_000);
        assert!(m.disk_short());
        m.free_bytes = Some(6_000_000_000);
        assert!(!m.disk_short());
    }

    /// "How many people?" is sent as `people` (1 to 8) when the sheet knows it; "Not sure" and a
    /// voice note send nothing; the shortcut never sends it.
    #[test]
    fn voc1_the_people_answer_is_sent_with_the_start() {
        let mut m = model();
        for (p, want) in [
            (People::NotSure, None),
            (People::Two, Some(2)),
            (People::Three, Some(3)),
            (People::Four, Some(4)),
            (People::FivePlus, Some(5)),
        ] {
            m.people = p;
            assert_eq!(p.stored(), want);
            let params = m.params().unwrap();
            assert_eq!(
                params.get("people").and_then(Value::as_u64),
                want.map(u64::from)
            );
        }
        m.choose(Preset::Note);
        m.people = People::Four;
        assert!(
            m.params().unwrap().get("people").is_none(),
            "a voice note asks nobody"
        );
        assert!(defaults().quick_params().get("people").is_none());
    }

    #[test]
    fn people_choices_feed_the_speaker_hints() {
        let counts: Vec<_> = People::ALL
            .iter()
            .map(|p| (p.count(), p.at_least()))
            .collect();
        assert_eq!(
            counts,
            [
                (None, None),
                (Some(2), None),
                (Some(3), None),
                (Some(4), None),
                (None, Some(5))
            ]
        );
        assert_eq!(People::NotSure.label(), "Not sure");
        assert_eq!(People::FivePlus.label(), "5+");
    }

    /// The window height follows the rows: it grows with the consent line, notes and an open
    /// picker, and it is a plausible size for the board (about 650 to 750 pt).
    #[test]
    fn heights_follow_the_rows() {
        let m = model();
        let h = m.heights();
        assert!((600.0..800.0).contains(&h.total()), "{}", h.total());
        let mut quiet = m.clone();
        quiet.consent_dismissed = true;
        assert!(quiet.heights().body < h.body);
        let mut open = m.clone();
        open.picker_open = true;
        open.set_devices(devices(Perm::Authorized, Perm::Authorized, true));
        assert!(open.heights().body > h.body);
        let mut note = m.clone();
        note.set_devices(devices(Perm::Denied, Perm::Authorized, true));
        assert!(note.heights().footer > h.footer, "the blocker line");
        let mut voice = m.clone();
        voice.choose(Preset::Note);
        assert!(voice.heights().body < h.body, "no people row");
        assert_eq!(AUDIO_SHEET_WIDTH, 760.0);
        assert_eq!(METER_SEGMENTS, 40);
    }
}
