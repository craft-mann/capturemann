//! Settings -> Webcam & audio, boards R13 and R13b: the models behind the Camera and Microphone
//! dropdowns, the rules that start and stop a check (`CheckSession`, shared by the camera and the
//! microphone) and the states of the Camera check card. Everything here is pure (no window, no
//! device), so the rules are tested with fakes.
//!
//! # The two settings
//!
//! `camera.device` and `audio.mic_device` hold a device id. A missing key, `null` or an empty
//! string means the system default. They are the defaults wherever a device is chosen and none is
//! named: the Record sheet's first choice, a hotkey start, and the daemon for `start_recording`
//! (`camera: on`, `mic: default`). A device named by the request or picked in the sheet for one
//! recording wins. A saved device that is not connected is replaced by the system default for that
//! recording and the saved id is kept, so the device returns when it is plugged in again.

use cm_i18n::{tr, tr_ctx};
use serde_json::{json, Value};

use crate::recording::sheet::DeviceChoice;

// ---- the dropdowns ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Camera,
    Mic,
}

/// A small tag next to a device name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// The system's own default device.
    Default,
    /// The saved device is not plugged in.
    NotConnected,
}

impl Hint {
    pub fn label(self) -> String {
        match self {
            Hint::Default => tr_ctx!("device hint", "Default"),
            Hint::NotConnected => tr_ctx!("device hint", "Not connected"),
        }
    }
}

/// One line of an open dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceOption {
    /// What choosing it saves: `None` is the system default (the key is set to `null`).
    pub value: Option<String>,
    pub name: String,
    pub hint: Option<Hint>,
    pub chosen: bool,
}

/// The dropdown of one row: its trigger and its options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropdown {
    pub options: Vec<DeviceOption>,
    /// The name shown on the closed dropdown.
    pub trigger: String,
    pub trigger_hint: Option<Hint>,
    /// The saved device is not connected: the note line under the row shows.
    pub fallback_note: bool,
}

fn system_default_name() -> String {
    tr_ctx!("device", "System default")
}

fn saved_device_name(kind: DeviceKind) -> String {
    match kind {
        DeviceKind::Camera => tr_ctx!("device", "Saved camera"),
        DeviceKind::Mic => tr_ctx!("device", "Saved microphone"),
    }
}

/// The note under a row whose saved device is not connected.
pub fn fallback_note_text() -> String {
    tr!("Using the system default until it is plugged in.")
}

impl Dropdown {
    /// `devices` is what `list_devices` reported (`loaded`: it has been reported at all);
    /// `saved` is the setting; `known_name` is the name this device had when it was last listed in
    /// this session, which is all that is known of a device that is gone.
    pub fn build(
        kind: DeviceKind,
        devices: &[DeviceChoice],
        loaded: bool,
        saved: Option<&str>,
        known_name: Option<&str>,
    ) -> Dropdown {
        let saved = saved.filter(|s| !s.is_empty());
        let connected = saved.is_some_and(|s| devices.iter().any(|d| d.id == s));
        let missing = loaded && saved.is_some() && !connected;
        let default_name = devices
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.name.clone());
        let mut options = vec![DeviceOption {
            value: None,
            hint: default_name.is_some().then_some(Hint::Default),
            name: default_name.unwrap_or_else(system_default_name),
            chosen: saved.is_none(),
        }];
        for d in devices {
            options.push(DeviceOption {
                value: Some(d.id.clone()),
                name: d.name.clone(),
                hint: None,
                chosen: saved == Some(d.id.as_str()),
            });
        }
        if missing {
            options.push(DeviceOption {
                value: saved.map(str::to_owned),
                name: known_name.map_or_else(|| saved_device_name(kind), str::to_owned),
                hint: Some(Hint::NotConnected),
                chosen: true,
            });
        }
        // Before the first listing the saved device is not known to be missing: its name is shown
        // as far as it is known, without a claim.
        let (trigger, trigger_hint) = match options.iter().find(|o| o.chosen) {
            Some(o) => (o.name.clone(), o.hint.filter(|h| *h == Hint::NotConnected)),
            None => (
                known_name.map_or_else(|| saved_device_name(kind), str::to_owned),
                None,
            ),
        };
        Dropdown {
            options,
            trigger,
            trigger_hint,
            fallback_note: missing,
        }
    }
}

