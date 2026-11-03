//! Tests with synthetic inputs: generated cursor logs, synthesised audio and scripted
//! transcripts. Each test cites the acceptance criterion it covers.

#![allow(clippy::unwrap_used)]

use cm_core::edit::{parse_time, Span};
use cm_core::{EditContext, EditList, MediaTime, PauseGap};
use cm_cursor::CursorLog;
use serde_json::{json, Value};

use super::filler::fold;
use super::*;
use crate::transcribe::{Transcript, Word};

fn t(secs: f64) -> MediaTime {
    MediaTime::from_secs_f64(secs).unwrap()
}

fn span(a: f64, b: f64) -> Span {
    Span::new(t(a), t(b))
}

fn secs(m: MediaTime) -> f64 {
    m.as_secs_f64()
}

fn edit_list(end_s: f64, gaps: &[PauseGap]) -> EditList {
    EditList::full(&EditContext {
        end: t(end_s),
        live_at: None,
        gaps,
    })
}

// ---- cursor logs -------------------------------------------------------------------------

const W: f64 = 1280.0;
const H: f64 = 720.0;

enum Ev {
    Move(f64, f64, f64),
    Click(f64, f64, f64),
}

/// A `cursor.jsonl` for a 1280x720 display from `(seconds, fraction x, fraction y)` events.
fn log_of(events: &[Ev]) -> CursorLog {
    let mut lines = vec![json!({
        "type": "session", "schema": 1, "tick_rate": cm_core::TICKS_PER_SECOND,
        "clock": "test", "platform": "test",
        "target": {"kind": "display", "display_id": 1, "origin": [0.0, 0.0],
                   "size": [1280, 720], "scale": 1.0},
        "clicks": "available", "scroll": "available", "move_source": "fake",
        "permission_missing": null
    })];
    for e in events {
        let (secs, fx, fy, click) = match e {
            Ev::Move(s, x, y) => (*s, *x, *y, false),
            Ev::Click(s, x, y) => (*s, *x, *y, true),
        };
        let tick = (secs * cm_core::TICKS_PER_SECOND as f64).round() as i64;
        let (x, y) = ((fx * W) as i64, (fy * H) as i64);
        lines.push(if click {
            json!({"type": "down", "t": tick, "button": "left", "x": x, "y": y})
        } else {
            json!({"type": "move", "t": tick, "x": x, "y": y})
        });
    }
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    CursorLog::parse(text.as_bytes()).unwrap()
}

/// Fast zig-zag motion (a move every 100 ms, each more than 2 % of the frame) from `from` to
/// `to` seconds: it never rests, so it is no dwell.
fn travel(from: f64, to: f64) -> Vec<Ev> {
    let mut v = Vec::new();
    let mut s = from;
    let mut k = 0;
    while s < to {
        let x = if k % 2 == 0 { 0.2 } else { 0.8 };
        v.push(Ev::Move(s, x, 0.5));
        s += 0.1;
        k += 1;
    }
    v
}

/// AI-3.AC1: three dwells of 1 s give exactly three suggestions, centred on the dwells.
#[test]
fn ai3_ac1_three_dwells_give_three_suggestions_at_their_centres() {
    let mut ev = travel(0.0, 2.0);
    // Dwell 1: 2.0 to 3.0 s at (0.30, 0.40), with a jitter inside 1 % of the frame.
    ev.push(Ev::Move(2.0, 0.30, 0.40));
    ev.push(Ev::Move(2.4, 0.305, 0.402));
    ev.extend(travel(3.0, 9.0));
    ev.push(Ev::Move(9.0, 0.60, 0.55));
    ev.extend(travel(10.0, 16.0));
    ev.push(Ev::Move(16.0, 0.45, 0.30));
    ev.extend(travel(17.0, 19.95));
    let log = log_of(&ev);
    let z = suggest_zooms(&log, &edit_list(20.0, &[]), &[], &ZoomOptions::default()).unwrap();
    assert_eq!(z.len(), 3, "{z:#?}");
    for (s, centre) in z.iter().zip([2.5, 9.5, 16.5]) {
        assert_eq!(s.reason, "dwell");
        assert!(
            (secs(s.center()) - centre).abs() <= 0.05,
            "centre {} vs {centre}",
            secs(s.center())
        );
        // 1 s dwell + 0.4 s ease on each side.
        assert!((secs(s.end) - secs(s.start) - 1.8).abs() < 0.01);
        assert_eq!(s.scale, 1.8);
    }
    assert!((z[0].focus.0 - 0.3025).abs() < 0.01 && (z[0].focus.1 - 0.401).abs() < 0.01);
    assert!((z[1].focus.0 - 0.60).abs() < 0.01 && (z[2].focus.1 - 0.30).abs() < 0.01);
}

