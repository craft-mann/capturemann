//! The voice part of the AI page, display-free (board V8, AI-7, AI-8): the speaker-label
//! settings, the text-model provider section and this Mac's memory.
//!
//! The built-in text model only (maintainer decision Oct 27, 2026): None or Built in (the text
//! engine CaptureMann ships).
//! Nothing here touches the network or the screen by itself. Every change returns
//! the settings to write (`Vec<(key, value)>`), which `ui::ai` sends through the same app-only
//! path as `ai.glossary`; an agent cannot write them (`ai.*` is app-only).

use cm_ai::text_model::{Provider, KEY_PROVIDER};
use cm_i18n::{fmt, tr, tr_ctx};
use serde_json::{json, Map, Value};

/// `ai.auto_speakers`: find speakers after each transcript (app-only, default off).
pub const KEY_AUTO_SPEAKERS: &str = "ai.auto_speakers";
/// `profile.display_name`: the name of the user's own words (headset mode), default "Me".
pub const KEY_DISPLAY_NAME: &str = "profile.display_name";
/// The default of [`KEY_DISPLAY_NAME`].
pub const DEFAULT_DISPLAY_NAME: &str = "Me";

// ---- this Mac's memory -------------------------------------------------------------------------

/// This Mac's installed memory in bytes (`sysctl hw.memsize`), read only. `None` elsewhere or
/// when the answer cannot be read.
pub fn system_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Whole GB (2^30 bytes, rounded: a "24 GB" Mac reports 24 GiB).
pub fn memory_gb(bytes: u64) -> u32 {
    (bytes as f64 / (1024.0 * 1024.0 * 1024.0)).round() as u32
}

// ---- speaker labels (AI-8) ---------------------------------------------------------------------

/// What the speaker engine needs, as one offer: the model files the daemon lists with kind
/// `speaker` (AI-8: pyannote segmentation, MIT, and the CAM++ voice model, Apache-2.0), added up.
/// The files themselves are rows of the page ([`crate::ai::models::ModelsVm::speaker_rows`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerModelEntry {
    pub id: String,
    pub label: String,
    /// SPDX names shown before the download, `MIT, Apache-2.0`.
    pub licence: String,
    /// What is still missing, MB.
    pub size_mb: u32,
}

/// What the speaker model card shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeakerCard {
    /// The daemon lists no speaker model (an older daemon, or the list has not arrived).
    Unavailable,
    /// Files to download or import: their licences and size are on the card first.
    Offer(SpeakerModelEntry),
    /// Everything is installed, by name.
    Installed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerVm {
    pub auto_speakers: bool,
    pub display_name: String,
    /// The offer, while some file is missing; `None` before the list arrives or when it lists no
    /// speaker model.
    pub offer: Option<SpeakerModelEntry>,
    /// The name of the installed speaker models, when all of them are.
    pub installed: Option<String>,
}

impl Default for SpeakerVm {
    fn default() -> Self {
        SpeakerVm {
            auto_speakers: false,
            display_name: DEFAULT_DISPLAY_NAME.to_owned(),
            offer: None,
            installed: None,
        }
    }
}

impl SpeakerVm {
    pub fn from_settings(s: &Map<String, Value>) -> SpeakerVm {
        SpeakerVm {
            auto_speakers: s
                .get(KEY_AUTO_SPEAKERS)
                .and_then(Value::as_bool)
                .unwrap_or(false),
            display_name: display_name_of(s.get(KEY_DISPLAY_NAME)),
            ..SpeakerVm::default()
        }
    }

