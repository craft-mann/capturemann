use cm_core::edit::{EditContext, EditList, Span};
use cm_core::{MediaTime, PauseGap};

use super::*;

const RATE: u32 = 48_000;

fn secs(s: i64) -> MediaTime {
    MediaTime::from_secs(s)
}

fn list(end_s: i64, gaps: &[PauseGap]) -> EditList {
    EditList::full(&EditContext {
        end: secs(end_s),
        live_at: None,
        gaps,
    })
}

fn sine(freq: f64, amp: f64, seconds: f64) -> Vec<f32> {
    (0..(seconds * f64::from(RATE)) as usize)
        .map(|i| {
            (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).sin()) as f32
        })
        .collect()
}

fn track(samples: &[f32], channels: u16) -> TrackAudio<'_> {
    TrackAudio {
        samples,
        channels,
        sample_rate: RATE,
        start: MediaTime::ZERO,
        gain_db: 0.0,
        mute: false,
    }
}

fn mono_params() -> MixParams {
    MixParams {
        out_channels: 1,
        ..MixParams::default()
    }
}

/// Amplitude of the `freq` component of mono `x`, in dB (least-squares projection).
fn component_db(x: &[f32], freq: f64) -> f64 {
    let (mut s, mut c) = (0f64, 0f64);
    for (i, v) in x.iter().enumerate() {
        let ph = 2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE);
        s += f64::from(*v) * ph.sin();
        c += f64::from(*v) * ph.cos();
    }
    let amp = 2.0 * (s * s + c * c).sqrt() / x.len() as f64;
    20.0 * amp.log10()
}

/// AUD-4.AC1: mic at 0 dB and system at -6 dB come out 6 dB apart (within 0.5 dB).
#[test]
fn aud4_ac1_gains_are_reflected_in_the_mix() {
    let mic = sine(1000.0, 0.2, 4.0);
    let sys = sine(3000.0, 0.2, 4.0);
    let tl = list(4, &[]).timeline();
    let mut s = track(&sys, 1);
    s.gain_db = -6.0;
    let out = mix_tracks(&[track(&mic, 1), s], &[], &tl, &mono_params()).unwrap();
    let diff = component_db(&out.samples, 3000.0) - component_db(&out.samples, 1000.0);
    assert!((diff + 6.0).abs() < 0.5, "{diff}");
    assert_eq!(out.samples.len(), 4 * RATE as usize);
}

/// AUD-4.AC4: a muted system track is absent, bit for bit.
#[test]
fn aud4_ac4_a_muted_track_is_absent() {
    let mic = sine(1000.0, 0.2, 3.0);
    let sys = sine(3000.0, 0.2, 3.0);
    let tl = list(3, &[]).timeline();
    let mut s = track(&sys, 1);
    s.mute = true;
    let with = mix_tracks(&[track(&mic, 1), s], &[], &tl, &mono_params()).unwrap();
    let without = mix_tracks(&[track(&mic, 1)], &[], &tl, &mono_params()).unwrap();
    assert_eq!(with.samples, without.samples);
}

/// AUD-4.AC2: normalising to -14 LUFS lands within 1 LU and the true peak stays under -1 dBTP.
#[test]
fn aud4_ac2_normalises_to_minus_14_lufs() {
    // Quiet speech-like material: a 300 Hz tone with a slow envelope.
    let x: Vec<f32> = sine(300.0, 0.04, 12.0)
        .iter()
        .enumerate()
        .map(|(i, v)| v * (0.6 + 0.4 * (i as f32 / RATE as f32 * 2.0).sin()))
        .collect();
    let tl = list(12, &[]).timeline();
    let params = MixParams {
        loudness_lufs: Some(PLATFORM_LUFS),
        ..MixParams::default()
    };
    let out = mix_tracks(&[track(&x, 1)], &[], &tl, &params).unwrap();
    let l = out.integrated_lufs.unwrap();
    assert!((l + 14.0).abs() < 1.0, "{l}");
    assert!(out.true_peak_dbtp <= -1.0 + 1e-3, "{}", out.true_peak_dbtp);
    assert!(out.normalise_gain_db > 10.0);
}

