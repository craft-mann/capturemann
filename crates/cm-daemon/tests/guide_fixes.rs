//! Click Guide fixes of the Nov 16, 2026 QA round (cg-engine): a fast burst of clicks, a window
//! that was moved, titles from the screen, the Markdown export, the readable confirmation
//! target. Scripted clicks, the fake screen and scripted text readers: nothing real.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cm_capture::fake::FakeBackend;
use cm_capture::{CaptureBackend, Capturer, DisplayInfo, Frame, WindowInfo};
use cm_core::CmError;
use cm_daemon::guide::ScriptedClicks;
use cm_daemon::testing::FixedConfirmer;
use cm_daemon::{Daemon, DaemonBuilder, Decision};
use cm_render::ocr::{TextLine, TextRecognizer};
use common::Client;
use image::RgbaImage;
use serde_json::{json, Value};

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

/// A desktop whose display 1 can be noise (a real screen does not compress like the fake's
/// pattern) and whose windows can be moved.
struct Wrap {
    inner: FakeBackend,
    noise: Option<Frame>,
    window_shift: Arc<Mutex<(i32, i32)>>,
}

impl CaptureBackend for Wrap {
    fn check_permission(&self) -> Result<(), CmError> {
        self.inner.check_permission()
    }
    fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
        self.inner.displays()
    }
    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        let (dx, dy) = *self.window_shift.lock().unwrap();
        let mut w = self.inner.windows()?;
        for x in &mut w {
            x.bounds.origin.x += dx;
            x.bounds.origin.y += dy;
        }
        Ok(w)
    }
    fn capture_display(&self, id: u32) -> Result<Frame, CmError> {
        match &self.noise {
            Some(f) if id == 1 => Ok(f.clone()),
            _ => self.inner.capture_display(id),
        }
    }
    fn capture_window(&self, id: u32) -> Result<Frame, CmError> {
        self.inner.capture_window(id)
    }
}

fn noise_frame(w: u32, h: u32) -> Frame {
    // Like a screen: flat areas (white, grey panels), text-like detail along many rows and a few
    // photo-like patches of noise; compresses to a few MB, not to nothing and not to the size of
    // raw noise.
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let mut v = vec![255u8; w as usize * h as usize * 4];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            let panel = if (x / 700 + y / 450) % 2 == 0 {
                255
            } else {
                238
            };
            let photo = (x / 300 + y / 300) % 7 == 3;
            let text = y % 24 < 14 && x % 11 < 6 && rnd() % 3 == 0;
            let c = if photo {
                (rnd() & 0xFF) as u8
            } else if text {
                30
            } else {
                panel
            };
            v[i] = c;
            v[i + 1] = c;
            v[i + 2] = c;
        }
    }
    Frame::new(w, h, v).unwrap()
}

/// A reader that takes `ms` per call and finds the lines `lines`.
struct Reader {
    ms: u64,
    lines: Vec<TextLine>,
    calls: Arc<AtomicU64>,
}

impl TextRecognizer for Reader {
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn recognize(&self, _: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.ms > 0 {
            std::thread::sleep(Duration::from_millis(self.ms));
        }
        Ok(self.lines.clone())
    }
}

struct Rig {
    daemon: Daemon,
    clicks: ScriptedClicks,
    shift: Arc<Mutex<(i32, i32)>>,
    confirmer: Arc<FixedConfirmer>,
    dir: tempfile::TempDir,
}

fn rig_with(
    noise: Option<(u32, u32)>,
    reader: Reader,
    settings: Option<serde_json::Map<String, Value>>,
    decision: Decision,
) -> Rig {
    let shift = Arc::new(Mutex::new((0, 0)));
    let (w, h) = noise.unwrap_or((400, 300));
    let inner = FakeBackend::new()
        .with_display(1, 0, 0, w, h, 2.0)
        .with_window(10, "Editor", "Code", 100, 1, (20, 10, 100, 80), false);
    let backend = Wrap {
        inner,
        noise: noise.map(|(w, h)| noise_frame(w, h)),
        window_shift: shift.clone(),
    };
    let dir = tempfile::tempdir().unwrap();
    let lib = cm_library::Library::open_or_create(dir.path()).unwrap();
    let clicks = ScriptedClicks::new();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let mut b: DaemonBuilder = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib)
        .guide_clicks(clicks.clone())
        .text_recognizer(reader)
        .confirmer(confirmer.clone());
    if let Some(s) = settings {
        b = b.settings(cm_daemon::MemorySettings::with_defaults(s));
    }
    Rig {
        daemon: b.build(),
        clicks,
        shift,
        confirmer,
        dir,
    }
}

