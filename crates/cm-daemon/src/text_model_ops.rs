//! `get_text_model` and `test_text_model` (AI-7), and the helpers the later callers (AI-5
//! translation, AI-9 reports, AI-2 summaries) use to reach the text model.
//!
//! The settings are `ai.text.*` ([`cm_ai::text_model`]); they are app-only (`ai.*` is on the
//! [`crate::engines::APP_ONLY_SETTINGS`] list, so `set_settings` from an agent is refused). The
//! API key lives in the keychain under the name in `ai.text.key_name`; this module reads it only
//! to build a request and never returns it, logs it or puts it in an event.
//!
//! # For callers
//!
//! * [`Inner::text_model_config`] reads the settings, [`Inner::text_model_client`] builds the
//!   [`TextModel`] (key included) off the async threads' worry: it is blocking, call it inside
//!   [`blocking`](crate::daemon::blocking).
//! * [`Inner::text_model_gate`] is the one place that decides what a caller must do before text
//!   leaves the machine: provider `none` is `invalid_input` with `data.reason = "no_text_model"`;
//!   a remote provider returns a [`SendPlan`] whose `text` and `details` go into the caller's
//!   Confirm dialog (host and number of characters, AI-7.AC3); a local provider returns `None`
//!   (Free). The app additionally shows the first-use notice ([`Inner::text_model_notice_pending`])
//!   before the first send and records the host in `ai.text.remote_notice_host`.

use std::sync::{Arc, Mutex, PoisonError};

use cm_ai::text_model::engine::{EmbeddedTextModel, EngineSpec};
use cm_ai::text_model::{
    self, ErrorKind, Provider, SendPlan, TextModel, TextModelConfig, TextModelError,
    KEY_NOTICE_HOST,
};
use cm_auth::{SecretStore, VaultStore};
use cm_core::CmError;
use serde_json::{json, Value};

use crate::daemon::{blocking, Inner};
use crate::error::DaemonError;
use crate::storage_cloud::KEYCHAIN_SERVICE;

/// The action names of this module.
pub const TEXT_MODEL_METHODS: &[&str] = &["get_text_model", "test_text_model"];

/// Keychain access and the last test result.
pub struct TextModelState {
    secrets: Option<Arc<dyn SecretStore>>,
    /// The result of the last Test for the settings it ran with: `(fingerprint, structured)`.
    last_test: Mutex<Option<(String, bool)>>,
    /// A model used instead of the HTTP client (tests and embedding; see
    /// [`crate::Daemon::set_text_model_override`]).
    override_model: Mutex<Option<Arc<dyn TextModel>>>,
}

/// Lends a shared model to a caller that wants a `Box<dyn TextModel>`.
struct SharedModel(Arc<dyn TextModel>);

impl TextModel for SharedModel {
    fn complete_json(
        &self,
        req: &text_model::JsonRequest,
    ) -> Result<text_model::JsonReply, TextModelError> {
        self.0.complete_json(req)
    }
}

impl TextModelState {
    pub fn new(secrets: Option<Arc<dyn SecretStore>>) -> Self {
        TextModelState {
            secrets,
            last_test: Mutex::new(None),
            override_model: Mutex::new(None),
        }
    }

    fn store(&self) -> Arc<dyn SecretStore> {
        self.secrets
            .clone()
            .unwrap_or_else(|| Arc::new(VaultStore::system(KEYCHAIN_SERVICE)))
    }
}

/// The built-in model the settings ask for (`None`: the store's active one).
fn embedded_wanted(c: &TextModelConfig) -> Option<&str> {
    (!c.embedded_model.is_empty()).then_some(c.embedded_model.as_str())
}

/// A model that only fails: what the built-in provider answers when its model is not there.
struct FailingModel(TextModelError);

impl TextModel for FailingModel {
    fn complete_json(
        &self,
        _req: &text_model::JsonRequest,
    ) -> Result<text_model::JsonReply, TextModelError> {
        Err(self.0.clone())
    }
}

/// What identifies the settings a test result belongs to (no key).
fn fingerprint(c: &TextModelConfig) -> String {
    format!(
        "{}|{}|{}",
        c.provider.as_str(),
        c.effective_base_url(),
        c.model
    )
}

/// The URL without any `user:password@` part.
fn display_url(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let (authority, tail) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
            let host = authority.rsplit('@').next().unwrap_or(authority);
            format!("{scheme}://{host}{tail}")
        }
        None => String::new(),
    }
}

