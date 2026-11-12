//! The view bar's logic (0.3.3, board N7): the zoom menu and the dimming of Undo and Redo.
//! Pure, no window.

use cm_i18n::tr;

/// What a zoom menu entry does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomChoice {
    Fit,
    /// A fixed percentage of device pixels (100 is "Actual size").
    Percent(u32),
    /// The current zoom when it is none of the others: shown checked, picking it changes nothing.
    Custom(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZoomEntry {
    pub choice: ZoomChoice,
    pub label: String,
    pub checked: bool,
}

/// The fixed entries after "Fit to window", in menu order.
pub const FIXED_PERCENTS: [u32; 2] = [100, 200];

/// The zoom value button: "68%".
pub fn zoom_label(percent: f64) -> String {
    format!("{percent:.0}%")
}

/// The zoom menu for the current zoom: Fit to window, Actual size (100%), 200%, and a checked
/// "Custom: n%" when the zoom is none of those. `fit` is true while the zoom follows the window.
pub fn zoom_entries(percent: f64, fit: bool) -> Vec<ZoomEntry> {
    let on = |p: u32| !fit && (percent - f64::from(p)).abs() < 0.5;
    let mut v = vec![ZoomEntry {
        choice: ZoomChoice::Fit,
        label: tr!("Fit to window"),
        checked: fit,
    }];
    for p in FIXED_PERCENTS {
        v.push(ZoomEntry {
            choice: ZoomChoice::Percent(p),
            label: if p == 100 {
                tr!("Actual size (100%)")
            } else {
                format!("{p}%")
            },
            checked: on(p),
        });
    }
    if !fit && !FIXED_PERCENTS.iter().any(|p| on(*p)) {
        let n = percent.round() as u32;
        v.push(ZoomEntry {
            choice: ZoomChoice::Custom(n),
            label: tr!("Custom: {n}%", n = n),
            checked: true,
        });
    }
    v
}

/// Undo and Redo dim to 35% when there is nothing to do (board N7).
pub fn history_opacity(enabled: bool) -> f32 {
    if enabled {
        1.0
    } else {
        0.35
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(v: &[ZoomEntry]) -> Vec<ZoomChoice> {
        v.iter().filter(|e| e.checked).map(|e| e.choice).collect()
    }

    #[test]
    fn fit_checks_only_fit_whatever_the_percent() {
        let v = zoom_entries(100.0, true);
        assert_eq!(v.len(), 3);
        assert_eq!(checked(&v), [ZoomChoice::Fit]);
        assert_eq!(v[0].label, "Fit to window");
        assert_eq!(v[1].label, "Actual size (100%)");
        assert_eq!(v[2].label, "200%");
    }

    #[test]
    fn fixed_zooms_are_checked() {
        assert_eq!(
            checked(&zoom_entries(100.0, false)),
            [ZoomChoice::Percent(100)]
        );
        assert_eq!(
            checked(&zoom_entries(200.2, false)),
            [ZoomChoice::Percent(200)]
        );
        assert_eq!(zoom_entries(100.0, false).len(), 3);
    }

    #[test]
    fn other_zooms_add_a_checked_custom_entry() {
        let v = zoom_entries(68.4, false);
        assert_eq!(v.len(), 4);
        assert_eq!(v[3].choice, ZoomChoice::Custom(68));
        assert_eq!(v[3].label, "Custom: 68%");
        assert_eq!(checked(&v), [ZoomChoice::Custom(68)]);
    }

    #[test]
    fn labels_and_dimming() {
        assert_eq!(zoom_label(67.6), "68%");
        assert_eq!(history_opacity(true), 1.0);
        assert_eq!(history_opacity(false), 0.35);
    }
}
