//! The image strip under the editor canvas (0.3.3, board N8): which images it lists, which one
//! is current, when it is shown, what a thumbnail is called and which part of the list needs
//! thumbnails. Pure logic, no window: `ui::editor_view::image_strip` only draws it.

use std::ops::Range;

use cm_i18n::{tr, trn};
use time::OffsetDateTime;

use crate::library::{CaptureItem, Kind};

/// The per-user preference (a bool): the Images button. Shown unless the user hid it.
pub const STRIP_PREF_KEY: &str = "editor.image_strip";
/// Below this window height (logical px) the strip hides by itself.
pub const MIN_WINDOW_H: f32 = 640.;
/// Thumbnail box, gap and the strip's height (board N8).
pub const THUMB_W: f32 = 92.;
pub const THUMB_H: f32 = 56.;
pub const THUMB_GAP: f32 = 8.;
pub const STRIP_H: f32 = 76.;
/// Thumbnails requested beyond the visible ones on each side.
const OVERSCAN: usize = 3;

/// One image of the strip.
#[derive(Debug, Clone, PartialEq)]
pub struct StripItem {
    pub id: String,
    pub title: String,
    pub created: OffsetDateTime,
}

/// The ordered list the Library showed when the image was opened (same filter, search and
/// sort), reduced to the images the editor can open.
pub fn from_library(items: &[CaptureItem]) -> Vec<StripItem> {
    items
        .iter()
        .filter(|i| i.kind == Kind::Screenshot && !i.broken)
        .map(|i| StripItem {
            id: i.id.clone(),
            title: i.title.clone(),
            created: i.created,
        })
        .collect()
}

pub fn index_of(list: &[StripItem], id: &str) -> Option<usize> {
    list.iter().position(|i| i.id == id)
}

/// The image `delta` places away from `id`, or `None` at either end (no wrap) or when `id` is
/// not in the list.
pub fn neighbor<'a>(list: &'a [StripItem], id: &str, delta: isize) -> Option<&'a StripItem> {
    let at = index_of(list, id)? as isize + delta;
    usize::try_from(at).ok().and_then(|i| list.get(i))
}

/// The ids whose full image is worth loading ahead: previous and next.
pub fn preload_ids(list: &[StripItem], id: &str) -> Vec<String> {
    [-1, 1]
        .into_iter()
        .filter_map(|d| neighbor(list, id, d).map(|i| i.id.clone()))
        .collect()
}

/// Whether the strip is drawn: the user did not hide it, the list (which holds the open image)
/// has more than one image, and the window is tall enough.
pub fn strip_visible(list: &[StripItem], current: &str, window_h: f32, pref_shown: bool) -> bool {
    pref_shown && images_button_available(list, current) && window_h >= MIN_WINDOW_H
}

/// The Images button is offered when there is something to switch to.
pub fn images_button_available(list: &[StripItem], current: &str) -> bool {
    list.len() > 1 && index_of(list, current).is_some()
}

/// "Brightdesk billing, 3 annotations, 3 of 24": the option's accessible name.
pub fn option_label(title: &str, annotations: Option<usize>, pos: usize, total: usize) -> String {
    let place = tr!("{pos} of {total}", pos = pos, total = total);
    match annotations {
        Some(n) => tr!(
            "{title}, {annotations}, {place}",
            title = title,
            annotations = annotation_count(n),
            place = place
        ),
        None => tr!("{title}, {place}", title = title, place = place),
    }
}

/// "2 annotations".
pub fn annotation_count(n: usize) -> String {
    trn!("{n} annotation", "{n} annotations", n as u64)
}

/// The tooltip's second line: "Today 10:42 · 2 annotations · 5 of 24".
pub fn tooltip_meta(date: &str, annotations: Option<usize>, pos: usize, total: usize) -> String {
    let mut parts = vec![date.to_owned()];
    if let Some(n) = annotations.filter(|n| *n > 0) {
        parts.push(annotation_count(n));
    }
    parts.push(tr!("{pos} of {total}", pos = pos, total = total));
    parts.join(" \u{b7} ")
}

/// Total width of the scrolling row for `n` thumbnails.
pub fn content_width(n: usize) -> f32 {
    if n == 0 {
        0.
    } else {
        n as f32 * THUMB_W + (n - 1) as f32 * THUMB_GAP
    }
}

/// The thumbnails that need their picture: those in view (scrolled `offset` px from the start,
/// `viewport` px wide) plus a few beyond each edge. Everything else stays an empty box.
pub fn visible_range(offset: f32, viewport: f32, n: usize) -> Range<usize> {
    if n == 0 {
        return 0..0;
    }
    let step = THUMB_W + THUMB_GAP;
    let first = (offset.max(0.) / step).floor() as usize;
    let last = ((offset.max(0.) + viewport.max(0.)) / step).ceil() as usize;
    first.saturating_sub(OVERSCAN).min(n)..(last + OVERSCAN).min(n)
}

/// The scroll offset that shows thumbnail `index` fully, changing `offset` as little as
/// possible (the strip keeps the current image in view).
pub fn scroll_into_view(index: usize, offset: f32, viewport: f32, n: usize) -> f32 {
    let left = index as f32 * (THUMB_W + THUMB_GAP);
    let right = left + THUMB_W;
    let max = (content_width(n) - viewport).max(0.);
    let o = if left < offset {
        left
    } else if right > offset + viewport {
        right - viewport
    } else {
        offset
    };
    o.clamp(0., max)
}

