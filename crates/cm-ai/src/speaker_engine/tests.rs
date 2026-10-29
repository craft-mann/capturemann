use std::sync::Mutex;

use cm_core::MediaTime;

use super::*;
use crate::model::{Catalog, ModelStore};

fn turn(start: f64, end: f64, speaker: i32) -> Turn {
    Turn {
        start,
        end,
        speaker,
    }
}

/// An engine that answers from a list: the first answer for an automatic run, the second for an
/// exact count. It records every call.
struct Scripted {
    auto: Vec<Turn>,
    exact: Vec<Turn>,
    calls: Mutex<Vec<ClusterParams>>,
}

impl Scripted {
    fn new(auto: Vec<Turn>, exact: Vec<Turn>) -> Scripted {
        Scripted {
            auto,
            exact,
            calls: Mutex::new(Vec::new()),
        }
    }
    fn calls(&self) -> Vec<ClusterParams> {
        self.calls.lock().unwrap().clone()
    }
}

impl TurnEngine for Scripted {
    fn run(&self, _: &SpeakerModels, _: &[f32], p: ClusterParams) -> Result<Vec<Turn>, CmError> {
        self.calls.lock().unwrap().push(p);
        Ok(if p.num_clusters.is_some() {
            self.exact.clone()
        } else {
            self.auto.clone()
        })
    }
}

fn models() -> SpeakerModels {
    SpeakerModels {
        segmentation: "seg.onnx".into(),
        embedding: "emb.onnx".into(),
    }
}

#[test]
fn an_exact_number_of_speakers_is_passed_to_the_engine_once() {
    let e = Scripted::new(vec![], vec![turn(0.0, 5.0, 0), turn(5.0, 9.0, 1)]);
    let hints = DiarizeHints {
        num_speakers: Some(2),
        ..Default::default()
    };
    let out = run_with_hints(&e, &models(), &[0.0; 16], &hints).unwrap();
    assert_eq!(out.len(), 2);
    let calls = e.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].num_clusters, Some(2));
}

#[test]
fn an_automatic_run_uses_the_high_threshold_and_folds_tiny_clusters() {
    // Three real voices and two crumbs of 1 s each.
    let auto = vec![
        turn(0.0, 40.0, 0),
        turn(40.0, 60.0, 1),
        turn(60.0, 61.0, 7),
        turn(61.0, 90.0, 0),
        turn(90.0, 91.0, 9),
        turn(91.0, 100.0, 1),
    ];
    let e = Scripted::new(auto, vec![]);
    let out = run_with_hints(&e, &models(), &[0.0; 16], &DiarizeHints::default()).unwrap();
    let calls = e.calls();
    assert_eq!(calls.len(), 1, "no second pass inside the allowed range");
    assert_eq!(calls[0].num_clusters, None);
    assert!((calls[0].threshold - AUTO_THRESHOLD).abs() < f32::EPSILON);
    let mut ids: Vec<i32> = out.iter().map(|t| t.speaker).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids, vec![0, 1], "the crumbs 7 and 9 were folded");
}

#[test]
fn too_many_automatic_speakers_cause_one_more_pass_with_the_maximum() {
    // 12 voices of 10 s each, none small.
    let auto: Vec<Turn> = (0..12)
        .map(|i| turn(f64::from(i) * 10.0, f64::from(i) * 10.0 + 10.0, i))
        .collect();
    let e = Scripted::new(auto, vec![turn(0.0, 50.0, 0), turn(50.0, 99.0, 1)]);
    let hints = DiarizeHints {
        max_speakers: Some(2),
        ..Default::default()
    };
    let out = run_with_hints(&e, &models(), &[0.0; 16], &hints).unwrap();
    let calls = e.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].num_clusters, Some(2));
    assert_eq!(out.len(), 2);
}

