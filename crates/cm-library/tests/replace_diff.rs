//! R1-3: `annotate { replace: true }` is stored as a diff by op id, not as clear-all plus
//! add-each, so a GUI that sends the whole list on every edit does not grow the manifest by the
//! whole list each time.
#![allow(clippy::unwrap_used)]
mod common;
use cm_core::Actor;
use cm_library::ops::{AnnotationOp, OpKind, Rc};
use cm_library::ShotBundle;
use common::*;
use serde_json::{json, Value};
use std::fs;

fn rect(id: &str, x: f64) -> AnnotationOp {
    AnnotationOp::new(
        id,
        OpKind::Rect {
            rect: Rc {
                x,
                y: 2.0,
                w: 30.0,
                h: 40.0,
            },
            color: red(),
            width: 4.0,
            fill: None,
            radius: 6.0,
        },
    )
}

fn ids(b: &ShotBundle) -> Vec<String> {
    b.ops().iter().map(|o| o.id.clone()).collect()
}

fn manifest(dir: &std::path::Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}

fn new_bundle(dir: &std::path::Path) -> ShotBundle {
    ShotBundle::create(dir, "S", &png(64, 48, 1), T0).unwrap()
}

fn last_changes(b: &mut ShotBundle) -> Vec<Value> {
    b.save().unwrap();
    let m = manifest(b.dir());
    m["history"]["undo"].as_array().unwrap().last().unwrap()["changes"]
        .as_array()
        .unwrap()
        .clone()
}

/// The probe: 30 shapes edited 200 times through the whole list. The old representation stored
/// the full list twice per edit (about 4.6 MB for the history); the diff stores one op pair.
#[test]
fn probe_200_edits_on_30_shapes_keep_the_manifest_small() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    let mut list: Vec<AnnotationOp> = (0..30)
        .map(|i| rect(&format!("op_{}", i + 1), f64::from(i)))
        .collect();
    b.add_ops(list.clone(), user(), t(0)).unwrap();
    b.save().unwrap();
    for edit in 0..200 {
        // The GUI drags one shape a little and sends everything.
        let k = (edit * 7) % 30;
        list[k] = rect(&format!("op_{}", k + 1), 100.0 + edit as f64);
        b.annotate(list.clone(), true, user(), t(1 + edit as i64))
            .unwrap();
        b.save().unwrap();
        assert_eq!(b.ops(), list.as_slice());
    }
    let size = fs::metadata(b.dir().join("manifest.json")).unwrap().len();
    let history = serde_json::to_vec(&manifest(b.dir())["history"])
        .unwrap()
        .len();
    assert!(history < 200_000, "history is {history} bytes");
    assert!(size < 250_000, "manifest is {size} bytes");
    // Each step is one ReplaceOp.
    let m = manifest(b.dir());
    let steps = m["history"]["undo"].as_array().unwrap();
    assert_eq!(steps.len(), 200);
    assert!(steps
        .iter()
        .all(|s| s["changes"].as_array().unwrap().len() == 1));
    assert_eq!(steps[199]["changes"][0]["change"], "replace_op");
}

#[test]
fn undo_and_redo_restore_exact_states_also_after_reopening() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    let mut list: Vec<AnnotationOp> = (0..12)
        .map(|i| rect(&format!("op_{}", i + 1), f64::from(i)))
        .collect();
    let mut states: Vec<Vec<AnnotationOp>> = vec![Vec::new()];
    b.add_ops(list.clone(), user(), t(0)).unwrap();
    states.push(list.clone());
    for edit in 0..40usize {
        match edit % 5 {
            // move one
            0 | 1 => {
                let k = edit % list.len();
                let id = list[k].id.clone();
                list[k] = rect(&id, 50.0 + edit as f64);
            }
            // delete one (never the last few)
            2 => {
                if list.len() > 6 {
                    list.remove(edit % list.len());
                }
            }
            // add one at the end with a fresh id
            3 => list.push(rect("", 70.0 + edit as f64)),
            // reorder: rotate
            _ => list.rotate_left(1),
        }
        b.annotate(list.clone(), true, user(), t(1 + edit as i64))
            .unwrap();
        // Blank ids were minted; keep the bundle's version as the expected state.
        list = b.ops().to_vec();
        states.push(list.clone());
        assert_eq!(b.ops(), list.as_slice());
    }
    b.save().unwrap();
    let dir = b.dir().to_owned();
    // Same session.
    for want in states.iter().rev().skip(1).take(20) {
        b.undo(user(), t(500)).unwrap();
        assert_eq!(b.ops(), want.as_slice());
    }
    // Back to the tip with redo, then persist and reopen.
    while b.redo(user(), t(600)).unwrap().is_some() {}
    assert_eq!(b.ops(), states.last().unwrap().as_slice());
    b.save().unwrap();
    drop(b);
    let mut b = ShotBundle::open(&dir).unwrap();
    assert!(b.warnings().is_empty(), "{:?}", b.warnings());
    assert_eq!(b.ops(), states.last().unwrap().as_slice());
    for want in states.iter().rev().skip(1) {
        if b.undo(user(), t(700)).is_none() {
            break;
        }
        assert_eq!(b.ops(), want.as_slice());
    }
    assert!(!b.can_undo());
}