/// AI-3.AC2: two clicks one second apart merge into one suggestion (the stronger, here the
/// earlier of two equal clicks).
#[test]
fn ai3_ac2_two_clicks_one_second_apart_give_one_suggestion() {
    let mut ev = travel(0.0, 3.0);
    ev.push(Ev::Click(3.0, 0.2, 0.3));
    ev.extend(travel(3.1, 4.0));
    ev.push(Ev::Click(4.0, 0.7, 0.6));
    ev.extend(travel(4.1, 11.95));
    let z = suggest_zooms(
        &log_of(&ev),
        &edit_list(12.0, &[]),
        &[],
        &ZoomOptions::default(),
    )
    .unwrap();
    assert_eq!(z.len(), 1, "{z:#?}");
    assert_eq!(z[0].reason, "click");
    // Centred on the click, 1.5 s long.
    assert!((secs(z[0].center()) - 3.0).abs() < 0.001);
    assert!((secs(z[0].end) - secs(z[0].start) - 1.5).abs() < 0.001);
    assert!((z[0].focus.0 - 0.2778).abs() < 0.001, "{:?}", z[0].focus);
}

/// AI-3: a click inside a dwell makes no extra suggestion; a dwell beats a click it merges with.
#[test]
fn ai3_a_click_inside_a_dwell_is_part_of_the_dwell() {
    let mut ev = travel(0.0, 5.0);
    ev.push(Ev::Move(5.0, 0.5, 0.5));
    ev.push(Ev::Click(5.5, 0.5, 0.5));
    ev.extend(travel(6.0, 11.95));
    let z = suggest_zooms(
        &log_of(&ev),
        &edit_list(12.0, &[]),
        &[],
        &ZoomOptions::default(),
    )
    .unwrap();
    assert_eq!(z.len(), 1, "{z:#?}");
    assert_eq!(z[0].reason, "dwell");
}

/// AI-3: a pointer parked longer than 2.6 s is not a dwell, but a click while parked still is a
/// click suggestion.
#[test]
fn ai3_a_parked_pointer_is_not_a_dwell() {
    let mut ev = travel(0.0, 2.0);
    ev.push(Ev::Move(2.0, 0.5, 0.5));
    ev.push(Ev::Click(4.0, 0.5, 0.5));
    ev.push(Ev::Move(8.0, 0.1, 0.1));
    let z = suggest_zooms(
        &log_of(&ev),
        &edit_list(12.0, &[]),
        &[],
        &ZoomOptions::default(),
    )
    .unwrap();
    assert_eq!(z.len(), 1, "{z:#?}");
    assert_eq!(z[0].reason, "click");
}

/// AI-3.AC3: suggesting changes nothing; the edit list is only touched when the accepted
/// parameters go through `set_zooms`, and they are valid zoom regions.
#[test]
fn ai3_ac3_suggestions_change_nothing_until_accepted() {
    let mut ev = travel(0.0, 3.0);
    ev.push(Ev::Click(3.0, 0.5, 0.5));
    ev.extend(travel(3.1, 8.0));
    let edits = edit_list(12.0, &[]);
    let before = edits.clone();
    let z = suggest_zooms(&log_of(&ev), &edits, &[], &ZoomOptions::default()).unwrap();
    assert_eq!(edits, before);
    assert_eq!(before.view.zooms.len(), 0);
    let p = zooms_params(&z);
    assert_eq!(p["replace"], json!(false));
    let first = &p["zooms"][0];
    let region = z[0].to_region();
    assert_eq!(parse_time("at", &first["at"]).unwrap(), region.at);
    assert_eq!(
        parse_time("length", &first["length"]).unwrap(),
        region.length
    );
    for s in &z {
        s.to_region().validate("zoom").unwrap();
    }
}

/// AI-3: no telemetry is `invalid_input`.
#[test]
fn ai3_no_cursor_data_is_invalid_input() {
    let err = suggest_zooms(
        &log_of(&[]),
        &edit_list(10.0, &[]),
        &[],
        &ZoomOptions::default(),
    )
    .unwrap_err();
    assert_eq!(err.code().as_str(), "invalid_input");
    assert!(err.to_string().contains("no cursor data"), "{err}");
}

/// AI-3: the focus is clamped so the zoomed view stays inside the frame.
#[test]
fn ai3_focus_is_clamped_inside_the_frame() {
    let mut ev = travel(0.0, 3.0);
    ev.push(Ev::Click(3.0, 0.01, 0.99));
    ev.extend(travel(3.1, 6.0));
    let z = suggest_zooms(
        &log_of(&ev),
        &edit_list(12.0, &[]),
        &[],
        &ZoomOptions::default(),
    )
    .unwrap();
    let half = 0.5 / 1.8;
    assert!((z[0].focus.0 - half as f32).abs() < 1e-4);
    assert!((z[0].focus.1 - (1.0 - half) as f32).abs() < 1e-4);
}