fn plain_reader() -> Reader {
    Reader {
        ms: 0,
        lines: Vec::new(),
        calls: Arc::default(),
    }
}

async fn start_target(c: &mut Client, target: Value) -> String {
    let r = c
        .call(
            "start_guide",
            json!({"target": target, "countdown_s": 0, "title": "Set up"}),
        )
        .await;
    assert_eq!(ok(&r)["state"], "recording", "{r}");
    ok(&r)["guide_id"].as_str().unwrap().to_owned()
}

/// `n` clicks `gap_ms` apart on a 2880 x 1800 display read by a text reader that takes 150 ms;
/// returns how many steps the guide kept and prints the numbers.
async fn burst(n: u32, gap_ms: u64) -> u64 {
    let reader = Reader {
        ms: 150,
        lines: vec![TextLine::laid_out("Save", 10.0, 10.0, 8.0, 16.0)],
        calls: Arc::default(),
    };
    let r = rig_with(Some((2880, 1800)), reader, None, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start_target(&mut c, json!({"display_id": 1})).await;
    let t0 = Instant::now();
    for i in 0..n {
        r.clicks.click(20.0 + f64::from(i), 28.0);
        tokio::time::sleep(Duration::from_millis(gap_ms)).await;
    }
    let clicking = t0.elapsed();
    let done = c.call("stop_guide", json!({"guide_id": id})).await;
    let kept = ok(&done)["step_count"].as_u64().unwrap();
    let total = t0.elapsed();
    println!(
        "burst of {n} clicks every {gap_ms} ms: {kept} kept, clicking took {clicking:?}, stop done \
         at {total:?} ({:.1} steps/s written)",
        kept as f64 / total.as_secs_f64()
    );
    kept
}

/// QA macOS pass 7: 520 clicks 100 ms apart kept 58 (0.8 steps a second written, with repeated
/// "click was not saved" toasts). The frame is written without waiting for the text read, the
/// PNG is cheaper and made off the click path: at 5 clicks a second every click is kept, and a
/// burst of 10 a second keeps nearly all of the first minute.
#[tokio::test]
async fn cg_five_clicks_a_second_are_all_kept() {
    assert_eq!(burst(40, 200).await, 40);
}

#[tokio::test]
async fn cg_a_burst_of_ten_clicks_a_second_keeps_most() {
    let kept = burst(60, 100).await;
    assert!(kept >= 54, "only {kept} of 60 clicks were kept");
}

/// A followed window that was moved after the guide started: the click is mapped to where the
/// window is now, not where it was (QA macOS pass 7).
#[tokio::test]
async fn cg_a_moved_window_still_gets_the_ring_on_the_clicked_place() {
    let r = rig_with(None, plain_reader(), None, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start_target(&mut c, json!({"window_id": 10})).await;
    // The window moves 50 right and 40 down; the click was on its button at (30, 28), which the
    // OS mapped with the rectangle of the start.
    *r.shift.lock().unwrap() = (50, 40);
    r.clicks.click(30.0 + 50.0, 28.0 + 40.0);
    let done = c.call("stop_guide", json!({"guide_id": id})).await;
    let s = &ok(&done)["steps"][0];
    assert_eq!(s["click"]["x"].as_f64(), Some(30.0), "{s}");
    assert_eq!(s["click"]["y"].as_f64(), Some(28.0), "{s}");
}

fn line_near_click(t: &str) -> Vec<TextLine> {
    vec![TextLine::laid_out(t, 10.0, 10.0, 8.0, 16.0)]
}

async fn click_and_stop(r: &Rig, c: &mut Client, id: &str, n: u32) -> Value {
    for i in 0..n {
        r.clicks.click(20.0 + f64::from(i) * 3.0, 28.0);
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    ok(&c.call("stop_guide", json!({"guide_id": id})).await).clone()
}

/// QA Linux pass 1: `Click "ROW 0006) EEE"`. Text that is not a label gives the generic title,
/// and the guide says which titles were drafted from the screen and which are generic.
#[tokio::test]
async fn cg_titles_use_screen_text_only_when_it_looks_like_a_label() {
    for (text, title, source) in [
        ("Save", "Click \"Save\"", "screen_text"),
        ("ROW 0006) EEE", "Click here", "generic"),
        ("jane.doe@example.com", "Click here", "generic"),
    ] {
        let reader = Reader {
            ms: 0,
            lines: line_near_click(text),
            calls: Arc::default(),
        };
        let r = rig_with(None, reader, None, Decision::Approve);
        let mut c = Client::connect(&r.daemon, "agent");
        let id = start_target(&mut c, json!({"display_id": 1})).await;
        let done = click_and_stop(&r, &mut c, &id, 1).await;
        let s = &done["steps"][0];
        assert_eq!(s["title"], title, "{text}: {s}");
        assert_eq!(s["title_source"], source, "{text}: {s}");
    }
}

/// `guide.titles_from_screen` off: no screen text in any title, and the screen is not read.
#[tokio::test]
async fn cg_titles_from_screen_can_be_turned_off_and_then_nothing_is_read() {
    let calls = Arc::new(AtomicU64::new(0));
    let reader = Reader {
        ms: 0,
        lines: line_near_click("Save"),
        calls: calls.clone(),
    };
    let mut settings = serde_json::Map::new();
    settings.insert("guide.titles_from_screen".into(), json!(false));
    let r = rig_with(None, reader, Some(settings), Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start_target(&mut c, json!({"display_id": 1})).await;
    let done = click_and_stop(&r, &mut c, &id, 3).await;
    for s in done["steps"].as_array().unwrap() {
        assert_eq!(s["title"], "Click here");
        assert_eq!(s["title_source"], "generic");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the screen was read");
    // An agent cannot turn it back on: the family is not one an agent writes.
    let r = c
        .call(
            "set_settings",
            json!({"values": {"guide.titles_from_screen": true}}),
        )
        .await;
    assert!(r.get("error").is_some(), "{r}");
}

/// A text reader that has stalled does not hold up the steps: they are written at once with
/// the generic title, and the titles that were read arrive when it moves again.
#[tokio::test]
async fn cg_a_stalled_text_reader_does_not_stop_the_steps() {
    struct Gate(Arc<std::sync::atomic::AtomicBool>);
    impl TextRecognizer for Gate {
        fn name(&self) -> &'static str {
            "gate"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn recognize(&self, _: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
            while !self.0.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(line_near_click("Save"))
        }
    }
    let open = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shift = Arc::new(Mutex::new((0, 0)));
    let backend = Wrap {
        inner: FakeBackend::new().with_display(1, 0, 0, 400, 300, 2.0),
        noise: None,
        window_shift: shift,
    };
    let dir = tempfile::tempdir().unwrap();
    let lib = cm_library::Library::open_or_create(dir.path()).unwrap();
    let clicks = ScriptedClicks::new();
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib)
        .guide_clicks(clicks.clone())
        .text_recognizer(Gate(open.clone()))
        .confirmer(Arc::new(FixedConfirmer::new(Decision::Approve)))
        .build();
    let mut c = Client::connect(&daemon, "agent");
    let id = start_target(&mut c, json!({"display_id": 1})).await;
    for i in 0..12 {
        clicks.click(20.0 + f64::from(i) * 4.0, 28.0);
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    let mut recorded = 0;
    for _ in 0..100 {
        let g = ok(&c.call("get_guide", json!({"guide_id": id})).await).clone();
        recorded = g["recorded_steps"].as_u64().unwrap();
        if recorded == 12 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(recorded, 12);
    open.store(true, Ordering::SeqCst);
    let done = ok(&c.call("stop_guide", json!({"guide_id": id})).await).clone();
    let steps = done["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 12);
    let named = steps
        .iter()
        .filter(|s| s["title_source"] == "screen_text")
        .count();
    // The reader holds one and four wait; the later ones keep the generic title.
    assert!((5..12).contains(&named), "{named} named");
    assert_eq!(steps[11]["title"], "Click here");
}

/// QA macOS pass 7: the Markdown export wrote over the files of an earlier export and kept live
/// links of a title. It writes into a new folder, tells the path, and writes titles as text.
#[tokio::test]
async fn cg_markdown_export_never_overwrites_and_writes_titles_as_plain_text() {
    let r = rig_with(None, plain_reader(), None, Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = start_target(&mut c, json!({"display_id": 1})).await;
    let done = click_and_stop(&r, &mut c, &id, 2).await;
    let step = done["steps"][0]["id"].as_str().unwrap().to_owned();
    let evil = "[Open](https://evil.example/x) <img src=x> `rm` www.evil.example";
    ok(&c
        .call(
            "set_guide",
            json!({"guide_id": id, "title": evil, "steps": [{"id": step, "title": evil}]}),
        )
        .await);
    let out = r.dir.path().join("exports");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("guide.md"), "my own notes").unwrap();
    let ex = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "markdown", "dest": out}),
        )
        .await;
    let ex = ok(&ex);
    let path = std::path::PathBuf::from(ex["path"].as_str().unwrap());
    assert_ne!(path, out, "{ex}");
    assert_eq!(ex["renamed"], true);
    assert!(path.starts_with(&out) && path.join("guide.md").is_file());
    assert_eq!(
        std::fs::read_to_string(out.join("guide.md")).unwrap(),
        "my own notes"
    );
    let md = std::fs::read_to_string(path.join("guide.md")).unwrap();
    assert!(!md.contains("](https"), "{md}");
    assert!(!md.replace("\\<", "").contains('<'), "{md}");
    assert!(!md.contains("https://"), "{md}");
    assert!(!md.contains("www.evil"), "{md}");
    assert!(md.contains("\\[Open\\]"), "{md}");
    // Again: a second folder, the first untouched.
    let ex2 = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "markdown", "dest": out}),
        )
        .await;
    let path2 = std::path::PathBuf::from(ok(&ex2)["path"].as_str().unwrap());
    assert_ne!(path2, path);
    assert!(path.join("guide.md").is_file() && path2.join("guide.md").is_file());
    // HTML to a file that is there: `name (2).html`.
    let file = r.dir.path().join("g.html");
    std::fs::write(&file, "old").unwrap();
    let h = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "html", "dest": file}),
        )
        .await;
    let h = ok(&h);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
    assert!(h["path"].as_str().unwrap().ends_with("g (2).html"), "{h}");
}

