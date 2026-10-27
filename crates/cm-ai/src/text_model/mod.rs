//! The text model provider (AI-7): the one way summaries, translations and reports ask a language
//! model for structured text.
//!
//! * [`TextModel::complete_json`] sends a prompt and a JSON schema and returns a value that
//!   matches the schema. A reply that does not match is sent back once with the validation error;
//!   a second miss is `job_failed` with `reason: "invalid_model_output"` ([`TextModelError`]).
//! * Providers ([`Provider`]): `none` (no network, callers get `invalid_input` with
//!   `data.reason = "no_text_model"`) and `embedded` (CaptureMann's own `llama-server` helper on a
//!   loopback port, see [`engine`]; the models are [`store`]). These are the only two settings can
//!   select. `local` is the internal loopback client the engine talks to its helper with;
//!   `openai_compatible` and `anthropic` are dormant and unreachable (settings cannot select them).
//! * **Local versus remote is decided by the URL host** ([`TextModelConfig::is_local`]): loopback
//!   is local, everything else, LAN included, is remote. A `local` provider only ever talks to
//!   loopback: with any other host every request is refused before it is sent.
//! * Keys are never in settings (only `ai.text.key_name`); the caller reads the key from the
//!   keychain and passes it in. Nothing here logs or returns it.
//! * [`chunk`] is the shared map-reduce helper for transcripts longer than the context.
//!
//! The settings (`ai.text.*`, app-only) are read with [`TextModelConfig::from_settings`].

pub mod chunk;
pub mod engine;
mod http;
pub mod schema;
pub mod store;

use std::fmt;

use cm_core::CmError;
use serde::Serialize;
use serde_json::{json, Map, Value};

pub use self::http::HttpTextModel;

/// Settings keys.
pub const KEY_PROVIDER: &str = "ai.text.provider";
pub const KEY_MODEL: &str = "ai.text.model";
/// The built-in model (a `text_models.json` id) the `embedded` provider runs; empty: the
/// text-model store's active one.
pub const KEY_EMBEDDED_MODEL: &str = "ai.text.embedded_model";
pub const KEY_KEY_NAME: &str = "ai.text.key_name";
pub const KEY_CONTEXT_TOKENS: &str = "ai.text.context_tokens";
pub const KEY_TIMEOUT_S: &str = "ai.text.timeout_s";
/// The host the user has been told text is sent to ("Text from your recordings will be sent to
/// <host>"); a different host shows the notice again.
pub const KEY_NOTICE_HOST: &str = "ai.text.remote_notice_host";

pub const DEFAULT_CONTEXT_TOKENS: u32 = 8192;
pub const DEFAULT_TIMEOUT_S: u64 = 300;
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com";

/// The message of an unreachable server (AI-7).
pub const UNREACHABLE_HINT: &str = "Open the AI page and check the built-in text model";

/// Where the text model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    None,
    Local,
    /// The helper process CaptureMann ships ([`engine`]).
    Embedded,
    OpenAiCompatible,
    Anthropic,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::None => "none",
            Provider::Local => "local",
            Provider::Embedded => "embedded",
            Provider::OpenAiCompatible => "openai_compatible",
            Provider::Anthropic => "anthropic",
        }
    }

    pub fn parse(s: &str) -> Option<Provider> {
        Some(match s {
            "none" => Provider::None,
            "local" => Provider::Local,
            "embedded" => Provider::Embedded,
            "openai_compatible" => Provider::OpenAiCompatible,
            "anthropic" => Provider::Anthropic,
            _ => return None,
        })
    }
}

/// The `ai.text.*` settings.
#[derive(Debug, Clone, PartialEq)]
pub struct TextModelConfig {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    /// `embedded` only: the built-in model id (empty: the store's active model).
    pub embedded_model: String,
    /// The keychain entry name of the API key (never the key).
    pub key_name: Option<String>,
    pub context_tokens: u32,
    pub timeout_s: u64,
}

impl Default for TextModelConfig {
    fn default() -> Self {
        TextModelConfig {
            provider: Provider::None,
            base_url: String::new(),
            model: String::new(),
            embedded_model: String::new(),
            key_name: None,
            context_tokens: DEFAULT_CONTEXT_TOKENS,
            timeout_s: DEFAULT_TIMEOUT_S,
        }
    }
}

