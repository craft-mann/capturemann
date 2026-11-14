//! Click Guide (GDE-1 to GDE-4) through the daemon, with a scripted click source, the fake
//! screen and a scripted text recognizer: no real click, screen or OCR engine.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use cm_core::CmError;
use cm_daemon::guide::ScriptedClicks;
use cm_daemon::testing::{FixedConfirmer, RecordingClipboard};
use cm_daemon::{Daemon, Decision};
use cm_render::ocr::{TextLine, TextRecognizer};
use common::{builder, desktop, Client};
use image::RgbaImage;
use serde_json::{json, Value};

/// Five buttons side by side and an email address near the bottom; the same for every frame.
#[derive(Clone)]
struct Screen {
    with_email: bool,
}

impl TextRecognizer for Screen {
    fn name(&self) -> &'static str {
        "screen"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn recognize(&self, _: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        let mut v: Vec<TextLine> = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"]
            .iter()
            .enumerate()
            .map(|(i, t)| TextLine::laid_out(t, 20.0 + i as f64 * 70.0, 20.0, 8.0, 16.0))
            .collect();
        if self.with_email {
            v.push(TextLine::laid_out(
                "Owner: jane.doe@example.com",
                10.0,
                200.0,
                8.0,
                16.0,
            ));
        }
        Ok(v)
    }
}

struct Rig {
    daemon: Daemon,
    clicks: ScriptedClicks,
    clip: Arc<RecordingClipboard>,
    dir: tempfile::TempDir,
}

fn rig(clicks: ScriptedClicks, with_email: bool, decision: Decision) -> Rig {
    let (b, dir) = builder(desktop());
    let clip = Arc::new(RecordingClipboard::default());
    let daemon = b
        .guide_clicks(clicks.clone())
        .text_recognizer(Screen { with_email })
        .confirmer(Arc::new(FixedConfirmer::new(decision)))
        .clipboard(clip.clone())
        .build();
    Rig {
        daemon,
        clicks,
        clip,
        dir,
    }
}

fn guide_folder(r: &Rig) -> std::path::PathBuf {
    std::fs::read_dir(r.dir.path())
        .unwrap()
        .flatten()
        .find(|e| e.path().extension().is_some_and(|x| x == "cmguide"))
        .unwrap()
        .path()
}

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn code(r: &Value) -> &str {
    r["error"]["data"]["code"].as_str().unwrap()
}

async fn start(c: &mut Client) -> String {
    let r = c
        .call(
            "start_guide",
            json!({"target": {"display_id": 1}, "countdown_s": 0, "title": "Set up"}),
        )
        .await;
    assert_eq!(ok(&r)["state"], "recording", "{r}");
    ok(&r)["guide_id"].as_str().unwrap().to_owned()
}

/// Local px of a click on button `i` (the label sits at x = 20 + 70 i, y = 20).
fn on_button(i: usize) -> (f64, f64) {
    (24.0 + i as f64 * 70.0, 28.0)
}

