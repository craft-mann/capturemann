//! Read-only: what Suggest cuts does on a bundle, as counts only (no text of the recording is
//! printed). The final list comes from the daemon's own suggestion service (the call the editor
//! makes); the breakdown repeats its steps with the same `cm_ai` functions.
//!
//! ```text
//! CM_BUNDLE=<name.cmrec> cargo run --release -p cm-daemon --example cuts_probe
//! ```

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use cm_ai::suggest::filler::filler_cuts;
use cm_ai::suggest::silence::{
    decode_mono_f32, silence_cuts, veto_speech, SilenceOptions, ANALYSIS_RATE,
};
use cm_ai::transcribe::Transcript;
use cm_core::edit::Span;
use cm_core::MediaTime;
use cm_library::rec::read_manifest;
use serde_json::{json, Value};

fn secs(t: MediaTime) -> f64 {
    t.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64
}

fn main() {
    let dir = PathBuf::from(std::env::var("CM_BUNDLE").expect("CM_BUNDLE=<bundle dir>"));
    let manifest = read_manifest(&dir).unwrap();
    let transcript: Option<Transcript> = std::fs::read_to_string(dir.join("transcript.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let mut out = json!({ "gaps": manifest.gaps.len() });
    // The track the call site picks.
    let track = ["mic", "system"].iter().find_map(|name| {
        let t = manifest.tracks.get(*name)?;
        let start = t.get("start").and_then(Value::as_i64).unwrap_or(0);
        ["raw_file", "file"].iter().find_map(|key| {
            let file = t.get(*key).and_then(Value::as_str)?;
            let path = dir.join(file);
            path.is_file()
                .then(|| (name.to_string(), key.to_string(), path, start))
        })
    });
    let tools = cm_media::tools::locate().unwrap();
    let opts = SilenceOptions::default();
    let mut speech: Vec<Span> = transcript
        .iter()
        .flat_map(|t| t.words.iter().map(|w| Span::new(w.start, w.end)))
        .collect();
    let mut seg_secs = 0.0;
    if let Some(v) = manifest.extra.get(cm_ai::speakers::MANIFEST_SEGMENTS) {
        let segs: Vec<cm_ai::speakers::Segment> =
            serde_json::from_value(v.clone()).unwrap_or_default();
        seg_secs = segs.iter().map(|s| secs(s.end) - secs(s.start)).sum();
        speech.extend(segs.iter().map(|s| Span::new(s.start, s.end)));
        out["speaker_segments"] = json!(segs.len());
    }
    out["words"] = json!(transcript.as_ref().map(|t| t.words.len()));
    if let Some(t) = &transcript {
        let h = cm_ai::failure_kinds::health(&t.words, &t.language);
        out["transcript_health"] = json!({"replacement_char": h.replacement_char, "foreign_script": h.foreign_script, "low_confidence": h.low_confidence, "broken_and_doubtful": h.broken_and_doubtful});
    }
    out["speaker_engine_secs"] = json!(seg_secs);
    let (mut pauses_before, mut pauses_after) = (vec![], vec![]);
    if let Some((name, key, path, start)) = track {
        out["track"] = json!(format!("{name}.{key}"));
        let samples = decode_mono_f32(&tools.ffmpeg, &path, ANALYSIS_RATE).unwrap();
        out["audio_secs"] = json!(samples.len() as f64 / f64::from(ANALYSIS_RATE));
        // The levels the rule sees (20 ms frames, dBFS).
        let n = (f64::from(ANALYSIS_RATE) * 0.02) as usize;
        let mut levels: Vec<f32> = samples
            .chunks(n)
            .filter(|c| c.len() == n)
            .map(|c| {
                (10.0
                    * (c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / n as f64 + 1e-12)
                        .log10()) as f32
            })
            .collect();
        let thr = cm_ai::suggest::silence::adaptive_threshold_db(&levels, opts.silence_db);
        levels.sort_by(f32::total_cmp);
        out["floor_p10_db"] = json!(levels[levels.len() / 10]);
        out["speech_p90_db"] = json!(levels[levels.len() * 9 / 10]);
        out["threshold_db"] = json!(thr);
        out["frames_below_threshold_share"] =
            json!(levels.iter().filter(|l| **l < thr).count() as f64 / levels.len() as f64);
        pauses_before = silence_cuts(
            &samples,
            ANALYSIS_RATE,
            MediaTime::from_ticks(start),
            &manifest.gaps,
            &opts,
        )
        .unwrap();
        pauses_after = veto_speech(pauses_before.clone(), &speech, &opts);
    }
    let fillers = transcript
        .as_ref()
        .map(|t| filler_cuts(t, &[]))
        .unwrap_or_default();
    out["pauses_before_veto"] = json!(pauses_before.len());
    out["pauses_after_veto"] = json!(pauses_after.len());
    out["filler_cuts"] = json!(fillers.len());
    // The final list as the editor gets it.
    let services =
        cm_daemon::ai_engine::services_with(std::env::temp_dir().join("cuts_probe_models"), None);
    let finals = services.suggest.cuts(&dir, transcript.as_ref()).unwrap();
    let pauses: Vec<_> = finals.iter().filter(|c| c.kind == "silence").collect();
    out["final_total"] = json!(finals.len());
    out["final_pauses"] = json!(pauses.len());
    out["final_fillers"] = json!(finals.len() - pauses.len());
    out["final_secs_covered"] = json!(finals
        .iter()
        .map(|c| secs(c.end) - secs(c.start))
        .sum::<f64>());
    out["final_pauses_first_5_min"] =
        json!(pauses.iter().filter(|c| secs(c.start) < 300.0).count());
    let words_inside = transcript
        .iter()
        .flat_map(|t| t.words.iter())
        .filter(|w| {
            pauses
                .iter()
                .any(|c| secs(w.start) < secs(c.end) && secs(w.end) > secs(c.start))
        })
        .count();
    out["words_inside_final_pauses"] = json!(words_inside);
    let seg_inside: f64 = manifest
        .extra
        .get(cm_ai::speakers::MANIFEST_SEGMENTS)
        .and_then(|v| serde_json::from_value::<Vec<cm_ai::speakers::Segment>>(v.clone()).ok())
        .unwrap_or_default()
        .iter()
        .map(|s| {
            pauses
                .iter()
                .map(|c| (secs(s.end).min(secs(c.end)) - secs(s.start).max(secs(c.start))).max(0.0))
                .sum::<f64>()
        })
        .sum();
    out["speaker_engine_secs_inside_final_pauses"] = json!(seg_inside);
    // Does the noise-reduced copy hear speech where the pauses are? Its own floor (10th percentile)
    // against the loudest decile of each final pause.
    if let Some(t) = manifest.tracks.get("mic") {
        let den = t
            .get("file")
            .and_then(Value::as_str)
            .filter(|f| Some(*f) != t.get("raw_file").and_then(Value::as_str));
        if let Some(den) = den {
            let x = decode_mono_f32(&tools.ffmpeg, &dir.join(den), ANALYSIS_RATE).unwrap();
            let n = (f64::from(ANALYSIS_RATE) * 0.02) as usize;
            let lv: Vec<f32> = x
                .chunks(n)
                .filter(|c| c.len() == n)
                .map(|c| {
                    (10.0
                        * (c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / n as f64 + 1e-12)
                            .log10()) as f32
                })
                .collect();
            let mut sorted = lv.clone();
            sorted.sort_by(f32::total_cmp);
            let floor = sorted[sorted.len() / 10];
            out["denoised_floor_p10_db"] = json!(floor);
            out["denoised_median_db"] = json!(sorted[sorted.len() / 2]);
            let mut loud = 0;
            for c in &pauses {
                let (a, b) = (
                    (secs(c.start) / 0.02) as usize,
                    ((secs(c.end) / 0.02) as usize).min(lv.len()),
                );
                if b > a {
                    let mut f: Vec<f32> = lv[a..b].to_vec();
                    f.sort_by(f32::total_cmp);
                    if f[f.len() * 9 / 10] > floor + 20.0 {
                        loud += 1;
                    }
                }
            }
            out["final_pauses_where_denoised_copy_has_speech_level_frames"] = json!(loud);
        }
    }
    // Pauses with a gap in the pause list (where the recording was paused), and their length mix.
    let mut lens: Vec<f64> = pauses.iter().map(|c| secs(c.end) - secs(c.start)).collect();
    lens.sort_by(f64::total_cmp);
    out["pause_len_median"] = json!(lens.get(lens.len() / 2));
    println!("{out}");
}