/// AI-3: a zoom never overlaps a cut or a recording pause; a region cut too short is dropped.
#[test]
fn ai3_zooms_never_overlap_cuts_or_pauses() {
    let mut ev = travel(0.0, 3.0);
    ev.push(Ev::Click(3.0, 0.5, 0.5));
    ev.extend(travel(3.1, 8.0));
    ev.push(Ev::Click(8.0, 0.4, 0.4));
    ev.extend(travel(8.1, 11.0));
    let log = log_of(&ev);
    // A cut over the first click leaves 0.55 s before it: dropped. The second click is fine.
    let cut = edit_list(14.0, &[]).cut(&[span(2.8, 4.5)]).unwrap();
    let z = suggest_zooms(&log, &cut, &[], &ZoomOptions::default()).unwrap();
    assert_eq!(z.len(), 1, "{z:#?}");
    assert!((secs(z[0].center()) - 8.0).abs() < 0.001);
    // A pause right after the second click clips its region to the part before it.
    let gaps = [PauseGap {
        from: t(8.5),
        to: t(9.5),
    }];
    let z = suggest_zooms(
        &log,
        &edit_list(14.0, &gaps),
        &gaps,
        &ZoomOptions::default(),
    )
    .unwrap();
    let last = z.last().unwrap();
    assert!(secs(last.end) <= 8.5 + 1e-9, "{last:?}");
    for s in &z {
        assert!(!overlaps(s_span(s), span(8.5, 9.5)) || s.start >= t(9.5));
    }
}

fn s_span(z: &ZoomSuggestion) -> Span {
    Span::new(z.start, z.end)
}

/// AI-3: at most one suggestion per 6 s of video, strongest first, and `max` caps further.
#[test]
fn ai3_density_cap_and_max() {
    let mut ev = Vec::new();
    for k in 0..5 {
        let c = 2.0 + 2.0 * f64::from(k);
        ev.extend(travel(c - 1.9, c));
        ev.push(Ev::Click(c, 0.5, 0.5));
    }
    ev.extend(travel(10.1, 11.5));
    let log = log_of(&ev);
    let edits = edit_list(12.0, &[]);
    let z = suggest_zooms(&log, &edits, &[], &ZoomOptions::default()).unwrap();
    assert_eq!(z.len(), 2, "12 s of video allow 2: {z:#?}");
    let z = suggest_zooms(
        &log,
        &edits,
        &[],
        &ZoomOptions {
            max: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(z.len(), 1);
}

/// AI-3: the same input gives the same output, and a bad scale is rejected.
#[test]
fn ai3_is_deterministic_and_validates_the_scale() {
    let mut ev = travel(0.0, 3.0);
    ev.push(Ev::Click(3.0, 0.5, 0.5));
    ev.extend(travel(3.1, 8.0));
    let log = log_of(&ev);
    let edits = edit_list(12.0, &[]);
    let a = suggest_zooms(&log, &edits, &[], &ZoomOptions::default()).unwrap();
    let b = suggest_zooms(&log, &edits, &[], &ZoomOptions::default()).unwrap();
    assert_eq!(a, b);
    let bad = ZoomOptions {
        scale: 5.0,
        ..Default::default()
    };
    assert_eq!(
        suggest_zooms(&log, &edits, &[], &bad)
            .unwrap_err()
            .code()
            .as_str(),
        "invalid_input"
    );
}

// ---- audio ------------------------------------------------------------------------------

const RATE: u32 = 16_000;

/// Mono audio from `(seconds, level)` parts: level `Some(a)` is a 440 Hz tone of amplitude `a`,
/// `None` is digital silence, and `Hiss(a)` is deterministic noise.
enum Part {
    Tone(f64, f32),
    Quiet(f64),
    Hiss(f64, f32),
}

fn audio(parts: &[Part]) -> Vec<f32> {
    let mut out = Vec::new();
    let mut seed: u32 = 12345;
    for p in parts {
        let (secs, f): (f64, Box<dyn FnMut(usize) -> f32>) = match p {
            Part::Tone(s, a) => {
                let a = *a;
                (
                    *s,
                    Box::new(move |i| {
                        a * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / RATE as f32).sin()
                    }),
                )
            }
            Part::Quiet(s) => (*s, Box::new(|_| 0.0)),
            Part::Hiss(s, a) => {
                let a = *a;
                (
                    *s,
                    Box::new(move |_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        a * ((seed >> 8) as f32 / (1 << 23) as f32 - 1.0)
                    }),
                )
            }
        };
        let n = (secs * f64::from(RATE)) as usize;
        let mut f = f;
        out.extend((0..n).map(&mut f));
    }
    out
}

/// AI-4.AC1: ten silences of 2 s give ten suggestions, each removing 1.6 s (0.4 s stays).
#[test]
fn ai4_ac1_ten_silences_each_remove_1_6_seconds() {
    let mut parts = vec![Part::Tone(1.0, 0.3)];
    for _ in 0..10 {
        parts.push(Part::Quiet(2.0));
        parts.push(Part::Tone(1.0, 0.3));
    }
    let cuts = silence_cuts(
        &audio(&parts),
        RATE,
        MediaTime::ZERO,
        &[],
        &SilenceOptions::default(),
    )
    .unwrap();
    assert_eq!(cuts.len(), 10, "{cuts:#?}");
    for (k, c) in cuts.iter().enumerate() {
        let removed = secs(c.removes());
        assert!((removed - 1.6).abs() < 0.05, "cut {k} removes {removed}");
        // The pause starts at 1 + 3k s; 0.2 s of it stays before the cut.
        assert!((secs(c.start) - (1.2 + 3.0 * k as f64)).abs() < 0.05);
        assert_eq!(c.kind, "silence");
    }
}