/// The host of a URL, lowercase, without brackets, port or credentials.
pub fn url_host(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Whether `host` is this machine: `localhost`, `127.0.0.0/8` or `::1`.
pub fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_end_matches('.');
    if h == "localhost" {
        return true;
    }
    h.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.to_canonical().is_loopback())
}

impl TextModelConfig {
    /// Reads `ai.text.*` from the settings map. Unknown, malformed or unavailable (`local`, `openai_compatible`,
    /// `anthropic`) values fall back to the defaults (provider `none`), so a hand-edited file can never turn the
    /// network on. There is no server address setting: the built-in engine's address is chosen when it starts.
    pub fn from_settings(s: &Map<String, Value>) -> TextModelConfig {
        let text = |k: &str| {
            s.get(k)
                .and_then(Value::as_str)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let provider = text(KEY_PROVIDER)
            .and_then(|p| Provider::parse(&p))
            // The built-in engine only in 0.2.0 (maintainer, Oct 27, 2026): the other providers
            // stay in the code but a settings file can not select them.
            .filter(|p| matches!(p, Provider::None | Provider::Embedded))
            .unwrap_or(Provider::None);
        TextModelConfig {
            provider,
            base_url: String::new(),
            model: text(KEY_MODEL).unwrap_or_default(),
            embedded_model: text(KEY_EMBEDDED_MODEL).unwrap_or_default(),
            key_name: text(KEY_KEY_NAME),
            context_tokens: s
                .get(KEY_CONTEXT_TOKENS)
                .and_then(Value::as_u64)
                .filter(|n| (512..=4_000_000).contains(n))
                .map_or(DEFAULT_CONTEXT_TOKENS, |n| n as u32),
            timeout_s: s
                .get(KEY_TIMEOUT_S)
                .and_then(Value::as_u64)
                .filter(|n| (1..=3600).contains(n))
                .unwrap_or(DEFAULT_TIMEOUT_S),
        }
    }

    /// The URL requests go to: the setting, else the provider's default.
    pub fn effective_base_url(&self) -> String {
        let url = self.base_url.trim().trim_end_matches('/');
        if !url.is_empty() {
            return url.to_owned();
        }
        match self.provider {
            Provider::Anthropic => ANTHROPIC_URL.to_owned(),
            // The embedded helper's address is chosen when it starts ([`engine`]); a setting
            // never names it.
            Provider::None | Provider::Local | Provider::OpenAiCompatible | Provider::Embedded => {
                String::new()
            }
        }
    }

    /// The host requests go to (`None` for provider `none` or an unusable URL).
    pub fn host(&self) -> Option<String> {
        match self.provider {
            Provider::None => None,
            Provider::Embedded => Some("127.0.0.1".to_owned()),
            _ => url_host(&self.effective_base_url()),
        }
    }

    /// Whether the model runs on this machine. Decided by the URL host alone: a `local`
    /// provider with a LAN address is remote (and refused, see [`check_endpoint`](Self::check_endpoint)).
    /// Provider `none` is not remote either: it sends nothing, so it counts as local.
    pub fn is_local(&self) -> bool {
        match self.provider {
            Provider::None | Provider::Embedded => true,
            _ => self.host().is_some_and(|h| is_loopback_host(&h)),
        }
    }

    /// Whether text would leave this machine.
    pub fn is_remote(&self) -> bool {
        self.provider != Provider::None && !self.is_local()
    }

    /// The checks every request starts with: a usable URL and, for a `local` provider, loopback.
    pub fn check_endpoint(&self) -> Result<(), TextModelError> {
        if self.provider == Provider::None {
            return Err(TextModelError::no_text_model());
        }
        if self.provider == Provider::Embedded {
            // The helper listens on 127.0.0.1 only; the client built for it is a `local`
            // provider, so this check runs again on its address.
            return Ok(());
        }
        let url = self.effective_base_url();
        let Some(host) = url_host(&url) else {
            return Err(TextModelError::invalid_input(
                "The text model needs a server address",
            ));
        };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(TextModelError::invalid_input(
                "The text model address must start with http:// or https://",
            ));
        }
        if self.provider == Provider::Local && !is_loopback_host(&host) {
            return Err(TextModelError::invalid_input(format!(
                "A local text model must run on this computer (127.0.0.1 or localhost), not on {host}."
            )));
        }
        // Plain http is only for this machine: a key or recording text never crosses a network
        // unencrypted.
        if url.starts_with("http://") && !is_loopback_host(&host) {
            return Err(TextModelError::invalid_input(format!(
                "{host} must be reached over https://"
            )));
        }
        Ok(())
    }

