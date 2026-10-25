//! The headless video-frame compositor (M4): one output frame from a [`Scene`].
//!
//! A scene is plain data (no edit model, no timeline): the output size, a background (none,
//! color, gradient, image), padding and corner radius (EDT-3), the screen frame with an optional
//! crop/zoom rectangle, the camera frame with a layout (CAM-3: screen + bubble, screen only,
//! camera full, side by side) and an optional annotation overlay. [`Compositor::render`] draws it
//! on the GPU (`wgpu`, Metal on macOS, Direct3D 12 on Windows) without a window and reads the
//! result back as an RGBA image. The same code can later feed the editor preview.
//!
//! [`plan`] is the pure geometry step (where each tile goes, the crop it shows); it needs no GPU
//! and is unit tested directly.
//!
//! Units: pixels of the output frame unless a field says "fraction" or "pct". Colors are 8-bit
//! sRGB, blended in sRGB-encoded space.

mod caption;
mod gpu;
mod plan;

pub use caption::{
    caption_px, max_text_width, place as place_caption, safe_insets, CaptionLayer, BOX_ALPHA,
    CAPTION_SIZE_FRAC, MIN_CAPTION_PX,
};
pub use gpu::Compositor;
pub use plan::{padded_cell, plan, plan_blend, Plan, Tile};

use cm_core::CmError;

/// Pixel layout of a [`Frame`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 4 bytes per pixel, R G B A, straight alpha (alpha is ignored for video frames).
    Rgba8,
    /// 4 bytes per pixel, B G R A (what ScreenCaptureKit and many cameras deliver).
    Bgra8,
    /// Biplanar 4:2:0: `width * height` luma bytes, then `width * height / 2` interleaved U V
    /// bytes. BT.709, limited (video) range. Width and height must be even.
    Nv12,
}

/// A borrowed picture. Rows are tightly packed (no stride).
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: &'a [u8],
}

impl<'a> Frame<'a> {
    pub fn rgba(width: u32, height: u32, data: &'a [u8]) -> Self {
        Frame {
            width,
            height,
            format: PixelFormat::Rgba8,
            data,
        }
    }

    /// Checks the size, the even-dimension rule of NV12 and the byte count.
    pub fn validate(&self, what: &str) -> Result<(), CmError> {
        let (w, h) = (u64::from(self.width), u64::from(self.height));
        if w == 0 || h == 0 || w > 16_384 || h > 16_384 {
            return Err(CmError::invalid_field(
                what,
                format!("frame size {}x{} is out of range", self.width, self.height),
            ));
        }
        let want = match self.format {
            PixelFormat::Rgba8 | PixelFormat::Bgra8 => w * h * 4,
            PixelFormat::Nv12 => {
                if w % 2 != 0 || h % 2 != 0 {
                    return Err(CmError::invalid_field(
                        what,
                        "NV12 needs even width and height",
                    ));
                }
                w * h * 3 / 2
            }
        };
        if self.data.len() as u64 != want {
            return Err(CmError::invalid_field(
                what,
                format!("expected {want} bytes, got {}", self.data.len()),
            ));
        }
        Ok(())
    }
}

/// The frame behind the content.
#[derive(Debug, Clone, Copy, Default)]
pub enum Background<'a> {
    /// Opaque black: bars and padding show black (EDT-3: background off).
    #[default]
    None,
    Color([u8; 3]),
    /// Linear gradient from `from` to `to`; `angle_deg` is the direction it runs in, clockwise
    /// from "up": 0 runs bottom to top, 90 left to right, 180 top to bottom.
    Gradient {
        from: [u8; 3],
        to: [u8; 3],
        angle_deg: f32,
    },
    /// Scaled to cover the output, centered, never distorted (EDT-3.AC2).
    Image(Frame<'a>),
}

/// The screen picture and the part of it that is shown.
#[derive(Debug, Clone, Copy)]
pub struct ScreenLayer<'a> {
    pub frame: Frame<'a>,
    /// Crop/zoom rectangle as fractions of the frame `[x, y, w, h]`; `None` shows all of it.
    pub crop: Option<[f32; 4]>,
}

impl<'a> ScreenLayer<'a> {
    pub fn new(frame: Frame<'a>) -> Self {
        ScreenLayer { frame, crop: None }
    }
}

/// The camera picture.
#[derive(Debug, Clone, Copy)]
pub struct CameraLayer<'a> {
    pub frame: Frame<'a>,
    /// Flip horizontally (a mirrored selfie view).
    pub mirror: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleShape {
    Circle,
    Rounded,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    pub width_px: f32,
    pub color: [u8; 4],
}

