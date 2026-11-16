#![allow(clippy::unwrap_used)]
//! Click Guide bundles (GDE-1, GDE-2) and the `guide` capture kind.
mod common;
use cm_core::annotation::OpKind;
use cm_core::Actor;
use cm_library::guide::{ClickPoint, GuideBundle};
use cm_library::manifest::CaptureKind;
use cm_library::Library;
use common::*;
use std::fs;

fn click(x: f64) -> ClickPoint {
    ClickPoint {
        x,
        y: 20.0,
        button: "left".into(),
        count: 1,
        scale: 1.0,
    }
}

fn make(parent: &std::path::Path, n: usize) -> GuideBundle {
    let mut g = GuideBundle::create(parent, "Set up SSO", serde_json::json!({}), T0).unwrap();
    for i in 0..n {
        g.add_click_step(
            &png(120, 80, i as u8),
            click(10.0 + i as f64 * 20.0),
            &format!("Click \"B{i}\""),
            &Actor::User,
            T0,
        )
        .unwrap();
    }
    g
}

fn badge_n(g: &GuideBundle, id: &str) -> u32 {
    let b = g.open_step(id).unwrap();
    b.ops()
        .iter()
        .find_map(|o| match o.kind {
            OpKind::Step { n, .. } => Some(n),
            _ => None,
        })
        .unwrap()
}

fn steps_dirs(g: &GuideBundle) -> usize {
    fs::read_dir(g.dir().join("steps")).unwrap().count()
}

#[test]
fn recorded_steps_have_badge_at_click_gde_1_ac1() {
    let tmp = tempfile::tempdir().unwrap();
    let g = make(tmp.path(), 5);
    assert_eq!(g.steps().len(), 5);
    let s = &g.steps()[2];
    let b = g.open_step(&s.id).unwrap();
    let at = b
        .ops()
        .iter()
        .find_map(|o| match o.kind {
            OpKind::Step { at, .. } => Some(at),
            _ => None,
        })
        .unwrap();
    // The ring is centred on the click (within 2 px); the badge sits on the ring, close by.
    let ring = b
        .ops()
        .iter()
        .find_map(|o| match &o.kind {
            OpKind::Ellipse { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("a ring");
    assert!((ring.x + ring.w / 2.0 - 50.0).abs() <= 2.0);
    assert!((ring.y + ring.h / 2.0 - 20.0).abs() <= 2.0);
    let dist = ((at.x - 50.0).powi(2) + (at.y - 20.0).powi(2)).sqrt();
    assert!(dist <= ring.w / 2.0 * 1.2, "badge {dist} from the click");
    assert_eq!(badge_n(&g, &s.id), 3);
    assert_eq!(s.title, "Click \"B2\"");
}

#[test]
fn move_step_renumbers_in_one_undo_step_gde_2_ac1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = make(tmp.path(), 4);
    let ids: Vec<String> = g.steps().iter().map(|s| s.id.clone()).collect();
    g.move_step(&ids[3], 1, &Actor::User, T0).unwrap();
    let order: Vec<_> = g.steps().iter().map(|s| s.id.clone()).collect();
    let want: Vec<String> = [0, 3, 1, 2].iter().map(|i| ids[*i].clone()).collect();
    assert_eq!(order, want);
    assert_eq!(badge_n(&g, &ids[3]), 2);
    assert_eq!(badge_n(&g, &ids[1]), 3);
    g.undo(&Actor::User, T0).unwrap();
    let order: Vec<_> = g.steps().iter().map(|s| s.id.clone()).collect();
    assert_eq!(order, ids);
    assert_eq!(badge_n(&g, &ids[3]), 4);
    assert!(!g.can_undo());
    g.redo(&Actor::User, T0).unwrap();
    assert_eq!(g.steps()[1].id, ids[3]);
}

#[test]
fn deleted_step_leaves_no_orphan_gde_2_ac2() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = make(tmp.path(), 3);
    let id = g.steps()[1].id.clone();
    g.delete_step(&id, &Actor::User, T0).unwrap();
    assert_eq!(g.steps().len(), 2);
    assert_eq!(steps_dirs(&g), 2, "steps/ holds only the current images");
    // The undo brings the picture back.
    g.undo(&Actor::User, T0).unwrap();
    assert_eq!(steps_dirs(&g), 3);
    assert!(!g.open_step(&id).unwrap().ops().is_empty());
    // Once the history no longer needs it, it is gone for good.
    g.delete_step(&id, &Actor::User, T0).unwrap();
    for i in 0..60 {
        g.set_intro(&format!("x{i}"), &Actor::User, T0).unwrap();
    }
    assert!(!g.dir().join(".undo").exists());
    assert_eq!(steps_dirs(&g), 2);
}

#[test]
fn duplicate_note_retake_and_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = make(tmp.path(), 2);
    let a = g.steps()[0].id.clone();
    let copy = g.duplicate_step(&a, &Actor::User, T0).unwrap();
    assert_eq!(g.steps()[1].id, copy);
    assert_eq!(badge_n(&g, &copy), 2);
    let third = g.steps()[2].id.clone();
    assert_eq!(badge_n(&g, &third), 3);
    let note = g
        .insert_note_step(Some(&a), "Type your password", "", &Actor::User, T0)
        .unwrap();
    assert!(g.step(&note).unwrap().is_note());
    g.retake_step(&copy, &png(120, 80, 99), &Actor::User, T0)
        .unwrap();
    assert!(!g.step(&copy).unwrap().has_proposals);
    assert_eq!(steps_dirs(&g), 3);
    let again = GuideBundle::open(g.dir()).unwrap();
    assert_eq!(again.steps().len(), 4);
    assert_eq!(again.id(), g.id());
}

