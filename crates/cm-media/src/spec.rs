//! Reads what the quick export needs out of a `.cmrec` manifest (as JSON, so this crate does not
//! depend on `cm-library`).
//!
//! * `tracks.<screen|camera|mic|system>`: `file`, `start`/`end` (session ticks), details (`fps`,
//!   `size`, `source_size`, `scale`, `mirror`, `channels`). A track with `"missing": true` is
//!   ignored (REC-8). The mic entry already names the denoised file when denoising was on.
//! * `gaps`: pause gaps in session time (`[{from, to}]` ticks). When the manifest has none, the
//!   gaps are read from a track's details instead: `tracks.screen.pauses` (new key) or
//!   `tracks.screen.gaps` (old key), then `tracks.cursor.pauses` or `tracks.cursor.pause_gaps`
//!   (old key), so recordings written before and after the rename both export (R5).
//! * `edits[0].layout.camera`: the bubble layout persisted at stop. If absent (older or recovered
//!   bundles) the layout is derived from `settings.camera` and the screen size.
//!
//! The FFmpeg graph ([`crate::graph`]) does not apply the edit list; [`crate::edited`] does (it is
//! chosen when the list changes the picture).

use cm_core::CmError;
use serde_json::Value;

use crate::layout::{camera_layout, CameraLayout};
use crate::timeline::{gaps_from_json, offset_seconds, Gap};

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenTrack {
    pub file: String,
    pub start: i64,
    pub end: i64,
    pub fps: f64,
    /// Master size in px, as recorded.
    pub size: (u32, u32),
    /// Recorded area in points = `source_size / scale`.
    pub area_pt: (f64, f64),
    /// The recorded area in physical px (the master may be scaled down from it).
    pub source_size: (u32, u32),
    /// Physical px per point of the recorded display.
    pub scale: f64,
    /// The cursor is not baked into the picture ("Record cursor separately", EDT-4).
    pub cursor_separate: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraTrack {
    pub file: String,
    pub start: i64,
    pub end: i64,
    pub mirror: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioTrack {
    pub file: String,
    pub start: i64,
    pub end: i64,
    pub channels: u32,
}

/// Everything the quick export reads from the manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickSpec {
    pub title: String,
    pub screen: ScreenTrack,
    pub camera: Option<CameraTrack>,
    pub mic: Option<AudioTrack>,
    pub system: Option<AudioTrack>,
    pub gaps: Vec<Gap>,
    /// `live_at` of the manifest (session ticks): the moment every track was delivering, measured
    /// at record time on the recording machine. The default in-point of every export.
    pub live_at: Option<i64>,
    /// The bubble to draw; `Some` only when there is a camera track and it is visible.
    pub layout: Option<CameraLayout>,
    /// Why something is left out (a hidden bubble, a missing track), for the export entry.
    pub notes: Vec<String>,
}

fn track<'a>(m: &'a Value, name: &str, notes: &mut Vec<String>) -> Option<&'a Value> {
    let t = m.get("tracks")?.get(name)?;
    if t.get("missing").and_then(Value::as_bool) == Some(true) {
        notes.push(format!("the {name} track could not be recovered"));
        return None;
    }
    t.get("file").and_then(Value::as_str)?;
    Some(t)
}

