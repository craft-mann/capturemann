//! 1080p60 preview test (docs/project/spikes.md S5 follow-up): shows synthetic 1920x1080 frames at 60 fps in
//! a GPUI window for N seconds and prints presented fps, late frames, CPU and memory.
//!
//! `cargo run --release -p cm-desktop --example preview_bench -- [surface|cpu] [seconds]`
//!
//! Needs a display; opens one window; asks for no permissions and records nothing.

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};

    use cm_desktop::preview::{PreviewPath, PreviewSurface};
    use gpui::{prelude::*, px, size, App, Bounds, WindowBounds, WindowOptions};

    let mut args = std::env::args().skip(1);
    let path = match args.next().as_deref() {
        Some("cpu") => PreviewPath::Cpu,
        _ => PreviewPath::Surface,
    };
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let warmup = Duration::from_secs(2);

    fn cpu_seconds() -> f64 {
        // SAFETY: plain libc call with a zeroed out struct.
        unsafe {
            let mut ru: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut ru);
            let t = |tv: libc::timeval| tv.tv_sec as f64 + tv.tv_usec as f64 / 1e6;
            t(ru.ru_utime) + t(ru.ru_stime)
        }
    }
    fn rss_mb() -> f64 {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok();
        out.and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<f64>()
                .ok()
        })
        .map_or(0.0, |kb| kb / 1024.0)
    }

    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
        let win = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |_, cx| cx.new(|_| PreviewSurface::new(path, (1920, 1080), 60)),
            )
            .expect("open window");
        let Ok(view) = win.entity(cx) else {
            cx.quit();
            return;
        };
        cx.activate(true);
        let t0 = Instant::now();
        cx.spawn(async move |cx| {
            let ord = std::sync::atomic::Ordering::Relaxed;
            // Measure after a warm-up so the first-frame cost (pipeline compile) is excluded.
            cx.background_executor().timer(warmup).await;
            let (cpu0, wall0) = (cpu_seconds(), Instant::now());
            let base = cx.update(|cx| view.read(cx).summary());
            cx.background_executor()
                .timer(Duration::from_secs(secs))
                .await;
            let wall = wall0.elapsed().as_secs_f64();
            let cpu = (cpu_seconds() - cpu0) / wall * 100.0;
            let rss = rss_mb();
            cx.update(|cx| {
                let v = view.read(cx);
                let s = v.summary();
                let ws = v.worker_stats();
                let produced = ws.produced.load(ord);
                let avg_ms = ws.produce_ns_total.load(ord) as f64 / produced.max(1) as f64 / 1e6;
                println!("path:               {}", path.label());
                println!("window:             {secs} s after {} s warm-up", warmup.as_secs());
                println!(
                    "presented new fps:  {:.1}  ({} frames)",
                    (s.presented - base.presented) as f64 / wall,
                    s.presented - base.presented
                );
                println!("late (gap > 33 ms):  {}", s.late - base.late);
                println!("jitter (gap > 25 ms): {}", s.jittered - base.jittered);
                println!("skipped (unshown):  {}", s.skipped - base.skipped);
                println!("worst gap:          {:.1} ms (whole run)", s.worst_gap_ms);
                println!("redraw repeats:     {} (whole run)", s.repeats);
                println!(
                    "source:             {produced} produced, {} late, avg {avg_ms:.2} ms, max {:.2} ms per frame (worker thread)",
                    ws.late_produced.load(ord),
                    ws.produce_ns_max.load(ord) as f64 / 1e6
                );
                println!("process CPU:        {cpu:.0}% of one core (all threads, incl. the source)");
                println!("RSS:                {rss:.0} MB");
                println!("total run:          {:.1} s", t0.elapsed().as_secs_f64());
                cx.quit();
            });
        })
        .detach();
    });
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("preview_bench is macOS only for now (the surface path needs CVPixelBuffer)");
}