/// AI-4: a pause shorter than the minimum is left alone; a lower minimum finds it.
#[test]
fn ai4_short_pauses_need_a_lower_minimum() {
    let a = audio(&[Part::Tone(1.0, 0.3), Part::Quiet(1.0), Part::Tone(1.0, 0.3)]);
    let none = silence_cuts(&a, RATE, MediaTime::ZERO, &[], &SilenceOptions::default()).unwrap();
    assert!(none.is_empty());
    let opts = SilenceOptions {
        min_silence_s: 0.5,
        ..Default::default()
    };
    let one = silence_cuts(&a, RATE, MediaTime::ZERO, &[], &opts).unwrap();
    assert_eq!(one.len(), 1);
    assert!((secs(one[0].removes()) - 0.6).abs() < 0.05);
}

/// AI-4: in a noisy room the threshold follows the noise floor, so a hissy pause is still found.
#[test]
fn ai4_adaptive_noise_floor_finds_a_pause_in_room_noise() {
    // Hiss at about -30 dBFS rms (amplitude 0.055 uniform) is above the fixed -40 dBFS level.
    let a = audio(&[
        Part::Tone(1.0, 0.5),
        Part::Hiss(2.0, 0.055),
        Part::Tone(1.0, 0.5),
        Part::Hiss(2.0, 0.055),
        Part::Tone(1.0, 0.5),
    ]);
    let cuts = silence_cuts(&a, RATE, MediaTime::ZERO, &[], &SilenceOptions::default()).unwrap();
    assert_eq!(cuts.len(), 2, "{cuts:#?}");
    // With a threshold of -40 dBFS and no adaptation nothing would match: check the maths.
    let levels = super::silence::threshold_db(&[-30.0, -30.0, -12.0, -12.0, -12.0], -40.0);
    assert!(levels > -40.0 && levels <= -20.0, "{levels}");
}

/// AI-4: the cuts are in session time: the track start shifts them.
#[test]
fn ai4_track_start_maps_to_session_time() {
    let a = audio(&[Part::Tone(1.0, 0.3), Part::Quiet(2.0), Part::Tone(1.0, 0.3)]);
    let cuts = silence_cuts(&a, RATE, t(0.58), &[], &SilenceOptions::default()).unwrap();
    assert_eq!(cuts.len(), 1);
    assert!((secs(cuts[0].start) - (0.58 + 1.2)).abs() < 0.05);
}

/// AI-4: a recording pause joins two parts of the file; a silence across the join is judged on
/// each side, and times after the pause are session times.
#[test]
fn ai4_silence_across_a_recording_pause_is_split_at_the_join() {
    // The file holds 5 s then 5 s; the pause (session 5.0 to 6.0) sits between them.
    let gaps = [PauseGap {
        from: t(5.0),
        to: t(6.0),
    }];
    // File 4.0 to 6.0 silent: 1 s on each side of the join, below the 1.2 s minimum.
    let a = audio(&[Part::Tone(4.0, 0.3), Part::Quiet(2.0), Part::Tone(4.0, 0.3)]);
    let cuts = silence_cuts(&a, RATE, MediaTime::ZERO, &gaps, &SilenceOptions::default()).unwrap();
    assert!(cuts.is_empty(), "{cuts:#?}");
    // File 3.0 to 6.0 silent: 2 s before the join (session 3 to 5) and 1 s after it.
    let a = audio(&[Part::Tone(3.0, 0.3), Part::Quiet(3.0), Part::Tone(4.0, 0.3)]);
    let cuts = silence_cuts(&a, RATE, MediaTime::ZERO, &gaps, &SilenceOptions::default()).unwrap();
    assert_eq!(cuts.len(), 1, "{cuts:#?}");
    assert!((secs(cuts[0].start) - 3.2).abs() < 0.05 && (secs(cuts[0].end) - 4.8).abs() < 0.05);
    // A silence after the pause is in session time (file 7 s is session 8 s).
    let a = audio(&[Part::Tone(7.0, 0.3), Part::Quiet(2.0), Part::Tone(1.0, 0.3)]);
    let cuts = silence_cuts(&a, RATE, MediaTime::ZERO, &gaps, &SilenceOptions::default()).unwrap();
    assert_eq!(cuts.len(), 1);
    assert!((secs(cuts[0].start) - 8.2).abs() < 0.05, "{cuts:#?}");
}

/// AI-4: options are validated.
#[test]
fn ai4_options_are_validated() {
    let a = audio(&[Part::Tone(1.0, 0.3)]);
    for opts in [
        SilenceOptions {
            silence_db: 5.0,
            ..Default::default()
        },
        SilenceOptions {
            min_silence_s: 0.1,
            ..Default::default()
        },
        SilenceOptions {
            min_silence_s: 9.0,
            ..Default::default()
        },
        SilenceOptions {
            keep_s: 2.0,
            ..Default::default()
        },
    ] {
        let err = silence_cuts(&a, RATE, MediaTime::ZERO, &[], &opts).unwrap_err();
        assert_eq!(err.code().as_str(), "invalid_input", "{opts:?}");
    }
}

