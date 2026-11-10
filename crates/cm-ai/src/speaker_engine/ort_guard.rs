//! One ONNX Runtime environment for the whole process, and one lock around every creation and
//! teardown of a sherpa-onnx object.
//!
//! Every sherpa-onnx model (segmentation, voice prints) takes a reference on ONNX Runtime's
//! process-wide environment. With one object at a time, each window of a long recording created
//! the environment and destroyed it again; on Nov 10, 2026 the destruction of the last reference
//! (`LoggingManager::~LoggingManager`, a C++ exception out of a destructor, so `std::terminate`)
//! aborted the app in the 76 % of a 4 h 13 min run. The abort cannot be caught, so the environment
//! must never reach zero references: the first object created leaks a small keep-alive extractor
//! (the voice-print model, about 28 MB) that holds a reference until the process ends. Creation
//! and destruction of objects also run under one lock, so two jobs never overlap in environment or
//! session setup and teardown. Running (`process`) is not under the lock.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use sherpa_onnx::{SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig};

static LOCK: Mutex<()> = Mutex::new(());
static KEEPERS: AtomicUsize = AtomicUsize::new(0);

/// Keep-alive objects created in this process (1 once a model was loaded, never more).
pub(crate) fn keepers() -> usize {
    KEEPERS.load(Ordering::SeqCst)
}

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Hold the environment open: called with the lock held, before the first object is created.
fn ensure_keeper(embedding_model: &Path) {
    if KEEPERS.load(Ordering::SeqCst) > 0 {
        return;
    }
    let Some(model) = embedding_model.to_str().map(str::to_owned) else {
        return;
    };
    let keeper = SpeakerEmbeddingExtractor::create(&SpeakerEmbeddingExtractorConfig {
        model: Some(model),
        num_threads: 1,
        ..Default::default()
    });
    if let Some(keeper) = keeper {
        // Never dropped: the environment lives until the process ends.
        std::mem::forget(keeper);
        KEEPERS.fetch_add(1, Ordering::SeqCst);
    }
}

/// Create a sherpa-onnx object under the lock, with the environment held open.
pub(crate) fn create<T>(embedding_model: &Path, make: impl FnOnce() -> Option<T>) -> Option<T> {
    let _guard = lock();
    ensure_keeper(embedding_model);
    make()
}

/// Drop a sherpa-onnx object under the lock.
pub(crate) fn destroy<T>(value: T) {
    let _guard = lock();
    drop(value);
}
