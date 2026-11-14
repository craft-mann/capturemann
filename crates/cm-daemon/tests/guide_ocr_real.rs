//! GDE-3.AC2 against the real OS text recognizer, for every export format: a step image with a
//! blur shows the blur, and OCR finds no text inside the blurred rectangle, in the Markdown
//! images, in the images embedded in the HTML file and in the images inside the PDF.
//!
//! The pictures are synthetic (the fake screen plus text drawn by the project's own renderer; no
//! screen, camera or microphone). Ignored: prints SKIPPED unless `CM_OCR_REAL=1` and a recognizer
//! is installed.
//!
//! `CM_OCR_REAL=1 cargo test -p cm-daemon --test guide_ocr_real -- --ignored --nocapture`

#![allow(clippy::unwrap_used)]

mod common;

use std::io::Read;

use base64::Engine as _;
use std::sync::Arc;

use cm_daemon::guide::ScriptedClicks;
use cm_daemon::testing::FixedConfirmer;
use cm_daemon::Decision;
use cm_render::ocr::system_recognizer;
use common::{builder, desktop, Client};
use image::RgbaImage;
use serde_json::json;

fn ready() -> bool {
    if std::env::var("CM_OCR_REAL").as_deref() != Ok("1") {
        println!("SKIPPED: set CM_OCR_REAL=1 to run the real text recognizer");
        return false;
    }
    if !system_recognizer().is_available() {
        println!("SKIPPED: no text recognizer on this machine");
        return false;
    }
    true
}

fn read(img: &RgbaImage) -> String {
    system_recognizer()
        .recognize(img)
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The PNGs of the `data:image/png;base64,` URIs of an HTML file, in order.
fn html_images(html: &str) -> Vec<RgbaImage> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("data:image/png;base64,") {
        rest = &rest[i + "data:image/png;base64,".len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='))
            .unwrap_or(rest.len());
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&rest[..end])
            .unwrap();
        out.push(image::load_from_memory(&bytes).unwrap().to_rgba8());
        rest = &rest[end..];
    }
    out
}

fn number_after(dict: &str, key: &str) -> Option<u32> {
    let i = dict.find(key)? + key.len();
    let digits: String = dict[i..]
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// The 8-bit RGB images of a PDF written by the exporter: image XObjects whose stream is
/// Flate-compressed raw samples (krilla decodes the PNG and embeds the pixels). Alpha masks and
/// anything smaller than 100 px are skipped.
fn pdf_images(pdf: &[u8]) -> Vec<RgbaImage> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(s) = find(pdf, b"stream", at) {
        let after = s + b"stream".len();
        // `endstream` also contains "stream": skip it.
        if s >= 3 && &pdf[s - 3..s] == b"end" {
            at = after;
            continue;
        }
        let body = if pdf[after..].starts_with(b"\r\n") {
            after + 2
        } else {
            after + 1
        };
        let Some(e) = find(pdf, b"endstream", body) else {
            break;
        };
        // The dictionary is the text just before `stream`, back to its `<<`.
        let head = String::from_utf8_lossy(&pdf[s.saturating_sub(600)..s]).into_owned();
        let dict = head.rfind("<<").map_or(head.as_str(), |i| &head[i..]);
        if dict.contains("/Image") && dict.contains("/DeviceRGB") {
            if let (Some(w), Some(h)) =
                (number_after(dict, "/Width"), number_after(dict, "/Height"))
            {
                let mut raw = Vec::new();
                let data = &pdf[body..e];
                if flate2::read::ZlibDecoder::new(data)
                    .read_to_end(&mut raw)
                    .is_ok()
                    && raw.len() == (w * h * 3) as usize
                    && w >= 100
                    && h >= 100
                {
                    let mut img = RgbaImage::new(w, h);
                    for (i, p) in img.pixels_mut().enumerate() {
                        p.0 = [raw[i * 3], raw[i * 3 + 1], raw[i * 3 + 2], 255];
                    }
                    out.push(img);
                }
            }
        }
        at = e + b"endstream".len();
    }
    out
}

