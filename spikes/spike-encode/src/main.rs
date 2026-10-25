//! S6 `spike-encode`: 4K60-class native screen capture + 1080p camera, hardware encoded
//! in-process to fragmented MP4, on one clock, with a camera bubble excluded from capture.
//!
//! Subcommands (macOS only; the Windows side is intentionally not written, see docs/SPIKES.md):
//!
//! ```text
//! spike-encode preflight                      # permission + display + device report
//! spike-encode run [options]                  # the measured recording
//! spike-encode verify-bubble                  # bubble-absent-from-capture test with controls
//! spike-encode inspect <file.mp4>             # box layout (moof count) + ffprobe + decode check
//!
//! run options:
//!   --codec hevc|h264        screen codec (default hevc)
//!   --secs N                 duration (default 20)
//!   --fps N                  screen fps (default 60)
//!   --pixfmt 420v|bgra       SCK pixel format (default 420v, NV12 video range)
//!   --width W --height H     output pixels (default: display native, points x pointPixelScale)
//!   --mbps N                 screen bitrate (default from the spec table: HEVC 60, H.264 90)
//!   --camera on|off          record the camera as a second track (default on if authorised)
//!   --camera-res 1080|720    (default 1080)
//!   --bubble marker|camera|off  window shown as the bubble (default marker: solid magenta)
//!   --exclude none|sharing|filter|both   how the bubble is excluded (default both)
//!   --motion on|off          animate a window so the screen changes every frame (default on)
//!   --request-camera         ask macOS for camera access if not yet decided
//!   --sck-rate N             SCK minimumFrameInterval = 1/N (default 1.1 x fps; exactly 60 skips vsyncs)
//!   --container cmaf|mp4|mov cmaf = segment delegate fMP4 (default, works); mp4/mov = plain writer
//!                            with movieFragmentInterval (measured: NOT fragmented)
//!   --out DIR                default spike-out/encode (relative to the current directory)
//! ```

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
fn main() {
    mac::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "spike-encode: only the macOS implementation exists. The Windows version (WGC + Media \
         Foundation hardware MFT + WDA_EXCLUDEFROMCAPTURE) is not written; see docs/SPIKES.md S6."
    );
    std::process::exit(2);
}
