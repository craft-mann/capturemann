//! Per-OS screen capture backends feeding a [`FrameSink`] with BGRA frames.

#[cfg(target_os = "windows")]
pub use win::{start, stop};

#[cfg(target_os = "macos")]
pub use mac::{start, stop};

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub use unsupported::{start, stop};

// ---------------------------------------------------------------------------
// Windows: Windows.Graphics.Capture via `windows-capture`
#[cfg(target_os = "windows")]
mod win {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use anyhow::{anyhow, Result};
    use windows_capture::{
        capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
        frame::Frame,
        graphics_capture_api::InternalCaptureControl,
        monitor::Monitor,
        settings::{
            ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
            MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
        },
    };

    use crate::{sink::FrameSink, Opts};

    type BoxErr = Box<dyn std::error::Error + Send + Sync>;

    pub struct Handle(CaptureControl<Handler, BoxErr>);

    pub struct Handler {
        sink: Arc<FrameSink>,
        stop: Arc<AtomicBool>,
        scratch: Vec<u8>,
    }

    impl GraphicsCaptureApiHandler for Handler {
        type Flags = (Arc<FrameSink>, Arc<AtomicBool>);
        type Error = BoxErr;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            let (sink, stop) = ctx.flags;
            Ok(Self {
                sink,
                stop,
                scratch: Vec::new(),
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            let (w, h) = (frame.width(), frame.height());
            let fb = frame.buffer()?;
            let data = fb.as_nopadding_buffer(&mut self.scratch);
            self.sink.push(data, w, h, w as usize * 4);
            if self.stop.load(Ordering::Relaxed) {
                control.stop();
            }
            Ok(())
        }

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            self.stop.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    pub fn start(opts: &Opts, sink: Arc<FrameSink>, stop: Arc<AtomicBool>) -> Result<Handle> {
        // windows-capture uses one-based monitor indices.
        let monitor = Monitor::from_index(opts.display + 1)
            .map_err(|e| anyhow!("monitor {}: {e}", opts.display))?;
        println!(
            "capturing {} ({}x{}) with Windows.Graphics.Capture",
            monitor.name().unwrap_or_default(),
            monitor.width().unwrap_or(0),
            monitor.height().unwrap_or(0)
        );
        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::Default, // Windows' own yellow border doubles as the recording indicator
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            (sink, stop),
        );
        let control =
            Handler::start_free_threaded(settings).map_err(|e| anyhow!("WGC start: {e}"))?;
        Ok(Handle(control))
    }

    pub fn stop(h: Handle) {
        if let Err(e) = h.0.stop() {
            eprintln!("WGC stop: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// macOS: ScreenCaptureKit via `screencapturekit`
#[cfg(target_os = "macos")]
mod mac {
    use std::sync::{atomic::AtomicBool, Arc};

    use anyhow::{anyhow, Context as _, Result};
    use screencapturekit::{cv::CVPixelBufferLockFlags, prelude::*};

    use crate::{sink::FrameSink, Opts};

    pub struct Handle(SCStream);

    struct Output {
        sink: Arc<FrameSink>,
    }

    impl SCStreamOutputTrait for Output {
        fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
            if !matches!(of_type, SCStreamOutputType::Screen) {
                return;
            }
            // Idle/blank status frames carry no pixel buffer; skip them.
            let Some(pb) = sample.pixel_buffer() else {
                return;
            };
            let Ok(guard) = pb.lock(CVPixelBufferLockFlags::READ_ONLY) else {
                return;
            };
            let (w, h, stride) = (
                guard.width() as u32,
                guard.height() as u32,
                guard.bytes_per_row(),
            );
            // SAFETY: the buffer stays locked for as long as `guard` lives.
            let Some(data) = (unsafe { guard.as_slice() }) else {
                return;
            };
            self.sink.push(data, w, h, stride);
        }
    }

    pub fn start(opts: &Opts, sink: Arc<FrameSink>, _stop: Arc<AtomicBool>) -> Result<Handle> {
        let content = SCShareableContent::get().map_err(|e| {
            anyhow!("SCShareableContent: {e:?} (grant Screen Recording to your terminal)")
        })?;
        let displays = content.displays();
        let display = displays
            .get(opts.display)
            .context("no such display index")?;
        let (dw, dh) = (display.width(), display.height()); // points
                                                            // Ask ScreenCaptureKit to scale on the GPU so we pipe 1080p, not 5K, to FFmpeg.
        let out_h = opts.height;
        let out_w = ((out_h as f64 * dw as f64 / dh as f64).round() as u32) & !1;
        println!(
            "capturing display {} ({dw}x{dh} pt) at {out_w}x{out_h} with ScreenCaptureKit",
            display.display_id()
        );

        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| anyhow!("content filter: {e:?}"))?;
        let config = SCStreamConfiguration::new()
            .with_width(out_w)
            .with_height(out_h)
            .with_pixel_format(PixelFormat::BGRA)
            .with_minimum_frame_interval(&CMTime::new(1, opts.fps as i32))
            .with_shows_cursor(true)
            .with_queue_depth(6);
        let mut stream = SCStream::new(&filter, &config).map_err(|e| anyhow!("SCStream: {e:?}"))?;
        stream
            .add_output_handler(Output { sink }, SCStreamOutputType::Screen)
            .map_err(|e| anyhow!("add output: {e:?}"))?;
        stream
            .start_capture()
            .map_err(|e| anyhow!("start capture: {e:?}"))?;
        Ok(Handle(stream))
    }

    pub fn stop(h: Handle) {
        if let Err(e) = h.0.stop_capture() {
            eprintln!("SCK stop: {e:?}");
        }
    }
}

// ---------------------------------------------------------------------------
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod unsupported {
    use std::sync::{atomic::AtomicBool, Arc};

    use crate::{sink::FrameSink, Opts};

    pub struct Handle;

    pub fn start(_: &Opts, _: Arc<FrameSink>, _: Arc<AtomicBool>) -> anyhow::Result<Handle> {
        anyhow::bail!("spike-record supports Windows and macOS only (Linux/PipeWire is v0.2)")
    }

    pub fn stop(_: Handle) {}
}
