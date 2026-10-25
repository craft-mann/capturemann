//! Single-frame display screenshots that leave this process's own windows out (macOS).
//!
//! `xcap`'s `Monitor::capture_image` has no process filter, so CaptureMann's own windows (the
//! sticky screenshot toast with its Annotate button, a confirmation dialog, the library) could
//! end up in a display or region screenshot. `SCScreenshotManager.captureImage(contentFilter:)`
//! (macOS 14+) takes the same `SCContentFilter` the recording path uses
//! (`record/sck.rs`), so the frame is taken with the own application excluded. On macOS 13 the
//! selector is missing and [`capture_display_excluding`] returns `Ok(None)`: the caller falls
//! back to the xcap frame (documented limitation).
//!
//! Window screenshots are not affected (a window capture only contains that window).

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use cm_core::CmError;
use objc2::rc::Retained;
use objc2::{sel, AnyThread, ClassType};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGDataProvider, CGImage};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCRunningApplication, SCScreenshotManager, SCShareableContent,
    SCStreamConfiguration, SCWindow,
};

use crate::Frame;

const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// `kCVPixelFormatType_32BGRA`.
const FOURCC_BGRA: u32 = 0x4247_5241;

/// Which processes a screenshot leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Exclusions {
    pub pids: Vec<u32>,
    /// Windows of an excluded process that stay in the frame (the library window).
    pub keep_windows: Vec<u32>,
}

impl Exclusions {
    pub fn pid(pid: u32) -> Self {
        Exclusions {
            pids: vec![pid],
            keep_windows: Vec::new(),
        }
    }

    /// Adds the windows that stay in the frame although their process is excluded.
    pub fn keeping(mut self, ids: Vec<u32>) -> Self {
        self.keep_windows = ids;
        self
    }

    /// The ids of `windows` (`(id, owner pid)` as SCK lists them) that are an exception: kept
    /// ids whose owner is excluded. An id that is not listed stays excluded (fail-safe).
    pub fn select_kept(&self, windows: &[(u32, Option<u32>)]) -> Vec<u32> {
        windows
            .iter()
            .filter(|(id, owner)| {
                self.keep_windows.contains(id) && owner.is_some_and(|o| self.pids.contains(&o))
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// The pids among `running` (the shareable applications) that must be excluded. Empty
    /// means the process has no capturable app entry (no windows), so nothing to exclude.
    pub fn select(&self, running: &[u32]) -> Vec<u32> {
        running
            .iter()
            .copied()
            .filter(|p| self.pids.contains(p))
            .collect()
    }
}

/// Moves an Objective-C handle through a channel.
struct SendBox<T>(T);
// SAFETY: the wrapped ScreenCaptureKit objects are immutable results handed to exactly one
// receiving thread; ScreenCaptureKit objects are thread-safe.
unsafe impl<T> Send for SendBox<T> {}

fn shareable_content() -> Result<Retained<SCShareableContent>, CmError> {
    let (tx, rx) = mpsc::channel::<Result<SendBox<Retained<SCShareableContent>>, String>>();
    let block = RcBlock::new(move |c: *mut SCShareableContent, e: *mut NSError| {
        // SAFETY: SCK passes either a valid content object or a valid error.
        let r = match unsafe { Retained::retain(c) } {
            Some(c) => Ok(SendBox(c)),
            None => Err(unsafe { e.as_ref() }.map_or_else(
                || "unknown error".to_owned(),
                |e| e.localizedDescription().to_string(),
            )),
        };
        let _ = tx.send(r);
    });
    // SAFETY: the completion handler block is copied by SCK and outlives the call.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    rx.recv_timeout(STEP_TIMEOUT)
        .map_err(|_| CmError::internal("ScreenCaptureKit did not answer in time"))?
        .map(|b| b.0)
        .map_err(|m| CmError::internal(format!("ScreenCaptureKit: {m}")))
}

fn available() -> bool {
    SCScreenshotManager::class().responds_to(sel!(
        captureImageWithFilter:configuration:completionHandler:
    ))
}

/// A full-display screenshot at `size` physical px without the windows of `exclude`.
///
/// `Ok(None)`: not supported here (macOS 13), use the xcap frame. Errors are real failures
/// (denied permission, timeout); the capturer also falls back on them rather than failing a
/// screenshot the xcap path could take.
pub fn capture_display_excluding(
    display_id: u32,
    size: (u32, u32),
    exclude: &Exclusions,
) -> Result<Option<Frame>, CmError> {
    if !available() {
        return Ok(None);
    }
    let content = shareable_content()?;
    // SAFETY: plain getters on live ScreenCaptureKit objects, and initializers called with the
    // documented argument types.
    let filter = unsafe {
        let display = content
            .displays()
            .iter()
            .find(|d| d.displayID() == display_id)
            .ok_or_else(|| CmError::not_found(format!("Display {display_id} not found")))?;
        let apps: Vec<Retained<SCRunningApplication>> = content
            .applications()
            .iter()
            .filter(|a| exclude.pids.contains(&(a.processID() as u32)))
            .collect();
        if apps.is_empty() {
            SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &display,
                &NSArray::new(),
            )
        } else {
            // `exceptingWindows` are shown although their app is excluded (SCStream.h).
            let listed: Vec<(u32, Option<u32>)> = content
                .windows()
                .iter()
                .map(|w| {
                    (
                        w.windowID(),
                        w.owningApplication()
                            .and_then(|o| u32::try_from(o.processID()).ok()),
                    )
                })
                .collect();
            let keep = exclude.select_kept(&listed);
            let kept: Vec<Retained<SCWindow>> = content
                .windows()
                .iter()
                .filter(|w| keep.contains(&w.windowID()))
                .collect();
            SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                SCContentFilter::alloc(),
                &display,
                &NSArray::from_retained_slice(&apps),
                &NSArray::from_retained_slice(&kept),
            )
        }
    };
    // SAFETY: setters on a configuration object owned here.
    let cfg = unsafe {
        let cfg = SCStreamConfiguration::new();
        cfg.setWidth(size.0 as usize);
        cfg.setHeight(size.1 as usize);
        cfg.setPixelFormat(FOURCC_BGRA);
        cfg.setShowsCursor(false);
        cfg
    };
    let (tx, rx) = mpsc::channel::<Result<SendBox<CFRetained<CGImage>>, String>>();
    let block = RcBlock::new(move |img: *mut CGImage, e: *mut NSError| {
        // SAFETY: SCK passes either a valid (+0) image or a valid error; the image is retained
        // for our own reference before the handler returns.
        let r = match unsafe { img.as_ref() } {
            Some(i) => Ok(SendBox(CFRetained::from(i))),
            None => Err(unsafe { e.as_ref() }.map_or_else(
                || "unknown error".to_owned(),
                |e| e.localizedDescription().to_string(),
            )),
        };
        let _ = tx.send(r);
    });
    // SAFETY: documented signature; the block is copied by SCK.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &cfg,
            Some(&block),
        );
    }
    let image = rx
        .recv_timeout(STEP_TIMEOUT)
        .map_err(|_| CmError::internal("ScreenCaptureKit screenshot timed out"))?
        .map_err(|m| CmError::internal(format!("ScreenCaptureKit screenshot: {m}")))?
        .0;
    frame_from_cgimage(&image).map(Some)
}

