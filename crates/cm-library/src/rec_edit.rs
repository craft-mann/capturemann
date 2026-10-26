//! Edits of a recording (EDT-1 to EDT-3) as persisted data.
//!
//! The edit list lives in `manifest.edits[0]` (shape: `cm_core::edit`). Every change is one
//! revision (`trim`, `cut_segments`, ... or `undo`/`redo`) tagged with who made it, and one step
//! in `manifest.edit_history`:
//!
//! ```json
//! "edit_history": { "cursor": 2, "steps": [
//!   { "rev": 2, "action": "trim", "by": "user", "before": {..edits[0]..}, "after": {..} }, ... ] }
//! ```
//!
//! `cursor` counts the applied steps: undo restores `steps[cursor - 1].before`, redo re-applies
//! `steps[cursor].after`, and a new edit drops the steps after the cursor (linear undo, AGT-3).
//! Each step holds both full edit entries; they are a few hundred bytes. The master tracks are
//! never opened for writing.

use std::path::Path;

use cm_core::{Actor, CmError, EditContext, EditList, MediaTime, Revision, DEFAULT_REVISION_LIMIT};
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::manifest::{Marker, RecManifest};
use crate::rec::{read_manifest, write_manifest};

/// Reserved top-level manifest key holding the undo history.
pub const HISTORY_KEY: &str = "edit_history";

/// The caption lines of a recording (AI-1): the `lines` array inside the manifest's `captions`
/// object (`{ "enabled", "burn_in", "lines": [{ "start", "end", "text" }, ...] }`, times in
/// ticks). They are undoable like an edit: a `set_captions` step keeps the `captions` object
/// before and after it in `edit_history` (`captions_before`, `captions_after`), and `undo` /
/// `redo` restore it.
pub fn captions_of(m: &RecManifest) -> Option<&Value> {
    m.captions.get("lines").filter(|l| l.is_array())
}

fn ticks(v: Option<&Value>) -> Option<MediaTime> {
    v.and_then(Value::as_i64).map(MediaTime::from_ticks)
}

/// Session time at which the recording ends: the screen track's end, else the latest track end,
/// else the master length.
pub fn session_end(m: &RecManifest) -> MediaTime {
    let end_of = |t: &Value| {
        (t.get("missing") != Some(&json!(true)))
            .then(|| ticks(t.get("end")))
            .flatten()
    };
    m.tracks
        .get("screen")
        .and_then(end_of)
        .or_else(|| m.tracks.values().filter_map(end_of).max())
        .or(m.duration)
        .unwrap_or(MediaTime::ZERO)
}

/// `live_at` of the manifest, the default in-point.
pub fn live_at(m: &RecManifest) -> Option<MediaTime> {
    ticks(m.extra.get("live_at"))
}

/// The recording's edit list (the default list when `edits` is empty; legacy entries migrate).
pub fn load_edits(m: &RecManifest) -> EditList {
    let cx = EditContext {
        end: session_end(m),
        live_at: live_at(m),
        gaps: &m.gaps,
    };
    match m.edits.first() {
        Some(e) => EditList::from_json(e, &cx),
        None => EditList::full(&cx),
    }
}

/// `(can_undo, can_redo)` of a manifest's edit history.
pub fn history_flags(m: &RecManifest) -> (bool, bool) {
    let h = m.extra.get(HISTORY_KEY);
    let cursor = h.and_then(|h| h["cursor"].as_u64()).unwrap_or(0) as usize;
    let len = h.and_then(|h| h["steps"].as_array()).map_or(0, Vec::len);
    (cursor > 0, cursor < len)
}

/// What one edit, undo or redo did.
#[derive(Debug, Clone)]
pub struct EditOutcome {
    pub revision: Revision,
    pub edits: EditList,
    /// Output length after the change.
    pub duration: MediaTime,
    pub can_undo: bool,
    pub can_redo: bool,
}

fn outcome(m: &RecManifest, revision: Revision) -> EditOutcome {
    let edits = load_edits(m);
    let (can_undo, can_redo) = history_flags(m);
    EditOutcome {
        // Pauses are not in the media, so they are not in the output (EDT-1, REC-4).
        duration: edits.without_pauses(&m.gaps).timeline().duration(),
        revision,
        edits,
        can_undo,
        can_redo,
    }
}

fn push_revision(m: &mut RecManifest, action: &str, by: Actor, at: OffsetDateTime) -> Revision {
    let rev = Revision {
        rev: m.revisions.last().map_or(0, |r| r.rev) + 1,
        action: action.to_owned(),
        by,
        at,
    };
    m.revisions.push(rev.clone());
    let over = m.revisions.len().saturating_sub(DEFAULT_REVISION_LIMIT);
    if over > 0 {
        m.revisions.drain(..over);
    }
    rev
}

