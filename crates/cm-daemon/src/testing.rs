//! Fakes for tests (this crate's and downstream ones): scripted confirmers, a recording
//! clipboard, a recording external-actions engine.

pub mod fake_ai;
pub mod fake_sources;
pub mod fake_text;

pub use cm_render::ocr::{ScriptedRecognizer, TextLine};
pub use fake_ai::FakeAi;
pub use fake_sources::{FakeSources, FakeTrackHandle, LastDevices};
pub use fake_text::{FakeTextEngine, FakeTextModels};

use async_trait::async_trait;
use cm_core::{Actor, CmError};
use serde_json::{json, Value};
use std::sync::{Mutex, PoisonError};

use crate::action::Action;
use crate::engines::{Clipboard, ExternalActions};
use crate::error::DaemonError;
use crate::gate::{ConfirmationRequest, Confirmer, Decision};

/// Answers every dialog with the same decision and remembers what it was shown.
#[derive(Debug)]
pub struct FixedConfirmer {
    decision: Decision,
    shown: Mutex<Vec<ConfirmationRequest>>,
}

impl FixedConfirmer {
    pub fn new(decision: Decision) -> Self {
        FixedConfirmer {
            decision,
            shown: Mutex::new(Vec::new()),
        }
    }

    pub fn shown(&self) -> Vec<ConfirmationRequest> {
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl Confirmer for FixedConfirmer {
    async fn confirm(&self, request: &ConfirmationRequest) -> Decision {
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        self.decision
    }
}

/// Lets a test keep a handle on the confirmer it gave the daemon (to read what was shown).
#[async_trait]
impl Confirmer for std::sync::Arc<FixedConfirmer> {
    async fn confirm(&self, request: &ConfirmationRequest) -> Decision {
        self.as_ref().confirm(request).await
    }
}

/// Never answers (the user walked away): drives the timeout path.
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverAnswers;

#[async_trait]
impl Confirmer for NeverAnswers {
    async fn confirm(&self, _request: &ConfirmationRequest) -> Decision {
        std::future::pending().await
    }
}

/// Keeps the last PNG it was given.
#[derive(Debug, Default)]
pub struct RecordingClipboard {
    last: Mutex<Option<Vec<u8>>>,
}

impl RecordingClipboard {
    pub fn last_png(&self) -> Option<Vec<u8>> {
        self.last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Clipboard for std::sync::Arc<RecordingClipboard> {
    fn copy_png(&self, png: &[u8]) -> Result<(), CmError> {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) = Some(png.to_vec());
        Ok(())
    }
}

/// Supports the three Confirm-tier actions and counts how often each ran (so tests can show
/// "nothing happened before the user clicked", AGT-2.AC1, and idempotency, AGT-3.AC4).
#[derive(Debug, Default)]
pub struct CountingExternal {
    calls: Mutex<Vec<(String, Actor)>>,
}

impl CountingExternal {
    pub fn calls(&self) -> Vec<(String, Actor)> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl ExternalActions for std::sync::Arc<CountingExternal> {
    fn supports(&self, _method: &str) -> bool {
        true
    }

    async fn execute(&self, actor: &Actor, action: &Action) -> Result<Value, DaemonError> {
        let n = {
            let mut g = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
            g.push((action.method().to_owned(), actor.clone()));
            g.len()
        };
        Ok(json!({ "ok": true, "call": n }))
    }
}

/// A [`crate::Trasher`] that never touches the real Trash: it moves the folder into `dest` (or
/// deletes it when there is none) and remembers what it was asked to trash.
#[derive(Debug, Default)]
pub struct FakeTrasher {
    dest: Option<std::path::PathBuf>,
    trashed: Mutex<Vec<std::path::PathBuf>>,
    fail: std::sync::atomic::AtomicBool,
}

impl FakeTrasher {
    /// Deletes the folder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the folder into `dest` (an existing directory), like the Trash would.
    pub fn into_dir(dest: impl Into<std::path::PathBuf>) -> Self {
        FakeTrasher {
            dest: Some(dest.into()),
            ..Self::default()
        }
    }

    /// Makes every later call fail (the Trash is unavailable).
    pub fn fail_from_now_on(&self) {
        self.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The folders it was asked to trash, in order.
    pub fn trashed(&self) -> Vec<std::path::PathBuf> {
        self.trashed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl crate::Trasher for FakeTrasher {
    fn trash(&self, path: &std::path::Path) -> Result<(), CmError> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(CmError::internal("the Trash is not available"));
        }
        let io = |e: std::io::Error| CmError::internal(format!("fake trash: {e}"));
        match &self.dest {
            Some(dest) => {
                let name = path.file_name().unwrap_or_default();
                std::fs::rename(path, dest.join(name)).map_err(io)?;
            }
            None => std::fs::remove_dir_all(path).map_err(io)?,
        }
        self.trashed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path.to_owned());
        Ok(())
    }
}

/// Lets a test keep a handle on the trasher it gave the daemon.
impl crate::Trasher for std::sync::Arc<FakeTrasher> {
    fn trash(&self, path: &std::path::Path) -> Result<(), CmError> {
        self.as_ref().trash(path)
    }
}