/// The value `set_settings` gets for a choice: `null` for the system default.
pub fn setting_value(value: &Option<String>) -> Value {
    match value {
        Some(id) => json!(id),
        None => Value::Null,
    }
}

/// The saved device id of a settings snapshot (`None`: unset, null or empty).
pub fn saved_device(settings: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
}

/// The device a live preview opens: the saved camera when it is connected, else the system
/// default (`None`).
pub fn live_device(devices: &[DeviceChoice], saved: Option<&str>) -> Option<String> {
    saved
        .filter(|s| devices.iter().any(|d| d.id == *s))
        .map(str::to_owned)
}

// ---- the check session: when a device may be on ----------------------------------------------

/// What decides whether the camera or the microphone of the check may be on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// The permission (camera or microphone) is granted.
    pub permission: bool,
    /// The Webcam & audio page is the page on screen.
    pub on_page: bool,
    /// The app's window is in front (not hidden, not deactivated).
    pub window_active: bool,
    /// A recording (or its countdown, or the Record sheet's own bubble) owns the devices.
    pub recording: bool,
}

/// What the page has to do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Open the device and start the stream.
    Start,
    /// Close the device (the camera light and the orange microphone dot go out).
    Stop,
    /// Go to CaptureMann's own Settings -> Permissions page. Nothing is asked on the way.
    OpenPermissions,
}

/// The user's wish for one check (the camera, or the microphone) and the rules that end it.
///
/// A check is off until the user presses Turn on camera / Test microphone. It is never saved. It
/// ends when the user turns it off, leaves the page, the window is not in front, or the
/// permission goes away. A recording does not end the wish: the check pauses and resumes when the
/// recording is over (the window must then still be in front).
#[derive(Debug, Clone, Default)]
pub struct CheckSession {
    requested: bool,
    /// The user pressed the start button and the permission was missing: the "No permission"
    /// block shows.
    no_permission: bool,
    /// The user pressed "Allow access": when they come back to the page with the permission
    /// granted, the check starts by itself.
    resume_after_permission: bool,
    was_on_page: bool,
}

impl CheckSession {
    pub fn requested(&self) -> bool {
        self.requested
    }

    pub fn shows_no_permission(&self) -> bool {
        self.no_permission
    }

    /// Turn on camera / Test microphone.
    pub fn press_start(&mut self, f: Facts) {
        if f.permission {
            self.requested = true;
            self.no_permission = false;
        } else {
            self.requested = false;
            self.no_permission = true;
        }
    }

    /// Turn off camera / Turn off microphone.
    pub fn press_stop(&mut self) {
        self.requested = false;
        self.no_permission = false;
    }

    /// "Allow access": the page of the Permissions list opens; the request itself happens there.
    pub fn press_allow_access(&mut self) -> Effect {
        self.resume_after_permission = true;
        Effect::OpenPermissions
    }

    /// The device could not be opened: the check is off again (the page shows why).
    pub fn start_failed(&mut self, permission_missing: bool) {
        self.requested = false;
        self.no_permission = permission_missing;
    }