    /// Takes the speaker models of a `list_models` answer (the rows with kind `speaker`).
    pub fn set_models(&mut self, models: &[cm_ai::model::ModelInfo]) {
        if models.is_empty() {
            self.offer = None;
            self.installed = None;
            return;
        }
        let missing: Vec<&cm_ai::model::ModelInfo> =
            models.iter().filter(|m| !m.installed).collect();
        if missing.is_empty() {
            self.offer = None;
            self.installed = Some(tr!("Speaker models (pyannote segmentation and CAM++)"));
            return;
        }
        self.installed = None;
        let mut licences: Vec<&str> = Vec::new();
        for m in models.iter().filter(|m| !m.license.is_empty()) {
            if !licences.contains(&m.license.as_str()) {
                licences.push(m.license.as_str());
            }
        }
        self.offer = Some(SpeakerModelEntry {
            id: "speaker".to_owned(),
            label: tr!("Speaker models"),
            licence: licences.join(", "),
            size_mb: missing.iter().map(|m| m.size_mb).sum(),
        });
    }

    pub fn card(&self) -> SpeakerCard {
        if let Some(name) = &self.installed {
            return SpeakerCard::Installed(name.clone());
        }
        match &self.offer {
            Some(m) => SpeakerCard::Offer(m.clone()),
            None => SpeakerCard::Unavailable,
        }
    }

    /// The switch needs an installed model.
    pub fn auto_enabled(&self) -> bool {
        self.installed.is_some()
    }

    /// The switch is shown on only when it is on and usable.
    pub fn auto_on(&self) -> bool {
        self.auto_speakers && self.auto_enabled()
    }

    /// Flips the switch. Without a model nothing changes.
    pub fn set_auto(&mut self, on: bool) -> Vec<(&'static str, Value)> {
        if !self.auto_enabled() {
            return Vec::new();
        }
        self.auto_speakers = on;
        vec![(KEY_AUTO_SPEAKERS, Value::Bool(on))]
    }

    /// Saves the name; an empty box means the default "Me".
    pub fn set_display_name(&mut self, text: &str) -> Vec<(&'static str, Value)> {
        let name = clean_name(text);
        self.display_name = name.clone();
        vec![(KEY_DISPLAY_NAME, Value::String(name))]
    }

    /// Whether the box holds something other than what is saved.
    pub fn name_dirty(&self, text: &str) -> bool {
        clean_name(text) != self.display_name
    }
}

fn clean_name(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        DEFAULT_DISPLAY_NAME.to_owned()
    } else {
        t.chars().take(60).collect()
    }
}

fn display_name_of(v: Option<&Value>) -> String {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| DEFAULT_DISPLAY_NAME.to_owned(), str::to_owned)
}

// ---- the text model (AI-7) ---------------------------------------------------------------------

/// The segmented control: None / Built in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderChoice {
    #[default]
    None,
    /// The engine inside CaptureMann (provider `embedded`).
    BuiltIn,
}

impl ProviderChoice {
    pub const ALL: [ProviderChoice; 2] = [ProviderChoice::None, ProviderChoice::BuiltIn];

    pub fn provider(self) -> Provider {
        match self {
            ProviderChoice::None => Provider::None,
            ProviderChoice::BuiltIn => Provider::Embedded,
        }
    }

    /// Any other provider (a hand-edited file) shows as None: the daemon treats it as none too.
    pub fn from_provider(p: Provider) -> ProviderChoice {
        match p {
            Provider::Embedded => ProviderChoice::BuiltIn,
            _ => ProviderChoice::None,
        }
    }

    pub fn label(self) -> String {
        match self {
            ProviderChoice::None => tr_ctx!("text model provider", "None"),
            ProviderChoice::BuiltIn => tr_ctx!("text model provider", "Built in"),
        }
    }
}

/// The built-in engine as `get_text_model` reports it (`engine`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineLine {
    /// Not running; it starts when a report or a translation needs it.
    Stopped,
    Starting,
    /// Running this model; it stops after this many seconds without a request.
    Running {
        model: String,
        idle_stop_s: u64,
    },
    /// The helper is not in this app (`engine_missing`).
    Missing,
    /// It could not start or stopped (`engine_failed`, `not_enough_memory`): the daemon's message.
    Failed(String),
}