#[test]
fn focus_crop_is_a_crop_op_gde_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = GuideBundle::create(tmp.path(), "Big", serde_json::json!({}), T0).unwrap();
    let s = g
        .add_click_step(
            &png(2000, 1400, 3),
            click(1900.0),
            "Click here",
            &Actor::User,
            T0,
        )
        .unwrap();
    assert!(s.focus_crop);
    let b = g.open_step(&s.id).unwrap();
    let r = b.visible_rect().unwrap();
    assert_eq!((r.w, r.h), (1200.0, 800.0));
    assert_eq!(r.x, 800.0); // clamped to the frame's right edge
    g.set_focus_crop(&s.id, false, &Actor::User, T0).unwrap();
    assert!(g.open_step(&s.id).unwrap().visible_rect().is_none());
}

#[test]
fn library_indexes_a_guide_and_not_its_steps() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let mut l = Library::open_or_create(&root).unwrap();
    let g = make(&root, 2);
    let rec = l.index_dir(g.dir()).unwrap();
    assert_eq!(rec.kind, CaptureKind::Guide);
    assert_eq!(rec.title, "Set up SSO");
    let rep = l.rescan().unwrap();
    assert_eq!(rep.indexed, 1);
    assert_eq!(l.count().unwrap(), 1);
    // A step image opens by `<guide id>/<step id>` and saving it refreshes the guide's row only.
    let sid = g.steps()[0].id.clone();
    let mut b = l.open_screenshot(&format!("{}/{}", rec.id, sid)).unwrap();
    l.save_shot(&mut b).unwrap();
    assert_eq!(l.count().unwrap(), 1);
    // Delete and rename treat it like any bundle.
    let r = l.rename(&rec.id, "Renamed guide").unwrap();
    assert!(r.path.ends_with(".cmguide"));
    assert!(l.remove(&rec.id).unwrap());
}

fn click_scaled(x: f64, y: f64, scale: f64) -> ClickPoint {
    ClickPoint {
        x,
        y,
        button: "left".into(),
        count: 1,
        scale,
    }
}

