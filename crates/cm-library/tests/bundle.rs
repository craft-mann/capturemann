#![allow(clippy::unwrap_used)]
mod common;
use cm_core::{Actor, ErrorCode};
use cm_library::manifest::SCHEMA_VERSION;
use cm_library::ops::{OpKind, Pt};
use cm_library::thumbs::write_poster;
use cm_library::ShotBundle;
use common::*;
use std::fs;

fn new_bundle(dir: &std::path::Path) -> ShotBundle {
    ShotBundle::create(dir, "Screenshot A", &png(64, 48, 1), T0).unwrap()
}

fn step_ns(b: &ShotBundle) -> Vec<u32> {
    b.ops()
        .iter()
        .filter_map(|o| match o.kind {
            OpKind::Step { n, .. } => Some(n),
            _ => None,
        })
        .collect()
}

#[test]
fn create_layout_and_manifest_lib_1_cap_4() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = png(64, 48, 1);
    let b = ShotBundle::create(tmp.path(), "Screenshot A", &bytes, T0).unwrap();
    assert_eq!(b.dir().file_name().unwrap(), "Screenshot A.cmshot");
    assert_eq!(fs::read(b.original_path()).unwrap(), bytes);
    let m = b.manifest();
    assert_eq!(m.schema, SCHEMA_VERSION);
    assert!(m.id.starts_with("shot_"));
    assert_eq!((m.original.size.width, m.original.size.height), (64, 48));
    // Same title again gets a suffix; nothing is overwritten (CAP-4.AC3 spirit).
    let b2 = ShotBundle::create(tmp.path(), "Screenshot A", &png(8, 8, 2), T0).unwrap();
    assert_eq!(b2.dir().file_name().unwrap(), "Screenshot A (2).cmshot");
    assert_ne!(b2.manifest().id, m.id);
    // Not a PNG: rejected, no folder left behind.
    let e = ShotBundle::create(tmp.path(), "bad", b"nope", T0).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(!tmp.path().join("bad.cmshot").exists());
}

#[test]
fn undo_five_edits_restores_empty_ops_ann_3_ac1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    for i in 0..5 {
        b.add_ops(vec![arrow()], user(), t(i)).unwrap();
    }
    assert_eq!(b.ops().len(), 5);
    for i in 0..5 {
        assert!(b.undo(user(), t(10 + i)).is_some());
    }
    assert!(b.ops().is_empty());
    assert!(b.undo(user(), t(20)).is_none());
}

#[test]
fn agent_call_is_one_step_ann_3_ac2() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    let ops: Vec<_> = (0..10).map(|_| rect_op()).collect();
    let r = b.annotate(ops, false, agent(), t(1)).unwrap();
    assert_eq!(r.by, agent());
    assert_eq!(b.ops().len(), 10);
    b.undo(user(), t(2)).unwrap();
    assert!(b.ops().is_empty());
    assert_eq!(b.revisions().len(), 2);
    assert_eq!(b.revisions()[0].by.to_string(), "agent:claude");
    assert_eq!(b.revisions()[1].action, "undo");
}

#[test]
fn undo_survives_restart_ann_3_ac3() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    b.add_ops(vec![rect_op()], user(), t(2)).unwrap();
    let id = b.ops()[0].id.clone();
    b.update_op(
        &id,
        OpKind::Line {
            from: Pt { x: 0.0, y: 0.0 },
            to: Pt { x: 5.0, y: 5.0 },
            color: red(),
            width: 2.0,
        },
        user(),
        t(3),
    )
    .unwrap();
    b.save().unwrap();
    let dir = b.dir().to_owned();
    drop(b);

    let mut b = ShotBundle::open(&dir).unwrap();
    assert!(b.warnings().is_empty(), "{:?}", b.warnings());
    assert_eq!(b.ops().len(), 2);
    assert_eq!(b.next_undo_name(), Some("update_op"));
    // Undo the update: back to the arrow.
    b.undo(user(), t(4)).unwrap();
    assert_eq!(b.ops()[0].kind.name(), "arrow");
    b.undo(user(), t(5)).unwrap();
    assert_eq!(b.ops().len(), 1);
    // Redo state also survives a save/open round trip.
    b.save().unwrap();
    let mut b = ShotBundle::open(&dir).unwrap();
    assert!(b.can_redo());
    b.redo(user(), t(6)).unwrap().unwrap();
    assert_eq!(b.ops().len(), 2);
    b.undo(user(), t(7)).unwrap();
    b.undo(user(), t(8)).unwrap();
    assert!(b.ops().is_empty());
    assert!(!b.can_undo());
    // Revision numbers stay monotonic across sessions.
    let revs: Vec<u64> = b.revisions().iter().map(|r| r.rev).collect();
    assert!(revs.windows(2).all(|w| w[1] == w[0] + 1), "{revs:?}");
}

