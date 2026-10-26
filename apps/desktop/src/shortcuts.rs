//! Global shortcut model (CAP-2): chords, defaults per platform, parsing from settings,
//! conflict checking and keycap labels. Pure logic; registration with the OS lives in
//! `platform::register_shortcuts`.

use cm_i18n::tr;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
}

impl Platform {
    pub const fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Windows
        }
    }
}

/// A modifier combination plus one key, e.g. `alt+shift+1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Command on macOS, the Windows key on Windows.
    pub meta: bool,
    /// Lowercase key name: `a`..`z`, `0`..`9`, `f1`..`f24`, `space`.
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordError {
    Empty,
    NoKey,
    TwoKeys(String, String),
    UnknownToken(String),
    /// A global shortcut needs at least one modifier (a bare key would steal typing).
    NoModifier,
}

impl fmt::Display for ChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChordError::Empty => f.write_str(&tr!("the shortcut is empty")),
            ChordError::NoKey => f.write_str(&tr!("the shortcut has no key")),
            ChordError::TwoKeys(a, b) => f.write_str(&tr!(
                "two keys in one shortcut: `{a}` and `{b}`",
                a = a,
                b = b
            )),
            ChordError::UnknownToken(t) => {
                f.write_str(&tr!("unknown key or modifier `{t}`", t = t))
            }
            ChordError::NoModifier => f.write_str(&tr!("a global shortcut needs a modifier key")),
        }
    }
}

impl std::error::Error for ChordError {}

fn is_key_name(t: &str) -> bool {
    if t == "space" {
        return true;
    }
    let mut chars = t.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_ascii_lowercase() || c.is_ascii_digit(),
        (Some('f'), Some(_)) => t[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)),
        _ => false,
    }
}

impl Chord {
    pub fn parse(s: &str) -> Result<Chord, ChordError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ChordError::Empty);
        }
        let mut c = Chord {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            key: String::new(),
        };
        for raw in s.split('+') {
            let t = raw.trim().to_ascii_lowercase();
            match t.as_str() {
                "ctrl" | "control" => c.ctrl = true,
                "alt" | "option" | "opt" => c.alt = true,
                "shift" => c.shift = true,
                "cmd" | "command" | "super" | "meta" | "win" => c.meta = true,
                t if is_key_name(t) => {
                    if c.key.is_empty() {
                        c.key = t.to_owned();
                    } else {
                        return Err(ChordError::TwoKeys(c.key, t.to_owned()));
                    }
                }
                _ => return Err(ChordError::UnknownToken(raw.trim().to_owned())),
            }
        }
        if c.key.is_empty() {
            return Err(ChordError::NoKey);
        }
        if !(c.ctrl || c.alt || c.shift || c.meta) {
            return Err(ChordError::NoModifier);
        }
        Ok(c)
    }

    /// The chord a key press makes: `key` is the pressed key without modifiers (`r`, `5`,
    /// `space`, `f6`; any case). A press with no modifier is refused, like a settings entry
    /// (CAP-2: a bare key would steal typing).
    pub fn from_pressed(
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
        key: &str,
    ) -> Result<Chord, ChordError> {
        let key = key.trim().to_ascii_lowercase();
        if !is_key_name(&key) {
            return Err(ChordError::UnknownToken(key));
        }
        if !(ctrl || alt || shift || meta) {
            return Err(ChordError::NoModifier);
        }
        Ok(Chord {
            ctrl,
            alt,
            shift,
            meta,
            key,
        })
    }

    /// Canonical settings form: `ctrl+alt+shift+meta+key`.
    pub fn to_setting(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.meta {
            parts.push("meta");
        }
        parts.push(&self.key);
        parts.join("+")
    }

    /// Keycap labels (DESIGN section 4): symbols on macOS, words on Windows.
    pub fn keycaps(&self, platform: Platform) -> Vec<String> {
        let mut out = Vec::new();
        match platform {
            Platform::Mac => {
                if self.ctrl {
                    out.push("\u{2303}".to_owned());
                }
                if self.alt {
                    out.push("\u{2325}".to_owned());
                }
                if self.shift {
                    out.push("\u{21E7}".to_owned());
                }
                if self.meta {
                    out.push("\u{2318}".to_owned());
                }
            }
            Platform::Windows => {
                if self.ctrl {
                    out.push("Ctrl".to_owned());
                }
                if self.alt {
                    out.push("Alt".to_owned());
                }
                if self.shift {
                    out.push("Shift".to_owned());
                }
                if self.meta {
                    out.push("Win".to_owned());
                }
            }
        }
        out.push(key_label(&self.key));
        out
    }
}