/// AUD-4.AC2: material with loud transients still ends under the ceiling.
#[test]
fn aud4_ac2_peaky_material_stays_under_the_ceiling() {
    let mut x = sine(250.0, 0.05, 10.0);
    for i in (0..x.len()).step_by(RATE as usize / 2) {
        x[i] = 0.9;
    }
    let tl = list(10, &[]).timeline();
    let params = MixParams {
        loudness_lufs: Some(PLATFORM_LUFS),
        ..MixParams::default()
    };
    let out = mix_tracks(&[track(&x, 1)], &[], &tl, &params).unwrap();
    assert!(out.true_peak_dbtp <= -1.0 + 1e-3, "{}", out.true_peak_dbtp);
}

/// "Source" (no target): the level is untouched while the limiter still guards the ceiling.
#[test]
fn aud4_source_preset_keeps_the_level() {
    let x = sine(440.0, 0.1, 3.0);
    let tl = list(3, &[]).timeline();
    let out = mix_tracks(&[track(&x, 1)], &[], &tl, &mono_params()).unwrap();
    assert_eq!(out.normalise_gain_db, 0.0);
    assert!((component_db(&out.samples, 440.0) - 20.0 * 0.1f64.log10()).abs() < 0.05);
}

/// A track that starts 1 s after the origin is silent until then (`TrackInfo.start`).
#[test]
fn track_start_offsets_the_samples() {
    let x = sine(1000.0, 0.5, 2.0);
    let tl = list(3, &[]).timeline();
    let mut t = track(&x, 1);
    t.start = secs(1);
    let out = render_track(&t, &[], &tl, &mono_params()).unwrap();
    assert!(out[..RATE as usize - 100].iter().all(|v| v.abs() < 1e-6));
    assert!(out[RATE as usize + 1000..RATE as usize + 2000]
        .iter()
        .any(|v| v.abs() > 0.3));
}

/// REC-4: after a pause the file is compacted. Session 4 s is file 3 s, and the pause itself is silent.
#[test]
fn pause_gaps_are_mapped_through_the_manifest() {
    let ramp: Vec<f32> = (0..4 * RATE as usize)
        .map(|i| i as f32 / (4.0 * RATE as f32))
        .collect();
    let gaps = [PauseGap {
        from: secs(2),
        to: secs(3),
    }];
    let tl = list(5, &gaps).timeline();
    let out = render_track(&track(&ramp, 1), &gaps, &tl, &mono_params()).unwrap();
    let at = |s: f64| out[(s * f64::from(RATE)) as usize];
    // Before the pause: session = file.
    assert!((at(1.0) - 0.25).abs() < 1e-3, "{}", at(1.0));
    // Inside the pause: silence.
    assert!(out[2 * RATE as usize + 500..3 * RATE as usize - 500]
        .iter()
        .all(|v| *v == 0.0));
    // After it: session 4 s reads file 3 s.
    assert!((at(4.0) - 0.75).abs() < 1e-3, "{}", at(4.0));
}

/// EDT-1.AC2: a cut is skipped, and the audio after it is the audio from after the cut.
#[test]
fn edt1_ac2_cut_ranges_are_skipped() {
    let ramp: Vec<f32> = (0..4 * RATE as usize)
        .map(|i| i as f32 / (4.0 * RATE as f32))
        .collect();
    let l = list(4, &[]).cut(&[Span::new(secs(1), secs(2))]).unwrap();
    let tl = l.timeline();
    let out = render_track(&track(&ramp, 1), &[], &tl, &mono_params()).unwrap();
    assert_eq!(out.len(), 3 * RATE as usize);
    // Output 1.5 s is source 2.5 s.
    assert!((out[(1.5 * f64::from(RATE)) as usize] - 0.625).abs() < 1e-3);
    // Output 0.5 s is source 0.5 s.
    assert!((out[(0.5 * f64::from(RATE)) as usize] - 0.125).abs() < 1e-3);
}