/// AI-4 edge case: pauses while the screen changes are skipped unless asked for, and fillers are
/// never dropped; the heuristic counts a pixel as changed beyond a tolerance.
#[test]
fn ai4_screen_activity_heuristic() {
    struct Scripted(f64);
    impl ScreenActivity for Scripted {
        fn changed_fraction(&self, _: Span) -> f64 {
            self.0
        }
    }
    let cuts = vec![
        CutSuggestion {
            start: t(1.0),
            end: t(2.0),
            kind: "silence".into(),
            detail: "2.0 s pause".into(),
        },
        CutSuggestion {
            start: t(3.0),
            end: t(3.2),
            kind: "filler".into(),
            detail: "um".into(),
        },
    ];
    let active = Scripted(0.2);
    let kept = drop_active(cuts.clone(), &active, false);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].kind, "filler");
    assert_eq!(drop_active(cuts.clone(), &active, true).len(), 2);
    assert_eq!(drop_active(cuts.clone(), &Scripted(0.04), false).len(), 2);

    let a = vec![10u8; 100 * 4];
    assert_eq!(frame_diff_fraction(&a, &a, 8), 0.0);
    let mut b = a.clone();
    for px in b.as_chunks_mut::<4>().0.iter_mut().take(6) {
        px[0] = 200;
    }
    assert!((frame_diff_fraction(&a, &b, 8) - 0.06).abs() < 1e-9);
    // Compression noise below the tolerance is not a change; alpha is ignored.
    let mut noisy = a.clone();
    for px in noisy.as_chunks_mut::<4>().0.iter_mut() {
        px[1] = 14;
        px[3] = 0;
    }
    assert_eq!(frame_diff_fraction(&a, &noisy, 8), 0.0);
    assert_eq!(frame_diff_fraction(&a, &a[..40], 8), 1.0);
}

/// Decoding a file with the FFmpeg sidecar gives the samples back (skipped without FFmpeg).
#[test]
fn ai4_decode_reads_a_file_with_ffmpeg() {
    let Some(ffmpeg) = [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ]
    .iter()
    .map(std::path::PathBuf::from)
    .find(|p| p.exists()) else {
        println!("SKIPPED: no ffmpeg");
        return;
    };
    // A 16-bit mono WAV of a 440 Hz tone, 0.5 s, written by hand.
    let pcm: Vec<i16> = (0..8000)
        .map(|i| (12000.0 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin()) as i16)
        .collect();
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + pcm.len() as u32 * 2).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(32000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((pcm.len() as u32 * 2).to_le_bytes());
    for s in &pcm {
        wav.extend(s.to_le_bytes());
    }
    let dir = std::env::temp_dir().join(format!("cm-ai-decode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tone.wav");
    std::fs::write(&file, wav).unwrap();
    let samples = decode_mono_f32(&ffmpeg, &file, 16_000).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        (samples.len() as i64 - 8000).abs() <= 2,
        "{}",
        samples.len()
    );
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!((peak - 0.366).abs() < 0.02, "{peak}");
    let missing = decode_mono_f32(&ffmpeg, &dir.join("nope.wav"), 16_000).unwrap_err();
    assert_eq!(missing.code().as_str(), "not_found");
}

// ---- fillers ----------------------------------------------------------------------------

fn word(text: &str, start: f64, end: f64) -> Word {
    Word {
        text: text.into(),
        start: t(start),
        end: t(end),
        confidence: 0.9,
    }
}

fn transcript(language: &str, words: Vec<Word>) -> Transcript {
    Transcript {
        version: 1,
        language: language.into(),
        model: "small".into(),
        words,
        note: None,
    }
}

/// AI-4: an isolated filler is suggested with a small pad; a glued one is not.
#[test]
fn ai4_isolated_fillers_only() {
    let tr = transcript(
        "en",
        vec![
            word("So", 0.0, 0.3),
            word("um,", 0.6, 0.9), // 300 ms before, 300 ms after: isolated
            word("the", 1.2, 1.4),
            word("uh", 1.45, 1.65), // 50 ms before: glued to the sentence
            word("result", 1.7, 2.1),
            word("Ummm", 2.5, 3.0), // stretched spelling, isolated
            word("works", 3.3, 3.6),
        ],
    );
    let cuts = filler_cuts(&tr, &[]);
    assert_eq!(cuts.len(), 2, "{cuts:#?}");
    assert_eq!(cuts[0].detail, "um");
    assert!((secs(cuts[0].start) - 0.57).abs() < 1e-6 && (secs(cuts[0].end) - 0.93).abs() < 1e-6);
    assert_eq!(cuts[1].detail, "um");
    assert!(cuts.iter().all(|c| c.kind == "filler"));
}