/// Where the keyboard focus goes in the listbox: arrows move by one, clamped.
pub fn move_focus(current: usize, delta: isize, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    (current as isize + delta).clamp(0, n as isize - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn it(id: &str) -> StripItem {
        StripItem {
            id: id.into(),
            title: format!("Title {id}"),
            created: datetime!(2026-11-12 10:42 UTC),
        }
    }
    fn list(n: usize) -> Vec<StripItem> {
        (0..n).map(|i| it(&format!("shot_{i}"))).collect()
    }

    fn cap(id: &str, kind: Kind, broken: bool) -> CaptureItem {
        CaptureItem {
            id: id.into(),
            title: id.into(),
            kind,
            created: datetime!(2026-11-12 10:42 UTC),
            duration_ticks: None,
            width: None,
            height: None,
            size_bytes: 0,
            broken,
            uploads: Vec::new(),
            publishes: Vec::new(),
            imported_from: None,
        }
    }

    #[test]
    fn list_keeps_library_order_and_only_openable_screenshots() {
        let items = [
            cap("b", Kind::Screenshot, false),
            cap("rec_1", Kind::Recording, false),
            cap("a", Kind::Screenshot, false),
            cap("bad", Kind::Screenshot, true),
            cap("aud", Kind::Audio, false),
            cap("c", Kind::Screenshot, false),
        ];
        let ids: Vec<String> = from_library(&items).into_iter().map(|i| i.id).collect();
        assert_eq!(ids, ["b", "a", "c"]);
    }

    #[test]
    fn current_index_and_neighbors() {
        let l = list(4);
        assert_eq!(index_of(&l, "shot_2"), Some(2));
        assert_eq!(index_of(&l, "nope"), None);
        assert_eq!(neighbor(&l, "shot_2", 1).unwrap().id, "shot_3");
        assert_eq!(neighbor(&l, "shot_2", -1).unwrap().id, "shot_1");
        assert!(neighbor(&l, "shot_3", 1).is_none());
        assert!(neighbor(&l, "shot_0", -1).is_none());
        assert!(neighbor(&l, "nope", 1).is_none());
        assert_eq!(preload_ids(&l, "shot_0"), ["shot_1"]);
        assert_eq!(preload_ids(&l, "shot_2"), ["shot_1", "shot_3"]);
        assert!(preload_ids(&l, "nope").is_empty());
    }

    #[test]
    fn hide_rules() {
        let l = list(3);
        assert!(strip_visible(&l, "shot_1", 800., true));
        // Hidden by the Images button, by a short window, by a single image, by an unknown id.
        assert!(!strip_visible(&l, "shot_1", 800., false));
        assert!(!strip_visible(&l, "shot_1", 639., true));
        assert!(strip_visible(&l, "shot_1", 640., true));
        assert!(!strip_visible(&list(1), "shot_0", 800., true));
        assert!(!strip_visible(&l, "gone", 800., true));
        assert!(!strip_visible(&[], "x", 800., true));
        assert!(images_button_available(&l, "shot_1"));
        assert!(!images_button_available(&list(1), "shot_0"));
    }

    #[test]
    fn labels() {
        assert_eq!(
            option_label("Brightdesk billing", Some(3), 3, 24),
            "Brightdesk billing, 3 annotations, 3 of 24"
        );
        assert_eq!(
            option_label("Brightdesk billing", Some(1), 3, 24),
            "Brightdesk billing, 1 annotation, 3 of 24"
        );
        assert_eq!(option_label("A", None, 1, 2), "A, 1 of 2");
        assert_eq!(
            tooltip_meta("Today 10:42", Some(2), 5, 24),
            "Today 10:42 \u{b7} 2 annotations \u{b7} 5 of 24"
        );
        assert_eq!(
            tooltip_meta("Today 10:42", Some(0), 5, 24),
            "Today 10:42 \u{b7} 5 of 24"
        );
        assert_eq!(
            tooltip_meta("Today 10:42", None, 5, 24),
            "Today 10:42 \u{b7} 5 of 24"
        );
    }

    #[test]
    fn only_the_visible_part_loads_thumbnails() {
        assert_eq!(visible_range(0., 500., 0), 0..0);
        // 500 px shows 5 boxes, plus 3 beyond the right edge.
        assert_eq!(visible_range(0., 500., 100), 0..8);
        assert_eq!(visible_range(1000., 500., 100), 7..18);
        assert_eq!(visible_range(9500., 500., 100), 92..100);
        assert_eq!(visible_range(0., 500., 4), 0..4);
    }

    #[test]
    fn current_image_scrolls_into_view() {
        let (vw, n) = (500., 50);
        assert_eq!(scroll_into_view(2, 0., vw, n), 0.);
        // Off to the right: the box's right edge meets the viewport's.
        assert_eq!(scroll_into_view(20, 0., vw, n), 20. * 100. + 92. - 500.);
        // Off to the left: its left edge.
        assert_eq!(scroll_into_view(3, 1000., vw, n), 300.);
        // Already visible: unchanged.
        assert_eq!(scroll_into_view(12, 1000., vw, n), 1000.);
        // The last box does not scroll past the content.
        assert_eq!(scroll_into_view(49, 0., vw, n), content_width(n) - vw);
    }

    #[test]
    fn keyboard_focus_is_clamped() {
        assert_eq!(move_focus(0, -1, 5), 0);
        assert_eq!(move_focus(4, 1, 5), 4);
        assert_eq!(move_focus(2, 1, 5), 3);
        assert_eq!(move_focus(0, 0, 0), 0);
    }
}
