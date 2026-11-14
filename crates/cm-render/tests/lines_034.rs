//! Recognition-box quality for Blueprint and Retype (0.3.4 QA D2 and D3): split merged lines,
//! drop non-text shapes, cover glyph edges. Pure tests use scripted recognizers; the measuring
//! test (`fixr_`) runs the real OS recognizer on synthetic dialogs and prints before/after counts.
//! `CM_OCR_REAL=1 cargo test -p cm-render --test lines_034 fixr_ -- --ignored --nocapture`
//! (`CM_FIXR_OUT=<dir>` writes the scenes and their Blueprint renders as PNGs).
#![allow(clippy::unwrap_used)]

use cm_core::annotation::BlueprintStyle;
use cm_core::CmError;
use cm_render::ocr::{refine_lines, TextLine, TextRecognizer};
use cm_render::{blueprint_bars, measure_blueprint, parse_ops_str, render_annotations, Rc};
use image::{Rgba, RgbaImage};

fn rect_px(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, c: [u8; 3]) {
    for yy in y..(y + h).min(img.height()) {
        for xx in x..(x + w).min(img.width()) {
            img.put_pixel(xx, yy, Rgba([c[0], c[1], c[2], 255]));
        }
    }
}

fn disc(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, c: [u8; 3]) {
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            let inside = (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
            if inside && x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                img.put_pixel(x as u32, y as u32, Rgba([c[0], c[1], c[2], 255]));
            }
        }
    }
}

/// A scene: the picture, the same picture without text, and the text's ground-truth boxes.
struct Scene {
    name: &'static str,
    img: RgbaImage,
    clean: RgbaImage,
    truth: Vec<Rc>,
}

impl Scene {
    fn new(name: &'static str, w: u32, h: u32, bg: [u8; 3]) -> Scene {
        let img = RgbaImage::from_pixel(w, h, Rgba([bg[0], bg[1], bg[2], 255]));
        Scene {
            name,
            clean: img.clone(),
            img,
            truth: Vec::new(),
        }
    }
    fn shape(&mut self, f: impl Fn(&mut RgbaImage)) {
        f(&mut self.img);
        f(&mut self.clean);
    }
    fn text(&mut self, x: u32, y: u32, t: &str, size: u32, color: &str) {
        let j = format!(
            r##"[{{"op":"text","at":[{x},{y}],"text":"{t}","size":{size},"color":"{color}","outline":null}}]"##
        );
        let after = render_annotations(&self.img, &parse_ops_str(&j).unwrap()).unwrap();
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for (xx, yy, p) in after.enumerate_pixels() {
            if p != self.img.get_pixel(xx, yy) {
                x0 = x0.min(xx);
                y0 = y0.min(yy);
                x1 = x1.max(xx);
                y1 = y1.max(yy);
            }
        }
        self.truth.push(Rc::new(
            f64::from(x0),
            f64::from(y0),
            f64::from(x1 - x0 + 1),
            f64::from(y1 - y0 + 1),
        ));
        self.img = after;
    }
}