fn key_label(key: &str) -> String {
    if key == "space" {
        "Space".to_owned()
    } else {
        key.to_ascii_uppercase()
    }
}

/// The actions that have a global shortcut (CAP-2 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    CaptureRegion,
    CaptureWindow,
    CaptureDisplay,
    ToggleRecording,
    PauseRecording,
    OpenLibrary,
    /// The command launcher (LNC-1).
    OpenLauncher,
    /// Discard the take being recorded (asks first, like the HUD button; R4-12).
    DiscardRecording,
    /// Discard the take and start over (asks first; R4-12).
    RestartRecording,
    /// Open the newest capture of this session in the editor: the keyboard route to a toast's
    /// Annotate button (R4-12).
    AnnotateLast,
    /// Start or stop an audio-only recording (VOC-1, Option+Shift+V): starts with the last used
    /// audio sources, as Option+Shift+R does for the screen.
    ToggleAudioRecording,
    /// Add a marker to the running recording at the HUD timer (VOC-1, Option+Shift+M).
    AddMarker,
}

impl ShortcutAction {
    pub const ALL: [ShortcutAction; 12] = [
        ShortcutAction::CaptureRegion,
        ShortcutAction::CaptureWindow,
        ShortcutAction::CaptureDisplay,
        ShortcutAction::ToggleRecording,
        ShortcutAction::PauseRecording,
        ShortcutAction::OpenLibrary,
        ShortcutAction::OpenLauncher,
        ShortcutAction::DiscardRecording,
        ShortcutAction::RestartRecording,
        ShortcutAction::AnnotateLast,
        ShortcutAction::ToggleAudioRecording,
        ShortcutAction::AddMarker,
    ];

    /// Settings key (`get_settings` / `set_settings` can read and change `shortcuts.*`).
    pub fn key(self) -> &'static str {
        match self {
            Self::CaptureRegion => "shortcuts.capture_region",
            Self::CaptureWindow => "shortcuts.capture_window",
            Self::CaptureDisplay => "shortcuts.capture_display",
            Self::ToggleRecording => "shortcuts.toggle_recording",
            Self::PauseRecording => "shortcuts.pause_recording",
            Self::OpenLibrary => "shortcuts.open_library",
            Self::OpenLauncher => "shortcuts.open_launcher",
            Self::DiscardRecording => "shortcuts.discard_recording",
            Self::RestartRecording => "shortcuts.restart_recording",
            Self::AnnotateLast => "shortcuts.annotate_last",
            Self::ToggleAudioRecording => "shortcuts.toggle_audio_recording",
            Self::AddMarker => "shortcuts.add_marker",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::CaptureRegion => tr!("Capture region"),
            Self::CaptureWindow => tr!("Capture window"),
            Self::CaptureDisplay => tr!("Capture display"),
            Self::ToggleRecording => tr!("Start or stop recording"),
            Self::PauseRecording => tr!("Pause or resume recording"),
            Self::OpenLibrary => tr!("Open library"),
            Self::OpenLauncher => tr!("Open launcher"),
            Self::DiscardRecording => tr!("Discard recording"),
            Self::RestartRecording => tr!("Restart recording"),
            Self::AnnotateLast => tr!("Annotate last capture"),
            Self::ToggleAudioRecording => tr!("Start or stop audio recording"),
            Self::AddMarker => tr!("Add a marker"),
        }
    }

    /// Every shortcut is registered now that recording exists (REC-5, CAP-2).
    pub fn is_available(self) -> bool {
        true
    }

    fn default_key(self) -> &'static str {
        match self {
            Self::CaptureRegion => "1",
            Self::CaptureWindow => "2",
            Self::CaptureDisplay => "3",
            Self::ToggleRecording => "r",
            Self::PauseRecording => "p",
            Self::OpenLibrary => "l",
            Self::OpenLauncher => "space",
            Self::DiscardRecording => "d",
            Self::RestartRecording => "t",
            Self::AnnotateLast => "a",
            Self::ToggleAudioRecording => "v",
            Self::AddMarker => "m",
        }
    }

    /// CAP-2 defaults: macOS Option+Shift+key, Windows Ctrl+Alt+Shift+key.
    pub fn default_chord(self, platform: Platform) -> Chord {
        Chord {
            ctrl: platform == Platform::Windows,
            alt: true,
            shift: true,
            meta: false,
            key: self.default_key().to_owned(),
        }
    }
}

