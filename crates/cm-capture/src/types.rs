use cm_core::{PhysicalRect, Rect, ScaleFactor};
use serde::{Deserialize, Serialize};

/// A display, as reported by `list_targets`. `bounds` are physical px in the global desktop space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub bounds: PhysicalRect,
    pub scale: ScaleFactor,
    pub is_primary: bool,
    /// The laptop's own panel (macOS `CGDisplayIsBuiltin`). `false` on Windows for now and when
    /// the OS does not say. Absent in older JSON, which parses as `false`.
    #[serde(default)]
    pub is_builtin: bool,
}

/// A top-level window, as reported by `list_targets`.
///
/// `bounds` are physical px expressed in the frame of the display named by `display_id`:
/// `bounds.origin - display.bounds.origin` is the window's position in that display's own
/// pixels. On a mixed-DPI desktop the global physical spaces of two displays can overlap, so a
/// client must match a window to a display by `display_id`, never by rectangle intersection
/// (R1-2). On a single-scale desktop this is the plain global desktop space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app: String,
    pub pid: u32,
    pub bounds: PhysicalRect,
    pub scale: ScaleFactor,
    /// The display holding most of the window, if known.
    pub display_id: Option<u32>,
    pub minimized: bool,
}

/// Result of `list_targets`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Targets {
    pub displays: Vec<DisplayInfo>,
    /// Front-to-back order when the OS provides it.
    pub windows: Vec<WindowInfo>,
}

/// A rectangle on one display, relative to that display's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegionRect {
    /// Logical px; converted with the display's scale (edges rounded, so adjacent rects tile).
    Logical(Rect),
    /// Physical px; used as is. This is what the overlay's frozen frame and the MCP `rect`
    /// (CAP-1.AC2) use.
    Physical(PhysicalRect),
}

/// What to capture (CAP-1). Matches the `capture_screenshot` MCP `target` shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureTarget {
    Display { display_id: u32 },
    Window { window_id: u32 },
    Region { display_id: u32, rect: RegionRect },
}

/// The kind of capture that produced a [`crate::Capture`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Display,
    Window,
    Region,
}

/// What the capture backend of this desktop can do, reported by `list_targets` as
/// `capabilities` so a CLI, an MCP client or an agent knows which targets exist before it asks.
/// All true except `pick` on a desktop where every target can be named by id (macOS, Windows,
/// X11). On a Wayland session the compositor does not list windows and shows its own picker:
/// `list_windows` and `capture_window` are false and `pick` is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// `list_targets` returns the windows (`windows` is empty when false).
    pub list_windows: bool,
    /// A window can be captured by its id.
    pub capture_window: bool,
    /// A region of a display can be captured by a rectangle.
    pub capture_region: bool,
    /// The desktop has its own interactive picker for a window, a region or a screen (the
    /// `pick` target of `capture_screenshot`). It needs a person at the screen to choose.
    pub pick: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            list_windows: true,
            capture_window: true,
            capture_region: true,
            pick: false,
        }
    }
}