#[test]
fn original_is_never_modified_ann_3_ac4() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    let before = fs::read(b.original_path()).unwrap();
    b.add_ops(vec![arrow(), rect_op()], user(), t(1)).unwrap();
    b.revert_to_original(user(), t(2)).unwrap();
    b.write_render(&png(64, 48, 9)).unwrap();
    b.save().unwrap();
    assert_eq!(fs::read(b.original_path()).unwrap(), before);
    assert!(b.verify_original().unwrap());
    assert!(fs::metadata(b.original_path())
        .unwrap()
        .permissions()
        .readonly());
}

#[test]
fn revert_is_one_undoable_step_ann_3() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![arrow(), rect_op()], user(), t(1)).unwrap();
    b.revert_to_original(user(), t(2)).unwrap();
    assert!(b.ops().is_empty());
    assert_eq!(
        b.revert_to_original(user(), t(3)).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    b.undo(user(), t(4)).unwrap();
    assert_eq!(b.ops().len(), 2);
}

#[test]
fn deleting_step_renumbers_ann_1_ac2() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![step(1), arrow(), step(2), step(3)], user(), t(1))
        .unwrap();
    let second = b.ops()[2].id.clone();
    b.remove_op(&second, user(), t(2)).unwrap();
    assert_eq!(step_ns(&b), vec![1, 2]);
    b.undo(user(), t(3)).unwrap();
    assert_eq!(step_ns(&b), vec![1, 2, 3]);
    assert_eq!(b.ops()[2].id, second);
}

#[test]
fn recolor_changes_only_that_op_ann_1_ac5() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![arrow(), rect_op()], user(), t(1)).unwrap();
    let id = b.ops()[1].id.clone();
    let untouched = b.ops()[0].clone();
    let OpKind::Rect {
        rect,
        width,
        fill,
        radius,
        ..
    } = b.ops()[1].kind.clone()
    else {
        panic!()
    };
    let blue = cm_library::ops::Color::parse("#2e8bff").unwrap();
    b.update_op(
        &id,
        OpKind::Rect {
            rect,
            color: blue,
            width,
            fill,
            radius,
        },
        user(),
        t(2),
    )
    .unwrap();
    assert_eq!(b.ops()[0], untouched);
    assert!(matches!(&b.ops()[1].kind, OpKind::Rect { color, .. } if *color == blue));
    b.undo(user(), t(3)).unwrap();
    assert!(matches!(&b.ops()[1].kind, OpKind::Rect { color, .. } if *color == red()));
}

#[test]
fn annotate_replace_is_one_step() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.add_ops(vec![arrow(), arrow()], user(), t(1)).unwrap();
    b.annotate(vec![rect_op()], true, agent(), t(2)).unwrap();
    assert_eq!(b.ops().len(), 1);
    b.undo(user(), t(3)).unwrap();
    assert_eq!(b.ops().len(), 2);
}