fn frame_from_cgimage(image: &CGImage) -> Result<Frame, CmError> {
    let (w, h) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
    let stride = CGImage::bytes_per_row(Some(image));
    if CGImage::bits_per_pixel(Some(image)) != 32 {
        return Err(CmError::internal("unexpected screenshot pixel format"));
    }
    let data = CGImage::data_provider(Some(image))
        .and_then(|p| CGDataProvider::data(Some(&p)))
        .ok_or_else(|| CmError::internal("screenshot has no pixel data"))?
        .to_vec();
    let (w32, h32) = (
        u32::try_from(w).map_err(|_| CmError::internal("screenshot too large"))?,
        u32::try_from(h).map_err(|_| CmError::internal("screenshot too large"))?,
    );
    Frame::new(w32, h32, bgra_to_rgba(&data, w, h, stride)?)
}

/// Repacks a BGRA buffer with row padding into tight opaque RGBA.
fn bgra_to_rgba(data: &[u8], w: usize, h: usize, stride: usize) -> Result<Vec<u8>, CmError> {
    if stride < w * 4 || data.len() < stride.saturating_mul(h.saturating_sub(1)) + w * 4 {
        return Err(CmError::internal("screenshot buffer is too small"));
    }
    let mut out = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        let line = &data[row * stride..row * stride + w * 4];
        for px in line.as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R4-6: the filter excludes exactly the own process's application entry.
    #[test]
    fn exclusion_selects_only_the_own_pid() {
        let e = Exclusions::pid(42);
        assert_eq!(e.select(&[1, 42, 7]), vec![42]);
        assert!(e.select(&[1, 7]).is_empty());
    }

    /// A kept window is an exception only when SCK lists it and its owner is excluded; the toast
    /// (not in the keep set) and unlisted ids stay excluded.
    #[test]
    fn kept_windows_must_be_listed_and_owned_by_an_excluded_process() {
        let e = Exclusions::pid(42).keeping(vec![100, 999, 300]);
        let listed = [
            (100, Some(42)),
            (101, Some(42)),
            (300, Some(7)),
            (301, None),
        ];
        assert_eq!(e.select_kept(&listed), vec![100]);
    }

    #[test]
    fn bgra_with_padding_becomes_tight_rgba() {
        // 2x2, stride 12 (4 bytes of padding per row).
        let data = [
            1, 2, 3, 0, 4, 5, 6, 0, 9, 9, 9, 9, //
            7, 8, 9, 0, 10, 11, 12, 0, 9, 9, 9, 9,
        ];
        let out = bgra_to_rgba(&data, 2, 2, 12).unwrap();
        assert_eq!(
            out,
            vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
        );
        assert!(bgra_to_rgba(&data[..8], 2, 2, 12).is_err());
    }
}
