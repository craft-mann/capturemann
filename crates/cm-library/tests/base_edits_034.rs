//! 0.3.4 plan problem 12 and ANN-9.AC4/AC7: a stored list with a base edit after an arrow (as a
//! 0.3.3 app writes it) loads sorted and saves sorted; `annotate` appends refuse a base edit
//! after another op.
#![allow(clippy::unwrap_used)]
mod common;
use cm_core::annotation::parse_ops_str;
use cm_library::ShotBundle;
use common::*;
use serde_json::Value;
use std::fs;

const RETYPE: &str = r##"{"id":"op_2","op":"retype","rect":[2,2,20,10],"fill":{"kind":"solid","colors":["#FFFFFF"]},"text":"Hi","size":10}"##;
const ARROW: &str =
    r##"{"id":"op_1","op":"arrow","from":[1,1],"to":[9,9],"color":"#FF3B3B","width":4}"##;

#[test]
fn a_033_list_with_a_base_edit_after_an_arrow_loads_and_saves_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = ShotBundle::create(tmp.path(), "S", &png(64, 48, 1), T0).unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    drop(b);
    let mf = dir.join("manifest.json");
    let mut v: Value = serde_json::from_slice(&fs::read(&mf).unwrap()).unwrap();
    v["ops"] = serde_json::from_str(&format!("[{ARROW},{RETYPE}]")).unwrap();
    fs::write(&mf, serde_json::to_vec(&v).unwrap()).unwrap();

    let mut b = ShotBundle::open(&dir).unwrap();
    let ids: Vec<&str> = b.ops().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["op_2", "op_1"], "base edits come first");
    b.save().unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(&mf).unwrap()).unwrap();
    let ops = saved["ops"].as_array().unwrap();
    assert_eq!(ops[0]["op"], "retype");
    assert_eq!(ops[1]["op"], "arrow");
}

#[test]
fn annotate_appends_refuse_a_base_edit_after_another_op() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = ShotBundle::create(tmp.path(), "S", &png(64, 48, 1), T0).unwrap();
    b.add_ops(parse_ops_str(&format!("[{ARROW}]")).unwrap(), user(), t(1))
        .unwrap();
    let r = parse_ops_str(&format!("[{RETYPE}]")).unwrap();
    let e = b.add_ops(r, user(), t(2)).unwrap_err();
    assert!(e.to_string().contains("ops[0]"), "{e}");
    assert_eq!(b.ops().len(), 1, "nothing stored");
}