/// A setup problem (no provider, bad address) as the shared `invalid_input` error with the
/// `data.reason` clients branch on.
pub fn setup_error(e: TextModelError) -> DaemonError {
    let data = e.error_data();
    DaemonError::WithData {
        error: CmError::invalid_input(e.message),
        data,
    }
}

impl Inner {
    /// The `ai.text.*` settings. With the built-in provider the model and the context are the
    /// resolved model's (the engine really runs with the catalogue's context), when one is
    /// downloaded.
    pub(crate) fn text_model_config(&self) -> Result<TextModelConfig, DaemonError> {
        let mut c = TextModelConfig::from_settings(&self.settings.all()?);
        if c.provider == Provider::Embedded {
            if let Ok(r) = self.ai.text_models.resolve(embedded_wanted(&c)) {
                c.model = r.id;
                c.context_tokens = r.context_tokens.max(512);
            }
        }
        Ok(c)
    }

    /// The checks every use of the text model starts with: a usable provider and, for the
    /// built-in one, a downloaded model (`no_text_model` otherwise).
    pub(crate) fn check_text_model(&self, c: &TextModelConfig) -> Result<(), DaemonError> {
        c.check_endpoint().map_err(setup_error)?;
        if c.provider == Provider::Embedded {
            self.ai
                .text_models
                .resolve(embedded_wanted(c))
                .map_err(|_| setup_error(TextModelError::no_text_model_named(&c.embedded_model)))?;
        }
        Ok(())
    }

    /// Whether a local text model is set up and usable right now (a provider is chosen, its
    /// address is loopback and, for the built-in one, a model is downloaded). What decides
    /// `auto` for a translation into a language Whisper cannot write.
    pub(crate) fn text_model_ready(&self) -> bool {
        self.text_model_config().is_ok_and(|c| {
            c.provider != Provider::None && c.is_local() && self.check_text_model(&c).is_ok()
        })
    }

    /// The model that talks to the built-in engine for `config`.
    fn embedded_client(&self, config: &TextModelConfig) -> Box<dyn TextModel> {
        match self.ai.text_models.resolve(embedded_wanted(config)) {
            Ok(r) => Box::new(EmbeddedTextModel::new(
                self.ai.text_engine.clone(),
                EngineSpec {
                    model_id: r.id,
                    model_path: r.path,
                    ram_mb: r.ram_mb,
                    context_tokens: r.context_tokens.max(512),
                },
                config,
            )),
            Err(_) => Box::new(FailingModel(TextModelError::no_text_model_named(
                &config.embedded_model,
            ))),
        }
    }

    /// The API key by the name in the settings; `None` without a name. Blocking (keychain).
    fn text_model_key(&self, config: &TextModelConfig) -> Result<Option<String>, CmError> {
        match &config.key_name {
            Some(name) => self.text_model.store().get(name),
            None => Ok(None),
        }
    }

