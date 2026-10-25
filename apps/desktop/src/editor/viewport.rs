//! Coordinate mapping between the on-screen canvas (logical px, origin at the canvas'
//! top-left) and the original image (physical px), with fit, zoom and pan.
//!
//! The canvas shows `content`, a rect of the original: the whole image, or the crop rect
//! (ANN-1: crop is non-destructive, ops outside it stay stored). `zoom` is logical screen
//! px per image px; "100%" means one image px per device px (`1 / dpr` logical px per image
//! px), so a 2x capture at 100% shows at its natural size.

use cm_core::{Pt, Rc};

/// Smallest and largest zoom, as a multiple of device px.
pub const MIN_ZOOM_PCT: f64 = 5.0;
pub const MAX_ZOOM_PCT: f64 = 1600.0;
/// Space kept free around a fitted image, in logical px.
pub const FIT_PADDING: f64 = 24.0;
/// How much of the content must stay on screen when panning, in logical px.
const KEEP_VISIBLE: f64 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Visible rect of the original, physical px.
    pub content: Rc,
    /// Canvas size, logical px.
    pub canvas: (f64, f64),
    /// Device pixel ratio of the window.
    pub dpr: f64,
    /// While true the zoom follows the canvas size ("Fit").
    pub fit: bool,
    zoom: f64,
    /// Offset of the content's center from the canvas center, logical px.
    pub pan: (f64, f64),
    /// Canvas space taken by floating chrome (tool pill, transport pill): the image is fitted
    /// and centred in the canvas minus these insets, so nothing sits under the pills.
    pub insets: Insets,
}

/// Free space reserved at each canvas edge, logical px.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Insets {
    pub const NONE: Insets = Insets {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
}

impl Viewport {
    pub fn new(content: Rc, canvas: (f64, f64), dpr: f64) -> Self {
        let mut v = Viewport {
            content,
            canvas,
            dpr: if dpr > 0.0 { dpr } else { 1.0 },
            fit: true,
            zoom: 1.0,
            pan: (0.0, 0.0),
            insets: Insets::NONE,
        };
        v.zoom = v.fit_zoom();
        v
    }

    /// Zoom that fits the content in the canvas, never above 100%.
    pub fn fit_zoom(&self) -> f64 {
        let i = self.insets;
        let cw = (self.canvas.0 - i.left - i.right - 2.0 * FIT_PADDING).max(16.0);
        let ch = (self.canvas.1 - i.top - i.bottom - 2.0 * FIT_PADDING).max(16.0);
        let (w, h) = (self.content.w.max(1.0), self.content.h.max(1.0));
        (cw / w).min(ch / h).min(1.0 / self.dpr)
    }

    /// Current zoom, logical px per image px.
    pub fn zoom(&self) -> f64 {
        if self.fit {
            self.fit_zoom()
        } else {
            self.zoom
        }
    }

    /// Zoom as a percentage of device px (what the zoom readout shows).
    pub fn zoom_percent(&self) -> f64 {
        self.zoom() * self.dpr * 100.0
    }

    /// Sets the chrome insets and re-centres (a fitted view follows them).
    pub fn set_insets(&mut self, insets: Insets) {
        if self.insets != insets {
            self.insets = insets;
            if self.fit {
                self.pan = (0.0, 0.0);
            } else {
                self.clamp_pan();
            }
        }
    }

    /// Centre of the free area, canvas px: the canvas centre shifted by the insets.
    fn centre(&self) -> (f64, f64) {
        let i = self.insets;
        (
            (self.canvas.0 + i.left - i.right) / 2.0,
            (self.canvas.1 + i.top - i.bottom) / 2.0,
        )
    }

    pub fn set_canvas(&mut self, canvas: (f64, f64)) {
        self.canvas = canvas;
        if self.fit {
            self.pan = (0.0, 0.0);
        } else {
            self.clamp_pan();
        }
    }

    /// Shows a different rect (a crop changed, or the crop tool toggled), re-fitting.
    pub fn set_content(&mut self, content: Rc) {
        if self.content != content {
            self.content = content;
            self.fit_view();
        }
    }

    pub fn fit_view(&mut self) {
        self.fit = true;
        self.pan = (0.0, 0.0);
        self.zoom = self.fit_zoom();
    }

