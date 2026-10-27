//! The speaker models that ship inside the app (AI-8): treated as installed, user copy wins,
//! never removable, refused when the file does not match its pin. The real models are 36 MB; these
//! tests pin small stand-in files, the rules are the same.

use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::model::{sha256_hex, Catalog, ModelStore};

struct NoEngine;

impl TurnEngine for NoEngine {
    fn run(&self, _: &SpeakerModels, _: &[f32], _: ClusterParams) -> Result<Vec<Turn>, CmError> {
        Ok(Vec::new())
    }
}

const SEG: &[u8] = b"segmentation bytes";
const EMB: &[u8] = b"embedding bytes";

/// A catalogue of the two speaker rows pinned to `SEG` and `EMB`, and a store whose bundled folder
/// holds the given files.
fn bundled_store(root: &Path, bundled: &Path, seg: &[u8], emb: &[u8]) -> ModelStore {
    let pins = root.join("pins");
    std::fs::create_dir_all(&pins).unwrap();
    let row = |id: &str, file: &str, bytes: &[u8]| {
        let p = pins.join(file);
        std::fs::write(&p, bytes).unwrap();
        serde_json::json!({
            "id": id, "kind": "speaker", "label": id, "file": file,
            "url": "https://example.invalid/x", "license": "MIT",
            "sha256": sha256_hex(&p).unwrap(), "bytes": bytes.len()
        })
    };
    let catalog = Catalog::parse(
        &serde_json::json!({ "models": [
            row(SEGMENTATION_ID, "seg.onnx", SEG),
            row(EMBEDDING_ID, "emb.onnx", EMB),
        ]})
        .to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(bundled).unwrap();
    std::fs::write(bundled.join("seg.onnx"), seg).unwrap();
    std::fs::write(bundled.join("emb.onnx"), emb).unwrap();
    ModelStore::with_catalog(root.join("store"), catalog).with_bundled_dir(Some(bundled.into()))
}

#[test]
fn bundled_speaker_models_count_as_installed_and_cannot_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    let store = bundled_store(dir.path(), &app, SEG, EMB);
    assert!(store.is_installed(SEGMENTATION_ID) && store.is_bundled(EMBEDDING_ID));
    let list = store.list();
    assert!(list.models.iter().all(|m| m.installed && m.bundled));
    let models = speaker_models(&store).expect("both bundled files are usable");
    assert_eq!(models.segmentation, app.join("seg.onnx"));
    let d = SherpaDiarizer::with_engine(store.clone(), Arc::new(NoEngine));
    d.ready().unwrap();
    // Nothing to remove: a clear refusal, the file stays.
    let e = store.remove(SEGMENTATION_ID).unwrap_err();
    assert!(e.to_string().contains("bundled"), "{e}");
    assert!(app.join("seg.onnx").is_file());
}

#[test]
fn a_user_copy_wins_over_the_bundled_one_and_removing_deletes_only_the_copy() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    let store = bundled_store(dir.path(), &app, SEG, EMB);
    std::fs::create_dir_all(store.models_dir()).unwrap();
    let copy = store.models_dir().join("seg.onnx");
    std::fs::write(&copy, b"user copy").unwrap();
    assert_eq!(store.path_of(SEGMENTATION_ID), Some(copy.clone()));
    assert!(store.remove(SEGMENTATION_ID).unwrap());
    assert!(!copy.exists());
    assert_eq!(store.path_of(SEGMENTATION_ID), Some(app.join("seg.onnx")));
}

#[test]
fn a_bundled_file_that_does_not_match_its_pin_is_refused_with_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    let store = bundled_store(dir.path(), &app, SEG, b"damaged!!!!!!!!");
    assert!(!store.is_installed(EMBEDDING_ID));
    assert!(store.bundled_problem(EMBEDDING_ID).is_some());
    assert!(!store.list().models.iter().all(|m| m.installed));
    let d = SherpaDiarizer::with_engine(store, Arc::new(NoEngine));
    let e = d.ready().unwrap_err();
    assert!(!crate::speakers::is_no_speaker_model(&e));
    assert!(e.to_string().contains("damaged"), "{e}");
}

#[test]
fn without_a_bundled_folder_nothing_is_bundled() {
    let dir = tempfile::tempdir().unwrap();
    let store = bundled_store(dir.path(), &dir.path().join("app"), SEG, EMB).with_bundled_dir(None);
    assert!(!store.is_installed(SEGMENTATION_ID));
    assert!(store.list().models.iter().all(|m| !m.bundled));
}