#[tokio::test]
async fn gde_1_ac1_five_clicks_make_five_steps_in_order() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start(&mut c).await;
    for i in 0..5 {
        let (x, y) = on_button(i);
        assert!(r.clicks.click(x, y));
    }
    let done = c.call("stop_guide", json!({"guide_id": id})).await;
    let g = ok(&done);
    assert_eq!(g["state"], "stopped");
    let steps = g["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 5);
    let names = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];
    for (i, s) in steps.iter().enumerate() {
        assert_eq!(s["number"], i + 1);
        assert_eq!(s["title"], format!("Click \"{}\"", names[i]));
        let (x, y) = on_button(i);
        assert_eq!(s["click"]["x"].as_f64(), Some(x));
        assert_eq!(s["click"]["y"].as_f64(), Some(y));
        // The step picture is an ordinary screenshot: its ops hold the ring on the click and the
        // badge on the ring.
        let cap = c
            .call("get_capture", json!({"capture_id": s["image_id"]}))
            .await;
        let ops = ok(&cap)["ops"].as_array().unwrap().clone();
        let badge = ops.iter().find(|o| o["op"] == "step").unwrap();
        assert_eq!(badge["n"], i + 1);
        let at = badge["at"].as_array().unwrap();
        let ring = ops.iter().find(|o| o["op"] == "ellipse").expect("a ring");
        let r = ring["rect"].as_array().unwrap();
        let (rx, ry, rw, rh) = (
            r[0].as_f64().unwrap(),
            r[1].as_f64().unwrap(),
            r[2].as_f64().unwrap(),
            r[3].as_f64().unwrap(),
        );
        assert!((rx + rw / 2.0 - x).abs() <= 2.0);
        assert!((ry + rh / 2.0 - y).abs() <= 2.0);
        let dist = ((at[0].as_f64().unwrap() - x).powi(2) + (at[1].as_f64().unwrap() - y).powi(2))
            .sqrt();
        assert!(dist <= rw / 2.0 * 1.2, "badge {dist} from the click");
    }
    // The card picture is the rendered first step.
    assert!(guide_folder(&r).join("thumbs/poster.jpg").is_file());
    // The guide is in the library as its own kind.
    let list = c.call("list_library", json!({})).await;
    let items = ok(&list)["captures"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(items.iter().any(|x| x["id"] == id && x["kind"] == "guide"));
}

#[tokio::test]
async fn gde_1_ac2_hud_and_outside_clicks_add_no_step() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start(&mut c).await;
    r.daemon
        .set_guide_exclusions(vec![[0.0, 0.0, 100.0, 100.0]]);
    // On the HUD.
    r.clicks
        .click_at(50.0, 50.0, 50.0, 50.0, false, "left", 1_000_000_000);
    // Outside the followed target.
    r.clicks
        .click_at(-10.0, 5.0, 500.0, 500.0, true, "left", 1_000_000_000);
    // A real one.
    let (x, y) = on_button(2);
    r.clicks.click(x, y);
    let g = c.call("stop_guide", json!({"guide_id": id})).await;
    assert_eq!(ok(&g)["step_count"], 1);
}

#[tokio::test]
async fn gde_1_ac3_no_steps_while_paused() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start(&mut c).await;
    r.clicks.click(on_button(0).0, on_button(0).1);
    let p = c.call("set_guide", json!({"paused": true})).await;
    assert_eq!(ok(&p)["state"], "paused");
    for i in 1..4 {
        r.clicks.click(on_button(i).0, on_button(i).1);
    }
    c.call("set_guide", json!({"paused": false})).await;
    r.clicks.click(on_button(4).0, on_button(4).1);
    let g = c.call("stop_guide", json!({"guide_id": id})).await;
    let g = ok(&g);
    assert_eq!(g["step_count"], 2);
    assert_eq!(g["steps"][1]["title"], "Click \"Echo\"");
}

#[tokio::test]
async fn gde_1_double_click_is_one_step_and_a_note_step_is_added() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start(&mut c).await;
    let (x, y) = on_button(1);
    r.clicks.click_at(x, y, x, y, false, "left", 1_000_000_000);
    r.clicks.click_at(x, y, x, y, false, "left", 100_000_000);
    let n = c
        .call(
            "set_guide",
            json!({"note_step": {"title": "Type your password"}}),
        )
        .await;
    assert_eq!(ok(&n)["recorded_steps"], 2);
    r.clicks.click_at(x, y, x, y, false, "right", 2_000_000_000);
    let g = c.call("stop_guide", json!({"guide_id": id})).await;
    let steps = ok(&g)["steps"].as_array().unwrap().clone();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0]["title"], "Double-click \"Bravo\"");
    assert_eq!(steps[1]["type"], "note");
    assert_eq!(steps[2]["title"], "Right-click \"Bravo\"");
}

