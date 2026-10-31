//! The monitor layout of a GNOME session, read from Mutter's `org.gnome.Mutter.DisplayConfig`
//! D-Bus interface (`GetCurrentState`). It needs no permission: any client of the session bus
//! may read it, which is how Settings draws its Displays page. Other compositors answer `None`
//! and the portal backend reports one "Desktop" display.

// Only Linux asks the bus; the other systems compile the pure parts for their tests.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use crate::layout::LogicalMonitor;

/// A physical monitor from `GetCurrentState`: its connector, an optional human name and the
/// pixels of its current mode.
#[derive(Debug, Clone, PartialEq)]
pub struct MutterMonitor {
    pub connector: String,
    pub display_name: Option<String>,
    pub mode_px: (i32, i32),
}

/// A logical monitor (a rectangle of the layout) from `GetCurrentState`.
#[derive(Debug, Clone, PartialEq)]
pub struct MutterLogical {
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    /// 0 normal, 1 90, 2 180, 3 270, 4 to 7 the same flipped.
    pub transform: u32,
    pub primary: bool,
    /// Connectors of the physical monitors on it (more than one when mirrored).
    pub connectors: Vec<String>,
}

/// Joins the two lists into layout rows. A logical monitor whose connector or mode is unknown is
/// dropped, and so is one with a non-positive scale. The size is the current mode divided by the
/// scale (rounded), with width and height swapped for a rotated monitor.
pub fn logical_monitors(
    monitors: &[MutterMonitor],
    logical: &[MutterLogical],
) -> Vec<LogicalMonitor> {
    logical
        .iter()
        .filter_map(|l| {
            if !(l.scale.is_finite() && l.scale > 0.0) {
                return None;
            }
            let m = l
                .connectors
                .iter()
                .find_map(|c| monitors.iter().find(|m| &m.connector == c))?;
            let (mut w, mut h) = m.mode_px;
            if w <= 0 || h <= 0 {
                return None;
            }
            if l.transform % 2 == 1 {
                std::mem::swap(&mut w, &mut h);
            }
            Some(LogicalMonitor {
                name: m
                    .display_name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| m.connector.clone()),
                x: l.x,
                y: l.y,
                width: (f64::from(w) / l.scale).round() as u32,
                height: (f64::from(h) / l.scale).round() as u32,
                scale: l.scale,
                primary: l.primary,
            })
        })
        .collect()
}

/// The monitors of the running GNOME session, `None` when the interface is not there (another
/// compositor, no session bus) or answers something unexpected.
#[cfg(target_os = "linux")]
pub fn current_monitors() -> Option<Vec<LogicalMonitor>> {
    use ashpd::zbus::zvariant::OwnedValue;
    use ashpd::zbus::Connection;
    use std::collections::HashMap;

    type Spec = (String, String, String, String);
    type Mode = (
        String,
        i32,
        i32,
        f64,
        f64,
        Vec<f64>,
        HashMap<String, OwnedValue>,
    );
    type Monitor = (Spec, Vec<Mode>, HashMap<String, OwnedValue>);
    type Logical = (
        i32,
        i32,
        f64,
        u32,
        bool,
        Vec<Spec>,
        HashMap<String, OwnedValue>,
    );
    type State = (u32, Vec<Monitor>, Vec<Logical>, HashMap<String, OwnedValue>);

    let state: State = futures_lite::future::block_on(async {
        let connection = Connection::session().await.ok()?;
        let reply = connection
            .call_method(
                Some("org.gnome.Mutter.DisplayConfig"),
                "/org/gnome/Mutter/DisplayConfig",
                Some("org.gnome.Mutter.DisplayConfig"),
                "GetCurrentState",
                &(),
            )
            .await
            .ok()?;
        reply.body().deserialize::<State>().ok()
    })?;
    let flag = |props: &HashMap<String, OwnedValue>, key: &str| {
        props
            .get(key)
            .and_then(|v| bool::try_from(v.try_clone().ok()?).ok())
            .unwrap_or(false)
    };
    let monitors: Vec<MutterMonitor> = state
        .1
        .iter()
        .filter_map(|(spec, modes, props)| {
            let mode = modes.iter().find(|m| flag(&m.6, "is-current"))?;
            Some(MutterMonitor {
                connector: spec.0.clone(),
                display_name: props
                    .get("display-name")
                    .and_then(|v| String::try_from(v.try_clone().ok()?).ok()),
                mode_px: (mode.1, mode.2),
            })
        })
        .collect();
    let logical: Vec<MutterLogical> = state
        .2
        .iter()
        .map(|l| MutterLogical {
            x: l.0,
            y: l.1,
            scale: l.2,
            transform: l.3,
            primary: l.4,
            connectors: l.5.iter().map(|s| s.0.clone()).collect(),
        })
        .collect();
    let out = logical_monitors(&monitors, &logical);
    (!out.is_empty()).then_some(out)
}

#[cfg(not(target_os = "linux"))]
pub fn current_monitors() -> Option<Vec<LogicalMonitor>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(connector: &str, name: Option<&str>, w: i32, h: i32) -> MutterMonitor {
        MutterMonitor {
            connector: connector.to_owned(),
            display_name: name.map(str::to_owned),
            mode_px: (w, h),
        }
    }

    fn log(x: i32, y: i32, scale: f64, transform: u32, primary: bool, c: &[&str]) -> MutterLogical {
        MutterLogical {
            x,
            y,
            scale,
            transform,
            primary,
            connectors: c.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn size_is_the_mode_over_the_scale() {
        let m = logical_monitors(
            &[
                mon("eDP-1", Some("Built-in display"), 2880, 1800),
                mon("DP-1", None, 2560, 1440),
            ],
            &[
                log(0, 0, 2.0, 0, true, &["eDP-1"]),
                log(1440, 0, 1.0, 0, false, &["DP-1"]),
            ],
        );
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].width, m[0].height), (1440, 900));
        assert_eq!(m[0].name, "Built-in display");
        // No display name: the connector.
        assert_eq!(m[1].name, "DP-1");
        assert_eq!((m[1].width, m[1].height), (2560, 1440));
    }

    #[test]
    fn a_fractional_scale_rounds_the_logical_size() {
        let m = logical_monitors(
            &[mon("eDP-1", None, 1920, 1080)],
            &[log(0, 0, 1.25, 0, true, &["eDP-1"])],
        );
        assert_eq!((m[0].width, m[0].height), (1536, 864));
        assert_eq!(m[0].scale, 1.25);
    }

    #[test]
    fn a_rotated_monitor_swaps_width_and_height() {
        for t in [1, 3, 5, 7] {
            let m = logical_monitors(
                &[mon("DP-1", None, 1920, 1080)],
                &[log(0, 0, 1.0, t, true, &["DP-1"])],
            );
            assert_eq!((m[0].width, m[0].height), (1080, 1920), "transform {t}");
        }
        let m = logical_monitors(
            &[mon("DP-1", None, 1920, 1080)],
            &[log(0, 0, 1.0, 2, true, &["DP-1"])],
        );
        assert_eq!((m[0].width, m[0].height), (1920, 1080));
    }

    #[test]
    fn unknown_connectors_and_bad_scales_are_dropped() {
        let m = logical_monitors(
            &[mon("DP-1", None, 100, 100)],
            &[
                log(0, 0, 1.0, 0, true, &["nope"]),
                log(0, 0, 0.0, 0, true, &["DP-1"]),
                log(0, 0, f64::NAN, 0, true, &["DP-1"]),
                log(5, 5, 1.0, 0, false, &["DP-1"]),
            ],
        );
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].x, m[0].y), (5, 5));
    }
}
