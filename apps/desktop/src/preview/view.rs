//! `PreviewSurface`: a GPUI view that shows a stream of 1080p frames and counts what reached the
//! screen. Frames come from a worker thread paced at the source rate, as a decoder would
//! deliver them; the view takes the newest one on each redraw.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gpui::{div, img, prelude::*, rgb, Context, ObjectFit, RenderImage, Window};
use image::{Frame, RgbaImage};

use super::frame::{nv12_to_bgra, SyntheticSource};
use super::stats::{PresentStats, PresentSummary};

/// How pixels reach the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewPath {
    /// IOSurface-backed NV12 `CVPixelBuffer` through GPUI's `surface()` (macOS only).
    Surface,
    /// NV12 to BGRA on the CPU, then GPUI's `img()` (atlas upload every frame).
    Cpu,
}

impl PreviewPath {
    pub fn label(self) -> &'static str {
        match self {
            PreviewPath::Surface => "surface (CVPixelBuffer NV12)",
            PreviewPath::Cpu => "cpu (BGRA img upload)",
        }
    }
}

enum Payload {
    Cpu(Arc<RenderImage>),
    #[cfg(target_os = "macos")]
    Surface(super::macos::SendBuffer),
}

struct Produced {
    index: u32,
    payload: Payload,
}

/// What the worker measured about itself.
#[derive(Default)]
pub struct WorkerStats {
    pub produced: AtomicU64,
    /// Frames finished more than half a period after their deadline.
    pub late_produced: AtomicU64,
    pub produce_ns_total: AtomicU64,
    pub produce_ns_max: AtomicU64,
}

type Slot = Arc<Mutex<Option<Produced>>>;

pub struct PreviewSurface {
    path: PreviewPath,
    slot: Slot,
    shown: Option<Payload>,
    stats: PresentStats,
    worker_stats: Arc<WorkerStats>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PreviewSurface {
    /// Starts the worker at `fps`. `size` is the source size (1920x1080 for the test).
    pub fn new(path: PreviewPath, size: (usize, usize), fps: u32) -> PreviewSurface {
        let slot: Slot = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stats = Arc::new(WorkerStats::default());
        let worker = {
            let (slot, stop, ws) = (slot.clone(), stop.clone(), worker_stats.clone());
            std::thread::Builder::new()
                .name("preview-source".into())
                .spawn(move || run_worker(path, size, fps, &slot, &stop, &ws))
                .ok()
        };
        PreviewSurface {
            path,
            slot,
            shown: None,
            stats: PresentStats::new(fps),
            worker_stats,
            stop,
            worker,
        }
    }

    pub fn path(&self) -> PreviewPath {
        self.path
    }

    /// Presentation numbers so far.
    pub fn summary(&self) -> PresentSummary {
        self.stats.summary(Instant::now())
    }

    pub fn worker_stats(&self) -> &WorkerStats {
        &self.worker_stats
    }

    /// Stops the worker and waits for it.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl Drop for PreviewSurface {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_worker(
    path: PreviewPath,
    size: (usize, usize),
    fps: u32,
    slot: &Slot,
    stop: &AtomicBool,
    ws: &WorkerStats,
) {
    let source = SyntheticSource::new(size.0, size.1);
    let period = Duration::from_secs_f64(1.0 / f64::from(fps));
    let t0 = Instant::now();
    let mut index = 0u32;
    while !stop.load(Ordering::Relaxed) {
        let deadline = t0 + period.mul_f64(f64::from(index));
        sleep_until(deadline);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let started = Instant::now();
        let nv12 = source.frame(index);
        let payload = match path {
            PreviewPath::Cpu => {
                let bgra = nv12_to_bgra(&nv12);
                let Some(buf) = RgbaImage::from_raw(nv12.width as u32, nv12.height as u32, bgra)
                else {
                    break;
                };
                Payload::Cpu(Arc::new(RenderImage::new([Frame::new(buf)])))
            }
            #[cfg(target_os = "macos")]
            PreviewPath::Surface => match super::macos::pixel_buffer(&nv12) {
                Ok(b) => Payload::Surface(b),
                Err(e) => {
                    eprintln!("preview: {e}");
                    break;
                }
            },
            #[cfg(not(target_os = "macos"))]
            PreviewPath::Surface => break,
        };
        let done = Instant::now();
        let took = done.duration_since(started).as_nanos() as u64;
        ws.produce_ns_total.fetch_add(took, Ordering::Relaxed);
        ws.produce_ns_max.fetch_max(took, Ordering::Relaxed);
        if done > deadline + period / 2 {
            ws.late_produced.fetch_add(1, Ordering::Relaxed);
        }
        ws.produced.fetch_add(1, Ordering::Relaxed);
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(Produced { index, payload });
        index = index.wrapping_add(1);
    }
}

/// Sleeps to within 1 ms of `deadline`, then spins the rest.
fn sleep_until(deadline: Instant) {
    let now = Instant::now();
    if let Some(left) = deadline.checked_duration_since(now) {
        if left > Duration::from_millis(2) {
            std::thread::sleep(left - Duration::from_millis(1));
        }
    }
    while Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

impl Render for PreviewSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let fresh = self
            .slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match fresh {
            Some(p) => {
                self.stats.new_frame(p.index, now);
                if let Some(Payload::Cpu(old)) = self.shown.replace(p.payload) {
                    // The window being drawn is out of `App.windows` during render: pass it, or the
                    // atlas keeps every frame (8 MB each at 1080p).
                    cx.drop_image(old, Some(window));
                }
            }
            None => self.stats.repeat(),
        }
        // A video view redraws every display frame; the stats tell new frames from repeats.
        window.request_animation_frame();
        let content = match &self.shown {
            Some(Payload::Cpu(image)) => Some(
                img(image.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            #[cfg(target_os = "macos")]
            Some(Payload::Surface(buf)) => Some(
                gpui::surface(buf.0.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            None => None,
        };
        div().size_full().bg(rgb(0x000000)).flex().children(content)
    }
}