#[test]
fn focus_crop_is_1200x800_logical_scaled_by_the_display_gde_1() {
    use cm_library::guide::focus_rect;
    // Scale 1: 1200 x 800 physical, centred on the click.
    let r = focus_rect(3000, 2000, (1500.0, 1000.0), 1.0).unwrap();
    assert_eq!((r.x, r.y, r.w, r.h), (900.0, 600.0, 1200.0, 800.0));
    // Scale 2 (Retina): 2400 x 1600 physical px = the same 1200 x 800 points.
    let r = focus_rect(3000, 2000, (1500.0, 1000.0), 2.0).unwrap();
    assert_eq!((r.x, r.y, r.w, r.h), (300.0, 200.0, 2400.0, 1600.0));
    // A frame no larger than the crop has nothing to crop.
    assert!(focus_rect(1200, 800, (10.0, 10.0), 1.0).is_none());
    assert!(focus_rect(2400, 1600, (10.0, 10.0), 2.0).is_none());
    // The crop is clamped to the frame, and the click stays inside it, at every edge.
    for scale in [1.0, 2.0] {
        for click in [
            (0.0, 0.0),
            (2999.0, 0.0),
            (0.0, 1999.0),
            (2999.0, 1999.0),
            (5.0, 1000.0),
        ] {
            let r = focus_rect(3000, 2000, click, scale).unwrap();
            assert!(r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= 3000.0 && r.y + r.h <= 2000.0);
            assert!(
                click.0 >= r.x && click.0 <= r.x + r.w && click.1 >= r.y && click.1 <= r.y + r.h,
                "click {click:?} outside {r:?} at scale {scale}"
            );
        }
    }
    // A frame narrower than the crop keeps its full width.
    let r = focus_rect(1000, 2000, (500.0, 1000.0), 1.0).unwrap();
    assert_eq!((r.x, r.w, r.h), (0.0, 1000.0, 800.0));
}

#[test]
fn a_retina_step_gets_the_logical_sized_crop_gde_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = GuideBundle::create(tmp.path(), "Retina", serde_json::json!({}), T0).unwrap();
    let s = g
        .add_click_step(
            &png(2880, 1800, 3),
            click_scaled(2870.0, 10.0, 2.0),
            "Click here",
            &Actor::User,
            T0,
        )
        .unwrap();
    assert!(s.focus_crop);
    let r = g.open_step(&s.id).unwrap().visible_rect().unwrap();
    assert_eq!((r.w, r.h), (2400.0, 1600.0));
    assert_eq!((r.x, r.y), (480.0, 0.0));
    // Showing the crop again after hiding it uses the same scale.
    g.set_focus_crop(&s.id, false, &Actor::User, T0).unwrap();
    g.set_focus_crop(&s.id, true, &Actor::User, T0).unwrap();
    let r = g.open_step(&s.id).unwrap().visible_rect().unwrap();
    assert_eq!((r.w, r.h), (2400.0, 1600.0));
}

