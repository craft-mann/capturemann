//! Icons for the bars (DESIGN §3, `design/icons/`): the app's Lucide SVGs are rasterised once per
//! size to an alpha mask with `usvg` + `resvg` + `tiny-skia` (no text, no raster images) and tinted
//! with the label colour when drawn. Masks are cached per (icon, size).

use crate::canvas::Canvas;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Region,
    Window,
    Display,
    LongShot,
    ClickGuide,
    AutoScroll,
    HandScroll,
    Vertical,
    Horizontal,
    Enter,
}

impl Icon {
    pub const ALL: [Icon; 10] = [
        Icon::Region,
        Icon::Window,
        Icon::Display,
        Icon::LongShot,
        Icon::ClickGuide,
        Icon::AutoScroll,
        Icon::HandScroll,
        Icon::Vertical,
        Icon::Horizontal,
        Icon::Enter,
    ];

    fn svg(self) -> &'static str {
        match self {
            Icon::Region => include_str!("../../desktop/assets/icons/cm-capture-region.svg"),
            Icon::Window => include_str!("../../desktop/assets/icons/app-window.svg"),
            Icon::Display => include_str!("../../desktop/assets/icons/monitor.svg"),
            // Not in the design kit as a file; see the comment inside the SVG.
            Icon::LongShot => include_str!("../assets/icons/cm-long-shot.svg"),
            Icon::ClickGuide => include_str!("../../desktop/assets/icons/mouse-pointer-click.svg"),
            Icon::AutoScroll => include_str!("../../desktop/assets/icons/chevrons-down.svg"),
            Icon::HandScroll => include_str!("../../desktop/assets/icons/hand.svg"),
            Icon::Vertical => include_str!("../../desktop/assets/icons/arrow-up-down.svg"),
            Icon::Horizontal => include_str!("../../desktop/assets/icons/arrow-left-right.svg"),
            Icon::Enter => include_str!("../assets/icons/corner-down-left.svg"),
        }
    }
}

/// A square alpha mask, `size` x `size`, row-major.
pub struct Mask {
    pub size: u32,
    pub alpha: Vec<u8>,
}

/// Rasterises an SVG (stroke `currentColor`) to an alpha mask of `size` px.
pub fn rasterise(svg: &str, size: u32) -> Option<Mask> {
    let svg = svg.replace("currentColor", "#ffffff");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pm = tiny_skia::Pixmap::new(size, size)?;
    let k = size as f32 / tree.size().width().max(1.0);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(k, k),
        &mut pm.as_mut(),
    );
    // Premultiplied RGBA: for white ink the alpha channel is the coverage.
    let alpha = pm.data().as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    Some(Mask { size, alpha })
}

type Cache = Mutex<HashMap<(Icon, u32), Option<Arc<Mask>>>>;

/// The cached mask of an icon at a size.
pub fn mask(icon: Icon, size: u32) -> Option<Arc<Mask>> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().ok()?;
    c.entry((icon, size))
        .or_insert_with(|| rasterise(icon.svg(), size).map(Arc::new))
        .clone()
}

/// Draws an icon with its top-left at (`x`, `y`), `size` px square, in `color`.
pub fn draw(canvas: &mut Canvas<'_>, icon: Icon, x: i32, y: i32, size: i32, color: u32) {
    let Some(m) = mask(icon, size.max(1) as u32) else {
        return;
    };
    for row in 0..m.size as i32 {
        for col in 0..m.size as i32 {
            let a = m.alpha[(row as u32 * m.size + col as u32) as usize];
            if a > 0 {
                canvas.put(x + col, y + row, color, a as u32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_rasterises_to_a_non_empty_mask_inside_its_box() {
        for icon in Icon::ALL {
            let m = mask(icon, 16).unwrap_or_else(|| panic!("{icon:?}"));
            assert_eq!(m.alpha.len(), 256);
            let ink: u32 = m.alpha.iter().map(|a| *a as u32).sum();
            assert!(ink > 600, "{icon:?} nearly empty: {ink}");
            // Cached: the same allocation comes back.
            assert!(Arc::ptr_eq(&m, &mask(icon, 16).unwrap()));
        }
    }
}
