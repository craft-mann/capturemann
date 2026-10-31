//! The permissions CaptureMann can use (PRM-1): what each one is for, whether it is required, its
//! live state and the action that fixes it.
//!
//! Display-free. A [`Provider`] answers "what is the state" and "ask the OS now"; the system
//! provider uses only the non-prompting checks of cm-capture, cm-audio, cm-camera and cm-cursor
//! for the first, and the OS request calls (which may open a system dialog) only from an explicit
//! user action for the second. [`FakeProvider`] backs tests and the `--demo-permissions` flags, so
//! nothing here ever has to touch the real grants.
//!
//! macOS cannot tell "never asked" from "denied" for Screen Recording and Input Monitoring without
//! prompting, so those are either granted or [`State::NotGranted`]. After Screen Recording is
//! switched on in System Settings the OS usually keeps denying capture to a running process until it
//! is relaunched; CaptureMann cannot probe that, so a process that started without the grant reports
//! [`State::GrantedNeedsRestart`] as soon as the preflight turns true, and the copy says "usually".

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use cm_capture::{CaptureBackend, DisplayInfo, Frame, WindowInfo};
use cm_core::{CmError, Permission};
use cm_i18n::{tr, tr_ctx};

/// How long a Microphone or Camera request waits for the user's answer to the system dialog.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Id {
    ScreenRecording,
    Microphone,
    Camera,
    InputMonitoring,
}

impl Id {
    /// Stable key for element ids and tests.
    pub fn key(self) -> &'static str {
        match self {
            Id::ScreenRecording => "screen-recording",
            Id::Microphone => "microphone",
            Id::Camera => "camera",
            Id::InputMonitoring => "input-monitoring",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    /// No per-app prompts here: the screen portal asks at capture time, and the microphone and
    /// camera are not gated by the OS (outside sandboxed packages).
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if std::env::consts::OS == "linux" {
            Os::Linux
        } else {
            Os::Mac
        }
    }
}

/// One permission as the user sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// The first check has not run yet.
    #[default]
    Unknown,
    Granted,
    /// Granted after this process started without it: macOS usually needs a relaunch.
    GrantedNeedsRestart,
    /// Screen Recording and Input Monitoring: not granted (never asked, denied, or dropped after a
    /// re-sign: macOS does not say which).
    NotGranted,
    /// Camera and Microphone: the system has not asked yet.
    NotDetermined,
    Denied,
    /// Blocked by policy (MDM, Screen Time).
    Restricted,
}

impl State {
    pub fn is_granted(self) -> bool {
        matches!(self, State::Granted | State::GrantedNeedsRestart)
    }

    /// The pill text.
    pub fn label(self) -> String {
        match self {
            State::Unknown => tr_ctx!("permission state", "Checking\u{2026}"),
            State::Granted => tr_ctx!("permission state", "Granted"),
            State::GrantedNeedsRestart => {
                tr_ctx!("permission state", "Granted \u{2014} restart required")
            }
            State::NotGranted => tr_ctx!("permission state", "Not granted"),
            State::NotDetermined => tr_ctx!("permission state", "Not asked yet"),
            State::Denied => tr_ctx!("permission state", "Denied"),
            State::Restricted => tr_ctx!("permission state", "Restricted by policy"),
        }
    }
}

/// What a button on a permission card does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// "Grant access\u{2026}": ask the OS now (may open a system dialog).
    Grant,
    /// "Open System Settings": the pane for this permission.
    OpenSettings,
    /// "Restart CaptureMann".
    Restart,
}

impl Action {
    pub fn label(self, os: Os) -> String {
        match (self, os) {
            (Action::Grant, _) => tr!("Grant access\u{2026}"),
            (Action::OpenSettings, Os::Mac) => tr!("Open System Settings"),
            (Action::OpenSettings, Os::Windows | Os::Linux) => tr!("Open Settings"),
            (Action::Restart, _) => tr!("Restart CaptureMann"),
        }
    }
}

/// The static facts about one permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub id: Id,
    pub name: String,
    /// One plain sentence: why CaptureMann asks.
    pub why: String,
    /// Which features need it.
    pub features: String,
    pub required: bool,
}

