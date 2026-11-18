//! What one Quick Capture run does, as plain data (QC-1, QC-3, QC-4): built from the panel or
//! from a preset, executed by `ui::quick_flow`. The tests check the decisions here without a
//! display or hardware.

use cm_core::preset::{Preset, PresetImageFormat, PresetKind, PresetTarget};
use serde_json::Value;

use super::state::PanelState;

/// Where the capture ends up (QC-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterMode {
    /// The after-capture bar: Editor and Copy are both off.
    Bar,
    /// No bar: the capture goes to the editor and the clipboard as chosen, and a toast offers
    /// Edit and Undo for [`TOAST_SECS`] seconds.
    Toast,
}

/// How long the Edit / Undo toast stays (QC-3.AC2).
pub const TOAST_SECS: u64 = 6;

pub fn after_mode(editor: bool, copy: bool) -> AfterMode {
    if editor || copy {
        AfterMode::Toast
    } else {
        AfterMode::Bar
    }
}

/// A screenshot run.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotRun {
    pub target: PresetTarget,
    pub delay_s: u32,
    pub copy: bool,
    pub editor: bool,
    pub redact: bool,
    /// The editor tool selected on opening (an annotation op name such as `step`).
    pub tool: Option<String>,
    /// The folder of a preset with `{date}` not yet expanded.
    pub save_to: Option<String>,
    pub format: Option<PresetImageFormat>,
    pub scale: Option<u8>,
    pub cursor: Option<bool>,
}

impl ShotRun {
    pub fn after_mode(&self) -> AfterMode {
        after_mode(self.editor, self.copy)
    }

    /// Whether anything is done to the capture after it is stored, besides the toast or bar.
    pub fn has_steps(&self) -> bool {
        self.redact || self.editor || self.save_to.is_some()
    }
}

/// A video run.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoRun {
    pub target: PresetTarget,
    pub mic: bool,
    pub system_audio: bool,
    pub webcam: bool,
    pub countdown_s: u32,
    pub fps: Option<u32>,
    pub cursor: Option<bool>,
}

/// An audio run.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioRun {
    pub mic: bool,
    pub system_audio: bool,
    /// Transcribe when the recording ends (the app starts the job after the stop).
    pub transcribe: bool,
    /// Show the marker key after the start.
    pub markers: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Run {
    Shot(ShotRun),
    Video(VideoRun),
    Audio(AudioRun),
}

impl PanelState {
    /// The run the action bar starts.
    pub fn run(&self) -> Run {
        let (s, v, a) = (&self.mem.shot, &self.mem.video, &self.mem.audio);
        let carried = self.preset.as_ref().filter(|p| p.kind == self.mem.kind);
        match self.mem.kind {
            PresetKind::Screenshot => Run::Shot(ShotRun {
                target: self.mem.shot_target,
                delay_s: s.delay_s,
                copy: s.copy,
                editor: s.editor,
                redact: s.redact,
                tool: carried
                    .filter(|_| s.editor)
                    .and_then(|p| p.after.tool.clone()),
                save_to: carried.and_then(|p| p.save_to.clone()),
                format: carried.and_then(|p| p.options.format),
                scale: carried.and_then(|p| p.options.scale),
                cursor: carried.and_then(|p| p.options.cursor),
            }),
            PresetKind::Video => Run::Video(VideoRun {
                target: self.mem.video_target,
                mic: v.mic,
                system_audio: v.system_audio,
                webcam: v.webcam,
                countdown_s: v.countdown_s,
                fps: carried.and_then(|p| p.options.fps),
                cursor: carried.and_then(|p| p.options.cursor),
            }),
            PresetKind::Audio => Run::Audio(AudioRun {
                mic: a.mic,
                system_audio: a.system_audio,
                transcribe: a.transcribe,
                markers: a.markers,
            }),
        }
    }
}