    fn set_zoom_percent_clamped(&mut self, pct: f64) {
        self.fit = false;
        self.zoom = pct.clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT) / 100.0 / self.dpr;
    }

    /// Top-left of the content on the canvas, logical px.
    pub fn origin(&self) -> (f64, f64) {
        let z = self.zoom();
        let (cx, cy) = self.centre();
        (
            cx + self.pan.0 - self.content.w * z / 2.0,
            cy + self.pan.1 - self.content.h * z / 2.0,
        )
    }

    /// The content's rect on the canvas (x, y, w, h), logical px.
    pub fn screen_rect(&self) -> (f64, f64, f64, f64) {
        let (ox, oy) = self.origin();
        let z = self.zoom();
        (ox, oy, self.content.w * z, self.content.h * z)
    }

    /// Canvas point to original-image point.
    pub fn to_image(&self, sx: f64, sy: f64) -> Pt {
        let (ox, oy) = self.origin();
        let z = self.zoom();
        Pt::new(
            self.content.x + (sx - ox) / z,
            self.content.y + (sy - oy) / z,
        )
    }

    /// Original-image point to canvas point.
    pub fn to_screen(&self, p: Pt) -> (f64, f64) {
        let (ox, oy) = self.origin();
        let z = self.zoom();
        (
            ox + (p.x - self.content.x) * z,
            oy + (p.y - self.content.y) * z,
        )
    }

    /// Image-space length of `screen_px` logical px (hit tolerances, handle sizes).
    pub fn image_len(&self, screen_px: f64) -> f64 {
        screen_px / self.zoom()
    }

    /// Zooms to `pct` keeping the image point under the canvas point `(ax, ay)` fixed.
    pub fn zoom_to_at(&mut self, pct: f64, ax: f64, ay: f64) {
        let anchor = self.to_image(ax, ay);
        self.set_zoom_percent_clamped(pct);
        // Solve pan so that `anchor` lands on (ax, ay) again.
        let z = self.zoom();
        let (mx, my) = self.centre();
        let cx = mx - self.content.w * z / 2.0;
        let cy = my - self.content.h * z / 2.0;
        self.pan = (
            ax - cx - (anchor.x - self.content.x) * z,
            ay - cy - (anchor.y - self.content.y) * z,
        );
        self.clamp_pan();
    }

    /// Multiplies the zoom by `factor` around `(ax, ay)`.
    pub fn zoom_by_at(&mut self, factor: f64, ax: f64, ay: f64) {
        let pct = self.zoom_percent() * factor;
        self.zoom_to_at(pct, ax, ay);
    }

    /// Keyboard zoom steps (Cmd +/-): around the canvas center.
    pub fn zoom_step(&mut self, zoom_in: bool) {
        let f = if zoom_in { 1.25 } else { 0.8 };
        let (cx, cy) = self.centre();
        self.zoom_by_at(f, cx, cy);
    }

    /// 100% (one image px per device px), centered.
    pub fn actual_size(&mut self) {
        self.pan = (0.0, 0.0);
        self.set_zoom_percent_clamped(100.0);
    }

    pub fn pan_by(&mut self, dx: f64, dy: f64) {
        if self.fit {
            // Panning leaves "fit": freeze the zoom first.
            self.zoom = self.fit_zoom();
            self.fit = false;
        }
        self.pan.0 += dx;
        self.pan.1 += dy;
        self.clamp_pan();
    }

    fn clamp_pan(&mut self) {
        let z = self.zoom();
        let (w, h) = (self.content.w * z, self.content.h * z);
        let (mx, my) = self.centre();
        let max_x = (self.canvas.0 / 2.0 + w / 2.0 - KEEP_VISIBLE
            + (mx - self.canvas.0 / 2.0).abs())
        .max(0.0);
        let max_y = (self.canvas.1 / 2.0 + h / 2.0 - KEEP_VISIBLE
            + (my - self.canvas.1 / 2.0).abs())
        .max(0.0);
        self.pan.0 = self.pan.0.clamp(-max_x, max_x);
        self.pan.1 = self.pan.1.clamp(-max_y, max_y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> Viewport {
        Viewport::new(Rc::new(0.0, 0.0, 3024.0, 1964.0), (1000.0, 700.0), 2.0)
    }

    /// ANN-1.AC1 (coordinates): a canvas point maps to physical image px and back.
    #[test]
    fn round_trips_between_canvas_and_image() {
        let mut v = vp();
        for step in 0..3 {
            let p = Pt::new(120.0, 300.0);
            let (sx, sy) = v.to_screen(p);
            let back = v.to_image(sx, sy);
            assert!((back.x - 120.0).abs() < 1e-9 && (back.y - 300.0).abs() < 1e-9);
            match step {
                0 => v.zoom_by_at(1.7, 300.0, 200.0),
                _ => v.pan_by(37.0, -12.0),
            }
        }
    }

    #[test]
    fn fit_never_upscales_past_100_percent_and_centers() {
        let v = Viewport::new(Rc::new(0.0, 0.0, 100.0, 50.0), (1000.0, 700.0), 2.0);
        assert!((v.zoom_percent() - 100.0).abs() < 1e-9);
        let (x, y, w, h) = v.screen_rect();
        assert!((x + w / 2.0 - 500.0).abs() < 1e-9 && (y + h / 2.0 - 350.0).abs() < 1e-9);
        let big = vp();
        let (_, _, w, h) = big.screen_rect();
        assert!(w <= 1000.0 - 2.0 * FIT_PADDING + 1e-9 && h <= 700.0 - 2.0 * FIT_PADDING + 1e-9);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed() {
        let mut v = vp();
        let anchor = (640.0, 410.0);
        let before = v.to_image(anchor.0, anchor.1);
        v.zoom_by_at(2.0, anchor.0, anchor.1);
        assert!(!v.fit);
        let after = v.to_image(anchor.0, anchor.1);
        assert!((before.x - after.x).abs() < 1e-6 && (before.y - after.y).abs() < 1e-6);
    }

    #[test]
    fn zoom_is_clamped_and_actual_size_is_one_image_px_per_device_px() {
        let mut v = vp();
        v.zoom_to_at(1.0e9, 10.0, 10.0);
        assert!((v.zoom_percent() - MAX_ZOOM_PCT).abs() < 1e-6);
        v.zoom_to_at(0.0, 10.0, 10.0);
        assert!((v.zoom_percent() - MIN_ZOOM_PCT).abs() < 1e-6);
        v.actual_size();
        assert!(
            (v.zoom() - 0.5).abs() < 1e-9,
            "2x display: 0.5 logical px per image px"
        );
    }

    #[test]
    fn pan_cannot_lose_the_image() {
        let mut v = vp();
        v.actual_size();
        v.pan_by(1.0e7, 1.0e7);
        let (x, y, _, _) = v.screen_rect();
        assert!(x < 1000.0 && y < 700.0, "image still intersects the canvas");
        v.fit_view();
        assert_eq!(v.pan, (0.0, 0.0));
    }

    /// ANN-1.AC4 (view): with a crop the canvas shows only the crop rect, and coordinates
    /// stay in original px (ops are never re-based).
    #[test]
    fn cropped_content_keeps_original_coordinates() {
        let v = Viewport::new(Rc::new(100.0, 50.0, 400.0, 300.0), (800.0, 600.0), 1.0);
        let (x, y, ..) = v.screen_rect();
        let p = v.to_image(x, y);
        assert!((p.x - 100.0).abs() < 1e-9 && (p.y - 50.0).abs() < 1e-9);
    }

    /// Redesign: the floating pills reserve canvas space; the fitted image is centred in the
    /// free area and never overlaps either pill (insets as the editor view uses them).
    #[test]
    fn fit_respects_chrome_insets() {
        let insets = Insets {
            left: 90.0,
            bottom: 90.0,
            top: 0.0,
            right: 0.0,
        };
        for canvas in [(1000.0, 700.0), (640.0, 480.0), (1600.0, 900.0)] {
            let mut v = Viewport::new(Rc::new(0.0, 0.0, 6000.0, 4000.0), canvas, 2.0);
            v.set_insets(insets);
            v.fit_view();
            let (x, y, w, h) = v.screen_rect();
            assert!(x >= insets.left - 1e-9, "left {x}");
            assert!(y + h <= canvas.1 - insets.bottom + 1e-9, "bottom {}", y + h);
            assert!(x + w <= canvas.0 - insets.right + 1e-9);
            assert!(y >= insets.top - 1e-9);
            // Fit still centres in the free area.
            let free_cx = (canvas.0 + insets.left - insets.right) / 2.0;
            assert!((x + w / 2.0 - free_cx).abs() < 1e-9);
            // Coordinates still round-trip with insets.
            let p = Pt::new(1234.0, 567.0);
            let (sx, sy) = v.to_screen(p);
            let back = v.to_image(sx, sy);
            assert!((back.x - p.x).abs() < 1e-9 && (back.y - p.y).abs() < 1e-9);
        }
    }

    #[test]
    fn zero_insets_change_nothing() {
        let mut a = vp();
        let b = vp();
        a.set_insets(Insets::NONE);
        assert_eq!(a, b);
    }

    #[test]
    fn zoom_around_anchor_with_insets_keeps_the_point() {
        let mut v = vp();
        v.set_insets(Insets {
            left: 90.0,
            bottom: 90.0,
            ..Insets::NONE
        });
        let before = v.to_image(400.0, 300.0);
        v.zoom_to_at(50.0, 400.0, 300.0);
        let after = v.to_image(400.0, 300.0);
        assert!((before.x - after.x).abs() < 1e-6 && (before.y - after.y).abs() < 1e-6);
    }
}
