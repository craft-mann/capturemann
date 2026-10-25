//! R1-4: ops and history written by a newer build at the same schema version survive an edit
//! by this build.
#![allow(clippy::unwrap_used)]
mod common;
use cm_core::Actor;
use cm_library::ShotBundle;
use common::*;
use serde_json::{json, Value};
use std::fs;

fn read_manifest(dir: &std::path::Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}

fn write_manifest(dir: &std::path::Path, v: &Value) {
    fs::write(dir.join("manifest.json"), serde_json::to_vec(v).unwrap()).unwrap();
}

#[test]
fn r1_4_unknown_ops_round_trip_through_an_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = ShotBundle::create(tmp.path(), "S", &png(64, 48, 1), T0).unwrap();
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    drop(b);
    let mut v = read_manifest(&dir);
    let first_id = v["ops"][0]["id"].as_str().unwrap().to_owned();
    let callout =
        json!({"id": "op_new", "op": "callout", "at": [1, 2], "text": "hi", "tail": {"x": 3}});
    v["ops"].as_array_mut().unwrap().push(callout.clone());
    let stamp = json!({"id": "op_s", "op": "stamp", "kind": "check"});
    v["ops"].as_array_mut().unwrap().insert(0, stamp.clone());
    write_manifest(&dir, &v);

    let mut b = ShotBundle::open(&dir).unwrap();
    assert_eq!(b.ops().len(), 1, "only the known op is editable");
    assert_eq!(b.warnings().len(), 2, "{:?}", b.warnings());
    // An edit and a save keep both unknown ops, in their positions, byte-equal as JSON.
    b.add_ops(vec![arrow()], user(), t(5)).unwrap();
    b.save().unwrap();
    let after = read_manifest(&dir);
    let ops = after["ops"].as_array().unwrap();
    assert_eq!(ops.len(), 4);
    assert_eq!(ops[0], stamp);
    assert_eq!(ops[1]["id"], first_id);
    assert_eq!(ops[2], callout);
    assert_eq!(ops[3]["op"], "arrow");
    assert!(!dir.join("manifest.broken.json").exists());
}

#[test]
fn r1_4_unknown_history_entries_are_kept_until_an_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = ShotBundle::create(tmp.path(), "S", &png(64, 48, 1), T0).unwrap();
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    drop(b);
    let mut v = read_manifest(&dir);
    let newer = json!({"undo": [{"name": "future", "by": "user",
        "changes": [{"change": "move_layer", "from": 1}]}], "redo": []});
    v["history"] = newer.clone();
    write_manifest(&dir, &v);

    let mut b = ShotBundle::open(&dir).unwrap();
    assert_eq!(b.ops().len(), 1);
    assert!(!b.can_undo());
    assert!(b.warnings().iter().any(|w| w.contains("undo history")));
    // A save without edits writes the history back untouched.
    b.save().unwrap();
    assert_eq!(read_manifest(&dir)["history"], newer);
    // Opening never quarantined the manifest.
    assert!(!dir.join("manifest.broken.json").exists());
    // The first edit replaces it with this build's own history.
    b.add_ops(vec![arrow()], Actor::User, t(9)).unwrap();
    b.save().unwrap();
    let h = &read_manifest(&dir)["history"];
    assert_eq!(h["undo"].as_array().unwrap().len(), 1);
}
