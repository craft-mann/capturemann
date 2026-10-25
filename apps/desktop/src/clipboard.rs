//! System clipboard for CAP-4, implementing the daemon's `Clipboard` hook: the macOS pasteboard
//! (PNG and TIFF flavors) or `arboard` elsewhere.

use cm_core::CmError;
use cm_daemon::Clipboard;
use std::time::Duration;

/// Decodes PNG bytes to straight RGBA8: `(width, height, pixels)`.
pub fn png_to_rgba(png: &[u8]) -> Result<(usize, usize, Vec<u8>), CmError> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| CmError::invalid_input(format!("not a PNG image: {e}")))?
        .to_rgba8();
    Ok((img.width() as usize, img.height() as usize, img.into_raw()))
}

/// Re-encodes PNG bytes as TIFF (CAP-4: older macOS apps and TIFF-only tools read this flavor).
pub fn png_to_tiff(png: &[u8]) -> Result<Vec<u8>, CmError> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| CmError::invalid_input(format!("not a PNG image: {e}")))?;
    let mut out = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut out),
        image::ImageFormat::Tiff,
    )
    .map_err(|e| CmError::internal(format!("cannot encode TIFF: {e}")))?;
    Ok(out)
}

/// The platform's clipboard, one write at a time. `tiff` is `Some` where the platform wants the
/// extra flavor next to the PNG (macOS); the implementation puts every flavor it is given on the
/// clipboard in one write.
pub trait Pasteboard: Send + Sync {
    fn set_image(&self, png: &[u8], tiff: Option<&[u8]>) -> Result<(), CmError>;

    /// Plain text (a share link). Fakes that only test images need not implement it.
    fn set_text(&self, _text: &str) -> Result<(), CmError> {
        Err(CmError::not_implemented("text clipboard"))
    }
}

/// The system clipboard behind the daemon's `Clipboard` hook: PNG always, TIFF as well when
/// `with_tiff`, and a bounded retry for a clipboard another app holds locked (Windows).
#[derive(Debug, Clone)]
pub struct ClipboardWriter<P: Pasteboard> {
    pasteboard: P,
    with_tiff: bool,
    /// Extra tries after the first failure.
    retries: u32,
    /// Wait between tries.
    pause: Duration,
}

/// Windows: a locked clipboard is retried 3 times over 300 ms (CAP-4 edge case).
pub const LOCKED_RETRIES: u32 = 3;
pub const LOCKED_PAUSE: Duration = Duration::from_millis(100);

impl<P: Pasteboard> ClipboardWriter<P> {
    pub fn new(pasteboard: P, with_tiff: bool, retries: u32, pause: Duration) -> Self {
        ClipboardWriter {
            pasteboard,
            with_tiff,
            retries,
            pause,
        }
    }
}

impl<P: Pasteboard> Clipboard for ClipboardWriter<P> {
    fn copy_png(&self, png: &[u8]) -> Result<(), CmError> {
        // A TIFF that cannot be made never costs the PNG.
        let tiff = if self.with_tiff {
            png_to_tiff(png).ok()
        } else {
            None
        };
        let mut tries = 0;
        loop {
            match self.pasteboard.set_image(png, tiff.as_deref()) {
                Ok(()) => return Ok(()),
                Err(e) if tries >= self.retries => return Err(e),
                Err(_) => {
                    tries += 1;
                    std::thread::sleep(self.pause);
                }
            }
        }
    }
}

/// macOS: the general pasteboard with `public.png` and `public.tiff` set together.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, Default)]
pub struct NativePasteboard;