#[test]
fn invalid_ops_are_rejected_atomically_ann_1_edge() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    let bad = cm_library::ops::AnnotationOp::new(
        "",
        OpKind::Blur {
            rect: cm_library::ops::Rc {
                x: 0.0,
                y: 0.0,
                w: 5.0,
                h: 5.0,
            },
            strength: 1,
        },
    );
    let e = b.add_ops(vec![arrow(), bad], user(), t(1)).unwrap_err();
    assert!(e.to_string().contains("ops[1]"), "{e}");
    assert!(b.ops().is_empty() && b.revisions().is_empty());
    assert!(b.remove_op("op_99", user(), t(2)).is_err());
}

#[test]
fn history_and_revisions_are_capped_ann_3() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    for i in 0..230 {
        b.add_ops(
            vec![rect_op()],
            if i % 2 == 0 { user() } else { agent() },
            t(i),
        )
        .unwrap();
    }
    assert_eq!(b.revisions().len(), 200);
    assert_eq!(b.revisions().last().unwrap().rev, 230);
    b.save().unwrap();
    let dir = b.dir().to_owned();
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(m["history"]["undo"].as_array().unwrap().len(), 200);
    assert_eq!(m["revisions"].as_array().unwrap().len(), 200);
    let mut b = ShotBundle::open(&dir).unwrap();
    assert!(b.warnings().is_empty(), "{:?}", b.warnings());
    for i in 0..200 {
        b.undo(Actor::User, t(300 + i)).unwrap();
    }
    assert!(!b.can_undo());
    assert_eq!(b.ops().len(), 30);
}

#[test]
fn corrupt_manifest_is_quarantined_ann_3_edge() {
    let tmp = tempfile::tempdir().unwrap();
    let b = new_bundle(tmp.path());
    let dir = b.dir().to_owned();
    drop(b);
    fs::write(dir.join("manifest.json"), b"{ not json").unwrap();
    let b = ShotBundle::open(&dir).unwrap();
    assert!(b.ops().is_empty());
    assert_eq!(b.warnings().len(), 1);
    assert_eq!(
        fs::read(dir.join("manifest.broken.json")).unwrap(),
        b"{ not json"
    );
    assert!(dir.join("manifest.json").is_file());
    // A second quarantine never overwrites the first (R1-4).
    fs::write(dir.join("manifest.json"), b"{ again").unwrap();
    let b = ShotBundle::open(&dir).unwrap();
    assert!(b.ops().is_empty());
    assert_eq!(
        fs::read(dir.join("manifest.broken.json")).unwrap(),
        b"{ not json"
    );
    let extra: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("manifest.broken-"))
        .collect();
    assert_eq!(extra.len(), 1, "{extra:?}");
    assert_eq!(fs::read(dir.join(&extra[0])).unwrap(), b"{ again");
}

#[test]
fn newer_schema_is_refused_and_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let b = new_bundle(tmp.path());
    let dir = b.dir().to_owned();
    drop(b);
    let p = dir.join("manifest.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["schema"] = 2.into();
    let text = serde_json::to_vec(&v).unwrap();
    fs::write(&p, &text).unwrap();
    let e = ShotBundle::open(&dir).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(
        e.to_string().contains("schema 2") && e.to_string().contains("supported schema 1"),
        "{e}"
    );
    assert_eq!(fs::read(&p).unwrap(), text);
    assert!(!dir.join("manifest.broken.json").exists());
}

#[test]
fn unknown_top_level_fields_are_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    let dir = b.dir().to_owned();
    b.save().unwrap();
    let p = dir.join("manifest.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["tags"] = serde_json::json!(["demo"]);
    fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();
    let mut b = ShotBundle::open(&dir).unwrap();
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    b.save().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    assert_eq!(v["tags"], serde_json::json!(["demo"]));
    assert_eq!(v["ops"].as_array().unwrap().len(), 1);
}

#[test]
fn render_slot_and_staleness_ann_3() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    assert!(b.render_is_stale());
    b.write_render(&png(64, 48, 3)).unwrap();
    assert!(!b.render_is_stale());
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    assert!(b.render_is_stale());
    assert!(b.write_render(b"junk").is_err());
}