#[tokio::test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
async fn gde_3_ac2_ocr_finds_no_text_inside_a_blur_in_markdown_html_and_pdf() {
    if !ready() {
        return;
    }
    let (b, dir) = builder(desktop());
    let clicks = ScriptedClicks::new();
    let daemon = b
        .guide_clicks(clicks.clone())
        .confirmer(Arc::new(FixedConfirmer::new(Decision::Approve)))
        .build();
    let mut c = Client::connect(&daemon, "agent");
    let r = c
        .call(
            "start_guide",
            json!({"target": {"display_id": 1}, "countdown_s": 0, "title": "OCR"}),
        )
        .await;
    assert_eq!(r["result"]["state"], "recording", "{r}");
    let id = r["result"]["guide_id"].as_str().unwrap().to_owned();
    // Two different spots: two clicks on one spot would fold into a double click.
    assert!(clicks.click(30.0, 40.0));
    assert!(clicks.click(300.0, 250.0));
    let done = c.call("stop_guide", json!({"guide_id": id})).await;
    let steps = done["result"]["steps"].as_array().unwrap().clone();
    let (plain, blurred) = match steps.len() {
        2 => (
            steps[0]["image_id"].as_str().unwrap().to_owned(),
            steps[1]["image_id"].as_str().unwrap().to_owned(),
        ),
        n => panic!("two clicks made {n} steps"),
    };
    let word = json!({"op": "text", "at": [20, 120], "text": "SECRETWORD 4111", "size": 28,
                      "color": "#161616", "outline": null});
    // Step 1: the text alone (the control: the recognizer can read the exported picture).
    // Step 2: the same text under a blur.
    for (cap, ops) in [
        (&plain, json!([word])),
        (
            &blurred,
            json!([word, {"op": "blur", "rect": [10, 110, 300, 50], "strength": 24}]),
        ),
    ] {
        let r = c
            .call(
                "annotate",
                json!({"capture_id": cap, "ops": ops, "return_image": false}),
            )
            .await;
        assert!(r.get("error").is_none(), "{r}");
    }

    let md = dir.path().join("md");
    let html = dir.path().join("guide.html");
    let pdf = dir.path().join("guide.pdf");
    for (format, dest) in [("markdown", &md), ("html", &html), ("pdf", &pdf)] {
        let r = c
            .call(
                "export_guide",
                json!({"guide_id": id, "format": format, "dest": dest}),
            )
            .await;
        assert!(r.get("error").is_none(), "{format}: {r}");
    }

    let sets: Vec<(&str, Vec<RgbaImage>)> = vec![
        (
            "markdown",
            (1..=2)
                .map(|n| {
                    image::open(md.join(format!("images/step-{n:02}.png")))
                        .unwrap()
                        .to_rgba8()
                })
                .collect(),
        ),
        (
            "html",
            html_images(&std::fs::read_to_string(&html).unwrap()),
        ),
        ("pdf", pdf_images(&std::fs::read(&pdf).unwrap())),
    ];
    for (format, imgs) in sets {
        assert_eq!(imgs.len(), 2, "{format}: found {} step images", imgs.len());
        let texts: Vec<String> = imgs.iter().map(read).collect();
        println!("{format}: {texts:?}");
        let readable = texts.iter().filter(|t| t.contains("SECRET")).count();
        // The control: without the blur the word is readable in this format's picture, so the
        // test can fail; with the blur, no picture of the format has the word twice.
        assert_eq!(
            readable, 1,
            "{format}: exactly the unblurred step must be readable, OCR read {texts:?}"
        );
        assert!(
            texts.iter().filter(|t| t.contains("4111")).count() <= 1,
            "{format}: {texts:?}"
        );
    }
}