/// Every permission CaptureMann can use on `os`, required ones first. Accessibility is not listed:
/// the global shortcuts use Carbon hot keys and the cursor track a listen-only tap, so nothing needs
/// it. Windows has no Screen Recording prompt.
pub fn specs(os: Os) -> Vec<Spec> {
    let mut v = Vec::new();
    if os == Os::Mac {
        v.push(Spec {
            id: Id::ScreenRecording,
            name: tr_ctx!("permission", "Screen Recording"),
            // TRANSLATORS: FACT: owner review required (what the permission is for)
            why: tr!("Lets CaptureMann see your screen so it can take screenshots and record. System audio is recorded under this same permission."),
            features: tr!("Screenshots, screen recording, the capture overlay, system audio"),
            required: true,
        });
    }
    v.push(Spec {
        id: Id::Microphone,
        name: tr_ctx!("permission", "Microphone"),
        // TRANSLATORS: FACT: owner review required (what the permission is for)
        why: tr!("Records your voice when you turn the microphone on."),
        features: tr!("Recording: microphone track"),
        required: false,
    });
    v.push(Spec {
        id: Id::Camera,
        name: tr_ctx!("permission", "Camera"),
        // TRANSLATORS: FACT: owner review required (what the permission is for)
        why: tr!("Shows your webcam in the bubble and records it as its own track."),
        features: tr!("Recording: webcam bubble and camera track"),
        required: false,
    });
    if os == Os::Mac {
        v.push(Spec {
            id: Id::InputMonitoring,
            name: tr_ctx!("permission", "Input Monitoring"),
            // TRANSLATORS: FACT: owner review required (what the permission is for)
            why: tr!("Lets recordings note mouse clicks and scroll direction. Without it only the cursor position is recorded."),
            features: tr!("Recording: cursor clicks and scroll"),
            required: false,
        });
    }
    v
}

/// Shown on Windows instead of a Screen Recording row.
pub fn windows_screen_note() -> String {
    tr!("Screen capture needs no permission on Windows. Microphone and camera can be switched off under Settings \u{2192} Privacy & security.")
}

/// The deep link to the pane of `id`, or `None` when there is none on `os`.
pub fn settings_link(os: Os, id: Id) -> Option<&'static str> {
    match (os, id) {
        (Os::Mac, Id::ScreenRecording) => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        (Os::Mac, Id::Microphone) => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        }
        (Os::Mac, Id::Camera) => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Camera")
        }
        (Os::Mac, Id::InputMonitoring) => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        }
        (Os::Windows, Id::Microphone) => Some("ms-settings:privacy-microphone"),
        (Os::Windows, Id::Camera) => Some("ms-settings:privacy-webcam"),
        (Os::Windows, _) | (Os::Linux, _) => None,
    }
}

/// The buttons a card shows, primary first. Granted shows none.
pub fn actions(os: Os, id: Id, state: State) -> Vec<Action> {
    let open = settings_link(os, id).map(|_| Action::OpenSettings);
    // Whether the OS has an API that can show its dialog for this permission here.
    let can_grant = os == Os::Mac;
    match state {
        State::Granted | State::Unknown => Vec::new(),
        State::GrantedNeedsRestart => vec![Action::Restart],
        // The dialog appears once; asking again is harmless, and System Settings is the way to
        // switch it back on.
        State::NotGranted => can_grant
            .then_some(Action::Grant)
            .into_iter()
            .chain(open)
            .collect(),
        State::NotDetermined => can_grant
            .then_some(Action::Grant)
            .into_iter()
            .chain(if can_grant { None } else { open })
            .collect(),
        // The OS will not prompt again.
        State::Denied | State::Restricted => open.into_iter().collect(),
    }
}

/// Whether a card should add the "stale grant" hint. macOS keeps a grant made for an older code
/// signature (an earlier ad-hoc build): the switch in System Settings looks on, but the check says
/// not granted, and only removing the entry and adding it again fixes it. The app cannot read the
/// TCC log, so this is a heuristic: Input Monitoring is still not granted after the user already
/// pressed Grant or Open System Settings on its card in this run (`tried`). It can also fire when
/// the switch really is off; the sentence it shows is worded for that.
pub fn stale_grant_hint(os: Os, id: Id, state: State, tried: bool) -> bool {
    os == Os::Mac && id == Id::InputMonitoring && state == State::NotGranted && tried
}

/// Just-in-time asking: turning a feature on in the Record sheet should ask for `id` first when the
/// OS has never asked. A denied or missing grant is handled by the card's other actions instead.
pub fn ask_before_use(os: Os, id: Id, state: State) -> bool {
    os == Os::Mac && state == State::NotDetermined && matches!(id, Id::Microphone | Id::Camera)
}

/// All states at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub os: Os,
    pub rows: Vec<(Id, State)>,
    /// The first check has run.
    pub checked: bool,
}

impl Snapshot {
    /// Before the first check: every row `Unknown`.
    pub fn unchecked(os: Os) -> Snapshot {
        Snapshot {
            os,
            rows: specs(os).iter().map(|s| (s.id, State::Unknown)).collect(),
            checked: false,
        }
    }

