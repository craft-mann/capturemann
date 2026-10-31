//! Screen capture on a Wayland session: the desktop portal's Screenshot interface. A Wayland
//! client may not read other programs' pixels, so the compositor takes the picture and hands back
//! a file. The first request asks the user for permission (the desktop remembers the answer).
//!
//! The portal returns one image of the whole desktop. The backend therefore reports a single
//! display, "Desktop", at scale 1 (the portal does not say how the monitors are laid out), and
//! no windows: a window or a region comes from cropping that image.

// Only Linux calls the portal; the other systems compile the pure parts for their tests.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cm_core::{CmError, PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};

use crate::{CaptureBackend, DisplayInfo, Frame, WindowInfo};

/// The id of the one display this backend reports.
pub const DESKTOP_ID: u32 = 1;

/// The local path of a `file://` URI (percent-escapes decoded); `None` for any other URI.
pub fn path_from_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file://host/path` and `file:///path`: the portal sends the second form.
    let path = &rest[rest.find('/')?..];
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = path.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(out).ok()?;
    Some(PathBuf::from(text))
}

/// Decodes a PNG (8 or 16 bit; grey, RGB or RGBA) into a [`Frame`].
pub fn frame_from_png(bytes: &[u8]) -> Result<Frame, CmError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| CmError::internal(format!("the portal's screenshot is not a PNG: {e}")))?;
    let mut buf =
        vec![
            0;
            reader
                .output_buffer_size()
                .ok_or_else(|| CmError::internal("the portal's screenshot is too large"))?
        ];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| CmError::internal(format!("cannot decode the portal's screenshot: {e}")))?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => px
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => {
            return Err(CmError::internal(
                "the portal's screenshot is palette-based",
            ))
        }
    };
    Frame::new(info.width, info.height, rgba)
}

/// How long the picture taken to learn the desktop size serves the capture that follows. Every
/// portal request is a visible event (a flash, perhaps a prompt), so a listing followed by a
/// capture must not take two.
const REUSE: Duration = Duration::from_secs(3);

/// Captures through `org.freedesktop.portal.Screenshot`.
#[derive(Debug, Default)]
pub struct PortalBackend {
    last: Mutex<Option<(Instant, Frame)>>,
}

impl PortalBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// The size of the picture still on hand, if it is recent enough to serve the next capture.
    fn recent_size(&self) -> Option<PhysicalSize> {
        let last = self.last.lock().ok()?;
        let (at, frame) = last.as_ref()?;
        (at.elapsed() < REUSE).then(|| PhysicalSize {
            width: frame.width(),
            height: frame.height(),
        })
    }

    fn take_recent(&self) -> Option<Frame> {
        let mut last = self.last.lock().ok()?;
        match last.take() {
            Some((at, frame)) if at.elapsed() < REUSE => Some(frame),
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    fn screenshot(&self) -> Result<Frame, CmError> {
        use ashpd::desktop::screenshot::Screenshot;

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CmError::internal(format!("cannot start the portal client: {e}")))?;
        let uri = runtime.block_on(async {
            let response = Screenshot::request()
                .interactive(false)
                .modal(false)
                .send()
                .await?
                .response()?;
            Ok::<_, ashpd::Error>(response.uri().as_str().to_owned())
        });
        let uri = uri.map_err(portal_error)?;
        let path = path_from_file_uri(&uri)
            .ok_or_else(|| CmError::internal(format!("the portal returned {uri}, not a file")))?;
        let bytes = std::fs::read(&path).map_err(|e| {
            CmError::internal(format!(
                "cannot read the screenshot {}: {e}",
                path.display()
            ))
        })?;
        // The portal wrote this file for us alone; leave nothing in the user's Pictures folder.
        let _ = std::fs::remove_file(&path);
        frame_from_png(&bytes)
    }

    #[cfg(not(target_os = "linux"))]
    fn screenshot(&self) -> Result<Frame, CmError> {
        Err(CmError::not_implemented(
            "the desktop portal exists on Linux only",
        ))
    }
}

#[cfg(target_os = "linux")]
fn portal_error(e: ashpd::Error) -> CmError {
    use ashpd::desktop::ResponseError;
    use ashpd::Error;
    match e {
        // "Cancelled" is a refused prompt; "Other" is what a prompt nobody answered ends as.
        Error::Response(ResponseError::Cancelled | ResponseError::Other) => {
            CmError::permission_missing(
                cm_core::Permission::Screen,
                "The desktop did not take the screenshot. Allow CaptureMann to take screenshots \
                 when the desktop asks (the request may also have timed out unanswered).",
            )
        }
        other => CmError::internal(format!(
            "the desktop portal cannot take a screenshot: {other}"
        )),
    }
}

impl CaptureBackend for PortalBackend {
    fn check_permission(&self) -> Result<(), CmError> {
        // The portal asks the user on the first capture; there is nothing to preflight.
        Ok(())
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
        // The size is only known from a picture; keep it for the capture that follows.
        let size = match self.recent_size() {
            Some(size) => size,
            None => {
                let frame = self.screenshot()?;
                let size = PhysicalSize {
                    width: frame.width(),
                    height: frame.height(),
                };
                if let Ok(mut last) = self.last.lock() {
                    *last = Some((Instant::now(), frame));
                }
                size
            }
        };
        Ok(vec![DisplayInfo {
            id: DESKTOP_ID,
            name: "Desktop".to_owned(),
            bounds: PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 0 },
                size,
            },
            scale: ScaleFactor::new(1.0)?,
            is_primary: true,
            is_builtin: false,
        }])
    }

    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        // A Wayland client cannot list other programs' windows.
        Ok(Vec::new())
    }

    fn capture_display(&self, display_id: u32) -> Result<Frame, CmError> {
        if display_id != DESKTOP_ID {
            return Err(CmError::not_found(format!("display {display_id}")));
        }
        match self.take_recent() {
            Some(frame) => Ok(frame),
            None => self.screenshot(),
        }
    }

    fn capture_window(&self, window_id: u32) -> Result<Frame, CmError> {
        Err(CmError::not_found(format!(
            "window {window_id}: a Wayland session does not list windows"
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_file_uri_becomes_a_path() {
        assert_eq!(
            path_from_file_uri("file:///home/a/Pictures/Screenshot%20from%202026.png"),
            Some(PathBuf::from("/home/a/Pictures/Screenshot from 2026.png"))
        );
        assert_eq!(
            path_from_file_uri("file://host/tmp/x.png"),
            Some(PathBuf::from("/tmp/x.png"))
        );
        assert_eq!(path_from_file_uri("https://example.com/x.png"), None);
        assert_eq!(path_from_file_uri("file:///bad%2"), None);
        assert_eq!(path_from_file_uri("file:///bad%zz"), None);
    }

    fn png_of(color: png::ColorType, w: u32, h: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn png_colour_types_all_become_rgba() {
        let rgb = png_of(png::ColorType::Rgb, 2, 1, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(
            frame_from_png(&rgb).unwrap(),
            Frame::new(2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]).unwrap()
        );
        let rgba = png_of(png::ColorType::Rgba, 1, 1, &[9, 8, 7, 6]);
        assert_eq!(
            frame_from_png(&rgba).unwrap(),
            Frame::new(1, 1, vec![9, 8, 7, 6]).unwrap()
        );
        let grey = png_of(png::ColorType::Grayscale, 1, 1, &[50]);
        assert_eq!(
            frame_from_png(&grey).unwrap(),
            Frame::new(1, 1, vec![50, 50, 50, 255]).unwrap()
        );
        assert!(frame_from_png(b"not a png").is_err());
    }
}
