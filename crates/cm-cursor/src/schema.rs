//! The `cursor.jsonl` schema (REC-7), version 1.
//!
//! Line 1 is the session header ([`CursorHeader`]); every further line is one [`CursorEvent`].
//! Times are `MediaTime` ticks (120,000/s) on the session clock and non-decreasing down the file.
//!
//! **Coordinate convention.** `x`/`y` are integer physical px relative to the top-left of the
//! recorded area (the capture target: a display, a region or a window), y down, so they equal
//! pixel coordinates in the screen track. The cursor hotspot position is reported. When the cursor
//! is outside the recorded area the event keeps its out-of-range coordinates (negative, or at or
//! beyond the target size) and carries `"outside": true`; this also holds for the part of a
//! multi-display desktop that is not the recorded display. The mapping from the OS's global space
//! is stored in the header (`target`, see [`crate::CursorTarget`]).
//!
//! **Privacy (NFR-9).** No key codes or characters exist in this schema. Modifiers appear only as
//! flags on button events. Scroll carries direction signs only.

use cm_core::{ErrorCode, Permission};
use serde::{Deserialize, Serialize};

use crate::target::CursorTarget;

/// Current schema version written in the header.
pub const SCHEMA_VERSION: u32 = 1;

/// Name of the telemetry file inside the bundle.
pub const CURSOR_FILE: &str = "cursor.jsonl";
/// Name of the cursor image directory inside the bundle.
pub const CURSORS_DIR: &str = "cursors";
/// Name of the image index inside [`CURSORS_DIR`].
pub const INDEX_FILE: &str = "index.json";

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other,
}

/// A modifier held during a click. Flags only; never key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Cmd,
}

/// Whether an input class could be recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable,
}

/// How positions are obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveSource {
    /// macOS listen-only `CGEventTap`: OS event timestamps.
    EventTap,
    /// Windows low-level mouse hook, stamped at hook entry.
    Hook,
    /// Position polling at 120 Hz (permission fallback): stamped at poll time.
    Poll,
    /// Injected source (tests).
    Fake,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn no_mods(m: &[Modifier]) -> bool {
    m.is_empty()
}

/// One telemetry event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CursorEvent {
    /// The pointer moved (rate limited to 120 per second, only on change).
    Move {
        t: i64,
        x: i32,
        y: i32,
        #[serde(default, skip_serializing_if = "is_false")]
        outside: bool,
    },
    /// A button went down.
    Down {
        t: i64,
        button: Button,
        x: i32,
        y: i32,
        #[serde(default, skip_serializing_if = "no_mods")]
        mods: Vec<Modifier>,
        #[serde(default, skip_serializing_if = "is_false")]
        outside: bool,
    },
    /// A button went up.
    Up {
        t: i64,
        button: Button,
        x: i32,
        y: i32,
        #[serde(default, skip_serializing_if = "no_mods")]
        mods: Vec<Modifier>,
        #[serde(default, skip_serializing_if = "is_false")]
        outside: bool,
    },
    /// Scroll direction only: the sign (-1, 0, 1) of the OS scroll delta per axis (macOS
    /// point-delta sign convention: `dy > 0` is wheel/fingers moving up).
    Scroll { t: i64, dx: i8, dy: i8 },
    /// The cursor image changed to `id` (`cursors/<id>.png`). `hotspot` is in px of that PNG,
    /// `scale` is PNG px per logical point (2.0 on Retina).
    Cursor {
        t: i64,
        id: String,
        hotspot: [i32; 2],
        scale: f64,
    },
    /// An event type this version does not know (forward compatibility); skipped by readers.
    #[serde(other)]
    Unknown,
}

impl CursorEvent {
    /// Event time in ticks (0 for [`CursorEvent::Unknown`]).
    pub fn t(&self) -> i64 {
        match self {
            CursorEvent::Move { t, .. }
            | CursorEvent::Down { t, .. }
            | CursorEvent::Up { t, .. }
            | CursorEvent::Scroll { t, .. }
            | CursorEvent::Cursor { t, .. } => *t,
            CursorEvent::Unknown => 0,
        }
    }

    pub(crate) fn set_t(&mut self, new_t: i64) {
        match self {
            CursorEvent::Move { t, .. }
            | CursorEvent::Down { t, .. }
            | CursorEvent::Up { t, .. }
            | CursorEvent::Scroll { t, .. }
            | CursorEvent::Cursor { t, .. } => *t = new_t,
            CursorEvent::Unknown => {}
        }
    }
}

/// Line 1 of `cursor.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorHeader {
    /// Always `"session"`.
    #[serde(rename = "type")]
    pub ty: String,
    pub schema: u32,
    pub tick_rate: i64,
    /// Which clock the OS stamps come from (informational).
    pub clock: String,
    pub platform: String,
    /// Mapping from the OS's global cursor space to the recorded area.
    pub target: CursorTarget,
    /// Button down/up events. `unavailable` when the OS denied input monitoring (REC-7.AC4).
    pub clicks: Availability,
    /// Scroll events (same availability as clicks).
    pub scroll: Availability,
    pub move_source: MoveSource,
    /// The permission whose absence degraded the recording, if any.
    pub permission_missing: Option<Permission>,
}

