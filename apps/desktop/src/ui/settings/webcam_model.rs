//! Settings -> Webcam & audio, board R13: the models behind the Camera and Microphone dropdowns,
//! the preview card and the live-preview switch. Everything here is pure (no window, no device),
//! so the rules are tested with fakes.
//!
//! # The two settings
//!
//! `camera.device` and `audio.mic_device` hold a device id. A missing key, `null` or an empty
//! string means the system default. They are the defaults wherever a device is chosen and none is
//! named: the Record sheet's first choice, a hotkey start, and the daemon for `start_recording`
//! (`camera: on`, `mic: default`). A device named by the request or picked in the sheet for one
//! recording wins. A saved device that is not connected is replaced by the system default for that
//! recording and the saved id is kept, so the device returns when it is plugged in again.

use cm_i18n::{tr, tr_ctx};
use serde_json::{json, Value};

use crate::recording::geometry::{bubble_dims, BubblePosition, BubbleShape, BubbleSize, Rect};
use crate::recording::sheet::DeviceChoice;

// ---- the dropdowns ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Camera,
    Mic,
}

/// A small tag next to a device name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// The system's own default device.
    Default,
    /// The saved device is not plugged in.
    NotConnected,
}

impl Hint {
    pub fn label(self) -> String {
        match self {
            Hint::Default => tr_ctx!("device hint", "Default"),
            Hint::NotConnected => tr_ctx!("device hint", "Not connected"),
        }
    }
}

/// One line of an open dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceOption {
    /// What choosing it saves: `None` is the system default (the key is set to `null`).
    pub value: Option<String>,
    pub name: String,
    pub hint: Option<Hint>,
    pub chosen: bool,
}

/// The dropdown of one row: its trigger and its options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropdown {
    pub options: Vec<DeviceOption>,
    /// The name shown on the closed dropdown.
    pub trigger: String,
    pub trigger_hint: Option<Hint>,
    /// The saved device is not connected: the note line under the row shows.
    pub fallback_note: bool,
}

fn system_default_name() -> String {
    tr_ctx!("device", "System default")
}

fn saved_device_name(kind: DeviceKind) -> String {
    match kind {
        DeviceKind::Camera => tr_ctx!("device", "Saved camera"),
        DeviceKind::Mic => tr_ctx!("device", "Saved microphone"),
    }
}

/// The note under a row whose saved device is not connected.
pub fn fallback_note_text() -> String {
    tr!("Using the system default until it is plugged in.")
}

impl Dropdown {
    /// `devices` is what `list_devices` reported (`loaded`: it has been reported at all);
    /// `saved` is the setting; `known_name` is the name this device had when it was last listed in
    /// this session, which is all that is known of a device that is gone.
    pub fn build(
        kind: DeviceKind,
        devices: &[DeviceChoice],
        loaded: bool,
        saved: Option<&str>,
        known_name: Option<&str>,
    ) -> Dropdown {
        let saved = saved.filter(|s| !s.is_empty());
        let connected = saved.is_some_and(|s| devices.iter().any(|d| d.id == s));
        let missing = loaded && saved.is_some() && !connected;
        let default_name = devices
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.name.clone());
        let mut options = vec![DeviceOption {
            value: None,
            hint: default_name.is_some().then_some(Hint::Default),
            name: default_name.unwrap_or_else(system_default_name),
            chosen: saved.is_none(),
        }];
        for d in devices {
            options.push(DeviceOption {
                value: Some(d.id.clone()),
                name: d.name.clone(),
                hint: None,
                chosen: saved == Some(d.id.as_str()),
            });
        }
        if missing {
            options.push(DeviceOption {
                value: saved.map(str::to_owned),
                name: known_name.map_or_else(|| saved_device_name(kind), str::to_owned),
                hint: Some(Hint::NotConnected),
                chosen: true,
            });
        }
        // Before the first listing the saved device is not known to be missing: its name is shown
        // as far as it is known, without a claim.
        let (trigger, trigger_hint) = match options.iter().find(|o| o.chosen) {
            Some(o) => (o.name.clone(), o.hint.filter(|h| *h == Hint::NotConnected)),
            None => (
                known_name.map_or_else(|| saved_device_name(kind), str::to_owned),
                None,
            ),
        };
        Dropdown {
            options,
            trigger,
            trigger_hint,
            fallback_note: missing,
        }
    }
}

