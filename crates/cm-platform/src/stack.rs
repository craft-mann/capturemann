//! The Linux desktop stack report: what this system can and cannot do for CaptureMann.
//!
//! Linux has no single permission system. Screenshots and recording go through the desktop
//! portal (ScreenCast and Screenshot over D-Bus) and PipeWire, secrets through a Secret Service,
//! hardware encoding through VAAPI. This module asks each of them (read-only: it never unlocks a
//! keyring, never opens a portal dialog, never starts a stream) and turns the answers into one
//! plain sentence per gap.
//!
//! The split: [`Inputs`] is the raw facts, [`probe`] fills it on Linux (the only OS-specific
//! part, in `sys`), and [`evaluate`] is pure, so every sentence is tested on any system.
//! The D-Bus questions are put to `busctl` (systemd) or `gdbus` (GLib), whichever exists, so the
//! crate takes no D-Bus dependency; when neither exists the answer is "unknown", not "missing".

/// The oldest ScreenCast portal that can remember a choice (restore tokens), decision D-R5.
pub const MIN_SCREENCAST_VERSION: u32 = 4;

/// The kind of graphical session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    X11,
    Wayland,
    /// No `DISPLAY` and no `WAYLAND_DISPLAY`.
    None,
}

/// The raw answers. `None` means "could not be asked" (no tool, no bus), which is never reported
/// as a gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inputs {
    pub session: Session,
    /// The desktop name (`XDG_CURRENT_DESKTOP`), for the report only.
    pub desktop: Option<String>,
    /// `Some(true)` when the portal service answered a property read.
    pub portal_answers: Option<bool>,
    pub screencast_version: Option<u32>,
    pub screenshot_version: Option<u32>,
    pub pipewire: bool,
    pub pulseaudio: bool,
    pub secret_service: Option<bool>,
    /// The `/dev/dri/renderD*` nodes.
    pub render_nodes: Vec<String>,
}

/// What a gap is about (the desktop app maps each to translated text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    NoSession,
    NoPortal,
    ScreenCastTooOld,
    NoScreenshotPortal,
    NoPipeWire,
    NoSecretService,
    NoHardwareEncoder,
}

/// Whether the gap stops a feature or only slows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Blocks,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub kind: GapKind,
    pub severity: Severity,
    /// One plain English sentence naming what is missing, what it costs and the way out.
    pub sentence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub inputs: Inputs,
    pub gaps: Vec<Gap>,
    /// Screenshots can be taken (X11 directly, Wayland through the Screenshot portal).
    pub screenshots_ok: bool,
    /// The ScreenCast portal (v4+) and PipeWire are both there.
    pub recording_stack_ok: bool,
}

impl Report {
    /// The facts as short lines (diagnostics report, `capturemann diagnostics`).
    pub fn fact_lines(&self) -> Vec<String> {
        let i = &self.inputs;
        let yn = |b: bool| if b { "yes" } else { "no" };
        let ask = |b: Option<bool>| b.map_or("unknown", yn);
        let ver = |v: Option<u32>| v.map_or_else(|| "none".to_owned(), |v| format!("v{v}"));
        let mut out = vec![format!(
            "session {}{}",
            match i.session {
                Session::X11 => "X11",
                Session::Wayland => "Wayland",
                Session::None => "none",
            },
            i.desktop
                .as_deref()
                .map_or_else(String::new, |d| format!(" ({d})"))
        )];
        out.push(format!(
            "desktop portal answers {}, ScreenCast {}, Screenshot {}",
            ask(i.portal_answers),
            ver(i.screencast_version),
            ver(i.screenshot_version)
        ));
        out.push(format!(
            "PipeWire {}, PulseAudio socket {}",
            yn(i.pipewire),
            yn(i.pulseaudio)
        ));
        out.push(format!("Secret Service {}", ask(i.secret_service)));
        out.push(if i.render_nodes.is_empty() {
            "VAAPI render device none".to_owned()
        } else {
            format!("VAAPI render device {}", i.render_nodes.join(", "))
        });
        out
    }

    /// Every gap's sentence, in the order found.
    pub fn gap_lines(&self) -> Vec<String> {
        self.gaps.iter().map(|g| g.sentence.clone()).collect()
    }

    pub fn has(&self, kind: GapKind) -> bool {
        self.gaps.iter().any(|g| g.kind == kind)
    }
}

