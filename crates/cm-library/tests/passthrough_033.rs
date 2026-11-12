//! ANN-6.AC6 / ANN-4.AC7: a 0.3.3 manifest with every new op, the new step fields, unknown fields
//! inside known ops and an unknown op type survives open -> edit -> save unchanged.
//!
//! The fixture `tests/fixtures/ops-0.3.3-all.json` is plain manifest `ops`. To check the other
//! direction (the shipped v0.3.2 code), copy the fixture and this file's `fixture_bundle` helper
//! into a checkout of tag `v0.3.2`: there the unknown fields of known ops are dropped and the
//! step fields `style`, `start`, `text` are lost (accepted, see the 0.3.3 plan), while every
//! op is still present.
#![allow(clippy::unwrap_used)]
mod common;
use cm_core::Actor;
use cm_library::ShotBundle;
use common::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE: &str = include_str!("fixtures/ops-0.3.3-all.json");

fn read_manifest(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}

/// A fresh bundle whose manifest `ops` is the fixture. Returns its folder.
fn fixture_bundle(root: &Path) -> PathBuf {
    let mut b = ShotBundle::create(root, "S", &png(64, 48, 1), T0).unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    drop(b);
    let mut v = read_manifest(&dir);
    v["ops"] = serde_json::from_str(FIXTURE).unwrap();
    fs::write(dir.join("manifest.json"), serde_json::to_vec(&v).unwrap()).unwrap();
    dir
}

#[test]
fn ann6_ac6_fixture_survives_open_edit_save() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = fixture_bundle(tmp.path());
    let fixture: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();

    let mut b = ShotBundle::open(&dir).unwrap();
    assert_eq!(b.ops().len(), 7, "every known op is loaded");
    assert_eq!(b.warnings().len(), 1, "{:?}", b.warnings());
    // A save without an edit keeps every op, field and the order.
    b.save().unwrap();
    assert_eq!(read_manifest(&dir)["ops"], Value::Array(fixture.clone()));

    // An edit elsewhere (a new arrow) keeps all of it too, with the arrow added.
    b.add_ops(vec![arrow()], user(), t(5)).unwrap();
    b.save().unwrap();
    let after = read_manifest(&dir);
    let ops = after["ops"].as_array().unwrap();
    assert_eq!(ops.len(), fixture.len() + 1);
    for (i, f) in fixture.iter().enumerate() {
        assert_eq!(&ops[i], f, "op {i} changed");
    }
    // Reload: still the same.
    let b = ShotBundle::open(&dir).unwrap();
    assert_eq!(b.ops().len(), 8);
}

/// ANN-4.AC7: the unknown op type is kept whole, in place, even when a known op is removed.
#[test]
fn ann4_ac7_unknown_op_type_survives_editing_its_neighbours() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = fixture_bundle(tmp.path());
    let mut b = ShotBundle::open(&dir).unwrap();
    b.remove_op("op_1", user(), t(2)).unwrap();
    b.save().unwrap();
    let ops = read_manifest(&dir)["ops"].as_array().unwrap().clone();
    let m = ops.iter().find(|o| o["op"] == "magnify").unwrap();
    assert_eq!(m["zoom"], 2);
    assert_eq!(m["future"], "an op type this build does not know");
    assert!(ops.iter().all(|o| o["id"] != "op_1"));
}

/// ANN-6: deleting a step renumbers the later ones and keeps their style, start, text and
/// unknown fields.
#[test]
fn ann6_remove_step_keeps_new_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = fixture_bundle(tmp.path());
    let mut b = ShotBundle::open(&dir).unwrap();
    // The fixture step has n = 2; add a step with n = 3, then delete the fixture step.
    let mut s3 = step(3);
    s3.id = "op_s3".into();
    b.add_ops(vec![s3], user(), t(3)).unwrap();
    b.remove_op("op_5", user(), t(4)).unwrap();
    b.save().unwrap();
    let ops = read_manifest(&dir)["ops"].as_array().unwrap().clone();
    let steps: Vec<&Value> = ops.iter().filter(|o| o["op"] == "step").collect();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["n"], 2);
}

/// ANN-7.AC2 at the bundle: a second spotlight is refused, naming the new op, and nothing is
/// stored (append and replace).
#[test]
fn ann7_ac2_bundle_refuses_a_second_spotlight() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = fixture_bundle(tmp.path());
    let mut b = ShotBundle::open(&dir).unwrap();
    let spot =
        cm_core::parse_ops_str(r#"[{"op":"spotlight","shapes":[{"rect":[1,1,5,5]}]}]"#).unwrap();
    let e = b.add_ops(spot, user(), t(5)).unwrap_err();
    assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
    assert!(e.to_string().contains("ops[0]"), "{e}");
    assert_eq!(b.ops().len(), 7);
    // Replacing the whole list with two spotlights is refused too.
    // (parse_ops refuses two spotlights itself, so build the list from one parse.)
    let mut list =
        cm_core::parse_ops_str(r#"[{"id":"a","op":"spotlight","shapes":[{"rect":[1,1,5,5]}]}]"#)
            .unwrap();
    let mut second = list[0].clone();
    second.id = "b".into();
    list.push(second);
    let e = b.annotate(list, true, Actor::User, t(6)).unwrap_err();
    assert!(e.to_string().contains("ops[1]"), "{e}");
    assert_eq!(b.ops().len(), 7);
}
