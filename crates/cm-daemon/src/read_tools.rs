//! Two read-only tools for agents: `get_cursor_track` (REC-7) and `get_platform_rules` (EXP-2).
//!
//! Both are Free and never change anything. `get_cursor_track` reads a recording's
//! `cursor.jsonl` through `cm-cursor`; `get_platform_rules` serves the publish limits of
//! `cm-publish` and the export presets of `cm-media` (the table that `CAPTUREMANN_PLATFORM_RULES`
//! can replace), so the app and agents check the same numbers before uploading.

use cm_core::edit::parse_time;
use cm_core::{CmError, MediaTime};
use cm_cursor::{CursorLog, CURSOR_FILE};
use cm_media::preset::{FrameDef, PresetDef};
use cm_media::Presets;
use cm_publish::{PlatformRules, YOUTUBE_UPLOADS_PER_DAY};
use serde_json::{json, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, Inner};
use crate::error::DaemonError;

pub(crate) const CURSOR_METHOD: &str = "get_cursor_track";
pub(crate) const RULES_METHOD: &str = "get_platform_rules";

/// Events returned when `limit` is absent, and the most one call may return. A 10-minute recording
/// holds about 70,000 moves, so a long track is read in pages (`next_from`).
pub const DEFAULT_CURSOR_LIMIT: usize = 5_000;
pub const MAX_CURSOR_LIMIT: usize = 50_000;

/// The publishing platforms the rules cover today.
const PLATFORMS: &[&str] = &["youtube"];

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CursorTrackParams {
    pub capture_id: String,
    /// Session time, inclusive. `None` = the start.
    pub from: Option<MediaTime>,
    /// Session time, exclusive. `None` = the end.
    pub to: Option<MediaTime>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlatformRulesParams {
    pub platform: String,
}

pub(crate) fn parse_cursor(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["capture_id", "from", "to", "limit"])?;
    let time = |key: &str| -> Result<Option<MediaTime>, CmError> {
        match f.obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => parse_time(key, v).map(Some),
        }
    };
    let (from, to) = (time("from")?, time("to")?);
    if let (Some(a), Some(b)) = (from, to) {
        if b < a {
            return Err(CmError::invalid_field("to", "must not be before `from`"));
        }
    }
    if from.is_some_and(|t| t < MediaTime::ZERO) {
        return Err(CmError::invalid_field("from", "must not be negative"));
    }
    let limit = match f.obj.get("limit") {
        None | Some(Value::Null) => DEFAULT_CURSOR_LIMIT,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=MAX_CURSOR_LIMIT as u64).contains(n))
            .ok_or_else(|| {
                CmError::invalid_field("limit", format!("must be 1 to {MAX_CURSOR_LIMIT}"))
            })? as usize,
    };
    Ok(Action::GetCursorTrack(CursorTrackParams {
        capture_id: f.str_req("capture_id")?,
        from,
        to,
        limit,
    }))
}

pub(crate) fn parse_rules(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["platform"])?;
    Ok(Action::GetPlatformRules(PlatformRulesParams {
        platform: f.str_req("platform")?.trim().to_ascii_lowercase(),
    }))
}

fn preset_json(d: &PresetDef) -> Value {
    json!({
        "id": d.id,
        "label": d.label,
        "format": d.kind.extension(),
        "frame": match d.frame {
            FrameDef::Master => json!("master"),
            FrameDef::Fixed(w, h) => json!([w, h]),
        },
        "fps": d.fps,
        "mbps_30": d.mbps_30,
        "mbps_60": d.mbps_60,
        "loudness_lufs": d.loudness_lufs,
        "refuse_upscale": d.refuse_upscale,
    })
}

/// The reply for one platform, from the rules and the preset table (a pure function, tested).
pub(crate) fn platform_rules_json(
    rules: &PlatformRules,
    uploads_per_day: u32,
    presets: &Presets,
    source: Option<&std::path::Path>,
) -> Value {
    let prefix = format!("{}-", rules.platform);
    let mine: Vec<&PresetDef> = presets
        .defs
        .iter()
        .filter(|d| d.id.starts_with(&prefix))
        .collect();
    let recommended = mine
        .iter()
        .find(|d| d.id.ends_with("-1080p"))
        .or(mine.first())
        .map(|d| d.id.clone());
    let mut required = Vec::new();
    if rules.title_required {
        required.push("title");
    }
    if rules.requires_made_for_kids {
        required.push("made_for_kids");
    }
    json!({
        "platform": rules.platform,
        "limits": {
            "max_bytes": rules.max_bytes,
            "max_duration_secs": rules.max_duration_secs,
            "video_extensions": rules.video_extensions,
            "title_max_chars": rules.title_max_chars,
            "description_max_bytes": rules.description_max_bytes,
            "tags_max_total_chars": rules.tags_max_total_chars,
            "thumbnail_max_bytes": rules.thumbnail_max_bytes,
            "thumbnail_extensions": rules.thumbnail_extensions,
            "uploads_per_day": uploads_per_day,
        },
        "required_fields": required,
        "privacy_options": rules.privacies.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        "default_privacy": rules.default_privacy.as_str(),
        "unverified_project_privacy": rules.unverified_project_privacy.map(|p| p.as_str()),
        "default_category": rules.default_category,
        "recommended_preset": recommended,
        "presets": mine.iter().map(|d| preset_json(d)).collect::<Vec<_>>(),
        "presets_source": source.map(|p| p.display().to_string()),
    })
}