/// Turns the raw answers into the report (pure).
pub fn evaluate(inputs: Inputs) -> Report {
    let mut gaps = Vec::new();
    let mut gap = |kind, severity, s: &str| {
        gaps.push(Gap {
            kind,
            severity,
            sentence: s.to_owned(),
        });
    };
    let portal_down = inputs.portal_answers == Some(false);
    if inputs.session == Session::None {
        gap(
            GapKind::NoSession,
            Severity::Blocks,
            "No graphical session was found (neither DISPLAY nor WAYLAND_DISPLAY is set); \
             CaptureMann needs to run inside a desktop session.",
        );
    }
    if portal_down {
        gap(
            GapKind::NoPortal,
            Severity::Blocks,
            "The desktop portal service does not answer, so Wayland screenshots and screen \
             recording are not possible. Install xdg-desktop-portal and the portal for your \
             desktop (GNOME, KDE, wlroots).",
        );
    } else if let Some(v) = inputs
        .screencast_version
        .filter(|v| *v < MIN_SCREENCAST_VERSION)
    {
        gap(
            GapKind::ScreenCastTooOld,
            Severity::Blocks,
            &format!(
                "Screen recording needs ScreenCast portal version {MIN_SCREENCAST_VERSION} or \
                 newer (it lets CaptureMann remember your choice); this system has version {v}. \
                 Update xdg-desktop-portal."
            ),
        );
    }
    if inputs.session == Session::Wayland
        && inputs.portal_answers == Some(true)
        && inputs.screenshot_version.is_none()
    {
        gap(
            GapKind::NoScreenshotPortal,
            Severity::Blocks,
            "The desktop portal has no Screenshot interface, so screenshots are not possible on \
             Wayland. Install the portal backend for your desktop.",
        );
    }
    if !inputs.pipewire {
        gap(
            GapKind::NoPipeWire,
            Severity::Blocks,
            if inputs.pulseaudio {
                "Recording needs PipeWire; this system only has PulseAudio. Install and start \
                 PipeWire (pipewire-pulse keeps your PulseAudio programs working)."
            } else {
                "Recording needs PipeWire, and no sound server socket was found. Install and \
                 start PipeWire."
            },
        );
    }
    if inputs.secret_service == Some(false) {
        gap(
            GapKind::NoSecretService,
            Severity::Blocks,
            "No Secret Service (GNOME Keyring or KWallet) was found, so Google and YouTube \
             sign-ins and API keys cannot be stored in the system keyring. Start one, or set \
             CAPTUREMANN_SECRETS=file to keep them in a file only you can read (not encrypted).",
        );
    }
    if inputs.render_nodes.is_empty() {
        gap(
            GapKind::NoHardwareEncoder,
            Severity::Note,
            "No hardware video encoder device (/dev/dri/renderD*) was found; recording will use \
             the software VP9 encoder, which is slower and uses more of the processor.",
        );
    }
    let screenshots_ok = match inputs.session {
        Session::X11 => true,
        Session::Wayland => {
            inputs.portal_answers != Some(false)
                && !gaps.iter().any(|g| g.kind == GapKind::NoScreenshotPortal)
        }
        Session::None => false,
    };
    let recording_stack_ok = inputs.session != Session::None
        && inputs.pipewire
        && !portal_down
        && inputs
            .screencast_version
            .is_some_and(|v| v >= MIN_SCREENCAST_VERSION);
    Report {
        inputs,
        gaps,
        screenshots_ok,
        recording_stack_ok,
    }
}

/// Asks this system. `None` off Linux, where none of this applies.
pub fn probe() -> Option<Report> {
    sys::probe().map(evaluate)
}

/// The version out of `busctl get-property` (`u 5`) or `gdbus call` (`(<uint32 5>,)`) output.
pub fn parse_version(out: &str) -> Option<u32> {
    out.split(|c: char| !c.is_ascii_digit())
        .rfind(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
}

/// Whether a bus-name listing (`busctl list` or `gdbus` array output) has `name` as a name.
pub fn lists_name(out: &str, name: &str) -> bool {
    out.split(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | ',' | '[' | ']' | '(' | ')'))
        .any(|w| w == name)
}