#[test]
fn poster_is_scaled_and_prefers_current_render() {
    let tmp = tempfile::tempdir().unwrap();
    let b = ShotBundle::create(tmp.path(), "Big", &png(1000, 500, 1), T0).unwrap();
    let p = write_poster(&b, 256).unwrap();
    assert_eq!(p, b.dir().join("thumbs/poster.jpg"));
    let img = image::open(&p).unwrap();
    assert_eq!((img.width(), img.height()), (256, 128));
    // Never upscales.
    let small = ShotBundle::create(tmp.path(), "Small", &png(40, 30, 1), T0).unwrap();
    let img = image::open(write_poster(&small, 256).unwrap()).unwrap();
    assert_eq!((img.width(), img.height()), (40, 30));
}

#[test]
fn rgba_create_and_crop_visible_rect() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = ShotBundle::create_from_rgba(tmp.path(), "Raw", 4, 4, vec![255; 64], T0).unwrap();
    assert_eq!(b.manifest().original.size.width, 4);
    assert!(ShotBundle::create_from_rgba(tmp.path(), "Raw2", 4, 4, vec![0; 3], T0).is_err());
    assert_eq!(b.visible_rect(), None);
    b.add_ops(
        vec![cm_library::ops::AnnotationOp::new(
            "",
            OpKind::Crop {
                rect: cm_library::ops::Rc {
                    x: 0.0,
                    y: 0.0,
                    w: 2.0,
                    h: 2.0,
                },
            },
        )],
        user(),
        t(1),
    )
    .unwrap();
    assert_eq!(b.visible_rect().map(|r| (r.w, r.h)), Some((2.0, 2.0)));
}

/// R1-12: a poster is never cut from the unredacted original while a render with ops is
/// missing; a stale poster that could show the redacted pixels is removed.
#[test]
fn r1_12_no_poster_from_the_original_when_ops_are_not_rendered() {
    use cm_library::ops::{AnnotationOp, Rc};
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    let poster = write_poster(&b, 64).unwrap();
    assert!(poster.is_file());
    // A non-redacting op: the old poster is kept, no new one is made.
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    assert!(write_poster(&b, 64).is_err());
    assert!(poster.is_file());
    // A blur with no render yet: the old poster (from the original) would leak it; removed.
    let blur = AnnotationOp::new(
        "",
        OpKind::Blur {
            rect: Rc {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            strength: 12,
        },
    );
    b.add_ops(vec![blur], user(), t(2)).unwrap();
    assert!(write_poster(&b, 64).is_err());
    assert!(!poster.exists());
    // Once the render is current the poster comes from it.
    b.write_render(&png(64, 48, 9)).unwrap();
    assert!(write_poster(&b, 64).unwrap().is_file());
}

/// R1-18: a failure while publishing the render leaves it stale on disk (never a render
/// recorded as current that disagrees with the ops), and no temp file stays behind.
#[test]
fn r1_18_failed_render_write_leaves_the_render_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = new_bundle(tmp.path());
    b.write_render(&png(64, 48, 3)).unwrap();
    assert!(!b.render_is_stale());
    let dir = b.dir().to_owned();
    // The ops change (a redaction is removed, say) and the final rename is made to fail by
    // putting a non-empty directory where render.png goes.
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    fs::remove_file(b.render_path()).unwrap();
    fs::create_dir(b.render_path()).unwrap();
    fs::write(b.render_path().join("x"), b"x").unwrap();
    assert!(b.write_render(&png(64, 48, 4)).is_err());
    assert!(b.render_is_stale());
    let on_disk = ShotBundle::open(&dir).unwrap();
    assert_eq!(
        on_disk.ops().len(),
        1,
        "the ops were saved before the render"
    );
    assert!(on_disk.manifest().render.is_none());
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    // Recovers once the obstacle is gone.
    fs::remove_dir_all(on_disk.render_path()).unwrap();
    let mut b = on_disk;
    b.write_render(&png(64, 48, 4)).unwrap();
    assert!(!b.render_is_stale());
    assert!(ShotBundle::open(&dir).unwrap().manifest().render.is_some());
}