/// The value `set_settings` gets for a choice: `null` for the system default.
pub fn setting_value(value: &Option<String>) -> Value {
    match value {
        Some(id) => json!(id),
        None => Value::Null,
    }
}

/// The saved device id of a settings snapshot (`None`: unset, null or empty).
pub fn saved_device(settings: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
}

/// The device a live preview opens: the saved camera when it is connected, else the system
/// default (`None`).
pub fn live_device(devices: &[DeviceChoice], saved: Option<&str>) -> Option<String> {
    saved
        .filter(|s| devices.iter().any(|d| d.id == *s))
        .map(str::to_owned)
}

// ---- the preview card ------------------------------------------------------------------------

/// The size of the display the card pretends to show, in logical px. The bubble is placed on it
/// with the real rules and the card scales the result down.
pub const VIRTUAL_SCREEN: (f64, f64) = (1440.0, 810.0);

/// Where and how the bubble is drawn on the wireframe screen, in [`VIRTUAL_SCREEN`] px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewBubble {
    pub rect: Rect,
    /// Outer corner radius.
    pub radius: f64,
    /// The silhouette is flipped (the preview is mirrored).
    pub flip: bool,
}

pub fn preview_bubble(
    size: BubbleSize,
    shape: BubbleShape,
    mirror: bool,
    position: BubblePosition,
) -> PreviewBubble {
    let (w, h) = bubble_dims(size, shape);
    let area = Rect::new(0.0, 0.0, VIRTUAL_SCREEN.0, VIRTUAL_SCREEN.1);
    PreviewBubble {
        rect: position.rect(&area, (w, h)),
        radius: shape.radius(w, h),
        flip: mirror,
    }
}

impl PreviewBubble {
    /// The rectangle on a card whose screen is `screen_w` px wide.
    pub fn scaled(&self, screen_w: f64) -> Rect {
        let k = screen_w / VIRTUAL_SCREEN.0;
        Rect::new(
            self.rect.x * k,
            self.rect.y * k,
            self.rect.w * k,
            self.rect.h * k,
        )
    }
}

/// The corner (or edge) a position names.
#[cfg(test)]
pub fn anchor_of(position: BubblePosition) -> Option<crate::recording::geometry::Anchor> {
    match position {
        BubblePosition::Anchor(a) => Some(a),
        BubblePosition::Fraction(..) => None,
    }
}

/// The caption pill: "Preview · M, circle, mirrored". One sentence with named placeholders, so a
/// language can order and inflect the three words as it needs.
pub fn preview_caption(size: BubbleSize, shape: BubbleShape, mirror: bool) -> String {
    let shape_word = match shape {
        BubbleShape::Circle => tr_ctx!("preview caption shape", "circle"),
        BubbleShape::Rounded => tr_ctx!("preview caption shape", "rounded"),
    };
    let mirror_word = if mirror {
        tr_ctx!("preview caption mirror", "mirrored")
    } else {
        tr_ctx!("preview caption mirror", "not mirrored")
    };
    tr!(
        "Preview \u{00B7} {size}, {shape}, {mirror}",
        size = size.key().to_uppercase(),
        shape = shape_word,
        mirror = mirror_word
    )
}

// ---- the live camera switch ------------------------------------------------------------------

/// What decides whether the card shows the live camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveInputs {
    /// The "Show live camera" switch.
    pub requested: bool,
    /// Camera permission is granted.
    pub permission: bool,
    /// The Webcam & audio page is the page on screen.
    pub on_page: bool,
    /// The app's window is in front (not hidden, not deactivated).
    pub window_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStep {
    Start,
    Stop,
    Keep,
}

impl LiveInputs {
    /// The camera may be on (and its indicator light with it).
    pub fn should_run(self) -> bool {
        self.requested && self.permission && self.on_page && self.window_active
    }

    /// What to do now that `running` (a preview is running or starting) and the inputs are known.
    pub fn step(self, running: bool) -> LiveStep {
        match (self.should_run(), running) {
            (true, false) => LiveStep::Start,
            (false, true) => LiveStep::Stop,
            _ => LiveStep::Keep,
        }
    }