/// The effective bindings (settings over defaults).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutMap {
    bindings: Vec<(ShortcutAction, Chord)>,
    platform: Platform,
    /// Settings entries that could not be parsed (the default is used instead).
    pub invalid: Vec<(ShortcutAction, String)>,
}

/// Rebinding refused: the chord already belongs to another action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub with: ShortcutAction,
}

impl fmt::Display for Conflict {
    /// CAP-2.AC2 wording.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&tr!("Already used by {action}", action = self.with.label()))
    }
}

impl ShortcutMap {
    pub fn from_settings(settings: &Map<String, Value>, platform: Platform) -> ShortcutMap {
        let mut bindings = Vec::new();
        let mut invalid = Vec::new();
        for a in ShortcutAction::ALL {
            let chord = match settings.get(a.key()).and_then(Value::as_str) {
                Some(s) => match Chord::parse(s) {
                    Ok(c) => c,
                    Err(_) => {
                        invalid.push((a, s.to_owned()));
                        a.default_chord(platform)
                    }
                },
                None => a.default_chord(platform),
            };
            bindings.push((a, chord));
        }
        ShortcutMap {
            bindings,
            platform,
            invalid,
        }
    }

    /// The map with `action` bound to `chord` (the caller checked [`ShortcutMap::check_rebind`]).
    pub fn with(&self, action: ShortcutAction, chord: Chord) -> ShortcutMap {
        let mut next = self.clone();
        for (a, c) in &mut next.bindings {
            if *a == action {
                *c = chord.clone();
            }
        }
        next.invalid.retain(|(a, _)| *a != action);
        next
    }

    pub fn defaults(platform: Platform) -> ShortcutMap {
        ShortcutMap::from_settings(&Map::new(), platform)
    }

    pub fn get(&self, a: ShortcutAction) -> Chord {
        self.bindings
            .iter()
            .find(|(x, _)| *x == a)
            .map(|(_, c)| c.clone())
            .unwrap_or_else(|| a.default_chord(self.platform))
    }

    pub fn iter(&self) -> impl Iterator<Item = (ShortcutAction, &Chord)> {
        self.bindings.iter().map(|(a, c)| (*a, c))
    }

    /// Pairs of actions bound to the same chord (a hand-edited settings file).
    pub fn conflicts(&self) -> Vec<(ShortcutAction, ShortcutAction)> {
        let mut seen: HashMap<&Chord, ShortcutAction> = HashMap::new();
        let mut out = Vec::new();
        for (a, c) in &self.bindings {
            if let Some(first) = seen.get(c) {
                out.push((*first, *a));
            } else {
                seen.insert(c, *a);
            }
        }
        out
    }

