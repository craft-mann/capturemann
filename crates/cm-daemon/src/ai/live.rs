//! Transcription while an audio recording runs (M2h, setting `ai.transcribe_while_recording`).
//!
//! When an audio recording starts (and the automatic transcription would run after it), a small
//! background task polls the recording's microphone file; every finished piece (about five minutes,
//! cut at a pause, only once the cut is certain) is transcribed with a few Whisper threads, and the
//! state is saved in the bundle (`transcript.live.json`). It is not a queue job, so the rule that
//! holds `transcribe` jobs while a recording runs does not touch it; it never runs beside a screen
//! recording. At Stop the task is told to stop ([`finish`]: the piece in hand is dropped, earlier
//! ones are kept), and the ordinary `transcribe` job picks the saved state up: only the last
//! piece is left. A discarded recording deletes the state; a crash leaves it and the same job
//! uses it on recovery. A pause or a mute needs nothing here: the media file is compacted at a
//! pause and a mute is silence in the file, exactly what the job at Stop reads.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde_json::{Map, Value};

use super::*;

/// Setting: transcribe the pieces of an audio recording while it runs. Default on when
/// `ai.auto_transcribe` is on.
pub const SETTING_TRANSCRIBE_WHILE_RECORDING: &str = "ai.transcribe_while_recording";

/// Hidden setting: how often the file is looked at, in milliseconds (default 30 s; tests shorten it
/// and read the period again after every look, so a test can slow the follower down).
pub const SETTING_LIVE_POLL_MS: &str = "ai.live_poll_ms";

/// Whether the settings ask for it: the user's choice, else on with the automatic transcription.
pub fn live_transcribe_enabled(settings: &Map<String, Value>, ram_gb: Option<u64>) -> bool {
    match settings
        .get(SETTING_TRANSCRIBE_WHILE_RECORDING)
        .and_then(Value::as_bool)
    {
        Some(choice) => choice && auto_transcribe_enabled(settings, ram_gb),
        None => auto_transcribe_enabled(settings, ram_gb),
    }
}

struct Handle {
    stop: Arc<AtomicBool>,
    join: tokio::task::JoinHandle<()>,
}

fn settings_poll(store: &Arc<dyn crate::engines::SettingsStore>) -> Duration {
    let ms = store
        .all()
        .ok()
        .and_then(|m| m.get(SETTING_LIVE_POLL_MS).and_then(Value::as_u64))
        .unwrap_or(30_000);
    Duration::from_millis(ms.max(20))
}

fn registry() -> &'static Mutex<HashMap<String, Handle>> {
    static R: OnceLock<Mutex<HashMap<String, Handle>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Stops following `capture_id` and waits until the task has ended (the piece in hand is dropped).
/// Does nothing when nothing is followed. The `transcribe` job waits for this before it reads the
/// saved state.
pub(super) async fn finish(capture_id: &str) -> bool {
    let Some(h) = registry()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(capture_id)
    else {
        return false;
    };
    h.stop.store(true, Ordering::SeqCst);
    let _ = h.join.await;
    true
}

/// The bundle folder of a recording in progress: the folder under the library root whose
/// `recording.lock` carries this id.
fn find_bundle(
    library: &Arc<std::sync::Mutex<cm_library::Library>>,
    id: &str,
) -> Option<std::path::PathBuf> {
    let root = lock_library(library).root().to_owned();
    std::fs::read_dir(root).ok()?.flatten().find_map(|e| {
        let text = std::fs::read_to_string(e.path().join("recording.lock")).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        (v.get("id").and_then(Value::as_str) == Some(id)).then(|| e.path())
    })
}

impl Inner {
    /// An audio recording started: follows it when the settings and the machine allow.
    pub(super) async fn live_begin(&self, capture_id: &str) -> Result<(), DaemonError> {
        let settings = self.settings.all().unwrap_or_default();
        if !live_transcribe_enabled(&settings, system_ram_gb()) {
            return Ok(());
        }
        // No model, no run, no notice, nothing downloaded.
        let list = self.model_list().await?;
        let model = Self::pick_model(&list, None)?;
        let language = super::jobs::languages_setting(Some(&settings));
        let glossary = cm_ai::glossary::from_value(settings.get(cm_ai::glossary::SETTING_GLOSSARY));
        let pipeline = cm_ai::pipeline::PipelineSettings::from_settings(&settings);
        let (library, id, svc) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.transcribe.clone(),
        );
        let settings_store = self.settings.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let join = tokio::spawn(async move {
            let mut run: Option<cm_ai::recording::live::LiveRun> = None;
            let mut failures = 0;
            let mut bundle: Option<std::path::PathBuf> = None;
            while !flag.load(Ordering::SeqCst) {
                // Sleeps in short slices so that Stop is answered at once; the period is read
                // again each time.
                let period = settings_poll(&settings_store);
                let mut slept = Duration::ZERO;
                while slept < period && !flag.load(Ordering::SeqCst) {
                    let slice = Duration::from_millis(100).min(period - slept);
                    tokio::time::sleep(slice).await;
                    slept += slice;
                }
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                if bundle.is_none() {
                    let (l, i) = (library.clone(), id.clone());
                    bundle = blocking(move || Ok(find_bundle(&l, &i)))
                        .await
                        .ok()
                        .flatten();
                }
                let Some(dir) = bundle.clone() else { continue };
                let req = LiveRequest {
                    bundle_dir: dir,
                    model: model.clone(),
                    language: language.clone(),
                    glossary: glossary.clone(),
                    pipeline,
                };
                let (svc, f) = (svc.clone(), flag.clone());
                let mut taken = run.take();
                let r = tokio::task::spawn_blocking(move || {
                    let r = svc.live_step(&req, &mut taken, &mut |_| !f.load(Ordering::SeqCst));
                    (r, taken)
                })
                .await;
                match r {
                    Ok((Ok(_), back)) => {
                        run = back;
                        failures = 0;
                    }
                    Ok((Err(_), back)) => {
                        run = back;
                        failures += 1;
                        if failures >= 3 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        registry()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(capture_id.to_owned(), Handle { stop, join });
        Ok(())
    }

    /// A recording was discarded (or restarted): stops following it and deletes the saved state.
    pub(super) async fn live_discard(&self, capture_id: &str) {
        let library = self.library.clone();
        let id = capture_id.to_owned();
        finish(capture_id).await;
        let dir = blocking(move || Ok(find_bundle(&library, &id)))
            .await
            .ok()
            .flatten();
        if let Some(d) = dir {
            cm_ai::recording::live::remove_live_state(&d);
        }
    }
}
