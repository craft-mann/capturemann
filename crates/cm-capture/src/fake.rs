//! Deterministic in-memory backend for tests (no display, no permissions).
//!
//! Every display is filled with [`pattern_pixel`], a function of the display-local physical
//! coordinates, so a crop can be verified pixel by pixel (CAP-1.AC2 golden test). A window
//! frame is the pattern of its display sampled at the window's offset ("1:1 mapping").

use std::sync::{Mutex, PoisonError};

use cm_core::{CmError, Permission, PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};

use crate::{CaptureBackend, DisplayInfo, Frame, WindowInfo};

/// The golden pattern: red = x, green = y (low bytes), blue = high bits of both, opaque.
pub fn pattern_pixel(x: u32, y: u32) -> [u8; 4] {
    [
        (x & 0xff) as u8,
        (y & 0xff) as u8,
        (((x >> 8) & 0x0f) | (((y >> 8) & 0x0f) << 4)) as u8,
        255,
    ]
}

fn pattern_frame(origin: (u32, u32), w: u32, h: u32) -> Frame {
    let mut v = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            v.extend_from_slice(&pattern_pixel(origin.0 + x, origin.1 + y));
        }
    }
    // Length is w*h*4 by construction.
    Frame::new(w, h, v).unwrap_or_else(|_| unreachable!())
}

/// A scripted backend. Build with [`FakeBackend::new`] and the `with_*` methods.
#[derive(Debug, Default)]
pub struct FakeBackend {
    displays: Vec<DisplayInfo>,
    windows: Vec<WindowInfo>,
    permission: bool,
    supports_exclusion: bool,
    calls: Mutex<Vec<String>>,
}

impl FakeBackend {
    /// Empty desktop with screen permission granted.
    pub fn new() -> Self {
        FakeBackend {
            permission: true,
            ..Default::default()
        }
    }

    /// Adds a display at global physical `(x, y)`; its logical size is `physical / scale`.
    pub fn with_display(mut self, id: u32, x: i32, y: i32, w: u32, h: u32, scale: f64) -> Self {
        self.displays.push(DisplayInfo {
            id,
            name: format!("Fake display {id}"),
            bounds: PhysicalRect {
                origin: PhysicalPoint { x, y },
                size: PhysicalSize {
                    width: w,
                    height: h,
                },
            },
            scale: ScaleFactor::new(scale).unwrap_or(ScaleFactor::ONE),
            is_primary: self.displays.is_empty(),
            is_builtin: false,
        });
        self
    }

    /// Marks display `id` as the laptop's built-in panel (default: none is).
    pub fn with_builtin(mut self, id: u32) -> Self {
        for d in self.displays.iter_mut().filter(|d| d.id == id) {
            d.is_builtin = true;
        }
        self
    }

    /// Adds a window at global physical `(x, y)` on `display_id` (should lie inside it).
    #[allow(clippy::too_many_arguments)]
    pub fn with_window(
        mut self,
        id: u32,
        title: &str,
        app: &str,
        pid: u32,
        display_id: u32,
        bounds: (i32, i32, u32, u32),
        minimized: bool,
    ) -> Self {
        let scale = self
            .displays
            .iter()
            .find(|d| d.id == display_id)
            .map_or(ScaleFactor::ONE, |d| d.scale);
        self.windows.push(WindowInfo {
            id,
            title: title.into(),
            app: app.into(),
            pid,
            bounds: PhysicalRect {
                origin: PhysicalPoint {
                    x: bounds.0,
                    y: bounds.1,
                },
                size: PhysicalSize {
                    width: bounds.2,
                    height: bounds.3,
                },
            },
            scale,
            display_id: Some(display_id),
            minimized,
        });
        self
    }

    /// Makes `capture_display_excluding` work (like macOS 14+ ScreenCaptureKit), logging
    /// `capture_display_excluding:<display>:<pid>`.
    pub fn with_process_exclusion(mut self) -> Self {
        self.supports_exclusion = true;
        self
    }

    /// Simulates a missing macOS Screen Recording permission.
    pub fn without_permission(mut self) -> Self {
        self.permission = false;
        self
    }

    /// Names of the capture calls made so far, e.g. `"capture_display:1"`.
    pub fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn log(&self, s: String) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(s);
    }
}

impl CaptureBackend for FakeBackend {
    fn check_permission(&self) -> Result<(), CmError> {
        if self.permission {
            Ok(())
        } else {
            Err(CmError::permission_missing(
                Permission::Screen,
                "Screen Recording permission is not granted",
            ))
        }
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
        self.check_permission()?;
        Ok(self.displays.clone())
    }

    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        self.check_permission()?;
        Ok(self.windows.clone())
    }

    fn capture_display(&self, display_id: u32) -> Result<Frame, CmError> {
        self.check_permission()?;
        self.log(format!("capture_display:{display_id}"));
        let d = self
            .displays
            .iter()
            .find(|d| d.id == display_id)
            .ok_or_else(|| CmError::not_found(format!("Display {display_id} not found")))?;
        Ok(pattern_frame(
            (0, 0),
            d.bounds.size.width,
            d.bounds.size.height,
        ))
    }

    fn capture_display_excluding(
        &self,
        display_id: u32,
        size: PhysicalSize,
        exclude_pid: u32,
    ) -> Result<Option<Frame>, CmError> {
        self.capture_display_excluding_except(display_id, size, exclude_pid, &[])
    }

    fn capture_display_excluding_except(
        &self,
        display_id: u32,
        _size: PhysicalSize,
        exclude_pid: u32,
        keep_window_ids: &[u32],
    ) -> Result<Option<Frame>, CmError> {
        if !self.supports_exclusion {
            return Ok(None);
        }
        self.check_permission()?;
        if keep_window_ids.is_empty() {
            self.log(format!(
                "capture_display_excluding:{display_id}:{exclude_pid}"
            ));
        } else {
            let ids: Vec<String> = keep_window_ids.iter().map(u32::to_string).collect();
            self.log(format!(
                "capture_display_excluding:{display_id}:{exclude_pid}:keep={}",
                ids.join(",")
            ));
        }
        let d = self
            .displays
            .iter()
            .find(|d| d.id == display_id)
            .ok_or_else(|| CmError::not_found(format!("Display {display_id} not found")))?;
        Ok(Some(pattern_frame(
            (0, 0),
            d.bounds.size.width,
            d.bounds.size.height,
        )))
    }

    fn capture_window(&self, window_id: u32) -> Result<Frame, CmError> {
        self.check_permission()?;
        self.log(format!("capture_window:{window_id}"));
        let w = self
            .windows
            .iter()
            .find(|w| w.id == window_id)
            .ok_or_else(|| CmError::not_found(format!("Window {window_id} not found")))?;
        let d = self
            .displays
            .iter()
            .find(|d| Some(d.id) == w.display_id)
            .ok_or_else(|| CmError::not_found("Window's display not found"))?;
        let ox = u32::try_from(w.bounds.origin.x - d.bounds.origin.x).unwrap_or(0);
        let oy = u32::try_from(w.bounds.origin.y - d.bounds.origin.y).unwrap_or(0);
        Ok(pattern_frame(
            (ox, oy),
            w.bounds.size.width,
            w.bounds.size.height,
        ))
    }
}