    /// The text the first-use notice shows.
    pub fn notice_text(&self) -> Option<String> {
        self.is_remote()
            .then(|| self.host())
            .flatten()
            .map(|h| format!("Text from your recordings will be sent to {h}"))
    }

    /// The Confirm-tier plan of an agent call that would send `chars` characters of text to
    /// this provider (AI-7.AC3). `None` for `none` and for a local provider: those are Free.
    /// The daemon shows `text` in the dialog; callers (AI-2, AI-5, AI-9) add it to their own plan.
    pub fn send_plan(&self, chars: usize) -> Option<SendPlan> {
        if !self.is_remote() {
            return None;
        }
        let host = self.host()?;
        Some(SendPlan {
            text: format!(
                "Send {chars} characters of text from your recordings to {host} \
                 (model {})",
                if self.model.is_empty() {
                    "not chosen"
                } else {
                    &self.model
                }
            ),
            host,
            chars,
            provider: self.provider,
            model: self.model.clone(),
        })
    }
}

/// What a Confirm dialog says about text leaving the machine.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SendPlan {
    /// The dialog line.
    pub text: String,
    pub host: String,
    pub chars: usize,
    pub provider: Provider,
    pub model: String,
}

impl SendPlan {
    pub fn details(&self) -> Value {
        json!({ "host": self.host, "characters": self.chars, "provider": self.provider,
                "model": self.model, "network": self.text })
    }
}

/// Why a text-model call failed. [`TextModelError::reason`] is the `job_failed` reason.
#[derive(Debug, Clone, PartialEq)]
pub struct TextModelError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Provider `none`: `invalid_input`, `data.reason = "no_text_model"`.
    NoTextModel,
    /// A setting is unusable: `invalid_input`.
    InvalidInput,
    /// `job_failed`, `reason: "text_model_unreachable"`.
    Unreachable,
    /// `job_failed`, `reason: "timeout"`.
    Timeout,
    /// `job_failed`, `reason: "invalid_model_output"`.
    InvalidOutput,
    /// The server answered with an error (model not loaded, out of memory, bad key): its message
    /// is passed through. `job_failed`, `reason: "text_model_error"`.
    Server,
    /// The built-in engine's helper (`llama-server`) is not in the app. `job_failed`,
    /// `reason: "engine_missing"`.
    EngineMissing,
    /// The helper would not start, stopped while loading or died again. `job_failed`,
    /// `reason: "engine_failed"`.
    EngineFailed,
    /// The model needs more memory than is free. `job_failed`, `reason: "not_enough_memory"`.
    NotEnoughMemory,
}

impl TextModelError {
    pub fn no_text_model() -> Self {
        TextModelError {
            kind: ErrorKind::NoTextModel,
            message: "No text model is set up. Open the AI page, download the built-in text model."
                .to_owned(),
        }
    }

    /// The built-in provider is chosen but the model file is not on this computer.
    pub fn no_text_model_named(id: &str) -> Self {
        TextModelError {
            kind: ErrorKind::NoTextModel,
            message: if id.is_empty() {
                "No text model is downloaded yet. Open the AI page to download one.".to_owned()
            } else {
                format!("The text model `{id}` is not downloaded. Open the AI page to download it.")
            },
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        TextModelError {
            kind: ErrorKind::InvalidInput,
            message: message.into(),
        }
    }

    /// The `job_failed` reason, or `invalid_input`'s `no_text_model` / `invalid_input`.
    pub fn reason(&self) -> &'static str {
        match self.kind {
            ErrorKind::NoTextModel => "no_text_model",
            ErrorKind::InvalidInput => "invalid_input",
            ErrorKind::Unreachable => "text_model_unreachable",
            ErrorKind::Timeout => "timeout",
            ErrorKind::InvalidOutput => "invalid_model_output",
            ErrorKind::Server => "text_model_error",
            ErrorKind::EngineMissing => "engine_missing",
            ErrorKind::EngineFailed => "engine_failed",
            ErrorKind::NotEnoughMemory => "not_enough_memory",
        }
    }