/// Decides the session from the environment values (pure).
pub fn session_of(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> Session {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    match session_type {
        Some("wayland") if set(wayland_display) => Session::Wayland,
        Some("x11") if set(display) => Session::X11,
        _ if set(wayland_display) => Session::Wayland,
        _ if set(display) => Session::X11,
        _ => Session::None,
    }
}

#[cfg(target_os = "linux")]
mod sys {
    use super::{lists_name, parse_version, session_of, Inputs};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

    /// Runs a tool with a deadline; `None` when it is missing, fails or is too slow.
    fn run(program: &str, args: &[&str]) -> Option<String> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            match child.try_wait().ok()? {
                Some(status) => {
                    if !status.success() {
                        return None;
                    }
                    let mut s = String::new();
                    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut s).ok()?;
                    return Some(s);
                }
                None if Instant::now() >= end => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }

    fn tool_exists(program: &str) -> bool {
        std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
    }

    /// One portal interface's `version` property.
    fn portal_version(iface: &str) -> Option<u32> {
        let interface = format!("org.freedesktop.portal.{iface}");
        if let Some(out) = run(
            "busctl",
            &[
                "--user",
                "get-property",
                PORTAL,
                PORTAL_PATH,
                &interface,
                "version",
            ],
        ) {
            return parse_version(&out);
        }
        let out = run(
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                PORTAL,
                "--object-path",
                PORTAL_PATH,
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                &interface,
                "version",
            ],
        )?;
        parse_version(&out)
    }

    /// Whether `name` is on the session bus or can be started by it.
    fn bus_has(name: &str) -> Option<bool> {
        if let Some(out) = run("busctl", &["--user", "--no-pager", "--no-legend", "list"]) {
            return Some(lists_name(&out, name));
        }
        let call = |method: &str| {
            run(
                "gdbus",
                &[
                    "call",
                    "--session",
                    "--dest",
                    "org.freedesktop.DBus",
                    "--object-path",
                    "/org/freedesktop/DBus",
                    "--method",
                    method,
                ],
            )
        };
        let names = call("org.freedesktop.DBus.ListNames")?;
        let activatable = call("org.freedesktop.DBus.ListActivatableNames").unwrap_or_default();
        Some(lists_name(&names, name) || lists_name(&activatable, name))
    }

    pub fn probe() -> Option<Inputs> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let session = session_of(
            var("XDG_SESSION_TYPE").as_deref(),
            var("WAYLAND_DISPLAY").as_deref(),
            var("DISPLAY").as_deref(),
        );
        let runtime = var("XDG_RUNTIME_DIR").map(PathBuf::from);
        let socket = |rel: &str| runtime.as_ref().is_some_and(|r| r.join(rel).exists());
        let has_tool = tool_exists("busctl") || tool_exists("gdbus");
        let has_bus = var("DBUS_SESSION_BUS_ADDRESS").is_some() || socket("bus");
        let (screencast_version, screenshot_version, portal_answers, secret_service) =
            if has_tool && has_bus {
                let cast = portal_version("ScreenCast");
                let shot = portal_version("Screenshot");
                (
                    cast,
                    shot,
                    Some(cast.is_some() || shot.is_some()),
                    bus_has("org.freedesktop.secrets"),
                )
            } else {
                (None, None, None, None)
            };
        let mut render_nodes: Vec<String> = std::fs::read_dir("/dev/dri")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("renderD"))
            .map(|n| format!("/dev/dri/{n}"))
            .collect();
        render_nodes.sort();
        Some(Inputs {
            session,
            desktop: var("XDG_CURRENT_DESKTOP"),
            portal_answers,
            screencast_version,
            screenshot_version,
            pipewire: socket("pipewire-0"),
            pulseaudio: socket("pulse/native"),
            secret_service,
            render_nodes,
        })
    }
}

#[cfg(not(target_os = "linux"))]
mod sys {
    pub fn probe() -> Option<super::Inputs> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Inputs {
        Inputs {
            session: Session::Wayland,
            desktop: Some("GNOME".into()),
            portal_answers: Some(true),
            screencast_version: Some(5),
            screenshot_version: Some(2),
            pipewire: true,
            pulseaudio: true,
            secret_service: Some(true),
            render_nodes: vec!["/dev/dri/renderD128".into()],
        }
    }

    #[test]
    fn a_complete_stack_has_no_gap() {
        let r = evaluate(good());
        assert!(r.gaps.is_empty());
        assert!(r.screenshots_ok && r.recording_stack_ok);
    }

