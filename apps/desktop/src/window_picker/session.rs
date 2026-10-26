//! What the picker needs from the outside world, as pure functions: turning the daemon's
//! `list_targets` JSON into the picker's windows and displays (plus where each window is on the
//! real screen, for the outline), the last-used memory in `settings.json`, and the colour and
//! letter of an app's tile on the rail and the cards.
//!
//! The GPUI side (`ui::window_picker`) opens the palette and calls these.

use std::collections::HashMap;

use cm_capture::Targets;
use serde_json::{Map, Value};

use super::{LastUsed, PickerDisplay, PickerWindow};
use crate::recording::geometry::Rect;

/// Where the memory of the last-picked windows lives. `app.*` is app-only: an agent can neither
/// read it as a setting it may change nor write it.
pub const KEY_LAST_USED: &str = "app.window_picker.last_used";

/// The daemon refuses a setting value over 16 KiB; stay well under it.
const MAX_STORED_BYTES: usize = 12 * 1024;

/// A window's place on the real screen: global points, origin top-left of the primary display
/// (what `platform_window` and GPUI window frames use).
pub type OutlineRect = Rect;

/// What the picker shows, read from one `list_targets` answer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listing {
    /// Front to back, as the daemon returned them.
    pub windows: Vec<PickerWindow>,
    pub displays: Vec<PickerDisplay>,
    /// By window id. A window whose place cannot be worked out has no outline.
    pub outlines: HashMap<u32, OutlineRect>,
}

/// Reads a `list_targets` answer. `display_frame` maps a display id to its frame in global points
/// (`platform_window::display_frame`); without it the display's physical origin is divided by its
/// scale, which is right for the common one-scale layouts.
///
/// Minimized windows are left out (macOS does not list them today).
/// Design gap: R19 draws a "Minimized" card; build it when the capture API can capture a
/// minimized window and the enumeration lists one.
pub fn parse_targets(
    value: &Value,
    display_frame: &dyn Fn(u32) -> Option<(f64, f64, f64, f64)>,
) -> Result<Listing, String> {
    let targets: Targets =
        serde_json::from_value(value.clone()).map_err(|e| format!("list_targets: {e}"))?;
    let displays: Vec<PickerDisplay> = targets
        .displays
        .iter()
        .map(|d| PickerDisplay {
            id: d.id,
            name: d.name.clone(),
            builtin: d.is_builtin,
        })
        .collect();
    let mut out = Listing {
        displays,
        ..Listing::default()
    };
    for w in &targets.windows {
        if w.minimized || w.bounds.size.width == 0 || w.bounds.size.height == 0 {
            continue;
        }
        if out.windows.iter().any(|x| x.id == w.id) {
            continue;
        }
        let title = if w.title.trim().is_empty() {
            w.app.clone()
        } else {
            w.title.clone()
        };
        out.windows.push(PickerWindow {
            id: w.id,
            title,
            app: w.app.clone(),
            width: w.bounds.size.width,
            height: w.bounds.size.height,
            display_id: w.display_id,
        });
        let outline = w
            .display_id
            .and_then(|id| targets.displays.iter().find(|d| d.id == id))
            .and_then(|d| {
                let frame = display_frame(d.id).unwrap_or_else(|| {
                    let s = d.scale.get();
                    (
                        f64::from(d.bounds.origin.x) / s,
                        f64::from(d.bounds.origin.y) / s,
                        f64::from(d.bounds.size.width) / s,
                        f64::from(d.bounds.size.height) / s,
                    )
                });
                outline_rect(
                    (w.bounds.origin.x, w.bounds.origin.y),
                    (w.bounds.size.width, w.bounds.size.height),
                    w.scale.get(),
                    (d.bounds.origin.x, d.bounds.origin.y),
                    frame,
                )
            });
        if let Some(r) = outline {
            out.outlines.insert(w.id, r);
        }
    }
    Ok(out)
}

/// A window's rectangle in global points. `origin` and `size` are physical px in the frame of its
/// display (`list_targets`: `bounds.origin - display.bounds.origin` is the position in that
/// display's own pixels); `display_frame` is that display's frame in global points. `None` for a
/// non-positive scale or an empty window.
pub fn outline_rect(
    origin: (i32, i32),
    size: (u32, u32),
    scale: f64,
    display_origin: (i32, i32),
    display_frame: (f64, f64, f64, f64),
) -> Option<OutlineRect> {
    if !(scale.is_finite() && scale > 0.0) || size.0 == 0 || size.1 == 0 {
        return None;
    }
    let (fx, fy, _, _) = display_frame;
    Some(Rect::new(
        fx + f64::from(origin.0 - display_origin.0) / scale,
        fy + f64::from(origin.1 - display_origin.1) / scale,
        f64::from(size.0) / scale,
        f64::from(size.1) / scale,
    ))
}

// ---------------------------------------------------------------------------------------------
// Last used

