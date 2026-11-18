//! Presets in the app (QC-4): the one-line summary in the user's language, the shortcut
//! conflict check against every other shortcut, and the plan of which preset shortcuts get a
//! global hotkey. Pure; the registration itself is in `ui::quick_flow`.

use std::fmt;

use cm_core::preset::{Preset, PresetKind};
use cm_i18n::{tr, tr_ctx};

use super::state::{kind_label, target_label};
use crate::shortcuts::{Chord, ShortcutAction, ShortcutMap};

/// The one-line summary a picker row shows: `Screenshot · Region · copy, redact`.
pub fn summary_text(p: &Preset) -> String {
    let mut parts = vec![kind_label(p.kind)];
    if let Some(t) = p.target {
        parts.push(target_label(t));
    }
    let (o, a) = (&p.options, &p.after);
    let mut tiles: Vec<String> = Vec::new();
    let on = |v: Option<bool>| v == Some(true);
    if on(o.mic) {
        tiles.push(tr_ctx!("preset summary", "mic"));
    }
    if on(o.system_audio) {
        tiles.push(tr_ctx!("preset summary", "system sound"));
    }
    if on(o.camera) {
        tiles.push(tr_ctx!("preset summary", "webcam"));
    }
    if a.copy_to_clipboard {
        tiles.push(tr_ctx!("preset summary", "copy"));
    }
    if a.auto_redact {
        tiles.push(tr_ctx!("preset summary", "redact"));
    }
    if a.open_in_editor {
        tiles.push(tr_ctx!("preset summary", "editor"));
    }
    if a.transcribe {
        tiles.push(tr_ctx!("preset summary", "transcribe"));
    }
    if !tiles.is_empty() {
        parts.push(tiles.join(", "));
    }
    parts.join(" \u{00B7} ")
}

/// What already owns a chord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Action(ShortcutAction),
    Preset(String),
}

impl fmt::Display for Owner {
    /// The CAP-2.AC2 wording, with the owner named.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Owner::Action(a) => f.write_str(&tr!("Already used by {action}", action = a.label())),
            Owner::Preset(n) => f.write_str(&tr!("Already used by the preset {name}", name = n)),
        }
    }
}

/// The parsed shortcut of a preset, `None` when it has none or cannot be read.
pub fn chord_of(p: &Preset) -> Option<Chord> {
    p.shortcut
        .as_deref()
        .and_then(|s| Chord::parse(s).ok())
        .filter(|c| !c.is_unset())
}

/// QC-4.AC5: may the preset `editing` (its id, `None` for a new one) take `chord`? The chord must
/// differ from every global shortcut and from every other preset's.
pub fn check_preset_chord(
    map: &ShortcutMap,
    presets: &[Preset],
    editing: Option<&str>,
    chord: &Chord,
) -> Result<(), Owner> {
    if let Some((a, _)) = map.iter().find(|(_, c)| *c == chord) {
        return Err(Owner::Action(a));
    }
    if let Some(p) = presets
        .iter()
        .filter(|p| Some(p.id.as_str()) != editing)
        .find(|p| chord_of(p).as_ref() == Some(chord))
    {
        return Err(Owner::Preset(p.name.clone()));
    }
    Ok(())
}

/// A global action about to take `chord`: does a preset own it?
pub fn check_action_chord(presets: &[Preset], chord: &Chord) -> Result<(), Owner> {
    match presets.iter().find(|p| chord_of(p).as_ref() == Some(chord)) {
        Some(p) => Err(Owner::Preset(p.name.clone())),
        None => Ok(()),
    }
}

/// The presets that get a global hotkey: a readable shortcut that no global action uses and that
/// no earlier preset already took (a hand-edited file may hold such a clash; the first wins).
pub fn hotkey_plan(map: &ShortcutMap, presets: &[Preset]) -> Vec<(String, Chord)> {
    let mut out: Vec<(String, Chord)> = Vec::new();
    for p in presets {
        let Some(c) = chord_of(p) else { continue };
        if map.iter().any(|(_, x)| *x == c) || out.iter().any(|(_, x)| *x == c) {
            continue;
        }
        out.push((p.id.clone(), c));
    }
    out
}

/// The kind a preset of `kind` is shown with in the edit sheet's first row.
pub fn kinds() -> [PresetKind; 3] {
    [PresetKind::Screenshot, PresetKind::Video, PresetKind::Audio]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::Platform;
    use cm_core::preset::starter_presets;

    fn with_shortcut(id: &str, chord: &str) -> Vec<Preset> {
        let mut v = starter_presets();
        for p in &mut v {
            if p.id == id {
                p.shortcut = Some(chord.to_owned());
            }
        }
        v
    }

    /// QC-4.AC5: a chord used by another action is refused with that action named.
    #[test]
    fn qc4_ac5_a_chord_of_an_action_is_refused_and_named() {
        let map = ShortcutMap::defaults(Platform::Mac);
        let presets = starter_presets();
        let taken = Chord::parse("alt+shift+1").unwrap();
        let e = check_preset_chord(&map, &presets, Some("bug-report"), &taken).unwrap_err();
        assert_eq!(e, Owner::Action(ShortcutAction::CaptureRegion));
        assert_eq!(e.to_string(), "Already used by Capture region");
        // The new Quick Capture shortcut is one of "every other shortcut".
        let qc = map.get(ShortcutAction::OpenQuickCapture);
        assert_eq!(
            check_preset_chord(&map, &presets, None, &qc),
            Err(Owner::Action(ShortcutAction::OpenQuickCapture))
        );
        let free = Chord::parse("alt+shift+b").unwrap();
        assert!(check_preset_chord(&map, &presets, Some("bug-report"), &free).is_ok());
    }

    /// QC-4.AC5: two presets cannot share a chord; a preset may keep its own.
    #[test]
    fn qc4_ac5_presets_do_not_share_a_chord() {
        let map = ShortcutMap::defaults(Platform::Windows);
        let presets = with_shortcut("bug-report", "ctrl+alt+b");
        let c = Chord::parse("ctrl+alt+b").unwrap();
        let e = check_preset_chord(&map, &presets, Some("meeting"), &c).unwrap_err();
        assert_eq!(e, Owner::Preset("Bug report".into()));
        assert_eq!(e.to_string(), "Already used by the preset Bug report");
        assert!(check_preset_chord(&map, &presets, Some("bug-report"), &c).is_ok());
        assert_eq!(check_action_chord(&presets, &c), Err(e));
    }

    #[test]
    fn the_hotkey_plan_skips_clashes_and_unreadable_chords() {
        let map = ShortcutMap::defaults(Platform::Mac);
        let mut presets = with_shortcut("bug-report", "alt+shift+b");
        presets[1].shortcut = Some("alt+shift+b".into()); // a hand-edited clash
        presets[2].shortcut = Some("alt+shift+2".into()); // a global action's
        presets[3].shortcut = Some("banana".into());
        let plan = hotkey_plan(&map, &presets);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, "bug-report");
    }

    #[test]
    fn the_summary_names_kind_target_and_tiles() {
        let p = &starter_presets()[0];
        assert_eq!(
            summary_text(p),
            "Screenshot \u{00B7} Region \u{00B7} copy, redact, editor"
        );
        assert_eq!(
            summary_text(&starter_presets()[3]),
            "Audio \u{00B7} mic, system sound, transcribe"
        );
    }
}
