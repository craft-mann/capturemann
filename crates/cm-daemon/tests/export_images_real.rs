//! BLK-1 / ANN-11 with the REAL OS text recognizer on a SYNTHETIC picture (text drawn by the
//! project's own renderer into the bundle's `original.png`; no screen, camera or microphone, the
//! capture comes from the scripted fake display). OCR finds the lines in the plain export and
//! none of them in the Blueprint export.
//!
//! Ignored: prints SKIPPED unless `CM_OCR_REAL=1` and a recognizer is installed.
//! `CM_OCR_REAL=1 cargo test -p cm-daemon --test export_images_real -- --ignored --nocapture`
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use cm_core::Actor;
use cm_daemon::testing::{FakeTrasher, FixedConfirmer};
use cm_daemon::Decision;
use cm_render::ocr::system_recognizer;
use common::{builder, desktop};
use serde_json::{json, Value};

fn read(img: &image::RgbaImage) -> String {
    system_recognizer()
        .recognize(img)
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join(" ")
}

#[tokio::test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
async fn blueprint_export_leaves_no_text_for_ocr() {
    if std::env::var("CM_OCR_REAL").as_deref() != Ok("1") || !system_recognizer().is_available() {
        println!("SKIPPED: set CM_OCR_REAL=1 and install a text recognizer");
        return;
    }
    let (b, lib) = builder(desktop());
    let trash = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let daemon = b
        .confirmer(Arc::new(FixedConfirmer::new(Decision::Approve)))
        .trasher(Arc::new(FakeTrasher::into_dir(trash.path())))
        .jobs(cm_daemon::library_job_queue(lib.path()).unwrap())
        .build();
    daemon.register_export_handler().unwrap();
    daemon.start_jobs().unwrap();
    let call = |m: &'static str, p: Value| {
        let d = daemon.clone();
        async move { d.execute_method(Actor::User, 1, m, &p).await.unwrap() }
    };
    let shot = call(
        "capture_screenshot",
        json!({"target": {"display_id": 1}, "title": "Words"}),
    )
    .await;
    let id = shot["capture_id"].as_str().unwrap().to_owned();
    let mut img = image::RgbaImage::from_pixel(700, 240, image::Rgba([245, 245, 247, 255]));
    for (i, t) in ["Account settings", "Billing address", "Delete project"]
        .iter()
        .enumerate()
    {
        let j = format!(
            r##"[{{"op":"text","at":[30,{}],"text":"{t}","size":30,"color":"#161616","outline":null}}]"##,
            24 + i * 70
        );
        img = cm_render::render_annotations(&img, &cm_render::parse_ops_str(&j).unwrap()).unwrap();
    }
    // The master is read-only after capture: this test swaps in a synthetic picture.
    let master = lib.path().join("Words.cmshot").join("original.png");
    let mut perm = std::fs::metadata(&master).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&master, perm).unwrap();
    img.save(&master).unwrap();
    for (blueprint, name) in [(false, "plain"), (true, "plan")] {
        let job = call(
            "export_images",
            json!({"capture_ids": [id.clone()], "dest": out.path(), "blueprint": blueprint,
                   "name_pattern": name}),
        )
        .await;
        let job_id = job["job_id"].as_str().unwrap().to_owned();
        let mut done = Value::Null;
        for _ in 0..1500 {
            let j = call("get_job", json!({"job_id": job_id})).await;
            if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
                done = j;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(done["result"]["exported"], 1, "{done}");
    }
    let open = |n: &str| image::open(out.path().join(n)).unwrap().to_rgba8();
    let plain = read(&open("plain.png"));
    let plan = read(&open("plan.png"));
    println!("plain: {plain:?}\nblueprint: {plan:?}");
    // Control: the plain export is readable, so the check can fail.
    assert!(
        plain.contains("Account") && plain.contains("Billing"),
        "{plain}"
    );
    for w in [
        "Account", "Billing", "Delete", "settings", "address", "project",
    ] {
        assert!(!plan.contains(w), "`{w}` still readable: {plan}");
    }
}
