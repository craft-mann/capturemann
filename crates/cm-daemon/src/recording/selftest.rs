//! The 60 fps self-test (REC-3): its input, its stored result and the gate that decides whether
//! `start_recording { fps: 60 }` is accepted.
//!
//! # What the self-test is
//!
//! `run_fps_selftest` records about 10 s of the chosen target at 60 fps with the real sources
//! (screen track only: no microphone, camera, system audio or cursor), measures the late-frame
//! percentage and the dropped frames of the screen track (NFR-2: at most 1% late), **deletes the
//! file** and stores the verdict. It runs in the engine ([`super::engine`]); this module holds
//! the pure parts so they can be tested without a recorder.
//!
//! # The stored result (a settings key, not a secret)
//!
//! `recording.fps60_selftest` (readable with `get_settings`; written only by the self-test, a
//! client's `set_settings` of this key is refused so nobody can forge a pass):
//!
//! ```json
//! { "version": 1,
//!   "results": {
//!     "1:3024x1964:hevc": { "passed": true, "late_pct": 0.12, "dropped": 0, "dropped_pct": 0.0,
//!       "frames": 598, "at": "2026-10-21T09:00:00Z", "display": 1, "size": [3024, 1964],
//!       "codec": "hevc", "app_version": "0.0.1", "duration_s": 10.0 } } }
//! ```
//!
//! The key is `display:WIDTHxHEIGHT:codec` of the *output* (what the file would be). The machine
//! fingerprint is display id + output size + codec + app version.
//!
//! # The gate (decision)
//!
//! A request for 60 fps is accepted when a result for the same display and codec
//!
//! 1. is not stale: written at most 30 days ago (a result dated more than a day in the future is
//!    treated as stale), and by the same app version, and
//! 2. passed at this output size, or at a larger one (a machine that keeps up at 2160p keeps up at
//!    1080p; a pass never covers a larger output), and
//! 3. is the newest verdict that applies: a fail at this size or a smaller one is newer than the
//!    pass (a tie goes to the fail), so a machine that got worse is judged by its latest run.
//!
//! Otherwise the request is `invalid_input` with `data.suggestion` (see [`Fps60Rejection`]).
//! Explicit requests (`start_recording { fps: 60 }`) are refused; a *default* of 60 coming from
//! the `recording.fps` setting falls back to 30 fps with a `fps60_not_validated` warning instead,
//! so a stale result never blocks recording.

use std::time::Duration;

use cm_core::CmError;
use cm_encode::{CodecChoice, OutputSize, VideoPlan};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::action::{Fields, ScreenshotTarget};

/// The settings key that holds the results.
pub const SETTING_KEY: &str = "recording.fps60_selftest";
/// Current format of the stored value.
pub const STORE_VERSION: u32 = 1;
/// Length of the self-test (REC-3: 10 s).
pub const DEFAULT_DURATION: Duration = Duration::from_secs(10);
/// A result older than this is stale.
pub const STALE_AFTER_DAYS: i64 = 30;
/// NFR-2: at most this share of late frames passes.
pub const MAX_LATE_PCT: f64 = 1.0;
/// At most this share of frames dropped by the recorder passes.
pub const MAX_DROPPED_PCT: f64 = 1.0;
/// At most this much capture stall (no delivery for over 1 s, R3-7) in the run passes.
pub const MAX_STALL_MS: u64 = 500;

/// Output heights a lower size can be suggested from (REC-2 presets).
const LADDER: [(&str, u32); 4] = [
    ("2160p", 2160),
    ("1440p", 1440),
    ("1080p", 1080),
    ("720p", 720),
];

/// `run_fps_selftest` input: `{ target?, size?, codec? }`. Missing fields come from the settings
/// (`recording.size`, `recording.codec`) and the primary display.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelftestParams {
    pub target: Option<ScreenshotTarget>,
    pub size: Option<OutputSize>,
    pub codec: Option<CodecChoice>,
}