/// The camera bubble over the screen (CAM-1, CAM-3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bubble {
    pub shape: BubbleShape,
    /// Diameter (side) as a fraction of the shorter output side.
    pub size: f32,
    /// Center as fractions of the output frame. The bubble is clamped so it lies fully inside
    /// the frame with a small safe margin (CAM-3.AC4).
    pub center: (f32, f32),
    pub ring: Option<Ring>,
}

impl Bubble {
    /// The default placement: bottom left (CAM-1).
    pub fn bottom_left() -> Self {
        Bubble {
            shape: BubbleShape::Circle,
            size: 0.22,
            center: (0.0, 1.0),
            ring: None,
        }
    }
}

/// How the screen and the camera share the frame (CAM-3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layout {
    ScreenOnly,
    /// Screen with the camera bubble on top. Without a camera layer this is `ScreenOnly`.
    ScreenBubble(Bubble),
    /// The camera fills the frame; the screen is not drawn.
    CameraFull,
    /// Screen on 2/3 of the width, camera on 1/3. Without a camera layer this is `ScreenOnly`.
    SideBySide,
}

/// What a click looks like (EDT-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClickStyle {
    #[default]
    None,
    /// A thin ring that grows and fades.
    Ring,
    /// A soft disc inside a ring that grows and fades.
    Ripple,
}

/// A click effect at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickMark {
    /// Where the click was, as fractions of the screen frame.
    pub pos: (f32, f32),
    /// 0 at the click, 1 when the effect has ended.
    pub progress: f32,
}

/// The cursor picture: straight-alpha RGBA and where its tip is.
#[derive(Debug, Clone, Copy)]
pub struct CursorSprite<'a> {
    /// Must be [`PixelFormat::Rgba8`] with straight (non-premultiplied) alpha.
    pub frame: Frame<'a>,
    /// The hotspot in px of the sprite.
    pub hotspot: (f32, f32),
}

/// The telemetry cursor and its click effects (EDT-4), drawn over the screen picture in the
/// screen's own pixel space: it follows the crop and the zoom like the content under it.
#[derive(Debug, Clone, Copy)]
pub struct CursorLayer<'a> {
    pub sprite: Option<CursorSprite<'a>>,
    /// The hotspot as fractions of the screen frame.
    pub pos: (f32, f32),
    /// Screen-frame px per sprite px (the cursor size setting, times the display density
    /// ratio).
    pub scale: f32,
    /// 0 to 1.
    pub opacity: f32,
    pub click_style: ClickStyle,
    /// At most [`MAX_CLICKS`] are drawn.
    pub clicks: &'a [ClickMark],
    /// Largest radius of a click effect in screen-frame px.
    pub effect_radius_px: f32,
    pub effect_color: [u8; 4],
}

/// Most click effects drawn at once.
pub const MAX_CLICKS: usize = 3;

/// Everything needed for one output frame.
#[derive(Debug, Clone, Copy)]
pub struct Scene<'a> {
    /// Output width and height in pixels.
    pub output: (u32, u32),
    pub background: Background<'a>,
    /// Padding as a percentage of the output width at the left and right, and of the height above
    /// and below, so the border of a screen with the frame's aspect is proportional (EDT-3: 0 to 20).
    pub padding_pct: f32,
    /// Corner radius of the screen (and camera tiles) in output pixels (EDT-3: 0 to 24).
    pub radius_px: f32,
    pub shadow: bool,
    pub screen: Option<ScreenLayer<'a>>,
    pub camera: Option<CameraLayer<'a>>,
    pub layout: Layout,
    /// Straight-alpha RGBA drawn over the screen picture, in the screen's own picture space
    /// (follows the crop). Typically the annotation layer rendered on a transparent base.
    pub annotations: Option<Frame<'a>>,
    /// The telemetry cursor (EDT-4); needs a screen layer.
    pub cursor: Option<CursorLayer<'a>>,
    /// The caption line to burn in (AI-1), drawn last, over everything else.
    pub captions: Option<CaptionLayer<'a>>,
}

impl<'a> Scene<'a> {
    /// A plain scene: no background, no padding, screen only.
    pub fn new(output: (u32, u32), screen: Frame<'a>) -> Self {
        Scene {
            output,
            background: Background::None,
            padding_pct: 0.0,
            radius_px: 0.0,
            shadow: false,
            screen: Some(ScreenLayer::new(screen)),
            camera: None,
            layout: Layout::ScreenOnly,
            annotations: None,
            cursor: None,
            captions: None,
        }
    }
}
