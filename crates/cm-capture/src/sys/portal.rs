//! Screen capture on a Wayland session: the desktop portal's Screenshot interface. A Wayland
//! client may not read other programs' pixels, so the compositor takes the picture and hands back
//! a file. The first request asks the user for permission (the desktop remembers the answer).
//!
//! The portal returns one image of the whole desktop. Where the compositor says how its monitors
//! are laid out (GNOME: [`super::mutter`]) each monitor is a display and a display or region
//! capture crops the picture ([`crate::layout`]); elsewhere the backend reports a single display,
//! "Desktop". There are no windows to list: a window comes from the desktop's own picker
//! (`Screenshot` with `interactive: true`), see [`PortalBackend`].

// Only Linux calls the portal; the other systems compile the pure parts for their tests.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cm_core::{CmError, PhysicalPoint, PhysicalSize};

use crate::layout::{layout_for_picture, single_desktop, DesktopLayout, LogicalMonitor};
use crate::{Capabilities, CaptureBackend, DisplayInfo, Frame, WindowInfo};

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

/// How long the desktop's own picker may stay open with nobody choosing (an agent asked while no
/// one sits at the screen). The request is then closed, which also closes the picker.
pub const PICK_TIMEOUT: Duration = Duration::from_secs(120);

/// The desktop portal's Screenshot interface as the backend needs it, so the backend is tested
/// with a scripted portal.
pub trait ScreenshotPortal: Send + Sync {
    /// One picture. `interactive: false` is the whole desktop, taken at once (the first request
    /// asks the user for permission). `interactive: true` shows the desktop's picker for an
    /// area, a window or a screen and returns what the person chose.
    fn take(&self, interactive: bool) -> Result<Frame, CmError>;
}

/// Where the monitor layout comes from; `None` when the desktop does not say.
pub trait MonitorSource: Send + Sync {
    fn monitors(&self) -> Option<Vec<LogicalMonitor>>;
}

/// The real portal (Linux only; elsewhere it answers `not_implemented`).
#[derive(Debug, Default)]
pub struct SystemPortal;

/// GNOME's monitor layout over D-Bus (see [`super::mutter`]).
#[derive(Debug, Default)]
pub struct MutterMonitors;

impl MonitorSource for MutterMonitors {
    fn monitors(&self) -> Option<Vec<LogicalMonitor>> {
        super::mutter::current_monitors()
    }
}

#[cfg(target_os = "linux")]
impl ScreenshotPortal for SystemPortal {
    fn take(&self, interactive: bool) -> Result<Frame, CmError> {
        use ashpd::desktop::screenshot::Screenshot;

        // One request at a time: the desktop's screenshot service does not answer two at once.
        static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let ask = async {
            let response = Screenshot::request()
                .interactive(interactive)
                .modal(false)
                .send()
                .await?
                .response()?;
            Ok::<_, ashpd::Error>(response.uri().as_str().to_owned())
        };
        let outcome: Result<String, PortalFailure> = if interactive {
            // Dropping the request future on a timeout closes the portal request, and with it
            // the picker the compositor put on screen.
            futures_lite::future::block_on(futures_lite::future::or(
                async { ask.await.map_err(PortalFailure::Portal) },
                async {
                    async_io::Timer::after(PICK_TIMEOUT).await;
                    Err(PortalFailure::TimedOut)
                },
            ))
        } else {
            futures_lite::future::block_on(ask).map_err(PortalFailure::Portal)
        };
        let uri = outcome.map_err(|f| portal_error(f, interactive))?;
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
}

#[cfg(not(target_os = "linux"))]
impl ScreenshotPortal for SystemPortal {
    fn take(&self, _interactive: bool) -> Result<Frame, CmError> {
        Err(CmError::not_implemented(
            "the desktop portal exists on Linux only",
        ))
    }
}

/// Why a portal request failed.
#[cfg(target_os = "linux")]
enum PortalFailure {
    Portal(ashpd::Error),
    TimedOut,
}

#[cfg(target_os = "linux")]
fn portal_error(failure: PortalFailure, interactive: bool) -> CmError {
    use ashpd::desktop::ResponseError;
    use ashpd::Error;
    match failure {
        PortalFailure::TimedOut => CmError::confirmation_timeout(format!(
            "Nobody chose anything in the desktop's screenshot picker within {} seconds, so it \
             was closed. An interactive screenshot needs a person at the screen.",
            PICK_TIMEOUT.as_secs()
        )),
        // In the picker "Cancelled" is Esc or a closed dialog. GNOME answers "Other" for the
        // same (it logs "InteractiveScreenshot didn't return a file" for Esc as well as for a
        // failed capture, so the two cannot be told apart): both are "no image was chosen".
        PortalFailure::Portal(Error::Response(ResponseError::Cancelled | ResponseError::Other))
            if interactive =>
        {
            CmError::user_declined(
                "The screenshot picker ended without an image (closed, or the desktop's picker \
                 failed). Try again, or use a display or region screenshot.",
            )
        }
        // Otherwise "Cancelled" is a refused prompt; "Other" is what a prompt nobody answered
        // ends as.
        PortalFailure::Portal(Error::Response(ResponseError::Cancelled | ResponseError::Other)) => {
            CmError::permission_missing(
                cm_core::Permission::Screen,
                "The desktop did not take the screenshot. Allow CaptureMann to take screenshots \
                 when the desktop asks (the request may also have timed out unanswered).",
            )
        }
        PortalFailure::Portal(other) => CmError::internal(format!(
            "the desktop portal cannot take a screenshot: {other}"
        )),
    }
}

/// Captures through `org.freedesktop.portal.Screenshot`.
///
/// The portal returns one image of the whole desktop. When the compositor says how its monitors
/// are laid out ([`MonitorSource`]) each monitor is a display and a display capture crops the
/// picture ([`crate::layout`]); otherwise there is one display, "Desktop". Windows cannot be
/// listed or captured by id on Wayland: [`CaptureBackend::pick_interactive`] shows the desktop's
/// own picker instead.
pub struct PortalBackend {
    portal: Box<dyn ScreenshotPortal>,
    monitors: Box<dyn MonitorSource>,
    last: Mutex<Option<(Instant, Frame)>>,
}

impl Default for PortalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PortalBackend {
    pub fn new() -> Self {
        Self::with_parts(Box::new(SystemPortal), Box::new(MutterMonitors))
    }