impl SelftestParams {
    /// Validates the pass-through fields of `run_fps_selftest`.
    pub fn parse(rest: &Map<String, Value>) -> Result<SelftestParams, CmError> {
        let value = Value::Object(rest.clone());
        let empty = Map::new();
        let f = Fields::new(&value, &empty)?;
        f.only(&["target", "size", "codec"])?;
        let target = match f.obj.get("target") {
            None | Some(Value::Null) => None,
            Some(v) => Some(ScreenshotTarget::parse(v)?),
        };
        let field = |key: &str, allowed: &str| -> Result<Option<Value>, CmError> {
            match f.obj.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) if v.is_string() => Ok(Some(v.clone())),
                Some(_) => Err(CmError::invalid_field(
                    key,
                    format!("must be one of {allowed}"),
                )),
            }
        };
        let size_allowed = "source, 2160p, 1440p, 1080p, 720p";
        let codec_allowed = "auto, hevc, h264";
        let size = field("size", size_allowed)?
            .map(|v| {
                serde_json::from_value(v).map_err(|_| {
                    CmError::invalid_field("size", format!("must be one of {size_allowed}"))
                })
            })
            .transpose()?;
        let codec = field("codec", codec_allowed)?
            .map(|v| {
                serde_json::from_value(v).map_err(|_| {
                    CmError::invalid_field("codec", format!("must be one of {codec_allowed}"))
                })
            })
            .transpose()?;
        Ok(SelftestParams {
            target,
            size,
            codec,
        })
    }
}

/// What a result is about: the machine and the settings it was measured on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    /// `None` when the plan has no display (a window whose display is unknown).
    pub display: Option<u32>,
    /// Output size in px.
    pub width: u32,
    pub height: u32,
    /// Resolved codec: `hevc` or `h264`.
    pub codec: String,
    pub app_version: String,
    /// The requested size preset and codec choice, for the `run_selftest` suggestion.
    pub size_preset: OutputSize,
    pub codec_choice: CodecChoice,
}

impl Fingerprint {
    pub fn new(
        display: Option<u32>,
        video: &VideoPlan,
        size_preset: OutputSize,
        codec_choice: CodecChoice,
        app_version: &str,
    ) -> Self {
        Fingerprint {
            display,
            width: video.width,
            height: video.height,
            codec: video.codec.as_str().to_owned(),
            app_version: app_version.to_owned(),
            size_preset,
            codec_choice,
        }
    }

    /// The key of the stored result: `display:WIDTHxHEIGHT:codec`.
    pub fn key(&self) -> String {
        entry_key(self.display, [self.width, self.height], &self.codec)
    }

    fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// `1440p`-style label of the output height.
    pub fn size_label(&self) -> String {
        format!("{}p", self.height)
    }
}

fn entry_key(display: Option<u32>, size: [u32; 2], codec: &str) -> String {
    let d = display.map_or_else(|| "none".to_owned(), |d| d.to_string());
    format!("{d}:{}x{}:{codec}", size[0], size[1])
}

/// One stored verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelftestEntry {
    pub passed: bool,
    /// Late frames over the run, percent (NFR-2).
    pub late_pct: f64,
    /// Frames the recorder dropped.
    pub dropped: u64,
    pub dropped_pct: f64,
    /// Frames written.
    pub frames: u64,
    /// RFC 3339 time of the run.
    pub at: String,
    pub display: Option<u32>,
    pub size: [u32; 2],
    pub codec: String,
    pub app_version: String,
    pub duration_s: f64,
    /// Capture stalls during the run, milliseconds (R3-7). Results stored before this existed
    /// read as 0.
    #[serde(default)]
    pub stall_ms: u64,
}

impl SelftestEntry {
    pub fn key(&self) -> String {
        entry_key(self.display, self.size, &self.codec)
    }

    fn pixels(&self) -> u64 {
        u64::from(self.size[0]) * u64::from(self.size[1])
    }

    fn written_at(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::parse(&self.at, &Rfc3339).ok()
    }

