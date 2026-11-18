//! Quick Capture (QC-1, QC-3, QC-4): the panel's state and rules, what a run does, where the
//! windows go and how presets meet shortcuts. Display-free and unit tested; the windows are in
//! `ui::quick_window`, `ui::quick_after`, `ui::quick_sheet` and `ui::quick_flow`.

use std::collections::HashMap;

use cm_core::preset::Preset;

pub mod area;
pub mod files;
pub mod place;
pub mod presets;
pub mod run;
pub mod state;

pub use run::{Area, Run, ShotRun};
pub use state::PanelState;

/// A screenshot run in flight (started from the panel or a preset): read by the capture flow
/// when the capture ends (QC-3).
#[derive(Debug, Clone, PartialEq)]
pub struct QuickCtx {
    pub run: ShotRun,
    /// Where the capture was taken, when known.
    pub area: Option<Area>,
    /// The name of the preset the run came from.
    pub preset_name: Option<String>,
}

/// Everything Quick Capture keeps in the app model.
#[derive(Debug, Default)]
pub struct QuickApp {
    pub panel: PanelState,
    /// The presets of `presets.json`, in file order.
    pub presets: Vec<Preset>,
    /// The notice about skipped presets (QC-4.AC4), shown once.
    pub notice: Option<String>,
    pub ctx: Option<QuickCtx>,
    /// Global hotkey id -> preset id, for the presets that have a shortcut.
    pub hotkeys: HashMap<u32, String>,
    /// The audio recording that started from the panel with Transcribe on: the app starts the
    /// transcription when it ends.
    pub transcribe_audio: bool,
    /// A Long Shot run that came from the panel or a preset: its steps wait for the capture.
    pub long_run: Option<ShotRun>,
    /// The chords of the presets' registered global hotkeys.
    pub registered: Vec<crate::shortcuts::Chord>,
    /// The editor tool to select when the editor opens this capture (capture id, tool id).
    pub editor_tool: Option<(String, String)>,
}
