//! A horizontal position slider (the publish sheet's thumbnail time). The owner keeps the value
//! and the drag state and attaches the listeners; this draws the track and knob and holds the
//! pure geometry and key rules, which are unit tested.
//!
//! Mouse: press or drag on the track sets the value from the pointer's x (the track's bounds are
//! measured at paint into the cell the owner passes). Keyboard: Left and Right move by `step`,
//! Shift by `big_step`, Home and End jump to the ends ([`nudge`]).

use std::cell::Cell;
use std::rc::Rc;

use gpui::{canvas, div, prelude::*, px, Bounds, ElementId, Pixels, Role, SharedString, Stateful};

use crate::theme::Theme;
use crate::ui::paint::rgba_of;

/// Height of the hit area (the visible track is thinner).
pub const HIT_H: f32 = 28.;
const TRACK_H: f32 = 4.;
const KNOB: f32 = 14.;

/// The value (0 to 1) for a pointer at `x` over a track at `left` that is `width` wide.
pub fn fraction_at(left: f32, width: f32, x: f32) -> f32 {
    if width <= 0.0 || width.is_nan() || x.is_nan() {
        return 0.0;
    }
    ((x - left) / width).clamp(0.0, 1.0)
}

/// The value after a key, or `None` when the key is not the slider's.
pub fn nudge(value: f32, key: &str, shift: bool, step: f32, big_step: f32) -> Option<f32> {
    let by = if shift { big_step } else { step };
    let v = match key {
        "left" | "down" => value - by,
        "right" | "up" => value + by,
        "home" => 0.0,
        "end" => 1.0,
        "pageup" => value + big_step,
        "pagedown" => value - big_step,
        _ => return None,
    };
    Some(v.clamp(0.0, 1.0))
}

/// What the slider draws.
pub struct SliderLook<'a> {
    pub label: &'a str,
    /// What assistive technology reads ("1:23 of 4:05").
    pub value_text: SharedString,
    pub value: f32,
    pub enabled: bool,
    pub focused: bool,
}

/// The slider itself. `bounds` receives the track's bounds at every paint. Attach `track_focus`,
/// `on_key_down` and the mouse listeners on the result.
pub fn slider(
    t: &Theme,
    id: impl Into<ElementId>,
    look: SliderLook<'_>,
    bounds: &Rc<Cell<Bounds<Pixels>>>,
) -> Stateful<gpui::Div> {
    let value = look.value.clamp(0.0, 1.0);
    let cell = bounds.clone();
    let knob_color = if look.enabled { t.text } else { t.mist };
    div()
        .id(id)
        .role(Role::Slider)
        .aria_label(SharedString::from(look.label.to_owned()))
        .aria_value(look.value_text)
        .relative()
        .w_full()
        .h(px(HIT_H))
        .when(!look.enabled, |d| d.opacity(0.5))
        .child(
            canvas(move |b: Bounds<Pixels>, _, _| cell.set(b), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px((HIT_H - TRACK_H) / 2.))
                .h(px(TRACK_H))
                .rounded(px(TRACK_H / 2.))
                .bg(rgba_of(t.raised))
                .child(
                    div()
                        .h_full()
                        .rounded(px(TRACK_H / 2.))
                        .bg(rgba_of(knob_color))
                        .w(gpui::relative(value)),
                ),
        )
        .child(
            // The knob's centre runs from the left edge to the right edge of the track.
            div()
                .absolute()
                .top(px((HIT_H - KNOB) / 2.))
                .left(gpui::relative(value))
                .ml(px(-KNOB / 2.))
                .size(px(KNOB))
                .rounded(px(KNOB / 2.))
                .bg(rgba_of(knob_color))
                .border_2()
                .border_color(rgba_of(if look.focused { t.mist } else { t.surface })),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_maps_to_the_track() {
        assert_eq!(fraction_at(100., 200., 100.), 0.0);
        assert_eq!(fraction_at(100., 200., 200.), 0.5);
        assert_eq!(fraction_at(100., 200., 300.), 1.0);
        assert_eq!(fraction_at(100., 200., 20.), 0.0, "left of the track");
        assert_eq!(fraction_at(100., 200., 900.), 1.0, "right of the track");
        assert_eq!(fraction_at(0., 0., 5.), 0.0, "no width yet");
        assert_eq!(fraction_at(0., 100., f32::NAN), 0.0);
    }

    #[test]
    fn arrows_move_by_a_step_and_shift_by_a_big_step() {
        assert_eq!(nudge(0.5, "right", false, 0.01, 0.1), Some(0.51));
        assert_eq!(nudge(0.5, "left", true, 0.01, 0.1), Some(0.4));
        assert_eq!(nudge(0.5, "home", false, 0.01, 0.1), Some(0.0));
        assert_eq!(nudge(0.5, "end", false, 0.01, 0.1), Some(1.0));
        assert_eq!(
            nudge(0.995, "right", false, 0.01, 0.1),
            Some(1.0),
            "clamped"
        );
        assert_eq!(nudge(0.005, "left", false, 0.01, 0.1), Some(0.0));
        assert_eq!(nudge(0.5, "pagedown", false, 0.01, 0.1), Some(0.4));
        assert_eq!(nudge(0.5, "a", false, 0.01, 0.1), None);
    }
}