#[cfg(target_os = "macos")]
impl Pasteboard for NativePasteboard {
    fn set_image(&self, png: &[u8], tiff: Option<&[u8]>) -> Result<(), CmError> {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
        use objc2_foundation::NSData;
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let fail = |what: &str| CmError::internal(format!("cannot copy {what} to the clipboard"));
        if let Some(t) = tiff {
            // SAFETY: reading an AppKit string constant.
            let ty = unsafe { NSPasteboardTypeTIFF };
            if !pb.setData_forType(Some(&NSData::with_bytes(t)), ty) {
                return Err(fail("the TIFF image"));
            }
        }
        // SAFETY: reading an AppKit string constant.
        let ty = unsafe { NSPasteboardTypePNG };
        if pb.setData_forType(Some(&NSData::with_bytes(png)), ty) {
            Ok(())
        } else {
            Err(fail("the image"))
        }
    }

    fn set_text(&self, text: &str) -> Result<(), CmError> {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        // SAFETY: reading an AppKit string constant.
        let ty = unsafe { NSPasteboardTypeString };
        if pb.setString_forType(&NSString::from_str(text), ty) {
            Ok(())
        } else {
            Err(CmError::internal("cannot copy the text to the clipboard"))
        }
    }
}

/// Windows and others: `arboard` (the clipboard keeps a bitmap).
#[cfg(not(target_os = "macos"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct NativePasteboard;

#[cfg(not(target_os = "macos"))]
impl Pasteboard for NativePasteboard {
    fn set_image(&self, png: &[u8], _tiff: Option<&[u8]>) -> Result<(), CmError> {
        let (width, height, bytes) = png_to_rgba(png)?;
        let mut cb = arboard::Clipboard::new()
            .map_err(|e| CmError::internal(format!("clipboard unavailable: {e}")))?;
        cb.set_image(arboard::ImageData {
            width,
            height,
            bytes: bytes.into(),
        })
        .map_err(|e| CmError::internal(format!("cannot copy to the clipboard: {e}")))
    }

    fn set_text(&self, text: &str) -> Result<(), CmError> {
        arboard::Clipboard::new()
            .map_err(|e| CmError::internal(format!("clipboard unavailable: {e}")))
            .and_then(|mut c| {
                c.set_text(text.to_owned())
                    .map_err(|e| CmError::internal(format!("cannot copy to the clipboard: {e}")))
            })
    }
}

/// The app's clipboard: PNG and TIFF on macOS; PNG with the locked-clipboard retry on Windows.
pub type SystemClipboard = ClipboardWriter<NativePasteboard>;

impl Default for SystemClipboard {
    fn default() -> Self {
        let mac = cfg!(target_os = "macos");
        ClipboardWriter::new(
            NativePasteboard,
            mac,
            if mac { 0 } else { LOCKED_RETRIES },
            LOCKED_PAUSE,
        )
    }
}

/// Plain text on the clipboard (share links, STO-4). A trait so the decision to copy is tested
/// with a fake and never touches the real clipboard.
pub trait TextClipboard {
    fn set_text(&self, text: &str) -> Result<(), String>;
}