    #[test]
    fn pulseaudio_alone_says_so() {
        let mut i = good();
        i.pipewire = false;
        let r = evaluate(i);
        assert!(r.has(GapKind::NoPipeWire));
        assert!(r.gaps[0]
            .sentence
            .starts_with("Recording needs PipeWire; this system only has PulseAudio"));
        assert!(!r.recording_stack_ok);
        assert!(r.screenshots_ok);
    }

    #[test]
    fn no_sound_server_is_a_different_sentence() {
        let mut i = good();
        i.pipewire = false;
        i.pulseaudio = false;
        assert!(evaluate(i).gaps[0].sentence.contains("no sound server"));
    }

    #[test]
    fn a_missing_portal_blocks_both() {
        let mut i = good();
        i.portal_answers = Some(false);
        i.screencast_version = None;
        i.screenshot_version = None;
        let r = evaluate(i);
        assert!(r.has(GapKind::NoPortal));
        assert!(!r.screenshots_ok && !r.recording_stack_ok);
    }

    #[test]
    fn an_old_screencast_portal_is_named_with_its_version() {
        let mut i = good();
        i.screencast_version = Some(3);
        let r = evaluate(i);
        assert!(r.has(GapKind::ScreenCastTooOld));
        assert!(r.gaps[0].sentence.contains("version 3"));
        assert!(!r.recording_stack_ok);
        assert!(r.screenshots_ok);
    }

    #[test]
    fn x11_takes_screenshots_without_a_portal() {
        let mut i = good();
        i.session = Session::X11;
        i.portal_answers = Some(false);
        i.screencast_version = None;
        i.screenshot_version = None;
        assert!(evaluate(i).screenshots_ok);
    }

    #[test]
    fn wayland_needs_the_screenshot_interface() {
        let mut i = good();
        i.screenshot_version = None;
        let r = evaluate(i);
        assert!(r.has(GapKind::NoScreenshotPortal));
        assert!(!r.screenshots_ok);
    }

    #[test]
    fn unknown_answers_are_not_gaps() {
        let mut i = good();
        i.portal_answers = None;
        i.screencast_version = None;
        i.screenshot_version = None;
        i.secret_service = None;
        assert!(evaluate(i).gaps.is_empty());
    }

    #[test]
    fn no_keyring_and_no_vaapi_are_reported() {
        let mut i = good();
        i.secret_service = Some(false);
        i.render_nodes.clear();
        let r = evaluate(i);
        assert!(r.has(GapKind::NoSecretService) && r.has(GapKind::NoHardwareEncoder));
        assert!(r.gaps[0].sentence.contains("CAPTUREMANN_SECRETS=file"));
        let note = r.gaps.iter().find(|g| g.kind == GapKind::NoHardwareEncoder);
        assert_eq!(note.map(|g| g.severity), Some(Severity::Note));
    }

    #[test]
    fn no_session_blocks() {
        let mut i = good();
        i.session = Session::None;
        let r = evaluate(i);
        assert!(r.has(GapKind::NoSession));
        assert!(!r.screenshots_ok && !r.recording_stack_ok);
    }

    #[test]
    fn the_tool_outputs_parse() {
        assert_eq!(parse_version("u 5\n"), Some(5));
        assert_eq!(parse_version("(<uint32 4>,)\n"), Some(4));
        assert_eq!(parse_version("garbage"), None);
        assert!(lists_name(
            "org.freedesktop.secrets 1234 gnome-keyring",
            "org.freedesktop.secrets"
        ));
        assert!(lists_name(
            "(['org.a', 'org.freedesktop.secrets'],)",
            "org.freedesktop.secrets"
        ));
        assert!(!lists_name(
            "org.freedesktop.secrets.x",
            "org.freedesktop.secrets"
        ));
    }

    #[test]
    fn the_session_comes_from_the_environment() {
        assert_eq!(
            session_of(Some("wayland"), Some("wayland-0"), Some(":0")),
            Session::Wayland
        );
        assert_eq!(session_of(Some("x11"), None, Some(":0")), Session::X11);
        assert_eq!(session_of(None, None, Some(":99")), Session::X11);
        assert_eq!(session_of(Some("tty"), None, None), Session::None);
        assert_eq!(session_of(Some("wayland"), None, None), Session::None);
    }

    #[test]
    fn the_fact_lines_name_every_probe() {
        let l = evaluate(good()).fact_lines();
        assert_eq!(l.len(), 5);
        assert!(l[0].contains("Wayland (GNOME)") && l[1].contains("ScreenCast v5"));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn off_linux_there_is_no_report() {
        assert!(probe().is_none());
    }
}
