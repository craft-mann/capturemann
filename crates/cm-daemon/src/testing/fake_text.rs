//! A scripted built-in text engine (AI-7, provider `embedded`): a text-model store that
//! "downloads" in steps and an engine that hands out a fixed address (a scripted server in the
//! test) or a scripted failure. Nothing here starts a process or reads a file of a real model.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cm_ai::model::{DownloadProgress, ModelInfo};
use cm_ai::text_model::engine::{EngineSpec, EngineState, EngineStatus, TextEngine};
use cm_ai::text_model::store::{ResolvedTextModel, TextModelInfo, TextModelList};
use cm_ai::text_model::{ErrorKind, TextModelError};
use cm_core::CmError;

use crate::ai::TextModelService;

struct Entry {
    id: &'static str,
    label: &'static str,
    size_mb: u32,
    ram_mb: u32,
    tier_gb: u32,
    installed: bool,
}

struct State {
    models: Vec<Entry>,
    active: Option<String>,
    download_steps: u32,
    step_ms: u64,
    downloads_started: Vec<String>,
}

/// The fake text-model store: three entries for the 8, 16 and 32 GB tiers, none installed.
#[derive(Clone)]
pub struct FakeTextModels(Arc<Mutex<State>>);

impl Default for FakeTextModels {
    fn default() -> Self {
        FakeTextModels::new()
    }
}

impl FakeTextModels {
    pub fn new() -> FakeTextModels {
        let e = |id, label, size_mb, ram_mb, tier_gb| Entry {
            id,
            label,
            size_mb,
            ram_mb,
            tier_gb,
            installed: false,
        };
        FakeTextModels(Arc::new(Mutex::new(State {
            models: vec![
                e("text-8gb", "Small test model", 2500, 4000, 8),
                e("text-16gb", "Medium test model", 5000, 8000, 16),
                e("text-32gb", "Large test model", 9000, 16000, 32),
            ],
            active: None,
            download_steps: 0,
            step_ms: 0,
            downloads_started: Vec::new(),
        })))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// These models are installed; the first is active.
    pub fn with_installed(self, ids: &[&str]) -> FakeTextModels {
        {
            let mut s = self.state();
            for m in &mut s.models {
                m.installed = ids.contains(&m.id);
            }
            s.active = ids.first().map(|i| (*i).to_owned());
        }
        self
    }

    /// Downloads take `steps` steps of `step_ms` milliseconds (so a test can cancel one).
    pub fn slow_downloads(self, steps: u32, step_ms: u64) -> FakeTextModels {
        {
            let mut s = self.state();
            s.download_steps = steps;
            s.step_ms = step_ms;
        }
        self
    }

    pub fn downloads_started(&self) -> Vec<String> {
        self.state().downloads_started.clone()
    }

    pub fn installed(&self) -> Vec<String> {
        self.state()
            .models
            .iter()
            .filter(|m| m.installed)
            .map(|m| m.id.to_owned())
            .collect()
    }
}

impl TextModelService for FakeTextModels {
    fn list(&self) -> Result<TextModelList, CmError> {
        let s = self.state();
        Ok(TextModelList {
            models: s
                .models
                .iter()
                .map(|m| TextModelInfo {
                    info: ModelInfo {
                        id: m.id.to_owned(),
                        label: m.label.to_owned(),
                        size_mb: m.size_mb,
                        ram_mb: m.ram_mb,
                        hint: String::new(),
                        installed: m.installed,
                        active: m.installed && s.active.as_deref() == Some(m.id),
                        path: m.installed.then(|| format!("/text-models/{}.gguf", m.id)),
                    },
                    tier_gb: m.tier_gb,
                    license: "Apache-2.0".to_owned(),
                    context_tokens: 8192,
                })
                .collect(),
            active: s.active.clone(),
            dir: "/text-models".to_owned(),
        })
    }