fn scenes() -> Vec<Scene> {
    let mut v = Vec::new();
    // 1: buttons side by side (blue and grey).
    let mut s = Scene::new("buttons", 700, 300, [236, 236, 236]);
    s.text(40, 30, "Account name: alice.example", 16, "#161616");
    s.shape(|i| {
        rect_px(i, 380, 220, 130, 36, [0, 122, 255]);
        rect_px(i, 530, 220, 130, 36, [0, 122, 255]);
        rect_px(i, 40, 220, 100, 32, [210, 210, 210]);
        rect_px(i, 150, 220, 100, 32, [210, 210, 210]);
        rect_px(i, 260, 220, 100, 32, [210, 210, 210]);
    });
    s.text(425, 229, "Save", 16, "#ffffff");
    s.text(568, 229, "Cancel", 16, "#ffffff");
    s.text(72, 227, "Help", 16, "#161616");
    s.text(176, 227, "Undo", 16, "#161616");
    s.text(284, 227, "Reset", 16, "#161616");
    v.push(s);
    // 2: round icons and a badge next to text.
    let mut s = Scene::new("icons", 700, 300, [245, 245, 247]);
    s.shape(|i| {
        disc(i, 60, 50, 26, [0, 122, 255]);
        rect_px(i, 52, 42, 16, 16, [255, 255, 255]);
        disc(i, 60, 130, 24, [60, 180, 90]);
        disc(i, 60, 210, 24, [230, 90, 60]);
        rect_px(i, 54, 195, 12, 30, [255, 255, 255]);
        disc(i, 600, 60, 14, [220, 40, 40]);
        disc(i, 600, 150, 30, [40, 40, 40]);
        disc(i, 600, 150, 22, [245, 245, 247]);
    });
    s.text(110, 38, "Network settings", 18, "#161616");
    s.text(110, 118, "Notifications", 18, "#161616");
    s.text(110, 198, "Storage and backups", 18, "#161616");
    s.text(594, 53, "3", 14, "#ffffff");
    v.push(s);
    // 3: tabs.
    let mut s = Scene::new("tabs", 700, 200, [225, 225, 228]);
    s.shape(|i| {
        rect_px(i, 20, 40, 160, 36, [250, 250, 250]);
        rect_px(i, 190, 40, 160, 36, [205, 205, 210]);
        rect_px(i, 360, 40, 160, 36, [205, 205, 210]);
    });
    s.text(60, 49, "Overview", 16, "#161616");
    s.text(235, 49, "Settings", 16, "#161616");
    s.text(405, 49, "Profile", 16, "#161616");
    s.text(24, 120, "Last synced 4 minutes ago", 14, "#505050");
    v.push(s);
    // 4: a settings list with icons and right-aligned values.
    let mut s = Scene::new("list", 700, 360, [250, 250, 250]);
    for k in 0..6u32 {
        let y = 20 + k * 52;
        s.shape(|i| {
            rect_px(
                i,
                24,
                y + 4,
                28,
                28,
                [90 + (k * 25) as u8, 120, 220 - (k * 20) as u8],
            );
            rect_px(i, 24, y + 44, 640, 1, [220, 220, 220]);
        });
    }
    let labels = [
        ("Wi-Fi", "Home network"),
        ("Bluetooth", "On"),
        ("Battery", "87 percent"),
        ("Sound", "Speakers"),
        ("Display", "Built-in"),
        ("Privacy", "Review"),
    ];
    for (k, (a, b)) in labels.iter().enumerate() {
        let y = 20 + k as u32 * 52;
        s.text(70, y + 8, a, 17, "#161616");
        s.text(470, y + 8, b, 17, "#707070");
    }
    v.push(s);
    // 5: dark dialog, three buttons close together.
    let mut s = Scene::new("dark", 700, 240, [32, 32, 36]);
    s.shape(|i| {
        rect_px(i, 330, 170, 100, 34, [70, 70, 78]);
        rect_px(i, 440, 170, 100, 34, [70, 70, 78]);
        rect_px(i, 550, 170, 110, 34, [10, 132, 255]);
    });
    s.text(30, 30, "Delete this item permanently?", 20, "#f0f0f0");
    s.text(30, 70, "This cannot be undone.", 15, "#a0a0a0");
    s.text(362, 177, "Skip", 15, "#f0f0f0");
    s.text(468, 177, "Later", 15, "#f0f0f0");
    s.text(584, 177, "Delete", 15, "#ffffff");
    v.push(s);
    // 6: words with wide gaps on one baseline: a menu bar, a table header and two small buttons.
    let mut s = Scene::new("gaps", 700, 260, [246, 246, 246]);
    s.shape(|i| {
        rect_px(i, 440, 190, 90, 30, [0, 122, 255]);
        rect_px(i, 550, 190, 90, 30, [0, 122, 255]);
    });
    for (x, t) in [
        (20u32, "File"),
        (80, "Edit"),
        (140, "View"),
        (210, "Window"),
        (300, "Help"),
    ] {
        s.text(x, 14, t, 15, "#161616");
    }
    for (x, t) in [(24u32, "Name"), (250, "Date modified"), (470, "Size")] {
        s.text(x, 80, t, 15, "#505050");
    }
    s.text(24, 120, "report.pdf", 15, "#161616");
    s.text(250, 120, "Nov 12, 2026", 15, "#161616");
    s.text(470, 120, "2.1 MB", 15, "#161616");
    s.text(464, 197, "Yes", 15, "#ffffff");
    s.text(576, 197, "No", 15, "#ffffff");
    v.push(s);
    v
}

fn overlap(a: Rc, b: Rc) -> bool {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    x1 > x0 && y1 > y0
}

#[derive(Default, Debug)]
struct Score {
    boxes: usize,
    merged: usize,
    non_text: usize,
    missed: usize,
    damage_px: usize,
    sliver_px: usize,
    junk_tokens: usize,
}