    /// Fresh enough and from this app version.
    fn current(&self, now: OffsetDateTime, version: &str) -> Staleness {
        let Some(at) = self.written_at() else {
            return Staleness::Stale;
        };
        let age = now - at;
        if age > time::Duration::days(STALE_AFTER_DAYS) || age < -time::Duration::days(1) {
            Staleness::Stale
        } else if self.app_version != version {
            Staleness::OtherVersion
        } else {
            Staleness::Current
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Staleness {
    Current,
    Stale,
    OtherVersion,
}

/// The pass rule: at most [`MAX_LATE_PCT`] late and [`MAX_DROPPED_PCT`] dropped, and the run
/// produced frames at all.
pub fn judge(late_pct: f64, dropped_pct: f64, frames: u64) -> bool {
    frames > 0 && late_pct <= MAX_LATE_PCT && dropped_pct <= MAX_DROPPED_PCT
}

/// [`judge`] plus the stall rule (R3-7): a run with more than [`MAX_STALL_MS`] of capture stalls
/// fails, even when the frames that did arrive were on time.
pub fn judge_with_stalls(late_pct: f64, dropped_pct: f64, frames: u64, stall_ms: u64) -> bool {
    judge(late_pct, dropped_pct, frames) && stall_ms <= MAX_STALL_MS
}

/// All stored entries (malformed ones are skipped).
pub fn load_entries(settings: &Map<String, Value>) -> Vec<SelftestEntry> {
    settings
        .get(SETTING_KEY)
        .and_then(|v| v.get("results"))
        .and_then(Value::as_object)
        .map(|m| {
            m.values()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The settings value after `entry` replaces the result with its key. Entries older than the
/// staleness window (relative to `now`) are dropped so the value stays small.
pub fn store_value(
    settings: &Map<String, Value>,
    entry: SelftestEntry,
    now: OffsetDateTime,
) -> Value {
    let mut results = Map::new();
    for e in load_entries(settings) {
        let keep = e
            .written_at()
            .is_some_and(|at| now - at <= time::Duration::days(STALE_AFTER_DAYS));
        if keep && e.key() != entry.key() {
            if let Ok(v) = serde_json::to_value(&e) {
                results.insert(e.key(), v);
            }
        }
    }
    if let Ok(v) = serde_json::to_value(&entry) {
        results.insert(entry.key(), v);
    }
    json!({ "version": STORE_VERSION, "results": results })
}

/// Why a request for 60 fps is not accepted.
#[derive(Debug, Clone, PartialEq)]
pub struct Fps60Rejection {
    /// `failed`, `stale`, `version_changed`, `settings_changed` or `not_validated`.
    pub reason: &'static str,
    /// The user-facing message (REC-3: "60 fps isn't stable on this machine at 2160p \u{2014} try
    /// 1440p").
    pub message: String,
    /// `data.suggestion`: `{ fps: 30, reason, size?, run_selftest: { method, params, cli } }`.
    pub suggestion: Value,
}

impl Fps60Rejection {
    /// The `invalid_input` error with `data: { suggestion }` (REC-3 agent interface).
    pub fn into_error(self) -> crate::error::DaemonError {
        crate::error::DaemonError::WithData {
            error: CmError::invalid_input(self.message),
            data: json!({ "suggestion": self.suggestion, "reason": self.reason }),
        }
    }
}

/// The gate's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Fps60Verdict {
    Allowed,
    Rejected(Fps60Rejection),
}

/// The next preset below `height`, if any.
fn lower_size(height: u32) -> Option<&'static str> {
    LADDER.iter().find(|(_, h)| *h < height).map(|(l, _)| *l)
}

fn size_preset_name(s: OutputSize) -> &'static str {
    match s {
        OutputSize::Source => "source",
        OutputSize::P2160 => "2160p",
        OutputSize::P1440 => "1440p",
        OutputSize::P1080 => "1080p",
        OutputSize::P720 => "720p",
    }
}

fn codec_choice_name(c: CodecChoice) -> &'static str {
    match c {
        CodecChoice::Auto => "auto",
        CodecChoice::Hevc => "hevc",
        CodecChoice::H264 => "h264",
    }
}

/// The `run_selftest` hint of a suggestion: the call that would unlock 60 fps.
fn run_hint(fp: &Fingerprint) -> Value {
    let target = fp.display.map(|d| json!({ "display_id": d }));
    json!({
        "method": "run_fps_selftest",
        "params": { "target": target, "size": size_preset_name(fp.size_preset),
                    "codec": codec_choice_name(fp.codec_choice) },
        "cli": "capturemann record selftest",
    })
}

/// Decides whether a request for 60 fps at `fp` is accepted, given the stored results.
pub fn evaluate(
    settings: &Map<String, Value>,
    fp: &Fingerprint,
    now: OffsetDateTime,
) -> Fps60Verdict {
    let same_machine: Vec<SelftestEntry> = load_entries(settings)
        .into_iter()
        .filter(|e| e.display == fp.display && e.codec == fp.codec)
        .collect();
    let current: Vec<&SelftestEntry> = same_machine
        .iter()
        .filter(|e| e.current(now, &fp.app_version) == Staleness::Current)
        .collect();

    // What counts: a pass at this size or a larger one, a fail at this size or a smaller one.
    // The newest of them decides (a fail wins a tie), so a machine that got better, or worse,
    // is judged by its latest run.
    let latest = current
        .iter()
        .filter(|e| {
            (e.passed && e.pixels() >= fp.pixels()) || (!e.passed && e.pixels() <= fp.pixels())
        })
        .max_by_key(|e| (e.written_at(), !e.passed));
    if latest.is_some_and(|e| e.passed) {
        return Fps60Verdict::Allowed;
    }

    let label = fp.size_label();
    let hint = run_hint(fp);
    let (reason, message, size) = if latest.is_some() {
        let lower = lower_size(fp.height);
        let message = match lower {
            Some(l) => format!("60 fps isn't stable on this machine at {label} \u{2014} try {l}"),
            None => {
                format!("60 fps isn't stable on this machine at {label} \u{2014} record at 30 fps")
            }
        };
        ("failed", message, lower)
    } else if same_machine
        .iter()
        .any(|e| e.key() == fp.key() && e.current(now, &fp.app_version) == Staleness::Stale)
    {
        (
            "stale",
            format!(
                "The 60 fps check for {label} is older than {STALE_AFTER_DAYS} days \u{2014} run the 60 fps self-test again (10 s) or record at 30 fps"
            ),
            None,
        )
    } else if same_machine
        .iter()
        .any(|e| e.current(now, &fp.app_version) == Staleness::OtherVersion)
    {
        (
            "version_changed",
            format!(
                "CaptureMann was updated since the last 60 fps check \u{2014} run the 60 fps self-test again (10 s) at {label} or record at 30 fps"
            ),
            None,
        )
    } else if !same_machine.is_empty() {
        (
            "settings_changed",
            format!(
                "60 fps was checked for other settings, not {label} with {} \u{2014} run the 60 fps self-test (10 s) or record at 30 fps",
                fp.codec
            ),
            None,
        )
    } else {
        (
            "not_validated",
            format!(
                "60 fps hasn't been checked on this machine at {label} yet \u{2014} run the 60 fps self-test (10 s) or record at 30 fps"
            ),
            None,
        )
    };
    let mut suggestion = json!({ "fps": 30, "reason": reason, "run_selftest": hint });
    if let (Some(size), Some(o)) = (size, suggestion.as_object_mut()) {
        o.insert("size".into(), json!(size));
    }
    Fps60Verdict::Rejected(Fps60Rejection {
        reason,
        message,
        suggestion,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-10-21 12:00 UTC);

    fn fp(h: u32) -> Fingerprint {
        Fingerprint {
            display: Some(1),
            width: h * 16 / 9,
            height: h,
            codec: "hevc".into(),
            app_version: "1.0".into(),
            size_preset: OutputSize::P2160,
            codec_choice: CodecChoice::Auto,
        }
    }

    fn entry(h: u32, passed: bool, at: OffsetDateTime) -> SelftestEntry {
        SelftestEntry {
            passed,
            late_pct: if passed { 0.1 } else { 4.0 },
            dropped: 0,
            dropped_pct: 0.0,
            frames: 600,
            at: at.format(&Rfc3339).unwrap(),
            display: Some(1),
            size: [h * 16 / 9, h],
            codec: "hevc".into(),
            app_version: "1.0".into(),
            duration_s: 10.0,
            stall_ms: 0,
        }
    }

    fn settings(entries: &[SelftestEntry]) -> Map<String, Value> {
        let mut m = Map::new();
        for e in entries {
            let v = store_value(&m, e.clone(), NOW);
            m.insert(SETTING_KEY.into(), v);
        }
        m
    }

    fn rejected(v: Fps60Verdict) -> Fps60Rejection {
        match v {
            Fps60Verdict::Rejected(r) => r,
            Fps60Verdict::Allowed => panic!("expected a rejection"),
        }
    }

    #[test]
    fn judge_follows_the_nfr_2_one_percent_rule() {
        assert!(judge(0.0, 0.0, 600));
        assert!(judge(1.0, 1.0, 600));
        assert!(!judge(1.01, 0.0, 600));
        assert!(!judge(0.0, 1.5, 600));
        assert!(!judge(0.0, 0.0, 0), "a run without frames never passes");
    }

    #[test]
    fn nothing_stored_is_not_validated_with_a_suggestion() {
        let r = rejected(evaluate(&Map::new(), &fp(2160), NOW));
        assert_eq!(r.reason, "not_validated");
        assert_eq!(r.suggestion["fps"], 30);
        assert_eq!(r.suggestion["run_selftest"]["method"], "run_fps_selftest");
        assert_eq!(
            r.suggestion["run_selftest"]["cli"],
            "capturemann record selftest"
        );
        assert_eq!(
            r.suggestion["run_selftest"]["params"]["target"]["display_id"],
            1
        );
        assert!(r.suggestion.get("size").is_none());
    }

    #[test]
    fn a_fresh_pass_for_the_same_settings_allows_60() {
        let s = settings(&[entry(2160, true, NOW - time::Duration::days(1))]);
        assert_eq!(evaluate(&s, &fp(2160), NOW), Fps60Verdict::Allowed);
        // A pass covers smaller outputs on the same display and codec, never larger ones.
        assert_eq!(evaluate(&s, &fp(1080), NOW), Fps60Verdict::Allowed);
        let s = settings(&[entry(1080, true, NOW)]);
        assert_eq!(
            rejected(evaluate(&s, &fp(2160), NOW)).reason,
            "settings_changed"
        );
    }

    #[test]
    fn a_fail_gives_the_spec_message_and_the_lower_size() {
        let s = settings(&[entry(2160, false, NOW)]);
        let r = rejected(evaluate(&s, &fp(2160), NOW));
        assert_eq!(r.reason, "failed");
        assert_eq!(
            r.message,
            "60 fps isn't stable on this machine at 2160p \u{2014} try 1440p"
        );
        assert_eq!(r.suggestion["size"], "1440p");
        assert_eq!(r.suggestion["fps"], 30);
        // At the bottom of the ladder there is nothing lower to offer.
        let s = settings(&[entry(720, false, NOW)]);
        let r = rejected(evaluate(&s, &fp(720), NOW));
        assert!(r.message.ends_with("record at 30 fps"), "{}", r.message);
        assert!(r.suggestion.get("size").is_none());
    }

    #[test]
    fn a_fail_at_a_smaller_size_blocks_a_larger_pass_claim_and_vice_versa() {
        // Passed at 2160p long ago in the same run set, failed at 1080p now: the fail wins at 1080p.
        let s = settings(&[entry(2160, true, NOW), entry(1080, false, NOW)]);
        assert_eq!(rejected(evaluate(&s, &fp(1080), NOW)).reason, "failed");
        // A fail at 1080p says nothing against a smaller output, which the 2160p pass covers.
        assert_eq!(evaluate(&s, &fp(720), NOW), Fps60Verdict::Allowed);
        // ... and it does count against a larger one.
        assert_eq!(rejected(evaluate(&s, &fp(1440), NOW)).reason, "failed");
    }

    #[test]
    fn the_newest_applicable_run_decides() {
        let day = time::Duration::days(1);
        // A later pass at 2160p supersedes an earlier fail at 1080p.
        let s = settings(&[
            entry(1080, false, NOW - day * 3),
            entry(2160, true, NOW - day),
        ]);
        assert_eq!(evaluate(&s, &fp(1080), NOW), Fps60Verdict::Allowed);
        // A later fail at 1080p supersedes an earlier pass at 2160p for 1080p and below.
        let s = settings(&[
            entry(2160, true, NOW - day * 3),
            entry(1080, false, NOW - day),
        ]);
        assert_eq!(rejected(evaluate(&s, &fp(1080), NOW)).reason, "failed");
        // A fail at 1080p also counts against anything larger.
        assert_eq!(rejected(evaluate(&s, &fp(1440), NOW)).reason, "failed");
    }

    #[test]
    fn a_result_older_than_30_days_is_stale() {
        // `store_value` prunes entries older than the window, so write the value by hand.
        let mut m = Map::new();
        m.insert(
            SETTING_KEY.into(),
            json!({"version": 1, "results": {
                "1:3840x2160:hevc": serde_json::to_value(entry(2160, true, NOW - time::Duration::days(31))).unwrap()}}),
        );
        assert_eq!(rejected(evaluate(&m, &fp(2160), NOW)).reason, "stale");
        let ok = NOW - time::Duration::days(29);
        let mut m2 = Map::new();
        m2.insert(
            SETTING_KEY.into(),
            json!({"version": 1, "results": {
                "1:3840x2160:hevc": serde_json::to_value(entry(2160, true, ok)).unwrap()}}),
        );
        assert_eq!(evaluate(&m2, &fp(2160), NOW), Fps60Verdict::Allowed);
    }

    #[test]
    fn a_changed_display_codec_size_or_version_is_not_covered() {
        let s = settings(&[entry(2160, true, NOW)]);
        let mut other_display = fp(2160);
        other_display.display = Some(2);
        assert_eq!(
            rejected(evaluate(&s, &other_display, NOW)).reason,
            "not_validated"
        );
        let mut other_codec = fp(2160);
        other_codec.codec = "h264".into();
        assert_eq!(
            rejected(evaluate(&s, &other_codec, NOW)).reason,
            "not_validated"
        );
        let mut larger = fp(2160);
        larger.width += 100;
        larger.height += 100;
        assert_eq!(
            rejected(evaluate(&s, &larger, NOW)).reason,
            "settings_changed"
        );
        let mut newer = fp(2160);
        newer.app_version = "1.1".into();
        assert_eq!(
            rejected(evaluate(&s, &newer, NOW)).reason,
            "version_changed"
        );
    }

    #[test]
    fn a_timestamp_from_the_future_is_stale() {
        let mut m = Map::new();
        m.insert(
            SETTING_KEY.into(),
            json!({"version": 1, "results": {
                "1:3840x2160:hevc": serde_json::to_value(entry(2160, true, NOW + time::Duration::days(5))).unwrap()}}),
        );
        assert_eq!(rejected(evaluate(&m, &fp(2160), NOW)).reason, "stale");
    }

    #[test]
    fn store_value_replaces_its_key_keeps_others_and_prunes_old_ones() {
        let old = entry(1080, true, NOW - time::Duration::days(40));
        let keep = entry(1440, true, NOW - time::Duration::days(2));
        let mut m = Map::new();
        m.insert(
            SETTING_KEY.into(),
            json!({"version": 1, "results": {
                old.key(): serde_json::to_value(&old).unwrap(),
                keep.key(): serde_json::to_value(&keep).unwrap(),
                "1:3840x2160:hevc": serde_json::to_value(entry(2160, false, NOW - time::Duration::days(3))).unwrap()}}),
        );
        let v = store_value(&m, entry(2160, true, NOW), NOW);
        assert_eq!(v["version"], 1);
        let results = v["results"].as_object().unwrap();
        assert_eq!(results.len(), 2, "{v}");
        assert_eq!(results["1:3840x2160:hevc"]["passed"], true);
        assert!(results.contains_key(&keep.key()));
    }

    #[test]
    fn params_validate_their_fields() {
        let p = |v: Value| SelftestParams::parse(v.as_object().unwrap());
        let ok = p(json!({"target": {"display_id": 1}, "size": "1080p", "codec": "hevc"})).unwrap();
        assert_eq!(ok.size, Some(OutputSize::P1080));
        assert_eq!(ok.codec, Some(CodecChoice::Hevc));
        assert_eq!(p(json!({})).unwrap().target, None);
        for (v, field) in [
            (json!({"size": "8k"}), "size"),
            (json!({"codec": "vp9"}), "codec"),
            (json!({"size": 4}), "size"),
            (json!({"fps": 60}), "fps"),
            (json!({"target": 3}), "target"),
        ] {
            let e = p(v.clone()).unwrap_err();
            assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput, "{v}");
            assert!(e.to_string().starts_with(field), "{v}: {e}");
        }
    }
}