/// The memory as stored (empty when never written or unreadable).
pub fn read_last_used(settings: &Map<String, Value>) -> LastUsed {
    settings
        .get(KEY_LAST_USED)
        .map(LastUsed::from_value)
        .unwrap_or_default()
}

/// The value to store: [`LastUsed::to_value`], oldest entries dropped until it fits the
/// daemon's size limit (very long window titles).
pub fn value_to_store(last_used: &LastUsed) -> Value {
    let mut shown = last_used.clone();
    loop {
        let v = shown.to_value();
        let size = serde_json::to_vec(&v).map_or(usize::MAX, |b| b.len());
        let entries = shown.entries().len();
        if size <= MAX_STORED_BYTES || entries <= 1 {
            return if size <= MAX_STORED_BYTES {
                v
            } else {
                Value::Array(Vec::new())
            };
        }
        let keep: Vec<_> = shown.entries()[..entries - 1].to_vec();
        shown = LastUsed::new();
        for e in keep.iter().rev() {
            shown.touch(&e.app, &e.title);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// App tiles

/// Tile fills of the board (kit tints, dark on light letters).
const TILE_COLOURS: [u32; 9] = [
    0x8FB0FF, 0x7FB6E8, 0xC9C9CF, 0x9EC7F0, 0xD7A6E0, 0xF5D27A, 0x8AB4F8, 0xB8E0C8, 0x9FC3FF,
];

/// The letter and fill (`0xRRGGBB`) of an app's tile: the app's first letter or digit in upper
/// case (`?` when it has none) on a colour picked by hashing the name, so the same app always
/// looks the same.
pub fn app_tile(app: &str) -> (char, u32) {
    let letter = app
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .unwrap_or('?');
    // FNV-1a over the folded name: stable across runs and platforms.
    let mut h: u32 = 0x811C_9DC5;
    for b in super::fold(app).bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    (letter, TILE_COLOURS[(h as usize) % TILE_COLOURS.len()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(id: u32) -> Option<(f64, f64, f64, f64)> {
        match id {
            1 => Some((0.0, 0.0, 1512.0, 982.0)),
            2 => Some((1512.0, -200.0, 2560.0, 1440.0)),
            _ => None,
        }
    }

    fn sample() -> Value {
        json!({
            "displays": [
                { "id": 1, "name": "Built-in Retina Display", "bounds": { "origin": {"x": 0, "y": 0}, "size": [3024, 1964] },
                  "scale": 2.0, "is_primary": true, "is_builtin": true },
                { "id": 2, "name": "LG UltraFine", "bounds": { "origin": {"x": 3024, "y": -400}, "size": [2560, 1440] },
                  "scale": 1.0, "is_primary": false }
            ],
            "windows": [
                { "id": 50, "title": "Docs: Q4 launch plan", "app": "Safari", "pid": 1,
                  "bounds": { "origin": {"x": 200, "y": 100}, "size": [1680, 1050] },
                  "scale": 2.0, "display_id": 1, "minimized": false },
                { "id": 51, "title": "  ", "app": "Terminal", "pid": 2,
                  "bounds": { "origin": {"x": 3124, "y": -300}, "size": [800, 600] },
                  "scale": 1.0, "display_id": 2, "minimized": false },
                { "id": 52, "title": "Hidden away", "app": "Preview", "pid": 3,
                  "bounds": { "origin": {"x": 0, "y": 0}, "size": [500, 400] },
                  "scale": 2.0, "display_id": 1, "minimized": true },
                { "id": 53, "title": "No display", "app": "Notes", "pid": 4,
                  "bounds": { "origin": {"x": 0, "y": 0}, "size": [500, 400] },
                  "scale": 2.0, "display_id": null, "minimized": false },
                { "id": 50, "title": "Duplicate id", "app": "Safari", "pid": 1,
                  "bounds": { "origin": {"x": 0, "y": 0}, "size": [10, 10] },
                  "scale": 1.0, "display_id": 1, "minimized": false },
                { "id": 54, "title": "Empty", "app": "Ghost", "pid": 5,
                  "bounds": { "origin": {"x": 0, "y": 0}, "size": [0, 0] },
                  "scale": 1.0, "display_id": 1, "minimized": false }
            ]
        })
    }

    #[test]
    fn maps_list_targets_to_the_picker_in_the_daemons_order() {
        let l = parse_targets(&sample(), &frame).unwrap();
        let ids: Vec<u32> = l.windows.iter().map(|w| w.id).collect();
        // Minimized, a repeated id and a zero-size window are not listed.
        assert_eq!(ids, vec![50, 51, 53]);
        assert_eq!(
            l.windows[0],
            PickerWindow {
                id: 50,
                title: "Docs: Q4 launch plan".into(),
                app: "Safari".into(),
                width: 1680,
                height: 1050,
                display_id: Some(1),
            }
        );
        // A blank title falls back to the app name.
        assert_eq!(l.windows[1].title, "Terminal");
        assert_eq!(l.windows[2].display_id, None);
        assert_eq!(l.displays.len(), 2);
        assert!(l.displays[0].builtin && !l.displays[1].builtin);
        assert_eq!(l.displays[1].name, "LG UltraFine");
    }

    #[test]
    fn outlines_are_global_points_per_display() {
        let l = parse_targets(&sample(), &frame).unwrap();
        // Retina built-in: 2 px per point, display at the origin.
        assert_eq!(l.outlines[&50], Rect::new(100.0, 50.0, 840.0, 525.0));
        // The external display sits right of the primary and starts above it: the window is
        // 100 px right of and 100 px below its display's corner (display origin 3024, -400).
        assert_eq!(l.outlines[&51], Rect::new(1612.0, -100.0, 800.0, 600.0));
        // No display: no outline, but the card is still listed.
        assert!(!l.outlines.contains_key(&53));
    }

    #[test]
    fn without_a_platform_frame_the_physical_origin_over_the_scale_is_used() {
        let l = parse_targets(&sample(), &|_| None).unwrap();
        // Display 2: frame origin (3024 / 1, -400 / 1).
        assert_eq!(l.outlines[&51], Rect::new(3124.0, -300.0, 800.0, 600.0));
        assert_eq!(l.outlines[&50], Rect::new(100.0, 50.0, 840.0, 525.0));
    }

    #[test]
    fn older_json_without_is_builtin_parses_as_external() {
        let mut v = sample();
        v["displays"][0]
            .as_object_mut()
            .unwrap()
            .remove("is_builtin");
        let l = parse_targets(&v, &frame).unwrap();
        assert!(!l.displays[0].builtin);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(parse_targets(&json!({"displays": 3}), &frame).is_err());
        assert!(parse_targets(&json!(null), &frame).is_err());
        let empty = parse_targets(&json!({"displays": [], "windows": []}), &frame).unwrap();
        assert!(empty.windows.is_empty() && empty.displays.is_empty());
    }

    #[test]
    fn outline_rejects_empty_or_unscaled() {
        assert!(outline_rect((0, 0), (0, 10), 2.0, (0, 0), (0.0, 0.0, 1.0, 1.0)).is_none());
        assert!(outline_rect((0, 0), (10, 10), 0.0, (0, 0), (0.0, 0.0, 1.0, 1.0)).is_none());
        assert!(outline_rect((0, 0), (10, 10), f64::NAN, (0, 0), (0.0, 0.0, 1.0, 1.0)).is_none());
    }

    #[test]
    fn last_used_round_trips_through_the_settings_map() {
        let mut lu = LastUsed::new();
        lu.touch("Safari", "Inbox");
        lu.touch("Finder", "Documents");
        let mut settings = Map::new();
        settings.insert(KEY_LAST_USED.into(), value_to_store(&lu));
        let back = read_last_used(&settings);
        assert_eq!(back, lu);
        assert_eq!(back.entries()[0].app, "Finder");
        // Missing or malformed: empty.
        assert!(read_last_used(&Map::new()).is_empty());
        settings.insert(KEY_LAST_USED.into(), json!("nope"));
        assert!(read_last_used(&settings).is_empty());
    }

    #[test]
    fn the_stored_memory_stays_under_the_daemons_limit() {
        let mut lu = LastUsed::new();
        for i in 0..8 {
            lu.touch("App", &format!("{i}{}", "x".repeat(3000)));
        }
        let v = value_to_store(&lu);
        let size = serde_json::to_vec(&v).unwrap().len();
        assert!(size <= MAX_STORED_BYTES, "{size}");
        // The newest entries are the ones kept.
        let back = LastUsed::from_value(&v);
        assert!(back.entries()[0].title.starts_with('7'));
        assert!(back.entries().len() < 8);
        // One title that alone is too long is not stored at all.
        let mut one = LastUsed::new();
        one.touch("App", &"y".repeat(20_000));
        assert_eq!(value_to_store(&one), json!([]));
    }

    #[test]
    fn the_key_is_app_only_for_agents() {
        assert!(cm_daemon::engines::is_app_only_setting(KEY_LAST_USED));
        assert!(cm_daemon::engines::validate_setting_key(KEY_LAST_USED).is_ok());
    }

    #[test]
    fn app_tiles_are_stable_and_use_the_first_letter() {
        assert_eq!(app_tile("Google Chrome").0, 'G');
        assert_eq!(app_tile("  terminal").0, 'T');
        assert_eq!(app_tile("1Password").0, '1');
        assert_eq!(app_tile("").0, '?');
        assert_eq!(app_tile("\u{1F600}").0, '?');
        assert_eq!(app_tile("Zoom"), app_tile("Zoom"));
        assert_eq!(app_tile("Zoom").1, app_tile("zoom").1);
        assert!(TILE_COLOURS.contains(&app_tile("Slack").1));
        // Different apps spread over the palette.
        let colours: std::collections::HashSet<u32> = [
            "Safari", "Finder", "Chrome", "Terminal", "Slack", "Notes", "Outlook", "Preview",
        ]
        .iter()
        .map(|a| app_tile(a).1)
        .collect();
        assert!(colours.len() >= 4);
    }
}