    fn download(
        &self,
        model: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError> {
        let (steps, step_ms) = {
            let mut s = self.state();
            s.downloads_started.push(model.to_owned());
            (s.download_steps.max(1), s.step_ms)
        };
        for i in 1..=steps {
            if step_ms > 0 {
                std::thread::sleep(Duration::from_millis(step_ms));
            }
            let keep_going = progress(DownloadProgress {
                model: model.to_owned(),
                bytes_done: u64::from(i),
                bytes_total: u64::from(steps),
            });
            if !keep_going {
                return Ok(false);
            }
        }
        let mut s = self.state();
        if let Some(m) = s.models.iter_mut().find(|m| m.id == model) {
            m.installed = true;
        }
        if s.active.is_none() {
            s.active = Some(model.to_owned());
        }
        Ok(true)
    }

    fn select(&self, model: &str) -> Result<(), CmError> {
        self.state().active = Some(model.to_owned());
        Ok(())
    }

    fn remove(&self, model: &str) -> Result<(), CmError> {
        let mut s = self.state();
        if let Some(m) = s.models.iter_mut().find(|m| m.id == model) {
            m.installed = false;
        }
        if s.active.as_deref() == Some(model) {
            s.active = None;
        }
        Ok(())
    }

    fn resolve(&self, requested: Option<&str>) -> Result<ResolvedTextModel, CmError> {
        let s = self.state();
        let id = requested
            .map(str::to_owned)
            .or_else(|| s.active.clone())
            .ok_or_else(|| cm_ai::text_model::store::no_text_model(None))?;
        s.models
            .iter()
            .find(|m| m.id == id && m.installed)
            .map(|m| ResolvedTextModel {
                id: m.id.to_owned(),
                path: PathBuf::from(format!("/text-models/{}.gguf", m.id)),
                ram_mb: m.ram_mb,
                context_tokens: 8192,
            })
            .ok_or_else(|| cm_ai::text_model::store::no_text_model(Some(&id)))
    }
}

struct EngineInner {
    /// The address to hand out (`http://127.0.0.1:<port>/v1` of a scripted server).
    url: String,
    fail: Option<ErrorKind>,
    specs: Vec<EngineSpec>,
    in_flight: u32,
    stops: u32,
    running_model: Option<String>,
}

/// The fake engine: hands out [`FakeTextEngine::new`]'s address, or a scripted failure.
#[derive(Clone)]
pub struct FakeTextEngine(Arc<Mutex<EngineInner>>);

impl FakeTextEngine {
    pub fn new(url: impl Into<String>) -> FakeTextEngine {
        FakeTextEngine(Arc::new(Mutex::new(EngineInner {
            url: url.into(),
            fail: None,
            specs: Vec::new(),
            in_flight: 0,
            stops: 0,
            running_model: None,
        })))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, EngineInner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// From now on `acquire` fails with this kind (`None`: works again).
    pub fn fail_with(&self, kind: Option<ErrorKind>) {
        self.state().fail = kind;
    }

    /// Every spec the engine was asked to run.
    pub fn specs(&self) -> Vec<EngineSpec> {
        self.state().specs.clone()
    }

    /// How many times `stop` was called.
    pub fn stops(&self) -> u32 {
        self.state().stops
    }

    /// Pretends a request is running (so removing its model is `busy`).
    pub fn set_in_flight(&self, n: u32) {
        self.state().in_flight = n;
    }
}

impl TextEngine for FakeTextEngine {
    fn acquire(&self, spec: &EngineSpec) -> Result<String, TextModelError> {
        let mut s = self.state();
        s.specs.push(spec.clone());
        if let Some(kind) = s.fail {
            let message = match kind {
                ErrorKind::EngineMissing => "The text engine was not found (scripted)",
                ErrorKind::NotEnoughMemory => "Not enough free memory (scripted)",
                _ => "The text engine failed (scripted)",
            };
            return Err(TextModelError {
                kind,
                message: message.to_owned(),
            });
        }
        s.running_model = Some(spec.model_id.clone());
        Ok(s.url.clone())
    }

    fn release(&self) {}

    fn status(&self) -> EngineStatus {
        let s = self.state();
        EngineStatus {
            state: if s.running_model.is_some() {
                EngineState::Running
            } else {
                EngineState::Stopped
            },
            model: s.running_model.clone(),
            port: s.running_model.as_ref().map(|_| 1),
            in_flight: s.in_flight,
            restarts: 0,
            helper: None,
            reason: None,
            error: None,
            idle_stop_s: 600,
        }
    }

    fn stop(&self) {
        let mut s = self.state();
        s.stops += 1;
        s.running_model = None;
    }

    fn reap_idle(&self) -> bool {
        false
    }
}