    /// Builds the snapshot from raw provider answers. `screen_at_start` is whether Screen Recording
    /// was granted when this process first looked (`None` before that).
    pub fn from_raw(os: Os, raw: &[(Id, State)], screen_at_start: Option<bool>) -> Snapshot {
        let rows = specs(os)
            .iter()
            .map(|s| {
                let mut st = raw
                    .iter()
                    .find(|(id, _)| *id == s.id)
                    .map_or(State::Unknown, |(_, st)| *st);
                if s.id == Id::ScreenRecording
                    && st == State::Granted
                    && screen_at_start == Some(false)
                {
                    st = State::GrantedNeedsRestart;
                }
                (s.id, st)
            })
            .collect();
        Snapshot {
            os,
            rows,
            checked: true,
        }
    }

    pub fn state(&self, id: Id) -> State {
        self.rows
            .iter()
            .find(|(i, _)| *i == id)
            .map_or(State::Unknown, |(_, s)| *s)
    }

    /// Required permissions that are known not to work yet (restart-required counts: captures
    /// are not reliable until the relaunch).
    pub fn required_missing(&self) -> Vec<Id> {
        specs(self.os)
            .iter()
            .filter(|s| s.required)
            .filter(|s| {
                matches!(
                    self.state(s.id),
                    State::NotGranted
                        | State::NotDetermined
                        | State::Denied
                        | State::Restricted
                        | State::GrantedNeedsRestart
                )
            })
            .map(|s| s.id)
            .collect()
    }

    /// Screen Recording is known to be missing or not yet usable (the library banner).
    pub fn screen_missing(&self) -> bool {
        self.required_missing().contains(&Id::ScreenRecording)
    }

    /// The setup screen's "Continue" is enabled: every required permission works.
    pub fn can_continue(&self) -> bool {
        self.checked && self.required_missing().is_empty()
    }

    /// The restart button is the primary action.
    pub fn needs_restart(&self) -> bool {
        self.rows
            .iter()
            .any(|(_, s)| *s == State::GrantedNeedsRestart)
    }
}

impl Snapshot {
    /// Every listed permission is plainly granted (restart-required does not count).
    pub fn all_granted(&self) -> bool {
        self.checked && self.rows.iter().all(|(_, s)| *s == State::Granted)
    }

    /// Some listed permission is not granted (required or optional); false before the first read.
    pub fn any_missing(&self) -> bool {
        self.checked && !self.all_granted()
    }

    /// Optional permissions that are not granted.
    pub fn optional_missing(&self) -> Vec<Id> {
        specs(self.os)
            .iter()
            .filter(|s| !s.required && self.state(s.id) != State::Granted)
            .map(|s| s.id)
            .collect()
    }
}

/// The setup screen (a screen of the main window) is shown when a required permission is missing,
/// or on the very first launch
/// (`permissions.onboarded` unset) while at least one permission, required or optional, is not
/// granted. With everything granted it is never shown (PRM-1.AC5).
pub fn show_first_run(snapshot: &Snapshot, onboarded: bool) -> bool {
    snapshot.checked
        && (!snapshot.required_missing().is_empty() || (!onboarded && snapshot.any_missing()))
}

/// Start-up: everything granted on a launch that has not been onboarded yet is recorded silently,
/// without showing the screen.
pub fn onboard_silently(snapshot: &Snapshot, onboarded: bool) -> bool {
    !onboarded && snapshot.all_granted()
}

/// The screen's "Skip optional" button: required permissions work, optional ones are missing.
pub fn can_skip_optional(snapshot: &Snapshot) -> bool {
    snapshot.can_continue() && snapshot.any_missing()
}

/// Leaving the screen (another sidebar item, "Not now", Continue, Skip) records the onboarding unless a required
/// permission is still missing, so optional permissions never bring it back (PRM-1.AC11).
pub fn closing_marks_onboarded(snapshot: &Snapshot) -> bool {
    snapshot.checked && snapshot.required_missing().is_empty()
}

/// What the library banner says when Screen Recording is missing.
// TRANSLATORS: FACT: owner review required (permission status)
pub fn screen_missing_title() -> String {
    tr!("Screen Recording permission is missing \u{2014} captures and recording won\u{2019}t work")
}

// TRANSLATORS: FACT: owner review required (permission status)
pub fn screen_restart_title() -> String {
    tr!("Screen Recording was granted \u{2014} restart CaptureMann to use it")
}

pub fn screen_missing_hint() -> String {
    tr!("After granting, quit and reopen CaptureMann.")
}

// ---- provider ----------------------------------------------------------------------------------

/// Where states come from and where requests go.
pub trait Provider: Send + Sync {
    fn os(&self) -> Os;
    /// The current state of `id`, without any dialog. Never `GrantedNeedsRestart`: the snapshot
    /// derives that.
    fn check(&self, id: Id) -> State;
    /// Asks the OS now (may show a dialog and wait for the answer) and returns the new state. Only
    /// from an explicit user action, on a worker thread.
    fn request(&self, id: Id) -> State;
}

