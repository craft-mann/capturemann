//! What each Actions option (LNC-1 Actions) asks the app to do, as plain data: the window
//! executes these plans, and the tests check the daemon request each one leads to without a
//! display.

use super::commands::{CommandId, LaunchOption, OPTION_DELAY_S};
use crate::recording::start::SheetPreset;

/// A record command: whether to pick a region first, and what the start sheet opens with. The
/// sheet always opens; pressing Start is the confirmation (REC-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordPlan {
    /// Pick a region with the overlay first (Record region).
    pub pick_region: bool,
    /// `Some(true)` webcam on, `Some(false)` off, `None` the remembered choice.
    pub camera: Option<bool>,
    pub preset: SheetPreset,
}

/// A screenshot command: a delay with the CAP-5 pill, and a one-off copy choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotPlan {
    /// Seconds to wait with the pill first; `None` uses the tray's delay setting (CAP-5).
    pub delay_s: Option<u32>,
    /// `Some(true)` copy to the clipboard, `Some(false)` do not; `None` follows the setting.
    pub copy: Option<bool>,
}

/// The plan of a record command's option, or `None` when `id` is not a record command or the
/// option is not one of its options.
pub fn record_plan(id: CommandId, option: LaunchOption) -> Option<RecordPlan> {
    use LaunchOption as O;
    let off = |mic, system_audio| SheetPreset {
        mic,
        system_audio,
        ..SheetPreset::default()
    };
    let (pick_region, camera, preset) = match (id, option) {
        // The defaults are what the commands did before the Actions list existed.
        (CommandId::RecordScreen, O::Default) => (false, Some(true), SheetPreset::default()),
        (CommandId::RecordScreen, O::NoWebcam) => (false, Some(false), SheetPreset::default()),
        (CommandId::RecordScreen, O::MicOnly) => (false, Some(false), off(Some(true), Some(false))),
        (CommandId::RecordScreen, O::ScreenOnly) => {
            (false, Some(false), off(Some(false), Some(false)))
        }
        (CommandId::RecordScreen, O::Fps60) => (
            false,
            Some(true),
            SheetPreset {
                fps: Some(60),
                ..SheetPreset::default()
            },
        ),
        (CommandId::RecordScreen, O::Countdown5) => (
            false,
            Some(true),
            SheetPreset {
                countdown_s: Some(OPTION_DELAY_S),
                ..SheetPreset::default()
            },
        ),
        (CommandId::RecordRegion, O::Default) => (true, None, SheetPreset::default()),
        (CommandId::RecordRegion, O::WithWebcam) => (true, Some(true), SheetPreset::default()),
        (CommandId::RecordRegion, O::NoAudio) => (true, None, off(Some(false), Some(false))),
        _ => return None,
    };
    Some(RecordPlan {
        pick_region,
        camera,
        preset,
    })
}