    /// Follows the page, the window, the permission and the recording. `running`: a stream is on
    /// or starting. Called by everything that can change one of the facts; safe to call any time.
    pub fn sync(&mut self, f: Facts, running: bool) -> Option<Effect> {
        let entering = f.on_page && !self.was_on_page;
        self.was_on_page = f.on_page;
        if entering {
            if self.resume_after_permission && f.permission {
                self.requested = true;
                self.no_permission = false;
            }
            self.resume_after_permission = false;
        }
        if !f.on_page {
            self.requested = false;
            self.no_permission = false;
        } else if self.no_permission && f.permission {
            // The user pressed the start button, then granted the permission: go on.
            self.requested = true;
            self.no_permission = false;
        } else if self.requested && !f.permission {
            self.requested = false;
            self.no_permission = true;
        }
        if !f.window_active && !f.recording {
            self.requested = false;
        }
        let should_run = self.should_run(f);
        match (should_run, running) {
            (true, false) => Some(Effect::Start),
            (false, true) => Some(Effect::Stop),
            _ => None,
        }
    }

    /// The device may be on (and its indicator with it).
    pub fn should_run(&self, f: Facts) -> bool {
        self.requested && f.permission && f.on_page && f.window_active && !f.recording
    }

    /// The check is wanted but a recording holds the devices: a plain note shows.
    pub fn paused_by_recording(&self, f: Facts) -> bool {
        self.requested && f.recording && f.permission && f.on_page
    }
}

// ---- the camera card -------------------------------------------------------------------------

/// Why the camera did not open or stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CamFailure {
    /// Another app has it.
    InUse,
    /// No camera, or the one in use was unplugged.
    NotFound,
    /// Anything else (the platform's message).
    Other(String),
}

/// What an error from the preview start means for the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    NoPermission,
    Failure(CamFailure),
}

pub fn classify_start_error(code: cm_core::ErrorCode, message: &str) -> StartError {
    use cm_core::ErrorCode;
    match code {
        ErrorCode::PermissionMissing => StartError::NoPermission,
        ErrorCode::DeviceBusy => StartError::Failure(CamFailure::InUse),
        ErrorCode::NotFound => StartError::Failure(CamFailure::NotFound),
        _ => StartError::Failure(CamFailure::Other(message.to_owned())),
    }
}

/// What the Camera check card shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraCard {
    /// The default: "Camera is off", Turn on camera.
    Off,
    /// Asked, no frame yet.
    Starting,
    /// Frames are arriving.
    Live,
    /// A recording holds the camera.
    Paused,
    NoPermission,
    InUse,
    NotFound,
    Other(String),
}

pub fn camera_card(
    session: &CheckSession,
    f: Facts,
    failure: Option<&CamFailure>,
    frames: bool,
) -> CameraCard {
    if session.shows_no_permission() {
        return CameraCard::NoPermission;
    }
    if let Some(failure) = failure {
        return match failure {
            CamFailure::InUse => CameraCard::InUse,
            CamFailure::NotFound => CameraCard::NotFound,
            CamFailure::Other(m) => CameraCard::Other(m.clone()),
        };
    }
    if session.paused_by_recording(f) {
        return CameraCard::Paused;
    }
    match (session.requested(), frames) {
        (true, true) => CameraCard::Live,
        (true, false) => CameraCard::Starting,
        (false, _) => CameraCard::Off,
    }
}

/// The mono readout of the Live chip: `1920 × 1080 · 30 fps`. Each part shows only when known.
pub fn readout(size: Option<(u32, u32)>, fps: Option<u32>) -> Option<String> {
    match (size, fps) {
        (Some((w, h)), Some(fps)) => Some(format!("{w} \u{00D7} {h} \u{00B7} {fps} fps")),
        (Some((w, h)), None) => Some(format!("{w} \u{00D7} {h}")),
        (None, Some(fps)) => Some(format!("{fps} fps")),
        (None, None) => None,
    }
}

/// Frames per second measured from the frames that arrive: the count at the start of a window
/// of at least a second and the count at its end.
#[derive(Debug, Clone, Default)]
pub struct FpsMeter {
    base: Option<(u64, u64)>,
    fps: Option<u32>,
}

impl FpsMeter {
    /// The window length, ms.
    pub const WINDOW_MS: u64 = 1000;

