//! The start sheet's model (REC-1, CAM-4, AUD-1): displays, devices and permissions as the daemon
//! reports them (`list_targets`, `list_devices`), the user's choices, and whether Start is allowed.
//! Display-free; `ui::recording::sheet` draws it.

use cm_core::ErrorCode;
use cm_i18n::tr;
use serde_json::Value;

use crate::recording::start::{
    resolve_camera_device, CameraChoice, SheetDefaults, StartOptions, Target, TargetKind,
};

/// CAM-1: does this daemon event make the sheet's device list stale? A camera was plugged in or
/// unplugged (`camera.connected`, `camera.disconnected`), also while no recording runs.
pub fn camera_event_changes_devices(event: &str) -> bool {
    matches!(event, "camera.connected" | "camera.disconnected")
}

/// A permission as `list_devices` reports it (snake_case strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Perm {
    Authorized,
    #[default]
    NotDetermined,
    Denied,
    Restricted,
}

impl Perm {
    pub fn parse(v: Option<&Value>) -> Perm {
        match v.and_then(Value::as_str) {
            Some("authorized" | "granted") => Perm::Authorized,
            Some("denied") => Perm::Denied,
            Some("restricted") => Perm::Restricted,
            _ => Perm::NotDetermined,
        }
    }

    /// Needs the user to act in System Settings (a prompt can no longer help).
    pub fn blocked(self) -> bool {
        matches!(self, Perm::Denied | Perm::Restricted)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayChoice {
    pub id: u32,
    pub name: String,
    /// Physical px.
    pub size: [u32; 2],
    pub scale: f64,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceChoice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// What `list_devices` said.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Devices {
    pub cameras: Vec<DeviceChoice>,
    pub cameras_error: Option<String>,
    pub camera_permission: Perm,
    pub mics: Vec<DeviceChoice>,
    pub mic_permission: Perm,
    pub system_audio_supported: bool,
    pub system_audio_reason: Option<String>,
    pub screen_permission: Perm,
    /// The device list has been loaded.
    pub loaded: bool,
}

impl Devices {
    pub fn parse(v: &Value) -> Devices {
        let list = |key: &str, default_key: &str| -> Vec<DeviceChoice> {
            v.get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|c| {
                    Some(DeviceChoice {
                        id: c.get("id")?.as_str()?.to_owned(),
                        name: c.get("name")?.as_str()?.to_owned(),
                        is_default: c.get(default_key).and_then(Value::as_bool) == Some(true),
                    })
                })
                .collect()
        };
        let sys = v.get("system_audio");
        Devices {
            cameras: list("cameras", "is_default"),
            cameras_error: v
                .get("cameras_error")
                .and_then(Value::as_str)
                .map(str::to_owned),
            camera_permission: Perm::parse(v.get("camera_permission")),
            mics: list("mics", "default"),
            mic_permission: Perm::parse(v.get("microphone_permission")),
            system_audio_supported: sys
                .and_then(|s| s.get("supported"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            system_audio_reason: sys
                .and_then(|s| s.get("reason"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            screen_permission: Perm::parse(v.get("screen_recording_permission")),
            loaded: true,
        }
    }
}

/// `list_targets { windows: false }`.
pub fn parse_displays(v: &Value) -> Vec<DisplayChoice> {
    v.get("displays")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let size = d.get("bounds")?.get("size")?;
            // `PhysicalSize` serializes as `[w, h]`; an object form is accepted too.
            let (w, h) = match size.as_array() {
                Some(a) => (a.first()?.as_u64()?, a.get(1)?.as_u64()?),
                None => (size.get("width")?.as_u64()?, size.get("height")?.as_u64()?),
            };
            Some(DisplayChoice {
                id: u32::try_from(d.get("id")?.as_u64()?).ok()?,
                name: d.get("name")?.as_str()?.to_owned(),
                size: [u32::try_from(w).ok()?, u32::try_from(h).ok()?],
                scale: d.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
                primary: d.get("is_primary").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

/// Where the `list_targets` call stands (REC-1). "No display found" is only for `Loaded` with zero
/// displays; a failed listing says why instead.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TargetsLoad {
    /// Asked, no answer yet.
    #[default]
    Loading,
    Loaded,
    /// The daemon answered with a shared error code (`permission_missing`, `daemon_unavailable`,
    /// `internal`, ...) and its message.
    Failed {
        code: ErrorCode,
        message: String,
    },
}

/// What the sheet shows above the rows when the listing failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetsProblem {
    /// Screen Recording is missing: the note, the System Settings and Permissions buttons and the
    /// restart hint.
    PermissionMissing { text: String, hint: String },
    /// Anything else: the daemon's message and a Retry button.
    Error { message: String },
}

/// A note under a row of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// Plain information.
    Info(String),
    /// Something the user must fix; `settings` offers "Open System Settings" (CAM-4.AC3).
    Problem { text: String, settings: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetModel {
    pub displays: Vec<DisplayChoice>,
    pub targets: TargetsLoad,
    pub devices: Devices,
    pub kind: TargetKind,
    pub display_id: Option<u32>,
    /// The window or region chosen with the overlay picker.
    pub picked: Option<Target>,
    pub camera: bool,
    pub camera_device: Option<String>,
    /// CAM-4.AC2: the remembered camera was missing and the default is used.
    pub camera_fell_back: bool,
    pub mic: bool,
    pub mic_device: Option<String>,
    pub system_audio: bool,
    pub bubble: CameraChoice,
}

impl SheetModel {
    pub fn new(defaults: &SheetDefaults) -> SheetModel {
        SheetModel {
            displays: Vec::new(),
            targets: TargetsLoad::Loading,
            devices: Devices::default(),
            kind: defaults.target_kind,
            display_id: None,
            picked: None,
            camera: defaults.camera,
            camera_device: defaults.camera_device.clone(),
            camera_fell_back: false,
            mic: defaults.mic,
            mic_device: defaults.mic_device.clone(),
            system_audio: defaults.system_audio,
            bubble: defaults.bubble.clone(),
        }
    }

    /// The displays arrived: select the primary one unless one is already chosen and still there.
    pub fn set_displays(&mut self, displays: Vec<DisplayChoice>) {
        self.targets = TargetsLoad::Loaded;
        self.displays = displays;
        let valid = self
            .display_id
            .is_some_and(|id| self.displays.iter().any(|d| d.id == id));
        if !valid {
            self.display_id = self
                .displays
                .iter()
                .find(|d| d.primary)
                .or_else(|| self.displays.first())
                .map(|d| d.id);
        }
    }

    /// The listing is being (re)loaded, e.g. after Retry.
    pub fn set_targets_loading(&mut self) {
        self.targets = TargetsLoad::Loading;
    }

    /// The listing failed. A call that carried no code counts as `internal`. No display can be
    /// chosen, so the old list is dropped.
    pub fn set_targets_failed(&mut self, code: Option<ErrorCode>, message: &str) {
        self.displays.clear();
        self.display_id = None;
        self.targets = TargetsLoad::Failed {
            code: code.unwrap_or(ErrorCode::Internal),
            message: message.trim().to_owned(),
        };
    }

    /// Applies a `list_targets` outcome.
    pub fn apply_targets(&mut self, result: Result<&Value, (Option<ErrorCode>, &str)>) {
        match result {
            Ok(v) => self.set_displays(parse_displays(v)),
            Err((code, message)) => self.set_targets_failed(code, message),
        }
    }

    /// Why the listing failed, as the sheet shows it.
    pub fn targets_problem(&self) -> Option<TargetsProblem> {
        let TargetsLoad::Failed { code, message } = &self.targets else {
            return None;
        };
        Some(if *code == ErrorCode::PermissionMissing {
            TargetsProblem::PermissionMissing {
                text: tr!("Screen Recording permission is missing"),
                hint: crate::permissions::screen_missing_hint(),
            }
        } else {
            TargetsProblem::Error {
                message: if message.is_empty() {
                    tr!("The displays could not be listed.")
                } else {
                    crate::daemon_text::error_text(Some(*code), message)
                },
            }
        })
    }

    /// The devices arrived: apply CAM-4.AC2 to the remembered camera and drop a remembered mic
    /// that is gone.
    pub fn set_devices(&mut self, devices: Devices) {
        let ids: Vec<String> = devices.cameras.iter().map(|c| c.id.clone()).collect();
        let (dev, fell_back) = resolve_camera_device(self.camera_device.as_deref(), &ids);
        self.camera_device = dev;
        self.camera_fell_back = fell_back;
        if let Some(m) = &self.mic_device {
            if !devices.mics.iter().any(|d| &d.id == m) {
                self.mic_device = None;
            }
        }
        // No camera, no camera toggle (CAM-4: "No camera found").
        if devices.loaded && devices.cameras.is_empty() {
            self.camera = false;
        }
        self.devices = devices;
    }

    /// Changing the target kind clears a picked window or region of another kind.
    pub fn set_kind(&mut self, kind: TargetKind) {
        self.kind = kind;
        if self.picked.is_some_and(|p| p.kind() != kind) {
            self.picked = None;
        }
    }

    pub fn pick(&mut self, target: Target) {
        self.kind = target.kind();
        self.picked = Some(target);
    }

    /// What will be recorded, or `None` while a window or region still has to be chosen.
    pub fn target(&self) -> Option<Target> {
        match self.kind {
            TargetKind::Display => self
                .display_id
                .map(|display_id| Target::Display { display_id }),
            TargetKind::Window | TargetKind::Region => self.picked,
        }
    }

    pub fn target_summary(&self) -> String {
        match self.target() {
            Some(Target::Display { display_id }) => self
                .displays
                .iter()
                .find(|d| d.id == display_id)
                .map(|d| {
                    tr!(
                        "{name} \u{00B7} {w} \u{00D7} {h}",
                        name = d.name,
                        w = d.size[0],
                        h = d.size[1]
                    )
                })
                .unwrap_or_else(|| tr!("Display {display_id}", display_id = display_id)),
            Some(t) => t.summary(),
            None => match self.kind {
                TargetKind::Window => tr!("Choose a window"),
                TargetKind::Region => tr!("Choose a region"),
                TargetKind::Display => match self.targets {
                    TargetsLoad::Loading => tr!("Looking for displays\u{2026}"),
                    TargetsLoad::Loaded => tr!("No display found"),
                    TargetsLoad::Failed { .. } => tr!("Displays unavailable"),
                },
            },
        }
    }

    /// Why Start is not available yet, or `None` when it is.
    pub fn blocker(&self) -> Option<String> {
        match self.targets_problem() {
            Some(TargetsProblem::PermissionMissing { .. }) => {
                return Some(tr!("Screen Recording permission is missing."))
            }
            Some(TargetsProblem::Error { .. }) => {
                return Some(tr!("The displays could not be listed."))
            }
            None => {}
        }
        if self.devices.screen_permission.blocked() {
            return Some(tr!("Screen Recording is turned off for CaptureMann."));
        }
        self.target().is_none().then(|| match self.kind {
            TargetKind::Window => tr!("Choose a window to record."),
            TargetKind::Region => tr!("Choose a region to record."),
            TargetKind::Display => match self.targets {
                TargetsLoad::Loading => tr!("Looking for displays\u{2026}"),
                _ => tr!("No display was found."),
            },
        })
    }

    pub fn camera_available(&self) -> bool {
        !self.devices.loaded || !self.devices.cameras.is_empty()
    }

    /// The note under the Camera row: no camera, permission denied (CAM-4.AC3), fallback
    /// (CAM-4.AC2).
    pub fn camera_note(&self) -> Option<Note> {
        if self.devices.loaded && self.devices.cameras.is_empty() {
            return Some(Note::Info(tr!("No camera found")));
        }
        if self.camera && self.devices.camera_permission.blocked() {
            return Some(Note::Problem {
                text: tr!(
                    "Camera access is off. Turn it on in System Settings; you can also record without the camera."
                ),
                settings: true,
            });
        }
        if self.camera_fell_back {
            return Some(Note::Info(tr!(
                "The camera chosen in Settings isn\u{2019}t connected. Using the system default."
            )));
        }
        None
    }

    pub fn mic_note(&self) -> Option<Note> {
        if self.mic && self.devices.mic_permission.blocked() {
            return Some(Note::Problem {
                text: tr!(
                    "Microphone access is off. Turn it on in System Settings; recording starts without it."
                ),
                settings: true,
            });
        }
        if self.devices.loaded && self.devices.mics.is_empty() {
            return Some(Note::Info(tr!("No microphone found")));
        }
        None
    }

    pub fn system_audio_note(&self) -> Option<String> {
        if self.devices.loaded && !self.devices.system_audio_supported {
            return Some(
                self.devices
                    .system_audio_reason
                    .clone()
                    .unwrap_or_else(|| tr!("System audio is not available on this computer")),
            );
        }
        None
    }

    /// The choices as a start request (`None` while there is no target). A camera that cannot be
    /// used because access is blocked is left out so recording can still start (CAM-4.AC3).
    pub fn options(&self) -> Option<StartOptions> {
        let target = self.target()?;
        let camera =
            (self.camera && self.camera_available() && !self.devices.camera_permission.blocked())
                .then(|| CameraChoice {
                    device: self.camera_device.clone(),
                    ..self.bubble.clone()
                });
        Some(StartOptions {
            target,
            camera,
            mic: self.mic && !self.devices.mic_permission.blocked(),
            mic_device: self.mic_device.clone(),
            system_audio: self.system_audio && self.devices.system_audio_supported,
        })
    }
}

// ---- layout: how tall the sheet window must be ------------------------------------------------
//
// The sheet is a native window, and GPUI cannot size a window to its content, so the window
// height is computed from the rows the model will show. The estimate only has to be close: the
// header and the footer (Cancel / Start) are fixed and the body between them scrolls, so a row
// that turns out taller than estimated, or a display too short for all rows, costs scrolling and
// never hides the buttons.

use crate::recording::geometry::Rect;

/// Window width in points (redesign R9: 600; it was 560).
pub const SHEET_WIDTH: f64 = 600.0;
/// Space kept free around the window on the visible part of the display.
pub const SHEET_MARGIN: f64 = 24.0;
/// The smallest window we ask for: header, a usable slice of body and the footer.
pub const SHEET_MIN_HEIGHT: f64 = 360.0;

/// Row heights in points, from the R9 board (`design/redesign/screens/html/NewRecordSheet.html`):
/// header 20 + 44 + 6, body padding 16 / 20 with 18 between blocks, 84 px kind tiles, 32 px
/// chips, settings rows of 14 + 36 + 14 inside one card.
mod row {
    pub const HEADER: f64 = 20.0 + 44.0 + 6.0;
    pub const BODY_TOP: f64 = 16.0;
    /// The gap between the blocks of the body.
    pub const SECTION_GAP: f64 = 18.0;
    pub const BODY_BOTTOM: f64 = 20.0;
    pub const KIND_TILES: f64 = 84.0;
    pub const SUMMARY: f64 = 20.0;
    pub const BUTTON: f64 = 40.0;
    pub const CHIPS: f64 = 32.0;
    /// A settings row of the card: padding 14 + the label and one hint line (20 + 16) + 14.
    pub const ROW: f64 = 14.0 + 36.0 + 14.0;
    pub const ROW_DIVIDER: f64 = 1.0;
    /// The card border.
    pub const CARD_BORDER: f64 = 2.0;
    /// The permission problem card around its text and buttons (padding 14, border 1).
    pub const PROBLEM_PAD: f64 = 14.0 + 14.0 + 2.0;
    pub const CAPTION_LINE: f64 = 16.0;
    /// Gap above an extra block inside a row (chips, a note).
    pub const NOTE_GAP: f64 = 10.0;
    pub const FOOTER: f64 = 14.0 + 40.0 + 14.0 + 1.0;
    pub const BLOCKER: f64 = 16.0 + 8.0;
    /// Characters of caption text that fit on one line of the body (556 px).
    pub const CHARS_PER_LINE: usize = 88;
    /// Characters of a row's hint that fit on one line next to the tile and the switch.
    pub const ROW_HINT_CHARS: usize = 74;
}

/// The icon over each kind tile and in the sheet's rows (the icon map: Display `monitor`, Window
/// `app-window`, Region `cm-capture-region`; webcam bubble `cm-webcam-bubble`, microphone `mic`,
/// system audio `volume-2`).
pub fn kind_icon(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Display => "monitor",
        TargetKind::Window => "app-window",
        TargetKind::Region => "cm-capture-region",
    }
}

/// The icons of the three source rows, in order: webcam bubble, microphone, system audio.
pub const SOURCE_ICONS: [&str; 3] = ["cm-webcam-bubble", "mic", "volume-2"];

/// A source of the recording besides the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Webcam,
    Microphone,
    SystemAudio,
}

/// One Include chip of the Window kind (R20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeChip {
    pub source: Source,
    pub icon: &'static str,
    pub label: String,
    pub on: bool,
    pub enabled: bool,
}

fn lines(text: &str) -> f64 {
    text.chars().count().div_ceil(row::CHARS_PER_LINE).max(1) as f64
}

/// The three parts of the window, top to bottom. The footer is never part of the scrolling body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetHeights {
    pub header: f64,
    /// All body rows, as if nothing scrolled.
    pub body: f64,
    /// Cancel and Start (and the reason Start is off, when there is one).
    pub footer: f64,
}

impl SheetHeights {
    pub fn total(&self) -> f64 {
        self.header + self.body + self.footer
    }
}

/// How the window is split once its height is known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetLayout {
    pub window: f64,
    /// Height the body can use (window minus header and footer).
    pub body_viewport: f64,
    /// The body is taller than its viewport and scrolls.
    pub body_scrolls: bool,
    /// Cancel and Start lie inside the window. Always true; kept as data so a test pins it.
    pub footer_visible: bool,
}

impl SheetModel {
    /// Heights of the rows the sheet shows for this model.
    pub fn heights(&self) -> SheetHeights {
        use row::*;
        // The blocks of the body, each after an 18 px gap: the kind tiles, the permission
        // problem card, the target row (chips, or the summary, or the summary and Choose button),
        // and the card of the three source rows.
        let mut body = BODY_TOP + KIND_TILES;
        body += match self.targets_problem() {
            None => 0.0,
            // Text, hint, and a row of buttons (Open System Settings / Permissions).
            Some(TargetsProblem::PermissionMissing { text, hint }) => {
                SECTION_GAP
                    + PROBLEM_PAD
                    + CAPTION_LINE * (lines(&text) + lines(&hint))
                    + 6.0
                    + BUTTON
            }
            // The message and the Retry button.
            Some(TargetsProblem::Error { message }) => {
                SECTION_GAP + PROBLEM_PAD + CAPTION_LINE * lines(&message) + 6.0 + BUTTON
            }
        };
        if self.kind == TargetKind::Window {
            return self.window_heights(body);
        }
        body += SECTION_GAP
            + match self.kind {
                TargetKind::Display if self.displays.len() > 1 => CHIPS,
                TargetKind::Display => SUMMARY,
                _ => BUTTON,
            };
        let note = |n: Option<Note>| match n {
            None => 0.0,
            Some(Note::Info(t)) => NOTE_GAP + CAPTION_LINE * lines(&t),
            Some(Note::Problem { text, settings }) => {
                NOTE_GAP + CAPTION_LINE * lines(&text) + if settings { 6.0 + BUTTON } else { 0.0 }
            }
        };
        let hint_extra =
            |t: &str| CAPTION_LINE * (t.chars().count().div_ceil(ROW_HINT_CHARS).max(1) - 1) as f64;
        let mut webcam = ROW + hint_extra(&self.camera_hint());
        if self.camera && self.devices.cameras.len() > 1 {
            webcam += NOTE_GAP + CHIPS;
        }
        webcam += note(self.camera_note());
        let mic = ROW + hint_extra(&self.mic_hint()) + note(self.mic_note());
        let system = ROW + hint_extra(&self.system_hint());
        body += SECTION_GAP
            + CARD_BORDER
            + webcam
            + ROW_DIVIDER
            + mic
            + ROW_DIVIDER
            + system
            + BODY_BOTTOM;
        let footer = FOOTER
            + if self.blocker().is_some() {
                BLOCKER
            } else {
                0.0
            };
        SheetHeights {
            header: HEADER,
            body,
            footer,
        }
    }

    /// R20: the Window kind replaces the target row and the three source rows with the window
    /// list (a fixed-height block) and one row of Include chips; the problem notes and the camera
    /// chips stay under that row. `body` is the height so far (top padding, kind tiles and the
    /// listing problem card).
    fn window_heights(&self, mut body: f64) -> SheetHeights {
        use crate::recording::window_rows::{INCLUDE_ROW_H, LIST_BLOCK_H};
        use row::*;
        let note = |n: Option<Note>| match n {
            None => 0.0,
            Some(Note::Info(t)) => NOTE_GAP + CAPTION_LINE * lines(&t),
            Some(Note::Problem { text, settings }) => {
                NOTE_GAP + CAPTION_LINE * lines(&text) + if settings { 6.0 + BUTTON } else { 0.0 }
            }
        };
        body += SECTION_GAP + f64::from(LIST_BLOCK_H);
        body += SECTION_GAP + f64::from(INCLUDE_ROW_H);
        if self.camera && self.devices.cameras.len() > 1 {
            body += NOTE_GAP + CHIPS;
        }
        body += note(self.camera_note());
        body += note(self.mic_note());
        if let Some(t) = self.system_audio_note() {
            body += NOTE_GAP + CAPTION_LINE * lines(&t);
        }
        body += BODY_BOTTOM;
        let footer = FOOTER
            + if self.blocker().is_some() {
                BLOCKER
            } else {
                0.0
            };
        SheetHeights {
            header: HEADER,
            body,
            footer,
        }
    }

    /// R20: the three sources as compact toggle chips (same state and rules as the switch rows of
    /// the other kinds: no camera or no system audio means a disabled chip, a blocked microphone
    /// shows off).
    pub fn include_chips(&self) -> [IncludeChip; 3] {
        let [cam_icon, mic_icon, sys_icon] = SOURCE_ICONS;
        let cam_enabled = self.camera_available();
        let sys_enabled = self.system_audio_note().is_none();
        [
            IncludeChip {
                source: Source::Webcam,
                icon: cam_icon,
                label: tr!("Webcam bubble"),
                on: self.camera && cam_enabled,
                enabled: cam_enabled,
            },
            IncludeChip {
                source: Source::Microphone,
                icon: mic_icon,
                label: tr!("Microphone"),
                on: self.mic && !self.devices.mic_permission.blocked(),
                enabled: true,
            },
            IncludeChip {
                source: Source::SystemAudio,
                icon: sys_icon,
                label: tr!("System audio"),
                on: self.system_audio && sys_enabled,
                enabled: sys_enabled,
            },
        ]
    }

    /// Flips a source; `false` when it cannot change (a disabled chip). The caller asks for the
    /// OS permission after a turn-on and syncs the camera preview.
    pub fn toggle_source(&mut self, source: Source) -> bool {
        match source {
            Source::Webcam if self.camera_available() => self.camera = !self.camera,
            Source::Microphone => self.mic = !self.mic,
            Source::SystemAudio if self.system_audio_note().is_none() => {
                self.system_audio = !self.system_audio
            }
            _ => return false,
        }
        true
    }

    /// The caption of the webcam row (the live preview text when the bubble is on).
    pub fn camera_hint(&self) -> String {
        if self.camera {
            tr!("Live preview on screen. Drag it anywhere; it stays out of the recording.")
        } else {
            tr!("A circle with your face on the recording.")
        }
    }

    /// The microphone that will be used: the chosen one, else the system default.
    pub fn mic_name(&self) -> Option<String> {
        self.mic_device
            .as_ref()
            .and_then(|id| self.devices.mics.iter().find(|d| &d.id == id))
            .or_else(|| self.devices.mics.iter().find(|d| d.is_default))
            .map(|d| d.name.clone())
    }

    /// The caption of the microphone row.
    pub fn mic_hint(&self) -> String {
        self.mic_name()
            .unwrap_or_else(|| tr!("System default input"))
    }

    /// The caption of the system audio row: why it is unavailable, else what it records.
    pub fn system_hint(&self) -> String {
        self.system_audio_note()
            .unwrap_or_else(|| tr!("Sounds from apps, without your own"))
    }
}

/// The window height for `heights` on a display whose visible part is `visible_h` tall: the full
/// content when it fits with margins, else as tall as the display allows (the body scrolls).
pub fn fit_height(heights: &SheetHeights, visible_h: f64) -> f64 {
    let cap = (visible_h - 2.0 * SHEET_MARGIN).max(1.0);
    heights
        .total()
        .max(SHEET_MIN_HEIGHT)
        .min(cap)
        .max(SHEET_MIN_HEIGHT.min(cap))
}

/// Splits a window of `window_h` into header, scrolling body and footer.
pub fn split(heights: &SheetHeights, window_h: f64) -> SheetLayout {
    let body_viewport = (window_h - heights.header - heights.footer).max(0.0);
    SheetLayout {
        window: window_h,
        body_viewport,
        body_scrolls: heights.body > body_viewport + 0.5,
        footer_visible: window_h >= heights.header + heights.footer,
    }
}

/// `r` moved (and shrunk when it must) to lie inside `bounds` with `margin` on every side.
pub fn clamp_into(r: Rect, bounds: &Rect, margin: f64) -> Rect {
    let w = r.w.min((bounds.w - 2.0 * margin).max(1.0));
    let h = r.h.min((bounds.h - 2.0 * margin).max(1.0));
    let x = r.x.clamp(
        bounds.x + margin,
        (bounds.right() - margin - w).max(bounds.x + margin),
    );
    let y = r.y.clamp(
        bounds.y + margin,
        (bounds.bottom() - margin - h).max(bounds.y + margin),
    );
    Rect::new(x, y, w, h)
}

/// Where a new sheet of `w` x `h` opens on `visible`: centered, a third of the free height from
/// the top (like the launcher), always inside the margins.
pub fn initial_frame(visible: &Rect, w: f64, h: f64) -> Rect {
    clamp_into(
        Rect::new(
            visible.x + (visible.w - w) / 2.0,
            visible.y + (visible.h - h) / 3.0,
            w,
            h,
        ),
        visible,
        SHEET_MARGIN,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn devices_json() -> Value {
        json!({
            "cameras": [
                {"id": "cam-a", "name": "FaceTime HD Camera", "is_default": true},
                {"id": "cam-b", "name": "Logitech C920", "is_default": false},
            ],
            "cameras_error": null, "camera_permission": "authorized",
            "mics": [{"id": "mic-1", "name": "MacBook Microphone", "default": true}],
            "microphone_permission": "authorized",
            "system_audio": {"supported": true, "backend": "sck", "reason": null},
            "screen_recording_permission": "authorized",
        })
    }

    fn displays_json() -> Value {
        json!({"displays": [
            {"id": 2, "name": "Studio Display", "bounds": {"origin": {"x": 3024, "y": 0}, "size": [5120, 2880]}, "scale": 2.0, "is_primary": false},
            {"id": 1, "name": "Built-in", "bounds": {"origin": {"x": 0, "y": 0}, "size": [3024, 1964]}, "scale": 2.0, "is_primary": true},
        ]})
    }

    fn sheet() -> SheetModel {
        let mut m = SheetModel::new(&SheetDefaults::from_settings(&serde_json::Map::new()));
        m.set_displays(parse_displays(&displays_json()));
        m.set_devices(Devices::parse(&devices_json()));
        m
    }

    #[test]
    fn devices_and_displays_parse() {
        let d = Devices::parse(&devices_json());
        assert_eq!(d.cameras.len(), 2);
        assert!(d.cameras[0].is_default && d.mics[0].is_default);
        assert!(d.system_audio_supported);
        assert_eq!(d.camera_permission, Perm::Authorized);
        let displays = parse_displays(&displays_json());
        assert_eq!(displays[1].size, [3024, 1964]);
        assert!(displays[1].primary);
    }

    /// REC-1: the primary display is the default target, and the request is what the user chose.
    #[test]
    fn rec1_default_target_is_the_primary_display() {
        let m = sheet();
        assert_eq!(m.display_id, Some(1));
        assert_eq!(m.target(), Some(Target::Display { display_id: 1 }));
        assert_eq!(m.target_summary(), "Built-in \u{00B7} 3024 \u{00D7} 1964");
        assert!(m.blocker().is_none());
        let o = m.options().unwrap();
        assert!(o.camera.is_none() && o.mic && !o.system_audio);
    }

    /// REC-1: window and region need a pick before Start; switching kind clears a stale pick.
    #[test]
    fn rec1_window_and_region_need_a_pick() {
        let mut m = sheet();
        m.set_kind(TargetKind::Region);
        assert_eq!(m.target(), None);
        assert_eq!(m.blocker().as_deref(), Some("Choose a region to record."));
        assert!(m.options().is_none());
        m.pick(Target::Region {
            display_id: 1,
            rect: [10, 10, 1920, 1080],
        });
        assert_eq!(m.target_summary(), "Region 1920 \u{00D7} 1080");
        assert!(m.blocker().is_none());
        m.set_kind(TargetKind::Window);
        assert_eq!(m.picked, None);
        m.set_kind(TargetKind::Display);
        assert_eq!(m.target(), Some(Target::Display { display_id: 1 }));
    }

    /// CAM-4: two cameras are both listed; the choice goes into the request.
    #[test]
    fn cam4_camera_choice_goes_into_the_request() {
        let mut m = sheet();
        m.camera = true;
        m.camera_device = Some("cam-b".into());
        let o = m.options().unwrap();
        assert_eq!(o.camera.unwrap().device.as_deref(), Some("cam-b"));
        assert_eq!(m.devices.cameras.len(), 2);
    }

    /// CAM-4.AC2: an unplugged remembered camera falls back to the default and a note says so.
    #[test]
    fn cam4_ac2_missing_camera_falls_back_with_a_note() {
        let mut m = SheetModel::new(&SheetDefaults::from_settings(
            serde_json::json!({"camera.device": "cam-gone", "recording.camera": true})
                .as_object()
                .unwrap(),
        ));
        m.set_devices(Devices::parse(&devices_json()));
        assert_eq!(m.camera_device, None);
        assert!(m.camera_fell_back && m.camera);
        assert!(matches!(m.camera_note(), Some(Note::Info(t)) if t.contains("system default")));
    }

    /// CAM-1: only the two hot-plug events make the device list stale.
    #[test]
    fn cam1_only_hotplug_events_refresh_the_device_list() {
        assert!(camera_event_changes_devices("camera.connected"));
        assert!(camera_event_changes_devices("camera.disconnected"));
        for other in ["recording.warning", "camera.changed", "capture.created", ""] {
            assert!(!camera_event_changes_devices(other), "{other}");
        }
    }

    /// CAM-1: unplugging the chosen camera while the sheet is open (the list is reloaded on the
    /// event) falls back to the default with the CAM-4.AC2 note; plugging it back lists it again
    /// and a later pick of it is honoured. With no camera left the toggle turns off.
    #[test]
    fn cam1_unplug_and_replug_while_the_sheet_is_open() {
        let mut m = sheet();
        m.camera = true;
        m.camera_device = Some("cam-b".into());
        let mut unplugged = devices_json();
        unplugged["cameras"] = json!([
            {"id": "cam-a", "name": "FaceTime HD Camera", "is_default": true},
        ]);
        m.set_devices(Devices::parse(&unplugged));
        assert_eq!(m.camera_device, None, "falls back to the system default");
        assert!(m.camera_fell_back && m.camera);
        assert_eq!(m.devices.cameras.len(), 1);

        m.set_devices(Devices::parse(&devices_json()));
        assert_eq!(m.devices.cameras.len(), 2, "the replugged camera is listed");
        m.camera_device = Some("cam-b".into());
        m.set_devices(Devices::parse(&devices_json()));
        assert_eq!(m.camera_device.as_deref(), Some("cam-b"));
        assert!(!m.camera_fell_back);

        let mut none = devices_json();
        none["cameras"] = json!([]);
        m.set_devices(Devices::parse(&none));
        assert!(!m.camera, "no camera left: the toggle turns off");
    }

    /// CAM-4.AC3: with camera access denied the sheet says so, offers System Settings, and
    /// recording still starts without the camera.
    #[test]
    fn cam4_ac3_denied_camera_does_not_block_recording() {
        let mut m = sheet();
        m.camera = true;
        let mut d = Devices::parse(&devices_json());
        d.camera_permission = Perm::Denied;
        m.set_devices(d);
        assert!(matches!(
            m.camera_note(),
            Some(Note::Problem { settings: true, .. })
        ));
        assert!(m.blocker().is_none());
        assert!(m.options().unwrap().camera.is_none());
    }

    /// CAM-4: no cameras at all disables the toggle with "No camera found".
    #[test]
    fn cam4_no_camera_found() {
        let mut m = sheet();
        m.camera = true;
        let mut j = devices_json();
        j["cameras"] = json!([]);
        m.set_devices(Devices::parse(&j));
        assert!(!m.camera && !m.camera_available());
        assert_eq!(m.camera_note(), Some(Note::Info("No camera found".into())));
    }

    #[test]
    fn microphone_and_system_audio_notes() {
        let mut m = sheet();
        m.system_audio = true;
        assert!(m.options().unwrap().system_audio);
        let mut j = devices_json();
        j["system_audio"] = json!({"supported": false, "reason": "Needs macOS 13"});
        j["microphone_permission"] = json!("denied");
        m.set_devices(Devices::parse(&j));
        assert_eq!(m.system_audio_note().as_deref(), Some("Needs macOS 13"));
        assert!(matches!(m.mic_note(), Some(Note::Problem { .. })));
        let o = m.options().unwrap();
        assert!(
            !o.system_audio && !o.mic,
            "unavailable sources are left out, not requested"
        );
    }

    /// REC-1: without Screen Recording access Start is blocked with the reason.
    #[test]
    fn screen_recording_permission_blocks_start() {
        let mut m = sheet();
        let mut j = devices_json();
        j["screen_recording_permission"] = json!("denied");
        m.set_devices(Devices::parse(&j));
        assert!(m.blocker().unwrap().contains("Screen Recording"));
    }

    #[test]
    fn a_vanished_display_falls_back_to_the_primary() {
        let mut m = sheet();
        m.display_id = Some(99);
        m.set_displays(parse_displays(&displays_json()));
        assert_eq!(m.display_id, Some(1));
        m.set_displays(Vec::new());
        assert_eq!(m.display_id, None);
        assert_eq!(m.target_summary(), "No display found");
    }

    // ---- the targets listing (REC-1 + shared error codes) ---------------------------------------

    fn fresh() -> SheetModel {
        SheetModel::new(&SheetDefaults::from_settings(&serde_json::Map::new()))
    }

    /// REC-1: while the listing runs the sheet says so; it never claims there is no display.
    #[test]
    fn rec1_loading_is_not_no_display() {
        let m = fresh();
        assert_eq!(m.targets, TargetsLoad::Loading);
        assert_eq!(m.target_summary(), "Looking for displays\u{2026}");
        assert!(m.blocker().is_some());
        assert!(m.targets_problem().is_none());
    }

    /// REC-1 + `permission_missing`: a failed listing is not an empty display list. The sheet names
    /// the permission, offers the restart hint and keeps Start off.
    #[test]
    fn rec1_permission_missing_is_not_no_display_found() {
        let mut m = fresh();
        m.apply_targets(Err((
            Some(ErrorCode::PermissionMissing),
            "Screen Recording permission is not granted.",
        )));
        assert!(matches!(
            m.targets,
            TargetsLoad::Failed {
                code: ErrorCode::PermissionMissing,
                ..
            }
        ));
        assert_ne!(m.target_summary(), "No display found");
        match m.targets_problem() {
            Some(TargetsProblem::PermissionMissing { text, hint }) => {
                assert_eq!(text, "Screen Recording permission is missing");
                assert!(hint.contains("quit and reopen CaptureMann"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            m.blocker().as_deref(),
            Some("Screen Recording permission is missing.")
        );
        assert!(m.options().is_none(), "Start has no target");
        // The same for window and region: nothing can be recorded.
        m.set_kind(TargetKind::Window);
        assert!(m.blocker().is_some());
    }

    /// Other shared error codes show the daemon's own message with Retry.
    #[test]
    fn rec1_other_errors_show_the_message() {
        for code in [ErrorCode::DaemonUnavailable, ErrorCode::Internal] {
            let mut m = fresh();
            m.apply_targets(Err((Some(code), " CaptureMann is not responding. ")));
            assert_eq!(
                m.targets_problem(),
                Some(TargetsProblem::Error {
                    message: "CaptureMann is not responding.".into()
                })
            );
            assert_ne!(m.target_summary(), "No display found");
            assert!(m.blocker().is_some());
        }
        // No code on the error: treated as internal; empty message gets a fallback.
        let mut m = fresh();
        m.apply_targets(Err((None, "")));
        assert!(matches!(
            m.targets,
            TargetsLoad::Failed {
                code: ErrorCode::Internal,
                ..
            }
        ));
        assert!(matches!(
            m.targets_problem(),
            Some(TargetsProblem::Error { message }) if message.contains("could not be listed")
        ));
    }

    /// "No display found" is reserved for a successful listing with zero displays; Retry and a later
    /// good listing recover.
    #[test]
    fn rec1_no_display_found_only_for_an_empty_success() {
        let mut m = fresh();
        m.apply_targets(Ok(&json!({"displays": []})));
        assert_eq!(m.targets, TargetsLoad::Loaded);
        assert_eq!(m.target_summary(), "No display found");
        assert_eq!(m.blocker().as_deref(), Some("No display was found."));
        assert!(m.targets_problem().is_none());

        m.apply_targets(Err((Some(ErrorCode::PermissionMissing), "x")));
        m.set_targets_loading();
        assert_eq!(m.targets, TargetsLoad::Loading);
        m.apply_targets(Ok(&displays_json()));
        assert_eq!(m.display_id, Some(1));
        assert!(m.blocker().is_none() && m.targets_problem().is_none());
    }

    /// The failure note makes the body taller; the footer is still always visible.
    #[test]
    fn rec5_failure_note_is_part_of_the_layout() {
        let ok = sheet();
        let mut bad = sheet();
        bad.apply_targets(Err((Some(ErrorCode::PermissionMissing), "x")));
        assert!(bad.heights().body > ok.heights().body + 40.0);
        let h = bad.heights();
        for visible in [300.0, 600.0, 1200.0] {
            assert!(split(&h, fit_height(&h, visible)).footer_visible);
        }
    }

    // ---- layout (REC-5 start sheet: Cancel and Start are always visible) ----------------------

    fn blocked_devices() -> Devices {
        let mut v = devices_json();
        v["camera_permission"] = json!("denied");
        v["microphone_permission"] = json!("denied");
        v["screen_recording_permission"] = json!("denied");
        v["system_audio"] =
            json!({"supported": false, "reason": "System audio needs macOS 13 or later"});
        Devices::parse(&v)
    }

    fn tallest() -> SheetModel {
        let mut m = sheet();
        m.camera = true;
        m.set_devices(blocked_devices());
        m
    }

    #[test]
    fn rec5_sheet_height_per_target_mode() {
        // The bug: Window and Region showed a "Choose ..." button and the reason Start is off
        // (one more footer line), which the fixed 560 pt window did not have room for.
        let mut m = sheet();
        m.camera = false;
        m.set_kind(TargetKind::Display);
        let display = m.heights();
        m.set_kind(TargetKind::Window);
        let window = m.heights();
        m.set_kind(TargetKind::Region);
        let region = m.heights();
        // Display: summary line + one chip per display (two displays here), no reason line.
        // Window and Region: the choose button instead, plus the footer reason.
        assert!(
            window.footer > display.footer,
            "the reason line is in the footer"
        );
        // R20: the Window kind has the window list (a fixed-height block) and one Include row
        // instead of the target row and the three source rows; Region keeps the Choose row.
        use crate::recording::window_rows::{INCLUDE_ROW_H, LIST_BLOCK_H};
        let list_and_chips = row::SECTION_GAP
            + f64::from(LIST_BLOCK_H)
            + row::SECTION_GAP
            + f64::from(INCLUDE_ROW_H);
        let source_card =
            row::SECTION_GAP + row::CARD_BORDER + 3.0 * row::ROW + 2.0 * row::ROW_DIVIDER;
        let display_target = row::SECTION_GAP + row::CHIPS;
        assert_eq!(
            window.body - display.body,
            list_and_chips - display_target - source_card,
            "the list and the chips replace the chips and the three rows"
        );
        assert_eq!(
            region.body - display.body,
            row::BUTTON - row::CHIPS,
            "no chips, the Choose row instead"
        );
        assert_eq!(window.footer, region.footer);
        assert!(window.total() > 0.0 && display.total() > 0.0);
        // A choice made through the picker clears the reason again.
        m.pick(Target::Display { display_id: 1 });
        assert_eq!(m.heights().footer, display.footer);
    }

    #[test]
    fn rec5_height_grows_with_the_rows_shown() {
        let mut m = sheet();
        m.camera = false;
        let base = m.heights().body;
        m.camera = true;
        // Two cameras: a chip row appears under the webcam toggle.
        assert!(m.heights().body > base + 30.0);
        let calm = m.heights().body;
        m.set_devices(blocked_devices());
        // A permission problem adds its text and an "Open System Settings" button, twice.
        assert!(m.heights().body > calm + 2.0 * 60.0);
        // Longer text wraps to more lines.
        let one = lines("short");
        let many = lines(&"x".repeat(row::CHARS_PER_LINE * 3 + 1));
        assert_eq!((one, many), (1.0, 4.0));
    }

    /// Redesign R9: 600 wide; the sheet's row texts and the problem states are all still modelled.
    #[test]
    fn r9_sheet_is_600_wide_and_keeps_every_problem_state() {
        assert_eq!(SHEET_WIDTH, 600.0);
        assert_eq!(
            TargetKind::ALL.map(kind_icon),
            ["monitor", "app-window", "cm-capture-region"]
        );
        assert_eq!(SOURCE_ICONS, ["cm-webcam-bubble", "mic", "volume-2"]);
        let t = tallest();
        // Camera, microphone and system audio each keep their problem text; the screen problem
        // keeps the blocker line in the footer.
        assert!(matches!(
            t.camera_note(),
            Some(Note::Problem { settings: true, .. })
        ));
        assert!(matches!(
            t.mic_note(),
            Some(Note::Problem { settings: true, .. })
        ));
        assert!(t.system_audio_note().is_some());
        assert!(t.blocker().is_some());
        assert_eq!(t.system_hint(), "System audio needs macOS 13 or later");
        // The calm state has the board's texts.
        let mut m = sheet();
        assert_eq!(m.mic_hint(), "MacBook Microphone");
        assert_eq!(m.system_hint(), "Sounds from apps, without your own");
        assert_eq!(m.camera_hint(), "A circle with your face on the recording.");
        m.camera = true;
        assert!(m.camera_hint().starts_with("Live preview on screen."));
        // The single-display Display state has a summary line, no chips.
        let mut one = sheet();
        one.set_displays(parse_displays(
            &json!({"displays": [displays_json()["displays"][1].clone()]}),
        ));
        assert_eq!(
            one.heights().body - sheet().heights().body,
            row::SUMMARY - row::CHIPS
        );
        // A long permission note grows the sheet.
        assert!(tallest().heights().body > sheet().heights().body + 100.0);
    }

    #[test]
    fn rec5_window_fits_the_content_when_the_display_is_tall_enough() {
        let h = tallest().heights();
        let win = fit_height(&h, 1200.0);
        assert!(
            (win - h.total()).abs() < 0.001,
            "exactly the content: {win}"
        );
        assert!(!split(&h, win).body_scrolls);
    }

    #[test]
    fn rec5_footer_is_always_visible_even_on_a_short_display() {
        let h = tallest().heights();
        for visible in [300.0, 480.0, 600.0, 700.0, 900.0, 1400.0] {
            let win = fit_height(&h, visible);
            let l = split(&h, win);
            assert!(l.footer_visible, "visible {visible}: footer cut off");
            assert!(
                win <= (visible - 2.0 * SHEET_MARGIN).max(1.0) + 0.001,
                "visible {visible}: window {win} does not fit"
            );
            assert!(l.body_viewport >= 0.0);
            if win < h.total() {
                assert!(
                    l.body_scrolls,
                    "visible {visible}: content cut with no scroll"
                );
            }
        }
        // 700 pt of screen: the body gets what is left after header and footer and scrolls.
        let l = split(&h, fit_height(&h, 700.0));
        assert!(l.body_scrolls && l.body_viewport > 150.0);
    }

    #[test]
    fn rec5_window_is_never_smaller_than_the_minimum_unless_the_display_is() {
        let h = SheetHeights {
            header: 10.0,
            body: 10.0,
            footer: 10.0,
        };
        assert_eq!(fit_height(&h, 2000.0), SHEET_MIN_HEIGHT);
        assert_eq!(fit_height(&h, 300.0), 300.0 - 2.0 * SHEET_MARGIN);
    }

    #[test]
    fn rec5_clamp_into_keeps_the_window_on_the_visible_part() {
        let visible = Rect::new(0.0, 38.0, 1512.0, 900.0);
        // Off the bottom: moved up so the margin is kept.
        let r = clamp_into(
            Rect::new(476.0, 700.0, 560.0, 400.0),
            &visible,
            SHEET_MARGIN,
        );
        assert_eq!((r.x, r.y), (476.0, 938.0 - SHEET_MARGIN - 400.0));
        // Above the menu bar: moved down to the margin.
        let r = clamp_into(Rect::new(476.0, 0.0, 560.0, 400.0), &visible, SHEET_MARGIN);
        assert_eq!(r.y, 38.0 + SHEET_MARGIN);
        // Too wide or too tall: shrunk.
        let r = clamp_into(Rect::new(0.0, 0.0, 2000.0, 2000.0), &visible, SHEET_MARGIN);
        assert_eq!((r.w, r.h), (1512.0 - 48.0, 900.0 - 48.0));
        // A secondary display left of and above the primary keeps its negative origin.
        let second = Rect::new(-221.0, -1050.0, 1920.0, 1050.0);
        let r = clamp_into(
            Rect::new(300.0, -300.0, 560.0, 700.0),
            &second,
            SHEET_MARGIN,
        );
        assert!(r.x >= second.x + SHEET_MARGIN && r.right() <= second.right() - SHEET_MARGIN);
        assert!(r.y >= second.y + SHEET_MARGIN && r.bottom() <= second.bottom() - SHEET_MARGIN);
    }

    #[test]
    fn rec5_initial_frame_is_centered_in_the_top_third_and_inside_the_margins() {
        let visible = Rect::new(-221.0, -1050.0, 1920.0, 1050.0);
        let r = initial_frame(&visible, SHEET_WIDTH, 700.0);
        assert_eq!(r.x, -221.0 + (1920.0 - SHEET_WIDTH) / 2.0);
        assert_eq!(r.y, -1050.0 + (1050.0 - 700.0) / 3.0);
        // Taller than the display allows: starts at the top margin.
        let tall = initial_frame(&Rect::new(0.0, 38.0, 1512.0, 500.0), SHEET_WIDTH, 900.0);
        assert_eq!(tall.y, 38.0 + SHEET_MARGIN);
        assert_eq!(tall.h, 500.0 - 2.0 * SHEET_MARGIN);
    }

    // ---- R20: the Window kind --------------------------------------------------------------

    /// Start stays off, with the existing footer reason, until a window is chosen; choosing one
    /// in the list (a `Target::Window`) turns it on and the request carries that window.
    #[test]
    fn r20_start_needs_a_chosen_window() {
        let mut m = sheet();
        m.set_kind(TargetKind::Window);
        assert_eq!(m.blocker().as_deref(), Some("Choose a window to record."));
        assert!(m.options().is_none());
        assert_eq!(m.target_summary(), "Choose a window");
        m.pick(Target::Window { window_id: 42 });
        assert!(m.blocker().is_none());
        assert_eq!(
            m.options().unwrap().target,
            Target::Window { window_id: 42 }
        );
        // A pick of a window on the Display kind switches the kind, as before.
        let mut d = sheet();
        d.pick(Target::Window { window_id: 7 });
        assert_eq!(d.kind, TargetKind::Window);
        // Switching away drops the window.
        d.set_kind(TargetKind::Display);
        assert_eq!(d.picked, None);
    }

    /// The chips carry the same state as the switch rows and the same limits.
    #[test]
    fn r20_chips_follow_the_sources() {
        let mut m = sheet();
        m.camera = false;
        m.mic = true;
        m.system_audio = false;
        let chips = m.include_chips();
        assert_eq!(chips.clone().map(|c| c.on), [false, true, false]);
        assert!(chips.iter().all(|c| c.enabled));
        assert_eq!(
            chips.clone().map(|c| c.label),
            ["Webcam bubble", "Microphone", "System audio"]
        );
        assert_eq!(chips.map(|c| c.icon), SOURCE_ICONS);
        // Toggling flips the same fields the switches flip, and what Start sends follows.
        assert!(m.toggle_source(Source::Webcam));
        assert!(m.toggle_source(Source::SystemAudio));
        assert!(m.toggle_source(Source::Microphone));
        assert_eq!(m.include_chips().map(|c| c.on), [true, false, true]);
        m.pick(Target::Window { window_id: 1 });
        let o = m.options().unwrap();
        assert!(o.camera.is_some() && !o.mic && o.system_audio);
    }

    #[test]
    fn r20_unavailable_sources_are_disabled_chips() {
        let mut m = sheet();
        m.camera = true;
        m.system_audio = true;
        let mut v = devices_json();
        v["cameras"] = json!([]);
        v["system_audio"] =
            json!({"supported": false, "reason": "System audio needs macOS 13 or later"});
        v["microphone_permission"] = json!("denied");
        m.set_devices(Devices::parse(&v));
        let chips = m.include_chips();
        // No camera: off and disabled. No system audio: off and disabled. Mic blocked: off.
        assert_eq!(
            chips.clone().map(|c| (c.on, c.enabled)),
            [(false, false), (false, true), (false, false)]
        );
        let before = m.clone();
        assert!(!m.toggle_source(Source::Webcam));
        assert!(!m.toggle_source(Source::SystemAudio));
        assert_eq!(m, before, "a disabled chip changes nothing");
    }

    /// Notes under the chips grow the sheet, the list block is fixed and the footer stays whole.
    #[test]
    fn r20_window_layout_is_fixed_list_plus_notes() {
        let mut m = sheet();
        m.set_kind(TargetKind::Window);
        m.camera = false;
        let calm = m.heights();
        m.camera = true;
        // Two cameras: the camera chips appear under the Include row.
        assert!(m.heights().body > calm.body + 30.0);
        let tall = {
            let mut t = tallest();
            t.set_kind(TargetKind::Window);
            t
        };
        assert!(tall.heights().body > calm.body + 100.0);
        for visible in [300.0, 600.0, 900.0, 1400.0] {
            let h = tall.heights();
            assert!(split(&h, fit_height(&h, visible)).footer_visible);
        }
        // The list area is a fixed height, so the body does not depend on how many windows
        // there are; choosing one changes the body not at all.
        let body = m.heights().body;
        m.pick(Target::Window { window_id: 3 });
        assert_eq!(m.heights().body, body);
    }

    /// I18N P2: the model's texts are produced when asked, so they follow the language.
    #[test]
    fn i18n_model_texts_follow_the_language() {
        use cm_i18n::{with_language, Lang};
        let mut m = sheet();
        m.set_kind(TargetKind::Region);
        assert_eq!(m.blocker().as_deref(), Some("Choose a region to record."));
        let pseudo = with_language(Lang::EnXa, || m.blocker().unwrap());
        assert_ne!(pseudo, "Choose a region to record.");
        let sel = sheet();
        let summary = with_language(Lang::EnXa, || sel.target_summary());
        assert!(
            summary.contains("3024") && summary.contains("1964"),
            "{summary}"
        );
    }
}
