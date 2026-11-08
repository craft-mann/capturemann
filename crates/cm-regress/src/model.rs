//! The case registry types: what a case is, what it needs, how it ends.

use crate::ctx::Ctx;

/// How much of the suite a run covers. Each tier includes the ones before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Under 3 minutes: version, diagnostics, daemon, targets, one screenshot, settings, MCP handshake.
    Smoke,
    /// About 15 to 20 minutes: everything that needs no sound, camera or long run.
    Standard,
    /// Adds recordings with pause, kill -9 recovery, export presets, import fixtures, sync probes.
    Full,
    /// Flag-gated hardware: sound, microphone, camera, second monitor, a long soak.
    Hardware,
}

impl Tier {
    pub fn parse(s: &str) -> Option<Tier> {
        match s {
            "smoke" => Some(Tier::Smoke),
            "standard" => Some(Tier::Standard),
            "full" => Some(Tier::Full),
            "hardware" => Some(Tier::Hardware),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Tier::Smoke => "smoke",
            Tier::Standard => "standard",
            Tier::Full => "full",
            Tier::Hardware => "hardware",
        }
    }
}

/// What a case needs from the machine or from the person who runs the suite. Everything but
/// `DesktopSession` must be allowed with `--allow` (the hardware ones record or play real signals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)] // FfmpegFull and Network are reserved for cases the macOS and Linux drivers add
pub enum Need {
    /// A logged-in desktop session (not an ssh session 0).
    DesktopSession,
    /// May capture the screen (screenshots, recordings).
    Screen,
    /// May play sound through the speakers.
    Sound,
    Camera,
    Mic,
    SecondMonitor,
    /// Speech models given with `--whisper-model`.
    Models,
    /// A full FFmpeg (with lavfi) given on PATH.
    FfmpegFull,
    Network,
    /// An installer given with `--installer`.
    Installer,
}

impl Need {
    pub fn name(self) -> &'static str {
        match self {
            Need::DesktopSession => "desktop_session",
            Need::Screen => "screen",
            Need::Sound => "sound",
            Need::Camera => "camera",
            Need::Mic => "mic",
            Need::SecondMonitor => "second_monitor",
            Need::Models => "models",
            Need::FfmpegFull => "ffmpeg_full",
            Need::Network => "network",
            Need::Installer => "installer",
        }
    }
    /// The `--allow` word that grants this need, if one is needed at all.
    pub fn allow_word(self) -> Option<&'static str> {
        match self {
            Need::DesktopSession | Need::Installer | Need::Models => None,
            other => Some(other.name()),
        }
    }
}

/// Operating systems a case can run on; the others report SKIP with the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    pub fn matches(self, name: &str) -> bool {
        matches!(
            (self, name),
            (Platform::Windows, "windows")
                | (Platform::MacOs, "macos")
                | (Platform::Linux, "linux")
        )
    }
}

pub const ALL: &[Platform] = &[Platform::Windows, Platform::MacOs, Platform::Linux];
pub const WIN: &[Platform] = &[Platform::Windows];
pub const LINUX: &[Platform] = &[Platform::Linux];
pub const WIN_LINUX: &[Platform] = &[Platform::Windows, Platform::Linux];
pub const WIN_MAC: &[Platform] = &[Platform::Windows, Platform::MacOs];
pub const MAC: &[Platform] = &[Platform::MacOs];

/// Why a case ended early.
#[derive(Debug, Clone)]
pub enum Stop {
    Fail(String),
    Skip(String),
}

pub type R<T = ()> = Result<T, Stop>;

impl From<String> for Stop {
    fn from(s: String) -> Stop {
        Stop::Fail(s)
    }
}
impl From<&str> for Stop {
    fn from(s: &str) -> Stop {
        Stop::Fail(s.to_owned())
    }
}
impl From<std::io::Error> for Stop {
    fn from(e: std::io::Error) -> Stop {
        Stop::Fail(format!("io error: {e}"))
    }
}
impl From<serde_json::Error> for Stop {
    fn from(e: serde_json::Error) -> Stop {
        Stop::Fail(format!("json error: {e}"))
    }
}

/// Ends the case with SKIP.
pub fn skip<T>(reason: impl Into<String>) -> R<T> {
    Err(Stop::Skip(reason.into()))
}

/// Ends the case with FAIL.
pub fn fail<T>(msg: impl Into<String>) -> R<T> {
    Err(Stop::Fail(msg.into()))
}

/// `check!(cond, "text {}", x)` fails the case when the condition is false.
#[macro_export]
macro_rules! check {
    ($cond:expr, $($arg:tt)+) => {{
        let ok: bool = $cond;
        if !ok {
            return Err($crate::model::Stop::Fail(format!($($arg)+)));
        }
    }};
}