fn set_edits0(m: &mut RecManifest, v: Value) {
    if m.edits.is_empty() {
        m.edits.push(v);
    } else {
        m.edits[0] = v;
    }
}

fn history_mut(m: &mut RecManifest) -> Result<(usize, Vec<Value>), CmError> {
    let h = m.extra.get(HISTORY_KEY);
    let steps = h
        .and_then(|h| h["steps"].as_array())
        .cloned()
        .unwrap_or_default();
    let cursor = h
        .and_then(|h| h["cursor"].as_u64())
        .map_or(0, |c| (c as usize).min(steps.len()));
    Ok((cursor, steps))
}

fn store_history(m: &mut RecManifest, cursor: usize, steps: Vec<Value>) {
    m.extra.insert(
        HISTORY_KEY.into(),
        json!({"cursor": cursor, "steps": steps}),
    );
}

/// Applies one edit as a single revision and undo step. `edit` receives the current list and the
/// recording's bounds and returns the new list or an error (nothing is written then).
pub fn apply_edit(
    dir: &Path,
    action: &str,
    by: Actor,
    at: OffsetDateTime,
    edit: impl FnOnce(&EditList, &EditContext<'_>) -> Result<EditList, CmError>,
) -> Result<EditOutcome, CmError> {
    let mut m = read_manifest(dir)?;
    let cx = EditContext {
        end: session_end(&m),
        live_at: live_at(&m),
        gaps: &m.gaps.clone(),
    };
    let before_list = load_edits(&m);
    let after_list = edit(&before_list, &cx)?;
    let before = before_list.to_json();
    let after = after_list.to_json();
    let (cursor, mut steps) = history_mut(&mut m)?;
    steps.truncate(cursor);
    set_edits0(&mut m, after.clone());
    let revision = push_revision(&mut m, action, by.clone(), at);
    steps.push(json!({
        "rev": revision.rev,
        "action": action,
        "by": by.to_string(),
        "before": before,
        "after": after,
    }));
    let over = steps.len().saturating_sub(DEFAULT_REVISION_LIMIT);
    steps.drain(..over);
    let cursor = steps.len();
    store_history(&mut m, cursor, steps);
    write_manifest(dir, &m)?;
    Ok(outcome(&m, revision))
}

/// Stores edited caption lines (AI-1) as one revision (`set_captions`) and one undo step. `lines`
/// is the JSON array of `{ start, end, text }` (ticks) the caller validated; an empty array clears
/// the captions. The edit list itself is unchanged.
pub fn apply_captions(
    dir: &Path,
    lines: Value,
    by: Actor,
    at: OffsetDateTime,
) -> Result<EditOutcome, CmError> {
    let mut m = read_manifest(dir)?;
    let before_captions = m.captions.clone();
    let mut after = match &before_captions {
        Value::Object(o) => o.clone(),
        _ => serde_json::Map::new(),
    };
    if lines.as_array().is_some_and(Vec::is_empty) {
        after.remove("lines");
    } else {
        after.insert("lines".into(), lines);
        // Editing lines is the user engaging with captions: the first-transcript default never
        // switches anything on afterwards.
        after.insert(OPTIONS_SET.into(), json!(true));
    }
    let after_captions = Value::Object(after);
    let edits_now = load_edits(&m).to_json();
    let (cursor, mut steps) = history_mut(&mut m)?;
    steps.truncate(cursor);
    m.captions = after_captions.clone();
    let revision = push_revision(&mut m, "set_captions", by.clone(), at);
    steps.push(json!({
        "rev": revision.rev,
        "action": "set_captions",
        "by": by.to_string(),
        "before": edits_now,
        "after": edits_now,
        "captions_before": before_captions,
        "captions_after": after_captions,
    }));
    let over = steps.len().saturating_sub(DEFAULT_REVISION_LIMIT);
    steps.drain(..over);
    let cursor = steps.len();
    store_history(&mut m, cursor, steps);
    write_manifest(dir, &m)?;
    Ok(outcome(&m, revision))
}

/// Longest marker label, in characters.
pub const MARKER_LABEL_MAX: usize = 200;

/// Adds a marker (VOC-1) to a finished recording as one revision (`add_marker`) and one undo
/// step; the edit list and the media are untouched. `at` is the recorded time (the HUD timer,
/// pauses removed) and must lie within the recording. Markers stay sorted by `at`. Returns the
/// marker as stored and the outcome.
pub fn apply_marker(
    dir: &Path,
    at: MediaTime,
    label: Option<&str>,
    by: Actor,
    wall: OffsetDateTime,
) -> Result<(Marker, EditOutcome), CmError> {
    let mut m = read_manifest(dir)?;
    if at < MediaTime::ZERO {
        return Err(CmError::invalid_field("at", "must not be negative"));
    }
    if let Some(d) = m.duration.filter(|d| at > *d) {
        return Err(CmError::invalid_field(
            "at",
            format!(
                "is after the end of the recording ({:.3} s)",
                d.as_secs_f64()
            ),
        ));
    }
    let label = normalize_marker_label(label)?;
    let marker = Marker {
        at,
        label,
        by: Some(by.to_string()),
    };
    let before = serde_json::to_value(&m.markers).unwrap_or(Value::Null);
    m.markers.push(marker.clone());
    m.markers.sort_by_key(|x| x.at);
    let after = serde_json::to_value(&m.markers).unwrap_or(Value::Null);
    let edits_now = load_edits(&m).to_json();
    let (cursor, mut steps) = history_mut(&mut m)?;
    steps.truncate(cursor);
    let revision = push_revision(&mut m, "add_marker", by.clone(), wall);
    steps.push(json!({
        "rev": revision.rev,
        "action": "add_marker",
        "by": by.to_string(),
        "before": edits_now,
        "after": edits_now,
        "markers_before": before,
        "markers_after": after,
    }));
    let over = steps.len().saturating_sub(DEFAULT_REVISION_LIMIT);
    steps.drain(..over);
    let cursor = steps.len();
    store_history(&mut m, cursor, steps);
    write_manifest(dir, &m)?;
    Ok((marker, outcome(&m, revision)))
}

/// Trims a marker label; an empty one means none. At most [`MARKER_LABEL_MAX`] characters.
pub fn normalize_marker_label(label: Option<&str>) -> Result<Option<String>, CmError> {
    let label = label.map(str::trim).filter(|l| !l.is_empty());
    if let Some(l) = label {
        if l.chars().count() > MARKER_LABEL_MAX {
            return Err(CmError::invalid_field(
                "label",
                format!("must be at most {MARKER_LABEL_MAX} characters"),
            ));
        }
    }
    Ok(label.map(str::to_owned))
}

/// Sets top-level manifest keys (`None` removes a key) as ONE revision named `action` and one undo
/// step; `undo` / `redo` restore the keys as they were before / after (`extra_before`,
/// `extra_after` in the step). The edit list is unchanged. Used for the speaker labels (AI-8:
/// `speakers`, `speaker_segments`). The keys must not be ones the manifest struct owns.
pub fn apply_extra(
    dir: &Path,
    action: &str,
    changes: Vec<(String, Option<Value>)>,
    by: Actor,
    at: OffsetDateTime,
) -> Result<EditOutcome, CmError> {
    let mut m = read_manifest(dir)?;
    let mut before = serde_json::Map::new();
    let mut after = serde_json::Map::new();
    for (key, value) in changes {
        before.insert(
            key.clone(),
            m.extra.get(&key).cloned().unwrap_or(Value::Null),
        );
        match value {
            Some(v) => {
                m.extra.insert(key.clone(), v.clone());
                after.insert(key, v);
            }
            None => {
                m.extra.remove(&key);
                after.insert(key, Value::Null);
            }
        }
    }
    let edits_now = load_edits(&m).to_json();
    let (cursor, mut steps) = history_mut(&mut m)?;
    steps.truncate(cursor);
    let revision = push_revision(&mut m, action, by.clone(), at);
    steps.push(json!({
        "rev": revision.rev,
        "action": action,
        "by": by.to_string(),
        "before": edits_now,
        "after": edits_now,
        "extra_before": before,
        "extra_after": after,
    }));
    let over = steps.len().saturating_sub(DEFAULT_REVISION_LIMIT);
    steps.drain(..over);
    let cursor = steps.len();
    store_history(&mut m, cursor, steps);
    write_manifest(dir, &m)?;
    Ok(outcome(&m, revision))
}

/// Sets top-level manifest keys WITHOUT a revision or an undo step (`None` removes a key): for
/// data a job produced, like the speakers of a new diarization (AI-8), where nothing was edited
/// by the user. A previous undo step that touched the same keys is dropped from the history so
/// that undo can never bring back data of an older run.
pub fn set_extra(dir: &Path, changes: Vec<(String, Option<Value>)>) -> Result<(), CmError> {
    let mut m = read_manifest(dir)?;
    let keys: Vec<String> = changes.iter().map(|(k, _)| k.clone()).collect();
    for (key, value) in changes {
        match value {
            Some(v) => {
                m.extra.insert(key, v);
            }
            None => {
                m.extra.remove(&key);
            }
        }
    }
    let (cursor, steps) = history_mut(&mut m)?;
    let touches = |s: &Value| {
        s.get("extra_before")
            .and_then(Value::as_object)
            .is_some_and(|o| keys.iter().any(|k| o.contains_key(k)))
    };
    if steps.iter().any(touches) {
        let kept_before_cursor = steps.iter().take(cursor).filter(|s| !touches(s)).count();
        let kept: Vec<Value> = steps.into_iter().filter(|s| !touches(s)).collect();
        store_history(&mut m, kept_before_cursor, kept);
    }
    write_manifest(dir, &m)
}

/// How captions are used for a recording (AI-1): the two switches in the manifest's `captions`
/// object. Both default to off when the keys are missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptionOptions {
    /// Show the captions on the video in the editor's preview.
    pub enabled: bool,
    /// Burn the captions into exported videos by default (an export can override it).
    pub burn_in: bool,
}

