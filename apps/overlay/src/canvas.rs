//! A tiny software canvas over a `0RGB` `u32` buffer (softbuffer's format): alpha fills,
//! anti-aliased rounded rects and 1 px lines. Pure CPU, no windowing, unit tested.

/// Integer rectangle in canvas px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> IRect {
        IRect { x, y, w, h }
    }

    pub fn grow(self, d: i32) -> IRect {
        IRect::new(self.x - d, self.y - d, self.w + 2 * d, self.h + 2 * d)
    }
}

/// Pixel buffer view. Colors are `0x00RRGGBB`; alpha is 0..=255.
pub struct Canvas<'a> {
    pub buf: &'a mut [u32],
    pub w: i32,
    pub h: i32,
}

/// Blends `src` over `dst` with `alpha` (0..=255).
pub fn blend(dst: u32, src: u32, alpha: u32) -> u32 {
    let a = alpha.min(255);
    if a == 255 {
        return src & 0x00FF_FFFF;
    }
    if a == 0 {
        return dst & 0x00FF_FFFF;
    }
    let mix = |s: u32, d: u32| (s * a + d * (255 - a) + 127) / 255;
    let r = mix((src >> 16) & 255, (dst >> 16) & 255);
    let g = mix((src >> 8) & 255, (dst >> 8) & 255);
    let b = mix(src & 255, dst & 255);
    (r << 16) | (g << 8) | b
}

/// Linear mix of two colors, `t` in 0..=255 (0 gives `a`).
pub fn lerp_color(a: u32, b: u32, t: u32) -> u32 {
    blend(a, b, t)
}

impl<'a> Canvas<'a> {
    /// `buf.len()` must be `w * h`; returns `None` otherwise.
    pub fn new(buf: &'a mut [u32], w: u32, h: u32) -> Option<Canvas<'a>> {
        let (w, h) = (i32::try_from(w).ok()?, i32::try_from(h).ok()?);
        (buf.len() == (w as usize) * (h as usize)).then_some(Canvas { buf, w, h })
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.w && y < self.h)
            .then(|| (y as usize) * (self.w as usize) + x as usize)
    }

    pub fn get(&self, x: i32, y: i32) -> Option<u32> {
        self.index(x, y).map(|i| self.buf[i])
    }

    pub fn put(&mut self, x: i32, y: i32, color: u32, alpha: u32) {
        if let Some(i) = self.index(x, y) {
            self.buf[i] = blend(self.buf[i], color, alpha);
        }
    }

    /// Fills `[x, x+w) x [y, y+h)` clipped to the canvas.
    pub fn fill_rect(&mut self, r: IRect, color: u32, alpha: u32) {
        let IRect { x, y, w, h } = r;
        let (x0, x1) = (x.max(0), (x + w).min(self.w));
        let (y0, y1) = (y.max(0), (y + h).min(self.h));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for yy in y0..y1 {
            let row = (yy as usize) * (self.w as usize);
            let slice = &mut self.buf[row + x0 as usize..row + x1 as usize];
            if alpha >= 255 {
                slice.fill(color & 0x00FF_FFFF);
            } else {
                for px in slice {
                    *px = blend(*px, color, alpha);
                }
            }
        }
    }

    /// 1 px (or `t` px) frame drawn *inside* the rect.
    pub fn stroke_rect_inside(&mut self, r: IRect, t: i32, color: u32, alpha: u32) {
        let IRect { x, y, w, h } = r;
        if w <= 0 || h <= 0 {
            return;
        }
        let t = t.min(w / 2 + 1).min(h / 2 + 1).max(1);
        self.fill_rect(IRect::new(x, y, w, t), color, alpha);
        self.fill_rect(IRect::new(x, y + h - t, w, t), color, alpha);
        self.fill_rect(IRect::new(x, y + t, t, h - 2 * t), color, alpha);
        self.fill_rect(IRect::new(x + w - t, y + t, t, h - 2 * t), color, alpha);
    }

    /// Frame of thickness `t` drawn just *outside* the rect.
    pub fn stroke_rect_outside(&mut self, r: IRect, t: i32, color: u32, alpha: u32) {
        self.stroke_rect_inside(r.grow(t), t, color, alpha);
    }

