//! `Presenter`: shows the newest RGBA picture a producer made, through `surface()` (NV12 buffer,
//! macOS) or `img()` (BGRA upload). Unlike [`super::PreviewSurface`], which paces a synthetic
//! worker for the 1080p60 test, the picture here comes from the editor's compositor on demand.

use std::sync::Arc;

use gpui::{img, prelude::*, AnyElement, App, ObjectFit, RenderImage, Window};
use image::{Frame, RgbaImage};

use super::convert::rgba_to_bgra;
use super::view::PreviewPath;

enum Payload {
    Cpu(Arc<RenderImage>),
    #[cfg(target_os = "macos")]
    Surface(super::macos::SendBuffer),
}

/// The path to use: `surface()` on macOS unless `CM_PREVIEW_PATH=cpu`, else the CPU upload.
pub fn default_path() -> PreviewPath {
    let forced_cpu = std::env::var("CM_PREVIEW_PATH").is_ok_and(|v| v.eq_ignore_ascii_case("cpu"));
    if cfg!(target_os = "macos") && !forced_cpu {
        PreviewPath::Surface
    } else {
        PreviewPath::Cpu
    }
}

pub struct Presenter {
    path: PreviewPath,
    shown: Option<Payload>,
}

impl Presenter {
    pub fn new(path: PreviewPath) -> Presenter {
        Presenter { path, shown: None }
    }

    pub fn path(&self) -> PreviewPath {
        self.path
    }

    pub fn has_picture(&self) -> bool {
        self.shown.is_some()
    }

    /// Makes `picture` the one on screen. Call from inside `render`: the CPU path hands the old
    /// image back to GPUI with the window (without it the atlas keeps every frame, docs/project/spikes.md S5).
    pub fn set(&mut self, picture: &RgbaImage, window: &mut Window, cx: &mut App) {
        let payload = match self.path {
            #[cfg(target_os = "macos")]
            PreviewPath::Surface => {
                match super::macos::pixel_buffer(&super::convert::rgba_to_nv12(picture)) {
                    Ok(b) => Payload::Surface(b),
                    Err(e) => {
                        log::warn!("preview surface failed, using the CPU path: {e}");
                        self.path = PreviewPath::Cpu;
                        return self.set(picture, window, cx);
                    }
                }
            }
            _ => Payload::Cpu(Arc::new(RenderImage::new([Frame::new(rgba_to_bgra(
                picture,
            ))]))),
        };
        if let Some(Payload::Cpu(old)) = self.shown.replace(payload) {
            cx.drop_image(old, Some(window));
        }
    }

    /// The element for the current picture, contained in its parent.
    pub fn element(&self) -> Option<AnyElement> {
        match self.shown.as_ref()? {
            Payload::Cpu(image) => Some(
                img(image.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            #[cfg(target_os = "macos")]
            Payload::Surface(buf) => Some(
                gpui::surface(buf.0.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
        }
    }
}