impl<P: Pasteboard> TextClipboard for ClipboardWriter<P> {
    fn set_text(&self, text: &str) -> Result<(), String> {
        self.pasteboard.set_text(text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    /// CAP-4: the clipboard holds the image at its pixel size.
    #[test]
    fn decodes_png_to_rgba() {
        let (w, h, px) = png_to_rgba(&tiny_png()).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(px.len(), 3 * 2 * 4);
        assert_eq!(&px[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn rejects_non_png() {
        assert!(png_to_rgba(b"hello").is_err());
    }

    /// A pasteboard that records what it was given and refuses the first `refuse` writes.
    #[derive(Default)]
    struct Fake {
        refuse: Mutex<u32>,
        writes: Mutex<Vec<(usize, Option<usize>)>>,
        attempts: Mutex<u32>,
    }
    impl Pasteboard for &Fake {
        fn set_image(&self, png: &[u8], tiff: Option<&[u8]>) -> Result<(), CmError> {
            *self.attempts.lock().unwrap() += 1;
            let mut r = self.refuse.lock().unwrap();
            if *r > 0 {
                *r -= 1;
                return Err(CmError::internal("clipboard is locked"));
            }
            self.writes
                .lock()
                .unwrap()
                .push((png.len(), tiff.map(<[u8]>::len)));
            Ok(())
        }
    }

    fn writer(f: &Fake, tiff: bool, retries: u32) -> ClipboardWriter<&Fake> {
        ClipboardWriter::new(f, tiff, retries, Duration::from_millis(1))
    }

    /// CAP-4: the TIFF is a real TIFF of the same size and pixels.
    #[test]
    fn tiff_flavor_decodes_to_the_same_pixels() {
        let tiff = png_to_tiff(&tiny_png()).unwrap();
        assert!(tiff.starts_with(b"II*\0") || tiff.starts_with(b"MM\0*"));
        let img = image::load_from_memory_with_format(&tiff, image::ImageFormat::Tiff)
            .unwrap()
            .to_rgba8();
        assert_eq!((img.width(), img.height()), (3, 2));
        assert_eq!(img.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert!(png_to_tiff(b"hello").is_err());
    }

    /// CAP-4 (macOS): PNG and TIFF are set in one write; elsewhere only the PNG.
    #[test]
    fn writes_the_flavors_asked_for() {
        let png = tiny_png();
        let f = Fake::default();
        writer(&f, true, 0).copy_png(&png).unwrap();
        writer(&f, false, 0).copy_png(&png).unwrap();
        let w = f.writes.lock().unwrap().clone();
        assert_eq!(w[0].0, png.len());
        assert!(w[0].1.is_some_and(|n| n > 0), "TIFF set next to the PNG");
        assert_eq!(w[1], (png.len(), None));
    }

    /// A TIFF that cannot be built never costs the PNG.
    #[test]
    fn tiff_failure_still_writes_the_png() {
        let f = Fake::default();
        writer(&f, true, 0).copy_png(b"not a png").unwrap();
        assert_eq!(f.writes.lock().unwrap()[0], (9, None));
    }

    /// CAP-4 edge case: a clipboard locked twice is retried and then works.
    #[test]
    fn retries_a_locked_clipboard() {
        let png = tiny_png();
        let f = Fake {
            refuse: Mutex::new(2),
            ..Fake::default()
        };
        writer(&f, false, LOCKED_RETRIES).copy_png(&png).unwrap();
        assert_eq!(*f.attempts.lock().unwrap(), 3);
        assert_eq!(f.writes.lock().unwrap().len(), 1);
    }

    /// After the retries the error comes back (the toast then says "Copied to library only").
    #[test]
    fn gives_up_after_the_retries() {
        let png = tiny_png();
        let f = Fake {
            refuse: Mutex::new(99),
            ..Fake::default()
        };
        assert!(writer(&f, false, LOCKED_RETRIES).copy_png(&png).is_err());
        assert_eq!(*f.attempts.lock().unwrap(), 1 + LOCKED_RETRIES);
        assert!(f.writes.lock().unwrap().is_empty());
        // No retries asked for: one try.
        let g = Fake {
            refuse: Mutex::new(99),
            ..Fake::default()
        };
        assert!(writer(&g, false, 0).copy_png(&png).is_err());
        assert_eq!(*g.attempts.lock().unwrap(), 1);
    }

    /// Writes the real macOS pasteboard (replaces what you copied): run by hand with
    /// `cargo test -p cm-desktop --lib clipboard -- --ignored`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "overwrites the user's clipboard"]
    fn real_pasteboard_has_png_and_tiff() {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
        SystemClipboard::default().copy_png(&tiny_png()).unwrap();
        let pb = NSPasteboard::generalPasteboard();
        // SAFETY: reading AppKit string constants.
        let (png, tiff) = unsafe {
            (
                pb.dataForType(NSPasteboardTypePNG),
                pb.dataForType(NSPasteboardTypeTIFF),
            )
        };
        assert!(png.is_some_and(|d| !d.is_empty()));
        assert!(tiff.is_some_and(|d| !d.is_empty()));
    }
}