impl CursorHeader {
    pub(crate) fn new(
        target: CursorTarget,
        status: &CursorStatus,
        clock: &str,
        platform: &str,
    ) -> Self {
        CursorHeader {
            ty: "session".into(),
            schema: SCHEMA_VERSION,
            tick_rate: cm_core::TICKS_PER_SECOND,
            clock: clock.into(),
            platform: platform.into(),
            target,
            clicks: status.clicks,
            scroll: status.scroll,
            move_source: status.move_source,
            permission_missing: status.missing_permission(),
        }
    }
}

/// A non-fatal problem the HUD can show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorWarning {
    /// `permission_missing` for the permission fallback.
    pub code: ErrorCode,
    pub permission: Option<Permission>,
    pub message: String,
}

/// What the cursor track can record in this session, known right after `start`. A degraded status
/// is a warning, never a failure of the recording (REC-7.AC4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorStatus {
    pub clicks: Availability,
    pub scroll: Availability,
    pub move_source: MoveSource,
    pub warnings: Vec<CursorWarning>,
}

impl CursorStatus {
    /// Everything works.
    pub fn full(move_source: MoveSource) -> Self {
        CursorStatus {
            clicks: Availability::Available,
            scroll: Availability::Available,
            move_source,
            warnings: Vec::new(),
        }
    }

    /// Positions only, because `permission` is missing: `permission_missing(<permission>)`.
    pub fn permission_fallback(permission: Permission, message: impl Into<String>) -> Self {
        CursorStatus {
            clicks: Availability::Unavailable,
            scroll: Availability::Unavailable,
            move_source: MoveSource::Poll,
            warnings: vec![CursorWarning {
                code: ErrorCode::PermissionMissing,
                permission: Some(permission),
                message: message.into(),
            }],
        }
    }

    /// True when anything is degraded.
    pub fn is_degraded(&self) -> bool {
        self.clicks == Availability::Unavailable || !self.warnings.is_empty()
    }

    /// The first `permission_missing` warning's permission.
    pub fn missing_permission(&self) -> Option<Permission> {
        self.warnings
            .iter()
            .find(|w| w.code == ErrorCode::PermissionMissing)
            .and_then(|w| w.permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_and_match_the_spec_example() {
        let evs = vec![
            CursorEvent::Move {
                t: 1_834_000,
                x: 1210,
                y: 644,
                outside: false,
            },
            CursorEvent::Down {
                t: 1_842_000,
                button: Button::Left,
                x: 1212,
                y: 646,
                mods: vec![Modifier::Shift],
                outside: false,
            },
            CursorEvent::Up {
                t: 1_850_000,
                button: Button::Right,
                x: -4,
                y: 10,
                mods: vec![],
                outside: true,
            },
            CursorEvent::Scroll {
                t: 1_860_000,
                dx: 0,
                dy: -1,
            },
            CursorEvent::Cursor {
                t: 1_842_000,
                id: "c_9f2a".into(),
                hotspot: [3, 2],
                scale: 2.0,
            },
        ];
        for e in &evs {
            let s = serde_json::to_string(e).unwrap();
            assert_eq!(&serde_json::from_str::<CursorEvent>(&s).unwrap(), e, "{s}");
        }
        assert_eq!(
            serde_json::to_string(&evs[0]).unwrap(),
            r#"{"type":"move","t":1834000,"x":1210,"y":644}"#
        );
        assert_eq!(
            serde_json::to_string(&evs[1]).unwrap(),
            r#"{"type":"down","t":1842000,"button":"left","x":1212,"y":646,"mods":["shift"]}"#
        );
        assert!(serde_json::to_string(&evs[2])
            .unwrap()
            .contains("\"outside\":true"));
    }

    #[test]
    fn unknown_types_decode_as_unknown() {
        let e: CursorEvent = serde_json::from_str(r#"{"type":"future","t":5,"z":1}"#).unwrap();
        assert_eq!(e, CursorEvent::Unknown);
    }

    #[test]
    fn schema_has_no_key_fields() {
        // Privacy (REC-7.AC3, NFR-9): the only keyboard-derived data is the modifier flag set.
        let all = serde_json::to_string(&CursorEvent::Down {
            t: 0,
            button: Button::Left,
            x: 0,
            y: 0,
            mods: vec![
                Modifier::Shift,
                Modifier::Ctrl,
                Modifier::Alt,
                Modifier::Cmd,
            ],
            outside: false,
        })
        .unwrap();
        assert!(!all.contains("key"));
    }
}