#[tokio::test]
async fn gde_1_ac4_a_step_with_an_email_is_marked_and_nothing_is_blurred() {
    let r = rig(ScriptedClicks::new(), true, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start(&mut c).await;
    r.clicks.click(on_button(0).0, on_button(0).1);
    let g = c.call("stop_guide", json!({"guide_id": id})).await;
    let s = &ok(&g)["steps"][0];
    assert_eq!(s["has_proposals"], true);
    let cap = c
        .call("get_capture", json!({"capture_id": s["image_id"]}))
        .await;
    let ops = ok(&cap)["ops"].as_array().unwrap().clone();
    assert!(ops
        .iter()
        .all(|o| o["op"] != "blur" && o["op"] != "pixelate"));
}

#[tokio::test]
async fn gde_1_ac5_ac6_a_missing_permission_records_nothing() {
    let err = CmError::permission_missing(
        cm_core::Permission::InputMonitoring,
        "Input Monitoring is off",
    );
    let r = rig(ScriptedClicks::failing(err), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let res = c
        .call(
            "start_guide",
            json!({"target": {"display_id": 1}, "countdown_s": 0}),
        )
        .await;
    assert_eq!(code(&res), "permission_missing", "{res}");
    assert!(!r.clicks.is_listening());
    let list = c.call("list_library", json!({})).await;
    assert!(ok(&list)["captures"].as_array().unwrap().is_empty());
    // Wayland: the reason comes back as not_implemented, the daemon carries on.
    let r = rig(
        ScriptedClicks::failing(CmError::not_implemented(
            "no input events on this Wayland session",
        )),
        false,
        Decision::Approve,
    );
    let mut c = Client::connect(&r.daemon, "agent");
    let res = c
        .call(
            "start_guide",
            json!({"target": {"display_id": 1}, "countdown_s": 0}),
        )
        .await;
    assert_eq!(code(&res), "not_implemented");
    ok(&c.call("list_targets", json!({})).await);
}

#[tokio::test]
async fn gde_4_start_guide_asks_first_and_declining_creates_nothing() {
    let r = rig(ScriptedClicks::new(), false, Decision::Decline);
    let mut c = Client::connect(&r.daemon, "agent");
    let res = c
        .call(
            "start_guide",
            json!({"target": {"display_id": 1}, "countdown_s": 0}),
        )
        .await;
    assert!(res.get("error").is_some());
    assert!(!r.clicks.is_listening());
    assert_eq!(
        std::fs::read_dir(r.dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "cmguide"))
            .count(),
        0
    );
}

async fn recorded(r: &Rig, c: &mut Client, n: usize) -> Value {
    let id = start(c).await;
    for i in 0..n {
        r.clicks.click(on_button(i).0, on_button(i).1);
    }
    ok(&c.call("stop_guide", json!({"guide_id": id})).await).clone()
}

#[tokio::test]
async fn gde_2_ac1_reorder_is_one_undo_step_and_renumbers() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let g = recorded(&r, &mut c, 4).await;
    let id = g["guide_id"].as_str().unwrap();
    let ids: Vec<String> = g["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    let order = json!([{"id": ids[0]}, {"id": ids[3], "title": "Was step 4"}, {"id": ids[1]}, {"id": ids[2]}]);
    let set = c
        .call("set_guide", json!({"guide_id": id, "steps": order}))
        .await;
    let steps = ok(&set)["steps"].as_array().unwrap().clone();
    assert_eq!(steps[1]["id"], ids[3]);
    assert_eq!(steps[1]["title"], "Was step 4");
    let cap = c
        .call("get_capture", json!({"capture_id": steps[1]["image_id"]}))
        .await;
    let badge = ok(&cap)["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["op"] == "step")
        .unwrap()
        .clone();
    assert_eq!(badge["n"], 2);
    // Titles, order and badges come back with ONE undo.
    let undo = c.call("undo", json!({"capture_id": id})).await;
    let back = &ok(&undo)["guide"]["steps"];
    assert_eq!(back[1]["id"], ids[1]);
    assert_ne!(back[1]["title"], "Was step 4");
    assert_eq!(ok(&undo)["can_undo"], false);
    let redo = c.call("redo", json!({"capture_id": id})).await;
    assert_eq!(ok(&redo)["guide"]["steps"][1]["id"], ids[3]);
}

#[tokio::test]
async fn gde_2_ac2_deleting_a_step_leaves_no_orphan_image() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let g = recorded(&r, &mut c, 3).await;
    let id = g["guide_id"].as_str().unwrap();
    let victim = g["steps"][1]["id"].as_str().unwrap();
    let set = c
        .call(
            "set_guide",
            json!({"guide_id": id, "steps": [{"id": victim, "delete": true}]}),
        )
        .await;
    assert_eq!(ok(&set)["step_count"], 2);
    let folder = std::fs::read_dir(r.dir.path())
        .unwrap()
        .flatten()
        .find(|e| e.path().extension().is_some_and(|x| x == "cmguide"))
        .unwrap()
        .path();
    assert_eq!(std::fs::read_dir(folder.join("steps")).unwrap().count(), 2);
}

#[tokio::test]
async fn gde_3_markdown_export_has_relative_images_and_the_blur_is_in_them() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let g = recorded(&r, &mut c, 5).await;
    let id = g["guide_id"].as_str().unwrap();
    // A pixelate over a region of step 3 (GDE-3.AC2): applied through the ordinary annotate.
    let step3 = g["steps"][2]["image_id"].as_str().unwrap();
    ok(&c
        .call(
            "annotate",
            json!({"capture_id": step3, "ops": [{"op": "pixelate", "rect": [200, 100, 100, 100], "block": 32}],
                   "return_image": false}),
        )
        .await);
    // A new blur makes the card picture stale: it is drawn again at once from the rendered
    // first step, never dropped (the Library card showed "No preview") nor left with old pixels.
    let poster = guide_folder(&r).join("thumbs/poster.jpg");
    assert!(poster.is_file());
    let before = std::fs::read(&poster).unwrap();
    let step1 = g["steps"][0]["image_id"].as_str().unwrap();
    ok(&c
        .call(
            "annotate",
            json!({"capture_id": step1, "ops": [{"op": "pixelate", "rect": [100, 100, 150, 150], "block": 32}],
                   "return_image": false}),
        )
        .await);
    assert!(poster.is_file());
    assert_ne!(
        std::fs::read(&poster).unwrap(),
        before,
        "the blur is in the poster"
    );
    let out = r.dir.path().join("export-md");
    let ex = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "markdown", "dest": out}),
        )
        .await;
    ok(&ex);
    let md = std::fs::read_to_string(out.join("guide.md")).unwrap();
    assert_eq!(md.matches("![").count(), 5);
    assert!(md.contains("# Set up"));
    assert!(md.contains("## 1. Click \"Alpha\""));
    assert!(md.contains("(images/step-03.png)"));
    assert!(
        !md.contains(r.dir.path().to_str().unwrap()),
        "no absolute paths"
    );
    for n in 1..=5 {
        assert!(out.join(format!("images/step-{n:02}.png")).is_file());
    }
    // The pixelated block holds one colour in the exported image, the original does not.
    let img = image::open(out.join("images/step-03.png"))
        .unwrap()
        .to_rgba8();
    let px = |x, y| *img.get_pixel(x, y);
    assert_eq!(px(205, 105), px(215, 115));
    let other = image::open(out.join("images/step-02.png"))
        .unwrap()
        .to_rgba8();
    assert_ne!(*other.get_pixel(205, 105), *other.get_pixel(215, 115));
}

