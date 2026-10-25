//! Cursor image extraction through AppKit (`NSCursor.currentSystemCursor`, deprecated but the
//! only public way to read another app's current cursor; proven in spike S4).
//!
//! Threading (R3-24): AppKit is documented as main-thread-only, and this poller runs on the
//! recorder's own thread. `NSCursor.currentSystemCursor`, `image` and `TIFFRepresentation` are
//! read-only and have worked off the main thread in every run on macOS 13 to 26 (spike S4, the
//! REC-7 manual runs); the result is checked for nil and a failure only drops that cursor image
//! (the track keeps its positions). If a future macOS starts to object, hop this call to the
//! main queue with `dispatch2::DispatchQueue::main().exec_sync`; the poll is a few hundred
//! microseconds and runs at most every 100 ms, so it is cheap, but a blocked main thread would
//! then stall the recorder.

#![allow(deprecated)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSCursor};
use objc2_foundation::NSDictionary;

use crate::image::CursorImage;
use crate::source::ImageProvider;

/// Reports the system cursor when it changes. Change detection hashes the TIFF bytes plus the
/// hotspot in process only (the persisted id comes from [`CursorImage::id`]).
pub struct MacImageProvider {
    last: Option<u64>,
    display_scale: f64,
}

impl MacImageProvider {
    /// `display_scale` picks the cursor representation closest to the recorded display's scale.
    pub fn new(display_scale: f64) -> Self {
        MacImageProvider {
            last: None,
            display_scale,
        }
    }
}

impl ImageProvider for MacImageProvider {
    fn poll(&mut self) -> Option<CursorImage> {
        let cursor = NSCursor::currentSystemCursor()?;
        let hot = cursor.hotSpot();
        let image = cursor.image();
        let size = image.size();
        let tiff = image.TIFFRepresentation()?;
        let mut h = DefaultHasher::new();
        tiff.to_vec().hash(&mut h);
        hot.x.to_bits().hash(&mut h);
        hot.y.to_bits().hash(&mut h);
        let key = h.finish();
        if self.last == Some(key) {
            return None;
        }
        self.last = Some(key);

        let reps = NSBitmapImageRep::imageRepsWithData(&tiff);
        let bitmaps = reps
            .iter()
            .filter_map(|r| r.downcast::<NSBitmapImageRep>().ok());
        let rep: Retained<NSBitmapImageRep> = best_rep(bitmaps, size.width, self.display_scale)?;
        let scale = if size.width > 0.0 {
            rep.pixelsWide() as f64 / size.width
        } else {
            1.0
        };
        // SAFETY: plain AppKit conversion of a bitmap rep with an empty property dictionary.
        let png = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        let hotspot = [
            (hot.x * scale).round() as i32,
            (hot.y * scale).round() as i32,
        ];
        CursorImage::from_png(&png.to_vec(), hotspot, scale).ok()
    }
}

/// The representation whose pixel scale is the smallest one at or above `want`, else the largest.
fn best_rep(
    reps: impl Iterator<Item = Retained<NSBitmapImageRep>>,
    width_pt: f64,
    want: f64,
) -> Option<Retained<NSBitmapImageRep>> {
    let scale_of = |r: &NSBitmapImageRep| {
        if width_pt > 0.0 {
            r.pixelsWide() as f64 / width_pt
        } else {
            1.0
        }
    };
    let mut best: Option<(f64, Retained<NSBitmapImageRep>)> = None;
    for r in reps {
        let s = scale_of(&r);
        let better = match &best {
            None => true,
            Some((bs, _)) => {
                if s >= want - 0.01 {
                    *bs < want - 0.01 || s < *bs
                } else {
                    *bs < want - 0.01 && s > *bs
                }
            }
        };
        if better {
            best = Some((s, r));
        }
    }
    best.map(|(_, r)| r)
}