fn eval(rec: &dyn TextRecognizer, s: &Scene, refine: bool, out: Option<&std::path::Path>) -> Score {
    let mut lines: Vec<TextLine> = rec
        .recognize(&s.img)
        .unwrap()
        .into_iter()
        .filter(|l| !l.text.trim().is_empty())
        .collect();
    if refine {
        lines = refine_lines(rec, &s.img, lines);
    }
    let rects: Vec<Rc> = lines
        .iter()
        .map(|l| {
            let r = l.rect;
            let (x0, y0) = (r.x.floor(), r.y.floor());
            Rc::new(x0, y0, (r.x + r.w).ceil() - x0, (r.y + r.h).ceil() - y0)
        })
        .collect();
    let mut sc = Score {
        boxes: rects.len(),
        ..Score::default()
    };
    for r in &rects {
        let n = s.truth.iter().filter(|t| overlap(*r, **t)).count();
        if n >= 2 {
            sc.merged += 1;
        }
        if n == 0 {
            sc.non_text += 1;
        }
    }
    sc.missed = s
        .truth
        .iter()
        .filter(|t| !rects.iter().any(|r| overlap(*r, **t)))
        .count();
    let styles = measure_blueprint(&s.img, &rects);
    let j = serde_json::json!([{
        "op": "blueprint", "boxes": rects, "style": "bar", "color": "auto", "radius": 3,
        "colors": styles.iter().map(|x| x.color).collect::<Vec<_>>(),
        "fills": styles.iter().map(|x| x.fill.clone()).collect::<Vec<_>>(),
    }]);
    let render = render_annotations(&s.img, &parse_ops_str(&j.to_string()).unwrap()).unwrap();
    let bars = blueprint_bars(&rects, BlueprintStyle::Bar);
    let in_bar = |x: u32, y: u32| {
        bars.iter().any(|b| {
            i64::from(x) >= b[0]
                && i64::from(x) < b[2]
                && i64::from(y) >= b[1]
                && i64::from(y) < b[3]
        })
    };
    let near = |x: u32, y: u32, pad: f64| {
        s.truth.iter().any(|t| {
            f64::from(x) >= t.x - pad
                && f64::from(x) < t.x + t.w + pad
                && f64::from(y) >= t.y - pad
                && f64::from(y) < t.y + t.h + pad
        })
    };
    let far = |a: &Rgba<u8>, b: &Rgba<u8>, tol: i32| {
        (0..3).any(|i| (i32::from(a.0[i]) - i32::from(b.0[i])).abs() >= tol)
    };
    for (x, y, p) in render.enumerate_pixels() {
        if !near(x, y, 3.0) && far(p, s.img.get_pixel(x, y), 8) {
            sc.damage_px += 1;
        }
        if near(x, y, 2.0) && !in_bar(x, y) && far(p, s.clean.get_pixel(x, y), 40) {
            sc.sliver_px += 1;
        }
    }
    let junk: Vec<_> = rec
        .recognize(&render)
        .unwrap()
        .into_iter()
        .filter(|l| !l.text.trim().is_empty())
        .collect();
    for j in &junk {
        println!(
            "  junk in {} (refine {refine}): {:?} {:?}",
            s.name, j.text, j.rect
        );
    }
    sc.junk_tokens = junk.len();
    if let Some(dir) = out {
        std::fs::create_dir_all(dir).unwrap();
        let tag = if refine { "after" } else { "before" };
        render
            .save(dir.join(format!("{}-{tag}.png", s.name)))
            .unwrap();
        if !refine {
            s.img.save(dir.join(format!("{}.png", s.name))).unwrap();
        }
    }
    sc
}

#[test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
fn fixr_synthetic_dialogs_before_and_after() {
    use cm_render::ocr::system_recognizer;
    let rec = system_recognizer();
    if std::env::var("CM_OCR_REAL").as_deref() != Ok("1") || !rec.is_available() {
        println!("SKIPPED: set CM_OCR_REAL=1 with a text recognizer");
        return;
    }
    let out = std::env::var("CM_FIXR_OUT")
        .ok()
        .map(std::path::PathBuf::from);
    let mut tot = [Score::default(), Score::default()];
    for s in scenes() {
        for (k, refine) in [false, true].into_iter().enumerate() {
            let sc = eval(rec.as_ref(), &s, refine, out.as_deref());
            println!(
                "{:<8} {}: {sc:?}",
                s.name,
                if refine { "after " } else { "before" }
            );
            let t = &mut tot[k];
            t.boxes += sc.boxes;
            t.merged += sc.merged;
            t.non_text += sc.non_text;
            t.missed += sc.missed;
            t.damage_px += sc.damage_px;
            t.sliver_px += sc.sliver_px;
            t.junk_tokens += sc.junk_tokens;
        }
    }
    println!("TOTAL before: {:?}\nTOTAL after:  {:?}", tot[0], tot[1]);
}

// ------------------------------------------------------------ scripted-recognizer tests

struct Scripted(Vec<TextLine>);

