//! `--demo-recording` and friends: drive the real recording UI from the real engine over **fake
//! sources**, so the HUD, countdown, area indicator and webcam bubble can be seen, tested and
//! screenshotted without recording the screen, the microphone or the camera.
//!
//! The engine, its events, the HUD model and every window are the production ones; only the tracks
//! (`cm_daemon::testing::FakeSources`), the display list (copied from the real displays, listing
//! them captures nothing) and the camera preview (synthetic frames) are fake. Recordings land in
//! the library the app was started with, so use a throwaway `CAPTUREMANN_LIBRARY`.
//!
//! Flags (all debug only):
//!
//! * `--demo-recording`: start a fake recording of the main display with the camera and the mic
//! * `--demo-region`: record a centered region (shows the scrim and brackets)
//! * `--demo-paused`: pause two seconds after it started (dashed outline, Resume)
//! * `--demo-agent`: start it as an agent would (the agent toast, the HUD chip)
//! * `--demo-countdown <s>`: countdown length 0..10, default 3 (use 10 to look at the countdown)
//! * `--demo-sheet`: open the start sheet instead of recording
//! * `--demo-no-camera`, `--demo-no-mic`: leave those out
//! * `CM_DEMO_CAPTURABLE=1`: do not hide the windows from capture (only so a window can be
//!   screenshotted with `screencapture -l`); never set it when real recording

use std::time::Duration;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;

/// What the demo does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoPlan {
    pub region: bool,
    pub camera: bool,
    pub mic: bool,
    pub pause_after: Option<Duration>,
    pub agent: bool,
    pub countdown_s: u32,
    pub sheet: bool,
}

impl DemoPlan {
    /// `None` when no demo flag is present.
    pub fn parse(args: &[String]) -> Option<DemoPlan> {
        let has = |f: &str| args.iter().any(|a| a == f);
        let sheet = has("--demo-sheet");
        if !(has("--demo-recording") || has("--demo-region") || has("--demo-agent") || sheet) {
            return None;
        }
        let countdown_s = args
            .iter()
            .position(|a| a == "--demo-countdown")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| *n <= 10)
            .unwrap_or(3);
        Some(DemoPlan {
            region: has("--demo-region"),
            camera: !has("--demo-no-camera"),
            mic: !has("--demo-no-mic"),
            pause_after: has("--demo-paused").then_some(Duration::from_secs(2)),
            agent: has("--demo-agent"),
            countdown_s,
            sheet,
        })
    }
}

/// A capturer over fake backends that mirrors the real displays (no capture happens). Falls back
/// to one 2880 x 1800 @2x display when the real ones cannot be listed.
pub fn fake_capturer() -> Capturer {
    // CM_DEBUG_SCREEN_PERMISSION / CM_DEBUG_TARGETS_ERROR make the listing fail (debug builds).
    if let Some(c) = crate::permissions::debug_capturer() {
        return c;
    }
    let mut backend = FakeBackend::new();
    let real = Capturer::system().list_targets_with(false).ok();
    let mut any = false;
    for d in real.iter().flat_map(|t| t.displays.iter()) {
        backend = backend.with_display(
            d.id,
            d.bounds.origin.x,
            d.bounds.origin.y,
            d.bounds.size.width,
            d.bounds.size.height,
            d.scale.get(),
        );
        any = true;
    }
    if !any {
        backend = backend.with_display(1, 0, 0, 2880, 1800, 2.0);
    }
    Capturer::new(backend).with_own_pid(None)
}

/// Whether the demo windows may be captured by `screencapture -l` (see the module docs).
pub fn capturable() -> bool {
    std::env::var_os("CM_DEMO_CAPTURABLE").is_some()
}

/// The `start_recording` params of a plan on a display of `size` physical px with id `display`.
pub fn params(plan: &DemoPlan, display: u32, size: [u32; 2]) -> serde_json::Value {
    let target = if plan.region {
        let (w, h) = (i64::from(size[0]) * 6 / 10, i64::from(size[1]) * 6 / 10);
        let (x, y) = (
            (i64::from(size[0]) - w) / 2,
            (i64::from(size[1]) - h) / 2 - 40,
        );
        serde_json::json!({ "display_id": display, "rect": [x, y.max(0), w, h] })
    } else {
        serde_json::json!({ "display_id": display })
    };
    serde_json::json!({
        "target": target,
        "camera": if plan.camera {
            serde_json::json!({ "shape": "circle", "size": "m", "position": "bottom-left" })
        } else {
            serde_json::json!(false)
        },
        "mic": plan.mic,
        "system_audio": false,
        "countdown_s": plan.countdown_s,
        "title": "Demo recording",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn flags_make_a_plan() {
        assert_eq!(DemoPlan::parse(&args(&["--theme", "dark"])), None);
        let p = DemoPlan::parse(&args(&[
            "--demo-recording",
            "--demo-paused",
            "--demo-countdown",
            "7",
        ]))
        .unwrap();
        assert!(p.camera && p.mic && !p.region && !p.agent);
        assert_eq!(p.pause_after, Some(Duration::from_secs(2)));
        assert_eq!(p.countdown_s, 7);
        let r = DemoPlan::parse(&args(&[
            "--demo-region",
            "--demo-no-camera",
            "--demo-agent",
        ]))
        .unwrap();
        assert!(r.region && !r.camera && r.agent);
        assert!(DemoPlan::parse(&args(&["--demo-sheet"])).unwrap().sheet);
    }

    /// The demo asks the engine the same thing the sheet would: a region is physical px relative
    /// to the display, centered, and the engine's own parser accepts the request.
    #[test]
    fn params_are_valid_start_requests() {
        let plan = DemoPlan::parse(&args(&["--demo-region"])).unwrap();
        let p = params(&plan, 1, [2880, 1800]);
        assert_eq!(
            p["target"]["rect"],
            serde_json::json!([576, 320, 1728, 1080])
        );
        let parsed = cm_daemon::recording::StartParams::parse(p.as_object().unwrap()).unwrap();
        assert!(parsed.camera.is_some() && parsed.mic.is_some());
        let full = DemoPlan::parse(&args(&[
            "--demo-recording",
            "--demo-no-camera",
            "--demo-no-mic",
        ]))
        .unwrap();
        let p = params(&full, 3, [100, 100]);
        assert_eq!(p["target"], serde_json::json!({"display_id": 3}));
        assert!(
            cm_daemon::recording::StartParams::parse(p.as_object().unwrap())
                .unwrap()
                .camera
                .is_none()
        );
    }
}