impl EngineLine {
    pub fn from_json(v: Option<&Value>) -> EngineLine {
        let Some(v) = v else {
            return EngineLine::Stopped;
        };
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        match s("state").as_str() {
            "starting" => EngineLine::Starting,
            "running" => EngineLine::Running {
                model: s("model"),
                idle_stop_s: v.get("idle_stop_s").and_then(Value::as_u64).unwrap_or(0),
            },
            "failed" if s("reason") == "engine_missing" => EngineLine::Missing,
            "failed" => EngineLine::Failed(s("error")),
            _ => EngineLine::Stopped,
        }
    }

    /// The status line under the Built in choice.
    pub fn text(&self) -> String {
        match self {
            EngineLine::Stopped => {
                tr!("Engine stopped. It starts by itself when a report or a translation needs it.")
            }
            EngineLine::Starting => tr!("Engine loading the model\u{2026}"),
            EngineLine::Running { model, idle_stop_s } => tr!(
                "Engine running {model}. It stops after {minutes} min without use.",
                model = model,
                minutes = (*idle_stop_s / 60).max(1)
            ),
            EngineLine::Missing => tr!(
                "The text engine is missing from this copy of CaptureMann. Reinstall CaptureMann."
            ),
            EngineLine::Failed(m) if m.is_empty() => tr!("The engine could not start."),
            EngineLine::Failed(m) => m.clone(),
        }
    }

    pub fn is_problem(&self) -> bool {
        matches!(self, EngineLine::Missing | EngineLine::Failed(_))
    }
}

/// The last Test.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TestState {
    #[default]
    Idle,
    Running,
    Passed {
        latency_ms: u64,
        structured: bool,
        /// The loopback host the test went to: the line says it only talks to it.
        host: Option<String>,
    },
    Failed(String),
}