/// Marker in the manifest's `captions` object: the user chose a switch or edited caption lines, so
/// the app never changes the switches by itself afterwards.
const OPTIONS_SET: &str = "options_set";

/// The caption switches of a recording.
pub fn caption_options(m: &RecManifest) -> CaptionOptions {
    let flag = |k: &str| m.captions.get(k).and_then(Value::as_bool).unwrap_or(false);
    CaptionOptions {
        enabled: flag("enabled"),
        burn_in: flag("burn_in"),
    }
}

/// Sets the caption switches (`None` keeps a value) as one revision (`set_caption_options`) and
/// one undo step; the caption lines and the edit list are untouched. A call that changes nothing
/// still records no revision and returns `None`.
pub fn apply_caption_options(
    dir: &Path,
    enabled: Option<bool>,
    burn_in: Option<bool>,
    by: Actor,
    at: OffsetDateTime,
) -> Result<Option<EditOutcome>, CmError> {
    let mut m = read_manifest(dir)?;
    let now = caption_options(&m);
    let want = CaptionOptions {
        enabled: enabled.unwrap_or(now.enabled),
        burn_in: burn_in.unwrap_or(now.burn_in),
    };
    if want == now {
        // Nothing to change, but an explicit choice still counts as "the user decided": remember
        // it (no revision) so the first-transcript default leaves their value alone.
        let explicit = enabled.is_some() || burn_in.is_some();
        if explicit && m.captions.get(OPTIONS_SET) != Some(&json!(true)) {
            let mut obj = match &m.captions {
                Value::Object(o) => o.clone(),
                _ => serde_json::Map::new(),
            };
            obj.insert(OPTIONS_SET.into(), json!(true));
            m.captions = Value::Object(obj);
            write_manifest(dir, &m)?;
        }
        return Ok(None);
    }
    let before_captions = m.captions.clone();
    let mut after = match &before_captions {
        Value::Object(o) => o.clone(),
        _ => serde_json::Map::new(),
    };
    after.insert("enabled".into(), json!(want.enabled));
    after.insert("burn_in".into(), json!(want.burn_in));
    after.insert(OPTIONS_SET.into(), json!(true));
    let after_captions = Value::Object(after);
    let edits_now = load_edits(&m).to_json();
    let (cursor, mut steps) = history_mut(&mut m)?;
    steps.truncate(cursor);
    m.captions = after_captions.clone();
    let revision = push_revision(&mut m, "set_caption_options", by.clone(), at);
    steps.push(json!({
        "rev": revision.rev,
        "action": "set_caption_options",
        "by": by.to_string(),
        "before": edits_now,
        "after": edits_now,
        "captions_before": before_captions,
        "captions_after": after_captions,
    }));
    let over = steps.len().saturating_sub(DEFAULT_REVISION_LIMIT);
    steps.drain(..over);
    let cursor = steps.len();
    store_history(&mut m, cursor, steps);
    write_manifest(dir, &m)?;
    Ok(Some(outcome(&m, revision)))
}