    /// CAP-2.AC2: may `action` take `chord`?
    pub fn check_rebind(&self, action: ShortcutAction, chord: &Chord) -> Result<(), Conflict> {
        match self.iter().find(|(a, c)| *a != action && *c == chord) {
            Some((with, _)) => Err(Conflict { with }),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// CAP-2 default table.
    #[test]
    fn defaults_match_cap2_table() {
        let mac = ShortcutMap::defaults(Platform::Mac);
        assert_eq!(
            mac.get(ShortcutAction::CaptureRegion).to_setting(),
            "alt+shift+1"
        );
        assert_eq!(
            mac.get(ShortcutAction::OpenLibrary).to_setting(),
            "alt+shift+l"
        );
        let win = ShortcutMap::defaults(Platform::Windows);
        assert_eq!(
            win.get(ShortcutAction::CaptureDisplay).to_setting(),
            "ctrl+alt+shift+3"
        );
        assert!(mac.conflicts().is_empty());
        assert!(win.conflicts().is_empty());
    }

    /// CAP-2: defaults avoid the OS screenshot keys and plain Ctrl+Alt.
    #[test]
    fn defaults_avoid_os_screenshot_keys() {
        for p in [Platform::Mac, Platform::Windows] {
            for (_, c) in ShortcutMap::defaults(p).iter() {
                assert!(c.alt && c.shift, "{c:?}");
                assert!(!c.meta);
            }
        }
    }

    #[test]
    fn parse_forms() {
        let c = Chord::parse("Option + Shift + 1").unwrap();
        assert_eq!(c.to_setting(), "alt+shift+1");
        assert_eq!(
            Chord::parse("cmd+ctrl+f5").unwrap().to_setting(),
            "ctrl+meta+f5"
        );
        assert_eq!(Chord::parse(""), Err(ChordError::Empty));
        assert_eq!(Chord::parse("alt+shift"), Err(ChordError::NoKey));
        assert_eq!(Chord::parse("a"), Err(ChordError::NoModifier));
        assert!(matches!(
            Chord::parse("alt+a+b"),
            Err(ChordError::TwoKeys(..))
        ));
        assert!(matches!(
            Chord::parse("alt+f99"),
            Err(ChordError::UnknownToken(_))
        ));
        assert!(matches!(
            Chord::parse("alt+?"),
            Err(ChordError::UnknownToken(_))
        ));
    }

    /// DESIGN section 4: macOS shows symbols, Windows shows words.
    #[test]
    fn keycap_labels() {
        let c = Chord::parse("alt+shift+1").unwrap();
        assert_eq!(c.keycaps(Platform::Mac), ["\u{2325}", "\u{21E7}", "1"]);
        assert_eq!(c.keycaps(Platform::Windows), ["Alt", "Shift", "1"]);
        let w = Chord::parse("ctrl+alt+shift+r").unwrap();
        assert_eq!(w.keycaps(Platform::Windows), ["Ctrl", "Alt", "Shift", "R"]);
    }

    /// CAP-2.AC2: rebinding to a combination in use is refused with "Already used by ...".
    #[test]
    fn rebind_conflict() {
        let map = ShortcutMap::defaults(Platform::Mac);
        let taken = Chord::parse("alt+shift+2").unwrap();
        let err = map
            .check_rebind(ShortcutAction::CaptureRegion, &taken)
            .unwrap_err();
        assert_eq!(err.to_string(), "Already used by Capture window");
        assert!(map
            .check_rebind(ShortcutAction::CaptureWindow, &taken)
            .is_ok());
        assert!(map
            .check_rebind(
                ShortcutAction::CaptureRegion,
                &Chord::parse("ctrl+alt+9").unwrap()
            )
            .is_ok());
    }

    #[test]
    fn settings_override_and_invalid_fall_back() {
        let s = json!({
            "shortcuts.capture_region": "ctrl+alt+r",
            "shortcuts.capture_window": "banana",
        });
        let Value::Object(m) = s else { unreachable!() };
        let map = ShortcutMap::from_settings(&m, Platform::Mac);
        assert_eq!(
            map.get(ShortcutAction::CaptureRegion).to_setting(),
            "ctrl+alt+r"
        );
        assert_eq!(
            map.get(ShortcutAction::CaptureWindow).to_setting(),
            "alt+shift+2"
        );
        assert_eq!(map.invalid.len(), 1);
    }

    #[test]
    fn hand_edited_duplicates_are_reported() {
        let Value::Object(m) = json!({"shortcuts.capture_window": "alt+shift+1"}) else {
            unreachable!()
        };
        let map = ShortcutMap::from_settings(&m, Platform::Mac);
        assert_eq!(
            map.conflicts(),
            vec![(ShortcutAction::CaptureRegion, ShortcutAction::CaptureWindow)]
        );
    }

    /// LNC-1: the launcher hotkey defaults to Option+Shift+Space (macOS) and
    /// Ctrl+Alt+Shift+Space (Windows), conflicts with the others are refused, and the keycap
    /// shows "Space".
    #[test]
    fn launcher_default_and_conflicts() {
        let mac = ShortcutMap::defaults(Platform::Mac);
        assert_eq!(
            mac.get(ShortcutAction::OpenLauncher).to_setting(),
            "alt+shift+space"
        );
        let win = ShortcutMap::defaults(Platform::Windows);
        assert_eq!(
            win.get(ShortcutAction::OpenLauncher)
                .keycaps(Platform::Windows),
            ["Ctrl", "Alt", "Shift", "Space"]
        );
        assert!(mac.conflicts().is_empty() && win.conflicts().is_empty());
        let taken = Chord::parse("alt+shift+space").unwrap();
        let err = mac
            .check_rebind(ShortcutAction::OpenLibrary, &taken)
            .unwrap_err();
        assert_eq!(err.to_string(), "Already used by Open launcher");
        assert!(ShortcutAction::OpenLauncher.is_available());
    }

    /// CAP-2.AC2 / R4-12: a pressed combination becomes a chord; bare keys and odd keys are
    /// refused, a rebinding keeps the others and does not collide with them.
    #[test]
    fn rebinding_from_a_key_press() {
        let c = Chord::from_pressed(false, true, true, false, "R").unwrap();
        assert_eq!(c.to_setting(), "alt+shift+r");
        assert_eq!(
            Chord::from_pressed(false, false, false, false, "r"),
            Err(ChordError::NoModifier)
        );
        assert!(matches!(
            Chord::from_pressed(true, false, false, false, "escape"),
            Err(ChordError::UnknownToken(_))
        ));
        let map = ShortcutMap::defaults(Platform::Mac);
        let free = Chord::parse("ctrl+alt+9").unwrap();
        assert!(map
            .check_rebind(ShortcutAction::CaptureRegion, &free)
            .is_ok());
        let next = map.with(ShortcutAction::CaptureRegion, free.clone());
        assert_eq!(next.get(ShortcutAction::CaptureRegion), free);
        assert_eq!(
            next.get(ShortcutAction::CaptureWindow),
            map.get(ShortcutAction::CaptureWindow)
        );
        // The old chord is free again.
        let old = map.get(ShortcutAction::CaptureRegion);
        assert!(next.check_rebind(ShortcutAction::OpenLibrary, &old).is_ok());
        assert!(next.conflicts().is_empty());
    }

    /// R4-12: Discard, Restart and Annotate-last have their own shortcuts, none collides with the
    /// others or with the launcher.
    #[test]
    fn r4_12_keyboard_routes_for_the_hud_and_the_toast() {
        for p in [Platform::Mac, Platform::Windows] {
            let m = ShortcutMap::defaults(p);
            assert!(m.conflicts().is_empty());
            assert_eq!(m.get(ShortcutAction::DiscardRecording).key, "d");
            assert_eq!(m.get(ShortcutAction::RestartRecording).key, "t");
            assert_eq!(m.get(ShortcutAction::AnnotateLast).key, "a");
        }
        let mut keys: Vec<_> = ShortcutAction::ALL.iter().map(|a| a.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), ShortcutAction::ALL.len());
    }

    /// VOC-1.AC3: Option+Shift+V (Ctrl+Alt+Shift+V) starts and stops an audio recording and
    /// Option+Shift+M adds a marker; neither collides with another default, and a rebinding onto
    /// them is refused like any other.
    #[test]
    fn voc1_audio_and_marker_shortcuts() {
        for p in [Platform::Mac, Platform::Windows] {
            let m = ShortcutMap::defaults(p);
            assert!(m.conflicts().is_empty());
            assert_eq!(m.get(ShortcutAction::ToggleAudioRecording).key, "v");
            assert_eq!(m.get(ShortcutAction::AddMarker).key, "m");
        }
        let mac = ShortcutMap::defaults(Platform::Mac);
        assert_eq!(
            mac.get(ShortcutAction::ToggleAudioRecording).to_setting(),
            "alt+shift+v"
        );
        let win = ShortcutMap::defaults(Platform::Windows);
        assert_eq!(
            win.get(ShortcutAction::AddMarker)
                .keycaps(Platform::Windows),
            ["Ctrl", "Alt", "Shift", "M"]
        );
        let err = mac
            .check_rebind(
                ShortcutAction::OpenLibrary,
                &Chord::parse("alt+shift+m").unwrap(),
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "Already used by Add a marker");
        assert_eq!(
            ShortcutAction::ToggleAudioRecording.key(),
            "shortcuts.toggle_audio_recording"
        );
    }

    /// CAP-2: the recording shortcuts (⌥⇧R toggles start/stop, ⌥⇧P pauses) are registered like the
    /// capture ones.
    #[test]
    fn cap2_recording_shortcuts_are_registered() {
        for a in ShortcutAction::ALL {
            assert!(a.is_available(), "{a:?}");
        }
        let mac = ShortcutMap::defaults(Platform::Mac);
        assert_eq!(
            mac.get(ShortcutAction::ToggleRecording)
                .keycaps(Platform::Mac),
            ["\u{2325}", "\u{21E7}", "R"]
        );
        assert_eq!(
            mac.get(ShortcutAction::PauseRecording)
                .keycaps(Platform::Mac),
            ["\u{2325}", "\u{21E7}", "P"]
        );
    }
}