#[test]
fn fewer_automatic_speakers_than_the_minimum_cause_one_more_pass() {
    let e = Scripted::new(
        vec![turn(0.0, 60.0, 0)],
        vec![turn(0.0, 30.0, 0), turn(30.0, 60.0, 1)],
    );
    let hints = DiarizeHints {
        min_speakers: Some(2),
        ..Default::default()
    };
    let out = run_with_hints(&e, &models(), &[0.0; 16], &hints).unwrap();
    assert_eq!(e.calls().len(), 2);
    assert_eq!(out.len(), 2);
}

#[test]
fn a_flicker_inside_one_speakers_talk_goes_to_that_speaker_and_the_turns_join() {
    let out = tidy_turns(vec![
        turn(0.0, 10.0, 0),
        turn(10.1, 10.4, 1), // 0.3 s flicker between two turns of speaker 0
        turn(10.5, 20.0, 0),
    ]);
    assert_eq!(out, vec![turn(0.0, 20.0, 0)]);
}

#[test]
fn a_short_turn_between_two_different_speakers_stays() {
    let out = tidy_turns(vec![
        turn(0.0, 10.0, 0),
        turn(10.1, 10.4, 1),
        turn(10.5, 20.0, 2),
    ]);
    assert_eq!(out.len(), 3);
}

#[test]
fn a_short_answer_after_a_pause_is_not_a_flicker() {
    let out = tidy_turns(vec![
        turn(0.0, 10.0, 0),
        turn(13.0, 13.4, 1), // far from both neighbours
        turn(16.0, 20.0, 0),
    ]);
    assert_eq!(out.len(), 3);
}

#[test]
fn overlapping_turns_of_different_speakers_are_kept() {
    let out = tidy_turns(vec![turn(0.0, 10.0, 0), turn(8.0, 14.0, 1)]);
    assert_eq!(out, vec![turn(0.0, 10.0, 0), turn(8.0, 14.0, 1)]);
}

#[test]
fn same_speaker_turns_closer_than_the_gap_are_joined_and_empty_turns_dropped() {
    let out = tidy_turns(vec![
        turn(0.0, 5.0, 3),
        turn(5.4, 8.0, 3),
        turn(9.0, 9.0, 3),
        turn(8.4, 12.0, 3),
    ]);
    assert_eq!(out, vec![turn(0.0, 12.0, 3)]);
}

#[test]
fn folding_changes_nothing_when_every_speaker_is_small() {
    let t = vec![turn(0.0, 2.0, 0), turn(2.0, 3.0, 1)];
    assert_eq!(fold_small_speakers(t.clone()), t);
}

fn store_with(dir: &std::path::Path, install: &[&str]) -> ModelStore {
    let store = ModelStore::with_catalog(dir, Catalog::builtin());
    std::fs::create_dir_all(store.models_dir()).unwrap();
    for id in install {
        let entry = store.catalog().get(id).unwrap();
        std::fs::write(store.models_dir().join(&entry.file), b"onnx").unwrap();
    }
    store
}

#[test]
fn without_both_model_files_the_diarizer_answers_no_speaker_model() {
    for installed in [&[][..], &[SEGMENTATION_ID][..], &[EMBEDDING_ID][..]] {
        let dir = tempfile::tempdir().unwrap();
        let d = SherpaDiarizer::with_engine(
            store_with(dir.path(), installed),
            Arc::new(Scripted::new(vec![], vec![])),
        );
        let e = d.ready().unwrap_err();
        assert!(crate::speakers::is_no_speaker_model(&e), "{installed:?}");
        let audio = Pcm16k {
            samples: &[0.0; 32_000],
            start: MediaTime::ZERO,
        };
        let e = d
            .diarize(audio, &DiarizeHints::default(), &mut |_| true)
            .unwrap_err();
        assert!(crate::speakers::is_no_speaker_model(&e));
    }
}

