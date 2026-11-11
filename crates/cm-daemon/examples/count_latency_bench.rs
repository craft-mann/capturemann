//! Times a screenshot through the daemon (fake capturer, no screen) and `list_library` over many
//! bundles: `cargo run --release -p cm-daemon --example count_latency_bench -- [--logical
//! 2560x1440] [--scale 2] [--n 250]`. Prints when `capture.created` is queued relative to the
//! call's start and when the call returns.
#![allow(clippy::unwrap_used)]

use std::time::Instant;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_core::Actor;
use cm_library::Library;
use serde_json::json;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

#[tokio::main]
async fn main() {
    let dims = arg("--logical").unwrap_or_else(|| "2560x1440".into());
    let (w, h) = dims.split_once('x').unwrap();
    let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
    let scale: f64 = arg("--scale").and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let n: usize = arg("--n").and_then(|s| s.parse().ok()).unwrap_or(250);
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(dir.path()).unwrap();
    let backend = FakeBackend::new()
        .with_display(1, 0, 0, w, h, scale)
        .with_display(2, w as i32, 0, 200, 100, 1.0);
    let daemon = cm_daemon::Daemon::builder(Capturer::new(backend).with_own_pid(None), lib).build();
    println!(
        "frame {}x{} physical",
        (w as f64 * scale) as u32,
        (h as f64 * scale) as u32
    );
    println!("| Run | event queued at (ms) | call returns (ms) |");
    println!("|---|---|---|");
    for i in 0..4 {
        let mut rx = daemon.subscribe();
        let t = Instant::now();
        let watcher = tokio::spawn(async move {
            loop {
                let e = rx.recv().await.unwrap();
                if e.name == "capture.created" {
                    return Instant::now();
                }
            }
        });
        daemon
            .execute_method(
                Actor::User,
                0,
                "capture_screenshot",
                &json!({"target": {"display_id": 1}}),
            )
            .await
            .unwrap();
        let call = t.elapsed();
        let ev = watcher.await.unwrap().duration_since(t);
        println!(
            "| {i} | {:.0} | {:.0} |",
            ev.as_secs_f64() * 1e3,
            call.as_secs_f64() * 1e3
        );
    }
    // The stages of `store_capture` on a frame with real-screen-like content (flat areas, text-like
    // noise blocks and a photo-like noisy region), which the fake's smooth gradient is not.
    {
        use base64::Engine as _;
        let (pw, ph) = ((w as f64 * scale) as u32, (h as f64 * scale) as u32);
        let mut px = vec![255u8; pw as usize * ph as usize * 4];
        let with_photo = std::env::var_os("BENCH_NO_PHOTO").is_none();
        let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
        for y in 0..ph {
            for x in 0..pw {
                let i = (y as usize * pw as usize + x as usize) * 4;
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                let photo = with_photo && x > pw / 2 && y < ph / 2;
                let text = (x / 40 + y / 40) % 3 == 0 && (s & 7) < 2;
                let v = if photo {
                    (s & 0xff) as u8 / 2 + 64
                } else if text {
                    30
                } else {
                    240
                };
                px[i] = v;
                px[i + 1] = v;
                px[i + 2] = v.wrapping_add((x / 16) as u8 & 7);
            }
        }
        let frame = cm_capture::Frame::new(pw, ph, px).unwrap();
        let t = Instant::now();
        let png = frame.to_png().unwrap();
        let t_png = t.elapsed();
        let lib2dir = tempfile::tempdir().unwrap();
        let mut lib2 = Library::open_or_create(lib2dir.path()).unwrap();
        let now = time::OffsetDateTime::now_utc();
        let t = Instant::now();
        let bundle = lib2
            .create_screenshot_without_poster(&png, None, now)
            .unwrap();
        let t_bundle = t.elapsed();
        let t = Instant::now();
        cm_library::thumbs::write_poster(&bundle, cm_library::thumbs::DEFAULT_POSTER_EDGE).unwrap();
        let t_poster = t.elapsed();
        let t = Instant::now();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        let t_b64 = t.elapsed();
        let t = Instant::now();
        let v = serde_json::to_string(&json!({"image": {"data": b64}})).unwrap();
        let t_json = t.elapsed();
        println!(
            "noisy {pw}x{ph}: PNG {} MB; to_png {:.0} ms; bundle+index {:.0} ms; poster {:.0} ms; base64 {:.0} ms; json {:.0} ms ({} MB)",
            png.len() / 1_000_000,
            t_png.as_secs_f64() * 1e3,
            t_bundle.as_secs_f64() * 1e3,
            t_poster.as_secs_f64() * 1e3,
            t_b64.as_secs_f64() * 1e3,
            t_json.as_secs_f64() * 1e3,
            v.len() / 1_000_000
        );
    }
    for i in 0..n {
        daemon
            .execute_method(
                Actor::User,
                0,
                "capture_screenshot",
                &json!({"target": {"display_id": 2}, "title": format!("s{i}")}),
            )
            .await
            .unwrap();
    }
    for r in 0..3 {
        let t = Instant::now();
        let v = daemon
            .execute_method(Actor::User, 0, "list_library", &json!({"limit": 100}))
            .await
            .unwrap();
        println!(
            "list_library page (limit 100) over {} bundles, run {r}: {:.1} ms ({} rows)",
            n + 4,
            t.elapsed().as_secs_f64() * 1e3,
            v["captures"].as_array().map_or(0, Vec::len)
        );
    }
}
