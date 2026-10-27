//! `get_text_model` and `test_text_model` (AI-7), and the helpers the later callers (AI-5
//! translation, AI-9 reports, AI-2 summaries) use to reach the text model.
//!
//! The settings are `ai.text.*` ([`cm_ai::text_model`]); they are app-only (`ai.*` is on the
//! [`crate::engines::APP_ONLY_SETTINGS`] list, so `set_settings` from an agent is refused). The
//! only text model is the built-in engine (provider `embedded`): no API key, no server address.
//!
//! # For callers
//!
//! * [`Inner::text_model_config`] reads the settings, [`Inner::text_model_client`] builds the
//!   [`TextModel`].
//! * [`Inner::text_model_check`] is the one place that decides whether text may be sent: provider
//!   `none` (or an embedded model that is not downloaded) is `invalid_input` with
//!   `data.reason = "no_text_model"`. Text goes only to the helper on this computer, so there is
//!   nothing to confirm and the callers are Free.

use std::sync::{Arc, Mutex, PoisonError};

use cm_ai::text_model::engine::{EmbeddedTextModel, EngineSpec};
use cm_ai::text_model::{self, ErrorKind, Provider, TextModel, TextModelConfig, TextModelError};
use cm_core::CmError;
use serde_json::{json, Value};

use crate::daemon::{blocking, Inner};
use crate::error::DaemonError;

/// The action names of this module.
pub const TEXT_MODEL_METHODS: &[&str] = &["get_text_model", "test_text_model"];

/// The last test result and the test override.
pub struct TextModelState {
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

    fn complete_json_streaming(
        &self,
        req: &text_model::JsonRequest,
        on_progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<text_model::JsonReply, TextModelError> {
        self.0.complete_json_streaming(req, on_progress)
    }
}

impl TextModelState {
    pub fn new() -> Self {
        TextModelState {
            last_test: Mutex::new(None),
            override_model: Mutex::new(None),
        }
    }
}

impl Default for TextModelState {
    fn default() -> Self {
        Self::new()
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

/// What identifies the settings a test result belongs to .
fn fingerprint(c: &TextModelConfig) -> String {
    format!("{}|{}", c.provider.as_str(), c.model)
}

/// A setup problem (no provider, no model) as the shared `invalid_input` error with the
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
            // A model id the catalogue no longer has (an earlier version's `text-8gb`, a hand
            // edit) reads as "no model selected": the engine is off, nothing crashes.
            if !c.embedded_model.is_empty()
                && !self
                    .ai
                    .text_models
                    .list()
                    .is_ok_and(|l| l.models.iter().any(|m| m.info.id == c.embedded_model))
            {
                c.provider = Provider::None;
                c.embedded_model.clear();
            }
        }
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

    /// Whether the text model is set up and usable right now (the built-in provider is chosen and
    /// a model is downloaded). What decides
    /// `auto` for a translation into a language Whisper cannot write.
    pub(crate) fn text_model_ready(&self) -> bool {
        self.text_model_config()
            .is_ok_and(|c| c.provider != Provider::None && self.check_text_model(&c).is_ok())
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

    /// The text model for the current settings.
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
        Ok(text_model::build(config))
    }

    /// Whether text may be sent to the text model (AI-7.AC2): `Err(no_text_model)` for provider
    /// `none` or a built-in model that is not downloaded. Text only ever goes to the helper on
    /// this computer, so there is no plan to confirm.
    pub(crate) fn text_model_check(&self) -> Result<(), DaemonError> {
        let config = self.text_model_config()?;
        self.check_text_model(&config)
    }

    /// `get_text_model`: the provider, model and limits.
    pub(crate) async fn get_text_model(&self) -> Result<Value, DaemonError> {
        let c = self.text_model_config()?;
        let structured = match c.provider {
            Provider::None => Value::Null,
            Provider::Embedded => self
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
            "context_tokens": c.context_tokens,
            "timeout_s": c.timeout_s,
            "structured_output": structured,
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
        let cfg = c.clone();
        let model = match self.text_model_client(&c) {
            Ok(m) => m,
            Err(e) => return Err(e.into()),
        };
        let outcome =
            blocking(move || Ok(text_model::test_model(model.as_ref(), &cfg.model))).await?;
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
    /// Whether text may be sent to the text model ([`Inner::text_model_check`]): `Err` with
    /// `data.reason = "no_text_model"` for provider `none` or a model that is not downloaded.
    pub fn text_model_check(&self) -> Result<(), DaemonError> {
        self.inner.text_model_check()
    }

    /// Replaces the text model by `model` (or restores it with `None`). For tests and for
    /// embedding a model that is not behind an HTTP server. The settings are still read: the
    /// provider must not be `none`.
    pub fn set_text_model_override(&self, model: Option<Arc<dyn TextModel>>) {
        *self
            .inner
            .text_model
            .override_model
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = model;
    }

    /// The text model for the current settings.
    pub fn text_model_client(&self) -> Result<Box<dyn TextModel>, DaemonError> {
        let config = self.inner.text_model_config()?;
        Ok(self.inner.text_model_client(&config)?)
    }
}

/// Whether `e` is the error of provider `none`.
pub fn is_no_text_model(e: &TextModelError) -> bool {
    e.kind == ErrorKind::NoTextModel
}