    /// A backend over another portal and monitor source (tests).
    pub fn with_parts(portal: Box<dyn ScreenshotPortal>, monitors: Box<dyn MonitorSource>) -> Self {
        PortalBackend {
            portal,
            monitors,
            last: Mutex::new(None),
        }
    }

    /// The size of the picture still on hand, if it is recent enough to serve the next capture.
    fn recent_size(&self) -> Option<PhysicalSize> {
        let last = self.last.lock().ok()?;
        let (at, frame) = last.as_ref()?;
        (at.elapsed() < REUSE).then(|| frame.size())
    }

    fn take_recent(&self) -> Option<Frame> {
        let mut last = self.last.lock().ok()?;
        match last.take() {
            Some((at, frame)) if at.elapsed() < REUSE => Some(frame),
            _ => None,
        }
    }

    fn layout_for(&self, picture: PhysicalSize) -> DesktopLayout {
        self.monitors
            .monitors()
            .and_then(|m| layout_for_picture(&m, picture))
            .unwrap_or_else(|| single_desktop(picture))
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
                let frame = self.portal.take(false)?;
                let size = frame.size();
                if let Ok(mut last) = self.last.lock() {
                    *last = Some((Instant::now(), frame));
                }
                size
            }
        };
        Ok(self.layout_for(size).displays)
    }

    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        // A Wayland client cannot list other programs' windows.
        Ok(Vec::new())
    }

    fn capture_display(&self, display_id: u32) -> Result<Frame, CmError> {
        let picture = match self.take_recent() {
            Some(frame) => frame,
            None => self.portal.take(false)?,
        };
        let layout = self.layout_for(picture.size());
        let crop = layout
            .crop_of(display_id)
            .ok_or_else(|| CmError::not_found(format!("display {display_id}")))?;
        if crop.origin == (PhysicalPoint { x: 0, y: 0 }) && crop.size == picture.size() {
            Ok(picture)
        } else {
            picture.crop(crop)
        }
    }

    fn capture_window(&self, window_id: u32) -> Result<Frame, CmError> {
        Err(CmError::not_implemented(format!(
            "window {window_id}: a Wayland session has no window ids. Use the desktop's own \
             picker instead (capture_screenshot with target {{\"pick\": true}}, \
             `capturemann capture screenshot --pick`)."
        )))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            list_windows: false,
            capture_window: false,
            capture_region: true,
            pick: true,
        }
    }

    fn pick_interactive(&self) -> Result<Frame, CmError> {
        // Whatever picture is on hand is older than the person's choice.
        let _ = self.take_recent();
        self.portal.take(true)
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

    /// A scripted portal: every non-interactive picture is a pattern of the given size, the
    /// interactive answer is scripted, and the calls are counted.
    struct FakePortal {
        size: PhysicalSize,
        pick: Mutex<Option<Result<Frame, CmError>>>,
        calls: Mutex<Vec<bool>>,
    }

    impl FakePortal {
        fn new(width: u32, height: u32) -> std::sync::Arc<Self> {
            std::sync::Arc::new(FakePortal {
                size: PhysicalSize { width, height },
                pick: Mutex::new(None),
                calls: Mutex::new(Vec::new()),
            })
        }
    }

    impl ScreenshotPortal for std::sync::Arc<FakePortal> {
        fn take(&self, interactive: bool) -> Result<Frame, CmError> {
            self.calls.lock().unwrap().push(interactive);
            if interactive {
                return self
                    .pick
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap_or_else(|| Err(CmError::user_declined("nothing scripted")));
            }
            // Pixel (x, y) encodes its position, so a crop can be checked exactly.
            let mut data = Vec::new();
            for y in 0..self.size.height {
                for x in 0..self.size.width {
                    data.extend_from_slice(&[(x % 251) as u8, (y % 251) as u8, 7, 255]);
                }
            }
            Frame::new(self.size.width, self.size.height, data)
        }
    }

    struct FakeMonitors(Option<Vec<LogicalMonitor>>);

    impl MonitorSource for FakeMonitors {
        fn monitors(&self) -> Option<Vec<LogicalMonitor>> {
            self.0.clone()
        }
    }

    fn mon(
        name: &str,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        scale: f64,
        primary: bool,
    ) -> LogicalMonitor {
        LogicalMonitor {
            name: name.to_owned(),
            x,
            y,
            width: w,
            height: h,
            scale,
            primary,
        }
    }

    fn backend(
        portal: &std::sync::Arc<FakePortal>,
        m: Option<Vec<LogicalMonitor>>,
    ) -> PortalBackend {
        PortalBackend::with_parts(Box::new(portal.clone()), Box::new(FakeMonitors(m)))
    }

    fn px(f: &Frame, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * f.width() + x) * 4) as usize;
        f.rgba()[i..i + 4].try_into().unwrap()
    }

    /// CAP-1 on Wayland without a monitor layout: one "Desktop" display.
    #[test]
    fn without_a_layout_there_is_one_desktop_display() {
        let portal = FakePortal::new(64, 32);
        let b = backend(&portal, None);
        let d = b.displays().unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].name, "Desktop");
        assert_eq!(
            d[0].bounds.size,
            PhysicalSize {
                width: 64,
                height: 32
            }
        );
        assert_eq!(b.capture_display(1).unwrap().size(), d[0].bounds.size);
        // The listing's picture served the capture: one portal request, not two.
        assert_eq!(*portal.calls.lock().unwrap(), vec![false]);
        assert!(b.capture_display(2).is_err());
    }

    /// CAP-1.AC1 / CAP-3: display N is a crop of the desktop picture, mixed DPI and negative
    /// origins included.
    #[test]
    fn each_monitor_is_a_crop_of_the_one_picture() {
        // Left monitor (negative x, 1x, 40x20 logical) and the primary (2x, 20x20 logical),
        // picture at 2x: 120x40 px.
        let portal = FakePortal::new(120, 40);
        let b = backend(
            &portal,
            Some(vec![
                mon("DP-1", -40, 0, 40, 20, 1.0, false),
                mon("eDP-1", 0, 0, 20, 20, 2.0, true),
            ]),
        );
        let d = b.displays().unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].name, "eDP-1");
        assert_eq!((d[0].bounds.origin.x, d[0].bounds.size.width), (0, 40));
        assert_eq!(d[1].bounds.origin.x, -80);
        assert_eq!(d[1].scale.get(), 1.0);
        let primary = b.capture_display(1).unwrap();
        assert_eq!(
            primary.size(),
            PhysicalSize {
                width: 40,
                height: 40
            }
        );
        // The primary starts 80 px into the picture.
        assert_eq!(px(&primary, 0, 0), [80, 0, 7, 255]);
        let left = b.capture_display(2).unwrap();
        assert_eq!(
            left.size(),
            PhysicalSize {
                width: 80,
                height: 40
            }
        );
        assert_eq!(px(&left, 79, 39), [79, 39, 7, 255]);
    }

    /// A layout that does not fit the picture is ignored: one Desktop display.
    #[test]
    fn a_layout_that_does_not_fit_the_picture_is_ignored() {
        let portal = FakePortal::new(100, 100);
        let b = backend(&portal, Some(vec![mon("A", 0, 0, 100, 40, 1.0, true)]));
        let d = b.displays().unwrap();
        assert_eq!((d.len(), d[0].name.as_str()), (1, "Desktop"));
    }

    /// The picker: the compositor's choice comes back as it is, and no stale desktop picture
    /// is used.
    #[test]
    fn the_pick_returns_what_the_person_chose() {
        let portal = FakePortal::new(64, 32);
        let b = backend(&portal, None);
        b.displays().unwrap();
        *portal.pick.lock().unwrap() = Some(Frame::new(2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]));
        let f = b.pick_interactive().unwrap();
        assert_eq!(
            f.size(),
            PhysicalSize {
                width: 2,
                height: 1
            }
        );
        assert_eq!(*portal.calls.lock().unwrap(), vec![false, true]);
        assert!(b.capabilities().pick);
        assert!(!b.capabilities().list_windows && !b.capabilities().capture_window);
        // Closing the picker is "declined", not a failure.
        *portal.pick.lock().unwrap() = Some(Err(CmError::user_declined("closed")));
        assert_eq!(
            b.pick_interactive().unwrap_err().code(),
            cm_core::ErrorCode::UserDeclined
        );
    }

    #[test]
    fn a_window_by_id_says_how_to_pick_one() {
        let b = backend(&FakePortal::new(8, 8), None);
        let e = b.capture_window(5).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::NotImplemented);
        assert!(e.to_string().contains("pick"), "{e}");
        assert!(b.windows().unwrap().is_empty());
    }
}