/// The frozen schema 1 keeps `has_proposals`; an old file that says `true` still reads, and a file
/// without the key reads as false (the feature is withdrawn for 0.3.4).
#[test]
fn an_old_step_with_has_proposals_still_reads() {
    use cm_library::guide::GuideStep;
    let old: GuideStep =
        serde_json::from_str(r#"{"id":"step_1","type":"image","title":"t","has_proposals":true}"#)
            .unwrap();
    assert!(old.has_proposals);
    let none: GuideStep =
        serde_json::from_str(r#"{"id":"step_1","type":"image","title":"t"}"#).unwrap();
    assert!(!none.has_proposals);
}

// ---- one crop for the whole guide (GDE-1, maintainer review Nov 15, 2026) ----------------------

fn r4(r: cm_core::annotation::Rc) -> (f64, f64, f64, f64) {
    (r.x, r.y, r.w, r.h)
}

#[test]
fn the_guide_crop_is_the_padded_box_of_all_clicks_gde_1() {
    use cm_library::guide::guide_crop_rect as rect;
    let f = (3000, 2000);
    // One click: the default focus size, as before (left edge, right edge, middle).
    let left = rect(f, &[((100.0, 1000.0), 1.0)]).unwrap();
    assert_eq!(r4(left), (0.0, 600.0, 1200.0, 800.0));
    let right = rect(f, &[((2900.0, 1000.0), 1.0)]).unwrap();
    assert_eq!(r4(right), (1800.0, 600.0, 1200.0, 800.0));
    let mid = rect(f, &[((1500.0, 1000.0), 1.0)]).unwrap();
    assert_eq!(r4(mid), (900.0, 600.0, 1200.0, 800.0));
    // Clicks 100, 200 and 300 px from the left: 600 px of padding on each side, clamped.
    let near = [
        ((100.0, 1000.0), 1.0),
        ((200.0, 1000.0), 1.0),
        ((300.0, 1000.0), 1.0),
    ];
    assert_eq!(r4(rect(f, &near).unwrap()), (0.0, 600.0, 1400.0, 800.0));
    // Clicks at both ends: the box is as wide as the frame but still short.
    let ends = [((200.0, 1000.0), 1.0), ((2800.0, 1000.0), 1.0)];
    assert_eq!(r4(rect(f, &ends).unwrap()), (0.0, 600.0, 3000.0, 800.0));
    // Corners spread over the frame: more than 85 % of it, so no crop.
    let wide = [((100.0, 100.0), 1.0), ((2900.0, 1900.0), 1.0)];
    assert!(rect(f, &wide).is_none());
    // A frame no larger than the default focus has nothing to crop; no clicks, no crop.
    assert!(rect((1200, 800), &[((10.0, 10.0), 1.0)]).is_none());
    assert!(rect(f, &[]).is_none());
    // Retina: the padding is 600 x 400 logical points times the click's scale.
    let retina = rect((6000, 4000), &[((3000.0, 2000.0), 2.0)]).unwrap();
    assert_eq!(r4(retina), (1800.0, 1200.0, 2400.0, 1600.0));
    // Different scales in one frame: each click pads by its own scale.
    let mixed = [((1000.0, 1000.0), 1.0), ((3000.0, 2000.0), 2.0)];
    let m = rect((6000, 4000), &mixed).unwrap();
    assert_eq!(r4(m), (400.0, 600.0, 3800.0, 2200.0));
    // Every click stays inside the box, whatever the edge.
    for c in [(0.0, 0.0), (2999.0, 0.0), (0.0, 1999.0), (2999.0, 1999.0)] {
        let r = rect(f, &[(c, 1.0)]).unwrap();
        assert!(c.0 >= r.x && c.0 <= r.x + r.w && c.1 >= r.y && c.1 <= r.y + r.h);
        assert!(r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= 3000.0 && r.y + r.h <= 2000.0);
    }
}

fn visible(g: &GuideBundle, i: usize) -> Option<(f64, f64, f64, f64)> {
    g.open_step(&g.steps()[i].id)
        .unwrap()
        .visible_rect()
        .map(r4)
}

fn add_at(g: &mut GuideBundle, w: u32, h: u32, x: f64, y: f64, seed: u8) -> String {
    g.add_click_step(
        &png(w, h, seed),
        click_scaled(x, y, 1.0),
        "Click",
        &Actor::User,
        T0,
    )
    .unwrap()
    .id
}

#[test]
fn every_step_of_a_guide_shares_one_crop_and_it_follows_the_clicks_gde_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = GuideBundle::create(tmp.path(), "Shared", serde_json::json!({}), T0).unwrap();
    // The reported guide: clicks near one side; each step used to centre on its own click.
    for (i, x) in [100.0, 200.0, 300.0].into_iter().enumerate() {
        add_at(&mut g, 3000, 2000, x, 1000.0, i as u8);
    }
    // While recording the earlier steps keep the rectangle of their time (rewriting all of them
    // on every click made a long guide slower with every step); the end of the recording makes
    // them equal.
    g.settle_crops(&Actor::User, T0).unwrap();
    let first = visible(&g, 0).unwrap();
    assert_eq!(first, (0.0, 600.0, 1400.0, 800.0));
    assert_eq!(visible(&g, 1), Some(first));
    assert_eq!(visible(&g, 2), Some(first));
    // A fourth click far away: the earlier steps follow.
    add_at(&mut g, 3000, 2000, 2800.0, 1000.0, 9);
    g.settle_crops(&Actor::User, T0).unwrap();
    let all = (0.0, 600.0, 3000.0, 800.0);
    for i in 0..4 {
        assert_eq!(visible(&g, i), Some(all), "step {i}");
    }
    // One step turns its crop off by hand; the others keep the guide crop.
    let id1 = g.steps()[1].id.clone();
    g.set_focus_crop(&id1, false, &Actor::User, T0).unwrap();
    assert_eq!(visible(&g, 1), None);
    assert_eq!(visible(&g, 0), Some(all));
    g.set_focus_crop(&id1, true, &Actor::User, T0).unwrap();
    assert_eq!(visible(&g, 1), Some(all));
    // Deleting the far step shrinks the box again, in every step.
    let last = g.steps()[3].id.clone();
    g.delete_step(&last, &Actor::User, T0).unwrap();
    for i in 0..3 {
        assert_eq!(visible(&g, i), Some(first), "step {i}");
    }
    // Undo brings the far step and the wide box back.
    g.undo(&Actor::User, T0).unwrap();
    assert_eq!(visible(&g, 0), Some(all));
    // Ring and badge stay in original-image coordinates, whatever the crop.
    let b = g.open_step(&g.steps()[0].id).unwrap();
    let ring = b
        .ops()
        .iter()
        .find_map(|o| match &o.kind {
            OpKind::Ellipse { rect, .. } => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert!((ring.x + ring.w / 2.0 - 100.0).abs() < 1.0);
    assert!((ring.y + ring.h / 2.0 - 1000.0).abs() < 1.0);
}

#[test]
fn a_second_frame_size_is_its_own_group_and_a_retake_moves_a_step_gde_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = GuideBundle::create(tmp.path(), "Groups", serde_json::json!({}), T0).unwrap();
    let a = add_at(&mut g, 3000, 2000, 500.0, 500.0, 1);
    // The display changed: a smaller frame is its own group with its own rectangle.
    add_at(&mut g, 2000, 1500, 1900.0, 1400.0, 2);
    assert_eq!(visible(&g, 0), Some((0.0, 100.0, 1200.0, 800.0)));
    assert_eq!(visible(&g, 1), Some((800.0, 700.0, 1200.0, 800.0)));
    // Retaking A on the small display puts both clicks in one group: the box covers the frame.
    g.retake_step(&a, &png(2000, 1500, 3), &Actor::User, T0)
        .unwrap();
    assert_eq!(visible(&g, 0), None);
    assert_eq!(visible(&g, 1), None);
}

#[test]
fn a_guide_of_a_window_keeps_the_whole_window_in_every_step_gde_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = GuideBundle::create(
        tmp.path(),
        "Window",
        serde_json::json!({ "window_id": 42 }),
        T0,
    )
    .unwrap();
    assert!(g.follows_window());
    // A frame much larger than the default focus: a display guide would crop it.
    add_at(&mut g, 3000, 2000, 500.0, 500.0, 1);
    add_at(&mut g, 3000, 2000, 2500.0, 1500.0, 2);
    assert_eq!(visible(&g, 0), None);
    assert_eq!(visible(&g, 1), None);
    assert!(!g.steps()[0].focus_crop);
    // A display guide still crops (see the tests above).
    let d = GuideBundle::create(
        tmp.path(),
        "Display",
        serde_json::json!({ "display_id": 1 }),
        T0,
    )
    .unwrap();
    assert!(!d.follows_window());
}