/// The run of a preset started by its shortcut. `resolved` is what the daemon's
/// `resolve_preset_params` made of `{ preset }` for `capture_screenshot` or `start_recording`
/// (QC-5: the same call an agent makes), so the delay, the copy choice and the sound sources are
/// the daemon's reading; what a call cannot carry (Auto-redact, the editor and its tool, the
/// folder, the format and scale, transcribing) comes from the preset itself.
pub fn run_from_resolved(preset: &Preset, resolved: &Value) -> Run {
    let b = |k: &str| resolved.get(k).and_then(Value::as_bool);
    let n = |k: &str| {
        resolved
            .get(k)
            .and_then(Value::as_f64)
            .map(|v| v.round().max(0.0) as u32)
    };
    match preset.kind {
        PresetKind::Screenshot => Run::Shot(ShotRun {
            target: preset.target.unwrap_or(PresetTarget::Region),
            delay_s: n("delay_s").unwrap_or(0),
            copy: b("copy_to_clipboard").unwrap_or(false),
            editor: preset.after.open_in_editor,
            redact: preset.after.auto_redact,
            tool: preset
                .after
                .tool
                .clone()
                .filter(|_| preset.after.open_in_editor),
            save_to: preset.save_to.clone(),
            format: preset.options.format,
            scale: preset.options.scale,
            cursor: preset.options.cursor,
        }),
        PresetKind::Video => Run::Video(VideoRun {
            target: preset
                .target
                .filter(|t| *t != PresetTarget::LongShot)
                .unwrap_or(PresetTarget::Display),
            mic: b("mic").unwrap_or(false),
            system_audio: b("system_audio").unwrap_or(false),
            webcam: b("camera").unwrap_or(false),
            countdown_s: n("countdown_s").unwrap_or(0),
            fps: n("fps"),
            cursor: b("cursor"),
        }),
        PresetKind::Audio => Run::Audio(AudioRun {
            mic: b("mic").unwrap_or(true),
            system_audio: b("system_audio").unwrap_or(false),
            transcribe: preset.after.transcribe,
            markers: preset
                .options
                .extra
                .get("markers")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
    }
}

/// The area a screenshot covered, for QC-3's Long Shot and Record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// `rect` is `[x, y, w, h]` in physical px relative to the display.
    Region {
        display_id: u32,
        rect: [i64; 4],
    },
    Window {
        window_id: u32,
    },
    /// The display is not known (a delayed display capture): the primary one is used.
    Display {
        display_id: Option<u32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick::state::{PanelState, Tile};
    use cm_core::preset::starter_presets;
    use serde_json::json;

    fn starter(name: &str) -> Preset {
        starter_presets()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
    }

    /// QC-3.AC1 and AC2: the bar shows only when both Editor and Copy are off.
    #[test]
    fn qc3_after_mode_follows_editor_and_copy() {
        assert_eq!(after_mode(false, false), AfterMode::Bar);
        assert_eq!(after_mode(true, false), AfterMode::Toast);
        assert_eq!(after_mode(false, true), AfterMode::Toast);
        assert_eq!(after_mode(true, true), AfterMode::Toast);
        assert_eq!(TOAST_SECS, 6);
        let mut s = PanelState::default();
        s.toggle(Tile::Copy);
        let Run::Shot(r) = s.run() else { panic!() };
        assert_eq!(r.after_mode(), AfterMode::Bar);
    }

    /// QC-4: a picked preset carries what the tiles cannot (tool, folder, scale).
    #[test]
    fn a_preset_run_carries_its_hidden_fields() {
        let mut s = PanelState::default();
        let mut p = starter("Bug report");
        p.save_to = Some("~/Bugs/{date}".into());
        s.apply_preset(&p);
        let Run::Shot(r) = s.run() else { panic!() };
        assert_eq!(r.tool.as_deref(), Some("step"));
        assert_eq!(r.save_to.as_deref(), Some("~/Bugs/{date}"));
        assert!(r.copy && r.redact && r.editor);
        // Editor off in the panel: the tool is not used.
        s.toggle(Tile::Editor);
        let Run::Shot(r) = s.run() else { panic!() };
        assert_eq!(r.tool, None);
    }

    /// QC-4.AC5 / QC-5: a shortcut run reads the daemon's resolved call, and agrees with the
    /// panel run of the same preset.
    #[test]
    fn a_shortcut_run_matches_the_panel_run() {
        for name in ["Bug report", "Docs shot", "Demo video", "Meeting"] {
            let p = starter(name);
            let mut panel = PanelState::default();
            panel.apply_preset(&p);
            let resolved = match p.kind {
                PresetKind::Screenshot => {
                    let mut m = serde_json::Map::new();
                    cm_core::preset::apply_to_screenshot(&p, &mut m).unwrap();
                    Value::Object(m)
                }
                _ => {
                    let mut m = serde_json::Map::new();
                    cm_core::preset::apply_to_recording(&p, &mut m).unwrap();
                    Value::Object(m)
                }
            };
            assert_eq!(run_from_resolved(&p, &resolved), panel.run(), "{name}");
        }
    }

    #[test]
    fn resolved_values_win_over_the_preset_tiles() {
        let p = starter("Demo video");
        let Run::Video(v) = run_from_resolved(&p, &json!({"mic": false, "countdown_s": 5})) else {
            panic!()
        };
        assert!(!v.mic);
        assert_eq!(v.countdown_s, 5);
    }
}