    /// The shared error: `invalid_input` for the two setup kinds, else `job_failed` for `job_id`.
    pub fn into_cm_error(self, job_id: &str) -> CmError {
        match self.kind {
            ErrorKind::NoTextModel | ErrorKind::InvalidInput => {
                CmError::invalid_input(self.message)
            }
            _ => {
                let reason = self.reason();
                match CmError::job_failed(job_id, reason) {
                    CmError::JobFailed { job_id, reason, .. } => CmError::JobFailed {
                        message: self.message,
                        job_id,
                        reason,
                    },
                    other => other,
                }
            }
        }
    }

    /// The `error.data` the daemon adds: `{"reason": ...}`.
    pub fn error_data(&self) -> Value {
        json!({ "reason": self.reason() })
    }
}

impl fmt::Display for TextModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TextModelError {}

/// One chat message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Message {
    pub role: &'static str,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Message {
            role: "user",
            content: content.into(),
        }
    }
}

/// A request for schema-shaped JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonRequest {
    /// Instructions for the model (the system prompt).
    pub system: Option<String>,
    pub messages: Vec<Message>,
    /// The JSON schema the reply must match.
    pub schema: Value,
    /// A short identifier for the schema (`[A-Za-z0-9_]`, used by the providers).
    pub schema_name: String,
    /// The most tokens the reply may use.
    pub max_tokens: u32,
}

impl JsonRequest {
    pub fn new(prompt: impl Into<String>, schema: Value) -> Self {
        JsonRequest {
            system: None,
            messages: vec![Message::user(prompt)],
            schema,
            schema_name: "reply".to_owned(),
            max_tokens: 4096,
        }
    }

    /// The characters the request sends (what the Confirm dialog counts).
    pub fn chars(&self) -> usize {
        self.system.as_deref().map_or(0, |s| s.chars().count())
            + self
                .messages
                .iter()
                .map(|m| m.content.chars().count())
                .sum::<usize>()
    }
}

/// A reply and how it was got.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonReply {
    pub value: Value,
    /// 1, or 2 when the first reply failed the schema and the retry fixed it.
    pub attempts: u32,
}

/// A language model that answers with schema-validated JSON.
pub trait TextModel: Send + Sync {
    /// Blocking. Validates against `req.schema`, retries once with the validation error, then
    /// fails with `invalid_model_output`.
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError>;
}

/// The provider `none`: no network call is ever made.
pub struct NoTextModel;

impl TextModel for NoTextModel {
    fn complete_json(&self, _req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        Err(TextModelError::no_text_model())
    }
}

/// The model for `config` (`key` is the API key read from the keychain by name, if any).
pub fn build(config: &TextModelConfig, key: Option<String>) -> Box<dyn TextModel> {
    match config.provider {
        Provider::None => Box::new(NoTextModel),
        // The engine is the daemon's to wire ([`engine::EmbeddedTextModel`]); without one the
        // provider cannot answer.
        Provider::Embedded => Box::new(engine::Unwired),
        _ => Box::new(HttpTextModel::new(config.clone(), key)),
    }
}

/// The outcome of the Test button.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestOutcome {
    pub ok: bool,
    pub latency_ms: u64,
    pub model: String,
    /// The first reply matched the schema without a retry.
    pub structured_output: bool,
    /// On a failure: the `job_failed` reason and the message to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Sends a short prompt and measures the reply (`test_text_model`, the AI page's Test button).