    /// `count`: frames pushed so far; `now_ms`: any monotonic clock. Returns the latest measure.
    pub fn update(&mut self, count: u64, now_ms: u64) -> Option<u32> {
        match self.base {
            None => self.base = Some((count, now_ms)),
            Some((c0, t0)) if now_ms.saturating_sub(t0) >= Self::WINDOW_MS => {
                let dt = (now_ms - t0) as f64;
                let n = count.saturating_sub(c0) as f64;
                self.fps = Some((n * 1000.0 / dt).round() as u32);
                self.base = Some((count, now_ms));
            }
            Some(_) => {}
        }
        self.fps
    }

    pub fn reset(&mut self) {
        *self = FpsMeter::default();
    }

    pub fn fps(&self) -> Option<u32> {
        self.fps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_i18n::{with_language, Lang};

    fn dev(id: &str, name: &str, default: bool) -> DeviceChoice {
        DeviceChoice {
            id: id.into(),
            name: name.into(),
            is_default: default,
        }
    }

    fn cams() -> Vec<DeviceChoice> {
        vec![
            dev("c1", "FaceTime HD Camera", true),
            dev("c2", "Studio Display Camera", false),
        ]
    }

    #[test]
    fn nothing_saved_chooses_the_system_default_first() {
        with_language(Lang::En, || {
            let d = Dropdown::build(DeviceKind::Camera, &cams(), true, None, None);
            assert_eq!(d.options.len(), 3);
            assert_eq!(d.options[0].value, None);
            assert_eq!(d.options[0].name, "FaceTime HD Camera");
            assert_eq!(d.options[0].hint, Some(Hint::Default));
            assert!(d.options[0].chosen && !d.options[1].chosen && !d.options[2].chosen);
            assert_eq!(d.trigger, "FaceTime HD Camera");
            assert_eq!(d.trigger_hint, None);
            assert!(!d.fallback_note);
        });
    }

    #[test]
    fn a_saved_device_is_marked_and_shown() {
        let d = Dropdown::build(DeviceKind::Camera, &cams(), true, Some("c2"), None);
        assert!(!d.options[0].chosen);
        assert!(d.options[2].chosen && d.options[2].value.as_deref() == Some("c2"));
        assert_eq!(d.trigger, "Studio Display Camera");
        assert!(!d.fallback_note);
    }

    #[test]
    fn no_default_flag_reads_system_default() {
        with_language(Lang::En, || {
            let list = vec![dev("m1", "USB Mic", false)];
            let d = Dropdown::build(DeviceKind::Mic, &list, true, None, None);
            assert_eq!(d.options[0].name, "System default");
            assert_eq!(d.options[0].hint, None);
            let none = Dropdown::build(DeviceKind::Mic, &[], true, None, None);
            assert_eq!(none.options.len(), 1);
            assert_eq!(none.trigger, "System default");
        });
    }

    /// A saved device that is gone: its name with "Not connected", a note, and no device chosen
    /// but that entry; the saved id stays what a click on it would save.
    #[test]
    fn a_missing_saved_device_shows_its_name_and_the_note() {
        with_language(Lang::En, || {
            let d = Dropdown::build(
                DeviceKind::Camera,
                &cams(),
                true,
                Some("gone"),
                Some("Old Cam"),
            );
            assert!(d.fallback_note);
            assert_eq!(d.trigger, "Old Cam");
            assert_eq!(d.trigger_hint, Some(Hint::NotConnected));
            assert_eq!(d.options.iter().filter(|o| o.chosen).count(), 1);
            let last = d.options.last().unwrap();
            assert_eq!(last.value.as_deref(), Some("gone"));
            assert_eq!(last.hint.map(Hint::label).as_deref(), Some("Not connected"));
            assert!(!d.options[0].chosen, "the default entry is not marked");
            // Name unknown (a new launch): a generic word, never the raw id.
            let d = Dropdown::build(DeviceKind::Mic, &[], true, Some("gone"), None);
            assert_eq!(d.trigger, "Saved microphone");
            assert_eq!(
                fallback_note_text(),
                "Using the system default until it is plugged in."
            );
        });
    }

    /// Before the first listing nothing is claimed about the saved device.
    #[test]
    fn before_the_first_listing_no_note() {
        let d = Dropdown::build(DeviceKind::Camera, &[], false, Some("c2"), Some("Studio"));
        assert!(!d.fallback_note);
        assert_eq!(d.trigger, "Studio");
        assert_eq!(d.trigger_hint, None);
    }

    /// Hot-plug: the same saved id is "not connected" after an unplug and normal again after the
    /// plug, with the saved value untouched.
    #[test]
    fn unplugging_and_plugging_back_follows_the_list() {
        let mut list = cams();
        let d = Dropdown::build(
            DeviceKind::Camera,
            &list,
            true,
            Some("c2"),
            Some("Studio Display Camera"),
        );
        assert!(!d.fallback_note);
        list.retain(|c| c.id != "c2");
        let d = Dropdown::build(
            DeviceKind::Camera,
            &list,
            true,
            Some("c2"),
            Some("Studio Display Camera"),
        );
        assert!(d.fallback_note);
        assert_eq!(d.trigger, "Studio Display Camera");
        list.push(dev("c2", "Studio Display Camera", false));
        let d = Dropdown::build(
            DeviceKind::Camera,
            &list,
            true,
            Some("c2"),
            Some("Studio Display Camera"),
        );
        assert!(!d.fallback_note);
        assert!(d.options.iter().find(|o| o.chosen).unwrap().hint.is_none());
    }

    #[test]
    fn choices_write_null_for_the_default_and_the_id_otherwise() {
        assert_eq!(setting_value(&None), Value::Null);
        assert_eq!(setting_value(&Some("c1".into())), json!("c1"));
        let mut m = serde_json::Map::new();
        assert_eq!(saved_device(&m, "camera.device"), None);
        m.insert("camera.device".into(), Value::Null);
        assert_eq!(saved_device(&m, "camera.device"), None);
        m.insert("camera.device".into(), json!(""));
        assert_eq!(saved_device(&m, "camera.device"), None);
        m.insert("camera.device".into(), json!("c2"));
        assert_eq!(saved_device(&m, "camera.device").as_deref(), Some("c2"));
    }

    #[test]
    fn the_live_preview_opens_the_saved_camera_only_when_connected() {
        assert_eq!(live_device(&cams(), Some("c2")).as_deref(), Some("c2"));
        assert_eq!(live_device(&cams(), Some("gone")), None);
        assert_eq!(live_device(&cams(), None), None);
    }

    #[test]
    fn mic_dropdown_never_offers_off() {
        let d = Dropdown::build(DeviceKind::Mic, &[dev("m1", "Mic", true)], true, None, None);
        assert!(d
            .options
            .iter()
            .all(|o| !o.name.eq_ignore_ascii_case("off") && o.value.as_deref() != Some("off")));
    }

    // ---- the settings are the same ones as before ----

    /// R13b moves rows around; every setting keeps its key and its reading (camera device, mic
    /// device, shape, size, mirror, remember position, noise suppression).
    #[test]
    fn every_existing_setting_keeps_its_key_and_round_trips() {
        use crate::recording::geometry::{BubbleShape, BubbleSize};
        use crate::recording::start as keys;
        assert_eq!(keys::KEY_CAMERA_DEVICE, "camera.device");
        assert_eq!(keys::KEY_MIC_DEVICE, "audio.mic_device");
        assert_eq!(keys::KEY_BUBBLE_SHAPE, "camera.shape");
        assert_eq!(keys::KEY_BUBBLE_SIZE, "camera.size");
        assert_eq!(keys::KEY_BUBBLE_MIRROR, "camera.mirror");
        assert_eq!(keys::KEY_BUBBLE_REMEMBER, "camera.remember_position");
        assert_eq!(keys::KEY_NOISE, "audio.noise_suppression");

        let mut m = serde_json::Map::new();
        m.insert(keys::KEY_CAMERA_DEVICE.into(), json!("c2"));
        m.insert(keys::KEY_MIC_DEVICE.into(), json!("m1"));
        m.insert(keys::KEY_BUBBLE_SHAPE.into(), json!("rounded"));
        m.insert(keys::KEY_BUBBLE_SIZE.into(), json!("l"));
        m.insert(keys::KEY_BUBBLE_MIRROR.into(), json!(false));
        m.insert(keys::KEY_NOISE.into(), json!(false));
        assert_eq!(
            saved_device(&m, keys::KEY_CAMERA_DEVICE).as_deref(),
            Some("c2")
        );
        assert_eq!(
            saved_device(&m, keys::KEY_MIC_DEVICE).as_deref(),
            Some("m1")
        );
        let b = keys::SheetDefaults::from_settings(&m).bubble;
        assert_eq!(b.shape, BubbleShape::Rounded);
        assert_eq!(b.size, BubbleSize::L);
        assert!(!b.mirror);
        assert!(!keys::RecordingSettings::from_settings(&m).noise_suppression);
        // The sizes and shapes the segmented controls write read back as themselves.
        for sz in BubbleSize::ALL {
            m.insert(keys::KEY_BUBBLE_SIZE.into(), json!(sz.key()));
            assert_eq!(keys::SheetDefaults::from_settings(&m).bubble.size, sz);
        }
        for sh in [BubbleShape::Circle, BubbleShape::Rounded] {
            m.insert(keys::KEY_BUBBLE_SHAPE.into(), json!(sh.key()));
            assert_eq!(keys::SheetDefaults::from_settings(&m).bubble.shape, sh);
        }
    }

    // ---- the check session ----

    fn facts(permission: bool, on_page: bool, window_active: bool, recording: bool) -> Facts {
        Facts {
            permission,
            on_page,
            window_active,
            recording,
        }
    }

    const OK: Facts = Facts {
        permission: true,
        on_page: true,
        window_active: true,
        recording: false,
    };

    /// A fake device layer: counts the opens, the closes and the system prompts.
    #[derive(Default)]
    struct FakeDevice {
        open: bool,
        opens: u32,
        closes: u32,
        prompts: u32,
        permission_page_opens: u32,
    }

    impl FakeDevice {
        fn apply(&mut self, e: Option<Effect>) {
            match e {
                Some(Effect::Start) => {
                    self.open = true;
                    self.opens += 1;
                }
                Some(Effect::Stop) => {
                    self.open = false;
                    self.closes += 1;
                }
                Some(Effect::OpenPermissions) => self.permission_page_opens += 1,
                None => {}
            }
        }
    }

    /// Opening the page opens nothing and asks for nothing, however often it is drawn.
    #[test]
    fn opening_the_page_opens_no_device_and_asks_for_nothing() {
        for permission in [true, false] {
            let mut s = CheckSession::default();
            let mut d = FakeDevice::default();
            for _ in 0..5 {
                d.apply(s.sync(facts(permission, true, true, false), d.open));
            }
            assert_eq!((d.opens, d.prompts, d.permission_page_opens), (0, 0, 0));
            assert!(!s.requested() && !s.shows_no_permission());
        }
    }

    #[test]
    fn the_start_button_starts_the_device() {
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(OK);
        d.apply(s.sync(OK, d.open));
        assert!(d.open);
        assert_eq!(d.opens, 1);
        // Further syncs keep it.
        d.apply(s.sync(OK, d.open));
        assert_eq!(d.opens, 1);
        // Turn off.
        s.press_stop();
        d.apply(s.sync(OK, d.open));
        assert!(!d.open);
        assert_eq!(d.closes, 1);
    }

    /// Without the permission the button opens nothing, asks nothing and shows the block.
    #[test]
    fn the_start_button_without_permission_shows_the_block_only() {
        let f = facts(false, true, true, false);
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(f);
        d.apply(s.sync(f, d.open));
        assert_eq!((d.opens, d.prompts), (0, 0));
        assert!(s.shows_no_permission());
        // Allow access goes to the Permissions page and nowhere else.
        d.apply(Some(s.press_allow_access()));
        assert_eq!(d.permission_page_opens, 1);
        assert_eq!(d.prompts, 0);
    }

    /// Allow access, then the user comes back with the permission: the check starts by itself,
    /// once. Coming back without it starts nothing and forgets the wish.
    #[test]
    fn coming_back_with_the_permission_starts_the_check() {
        let away = facts(false, false, true, false);
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.sync(facts(false, true, true, false), d.open);
        s.press_start(facts(false, true, true, false));
        d.apply(Some(s.press_allow_access()));
        // The Permissions page is another page.
        d.apply(s.sync(away, d.open));
        assert!(!s.shows_no_permission(), "the block goes with the page");
        d.apply(s.sync(facts(true, true, true, false), d.open));
        assert!(d.open && d.opens == 1);

        // Back without the permission: nothing starts, and a later grant does not either.
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.sync(facts(false, true, true, false), d.open);
        s.press_start(facts(false, true, true, false));
        s.press_allow_access();
        s.sync(away, d.open);
        d.apply(s.sync(facts(false, true, true, false), d.open));
        s.sync(away, d.open);
        d.apply(s.sync(facts(true, true, true, false), d.open));
        assert_eq!(d.opens, 0);
    }

    /// The permission arrives while the block shows (granted in System Settings): the wish the
    /// button expressed goes on.
    #[test]
    fn a_grant_while_the_block_shows_starts_the_check() {
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(facts(false, true, true, false));
        d.apply(s.sync(facts(false, true, true, false), d.open));
        d.apply(s.sync(OK, d.open));
        assert!(d.open);
        assert!(!s.shows_no_permission());
    }

    #[test]
    fn leaving_the_page_stops_the_device_and_forgets_the_wish() {
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(OK);
        d.apply(s.sync(OK, d.open));
        d.apply(s.sync(facts(true, false, true, false), d.open));
        assert!(!d.open);
        // Back on the page: still off.
        d.apply(s.sync(OK, d.open));
        assert!(!d.open && d.opens == 1);
    }

    #[test]
    fn a_window_that_is_not_in_front_stops_the_device() {
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(OK);
        d.apply(s.sync(OK, d.open));
        d.apply(s.sync(facts(true, true, false, false), d.open));
        assert!(!d.open);
        d.apply(s.sync(OK, d.open));
        assert!(!d.open, "it is not remembered across the window going away");
    }

    #[test]
    fn losing_the_permission_stops_the_device_and_shows_the_block() {
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(OK);
        d.apply(s.sync(OK, d.open));
        d.apply(s.sync(facts(false, true, true, false), d.open));
        assert!(!d.open);
        assert!(s.shows_no_permission());
    }

    /// A recording starting closes the device and shows the plain note; when it ends, the check
    /// goes on if the window is in front. A window that is not in front then ends the wish.
    #[test]
    fn a_recording_pauses_the_check_and_it_resumes_after() {
        let rec = facts(true, true, true, true);
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(OK);
        d.apply(s.sync(OK, d.open));
        d.apply(s.sync(rec, d.open));
        assert!(!d.open);
        assert!(s.paused_by_recording(rec));
        assert!(!s.shows_no_permission());
        // Another app is in front during the recording: the wish survives it.
        let rec_away = facts(true, true, false, true);
        d.apply(s.sync(rec_away, d.open));
        assert!(s.requested());
        d.apply(s.sync(OK, d.open));
        assert!(d.open && d.opens == 2);

        // The recording ends while the window is not in front: the check does not come back.
        let mut s = CheckSession::default();
        let mut d = FakeDevice::default();
        s.press_start(OK);
        d.apply(s.sync(OK, d.open));
        d.apply(s.sync(rec_away, d.open));
        d.apply(s.sync(facts(true, true, false, false), d.open));
        d.apply(s.sync(OK, d.open));
        assert!(!d.open);
    }

    #[test]
    fn a_failed_start_turns_the_check_off() {
        let mut s = CheckSession::default();
        s.press_start(OK);
        s.start_failed(false);
        assert!(!s.requested() && !s.shows_no_permission());
        s.press_start(OK);
        s.start_failed(true);
        assert!(s.shows_no_permission());
    }

    // ---- the camera card ----

    #[test]
    fn camera_start_errors_map_to_the_blocks() {
        use cm_core::ErrorCode;
        assert_eq!(
            classify_start_error(ErrorCode::PermissionMissing, "x"),
            StartError::NoPermission
        );
        assert_eq!(
            classify_start_error(ErrorCode::DeviceBusy, "x"),
            StartError::Failure(CamFailure::InUse)
        );
        assert_eq!(
            classify_start_error(ErrorCode::NotFound, "x"),
            StartError::Failure(CamFailure::NotFound)
        );
        assert_eq!(
            classify_start_error(ErrorCode::Internal, "boom"),
            StartError::Failure(CamFailure::Other("boom".into()))
        );
    }

    /// One state per block of the spec (section 4) plus the normal ones.
    #[test]
    fn the_camera_card_shows_the_block_that_matches() {
        let idle = CheckSession::default();
        assert_eq!(camera_card(&idle, OK, None, false), CameraCard::Off);
        // Frames without a wish (a stale slot) are not Live.
        assert_eq!(camera_card(&idle, OK, None, true), CameraCard::Off);

        let mut on = CheckSession::default();
        on.press_start(OK);
        assert_eq!(camera_card(&on, OK, None, false), CameraCard::Starting);
        assert_eq!(camera_card(&on, OK, None, true), CameraCard::Live);
        assert_eq!(
            camera_card(&on, facts(true, true, true, true), None, true),
            CameraCard::Paused
        );

        let mut denied = CheckSession::default();
        let f = facts(false, true, true, false);
        denied.press_start(f);
        assert_eq!(
            camera_card(&denied, f, None, false),
            CameraCard::NoPermission
        );

        assert_eq!(
            camera_card(&idle, OK, Some(&CamFailure::InUse), false),
            CameraCard::InUse
        );
        assert_eq!(
            camera_card(&idle, OK, Some(&CamFailure::NotFound), false),
            CameraCard::NotFound
        );
        assert_eq!(
            camera_card(&idle, OK, Some(&CamFailure::Other("m".into())), false),
            CameraCard::Other("m".into())
        );
    }

    #[test]
    fn the_readout_is_resolution_and_fps() {
        assert_eq!(
            readout(Some((1920, 1080)), Some(30)).as_deref(),
            Some("1920 \u{00D7} 1080 \u{00B7} 30 fps")
        );
        assert_eq!(
            readout(Some((1280, 720)), None).as_deref(),
            Some("1280 \u{00D7} 720")
        );
        assert_eq!(readout(None, Some(24)).as_deref(), Some("24 fps"));
        assert_eq!(readout(None, None), None);
    }

    #[test]
    fn the_fps_is_measured_over_windows_of_a_second() {
        let mut m = FpsMeter::default();
        assert_eq!(m.update(0, 0), None);
        assert_eq!(m.update(10, 400), None, "less than a window");
        assert_eq!(m.update(30, 1000), Some(30));
        assert_eq!(m.update(40, 1500), Some(30), "the last full window stays");
        assert_eq!(m.update(54, 2000), Some(24));
        m.reset();
        assert_eq!(m.fps(), None);
    }
}