impl TextRecognizer for Scripted {
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn recognize(&self, _i: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        Ok(self.0.clone())
    }
}

#[test]
#[ignore = "debug print; set CM_OCR_REAL=1"]
fn fixr_dump_lines() {
    use cm_render::ocr::system_recognizer;
    let rec = system_recognizer();
    for s in scenes() {
        for l in rec.recognize(&s.img).unwrap() {
            println!(
                "{} {:?} {:?} conf {} words {}",
                s.name,
                l.text,
                l.rect,
                l.confidence,
                l.words.len()
            );
        }
        println!("{} truth {:?}", s.name, s.truth);
    }
}

#[test]
fn a_line_without_a_gap_is_left_alone() {
    let mut s = Scene::new("plain", 300, 60, [240, 240, 240]);
    s.text(20, 20, "Hello there world", 16, "#101010");
    let line = TextLine::laid_out("Hello there world", 20.0, 20.0, 8.0, 18.0);
    let out = refine_lines(&Scripted(vec![line.clone()]), &s.img, vec![line.clone()]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, line.text);
}

/// Reads every crop as one line of text covering it (what a real recognizer does for one word).
struct WholeCrop;

impl TextRecognizer for WholeCrop {
    fn name(&self) -> &'static str {
        "whole_crop"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn recognize(&self, i: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        Ok(vec![TextLine {
            text: "piece".to_owned(),
            rect: Rc::new(0.0, 0.0, f64::from(i.width()), f64::from(i.height())),
            confidence: 0.9,
            words: Vec::new(),
        }])
    }
}

/// QA D2: one box over two labels with a wide gap between them becomes two boxes, each cut to
/// its own ink; a gap as narrow as a word space does not split.
#[test]
fn a_wide_gap_splits_a_line_and_a_word_space_does_not() {
    let mut s = Scene::new("gap", 420, 70, [236, 236, 236]);
    s.text(20, 20, "Save", 16, "#161616");
    s.text(220, 20, "Cancel", 16, "#161616");
    let merged = TextLine {
        text: "SaveCancel".to_owned(),
        rect: Rc::new(18.0, 18.0, 290.0, 22.0),
        confidence: 1.0,
        words: Vec::new(),
    };
    let out = refine_lines(&WholeCrop, &s.img, vec![merged]);
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(out[0].rect.x + out[0].rect.w < 100.0, "{:?}", out[0].rect);
    assert!(out[1].rect.x > 200.0, "{:?}", out[1].rect);
    let mut s = Scene::new("space", 300, 60, [236, 236, 236]);
    s.text(20, 20, "Save all", 16, "#161616");
    let one = TextLine {
        text: "Save all".to_owned(),
        rect: Rc::new(18.0, 18.0, 80.0, 22.0),
        confidence: 1.0,
        words: Vec::new(),
    };
    assert_eq!(refine_lines(&WholeCrop, &s.img, vec![one]).len(), 1);
}

/// A box much wider than its word shrinks to the ink with a pixel of margin and never leaves the
/// glyphs outside (a tight box grows over the edge).
#[test]
fn a_loose_box_is_cut_to_the_ink_and_a_tight_one_is_covered() {
    let mut s = Scene::new("fit", 300, 80, [236, 236, 236]);
    s.text(100, 30, "Hello", 18, "#161616");
    let t = s.truth[0];
    let loose = TextLine::laid_out("Hello", 40.0, 20.0, 40.0, 40.0);
    let out = refine_lines(&Scripted(vec![]), &s.img, vec![loose]);
    let r = out[0].rect;
    assert!(
        r.x <= t.x && r.x + r.w >= t.x + t.w && r.w <= t.w + 4.0,
        "{r:?} vs {t:?}"
    );
    let tight = TextLine {
        text: "Hello".to_owned(),
        rect: Rc::new(t.x + 2.0, t.y + 2.0, t.w - 4.0, t.h - 4.0),
        confidence: 1.0,
        words: Vec::new(),
    };
    let out = refine_lines(&Scripted(vec![]), &s.img, vec![tight]);
    let r = out[0].rect;
    assert!(r.x <= t.x + 1.0 && r.y <= t.y + 1.0, "{r:?} vs {t:?}");
}

#[test]
fn a_filled_disc_read_as_a_letter_is_dropped() {
    let mut s = Scene::new("disc", 200, 120, [245, 245, 247]);
    s.shape(|i| disc(i, 100, 60, 30, [0, 122, 255]));
    let mut line = TextLine::laid_out("O", 70.0, 30.0, 60.0, 60.0);
    line.confidence = 0.5;
    let out = refine_lines(&Scripted(vec![]), &s.img, vec![line]);
    assert!(out.is_empty());
}