/// AI-4.AC3: a filler inside a word ("umbrella") is not suggested.
#[test]
fn ai4_ac3_a_filler_inside_a_word_is_not_suggested() {
    let tr = transcript(
        "en",
        vec![
            word("umbrella", 1.0, 1.6),
            word("human", 3.0, 3.4),
            word("uhhuh", 5.0, 5.4),
            word("Er", 7.0, 7.2),
        ],
    );
    let cuts = filler_cuts(&tr, &[]);
    assert_eq!(cuts.len(), 1, "{cuts:#?}");
    assert_eq!(cuts[0].detail, "er");
    assert_eq!(fold("Ummm,"), "um");
    assert_eq!(fold("umbrella"), "umbrela");
}

/// AI-4: "you know" counts only as an isolated phrase.
#[test]
fn ai4_you_know_only_when_the_phrase_is_isolated() {
    let tr = transcript(
        "en-US",
        vec![
            word("it", 0.0, 0.2),
            word("you", 0.9, 1.0),
            word("know", 1.05, 1.25),
            word("works", 1.9, 2.3),
            word("did", 3.0, 3.2),
            word("you", 3.25, 3.4),
            word("know", 3.45, 3.7),
            word("that", 3.75, 4.0),
        ],
    );
    let cuts = filler_cuts(&tr, &[]);
    assert_eq!(cuts.len(), 1, "{cuts:#?}");
    assert_eq!(cuts[0].detail, "you know");
    assert!(secs(cuts[0].start) < 0.9 && secs(cuts[0].end) > 1.25);
}

/// AI-4: Vietnamese fillers, a language without a list, and extra words.
#[test]
fn ai4_vietnamese_and_other_languages() {
    let vi = transcript(
        "vi",
        vec![
            word("Xin", 0.0, 0.2),
            word("ờ", 0.6, 0.9),
            word("chào", 1.3, 1.6),
            word("ừ", 2.0, 2.2), // "yes": not a filler
            word("ừm", 2.6, 3.0),
        ],
    );
    let cuts = filler_cuts(&vi, &[]);
    assert_eq!(
        cuts.iter().map(|c| c.detail.as_str()).collect::<Vec<_>>(),
        ["ờ", "ừm"]
    );
    let fr = transcript("fr", vec![word("euh", 1.0, 1.4)]);
    assert!(filler_cuts(&fr, &[]).is_empty());
    assert_eq!(filler_cuts(&fr, &["euh".to_string()]).len(), 1);
    assert!(filler_cuts(&transcript("en", vec![]), &[]).is_empty());
}

// ---- merging and accepting ---------------------------------------------------------------

/// AI-4.AC2: accepting everything is one `cut_segments` call: one undo step, the kept length
/// drops by exactly what the suggestions remove, and a filler inside a pause is not counted twice.
#[test]
fn ai4_ac2_accept_all_is_one_cut_call() {
    let silence = vec![
        CutSuggestion {
            start: t(2.2),
            end: t(3.8),
            kind: "silence".into(),
            detail: "2.0 s pause".into(),
        },
        CutSuggestion {
            start: t(7.2),
            end: t(8.8),
            kind: "silence".into(),
            detail: "2.0 s pause".into(),
        },
    ];
    let fillers = vec![
        CutSuggestion {
            start: t(3.0),
            end: t(3.3),
            kind: "filler".into(),
            detail: "um".into(),
        },
        CutSuggestion {
            start: t(5.0),
            end: t(5.4),
            kind: "filler".into(),
            detail: "uh".into(),
        },
    ];
    let edits = edit_list(12.0, &[]);
    let merged = merge_cuts(silence, fillers, &edits, &[]);
    assert_eq!(
        merged.len(),
        3,
        "the filler inside the pause is dropped: {merged:#?}"
    );
    let starts: Vec<f64> = merged.iter().map(|c| secs(c.start)).collect();
    assert_eq!(starts, [2.2, 5.0, 7.2]);

    let summary = CutSummary::of(&merged);
    assert_eq!((summary.pauses, summary.fillers), (2, 1));
    assert!((secs(summary.removes) - 3.6).abs() < 1e-6);
    assert_eq!(summary.label(), "2 pauses, 1 filler, remove 4 s");

    // Apply through the same parsing `cut_segments` uses: one call, all ranges.
    let p = cuts_params(&merged);
    let ranges: Vec<Span> = p["ranges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            Span::new(
                parse_time("from", &r[0]).unwrap(),
                parse_time("to", &r[1]).unwrap(),
            )
        })
        .collect();
    let after = edits.cut(&ranges).unwrap();
    assert_eq!(after.cuts.len(), 3);
    let kept = |l: &EditList| l.timeline().duration();
    let drop = secs(kept(&edits)) - secs(kept(&after));
    assert!((drop - secs(summary.removes)).abs() < 1e-6, "{drop}");
}