    /// The switch after leaving the page, hiding or deactivating the window, or losing the
    /// permission: it goes back off, and it is never remembered across launches.
    pub fn requested_after(self) -> bool {
        self.requested && self.permission && self.on_page && self.window_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::geometry::Anchor;
    use cm_i18n::{with_language, Lang};

    fn dev(id: &str, name: &str, default: bool) -> DeviceChoice {
        DeviceChoice {
            id: id.into(),
            name: name.into(),
            is_default: default,
        }
    }

    fn cams() -> Vec<DeviceChoice> {
        vec![
            dev("c1", "FaceTime HD Camera", true),
            dev("c2", "Studio Display Camera", false),
        ]
    }

    #[test]
    fn nothing_saved_chooses_the_system_default_first() {
        with_language(Lang::En, || {
            let d = Dropdown::build(DeviceKind::Camera, &cams(), true, None, None);
            assert_eq!(d.options.len(), 3);
            assert_eq!(d.options[0].value, None);
            assert_eq!(d.options[0].name, "FaceTime HD Camera");
            assert_eq!(d.options[0].hint, Some(Hint::Default));
            assert!(d.options[0].chosen && !d.options[1].chosen && !d.options[2].chosen);
            assert_eq!(d.trigger, "FaceTime HD Camera");
            assert_eq!(d.trigger_hint, None);
            assert!(!d.fallback_note);
        });
    }

    #[test]
    fn a_saved_device_is_marked_and_shown() {
        let d = Dropdown::build(DeviceKind::Camera, &cams(), true, Some("c2"), None);
        assert!(!d.options[0].chosen);
        assert!(d.options[2].chosen && d.options[2].value.as_deref() == Some("c2"));
        assert_eq!(d.trigger, "Studio Display Camera");
        assert!(!d.fallback_note);
    }

    #[test]
    fn no_default_flag_reads_system_default() {
        with_language(Lang::En, || {
            let list = vec![dev("m1", "USB Mic", false)];
            let d = Dropdown::build(DeviceKind::Mic, &list, true, None, None);
            assert_eq!(d.options[0].name, "System default");
            assert_eq!(d.options[0].hint, None);
            let none = Dropdown::build(DeviceKind::Mic, &[], true, None, None);
            assert_eq!(none.options.len(), 1);
            assert_eq!(none.trigger, "System default");
        });
    }

    /// A saved device that is gone: its name with "Not connected", a note, and no device chosen
    /// but that entry; the saved id stays what a click on it would save.
    #[test]
    fn a_missing_saved_device_shows_its_name_and_the_note() {
        with_language(Lang::En, || {
            let d = Dropdown::build(
                DeviceKind::Camera,
                &cams(),
                true,
                Some("gone"),
                Some("Old Cam"),
            );
            assert!(d.fallback_note);
            assert_eq!(d.trigger, "Old Cam");
            assert_eq!(d.trigger_hint, Some(Hint::NotConnected));
            assert_eq!(d.options.iter().filter(|o| o.chosen).count(), 1);
            let last = d.options.last().unwrap();
            assert_eq!(last.value.as_deref(), Some("gone"));
            assert_eq!(last.hint.map(Hint::label).as_deref(), Some("Not connected"));
            assert!(!d.options[0].chosen, "the default entry is not marked");
            // Name unknown (a new launch): a generic word, never the raw id.
            let d = Dropdown::build(DeviceKind::Mic, &[], true, Some("gone"), None);
            assert_eq!(d.trigger, "Saved microphone");
            assert_eq!(
                fallback_note_text(),
                "Using the system default until it is plugged in."
            );
        });
    }

    /// Before the first listing nothing is claimed about the saved device.
    #[test]
    fn before_the_first_listing_no_note() {
        let d = Dropdown::build(DeviceKind::Camera, &[], false, Some("c2"), Some("Studio"));
        assert!(!d.fallback_note);
        assert_eq!(d.trigger, "Studio");
        assert_eq!(d.trigger_hint, None);
    }

    /// Hot-plug: the same saved id is "not connected" after an unplug and normal again after the
    /// plug, with the saved value untouched.
    #[test]
    fn unplugging_and_plugging_back_follows_the_list() {
        let mut list = cams();
        let d = Dropdown::build(
            DeviceKind::Camera,
            &list,
            true,
            Some("c2"),
            Some("Studio Display Camera"),
        );
        assert!(!d.fallback_note);
        list.retain(|c| c.id != "c2");
        let d = Dropdown::build(
            DeviceKind::Camera,
            &list,
            true,
            Some("c2"),
            Some("Studio Display Camera"),
        );
        assert!(d.fallback_note);
        assert_eq!(d.trigger, "Studio Display Camera");
        list.push(dev("c2", "Studio Display Camera", false));
        let d = Dropdown::build(
            DeviceKind::Camera,
            &list,
            true,
            Some("c2"),
            Some("Studio Display Camera"),
        );
        assert!(!d.fallback_note);
        assert!(d.options.iter().find(|o| o.chosen).unwrap().hint.is_none());
    }

    #[test]
    fn choices_write_null_for_the_default_and_the_id_otherwise() {
        assert_eq!(setting_value(&None), Value::Null);
        assert_eq!(setting_value(&Some("c1".into())), json!("c1"));
        let mut m = serde_json::Map::new();
        assert_eq!(saved_device(&m, "camera.device"), None);
        m.insert("camera.device".into(), Value::Null);
        assert_eq!(saved_device(&m, "camera.device"), None);
        m.insert("camera.device".into(), json!(""));
        assert_eq!(saved_device(&m, "camera.device"), None);
        m.insert("camera.device".into(), json!("c2"));
        assert_eq!(saved_device(&m, "camera.device").as_deref(), Some("c2"));
    }

    #[test]
    fn the_live_preview_opens_the_saved_camera_only_when_connected() {
        assert_eq!(live_device(&cams(), Some("c2")).as_deref(), Some("c2"));
        assert_eq!(live_device(&cams(), Some("gone")), None);
        assert_eq!(live_device(&cams(), None), None);
    }

    #[test]
    fn mic_dropdown_never_offers_off() {
        let d = Dropdown::build(DeviceKind::Mic, &[dev("m1", "Mic", true)], true, None, None);
        assert!(d
            .options
            .iter()
            .all(|o| !o.name.eq_ignore_ascii_case("off") && o.value.as_deref() != Some("off")));
    }

    // ---- caption ----

    #[test]
    fn the_caption_in_english_for_every_combination() {
        with_language(Lang::En, || {
            for (size, s) in [
                (BubbleSize::S, "S"),
                (BubbleSize::M, "M"),
                (BubbleSize::L, "L"),
            ] {
                for (shape, sh) in [
                    (BubbleShape::Circle, "circle"),
                    (BubbleShape::Rounded, "rounded"),
                ] {
                    for (mirror, m) in [(true, "mirrored"), (false, "not mirrored")] {
                        assert_eq!(
                            preview_caption(size, shape, mirror),
                            format!("Preview \u{00B7} {s}, {sh}, {m}")
                        );
                    }
                }
            }
        });
    }

    #[test]
    fn the_caption_in_vietnamese_for_every_combination() {
        with_language(Lang::Vi, || {
            for size in BubbleSize::ALL {
                for shape in [BubbleShape::Circle, BubbleShape::Rounded] {
                    for mirror in [true, false] {
                        let text = preview_caption(size, shape, mirror);
                        assert!(text.starts_with("Xem trước"), "{text}");
                        assert!(text.contains(&size.key().to_uppercase()), "{text}");
                        assert!(
                            text.contains(if shape == BubbleShape::Circle {
                                "tròn"
                            } else {
                                "bo góc"
                            }),
                            "{text}"
                        );
                        assert!(
                            text.contains(if mirror { "lật" } else { "không lật" }),
                            "{text}"
                        );
                        assert!(!text.contains('{'), "{text}");
                    }
                }
            }
            assert_eq!(
                preview_caption(BubbleSize::M, BubbleShape::Circle, true),
                "Xem trước \u{00B7} M, tròn, đã lật"
            );
        });
    }

    // ---- geometry ----

    #[test]
    fn the_bubble_sits_in_the_remembered_corner() {
        let at = |a| {
            preview_bubble(
                BubbleSize::M,
                BubbleShape::Circle,
                true,
                BubblePosition::Anchor(a),
            )
        };
        let (w, h) = VIRTUAL_SCREEN;
        let bl = at(Anchor::BottomLeft);
        assert_eq!((bl.rect.x, bl.rect.bottom()), (24.0, h - 24.0));
        let tr = at(Anchor::TopRight);
        assert_eq!((tr.rect.right(), tr.rect.y), (w - 24.0, 24.0));
        let tl = at(Anchor::TopLeft);
        assert_eq!((tl.rect.x, tl.rect.y), (24.0, 24.0));
        let br = at(Anchor::BottomRight);
        assert_eq!((br.rect.right(), br.rect.bottom()), (w - 24.0, h - 24.0));
        // A dragged position (fractions of the area) keeps the bubble inside the screen.
        let f = preview_bubble(
            BubbleSize::L,
            BubbleShape::Circle,
            false,
            BubblePosition::Fraction(1.0, 1.0),
        );
        assert!(f.rect.right() <= w && f.rect.bottom() <= h);
    }

    #[test]
    fn size_and_shape_follow_the_real_bubble() {
        let p = BubblePosition::default();
        let s = preview_bubble(BubbleSize::S, BubbleShape::Circle, true, p);
        let m = preview_bubble(BubbleSize::M, BubbleShape::Circle, true, p);
        let l = preview_bubble(BubbleSize::L, BubbleShape::Circle, true, p);
        assert_eq!((s.rect.w, m.rect.w, l.rect.w), (160.0, 240.0, 320.0));
        assert_eq!((m.rect.w, m.rect.h), (240.0, 240.0));
        assert_eq!(m.radius, 120.0, "a circle");
        let r = preview_bubble(BubbleSize::M, BubbleShape::Rounded, true, p);
        assert_eq!((r.rect.w, r.rect.h), (240.0, 180.0), "4:3");
        assert_eq!(r.radius, 24.0);
        // On the card the same proportions, scaled.
        let on_card = m.scaled(720.0);
        assert_eq!((on_card.w, on_card.h), (120.0, 120.0));
    }

    #[test]
    fn mirror_flips_the_silhouette_only() {
        let p = BubblePosition::default();
        let a = preview_bubble(BubbleSize::M, BubbleShape::Circle, true, p);
        let b = preview_bubble(BubbleSize::M, BubbleShape::Circle, false, p);
        assert!(a.flip && !b.flip);
        assert_eq!(a.rect, b.rect);
        assert_eq!(anchor_of(p), Some(Anchor::BottomLeft));
    }

    // ---- live state machine ----

    fn inputs(requested: bool, permission: bool, on_page: bool, window_active: bool) -> LiveInputs {
        LiveInputs {
            requested,
            permission,
            on_page,
            window_active,
        }
    }

    #[test]
    fn the_camera_starts_only_when_everything_allows_it() {
        assert_eq!(inputs(true, true, true, true).step(false), LiveStep::Start);
        for off in [
            inputs(false, true, true, true),
            inputs(true, false, true, true),
            inputs(true, true, false, true),
            inputs(true, true, true, false),
        ] {
            assert_eq!(off.step(false), LiveStep::Keep, "{off:?}");
        }
    }

    #[test]
    fn the_camera_stops_when_the_switch_goes_off_the_page_closes_or_the_app_deactivates() {
        assert_eq!(inputs(true, true, true, true).step(true), LiveStep::Keep);
        for off in [
            inputs(false, true, true, true),
            inputs(true, false, true, true),
            inputs(true, true, false, true),
            inputs(true, true, true, false),
        ] {
            assert_eq!(off.step(true), LiveStep::Stop, "{off:?}");
        }
        assert_eq!(inputs(false, true, true, true).step(false), LiveStep::Keep);
    }

    #[test]
    fn the_switch_goes_back_off_and_is_not_remembered() {
        assert!(inputs(true, true, true, true).requested_after());
        assert!(!inputs(true, true, false, true).requested_after());
        assert!(!inputs(true, true, true, false).requested_after());
        assert!(!inputs(true, false, true, true).requested_after());
        assert!(!inputs(false, true, true, true).requested_after());
    }
}