#[tokio::test]
async fn gde_3_html_is_one_file_and_rich_text_reaches_the_clipboard() {
    let r = rig(ScriptedClicks::new(), true, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let g = recorded(&r, &mut c, 2).await;
    let id = g["guide_id"].as_str().unwrap();
    let out = r.dir.path().join("guide.html");
    let ex = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "html", "dest": out, "headings": "step", "image_scale": 2}),
        )
        .await;
    let res = ok(&ex);
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(html.contains("data:image/png;base64,"));
    assert!(html.contains("Step 1 · Click"));
    // Auto-redact proposals not reviewed: named, exported without blurs.
    assert!(res["steps_to_review"].as_array().unwrap().len() == 2);
    assert!(res["warnings"][0]
        .as_str()
        .unwrap()
        .contains("redaction proposals"));
    let rich = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "rich_text"}),
        )
        .await;
    assert_eq!(ok(&rich)["copied_to_clipboard"], true);
    assert!(r.clip.last_html().unwrap().contains("<img"));
    assert!(r.clip.last_text().unwrap().contains("Click \"Alpha\""));
    // PDF (GDE-3, spike S4): one file with the fonts embedded, on the paper asked for.
    let pdf_path = r.dir.path().join("guide.pdf");
    let pdf = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "pdf", "dest": pdf_path, "paper": "letter"}),
        )
        .await;
    ok(&pdf);
    let bytes = std::fs::read(&pdf_path).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(String::from_utf8_lossy(&bytes).contains("Geist"));
}