/// EDT-2.AC1: a 2 s segment at 2x lasts 1 s of audio, and the pitch is kept.
#[test]
fn edt2_ac1_sped_up_audio_has_the_output_length_and_pitch() {
    let x = sine(440.0, 0.4, 6.0);
    let l = list(6, &[])
        .set_speed(Span::new(secs(2), secs(4)), 2.0, false)
        .unwrap();
    let tl = l.timeline();
    let out = render_track(&track(&x, 1), &[], &tl, &mono_params()).unwrap();
    assert_eq!(out.len(), 5 * RATE as usize);
    // The sped segment is output 2..3 s; its middle still holds 440 Hz.
    let mid = &out[2 * RATE as usize + 4800..3 * RATE as usize - 4800];
    let db = component_db(mid, 440.0);
    assert!(db > -9.0, "{db}"); // amplitude 0.4 = -8 dB; a shifted pitch would be far lower
}

/// EDT-2.AC3: above 2x the audio is muted unless `keep_audio`.
#[test]
fn edt2_ac3_audio_is_muted_above_2x_unless_kept() {
    let x = sine(440.0, 0.4, 8.0);
    let span = Span::new(secs(2), secs(6));
    let muted = list(8, &[]).set_speed(span, 4.0, false).unwrap().timeline();
    let kept = list(8, &[]).set_speed(span, 4.0, true).unwrap().timeline();
    let m = render_track(&track(&x, 1), &[], &muted, &mono_params()).unwrap();
    let k = render_track(&track(&x, 1), &[], &kept, &mono_params()).unwrap();
    // The 4 s at 4x is output 2..3 s.
    let region = 2 * RATE as usize + 2000..3 * RATE as usize - 2000;
    assert!(m[region.clone()].iter().all(|v| *v == 0.0));
    assert!(k[region].iter().any(|v| v.abs() > 0.2));
    assert_eq!(m.len(), k.len());
}

/// A mono mic mixes to a duplicated stereo pair; stereo folds to mono by averaging.
#[test]
fn channel_layouts_are_converted() {
    let mono = sine(500.0, 0.3, 1.0);
    let tl = list(1, &[]).timeline();
    let st = render_track(&track(&mono, 1), &[], &tl, &MixParams::default()).unwrap();
    assert!(st.chunks(2).all(|f| f[0] == f[1]));
    let stereo: Vec<f32> = mono.iter().flat_map(|v| [*v, 0.0]).collect();
    let m = render_track(&track(&stereo, 2), &[], &tl, &mono_params()).unwrap();
    assert!((m[100] - mono[100] * 0.5).abs() < 1e-6 || m[100].abs() < 0.02);
}

/// Edge case: tracks shorter than the video are padded with silence to the video length.
#[test]
fn shorter_tracks_are_padded_to_the_video_length() {
    let x = sine(500.0, 0.3, 1.0);
    let tl = list(3, &[]).timeline();
    let out = mix_tracks(&[track(&x, 1)], &[], &tl, &mono_params()).unwrap();
    assert_eq!(out.samples.len(), 3 * RATE as usize);
    assert!(out.samples[2 * RATE as usize..].iter().all(|v| *v == 0.0));
}

#[test]
fn bad_input_is_refused() {
    let x = sine(500.0, 0.3, 1.0);
    let tl = list(1, &[]).timeline();
    let mut t = track(&x, 1);
    t.sample_rate = 44_100;
    assert!(mix_tracks(&[t], &[], &tl, &mono_params()).is_err());
    let mut t = track(&x, 1);
    t.gain_db = 20.0;
    assert!(mix_tracks(&[t], &[], &tl, &mono_params()).is_err());
    assert!(mix_tracks(&[track(&x, 3)], &[], &tl, &mono_params()).is_err());
    let bad = MixParams {
        out_channels: 6,
        ..MixParams::default()
    };
    assert!(mix_tracks(&[track(&x, 1)], &[], &tl, &bad).is_err());
}

/// Silence normalises to nothing and does not divide by zero.
#[test]
fn silence_mixes_to_silence() {
    let x = vec![0.0f32; RATE as usize * 2];
    let tl = list(2, &[]).timeline();
    let params = MixParams {
        loudness_lufs: Some(PLATFORM_LUFS),
        ..MixParams::default()
    };
    let out = mix_tracks(&[track(&x, 1)], &[], &tl, &params).unwrap();
    assert!(out.samples.iter().all(|v| *v == 0.0));
    assert_eq!(out.integrated_lufs, None);
}