#[test]
fn each_kind_of_change_is_one_step_with_the_expected_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(
        vec![
            rect("a", 1.0),
            rect("b", 2.0),
            rect("c", 3.0),
            rect("d", 4.0),
        ],
        user(),
        t(0),
    )
    .unwrap();
    let base = b.ops().to_vec();

    // Changed + removed + added in one edit: three kinds of change, one step.
    let revs_before = b.revisions().len();
    b.annotate(
        vec![
            rect("a", 1.0),
            rect("b", 20.0),
            rect("d", 4.0),
            rect("e", 5.0),
        ],
        true,
        agent(),
        t(1),
    )
    .unwrap();
    assert_eq!(b.revisions().len(), revs_before + 1);
    assert_eq!(ids(&b), ["a", "b", "d", "e"]);
    let kinds: Vec<String> = last_changes(&mut b)
        .iter()
        .map(|c| c["change"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(kinds, ["remove_op", "replace_op", "add_op"]);
    b.undo(user(), t(2)).unwrap();
    assert_eq!(b.ops(), base.as_slice());
    b.redo(user(), t(3)).unwrap();
    assert_eq!(ids(&b), ["a", "b", "d", "e"]);

    // An op inserted in the middle keeps its place.
    b.annotate(
        vec![
            rect("a", 1.0),
            rect("m", 9.0),
            rect("b", 20.0),
            rect("d", 4.0),
            rect("e", 5.0),
        ],
        true,
        user(),
        t(4),
    )
    .unwrap();
    assert_eq!(ids(&b), ["a", "m", "b", "d", "e"]);
    let c = last_changes(&mut b);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["change"], "add_op");
    assert_eq!(c[0]["index"], 1);

    // Reordering the same ops is one ReorderOps and nothing else.
    b.annotate(
        vec![
            rect("e", 5.0),
            rect("a", 1.0),
            rect("m", 9.0),
            rect("b", 20.0),
            rect("d", 4.0),
        ],
        true,
        user(),
        t(5),
    )
    .unwrap();
    assert_eq!(ids(&b), ["e", "a", "m", "b", "d"]);
    let c = last_changes(&mut b);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0]["change"], "reorder_ops");
    b.undo(user(), t(6)).unwrap();
    assert_eq!(ids(&b), ["a", "m", "b", "d", "e"]);
    b.redo(user(), t(7)).unwrap();
    assert_eq!(ids(&b), ["e", "a", "m", "b", "d"]);

    // Reorder together with an add and a change still undoes cleanly.
    let before = b.ops().to_vec();
    b.annotate(
        vec![
            rect("d", 4.0),
            rect("n", 1.0),
            rect("e", 55.0),
            rect("b", 20.0),
            rect("m", 9.0),
            rect("a", 1.0),
        ],
        true,
        user(),
        t(8),
    )
    .unwrap();
    assert_eq!(ids(&b), ["d", "n", "e", "b", "m", "a"]);
    b.undo(user(), t(9)).unwrap();
    assert_eq!(b.ops(), before.as_slice());
}

#[test]
fn unchanged_replace_adds_no_step_and_ids_are_minted_without_clashes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![rect("op_1", 1.0), rect("op_2", 2.0)], user(), t(0))
        .unwrap();
    let revs = b.revisions().len();
    let same = b.ops().to_vec();
    b.annotate(same.clone(), true, user(), t(1)).unwrap();
    assert_eq!(
        b.revisions().len(),
        revs,
        "nothing changed, nothing recorded"
    );
    assert_eq!(b.ops(), same.as_slice());
    // New ops without ids get fresh ones, even when the list names a higher explicit id.
    let mut list = same;
    list.push(rect("", 3.0));
    list.push(rect("op_3", 9.0));
    list.push(rect("", 4.0));
    b.annotate(list, true, user(), t(2)).unwrap();
    let got = ids(&b);
    assert_eq!(got.len(), 5);
    let mut uniq = got.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), 5, "{got:?}");
    assert!(got.contains(&"op_3".to_owned()));
    // Duplicate ids in the list are refused and nothing changes.
    let before = b.ops().to_vec();
    let e = b
        .annotate(vec![rect("x", 1.0), rect("x", 2.0)], true, user(), t(3))
        .unwrap_err();
    assert!(e.to_string().contains("duplicate"), "{e}");
    assert_eq!(b.ops(), before.as_slice());
}