/// The default after a recording's FIRST transcript: show the captions in the editor's preview
/// (`enabled = true`), leave `burn_in` as it is (exports stay caption-free until the owner opts
/// in). It changes nothing when the user already decided: a switch was set explicitly (any
/// `set_caption_options` call that named one, even to the value it had) or caption lines were
/// edited (the `options_set` marker), or captions are already on. It is not a revision (nothing to
/// undo: the user has not acted yet). `true` when it switched the preview captions on.
pub fn default_captions_on_first_transcript(dir: &Path) -> Result<bool, CmError> {
    let mut m = read_manifest(dir)?;
    let mut obj = match &m.captions {
        Value::Object(o) => o.clone(),
        _ => serde_json::Map::new(),
    };
    let decided = obj.get(OPTIONS_SET) == Some(&json!(true))
        || obj.get("lines").is_some_and(Value::is_array)
        || obj.get("enabled") == Some(&json!(true));
    if decided {
        return Ok(false);
    }
    obj.insert("enabled".into(), json!(true));
    m.captions = Value::Object(obj);
    write_manifest(dir, &m)?;
    Ok(true)
}

/// Undoes (`undo`) or redoes the last edit as one revision. `None` when there is nothing to do.
pub fn step_history(
    dir: &Path,
    undo: bool,
    by: Actor,
    at: OffsetDateTime,
) -> Result<Option<EditOutcome>, CmError> {
    let mut m = read_manifest(dir)?;
    let (cursor, steps) = history_mut(&mut m)?;
    let (target, new_cursor) = if undo {
        match cursor.checked_sub(1).and_then(|i| steps.get(i)) {
            Some(s) => (s["before"].clone(), cursor - 1),
            None => return Ok(None),
        }
    } else {
        match steps.get(cursor) {
            Some(s) => (s["after"].clone(), cursor + 1),
            None => return Ok(None),
        }
    };
    set_edits0(&mut m, target);
    // A caption step carries the lines on both sides of it; restore the matching side.
    let step = if undo { cursor - 1 } else { cursor };
    let side = if undo {
        "captions_before"
    } else {
        "captions_after"
    };
    if let Some(c) = steps.get(step).and_then(|s| s.get(side)) {
        m.captions = c.clone();
    }
    // A marker step carries the marker list on both sides of it.
    let mside = if undo {
        "markers_before"
    } else {
        "markers_after"
    };
    if let Some(list) = steps.get(step).and_then(|s| s.get(mside)) {
        m.markers = serde_json::from_value(list.clone()).unwrap_or_default();
    }
    // A step from `apply_extra` carries the keys it set on both sides (null = absent).
    let extra_side = if undo { "extra_before" } else { "extra_after" };
    if let Some(keys) = steps
        .get(step)
        .and_then(|s| s.get(extra_side))
        .and_then(Value::as_object)
    {
        for (k, v) in keys {
            if v.is_null() {
                m.extra.remove(k);
            } else {
                m.extra.insert(k.clone(), v.clone());
            }
        }
    }
    let revision = push_revision(&mut m, if undo { "undo" } else { "redo" }, by, at);
    store_history(&mut m, new_cursor, steps);
    write_manifest(dir, &m)?;
    Ok(Some(outcome(&m, revision)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rec::{RecBundle, SessionInfo};
    use cm_core::{BackgroundKind, BackgroundUpdate, Span, TrackInfo, TrackKind, TrackStats};
    use serde_json::Map;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-10-22 10:00:00 UTC);

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn span(a: f64, b: f64) -> Span {
        Span::new(t(a), t(b))
    }

    /// A finished 60 s recording with a screen master on disk.
    fn recording(tmp: &Path) -> PathBuf {
        let session = SessionInfo {
            clock_origin_ns: 1,
            timebase: "uptime_raw_ns".into(),
            displays: vec![],
            devices: json!({}),
            app_version: "0.0.1".into(),
            extra: Map::new(),
        };
        let b = RecBundle::create(tmp, "Lesson", NOW, json!({"fps": 30}), &session).unwrap();
        std::fs::write(b.dir().join("screen.mp4"), b"master bytes").unwrap();
        let dir = b.dir().to_path_buf();
        let info = TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: t(0.0),
            end: t(60.0),
            details: json!({}),
            stats: TrackStats::default(),
        };
        b.finalize(&[info], NOW).unwrap();
        dir
    }

    fn hash(p: &Path) -> Vec<u8> {
        Sha256::digest(std::fs::read(p).unwrap()).to_vec()
    }

    fn cut(dir: &Path, ranges: &[Span], by: Actor) -> Result<EditOutcome, CmError> {
        apply_edit(dir, "cut_segments", by, NOW, |l, _| l.cut(ranges))
    }

    /// AI-8.AC4/AC5: a change to top-level keys is one revision and one undo step; undo and redo
    /// restore them (an absent key stays absent).
    #[test]
    fn ai_8_extra_keys_are_one_undoable_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let one = |v: &str| vec![("speakers".to_owned(), Some(json!([{"id": 1, "name": v}])))];
        let o = apply_extra(&dir, "set_speakers", one("Speaker 1"), Actor::User, NOW).unwrap();
        assert_eq!(o.revision.action, "set_speakers");
        let o = apply_extra(
            &dir,
            "set_speakers",
            one("Linh"),
            Actor::agent("claude").unwrap(),
            NOW,
        )
        .unwrap();
        assert_eq!(o.revision.by.to_string(), "agent:claude");
        let name = |d: &Path| read_manifest(d).unwrap().extra["speakers"][0]["name"].clone();
        assert_eq!(name(&dir), "Linh");
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert_eq!(name(&dir), "Speaker 1");
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert!(!read_manifest(&dir).unwrap().extra.contains_key("speakers"));
        step_history(&dir, false, Actor::User, NOW)
            .unwrap()
            .unwrap();
        step_history(&dir, false, Actor::User, NOW)
            .unwrap()
            .unwrap();
        assert_eq!(name(&dir), "Linh");
        // Removing a key is undoable too.
        apply_extra(
            &dir,
            "set_speakers",
            vec![("speakers".into(), None)],
            Actor::User,
            NOW,
        )
        .unwrap();
        assert!(!read_manifest(&dir).unwrap().extra.contains_key("speakers"));
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert_eq!(name(&dir), "Linh");
        // A new run replaces the keys without a revision, and no undo brings the old ones back.
        set_extra(&dir, vec![("speakers".into(), Some(json!([])))]).unwrap();
        assert_eq!(read_manifest(&dir).unwrap().extra["speakers"], json!([]));
        assert!(step_history(&dir, true, Actor::User, NOW)
            .unwrap()
            .is_none());
    }

    /// AI-1: the first transcript switches the preview captions on and leaves burn-in off, once,
    /// and never after the user chose (a switch named in `set_caption_options`, even to the
    /// value it had, or edited caption lines).
    #[test]
    fn ai_1_first_transcript_default_respects_the_user() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        assert!(default_captions_on_first_transcript(&dir).unwrap());
        let o = caption_options(&read_manifest(&dir).unwrap());
        assert!(o.enabled && !o.burn_in, "preview on, export off: {o:?}");
        assert!(
            !default_captions_on_first_transcript(&dir).unwrap(),
            "already on: nothing to do"
        );
        // No revision was recorded: there is nothing for the user to undo.
        assert!(!has_history(&dir));

        // The user turned captions off: a later (regenerated) transcript leaves that alone.
        apply_caption_options(&dir, Some(false), None, Actor::User, NOW).unwrap();
        assert!(has_history(&dir), "an explicit change IS a revision");
        assert!(!default_captions_on_first_transcript(&dir).unwrap());
        assert!(!caption_options(&read_manifest(&dir).unwrap()).enabled);

        // Explicitly choosing the value the switch already had also counts as a decision.
        let tmp2 = tempfile::tempdir().unwrap();
        let dir2 = recording(tmp2.path());
        assert!(
            apply_caption_options(&dir2, Some(false), None, Actor::User, NOW)
                .unwrap()
                .is_none()
        );
        assert!(!default_captions_on_first_transcript(&dir2).unwrap());

        // Edited lines are a decision too.
        let tmp3 = tempfile::tempdir().unwrap();
        let dir3 = recording(tmp3.path());
        let lines = json!([{"start": 120000, "end": 480000, "text": "Hi"}]);
        apply_captions(&dir3, lines, Actor::User, NOW).unwrap();
        assert!(!default_captions_on_first_transcript(&dir3).unwrap());
    }

    fn has_history(dir: &Path) -> bool {
        let m = read_manifest(dir).unwrap();
        m.extra
            .get(HISTORY_KEY)
            .and_then(|h| h.get("steps"))
            .and_then(Value::as_array)
            .is_some_and(|s| !s.is_empty())
    }

    /// AI-1: show and burn-in are one revision and one undo step each; a call that changes nothing
    /// records nothing; the caption lines survive; undo and redo restore both switches.
    #[test]
    fn ai_1_caption_options_are_one_undoable_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let lines = json!([{"start": 120000, "end": 480000, "text": "Hello there"}]);
        apply_captions(&dir, lines.clone(), Actor::User, NOW).unwrap();
        assert_eq!(
            caption_options(&read_manifest(&dir).unwrap()),
            CaptionOptions::default()
        );
        let o = apply_caption_options(&dir, Some(true), None, Actor::Agent("claude".into()), NOW)
            .unwrap()
            .unwrap();
        assert_eq!(o.revision.action, "set_caption_options");
        let m = read_manifest(&dir).unwrap();
        assert_eq!(
            caption_options(&m),
            CaptionOptions {
                enabled: true,
                burn_in: false
            }
        );
        assert_eq!(captions_of(&m), Some(&lines), "the lines are kept");
        // No change: no revision.
        let rev = m.revisions.len();
        assert!(
            apply_caption_options(&dir, Some(true), None, Actor::User, NOW)
                .unwrap()
                .is_none()
        );
        assert_eq!(read_manifest(&dir).unwrap().revisions.len(), rev);
        // Burn in too, then undo it, then undo the first one.
        apply_caption_options(&dir, None, Some(true), Actor::User, NOW)
            .unwrap()
            .unwrap();
        let both = caption_options(&read_manifest(&dir).unwrap());
        assert!(both.enabled && both.burn_in);
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        let one = caption_options(&read_manifest(&dir).unwrap());
        assert!(one.enabled && !one.burn_in);
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert_eq!(
            caption_options(&read_manifest(&dir).unwrap()),
            CaptionOptions::default()
        );
        step_history(&dir, false, Actor::User, NOW)
            .unwrap()
            .unwrap();
        step_history(&dir, false, Actor::User, NOW)
            .unwrap()
            .unwrap();
        assert_eq!(caption_options(&read_manifest(&dir).unwrap()), both);
        assert_eq!(
            captions_of(&read_manifest(&dir).unwrap()),
            Some(&lines),
            "the lines survive undo and redo of the switches"
        );
    }

    /// AI-1: caption lines are one revision and one undo step; `undo` restores the lines before
    /// (none), `redo` the edited ones, and the edit list and the masters are untouched.
    #[test]
    fn ai_1_set_captions_is_one_undoable_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let master = hash(&dir.join("screen.mp4"));
        let before = load_edits(&read_manifest(&dir).unwrap()).to_json();
        let lines = json!([{"start": 120000, "end": 480000, "text": "Hello there"}]);
        let o = apply_captions(&dir, lines.clone(), Actor::Agent("claude".into()), NOW).unwrap();
        assert_eq!(o.revision.action, "set_captions");
        assert!(o.can_undo && !o.can_redo);
        let m = read_manifest(&dir).unwrap();
        assert_eq!(captions_of(&m), Some(&lines));
        assert_eq!(
            load_edits(&m).to_json(),
            before,
            "the edit list is unchanged"
        );
        // Undo: no captions again; redo: the edited lines.
        let u = step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert_eq!(u.revision.action, "undo");
        assert!(captions_of(&read_manifest(&dir).unwrap()).is_none());
        step_history(&dir, false, Actor::User, NOW)
            .unwrap()
            .unwrap();
        assert_eq!(
            captions_of(&read_manifest(&dir).unwrap()).cloned().unwrap(),
            lines
        );
        // A second edit of the lines, then undo goes back to the first version.
        let lines2 = json!([{"start": 120000, "end": 600000, "text": "Hello again"}]);
        apply_captions(&dir, lines2.clone(), Actor::User, NOW).unwrap();
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert_eq!(
            captions_of(&read_manifest(&dir).unwrap()).cloned().unwrap(),
            lines
        );
        // An empty array clears them (still undoable).
        apply_captions(&dir, json!([]), Actor::User, NOW).unwrap();
        assert!(captions_of(&read_manifest(&dir).unwrap()).is_none());
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert!(captions_of(&read_manifest(&dir).unwrap()).is_some());
        assert_eq!(hash(&dir.join("screen.mp4")), master);
    }

    /// EDT-1 / REC-4: the reported duration leaves out the pauses, like the export.
    #[test]
    fn reported_duration_excludes_pause_gaps() {
        let tmp = tempfile::tempdir().unwrap();
        let session = SessionInfo {
            clock_origin_ns: 1,
            timebase: "uptime_raw_ns".into(),
            displays: vec![],
            devices: json!({}),
            app_version: "0.0.1".into(),
            extra: Map::new(),
        };
        let mut b =
            RecBundle::create(tmp.path(), "Paused", NOW, json!({"fps": 30}), &session).unwrap();
        b.add_gap(cm_core::PauseGap {
            from: t(20.0),
            to: t(30.0),
        })
        .unwrap();
        std::fs::write(b.dir().join("screen.mp4"), b"master bytes").unwrap();
        let dir = b.dir().to_path_buf();
        let info = TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: t(0.0),
            end: t(60.0),
            details: json!({}),
            stats: TrackStats::default(),
        };
        b.finalize(&[info], NOW).unwrap();
        let o = cut(&dir, &[span(40.0, 45.0)], Actor::User).unwrap();
        // 60 s - 10 s pause - 5 s cut.
        assert_eq!(o.duration, t(45.0));
    }

    /// A fresh recording reads as one full segment.
    #[test]
    fn a_new_recording_is_one_full_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let m = read_manifest(&dir).unwrap();
        let l = load_edits(&m);
        assert_eq!((l.start, l.end), (t(0.0), t(60.0)));
        assert!(l.cuts.is_empty());
        assert_eq!(history_flags(&m), (false, false));
    }

    /// EDT-1.AC4: trims and cuts never change a master file.
    #[test]
    fn edt1_ac4_masters_stay_byte_identical() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let before = hash(&dir.join("screen.mp4"));
        apply_edit(&dir, "trim", Actor::User, NOW, |l, cx| {
            l.trim(t(2.0), t(58.0), cx)
        })
        .unwrap();
        cut(&dir, &[span(10.0, 15.0), span(20.0, 22.0)], Actor::User).unwrap();
        step_history(&dir, true, Actor::User, NOW).unwrap();
        assert_eq!(hash(&dir.join("screen.mp4")), before);
    }

    /// EDT-1.AC1: the stored trim exports 56 s.
    #[test]
    fn edt1_ac1_trim_is_stored_and_measured() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let o = apply_edit(&dir, "trim", Actor::User, NOW, |l, cx| {
            l.trim(t(2.0), t(58.0), cx)
        })
        .unwrap();
        assert_eq!(o.duration, t(56.0));
        let m = read_manifest(&dir).unwrap();
        assert_eq!(m.edits[0]["in"], t(2.0).ticks());
        assert_eq!(m.edits[0]["version"], 2);
    }

    /// EDT-1.AC3 and AGT-3: each edit is one revision and one undo step; undo/redo walk them.
    #[test]
    fn edt1_ac3_each_edit_is_one_undo_step() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let r0 = read_manifest(&dir).unwrap().revisions.len();
        cut(&dir, &[span(10.0, 15.0)], Actor::User).unwrap();
        let o = apply_edit(&dir, "restore_segments", Actor::User, NOW, |l, _| {
            l.restore(&[span(10.0, 15.0)])
        })
        .unwrap();
        assert_eq!(o.revision.action, "restore_segments");
        assert_eq!(read_manifest(&dir).unwrap().revisions.len(), r0 + 2);
        let u = step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert_eq!(u.edits.cuts, vec![span(10.0, 15.0)]);
        assert!(u.can_undo && u.can_redo);
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        let m = read_manifest(&dir).unwrap();
        assert!(load_edits(&m).cuts.is_empty());
        assert!(step_history(&dir, true, Actor::User, NOW)
            .unwrap()
            .is_none());
        step_history(&dir, false, Actor::User, NOW)
            .unwrap()
            .unwrap();
        assert_eq!(
            load_edits(&read_manifest(&dir).unwrap()).cuts,
            vec![span(10.0, 15.0)]
        );
        // A new edit drops the redo tail.
        apply_edit(&dir, "split", Actor::User, NOW, |l, _| l.split(t(30.0))).unwrap();
        assert!(!history_flags(&read_manifest(&dir).unwrap()).1);
    }

    /// EDT-1.AC5: three ranges in one call are one revision and one undo.
    #[test]
    fn edt1_ac5_three_ranges_are_one_step() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let agent = Actor::agent("claude").unwrap();
        let o = cut(
            &dir,
            &[span(1.0, 2.0), span(5.0, 6.0), span(9.0, 10.0)],
            agent.clone(),
        )
        .unwrap();
        assert_eq!(o.revision.by, agent);
        assert_eq!(o.edits.cuts.len(), 3);
        step_history(&dir, true, Actor::User, NOW).unwrap().unwrap();
        assert!(load_edits(&read_manifest(&dir).unwrap()).cuts.is_empty());
        let m = read_manifest(&dir).unwrap();
        assert_eq!(
            m.revisions[m.revisions.len() - 2].by.to_string(),
            "agent:claude"
        );
    }

    /// A refused edit writes nothing.
    #[test]
    fn a_refused_edit_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        let before = std::fs::read(dir.join("manifest.json")).unwrap();
        let e = cut(&dir, &[span(0.1, 60.0)], Actor::User).unwrap_err();
        assert!(e.to_string().contains("Nothing left to export"));
        assert_eq!(std::fs::read(dir.join("manifest.json")).unwrap(), before);
    }

    /// EDT-2 and EDT-3 persist next to the layout the recorder wrote.
    #[test]
    fn speed_and_background_persist() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = recording(tmp.path());
        apply_edit(&dir, "set_speed", Actor::User, NOW, |l, _| {
            l.set_speed(span(10.0, 30.0), 2.0, false)
        })
        .unwrap();
        apply_edit(&dir, "set_background", Actor::User, NOW, |l, _| {
            l.set_background(&BackgroundUpdate {
                kind: BackgroundKind::Color,
                value: Some(json!("#112233")),
                padding_pct: Some(8.0),
                radius_px: Some(12),
                shadow: Some(true),
            })
        })
        .unwrap();
        let m = read_manifest(&dir).unwrap();
        let l = load_edits(&m);
        assert_eq!(l.speeds.len(), 1);
        assert_eq!(l.background.radius_px, 12);
        assert_eq!(m.edits[0]["background"]["value"], "#112233");
        assert_eq!(history_flags(&m), (true, false));
    }
}