/// One regression case.
pub struct Case {
    pub id: &'static str,
    pub title: &'static str,
    pub area: &'static str,
    /// Spec items, acceptance criteria and bug classes this case protects.
    pub specs: &'static [&'static str],
    pub tier: Tier,
    pub needs: &'static [Need],
    pub platforms: &'static [Platform],
    pub timeout_s: u64,
    pub run: fn(&mut Ctx) -> R,
}

/// What a case ended with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Skip,
}

impl Status {
    pub fn name(&self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
        }
    }
}

/// The configuration a run decides with.
pub struct RunEnv {
    pub tier: Tier,
    /// `--only` was given: the tier filter does not apply.
    pub only_selected: bool,
    pub allow: std::collections::BTreeSet<String>,
    pub has_desktop: bool,
    pub platform: &'static str,
    pub have_installer: bool,
    pub have_models: bool,
}

/// Why a case is not run in this configuration, if it is not.
pub fn not_runnable(case: &Case, env: &RunEnv) -> Option<String> {
    if !env.only_selected && case.tier > env.tier {
        return Some(format!(
            "tier {} is above the selected tier {}",
            case.tier.name(),
            env.tier.name()
        ));
    }
    if !case.platforms.iter().any(|p| p.matches(env.platform)) {
        return Some(format!("not implemented for {}", env.platform));
    }
    for need in case.needs {
        match need {
            Need::DesktopSession if !env.has_desktop => {
                return Some(
                    "needs a logged-in desktop session (this is session 0 or headless)".to_owned(),
                )
            }
            Need::Installer if !env.have_installer => {
                return Some("needs --installer <setup.exe>".to_owned())
            }
            Need::Models if !env.have_models => {
                return Some("needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case)".to_owned())
            }
            _ => {}
        }
        let word = need.allow_word().or(if *need == Need::Models {
            Some("models")
        } else {
            None
        });
        if let Some(word) = word {
            if !env.allow.contains(word) {
                return Some(format!("needs --allow {word}"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn dummy(ctx: &mut Ctx) -> R {
        let _ = ctx;
        Ok(())
    }

    fn case(tier: Tier, needs: &'static [Need], platforms: &'static [Platform]) -> Case {
        Case {
            id: "x.y",
            title: "t",
            area: "a",
            specs: &[],
            tier,
            needs,
            platforms,
            timeout_s: 10,
            run: dummy,
        }
    }

    fn env(
        tier: Tier,
        only: bool,
        desktop: bool,
        platform: &'static str,
        allow: &[&str],
    ) -> RunEnv {
        RunEnv {
            tier,
            only_selected: only,
            allow: allow
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<BTreeSet<_>>(),
            has_desktop: desktop,
            platform,
            have_installer: false,
            have_models: false,
        }
    }

    #[test]
    fn tier_filter_is_cumulative() {
        let c = case(Tier::Standard, &[], ALL);
        assert!(not_runnable(&c, &env(Tier::Smoke, false, true, "windows", &[])).is_some());
        assert!(not_runnable(&c, &env(Tier::Standard, false, true, "windows", &[])).is_none());
        assert!(not_runnable(&c, &env(Tier::Full, false, true, "windows", &[])).is_none());
        // --only overrides the tier but not the needs.
        assert!(not_runnable(&c, &env(Tier::Smoke, true, true, "windows", &[])).is_none());
    }

    #[test]
    fn needs_and_platforms_give_reasons() {
        let c = case(Tier::Smoke, &[Need::DesktopSession, Need::Screen], WIN);
        assert_eq!(
            not_runnable(&c, &env(Tier::Smoke, false, true, "macos", &[])).as_deref(),
            Some("not implemented for macos")
        );
        assert_eq!(
            not_runnable(&c, &env(Tier::Smoke, false, false, "windows", &[])).as_deref(),
            Some("needs a logged-in desktop session (this is session 0 or headless)")
        );
        assert_eq!(
            not_runnable(&c, &env(Tier::Smoke, false, true, "windows", &[])).as_deref(),
            Some("needs --allow screen")
        );
        assert!(not_runnable(&c, &env(Tier::Smoke, false, true, "windows", &["screen"])).is_none());
    }

    #[test]
    fn tiers_parse_and_order() {
        assert!(Tier::Smoke < Tier::Standard && Tier::Full < Tier::Hardware);
        assert_eq!(Tier::parse("full"), Some(Tier::Full));
        assert_eq!(Tier::parse("nope"), None);
    }
}