fn file_of(t: &Value) -> String {
    t.get("file")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn ticks(t: &Value, key: &str) -> i64 {
    t.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn pair_u32(v: Option<&Value>) -> Option<(u32, u32)> {
    let a = v?.as_array()?;
    let w = u32::try_from(a.first()?.as_u64()?).ok()?;
    let h = u32::try_from(a.get(1)?.as_u64()?).ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

fn audio(t: &Value, default_channels: u32) -> AudioTrack {
    AudioTrack {
        file: file_of(t),
        start: ticks(t, "start"),
        end: ticks(t, "end"),
        channels: t
            .get("channels")
            .and_then(Value::as_u64)
            .and_then(|c| u32::try_from(c).ok())
            .filter(|c| (1..=2).contains(c))
            .unwrap_or(default_channels),
    }
}

impl QuickSpec {
    /// Parses a manifest.
    pub fn from_manifest(m: &Value) -> Result<QuickSpec, CmError> {
        let mut notes = Vec::new();
        let screen_v = track(m, "screen", &mut notes).ok_or_else(|| {
            CmError::invalid_input(if notes.is_empty() {
                "This recording has no screen track, so there is nothing to export."
            } else {
                "The screen track of this recording could not be recovered, so there is nothing to export."
            })
        })?;
        let size = pair_u32(screen_v.get("size"))
            .ok_or_else(|| CmError::invalid_input("the screen track has no size"))?;
        let fps = screen_v
            .get("fps")
            .and_then(Value::as_f64)
            .filter(|f| (1.0..=240.0).contains(f))
            .unwrap_or(30.0);
        let scale = screen_v
            .get("scale")
            .and_then(Value::as_f64)
            .filter(|s| *s >= 1.0)
            .unwrap_or(1.0);
        let source = pair_u32(screen_v.get("source_size")).unwrap_or(size);
        let area_pt = (f64::from(source.0) / scale, f64::from(source.1) / scale);
        let screen = ScreenTrack {
            file: file_of(screen_v),
            start: ticks(screen_v, "start"),
            end: ticks(screen_v, "end"),
            fps,
            size,
            area_pt,
            source_size: source,
            scale,
            // Recordings made before the option existed bake the cursor in.
            cursor_separate: screen_v.get("cursor").and_then(Value::as_bool) == Some(false),
        };

        let camera_v = track(m, "camera", &mut notes);
        let camera = camera_v.map(|t| CameraTrack {
            file: file_of(t),
            start: ticks(t, "start"),
            end: ticks(t, "end"),
            mirror: t.get("mirror").and_then(Value::as_bool),
        });
        let mic = track(m, "mic", &mut notes).map(|t| audio(t, 1));
        let system = track(m, "system", &mut notes).map(|t| audio(t, 2));

        let settings_cam = m.get("settings").and_then(|s| s.get("camera"));
        let stored = m
            .get("edits")
            .and_then(|e| e.get(0))
            .and_then(|e| e.get("layout"))
            .and_then(|l| l.get("camera"));
        let mut layout = None;
        if let Some(cam) = &camera {
            let mut l = match stored.and_then(CameraLayout::from_json) {
                Some(l) => l,
                None => derive_layout(settings_cam, &screen),
            };
            // Mirror: the layout wins, then the track's flag, then the recording settings.
            l.mirror = l
                .mirror
                .or(cam.mirror)
                .or_else(|| {
                    settings_cam
                        .and_then(|c| c.get("mirror"))
                        .and_then(Value::as_bool)
                })
                .or(Some(false));
            if l.visible {
                layout = Some(l);
            } else {
                notes.push("the camera bubble is hidden in the layout".to_owned());
            }
        }
        Ok(QuickSpec {
            title: m
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Recording")
                .to_owned(),
            screen,
            camera,
            mic,
            system,
            gaps: session_gaps(m),
            live_at: m.get("live_at").and_then(Value::as_i64),
            layout,
            notes,
        })
    }

    /// Session time the export starts at: the manifest's `live_at` when it lies inside the screen
    /// track, else the screen's first frame (older recordings, or a `live_at` outside the file).
    pub fn origin(&self) -> i64 {
        match self.live_at {
            Some(l) if l > self.screen.start && l < self.screen.end => l,
            _ => self.screen.start,
        }
    }

    /// Seconds cut off the front of the screen file so the export starts at [`Self::origin`].
    pub fn front_trim_seconds(&self) -> f64 {
        offset_seconds(self.origin(), self.screen.start, &self.gaps)
    }

    /// The camera track to composite, if the bubble is visible.
    pub fn visible_camera(&self) -> Option<(&CameraTrack, &CameraLayout)> {
        Some((self.camera.as_ref()?, self.layout.as_ref()?))
    }
}

/// The pause gaps in session time: the manifest's, else a track's own list. Both the old keys
/// (`screen.gaps`, `cursor.pause_gaps`) and the new one (`pauses`) are accepted.
fn session_gaps(m: &Value) -> Vec<Gap> {
    let top = gaps_from_json(m.get("gaps"));
    if !top.is_empty() {
        return top;
    }
    let tracks = m.get("tracks");
    for (track, keys) in [
        ("screen", ["pauses", "gaps"]),
        ("cursor", ["pauses", "pause_gaps"]),
    ] {
        for key in keys {
            let g = gaps_from_json(tracks.and_then(|t| t.get(track)).and_then(|t| t.get(key)));
            if !g.is_empty() {
                return g;
            }
        }
    }
    Vec::new()
}

/// The layout from the recording settings (`settings.camera`) when none was stored.
fn derive_layout(settings_cam: Option<&Value>, screen: &ScreenTrack) -> CameraLayout {
    let get = |k: &str, d: &str| {
        settings_cam
            .and_then(|c| c.get(k))
            .and_then(Value::as_str)
            .unwrap_or(d)
            .to_owned()
    };
    let position = settings_cam
        .and_then(|c| c.get("position"))
        .cloned()
        .unwrap_or_else(|| Value::String("bottom-left".into()));
    camera_layout(
        &get("shape", "circle"),
        &get("size", "m"),
        &position,
        // The mirror flag is resolved by the caller (layout, then track, then settings).
        None,
        screen.area_pt,
    )
}

#[cfg(test)]
pub(crate) mod fixtures {
    use serde_json::{json, Value};

    /// A manifest shaped like the engine's (`RecManifest` JSON): screen 1280x720@30 starting at
    /// 390 ms, camera 190 ms later, mic 290 ms before the screen, a system track.
    pub fn manifest() -> Value {
        json!({
            "schema": 1, "id": "rec_TEST", "title": "Lesson 1", "created": "2026-10-21T09:00:00Z",
            "tick_rate": 120000, "duration": 1_200_000,
            "tracks": {
                "screen": {"file": "screen.mp4", "size": [1280, 720], "fps": 30, "codec": "h264",
                           "source_size": [2560, 1440], "scale": 2.0,
                           "start": 46800, "end": 46800 + 1_200_000 + 120_000, "stats": {}},
                "camera": {"file": "camera.mp4", "size": [1280, 720], "mirror": true, "fps": 30,
                           "start": 69600, "end": 69600 + 1_200_000, "stats": {}},
                "mic": {"file": "mic.denoised.wav", "rate": 48000, "channels": 1,
                        "raw_file": "mic.wav", "start": 12000, "end": 1_300_000, "stats": {}},
                "system": {"file": "system.wav", "rate": 48000, "channels": 2,
                           "start": 46800, "end": 1_300_000, "stats": {}}
            },
            "gaps": [{"from": 600_000, "to": 720_000}],
            "edits": [{"in": 0, "out": 1_200_000, "speed": 1.0}],
            "settings": {"camera": {"shape": "circle", "size": "m", "position": "bottom-right", "mirror": true}}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::manifest;
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_tracks_gaps_and_derives_the_layout_from_settings() {
        let s = QuickSpec::from_manifest(&manifest()).unwrap();
        assert_eq!(s.title, "Lesson 1");
        assert_eq!(s.screen.size, (1280, 720));
        assert_eq!(s.screen.fps, 30.0);
        assert_eq!(s.screen.area_pt, (1280.0, 720.0));
        assert_eq!(s.gaps, vec![(600_000, 720_000)]);
        assert_eq!(s.mic.as_ref().unwrap().file, "mic.denoised.wav");
        assert_eq!(s.mic.as_ref().unwrap().channels, 1);
        assert_eq!(s.system.as_ref().unwrap().channels, 2);
        let l = s.layout.unwrap();
        // 240 pt of a 1280 pt wide area.
        assert!((l.rect[2] - 240.0 / 1280.0).abs() < 1e-9);
        assert_eq!(l.mirror, Some(true));
    }

    /// The export origin is `live_at` when it lies inside the screen track, else the screen's
    /// first frame; never a number baked into the code (it is measured per recording).
    #[test]
    fn origin_is_live_at_inside_the_screen_track_else_the_screen_start() {
        let mut m = manifest();
        let screen_start = 46_800;
        let s = QuickSpec::from_manifest(&m).unwrap();
        assert_eq!((s.origin(), s.front_trim_seconds()), (screen_start, 0.0));
        m["live_at"] = json!(69_600);
        let s = QuickSpec::from_manifest(&m).unwrap();
        assert_eq!(s.origin(), 69_600);
        assert!((s.front_trim_seconds() - 0.19).abs() < 1e-9);
        // Outside the track (or exactly at its start): the screen's first frame.
        for bad in [0, screen_start, 46_800 + 1_320_000, 9_000_000] {
            m["live_at"] = json!(bad);
            let s = QuickSpec::from_manifest(&m).unwrap();
            assert_eq!((s.origin(), s.front_trim_seconds()), (screen_start, 0.0));
        }
        // A pause before live_at is not part of the trim (file time, not session time).
        m["live_at"] = json!(800_000);
        let s = QuickSpec::from_manifest(&m).unwrap();
        assert!((s.front_trim_seconds() - (680_000.0 - 46_800.0) / 120_000.0).abs() < 1e-9);
    }

    /// Pause gaps come from the manifest, else from the screen or cursor track, under either the
    /// old keys (`gaps`, `pause_gaps`) or the new `pauses`.
    #[test]
    fn gaps_fall_back_to_the_track_details_under_old_and_new_keys() {
        let gap = json!([{"from": 600_000, "to": 720_000}]);
        for (track, key) in [
            ("screen", "gaps"),
            ("screen", "pauses"),
            ("cursor", "pause_gaps"),
            ("cursor", "pauses"),
        ] {
            let mut m = manifest();
            m.as_object_mut().unwrap().remove("gaps");
            assert!(QuickSpec::from_manifest(&m).unwrap().gaps.is_empty());
            if track == "cursor" {
                m["tracks"]["cursor"] = json!({"file": "cursor.jsonl"});
            }
            m["tracks"][track][key] = gap.clone();
            assert_eq!(
                QuickSpec::from_manifest(&m).unwrap().gaps,
                vec![(600_000, 720_000)],
                "{track}.{key}"
            );
        }
        // The manifest's own list wins.
        let mut m = manifest();
        m["tracks"]["screen"]["pauses"] = json!([{"from": 1, "to": 2}]);
        assert_eq!(
            QuickSpec::from_manifest(&m).unwrap().gaps,
            vec![(600_000, 720_000)]
        );
    }

    #[test]
    fn a_stored_layout_wins_over_the_settings_and_can_hide_the_bubble() {
        let mut m = manifest();
        m["edits"][0]["layout"] = json!({"camera": {"visible": true, "shape": "rounded",
            "rect": [0.1, 0.2, 0.3, 0.4], "mirror": false}});
        let s = QuickSpec::from_manifest(&m).unwrap();
        let l = s.layout.unwrap();
        assert_eq!(l.shape, crate::layout::Shape::Rounded);
        assert_eq!(l.mirror, Some(false));
        assert_eq!(l.rect, [0.1, 0.2, 0.3, 0.4]);
        m["edits"][0]["layout"]["camera"]["visible"] = json!(false);
        let s = QuickSpec::from_manifest(&m).unwrap();
        assert!(s.layout.is_none() && s.camera.is_some());
        assert!(s.notes.iter().any(|n| n.contains("hidden")));
    }

    #[test]
    fn optional_tracks_and_missing_marks() {
        let mut m = manifest();
        m["tracks"].as_object_mut().unwrap().remove("camera");
        m["tracks"]["mic"] = json!({"file": "mic.wav", "missing": true, "reason": "x"});
        let s = QuickSpec::from_manifest(&m).unwrap();
        assert!(s.camera.is_none() && s.layout.is_none() && s.mic.is_none());
        assert!(s.system.is_some());
        assert!(s.notes.iter().any(|n| n.contains("mic")));
        m["tracks"].as_object_mut().unwrap().remove("screen");
        let e = QuickSpec::from_manifest(&m).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
    }

    #[test]
    fn mirror_falls_back_to_the_track_then_the_settings() {
        let mut m = manifest();
        m["tracks"]["camera"]["mirror"] = json!(false);
        assert_eq!(
            QuickSpec::from_manifest(&m).unwrap().layout.unwrap().mirror,
            Some(false)
        );
        m["tracks"]["camera"]
            .as_object_mut()
            .unwrap()
            .remove("mirror");
        assert_eq!(
            QuickSpec::from_manifest(&m).unwrap().layout.unwrap().mirror,
            Some(true)
        );
    }
}