impl Inner {
    /// `get_platform_rules`.
    pub(crate) async fn get_platform_rules(
        &self,
        p: PlatformRulesParams,
    ) -> Result<Value, DaemonError> {
        match p.platform.as_str() {
            "youtube" => {}
            "tiktok" | "facebook" => {
                return Err(DaemonError::NotImplemented(format!(
                    "the rules of {} are not built yet; YouTube is",
                    if p.platform == "tiktok" {
                        "TikTok"
                    } else {
                        "Facebook"
                    }
                )))
            }
            _ => {
                return Err(CmError::invalid_field(
                    "platform",
                    format!("must be one of: {}", PLATFORMS.join(", ")),
                )
                .into())
            }
        }
        blocking(move || {
            let loaded = Presets::load_default();
            let mut v = platform_rules_json(
                &PlatformRules::youtube(),
                YOUTUBE_UPLOADS_PER_DAY,
                &loaded.presets,
                loaded.source.as_deref(),
            );
            if let (Some(o), Some(w)) = (v.as_object_mut(), loaded.warning) {
                o.insert("warning".into(), json!(w));
            }
            Ok(v)
        })
        .await
    }

    /// `get_cursor_track`.
    pub(crate) async fn get_cursor_track(
        &self,
        p: CursorTrackParams,
    ) -> Result<Value, DaemonError> {
        let library = self.library.clone();
        blocking(move || {
            let (dir, manifest) = crate::ai::recording_bundle(&library, &p.capture_id)?;
            let file = manifest
                .tracks
                .get("cursor")
                .and_then(|t| t.get("file"))
                .and_then(Value::as_str)
                .unwrap_or(CURSOR_FILE);
            let path = dir.join(file);
            if !path.is_file() {
                return Err(CmError::not_found(format!(
                    "`{}` has no cursor track (it was recorded without the cursor)",
                    p.capture_id
                )));
            }
            let log = CursorLog::read(&path)?;
            Ok(cursor_json(&p, &log))
        })
        .await
    }
}

/// The reply for a parsed log (pure, tested).
pub(crate) fn cursor_json(p: &CursorTrackParams, log: &CursorLog) -> Value {
    let from = p.from.unwrap_or(MediaTime::ZERO);
    let to = p.to.unwrap_or(MediaTime::from_ticks(i64::MAX));
    let all = log.range(from, to);
    let page = &all[..all.len().min(p.limit)];
    // The next page starts at the first event left out; events share a tick, so it repeats none
    // of them only when the cut falls between two ticks. A cut inside a tick returns that whole
    // tick again, which a reader dedupes by position.
    let next_from = all.get(page.len()).map(|e| e.t());
    json!({
        "capture_id": p.capture_id,
        "tick_rate": log.header.tick_rate,
        "coordinates": "physical px relative to the top-left of the recorded area (the screen track), y down",
        "target": log.header.target,
        "clicks": log.header.clicks,
        "scroll": log.header.scroll,
        "move_source": log.header.move_source,
        "permission_missing": log.header.permission_missing,
        "from": from.ticks(),
        "to": p.to.map(MediaTime::ticks),
        "count": page.len(),
        "total_in_range": all.len(),
        "events": page,
        "next_from": next_from,
        "truncated_file": log.truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_cursor::{Availability, CursorEvent, CursorHeader, CursorTarget, MoveSource};

    fn log(n: i64) -> CursorLog {
        let target = CursorTarget::display(1, cm_core::Rect::new(0.0, 0.0, 100.0, 50.0), 2.0);
        CursorLog {
            header: CursorHeader {
                ty: "session".into(),
                schema: 1,
                tick_rate: 120_000,
                clock: "test".into(),
                platform: "test".into(),
                target,
                clicks: Availability::Available,
                scroll: Availability::Available,
                move_source: MoveSource::Fake,
                permission_missing: None,
            },
            events: (0..n)
                .map(|i| CursorEvent::Move {
                    t: i * 1000,
                    x: i as i32,
                    y: 2,
                    outside: false,
                })
                .collect(),
            truncated: false,
        }
    }

    fn params(from: Option<i64>, to: Option<i64>, limit: usize) -> CursorTrackParams {
        CursorTrackParams {
            capture_id: "rec_1".into(),
            from: from.map(MediaTime::from_ticks),
            to: to.map(MediaTime::from_ticks),
            limit,
        }
    }

    #[test]
    fn range_is_half_open_and_pages_with_next_from() {
        let l = log(10);
        let v = cursor_json(&params(Some(2000), Some(8000), 4), &l);
        assert_eq!(v["total_in_range"], 6);
        assert_eq!(v["count"], 4);
        assert_eq!(v["events"][0]["t"], 2000);
        assert_eq!(v["next_from"], 6000);
        let v2 = cursor_json(&params(Some(6000), Some(8000), 4), &l);
        assert_eq!(v2["count"], 2);
        assert!(v2["next_from"].is_null());
    }

    #[test]
    fn rules_name_the_youtube_presets() {
        let v = platform_rules_json(
            &PlatformRules::youtube(),
            YOUTUBE_UPLOADS_PER_DAY,
            &Presets::builtin(),
            None,
        );
        assert_eq!(v["limits"]["title_max_chars"], 100);
        assert_eq!(v["recommended_preset"], "youtube-1080p");
        assert_eq!(v["default_privacy"], "unlisted");
        assert!(v["required_fields"]
            .as_array()
            .unwrap()
            .contains(&json!("made_for_kids")));
        let ids: Vec<&str> = v["presets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["youtube-4k", "youtube-1440p", "youtube-1080p"]);
    }
}