/// The plan of a screenshot command's option, or `None` for an option that cannot run (copy
/// only: every screenshot is saved to the library) or is not a screenshot option.
pub fn shot_plan(option: LaunchOption) -> Option<ShotPlan> {
    use LaunchOption as O;
    let (delay_s, copy) = match option {
        O::Default => (None, None),
        O::SaveOnly => (None, Some(false)),
        O::CopyAndSave => (None, Some(true)),
        O::Delay5 => (Some(OPTION_DELAY_S), None),
        _ => return None,
    };
    Some(ShotPlan { delay_s, copy })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::start::{SheetDefaults, Target};
    use cm_daemon::recording::StartParams;
    use serde_json::{json, Map, Value};

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap_or_default()
    }

    /// The `start_recording` request the sheet sends for `plan` when the user presses Start
    /// without touching anything: the same steps `StartSheet::new` and `start` take.
    fn request(plan: &RecordPlan, saved: Value) -> Value {
        let mut d = SheetDefaults::from_settings(&map(saved));
        if let Some(c) = plan.camera {
            d.camera = c;
        }
        plan.preset.apply(&mut d.mic, &mut d.system_audio);
        let mut params = d.options(Target::Display { display_id: 1 }).to_params();
        params
            .as_object_mut()
            .unwrap()
            .extend(plan.preset.extra_params());
        params
    }

    /// LNC-1 Actions: each record option leads to the request it names, and the daemon's own
    /// parser accepts it.
    #[test]
    fn lnc1_record_options_build_the_right_start_request() {
        // The user saved "camera off, microphone on, system audio on".
        let saved = json!({"recording.camera": false, "recording.mic": true, "recording.system_audio": true});
        let req = |id, o| request(&record_plan(id, o).unwrap(), saved.clone());
        let cam_on = |p: &Value| p["camera"].is_object();

        // Default: as before (webcam on, saved audio choices, nothing else).
        let p = req(CommandId::RecordScreen, LaunchOption::Default);
        assert!(cam_on(&p) && p["mic"] == true && p["system_audio"] == true);
        assert!(p.get("fps").is_none() && p.get("countdown_s").is_none());
        // Without webcam: camera false, audio as saved.
        let p = req(CommandId::RecordScreen, LaunchOption::NoWebcam);
        assert!(p["camera"] == false && p["mic"] == true && p["system_audio"] == true);
        // Screen + microphone only.
        let p = req(CommandId::RecordScreen, LaunchOption::MicOnly);
        assert!(p["camera"] == false && p["mic"] == true && p["system_audio"] == false);
        // Screen only: no webcam, no audio.
        let p = req(CommandId::RecordScreen, LaunchOption::ScreenOnly);
        assert!(p["camera"] == false && p["mic"] == false && p["system_audio"] == false);
        // 60 fps and the 5 s countdown are explicit request fields, the webcam stays on.
        let p = req(CommandId::RecordScreen, LaunchOption::Fps60);
        assert!(cam_on(&p) && p["fps"] == 60 && p.get("countdown_s").is_none());
        let p = req(CommandId::RecordScreen, LaunchOption::Countdown5);
        assert!(cam_on(&p) && p["countdown_s"] == 5 && p.get("fps").is_none());
        // Record region: pick first; webcam only when asked; no-audio switches both off.
        let plan = record_plan(CommandId::RecordRegion, LaunchOption::Default).unwrap();
        assert!(plan.pick_region && plan.camera.is_none());
        let p = req(CommandId::RecordRegion, LaunchOption::Default);
        assert!(
            p["camera"] == false,
            "region keeps the remembered camera choice (off here)"
        );
        let p = req(CommandId::RecordRegion, LaunchOption::WithWebcam);
        assert!(cam_on(&p));
        let p = req(CommandId::RecordRegion, LaunchOption::NoAudio);
        assert!(p["mic"] == false && p["system_audio"] == false && p["camera"] == false);

        for id in [CommandId::RecordScreen, CommandId::RecordRegion] {
            for o in [
                LaunchOption::Default,
                LaunchOption::NoWebcam,
                LaunchOption::MicOnly,
                LaunchOption::ScreenOnly,
                LaunchOption::Fps60,
                LaunchOption::Countdown5,
                LaunchOption::WithWebcam,
                LaunchOption::NoAudio,
            ] {
                if let Some(plan) = record_plan(id, o) {
                    let p = request(&plan, saved.clone());
                    StartParams::parse(&map(p.clone()))
                        .unwrap_or_else(|e| panic!("{id:?} {o:?}: {p}: {e}"));
                }
            }
        }
    }

    /// An option that belongs to another command is not a plan.
    #[test]
    fn lnc1_options_of_other_commands_are_not_plans() {
        assert!(record_plan(CommandId::RecordRegion, LaunchOption::Fps60).is_none());
        assert!(record_plan(CommandId::RecordScreen, LaunchOption::WithWebcam).is_none());
        assert!(record_plan(CommandId::OpenLibrary, LaunchOption::Default).is_none());
        assert!(shot_plan(LaunchOption::Fps60).is_none());
    }

    /// LNC-1 Actions: the screenshot options map onto the capture flow's two knobs.
    #[test]
    fn lnc1_screenshot_options_map_to_delay_and_copy() {
        let p = |o| shot_plan(o).unwrap();
        assert_eq!(
            (
                p(LaunchOption::Default).delay_s,
                p(LaunchOption::Default).copy
            ),
            (None, None)
        );
        assert_eq!(p(LaunchOption::SaveOnly).copy, Some(false));
        assert_eq!(p(LaunchOption::CopyAndSave).copy, Some(true));
        assert_eq!(
            (
                p(LaunchOption::Delay5).delay_s,
                p(LaunchOption::Delay5).copy
            ),
            (Some(5), None)
        );
        assert!(
            shot_plan(LaunchOption::CopyOnly).is_none(),
            "copy only cannot run: every screenshot is saved to the library"
        );
    }
}