/// AI-4: suggestions never overlap cuts that exist, stay in the kept range, and sort by start.
#[test]
fn ai4_merge_respects_existing_cuts_and_the_kept_range() {
    let edits = edit_list(12.0, &[])
        .trim(
            t(1.0),
            t(11.0),
            &EditContext {
                end: t(12.0),
                live_at: None,
                gaps: &[],
            },
        )
        .unwrap()
        .cut(&[span(4.0, 5.0)])
        .unwrap();
    let silence = vec![
        CutSuggestion {
            start: t(0.2),
            end: t(1.8),
            kind: "silence".into(),
            detail: "2.0 s pause".into(),
        },
        CutSuggestion {
            start: t(3.0),
            end: t(6.0),
            kind: "silence".into(),
            detail: "3.4 s pause".into(),
        },
    ];
    let merged = merge_cuts(silence, Vec::new(), &edits, &[]);
    let spans: Vec<(f64, f64)> = merged
        .iter()
        .map(|c| (secs(c.start), secs(c.end)))
        .collect();
    assert_eq!(spans, [(1.0, 1.8), (3.0, 4.0), (5.0, 6.0)]);
}

/// The agent-facing JSON of a suggestion round-trips (the daemon returns it as the result).
#[test]
fn suggestions_serialize() {
    let z = ZoomSuggestion {
        start: t(1.0),
        end: t(2.0),
        focus: (0.4, 0.6),
        scale: 1.8,
        reason: "click".into(),
    };
    let v: Value = serde_json::to_value(&z).unwrap();
    let back: ZoomSuggestion = serde_json::from_value(v).unwrap();
    assert_eq!(back, z);
}

// ---- quiet recordings (M2g) -------------------------------------------------------------------

/// A quiet speech-like signal at `speech_db` dBFS RMS: syllables (a tone with a wobbling envelope,
/// 0.22 s) with 0.08 s of room hiss between them, inside each `(from, to)` span; room hiss at
/// `noise_db` dBFS RMS elsewhere.
fn quiet_speech(total: f64, spans: &[(f64, f64)], speech_db: f32, noise_db: f32) -> Vec<f32> {
    let amp = |db: f32| 10f32.powf(db / 20.0);
    let mut seed: u32 = 777;
    let mut hiss = move |rms: f32| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        rms * 3f32.sqrt() * ((seed >> 8) as f32 / (1 << 23) as f32 - 1.0)
    };
    let n = (total * f64::from(RATE)) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            let speaking = spans.iter().any(|&(a, b)| t >= a && t < b);
            if !speaking {
                return hiss(amp(noise_db));
            }
            let phase = t % 0.30;
            if phase >= 0.22 {
                return hiss(amp(noise_db));
            }
            let env = 0.7 + 0.3 * (t as f32 * 5.0).sin();
            // RMS of the sine is amp/sqrt 2, of the envelope about 0.7: scale to the stated level.
            let a = amp(speech_db) * 2f32.sqrt() / 0.72 * env * 0.95;
            a * (2.0 * std::f32::consts::PI * 300.0 * t as f32).sin()
        })
        .collect()
}

/// A gating denoiser: frames under -50 dBFS are pushed down 1.5 dB per dB, up to 40 dB.
fn gated(x: &[f32]) -> Vec<f32> {
    let n = (RATE / 50) as usize;
    let mut out = Vec::with_capacity(x.len());
    for c in x.chunks(n) {
        let ms = c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / c.len() as f64;
        let l = 10.0 * (ms + 1e-12).log10();
        let atten = ((-50.0 - l) * 1.5).clamp(0.0, 40.0);
        let g = 10f64.powf(-atten / 20.0) as f32;
        out.extend(c.iter().map(|s| s * g));
    }
    out
}

const QUIET_SPANS: [(f64, f64); 3] = [(0.0, 8.0), (10.0, 18.0), (21.0, 30.0)];

fn inside_speech(cuts: &[CutSuggestion], spans: &[(f64, f64)]) -> f64 {
    cuts.iter()
        .map(|c| {
            let (a, b) = (secs(c.start), secs(c.end));
            spans
                .iter()
                .map(|&(s, e)| (b.min(e) - a.max(s)).max(0.0))
                .sum::<f64>()
        })
        .sum()
}

/// M2g: speech at -45 dBFS RMS over room noise at -60 dBFS gets the two real pauses suggested and
/// nothing inside speech, on the raw signal and after a gating denoiser has pushed the quiet
/// stretches towards silence (the floor estimate then sits far below -60 dBFS).
#[test]
fn m2g_quiet_speech_is_not_cut_raw_or_denoised() {
    let raw = quiet_speech(30.0, &QUIET_SPANS, -45.0, -60.0);
    for (name, x) in [("raw", raw.clone()), ("gated", gated(&raw))] {
        let cuts =
            silence_cuts(&x, RATE, MediaTime::ZERO, &[], &SilenceOptions::default()).unwrap();
        assert_eq!(cuts.len(), 2, "{name}: {cuts:#?}");
        assert!(
            inside_speech(&cuts, &QUIET_SPANS) < 0.3,
            "{name}: {cuts:#?}"
        );
        assert!(
            secs(cuts[0].start) >= 8.0 && secs(cuts[0].end) <= 10.1,
            "{name}"
        );
        assert!(
            secs(cuts[1].start) >= 18.0 && secs(cuts[1].end) <= 21.1,
            "{name}"
        );
    }
}