pub fn test_model(model: &dyn TextModel, name: &str) -> TestOutcome {
    let req = JsonRequest::new(
        "Reply with the JSON object {\"ok\": true}.",
        json!({ "type": "object", "properties": { "ok": { "type": "boolean" } },
                "required": ["ok"], "additionalProperties": false }),
    );
    let started = std::time::Instant::now();
    let r = model.complete_json(&req);
    let latency_ms = started.elapsed().as_millis() as u64;
    match r {
        Ok(reply) => TestOutcome {
            ok: true,
            latency_ms,
            model: name.to_owned(),
            structured_output: reply.attempts == 1,
            reason: None,
            message: None,
        },
        Err(e) => TestOutcome {
            ok: false,
            latency_ms,
            model: name.to_owned(),
            structured_output: false,
            reason: Some(e.reason().to_owned()),
            message: Some(e.message),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(provider: &str, url: &str) -> TextModelConfig {
        let mut m = Map::new();
        m.insert(KEY_PROVIDER.into(), json!(provider));
        let mut c = TextModelConfig::from_settings(&m);
        // The dormant providers can not come from settings; set them directly.
        if let Some(p) = Provider::parse(provider) {
            c.provider = p;
        }
        c.base_url = url.to_owned();
        c
    }

    #[test]
    fn settings_can_not_select_a_remote_provider() {
        for p in ["local", "openai_compatible", "anthropic", "bogus"] {
            let mut m = Map::new();
            m.insert(KEY_PROVIDER.into(), json!(p));
            assert_eq!(TextModelConfig::from_settings(&m).provider, Provider::None);
        }
        let mut m = Map::new();
        m.insert(KEY_PROVIDER.into(), json!("embedded"));
        m.insert("ai.text.base_url".into(), json!("https://evil.example/v1"));
        let c = TextModelConfig::from_settings(&m);
        assert_eq!(c.provider, Provider::Embedded);
        assert_eq!(c.base_url, "");
    }

    #[test]
    fn hosts_and_loopback() {
        assert_eq!(
            url_host("http://127.0.0.1:8080/v1").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(url_host("http://[::1]:8081/v1").as_deref(), Some("::1"));
        assert_eq!(
            url_host("https://user:pw@API.Example.com/x").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(url_host("nonsense"), None);
        for h in ["localhost", "127.0.0.1", "127.9.9.9", "::1", "LOCALHOST"] {
            assert!(is_loopback_host(&h.to_ascii_lowercase()), "{h}");
        }
        for h in [
            "192.168.1.20",
            "10.0.0.1",
            "example.com",
            "localhost.evil.com",
            "0.0.0.0",
        ] {
            assert!(!is_loopback_host(h), "{h}");
        }
    }

    #[test]
    fn defaults_are_provider_none() {
        let c = TextModelConfig::from_settings(&Map::new());
        assert_eq!(c.provider, Provider::None);
        assert_eq!(c.context_tokens, 8192);
        assert_eq!(c.timeout_s, 300);
        assert!(c.send_plan(10).is_none());
        assert_eq!(c.check_endpoint().unwrap_err().kind, ErrorKind::NoTextModel);
        // An unknown provider never turns the network on.
        assert_eq!(cfg("skynet", "http://x").provider, Provider::None);
    }

    #[test]
    fn a_local_provider_with_a_lan_address_is_remote_and_refused() {
        let c = cfg("local", "http://192.168.1.20:8080/v1");
        assert!(!c.is_local());
        assert!(c.is_remote());
        let e = c.check_endpoint().unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidInput);
        assert!(e.message.contains("192.168.1.20"));
        // The same address through the OpenAI-compatible provider (over https) is a valid remote.
        let c = cfg("openai_compatible", "https://192.168.1.20/v1");
        assert!(c.is_remote());
        assert!(c.check_endpoint().is_ok());
        assert!(cfg("openai_compatible", "http://192.168.1.20:8080/v1")
            .check_endpoint()
            .is_err());
    }

    #[test]
    fn loopback_providers_are_local_with_no_plan() {
        let c = cfg("local", "http://127.0.0.1:8080/v1");
        assert!(c.is_local() && c.check_endpoint().is_ok());
        assert!(c.send_plan(100).is_none());
        let c = cfg("openai_compatible", "http://localhost:8081/v1");
        assert!(c.is_local());
        assert!(c.notice_text().is_none());
    }

    #[test]
    fn remote_plan_names_host_and_characters() {
        let mut c = cfg("anthropic", "");
        c.model = "claude-x".into();
        assert_eq!(c.host().as_deref(), Some("api.anthropic.com"));
        let p = c.send_plan(1234).unwrap();
        assert_eq!(p.host, "api.anthropic.com");
        assert!(p.text.contains("1234 characters") && p.text.contains("api.anthropic.com"));
        assert_eq!(
            c.notice_text().unwrap(),
            "Text from your recordings will be sent to api.anthropic.com"
        );
    }

    #[test]
    fn errors_map_to_the_shared_codes() {
        let e = TextModelError::no_text_model().into_cm_error("j1");
        assert_eq!(e.code().as_str(), "invalid_input");
        let e = TextModelError {
            kind: ErrorKind::InvalidOutput,
            message: "bad".into(),
        };
        assert_eq!(e.reason(), "invalid_model_output");
        match e.into_cm_error("j1") {
            CmError::JobFailed {
                reason,
                job_id,
                message,
            } => {
                assert_eq!(
                    (reason.as_str(), job_id.as_str(), message.as_str()),
                    ("invalid_model_output", "j1", "bad")
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