#[test]
fn a_replace_that_removes_everything_but_one_and_back_from_empty_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(
        vec![rect("a", 1.0), rect("b", 2.0), rect("c", 3.0)],
        user(),
        t(0),
    )
    .unwrap();
    b.annotate(vec![rect("b", 2.0)], true, user(), t(1))
        .unwrap();
    assert_eq!(ids(&b), ["b"]);
    b.undo(user(), t(2)).unwrap();
    assert_eq!(ids(&b), ["a", "b", "c"]);
    b.revert_to_original(user(), t(3)).unwrap();
    assert!(b.ops().is_empty());
    b.annotate(vec![rect("z", 1.0)], true, user(), t(4))
        .unwrap();
    assert_eq!(ids(&b), ["z"]);
    b.undo(user(), t(5)).unwrap();
    assert!(b.ops().is_empty());
}

#[test]
fn unknown_ops_survive_a_diff_replace() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![rect("a", 1.0), rect("b", 2.0)], user(), t(0))
        .unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    drop(b);
    let mut v = manifest(&dir);
    let callout = json!({"id": "op_new", "op": "callout", "at": [1, 2], "text": "hi"});
    v["ops"].as_array_mut().unwrap().insert(1, callout.clone());
    fs::write(dir.join("manifest.json"), serde_json::to_vec(&v).unwrap()).unwrap();
    let mut b = ShotBundle::open(&dir).unwrap();
    assert_eq!(b.ops().len(), 2);
    // The GUI sees two ops, reorders and edits them.
    b.annotate(vec![rect("b", 2.0), rect("a", 11.0)], true, user(), t(1))
        .unwrap();
    b.save().unwrap();
    let after = manifest(&dir);
    let ops = after["ops"].as_array().unwrap();
    assert_eq!(ops.len(), 3);
    assert!(ops.contains(&callout), "the unknown op is kept as stored");
    b.undo(user(), t(2)).unwrap();
    assert_eq!(ids(&b), ["a", "b"]);
}

#[test]
fn history_is_one_compact_line_and_pretty_history_still_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![rect("a", 1.0)], user(), t(0)).unwrap();
    b.annotate(vec![rect("a", 2.0)], true, user(), t(1))
        .unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    let text = fs::read_to_string(dir.join("manifest.json")).unwrap();
    let line = text.lines().find(|l| l.contains("\"history\"")).unwrap();
    assert!(
        line.contains("replace_op") && line.contains("add_op"),
        "{line}"
    );
    assert!(
        text.lines().count() > 10,
        "the rest of the manifest stays pretty"
    );
    // An older build wrote the history indented: still readable, undo works.
    let pretty = serde_json::to_string_pretty(&manifest(&dir)).unwrap();
    assert!(pretty.lines().count() > text.lines().count());
    fs::write(dir.join("manifest.json"), pretty).unwrap();
    let mut b = ShotBundle::open(&dir).unwrap();
    assert!(b.warnings().is_empty());
    b.undo(user(), t(2)).unwrap();
    assert_eq!(b.ops()[0], rect("a", 1.0));
}

#[test]
fn history_stored_as_clear_plus_add_by_an_older_build_still_undoes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![rect("a", 1.0), rect("b", 2.0)], user(), t(0))
        .unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    drop(b);
    // What the previous build wrote for `annotate { replace: true }` with one op `c`.
    let mut v = manifest(&dir);
    let old_ops = v["ops"].clone();
    let c = serde_json::to_value(rect("c", 5.0)).unwrap();
    v["ops"] = json!([c]);
    v["history"]["undo"] = json!([{
        "name": "annotate_replace", "by": "agent:claude",
        "changes": [{"change": "clear_ops", "ops": old_ops}, {"change": "add_op", "index": 0, "op": c}]
    }]);
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&v).unwrap(),
    )
    .unwrap();
    let mut b = ShotBundle::open(&dir).unwrap();
    assert!(b.warnings().is_empty(), "{:?}", b.warnings());
    assert!(b.can_undo());
    b.undo(Actor::User, t(1)).unwrap();
    assert_eq!(ids(&b), ["a", "b"]);
    b.redo(Actor::User, t(2)).unwrap().unwrap();
    assert_eq!(ids(&b), ["c"]);
}