/// What pressing Test does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestGate {
    /// Send the test now.
    Run,
    /// Say why it cannot run (the message is already worded).
    Blocked(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextVm {
    /// `get_text_model` has answered at least once.
    pub loaded: bool,
    pub provider: ProviderChoice,
    pub model: String,
    pub context_tokens: u32,
    pub host: Option<String>,
    /// The built-in engine's state (`get_text_model.engine`).
    pub engine: EngineLine,
    pub test: TestState,
    /// The last error of a settings write or a keychain write, worded.
    pub error: Option<String>,
}

impl Default for TextVm {
    fn default() -> Self {
        TextVm {
            loaded: false,
            provider: ProviderChoice::None,
            model: String::new(),
            context_tokens: cm_ai::text_model::DEFAULT_CONTEXT_TOKENS,
            host: None,
            engine: EngineLine::Stopped,
            test: TestState::Idle,
            error: None,
        }
    }
}

type Writes = Vec<(&'static str, Value)>;

impl TextVm {
    /// Takes the answer of `get_text_model`. The Test result belongs to the settings it ran
    /// with, so it is cleared when they changed.
    pub fn set_from_daemon(&mut self, v: &Value) {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let provider = Provider::parse(&s("provider")).unwrap_or(Provider::None);
        let changed =
            self.provider != ProviderChoice::from_provider(provider) || self.model != s("model");
        self.provider = ProviderChoice::from_provider(provider);
        self.model = s("model");
        self.host = v
            .get("host")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|h| !h.is_empty());
        self.context_tokens = v
            .get("context_tokens")
            .and_then(Value::as_u64)
            .map_or(cm_ai::text_model::DEFAULT_CONTEXT_TOKENS, |n| n as u32);
        self.engine = EngineLine::from_json(v.get("engine"));
        if changed && !matches!(self.test, TestState::Running) {
            self.test = TestState::Idle;
        }
        self.loaded = true;
    }

    /// A text model is set up and usable: a provider is chosen and it names a model (for the
    /// built-in one the daemon names the downloaded model, and nothing while none is).
    pub fn ready(&self) -> bool {
        self.provider != ProviderChoice::None && !self.model.trim().is_empty()
    }

    /// A press on the segmented control: the settings to write. Choosing Built in clears the
    /// shown model (the daemon names the downloaded one) and the Test result.
    pub fn choose_provider(&mut self, c: ProviderChoice) -> Writes {
        if c == self.provider {
            return Vec::new();
        }
        let w: Writes = vec![(KEY_PROVIDER, json!(c.provider().as_str()))];
        if c == ProviderChoice::BuiltIn {
            // The built-in model is picked in its list, not here.
            self.model.clear();
        }
        self.provider = c;
        self.test = TestState::Idle;
        self.error = None;
        w
    }

    /// Pressing Test: what has to happen first.
    pub fn begin_test(&mut self) -> TestGate {
        if self.provider == ProviderChoice::None {
            return TestGate::Blocked(tr!("Choose a text model provider first."));
        }
        if self.model.is_empty() {
            return TestGate::Blocked(tr!("Download a built-in model first."));
        }
        self.test = TestState::Running;
        TestGate::Run
    }

    /// The answer of `test_text_model` (or its error).
    pub fn test_answer(&mut self, r: Result<Value, String>) {
        self.test = match r {
            Err(message) => TestState::Failed(message),
            Ok(v) if v.get("ok").and_then(Value::as_bool) == Some(true) => TestState::Passed {
                latency_ms: v.get("latency_ms").and_then(Value::as_u64).unwrap_or(0),
                structured: v
                    .get("structured_output")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                host: self.host.clone(),
            },
            Ok(v) => TestState::Failed(
                v.get("message")
                    .and_then(Value::as_str)
                    .map_or_else(|| tr!("The text model did not answer."), str::to_owned),
            ),
        };
    }

    /// The Test result as the lines of the result card: `(headline, detail)`.
    pub fn test_lines(&self) -> Option<(String, String)> {
        match &self.test {
            TestState::Idle => None,
            TestState::Running => Some((tr!("Testing\u{2026}"), String::new())),
            TestState::Failed(m) => Some((tr!("The test failed"), m.clone())),
            TestState::Passed {
                latency_ms,
                structured,
                host,
            } => {
                let secs = fmt::decimal(*latency_ms as f64 / 1000.0, 1);
                let head = tr!("Replied in {seconds} s", seconds = secs);
                let structured = if *structured {
                    tr!("Structured output works")
                } else {
                    tr!("Structured output did not work; replies are checked and retried")
                };
                let talks = match host {
                    Some(h) => tr!("only talks to {host}", host = h),
                    None => String::new(),
                };
                let detail = if talks.is_empty() {
                    structured
                } else {
                    format!("{structured} \u{00b7} {talks}")
                };
                Some((head, detail))
            }
        }
    }
}

// ---- the page's whole voice state --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VoiceVm {
    pub speakers: SpeakerVm,
    pub text: TextVm,
    /// This Mac's memory, read once.
    pub memory_bytes: Option<u64>,
    /// The page asked for the first reads (memory, text model).
    pub requested: bool,
}

impl VoiceVm {
    pub fn from_settings(s: &Map<String, Value>) -> VoiceVm {
        VoiceVm {
            speakers: SpeakerVm::from_settings(s),
            ..VoiceVm::default()
        }
    }