    /// The text model for the current settings, key included. Blocking (keychain).
    pub(crate) fn text_model_client(
        &self,
        config: &TextModelConfig,
    ) -> Result<Box<dyn TextModel>, CmError> {
        if let Some(m) = self
            .text_model
            .override_model
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Ok(Box::new(SharedModel(m)));
        }
        if config.provider == Provider::Embedded {
            return Ok(self.embedded_client(config));
        }
        let key = self.text_model_key(config)?;
        Ok(text_model::build(config, key))
    }

    /// What a caller must do before it sends `chars` characters to the text model (AI-7.AC2,
    /// AC3): `Err(no_text_model)` for provider `none`, `Some(plan)` for a remote provider (the
    /// caller makes its action Confirm tier and shows the plan), `None` when it can run at once.
    pub(crate) fn text_model_gate(&self, chars: usize) -> Result<Option<SendPlan>, DaemonError> {
        let config = self.text_model_config()?;
        self.check_text_model(&config)?;
        Ok(config.send_plan(chars))
    }

    /// Whether the app still has to show "Text from your recordings will be sent to <host>" before
    /// the first send (a remote provider whose host the user has not been told about).
    pub(crate) fn text_model_notice_pending(&self) -> Result<bool, DaemonError> {
        let config = self.text_model_config()?;
        let told = self
            .settings
            .all()?
            .get(KEY_NOTICE_HOST)
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(config.is_remote() && told != config.host())
    }

    /// `get_text_model`: the provider, model and limits; never the key.
    pub(crate) async fn get_text_model(&self) -> Result<Value, DaemonError> {
        let c = self.text_model_config()?;
        let key_set = match (&c.key_name, c.provider) {
            (Some(_), Provider::OpenAiCompatible | Provider::Anthropic) => {
                let store = self.text_model.store();
                let name = c.key_name.clone().unwrap_or_default();
                blocking(move || store.get(&name))
                    .await
                    .ok()
                    .flatten()
                    .is_some()
            }
            _ => false,
        };
        let structured = match c.provider {
            Provider::None => Value::Null,
            // Anthropic's tool input is schema-shaped by construction.
            Provider::Anthropic => json!(true),
            _ => self
                .text_model
                .last_test
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .filter(|(f, _)| *f == fingerprint(&c))
                .map_or(Value::Null, |(_, s)| json!(s)),
        };
        Ok(json!({
            "provider": c.provider.as_str(),
            "model": c.model,
            "embedded_model": c.embedded_model,
            "engine": self.ai.text_engine.status(),
            "local": c.is_local(),
            "host": c.host(),
            "base_url": display_url(&c.effective_base_url()),
            "context_tokens": c.context_tokens,
            "timeout_s": c.timeout_s,
            "structured_output": structured,
            "has_key": key_set,
            "remote_notice_pending": self.text_model_notice_pending()?,
        }))
    }

    /// `test_text_model`: one short fixed prompt to the configured provider (and only to it).
    /// A failure is an answer (`ok: false`, `reason`, `message`), not an error, so the AI page
    /// can show it; provider `none` and unusable settings are `invalid_input`.
    pub(crate) async fn test_text_model(&self) -> Result<Value, DaemonError> {
        let c = self.text_model_config()?;
        self.check_text_model(&c)?;
        if c.model.is_empty() {
            return Err(setup_error(TextModelError::invalid_input(
                "Choose a text model on the AI page first",
            )));
        }
        let (cfg, secrets) = (c.clone(), self.text_model.store());
        // The built-in provider needs no key; its client is built here (no keychain, no wait).
        let embedded = (c.provider == Provider::Embedded).then(|| self.embedded_client(&c));
        let outcome = blocking(move || {
            let model = match embedded {
                Some(m) => m,
                None => {
                    let key = match &cfg.key_name {
                        Some(n) => secrets.get(n)?,
                        None => None,
                    };
                    text_model::build(&cfg, key)
                }
            };
            Ok(text_model::test_model(model.as_ref(), &cfg.model))
        })
        .await?;
        if outcome.ok || outcome.reason.as_deref() != Some("invalid_input") {
            *self
                .text_model
                .last_test
                .lock()
                .unwrap_or_else(PoisonError::into_inner) =
                Some((fingerprint(&c), outcome.ok && outcome.structured_output));
        }
        serde_json::to_value(&outcome).map_err(|e| DaemonError::Internal(e.to_string()))
    }
}

impl crate::daemon::Daemon {
    /// What a caller must do before sending `chars` characters of recording text to the text
    /// model ([`Inner::text_model_gate`]): `Err` with `data.reason = "no_text_model"` for provider
    /// `none`, `Some(plan)` for a remote provider (make the action Confirm tier and show
    /// `plan.text`), `None` when it can run at once.
    pub fn text_model_gate(&self, chars: usize) -> Result<Option<SendPlan>, DaemonError> {
        self.inner.text_model_gate(chars)
    }

    /// Whether the first-use notice ("Text from your recordings will be sent to <host>") is
    /// still to be shown. The app records the answer in `ai.text.remote_notice_host`.
    pub fn text_model_notice_pending(&self) -> Result<bool, DaemonError> {
        self.inner.text_model_notice_pending()
    }

    /// Replaces the HTTP text model by `model` (or restores it with `None`). For tests and for
    /// embedding a model that is not behind an HTTP server. The settings are still read: the
    /// provider must not be `none`, and a non-local provider is still refused.
    pub fn set_text_model_override(&self, model: Option<Arc<dyn TextModel>>) {
        *self
            .inner
            .text_model
            .override_model
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = model;
    }

    /// The text model for the current settings, key included. Blocking: call it inside
    /// `spawn_blocking`.
    pub fn text_model_client(&self) -> Result<Box<dyn TextModel>, DaemonError> {
        let config = self.inner.text_model_config()?;
        Ok(self.inner.text_model_client(&config)?)
    }
}

/// Whether `e` is the error of provider `none`.
pub fn is_no_text_model(e: &TextModelError) -> bool {
    e.kind == ErrorKind::NoTextModel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_shown_without_credentials() {
        assert_eq!(
            display_url("https://me:pw@host.example/v1"),
            "https://host.example/v1"
        );
        assert_eq!(
            display_url("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(display_url(""), "");
    }
}