/// Reads every row of `provider`.
pub fn probe(provider: &dyn Provider, screen_at_start: Option<bool>) -> Snapshot {
    let os = provider.os();
    let raw: Vec<(Id, State)> = specs(os)
        .iter()
        .map(|s| (s.id, provider.check(s.id)))
        .collect();
    Snapshot::from_raw(os, &raw, screen_at_start)
}

/// A scripted provider for tests and the demo flags.
pub struct FakeProvider {
    os: Os,
    states: Mutex<HashMap<Id, State>>,
    requests: Mutex<Vec<Id>>,
}

impl FakeProvider {
    pub fn new(os: Os, states: &[(Id, State)]) -> FakeProvider {
        FakeProvider {
            os,
            states: Mutex::new(states.iter().copied().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn set(&self, id: Id, state: State) {
        if let Ok(mut s) = self.states.lock() {
            s.insert(id, state);
        }
    }

    /// The permissions `request` was called for, in order.
    pub fn requests(&self) -> Vec<Id> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

impl Provider for FakeProvider {
    fn os(&self) -> Os {
        self.os
    }

    fn check(&self, id: Id) -> State {
        self.states
            .lock()
            .ok()
            .and_then(|s| s.get(&id).copied())
            .unwrap_or(State::NotDetermined)
    }

    /// Like the OS: a never-asked permission becomes granted (the user pressed Allow); anything
    /// else keeps its state (the OS shows no new dialog).
    fn request(&self, id: Id) -> State {
        if let Ok(mut r) = self.requests.lock() {
            r.push(id);
        }
        let now = self.check(id);
        if now == State::NotDetermined {
            self.set(id, State::Granted);
            return State::Granted;
        }
        now
    }
}

// ---- the real OS ---------------------------------------------------------------------------------

/// The non-prompting checks of the capture, audio, camera and cursor crates.
pub struct SystemProvider;

pub fn from_audio(s: cm_audio::PermissionState) -> State {
    match s {
        cm_audio::PermissionState::Authorized => State::Granted,
        cm_audio::PermissionState::NotDetermined => State::NotDetermined,
        cm_audio::PermissionState::Denied => State::Denied,
        cm_audio::PermissionState::Restricted => State::Restricted,
    }
}

pub fn from_camera(p: cm_camera::CameraPermission) -> State {
    match p {
        cm_camera::CameraPermission::Authorized => State::Granted,
        cm_camera::CameraPermission::NotDetermined => State::NotDetermined,
        cm_camera::CameraPermission::Denied => State::Denied,
        cm_camera::CameraPermission::Restricted => State::Restricted,
    }
}

/// Input Monitoring is granted when the cursor preflight lists no missing permission.
pub fn from_cursor(status: &cm_cursor::CursorStatus) -> State {
    match status.missing_permission() {
        Some(Permission::InputMonitoring) => State::NotGranted,
        _ => State::Granted,
    }
}

pub fn from_flag(granted: bool) -> State {
    if granted {
        State::Granted
    } else {
        State::NotGranted
    }
}

fn screen_granted() -> bool {
    if debug_screen_missing() {
        return false;
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        cm_capture::XcapBackend::new().check_permission().is_ok()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
mod ffi {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGRequestScreenCaptureAccess() -> bool;
        pub fn CGRequestListenEventAccess() -> bool;
    }
}

impl Provider for SystemProvider {
    fn os(&self) -> Os {
        Os::current()
    }

    fn check(&self, id: Id) -> State {
        match id {
            Id::ScreenRecording => from_flag(screen_granted()),
            Id::Microphone => from_audio(cm_audio::microphone_permission()),
            Id::Camera => from_camera(cm_camera::check_camera_permission()),
            Id::InputMonitoring => from_cursor(&cm_cursor::preflight()),
        }
    }

    fn request(&self, id: Id) -> State {
        match id {
            Id::ScreenRecording => {
                #[cfg(target_os = "macos")]
                {
                    // SAFETY: argument-less CoreGraphics call; may present the system dialog.
                    let _ = unsafe { ffi::CGRequestScreenCaptureAccess() };
                }
            }
            Id::Microphone => {
                return from_audio(cm_audio::request_microphone_access(REQUEST_TIMEOUT))
            }
            Id::Camera => return from_camera(cm_camera::request_camera_access(REQUEST_TIMEOUT)),
            Id::InputMonitoring => {
                #[cfg(target_os = "macos")]
                {
                    // SAFETY: argument-less CoreGraphics call; may present the system dialog.
                    let _ = unsafe { ffi::CGRequestListenEventAccess() };
                }
            }
        }
        self.check(id)
    }
}

/// Opens the pane or settings page of `id` (macOS `open`, Windows `start`). Errors are ignored: the
/// user sees nothing happen and can open the settings by hand.
pub fn open_settings(os: Os, id: Id) {
    let Some(link) = settings_link(os, id) else {
        return;
    };
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(link).spawn();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", link])
            .spawn();
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = link;
}

/// The fake provider of `--demo-permissions <scenario>` and whether Screen Recording counts as
/// granted at process start (for the restart logic). Scenarios: `missing`, `partial`, `granted`,
/// `restart`, `windows`; anything else is `missing`.
pub fn demo_provider(scenario: &str) -> (std::sync::Arc<dyn Provider>, Option<bool>) {
    use State::*;
    let mac = |screen, mic, cam, input| {
        std::sync::Arc::new(FakeProvider::new(
            Os::Mac,
            &[
                (Id::ScreenRecording, screen),
                (Id::Microphone, mic),
                (Id::Camera, cam),
                (Id::InputMonitoring, input),
            ],
        )) as std::sync::Arc<dyn Provider>
    };
    match scenario {
        "partial" => (mac(Granted, Granted, Denied, NotGranted), Some(true)),
        "granted" => (mac(Granted, Granted, Granted, Granted), Some(true)),
        // Granted in System Settings while the process started without it.
        "restart" => (
            mac(Granted, NotDetermined, NotDetermined, NotGranted),
            Some(false),
        ),
        "windows" => (
            std::sync::Arc::new(FakeProvider::new(
                Os::Windows,
                &[(Id::Microphone, Granted), (Id::Camera, Denied)],
            )),
            None,
        ),
        _ => (
            mac(NotGranted, NotDetermined, NotDetermined, NotGranted),
            Some(false),
        ),
    }
}

// ---- debug simulation ----------------------------------------------------------------------------
//
// `CM_DEBUG_SCREEN_PERMISSION=missing` makes the daemon's capturer answer every listing with
// `permission_missing(screen)` (and the preflight report it missing), so the failure states can be
// looked at without touching the real grant. `CM_DEBUG_TARGETS_ERROR=internal|daemon_unavailable`
// makes the listing fail with that code instead. Debug builds only.

#[cfg(debug_assertions)]
fn debug_screen_missing() -> bool {
    std::env::var("CM_DEBUG_SCREEN_PERMISSION").as_deref() == Ok("missing")
}

#[cfg(not(debug_assertions))]
fn debug_screen_missing() -> bool {
    false
}

/// A capture backend whose every call fails with the same error.
struct FailingBackend(CmError);

impl CaptureBackend for FailingBackend {
    fn check_permission(&self) -> Result<(), CmError> {
        Err(self.0.clone())
    }
    fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
        Err(self.0.clone())
    }
    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        Err(self.0.clone())
    }
    fn capture_display(&self, _display_id: u32) -> Result<Frame, CmError> {
        Err(self.0.clone())
    }
    fn capture_window(&self, _window_id: u32) -> Result<Frame, CmError> {
        Err(self.0.clone())
    }
}

/// The failing capturer the debug env vars ask for, if any.
#[cfg(debug_assertions)]
pub fn debug_capturer() -> Option<cm_capture::Capturer> {
    let error = if debug_screen_missing() {
        CmError::permission_missing(
            Permission::Screen,
            // i18n-ignore: a simulated error for --debug runs
            "Screen Recording permission is not granted. Enable it in System Settings > \
             Privacy & Security > Screen Recording, then restart CaptureMann.",
        )
    } else {
        match std::env::var("CM_DEBUG_TARGETS_ERROR").as_deref() {
            // i18n-ignore: simulated errors for --debug runs
            Ok("internal") => CmError::internal("The display list could not be read (simulated)."),
            Ok("daemon_unavailable") => {
                // i18n-ignore
                CmError::daemon_unavailable("CaptureMann is not responding (simulated).")
            }
            _ => return None,
        }
    };
    Some(cm_capture::Capturer::new(FailingBackend(error)).with_own_pid(None))
}

#[cfg(not(debug_assertions))]
pub fn debug_capturer() -> Option<cm_capture::Capturer> {
    let _ = FailingBackend(CmError::internal(""));
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_MISSING: [(Id, State); 4] = [
        (Id::ScreenRecording, State::NotGranted),
        (Id::Microphone, State::NotDetermined),
        (Id::Camera, State::Denied),
        (Id::InputMonitoring, State::NotGranted),
    ];

    /// PRM-1.AC1: macOS lists Screen Recording (required) and the three optional ones; Accessibility
    /// is not listed; Windows lists only microphone and camera.
    #[test]
    fn prm1_lists_what_the_os_needs() {
        let mac = specs(Os::Mac);
        let names: Vec<_> = mac.iter().map(|s| s.name.clone()).collect();
        assert_eq!(
            names,
            [
                "Screen Recording",
                "Microphone",
                "Camera",
                "Input Monitoring"
            ]
        );
        assert_eq!(
            mac.iter()
                .filter(|s| s.required)
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [Id::ScreenRecording]
        );
        assert!(mac
            .iter()
            .all(|s| !s.why.is_empty() && !s.features.is_empty()));
        assert!(!names.iter().any(|n| n == "Accessibility"));
        let win = specs(Os::Windows);
        assert_eq!(
            win.iter().map(|s| s.id).collect::<Vec<_>>(),
            [Id::Microphone, Id::Camera]
        );
    }

    /// PRM-1.AC4: the exact System Settings deep links.
    #[test]
    fn stale_grant_hint_needs_a_tried_not_granted_input_monitoring_on_mac() {
        let on = |os, id, st, tried| stale_grant_hint(os, id, st, tried);
        assert!(on(Os::Mac, Id::InputMonitoring, State::NotGranted, true));
        assert!(!on(Os::Mac, Id::InputMonitoring, State::NotGranted, false));
        for st in [
            State::Granted,
            State::GrantedNeedsRestart,
            State::Unknown,
            State::Denied,
            State::NotDetermined,
            State::Restricted,
        ] {
            assert!(!on(Os::Mac, Id::InputMonitoring, st, true), "{st:?}");
        }
        assert!(!on(
            Os::Windows,
            Id::InputMonitoring,
            State::NotGranted,
            true
        ));
        assert!(!on(Os::Mac, Id::ScreenRecording, State::NotGranted, true));
    }

    /// Linux has no per-app prompts: no Screen Recording or Input Monitoring card, no deep links,
    /// nothing to grant, and the Windows-only note is not shown there.
    #[test]
    fn linux_lists_only_microphone_and_camera_without_links() {
        let ids: Vec<Id> = specs(Os::Linux).iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![Id::Microphone, Id::Camera]);
        for id in [
            Id::ScreenRecording,
            Id::Microphone,
            Id::Camera,
            Id::InputMonitoring,
        ] {
            assert_eq!(settings_link(Os::Linux, id), None);
            assert!(!actions(Os::Linux, id, State::Denied).contains(&Action::OpenSettings));
        }
        assert!(!stale_grant_hint(
            Os::Linux,
            Id::InputMonitoring,
            State::NotGranted,
            true
        ));
    }

    #[test]
    fn prm1_deep_links() {
        let base = "x-apple.systempreferences:com.apple.preference.security?";
        for (id, pane) in [
            (Id::ScreenRecording, "Privacy_ScreenCapture"),
            (Id::Microphone, "Privacy_Microphone"),
            (Id::Camera, "Privacy_Camera"),
            (Id::InputMonitoring, "Privacy_ListenEvent"),
        ] {
            assert_eq!(
                settings_link(Os::Mac, id),
                Some(format!("{base}{pane}").as_str())
            );
        }
        assert_eq!(
            settings_link(Os::Windows, Id::Microphone),
            Some("ms-settings:privacy-microphone")
        );
        assert_eq!(
            settings_link(Os::Windows, Id::Camera),
            Some("ms-settings:privacy-webcam")
        );
        assert_eq!(settings_link(Os::Windows, Id::ScreenRecording), None);
    }

    /// PRM-1.AC3/AC4: Grant only where the OS can still ask; Open System Settings where it cannot.
    #[test]
    fn prm1_action_matrix() {
        use Action::*;
        let a = |id, st| actions(Os::Mac, id, st);
        assert_eq!(a(Id::Microphone, State::NotDetermined), [Grant]);
        assert_eq!(a(Id::Microphone, State::Denied), [OpenSettings]);
        assert_eq!(a(Id::Camera, State::Restricted), [OpenSettings]);
        assert_eq!(
            a(Id::ScreenRecording, State::NotGranted),
            [Grant, OpenSettings]
        );
        assert_eq!(
            a(Id::InputMonitoring, State::NotGranted),
            [Grant, OpenSettings]
        );
        assert_eq!(a(Id::ScreenRecording, State::Granted), []);
        assert_eq!(
            a(Id::ScreenRecording, State::GrantedNeedsRestart),
            [Restart]
        );
        assert_eq!(a(Id::Camera, State::Unknown), []);
        // Windows: nothing to request, only the settings page.
        assert_eq!(
            actions(Os::Windows, Id::Microphone, State::Denied),
            [OpenSettings]
        );
        assert_eq!(
            actions(Os::Windows, Id::Camera, State::NotDetermined),
            [OpenSettings]
        );
    }

    /// PRM-1.AC7: asking just in time covers camera and microphone that were never asked; a
    /// denied one is not asked again.
    #[test]
    fn prm1_just_in_time() {
        assert!(ask_before_use(Os::Mac, Id::Camera, State::NotDetermined));
        assert!(ask_before_use(
            Os::Mac,
            Id::Microphone,
            State::NotDetermined
        ));
        assert!(!ask_before_use(Os::Mac, Id::Camera, State::Denied));
        assert!(!ask_before_use(Os::Mac, Id::Microphone, State::Granted));
        assert!(!ask_before_use(
            Os::Mac,
            Id::ScreenRecording,
            State::NotGranted
        ));
        assert!(!ask_before_use(
            Os::Windows,
            Id::Camera,
            State::NotDetermined
        ));
    }

    /// PRM-1.AC6: a process that started without Screen Recording reports "restart required" once
    /// the preflight turns true; one that started with it reports plain Granted.
    #[test]
    fn prm1_restart_required_logic() {
        let granted = [(Id::ScreenRecording, State::Granted)];
        let s = Snapshot::from_raw(Os::Mac, &granted, Some(false));
        assert_eq!(s.state(Id::ScreenRecording), State::GrantedNeedsRestart);
        assert!(s.needs_restart());
        assert!(
            !s.can_continue(),
            "captures are not reliable before the relaunch"
        );
        let s = Snapshot::from_raw(Os::Mac, &granted, Some(true));
        assert_eq!(s.state(Id::ScreenRecording), State::Granted);
        let s = Snapshot::from_raw(Os::Mac, &granted, None);
        assert_eq!(s.state(Id::ScreenRecording), State::Granted);
        // Still missing stays missing.
        let s = Snapshot::from_raw(
            Os::Mac,
            &[(Id::ScreenRecording, State::NotGranted)],
            Some(false),
        );
        assert_eq!(s.state(Id::ScreenRecording), State::NotGranted);
    }

    /// PRM-1.AC5: Continue needs the required permission only; optional ones never gate it.
    #[test]
    fn prm1_required_vs_optional_gating() {
        let s = Snapshot::from_raw(Os::Mac, &ALL_MISSING, Some(false));
        assert_eq!(s.required_missing(), [Id::ScreenRecording]);
        assert!(!s.can_continue() && s.screen_missing());
        let mut raw = ALL_MISSING;
        raw[0].1 = State::Granted;
        let s = Snapshot::from_raw(Os::Mac, &raw, Some(true));
        assert!(s.can_continue() && !s.screen_missing());
        assert!(!Snapshot::unchecked(Os::Mac).can_continue());
        // Windows has no required permission.
        let w = Snapshot::from_raw(Os::Windows, &[(Id::Microphone, State::Denied)], None);
        assert!(w.can_continue());
    }

    fn granted_all() -> Snapshot {
        let raw = [
            (Id::ScreenRecording, State::Granted),
            (Id::Microphone, State::Granted),
            (Id::Camera, State::Granted),
            (Id::InputMonitoring, State::Granted),
        ];
        Snapshot::from_raw(Os::Mac, &raw, Some(true))
    }

    fn only_optional_missing() -> Snapshot {
        let mut raw = ALL_MISSING;
        raw[0].1 = State::Granted;
        Snapshot::from_raw(Os::Mac, &raw, Some(true))
    }

    /// PRM-1.AC5: all granted never shows the screen (and is onboarded silently); a missing
    /// required permission always shows it; only optional ones missing show it on the first
    /// launch alone.
    #[test]
    fn prm1_first_run_rule() {
        let missing = Snapshot::from_raw(Os::Mac, &ALL_MISSING, Some(false));
        let ok = granted_all();
        let opt = only_optional_missing();
        assert!(show_first_run(&missing, true));
        assert!(show_first_run(&missing, false));
        assert!(
            !show_first_run(&ok, false),
            "all granted: no screen, even on the first launch"
        );
        assert!(!show_first_run(&ok, true));
        assert!(onboard_silently(&ok, false));
        assert!(!onboard_silently(&ok, true));
        assert!(!onboard_silently(&opt, false));
        assert!(
            show_first_run(&opt, false),
            "first launch, optional missing: screen"
        );
        assert!(can_skip_optional(&opt) && !can_skip_optional(&ok) && !can_skip_optional(&missing));
        assert!(
            !show_first_run(&opt, true),
            "onboarded: optional never reopen it"
        );
        assert!(!show_first_run(&Snapshot::unchecked(Os::Mac), false));
        // Restart required counts as a missing required permission.
        let restart = Snapshot::from_raw(
            Os::Mac,
            &[(Id::ScreenRecording, State::Granted)],
            Some(false),
        );
        assert!(show_first_run(&restart, true));
        // Windows: nothing required; all granted means no screen.
        let w = Snapshot::from_raw(
            Os::Windows,
            &[
                (Id::Microphone, State::Granted),
                (Id::Camera, State::Granted),
            ],
            None,
        );
        assert!(!show_first_run(&w, false));
    }

    /// PRM-1.AC11: the screen moves on to the library by itself when everything is granted; leaving it
    /// marks onboarded only when no required permission is missing.
    #[test]
    fn prm1_auto_close_and_dismiss() {
        let p = FakeProvider::new(Os::Mac, &ALL_MISSING);
        let s = probe(&p, Some(false));
        assert!(!s.all_granted() && !closing_marks_onboarded(&s));
        p.set(Id::ScreenRecording, State::Granted);
        let s = probe(&p, Some(true));
        assert!(
            !s.all_granted(),
            "optional ones still missing: the screen stays"
        );
        assert!(s.can_continue() && closing_marks_onboarded(&s));
        p.set(Id::Microphone, State::Granted);
        p.set(Id::Camera, State::Granted);
        p.set(Id::InputMonitoring, State::Granted);
        assert!(probe(&p, Some(true)).all_granted(), "auto-advance");
        // Restart required is not "all set".
        let r = probe(&p, Some(false));
        assert!(!r.all_granted() && !closing_marks_onboarded(&r));
    }

    /// PRM-1.AC6/AC11: the restart-required state is never stored. A fresh process that already
    /// sees the grant at its first look reads plain Granted and the screen is not shown.
    #[test]
    fn prm1_restart_flag_never_persists() {
        let p = FakeProvider::new(
            Os::Mac,
            &[
                (Id::ScreenRecording, State::Granted),
                (Id::Microphone, State::Granted),
                (Id::Camera, State::Granted),
                (Id::InputMonitoring, State::Granted),
            ],
        );
        // First look of a new process: screen_at_start is None, then fixed to Some(true).
        let first = probe(&p, None);
        assert_eq!(first.state(Id::ScreenRecording), State::Granted);
        assert!(!first.needs_restart() && !show_first_run(&first, false));
        let later = probe(&p, Some(first.state(Id::ScreenRecording).is_granted()));
        assert_eq!(later.state(Id::ScreenRecording), State::Granted);
        // The old process (started without it) is the only one that reads restart-required.
        assert!(probe(&p, Some(false)).needs_restart());
    }

    /// PRM-1.AC2/AC8: states move with the provider, and a request is made only when asked for:
    /// probing never requests.
    #[test]
    fn prm1_provider_transitions_and_never_silent() {
        let p = FakeProvider::new(Os::Mac, &ALL_MISSING);
        let s = probe(&p, Some(false));
        assert_eq!(s.state(Id::Microphone), State::NotDetermined);
        assert!(p.requests().is_empty(), "checking must not request");
        assert_eq!(p.request(Id::Microphone), State::Granted);
        assert_eq!(p.request(Id::Camera), State::Denied, "denied stays denied");
        assert_eq!(p.requests(), [Id::Microphone, Id::Camera]);
        let s = probe(&p, Some(false));
        assert_eq!(s.state(Id::Microphone), State::Granted);
        assert_eq!(s.state(Id::Camera), State::Denied);
        // Screen Recording turned on in System Settings meanwhile.
        p.set(Id::ScreenRecording, State::Granted);
        let s = probe(&p, Some(false));
        assert_eq!(s.state(Id::ScreenRecording), State::GrantedNeedsRestart);
    }

    #[test]
    fn system_mappings() {
        assert_eq!(from_flag(true), State::Granted);
        assert_eq!(from_flag(false), State::NotGranted);
        assert_eq!(
            from_audio(cm_audio::PermissionState::Restricted),
            State::Restricted
        );
        assert_eq!(
            from_camera(cm_camera::CameraPermission::NotDetermined),
            State::NotDetermined
        );
        let full = cm_cursor::CursorStatus::full(cm_cursor::MoveSource::Poll);
        assert_eq!(from_cursor(&full), State::Granted);
        let missing =
            cm_cursor::CursorStatus::permission_fallback(Permission::InputMonitoring, "no");
        assert_eq!(from_cursor(&missing), State::NotGranted);
    }

    /// The simulated backend fails the way the daemon reports a missing grant.
    #[test]
    fn failing_backend_lists_with_the_shared_code() {
        let c = cm_capture::Capturer::new(FailingBackend(CmError::permission_missing(
            Permission::Screen,
            "x",
        )));
        let e = c.list_targets_with(false).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::PermissionMissing);
    }
}