/// QA macOS pass 7: the confirmation of a Click Guide (and of a Long Shot) showed its target as
/// `{..}`. It says `Display 1` or `Window: Editor (Code)`.
#[tokio::test]
async fn cg_the_confirmation_names_the_target_in_words() {
    let r = rig_with(None, plain_reader(), None, Decision::Decline);
    let mut c = Client::connect(&r.daemon, "agent");
    let g = c
        .call(
            "start_guide",
            json!({"target": {"window_id": 10}, "countdown_s": 0}),
        )
        .await;
    assert!(g.get("error").is_some(), "declined: {g}");
    let l = c
        .call(
            "capture_screenshot",
            json!({"mode": "long", "target": {"display_id": 1}, "scroll": "auto"}),
        )
        .await;
    assert!(l.get("error").is_some(), "declined: {l}");
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert_eq!(shown[0].details["target"], "Window: Editor (Code)");
    assert_eq!(shown[0].params["target"]["type"], "window");
    assert_eq!(shown[0].params["target"]["title"], "Editor");
    assert_eq!(shown[0].params["target"]["app"], "Code");
    assert_eq!(shown[1].details["target"], "Display 1", "{:?}", shown[1]);
    for v in shown[1].details.as_object().unwrap().values() {
        assert!(
            !v.as_str().unwrap_or_default().contains('{'),
            "{:?}",
            shown[1]
        );
    }
}

/// The same for the generic dialog of a tool the user set to Ask first.
#[tokio::test]
async fn cg_the_ask_first_dialog_names_the_target_in_words() {
    let mut settings = serde_json::Map::new();
    settings.insert(
        "agents.tool_policy".into(),
        json!({ "capture_screenshot": "ask" }),
    );
    let r = rig_with(None, plain_reader(), Some(settings), Decision::Decline);
    let mut c = Client::connect(&r.daemon, "agent");
    let _ = c
        .call(
            "capture_screenshot",
            json!({"mode": "long", "target": {"window_id": 10}, "scroll": "auto"}),
        )
        .await;
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].details["target"], "Window: Editor (Code)");
    for v in shown[0].details.as_object().unwrap().values() {
        assert!(
            !v.as_str().unwrap_or_default().contains('{'),
            "{:?}",
            shown[0]
        );
    }
}