#[test]
fn with_both_files_it_runs_and_shifts_the_times_by_the_audio_start() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(dir.path(), &[SEGMENTATION_ID, EMBEDDING_ID]);
    let engine = Arc::new(Scripted::new(
        vec![],
        vec![turn(0.0, 2.0, 5), turn(2.0, 4.0, 6)],
    ));
    let d = SherpaDiarizer::with_engine(store, engine.clone());
    d.ready().unwrap();
    let audio = Pcm16k {
        samples: &[0.0; 64_000],
        start: MediaTime::from_secs(10),
    };
    let hints = DiarizeHints {
        num_speakers: Some(2),
        ..Default::default()
    };
    let mut seen = Vec::new();
    let out = d
        .diarize(audio, &hints, &mut |p| {
            seen.push(p);
            true
        })
        .unwrap();
    assert_eq!(seen, vec![0.0, 1.0]);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].start, MediaTime::from_secs(10));
    assert_eq!(out[1].end, MediaTime::from_secs(14));
    assert_eq!((out[0].speaker, out[1].speaker), (5, 6));
    assert_eq!(d.model_id(), MODEL_ID);
}

#[test]
fn a_cancel_is_noticed_before_and_after_the_engine_call() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(dir.path(), &[SEGMENTATION_ID, EMBEDDING_ID]);
    let engine = Arc::new(Scripted::new(vec![], vec![turn(0.0, 2.0, 0)]));
    let d = SherpaDiarizer::with_engine(store, engine.clone());
    let audio = || Pcm16k {
        samples: &[0.0; 64_000],
        start: MediaTime::ZERO,
    };
    let hints = DiarizeHints {
        num_speakers: Some(1),
        ..Default::default()
    };
    let e = d.diarize(audio(), &hints, &mut |_| false).unwrap_err();
    assert!(crate::transcribe::is_cancelled(&e));
    assert!(
        engine.calls().is_empty(),
        "no engine call after an early cancel"
    );
    let mut n = 0;
    let e = d
        .diarize(audio(), &hints, &mut |_| {
            n += 1;
            n < 2
        })
        .unwrap_err();
    assert!(crate::transcribe::is_cancelled(&e));
}

#[test]
fn the_built_in_catalogue_has_both_speaker_rows_pinned() {
    let c = Catalog::builtin();
    c.check().unwrap();
    let seg = c.get(SEGMENTATION_ID).unwrap();
    let emb = c.get(EMBEDDING_ID).unwrap();
    assert!(seg.import_only && seg.url.is_empty());
    assert!(!emb.import_only && emb.url.starts_with("https://"));
    assert_eq!(seg.license, "MIT");
    assert_eq!(emb.license, "Apache-2.0");
    assert_eq!(seg.bytes, 5_992_913);
    assert_eq!(emb.bytes, 28_281_164);
    assert_eq!(
        seg.sha256,
        "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079"
    );
    assert_eq!(
        emb.sha256,
        "aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2"
    );
}

#[test]
fn speaker_models_never_become_the_speech_model_and_are_refused_by_transcribe() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(dir.path(), &[SEGMENTATION_ID, EMBEDDING_ID]);
    let list = store.list();
    assert!(list.active.is_none());
    assert!(list
        .models
        .iter()
        .filter(|m| m.kind == crate::model::ModelKind::Speaker)
        .all(|m| m.installed && !m.active));
    assert!(store.resolve(None).is_err(), "no speech model installed");
    assert!(store.resolve(Some(EMBEDDING_ID)).is_err());
    assert!(store.select(EMBEDDING_ID).is_err());
}

#[test]
fn a_speaker_model_is_imported_only_when_it_is_the_pinned_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModelStore::with_catalog(dir.path().join("store"), Catalog::builtin());
    let wrong = dir.path().join("model.onnx");
    std::fs::write(&wrong, vec![1u8; 5_992_913]).unwrap();
    let e = store.import(SEGMENTATION_ID, &wrong).unwrap_err();
    assert!(e.to_string().contains("expects"), "{e}");
    assert!(!store.is_installed(SEGMENTATION_ID));
    // A download of an import-only row is refused with the way out.
    let e = store.download(SEGMENTATION_ID, &mut |_| true).unwrap_err();
    assert!(e.to_string().contains("import"), "{e}");
}