    /// Anti-aliased rounded rect. `shade` maps the row offset (0..h) to the fill color, which
    /// lets keycaps use a vertical gradient; pass a constant closure for a flat fill.
    pub fn fill_rrect_with(
        &mut self,
        r: IRect,
        radius: f32,
        alpha: u32,
        shade: impl Fn(i32) -> u32,
    ) {
        let IRect { x, y, w, h } = r;
        if w <= 0 || h <= 0 {
            return;
        }
        let rad = radius.min(w as f32 / 2.0).min(h as f32 / 2.0).max(0.0);
        let (hx, hy) = (w as f32 / 2.0, h as f32 / 2.0);
        for yy in y.max(0)..(y + h).min(self.h) {
            let color = shade(yy - y);
            for xx in x.max(0)..(x + w).min(self.w) {
                let (px, py) = (
                    xx as f32 + 0.5 - (x as f32 + hx),
                    yy as f32 + 0.5 - (y as f32 + hy),
                );
                let (qx, qy) = (px.abs() - (hx - rad), py.abs() - (hy - rad));
                let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rad;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (cov * alpha as f32).round() as u32;
                    self.put(xx, yy, color, a);
                }
            }
        }
    }

    pub fn fill_rrect(&mut self, r: IRect, radius: f32, color: u32, alpha: u32) {
        self.fill_rrect_with(r, radius, alpha, |_| color);
    }

    /// One crop bracket (the logo's corner): an L of thickness `stroke` filling the outer edges of
    /// the square `r`, with a rounded outer corner of `radius`. `(right, bottom)` says which
    /// corner of the square it is the corner of.
    pub fn fill_bracket(
        &mut self,
        r: IRect,
        (right, bottom): (bool, bool),
        stroke: i32,
        radius: f32,
        color: u32,
    ) {
        let IRect { x, y, w, h } = r;
        if w <= 0 || h <= 0 || stroke <= 0 {
            return;
        }
        let rad = radius.min(w as f32 / 2.0).min(h as f32 / 2.0).max(0.0);
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                // Local coordinates measured from the bracket's own corner.
                let u = if right { x + w - 1 - xx } else { xx - x };
                let v = if bottom { y + h - 1 - yy } else { yy - y };
                let (fu, fv) = (u as f32 + 0.5, v as f32 + 0.5);
                // Outer shape: the square with a rounded corner at (0, 0) and rounded arm ends.
                let (hx, hy) = (w as f32 / 2.0, h as f32 / 2.0);
                let (qx, qy) = ((fu - hx).abs() - (hx - rad), (fv - hy).abs() - (hy - rad));
                let d_out = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rad;
                // The notch: everything farther than `stroke` from both outer edges.
                let (nx, ny) = (fu - stroke as f32, fv - stroke as f32);
                let in_notch = nx.min(ny);
                let cov = (0.5 - d_out).clamp(0.0, 1.0) * (0.5 - in_notch).clamp(0.0, 1.0);
                // `in_notch` is positive inside the notch (both coordinates past the stroke):
                // coverage must be zero there, so invert.
                let cov = if nx > 0.0 && ny > 0.0 {
                    cov * (0.5 - nx.min(ny)).clamp(0.0, 1.0)
                } else {
                    cov
                };
                if cov > 0.0 {
                    self.put(xx, yy, color, (cov * 255.0).round() as u32);
                }
            }
        }
    }

    /// Horizontal line of `len` px, `alpha`-blended.
    pub fn hline(&mut self, x: i32, y: i32, len: i32, color: u32, alpha: u32) {
        self.fill_rect(IRect::new(x, y, len, 1), color, alpha);
    }

    pub fn vline(&mut self, x: i32, y: i32, len: i32, color: u32, alpha: u32) {
        self.fill_rect(IRect::new(x, y, 1, len), color, alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(buf: &mut Vec<u32>, w: u32, h: u32) -> Canvas<'_> {
        *buf = vec![0; (w * h) as usize];
        Canvas::new(buf, w, h).unwrap()
    }

    #[test]
    fn blend_endpoints_and_midpoint() {
        assert_eq!(blend(0x102030, 0xFFFFFF, 0), 0x102030);
        assert_eq!(blend(0x102030, 0xAABBCC, 255), 0xAABBCC);
        assert_eq!(blend(0x000000, 0xFFFFFF, 128), 0x808080);
    }

    #[test]
    fn fill_rect_is_clipped() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 4, 4);
        c.fill_rect(IRect::new(-2, -2, 4, 4), 0xFFFFFF, 255);
        assert_eq!(c.get(1, 1), Some(0xFFFFFF));
        assert_eq!(c.get(2, 2), Some(0));
        assert_eq!(c.get(9, 9), None);
    }

    #[test]
    fn stroke_inside_leaves_the_interior_alone() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 10, 10);
        c.stroke_rect_inside(IRect::new(2, 2, 6, 6), 1, 0xFF0000, 255);
        assert_eq!(c.get(2, 2), Some(0xFF0000));
        assert_eq!(c.get(7, 7), Some(0xFF0000));
        assert_eq!(c.get(4, 4), Some(0));
        assert_eq!(c.get(1, 1), Some(0));
    }

    #[test]
    fn stroke_outside_surrounds_the_rect() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 10, 10);
        c.stroke_rect_outside(IRect::new(3, 3, 4, 4), 1, 0x00FF00, 255);
        assert_eq!(c.get(2, 2), Some(0x00FF00));
        assert_eq!(c.get(7, 7), Some(0x00FF00));
        assert_eq!(c.get(3, 3), Some(0));
    }

    #[test]
    fn rounded_rect_cuts_corners_but_fills_the_middle() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 20, 20);
        c.fill_rrect(IRect::new(0, 0, 20, 20), 10.0, 0xFFFFFF, 255);
        assert_eq!(c.get(10, 10), Some(0xFFFFFF));
        assert_eq!(c.get(0, 0), Some(0));
        assert_eq!(c.get(10, 0).map(|v| v > 0x808080), Some(true));
    }

    /// The crop bracket is an L: both arms, nothing in the notch, mirrored per corner.
    #[test]
    fn bracket_is_an_l_in_every_corner() {
        for (right, bottom) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut b = Vec::new();
            let mut c = canvas(&mut b, 40, 40);
            c.fill_bracket(IRect::new(4, 4, 28, 28), (right, bottom), 4, 6.0, 0xFFFFFF);
            let at = |c: &Canvas<'_>, u: i32, v: i32| {
                let x = if right { 4 + 27 - u } else { 4 + u };
                let y = if bottom { 4 + 27 - v } else { 4 + v };
                c.get(x, y).unwrap()
            };
            assert_eq!(
                at(&c, 20, 1),
                0xFFFFFF,
                "arm along the top {right} {bottom}"
            );
            assert_eq!(at(&c, 1, 20), 0xFFFFFF, "arm along the side");
            assert_eq!(at(&c, 14, 14), 0, "the notch is empty");
            assert_eq!(at(&c, 0, 0), 0, "the outer corner is rounded off");
        }
    }

    #[test]
    fn new_rejects_a_buffer_of_the_wrong_size() {
        let mut b = vec![0u32; 5];
        assert!(Canvas::new(&mut b, 2, 2).is_none());
    }
}