#[tokio::test]
async fn gde_1_stopping_with_no_step_keeps_no_guide() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start(&mut c).await;
    let g = c.call("stop_guide", json!({"guide_id": id})).await;
    assert_eq!(ok(&g)["discarded"], true);
    let list = c.call("list_library", json!({})).await;
    assert!(ok(&list)["captures"].as_array().unwrap().is_empty());
}

/// The Library card of a stopped guide: its size counts the step bundles and the poster, and the
/// stop is announced as a revision so the open Library reloads the row.
#[tokio::test]
async fn gde_1_a_stopped_guide_row_has_the_size_of_its_steps_and_a_poster() {
    let r = rig(ScriptedClicks::new(), false, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let g = recorded(&r, &mut c, 2).await;
    let id = g["guide_id"].as_str().unwrap();
    let list = c.call("list_library", json!({})).await;
    let row = &ok(&list)["captures"][0];
    assert_eq!(row["id"], id);
    let step_bytes: u64 = std::fs::read_dir(guide_folder(&r).join("steps"))
        .unwrap()
        .flatten()
        .map(|e| {
            std::fs::read_dir(e.path())
                .unwrap()
                .flatten()
                .map(|f| f.metadata().unwrap().len())
                .sum::<u64>()
        })
        .sum();
    assert!(step_bytes > 1000);
    assert!(row["size_bytes"].as_u64().unwrap() >= step_bytes, "{row}");
    assert!(guide_folder(&r).join("thumbs/poster.jpg").is_file());
}

/// A Retina display (scale 2): the step keeps the display scale, the focus crop is 1200 x 800
/// points (2400 x 1600 physical) and the title names text up to 80 points (160 px) away.
#[tokio::test]
async fn gde_1_retina_step_crops_in_logical_px_and_names_text_in_logical_px() {
    let (b, _dir) =
        builder(cm_capture::fake::FakeBackend::new().with_display(1, 0, 0, 2880, 1800, 2.0));
    let clicks = ScriptedClicks::new();
    let daemon = b
        .guide_clicks(clicks.clone())
        .text_recognizer(Screen { with_email: false })
        .confirmer(Arc::new(FixedConfirmer::new(Decision::Approve)))
        .build();
    let mut c = Client::connect(&daemon, "agent");
    let id = start(&mut c).await;
    // 130 physical px (65 points) below the "Alpha" label: named at scale 2.
    assert!(clicks.click(24.0, 36.0 + 130.0));
    let done = c.call("stop_guide", json!({"guide_id": id})).await;
    let s = &ok(&done)["steps"][0];
    assert_eq!(s["title"], "Click \"Alpha\"");
    assert_eq!(s["click"]["scale"].as_f64(), Some(2.0));
    let cap = c
        .call("get_capture", json!({"capture_id": s["image_id"]}))
        .await;
    let ops = ok(&cap)["ops"].as_array().unwrap().clone();
    let crop = ops.iter().find(|o| o["op"] == "crop").unwrap();
    assert_eq!(crop["rect"][2].as_f64(), Some(2400.0), "{crop}");
    assert_eq!(crop["rect"][3].as_f64(), Some(1600.0), "{crop}");
}