/// The same signal with the fixed -40 dBFS rule of 0.2.6 cuts into the speech: the baseline this
/// change was made against (`adaptive: false` keeps that rule for callers who set a level).
#[test]
fn m2g_the_fixed_rule_cuts_quiet_speech() {
    let x = quiet_speech(30.0, &QUIET_SPANS, -45.0, -60.0);
    let opts = SilenceOptions {
        adaptive: false,
        ..SilenceOptions::default()
    };
    let cuts = silence_cuts(&x, RATE, MediaTime::ZERO, &[], &opts).unwrap();
    assert!(inside_speech(&cuts, &QUIET_SPANS) > 10.0, "{cuts:#?}");
}

/// A loud recording is judged as before: the threshold follows its own speech level, and the pauses
/// of the ten-silences case stay the same.
#[test]
fn m2g_a_loud_recording_keeps_its_pauses() {
    let x = quiet_speech(30.0, &QUIET_SPANS, -20.0, -60.0);
    let cuts = silence_cuts(&x, RATE, MediaTime::ZERO, &[], &SilenceOptions::default()).unwrap();
    assert_eq!(cuts.len(), 2, "{cuts:#?}");
    assert!(inside_speech(&cuts, &QUIET_SPANS) < 0.3);
}

fn cut(a: f64, b: f64) -> CutSuggestion {
    CutSuggestion {
        start: t(a),
        end: t(b),
        kind: "silence".into(),
        detail: String::new(),
    }
}

/// M2g: where the transcript or the speaker engine says someone speaks, a pause is not suggested;
/// what is left on either side stays when it is still a pause.
#[test]
fn m2g_speech_evidence_vetoes_a_pause_inside_it() {
    let opts = SilenceOptions::default();
    // Speech 2.0 to 3.0 inside a pause 0.2 to 5.0: both sides stay (1.65 s and 1.85 s).
    let kept = veto_speech(vec![cut(0.2, 5.0)], &[Span::new(t(2.0), t(3.0))], &opts);
    assert_eq!(kept.len(), 2, "{kept:#?}");
    assert!((secs(kept[0].end) - 1.85).abs() < 0.01 && (secs(kept[1].start) - 3.15).abs() < 0.01);
    // Speech covering most of the pause leaves nothing worth cutting.
    let kept = veto_speech(vec![cut(0.2, 5.0)], &[Span::new(t(0.5), t(4.5))], &opts);
    assert!(kept.is_empty(), "{kept:#?}");
    // No evidence near it: untouched. Fillers are never vetoed.
    let far = Span::new(t(20.0), t(21.0));
    assert_eq!(
        veto_speech(vec![cut(0.2, 5.0)], &[far], &opts),
        vec![cut(0.2, 5.0)]
    );
    let mut filler = cut(2.0, 2.4);
    filler.kind = "filler".into();
    let kept = veto_speech(vec![filler.clone()], &[Span::new(t(1.0), t(3.0))], &opts);
    assert_eq!(kept, vec![filler]);
}

/// A level the user set forces the fixed rule.
#[test]
fn m2g_a_user_level_uses_the_fixed_rule() {
    let o = SilenceOptions::with_level(-35.0);
    assert!(!o.adaptive);
    assert_eq!(o.silence_db, -35.0);
    assert!(SilenceOptions::default().adaptive);
}

/// M2g: a quiet speaker the raw level rule would call silence is seen on the noise-reduced copy,
/// where the room noise is gone: the stretch becomes evidence and the veto removes the pause.
#[test]
fn m2g_activity_on_the_noise_reduced_copy_vetoes_a_pause() {
    // Raw: speech at -45 dBFS with room noise at -40: no usable gap between them, the raw rule
    // sees one flat level (nothing separates, nothing is suggested), so use the pause list of the
    // fixed rule to stand for "the raw rule called 8 to 18 s a pause".
    let spans = [(0.0, 8.0), (10.0, 18.0), (21.0, 30.0)];
    let denoised = gated(&quiet_speech(30.0, &spans, -45.0, -60.0));
    let act = activity_spans(&denoised, RATE, MediaTime::ZERO, &[], 20.0, 0.2);
    // One stretch per speech span (the syllables are bridged), none over the pauses.
    assert!(act.len() >= 3, "{act:?}");
    let over_pauses = act
        .iter()
        .filter(|s| {
            (secs(s.from) > 8.3 && secs(s.to) < 9.7) || (secs(s.from) > 18.3 && secs(s.to) < 20.7)
        })
        .count();
    assert_eq!(over_pauses, 0, "{act:?}");
    // A pause that the raw rule put over the second speech span is vetoed by it.
    let kept = veto_speech(vec![cut(9.0, 19.0)], &act, &SilenceOptions::default());
    assert!(
        kept.is_empty() || kept.iter().all(|c| secs(c.end) - secs(c.start) < 1.0),
        "{kept:#?}"
    );
}