    pub fn memory_gb(&self) -> Option<u32> {
        self.memory_bytes.map(memory_gb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_with(provider: &str, model: &str) -> TextVm {
        let mut t = TextVm::default();
        t.set_from_daemon(&json!({
            "provider": provider, "model": model, "host": "127.0.0.1", "context_tokens": 8192,
        }));
        t
    }

    #[test]
    fn memory_rounds_to_whole_gigabytes() {
        assert_eq!(memory_gb(24 * 1024 * 1024 * 1024), 24);
        assert_eq!(memory_gb(17_179_869_184), 16);
        assert_eq!(memory_gb(0), 0);
    }

    fn speaker_file(id: &str, license: &str, mb: u32, installed: bool) -> cm_ai::model::ModelInfo {
        cm_ai::model::ModelInfo {
            id: id.into(),
            label: id.into(),
            size_mb: mb,
            installed,
            license: license.into(),
            kind: cm_ai::model::ModelKind::Speaker,
            ..Default::default()
        }
    }

    #[test]
    fn the_speaker_card_is_unavailable_until_the_daemon_lists_speaker_models() {
        let mut vm = SpeakerVm::default();
        assert_eq!(vm.card(), SpeakerCard::Unavailable);
        assert!(!vm.auto_enabled());
        vm.set_models(&[]);
        assert_eq!(vm.card(), SpeakerCard::Unavailable, "do not invent a model");
    }

    #[test]
    fn the_speaker_card_offers_what_is_missing_with_licences_and_size() {
        let mut vm = SpeakerVm::default();
        vm.set_models(&[
            speaker_file("a", "MIT", 6, false),
            speaker_file("b", "Apache-2.0", 30, false),
        ]);
        let SpeakerCard::Offer(m) = vm.card() else {
            panic!("an offer")
        };
        assert_eq!((m.licence.as_str(), m.size_mb), ("MIT, Apache-2.0", 36));
        // One file installed: only the other one is counted.
        vm.set_models(&[
            speaker_file("a", "MIT", 6, true),
            speaker_file("b", "Apache-2.0", 30, false),
        ]);
        let SpeakerCard::Offer(m) = vm.card() else {
            panic!("an offer")
        };
        assert_eq!(m.size_mb, 30);
        assert!(!vm.auto_enabled(), "one of two files is not enough");
    }

    #[test]
    fn the_speaker_switch_needs_all_the_speaker_files() {
        let mut vm = SpeakerVm::default();
        assert!(vm.set_auto(true).is_empty());
        assert!(!vm.auto_on());
        vm.set_models(&[
            speaker_file("a", "MIT", 6, true),
            speaker_file("b", "Apache-2.0", 30, true),
        ]);
        assert!(matches!(vm.card(), SpeakerCard::Installed(_)));
        assert_eq!(
            vm.set_auto(true),
            vec![(KEY_AUTO_SPEAKERS, Value::Bool(true))]
        );
        assert!(vm.auto_on());
        // Removing one file turns the switch off again.
        vm.set_models(&[
            speaker_file("a", "MIT", 6, true),
            speaker_file("b", "Apache-2.0", 30, false),
        ]);
        assert!(!vm.auto_on());
    }

    #[test]
    fn the_name_defaults_to_me_and_empty_means_me() {
        let mut vm = SpeakerVm::from_settings(&Map::new());
        assert_eq!(vm.display_name, "Me");
        assert!(!vm.auto_speakers);
        assert!(vm.name_dirty("Trung"));
        assert_eq!(
            vm.set_display_name("  Trung "),
            vec![(KEY_DISPLAY_NAME, json!("Trung"))]
        );
        assert!(!vm.name_dirty("Trung"));
        assert_eq!(
            vm.set_display_name("   "),
            vec![(KEY_DISPLAY_NAME, json!("Me"))]
        );
        let mut s = Map::new();
        s.insert(KEY_DISPLAY_NAME.into(), json!("Linh"));
        s.insert(KEY_AUTO_SPEAKERS.into(), json!(true));
        let vm = SpeakerVm::from_settings(&s);
        assert_eq!(vm.display_name, "Linh");
        assert!(vm.auto_speakers && !vm.auto_on(), "on, but no model yet");
    }

    #[test]
    fn built_in_is_the_embedded_provider_and_comes_back_from_the_daemon() {
        assert_eq!(ProviderChoice::BuiltIn.provider(), Provider::Embedded);
        assert_eq!(
            ProviderChoice::from_provider(Provider::Embedded),
            ProviderChoice::BuiltIn
        );
        let mut t = TextVm::default();
        let w = t.choose_provider(ProviderChoice::BuiltIn);
        assert_eq!(w, vec![(KEY_PROVIDER, json!("embedded"))]);
        assert_eq!(t.provider, ProviderChoice::BuiltIn);
        // Choosing the same segment again writes nothing; None writes the provider only.
        assert!(t.choose_provider(ProviderChoice::BuiltIn).is_empty());
        assert_eq!(
            t.choose_provider(ProviderChoice::None),
            vec![(KEY_PROVIDER, json!("none"))]
        );
        t.choose_provider(ProviderChoice::BuiltIn);
        // Not ready until the daemon names a downloaded model.
        assert!(!t.ready());
        t.set_from_daemon(
            &json!({"provider": "embedded", "model": "text-16gb", "host": "127.0.0.1"}),
        );
        assert!(t.ready());
        t.set_from_daemon(&json!({"provider": "embedded", "model": ""}));
        assert!(!t.ready());
        assert!(matches!(t.begin_test(), TestGate::Blocked(m) if m.contains("Download")));
    }

    #[test]
    fn the_engine_line_follows_the_daemons_state() {
        assert_eq!(EngineLine::from_json(None), EngineLine::Stopped);
        let line = |v: Value| EngineLine::from_json(Some(&v));
        assert_eq!(line(json!({"state": "stopped"})), EngineLine::Stopped);
        assert_eq!(line(json!({"state": "starting"})), EngineLine::Starting);
        assert_eq!(
            line(json!({"state": "running", "model": "text-8gb", "idle_stop_s": 600})),
            EngineLine::Running {
                model: "text-8gb".into(),
                idle_stop_s: 600
            }
        );
        let missing = line(json!({"state": "failed", "reason": "engine_missing", "error": "x"}));
        assert_eq!(missing, EngineLine::Missing);
        assert!(missing.is_problem());
        assert!(missing.text().contains("Reinstall"));
        let failed = line(
            json!({"state": "failed", "reason": "not_enough_memory", "error": "Close other apps"}),
        );
        assert_eq!(failed.text(), "Close other apps");
        assert!(!EngineLine::Stopped.is_problem());
        let mut t = TextVm::default();
        t.set_from_daemon(&json!({"provider": "embedded", "engine": {"state": "starting"}}));
        assert_eq!(t.engine, EngineLine::Starting);
    }

    #[test]
    fn only_none_and_built_in_are_offered() {
        assert_eq!(ProviderChoice::ALL.len(), 2);
        // Any other provider from a hand-edited file shows as None.
        for p in ["local", "openai_compatible", "anthropic"] {
            assert_eq!(text_with(p, "m").provider, ProviderChoice::None, "{p}");
        }
        assert_eq!(text_with("embedded", "m").provider, ProviderChoice::BuiltIn);
    }

    #[test]
    fn test_is_blocked_until_the_setup_is_complete() {
        let mut t = TextVm::default();
        assert!(matches!(t.begin_test(), TestGate::Blocked(_)));
        let mut t = text_with("embedded", "");
        assert!(matches!(t.begin_test(), TestGate::Blocked(_)));
        let mut t = text_with("embedded", "text-8gb");
        assert_eq!(t.begin_test(), TestGate::Run);
        assert_eq!(t.test, TestState::Running);
    }

    #[test]
    fn the_test_result_says_time_structure_and_loopback() {
        let mut t = text_with("embedded", "text-8gb");
        assert!(t.test_lines().is_none());
        t.begin_test();
        t.test_answer(Ok(
            json!({"ok": true, "latency_ms": 1400, "model": "text-8gb", "structured_output": true}),
        ));
        let (head, detail) = t.test_lines().unwrap();
        assert_eq!(head, "Replied in 1.4 s");
        assert_eq!(
            detail,
            "Structured output works \u{00b7} only talks to 127.0.0.1"
        );
        t.test_answer(Ok(
            json!({"ok": false, "reason": "text_model_unreachable", "message": "Open the AI page and check the built-in text model"}),
        ));
        assert_eq!(
            t.test_lines().unwrap().1,
            "Open the AI page and check the built-in text model"
        );
        t.test_answer(Err("No text model".into()));
        assert!(matches!(t.test, TestState::Failed(_)));
    }

    #[test]
    fn changed_settings_clear_the_old_test_result() {
        let mut t = text_with("embedded", "a");
        t.test_answer(Ok(json!({"ok": true, "latency_ms": 10})));
        t.set_from_daemon(&json!({"provider": "embedded", "model": "a"}));
        assert!(matches!(t.test, TestState::Passed { .. }));
        t.set_from_daemon(&json!({"provider": "embedded", "model": "b"}));
        assert_eq!(t.test, TestState::Idle);
    }
}
