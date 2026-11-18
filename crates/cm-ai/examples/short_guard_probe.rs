//! Measures the short-audio guard (AI-1.AC12, 0.3.6): builds synthetic short clips and prints, for a
//! transcript and its audio, the numbers the doubtful-line rule reads. Not shipped; counts and
//! levels only, never text.
//!
//! ```text
//! short_guard_probe make --out <dir> --speech a.wav,b.wav,c.wav
//! short_guard_probe stats --audio <wav> [--transcript <transcript.json>]
//! ```
//!
//! `make` writes clips of 9 to 25 s: speech (from the given wav files, scaled to a level over its
//! active frames) in bursts over room noise, room noise alone, and quiet speech. `stats` prints the
//! reference (room, speech, range, non-silent seconds) and per line: words, share of low-confidence
//! words, speech-like share, margin over the room, and what [`cm_ai::guard`] does with it.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use cm_ai::audio::decode_16k_mono;
use cm_ai::guard;
use cm_ai::transcribe::Transcript;
use cm_core::MediaTime;

const RATE: usize = 16_000;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

/// RMS (dBFS) over the 20 ms frames within 20 dB of the loudest one.
fn active_db(x: &[f32]) -> f64 {
    let f = RATE / 50;
    let rms: Vec<f64> = x
        .chunks(f)
        .map(|c| (c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / c.len() as f64).sqrt())
        .collect();
    let max = rms.iter().copied().fold(0.0, f64::max);
    let (mut sum, mut n) = (0.0, 0usize);
    for r in rms.iter().filter(|r| db(**r) > db(max) - 20.0) {
        sum += r * r;
        n += 1;
    }
    db((sum / n.max(1) as f64).sqrt())
}

fn noise(len: usize, rms_db: f64, seed: u64) -> Vec<f32> {
    let rms = 10f64.powf(rms_db / 20.0);
    let mut x: u64 = 0x2545_F491_4F6C_DD1D ^ seed;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let u = (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
            (u * rms * 3f64.sqrt()) as f32
        })
        .collect()
}

fn write_wav(path: &Path, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .unwrap();
    }
    w.finalize().unwrap();
}

/// `secs` of room noise at `room_db` with `speech` (scaled to `level_db`) mixed in at each
/// `(start, speech index, seconds taken)`.
fn clip(
    secs: f64,
    room_db: f64,
    level_db: f64,
    speech: &[Vec<f32>],
    at: &[(f64, usize, f64)],
    seed: u64,
) -> Vec<f32> {
    let mut out = noise((secs * RATE as f64) as usize, room_db, seed);
    for &(start, i, take) in at {
        let s = &speech[i];
        let gain = 10f64.powf((level_db - active_db(s)) / 20.0) as f32;
        let n = ((take * RATE as f64) as usize).min(s.len());
        let off = (start * RATE as f64) as usize;
        for (k, v) in s[..n].iter().enumerate() {
            if off + k < out.len() {
                out[off + k] += v * gain;
            }
        }
    }
    out
}

fn make(out: &Path, speech: &[Vec<f32>]) {
    std::fs::create_dir_all(out).unwrap();
    let put = |name: &str, v: Vec<f32>| write_wav(&out.join(format!("{name}.wav")), &v);
    // The report's shape: two bursts of 1.5 s and 2.3 s at about -33 dBFS over a room near -60.
    put(
        "burst_two_9s",
        clip(
            9.0,
            -60.0,
            -33.0,
            speech,
            &[(1.5, 0, 1.5), (5.2, 1, 2.3)],
            1,
        ),
    );
    put("room_9s", clip(9.0, -60.0, -33.0, speech, &[], 2));
    put("room_20s", clip(20.0, -60.0, -33.0, speech, &[], 3));
    put("room_-50_12s", clip(12.0, -50.0, -33.0, speech, &[], 4));
    put(
        "speech_12s",
        clip(
            12.0,
            -60.0,
            -33.0,
            speech,
            &[(1.0, 0, 5.0), (7.0, 2, 4.5)],
            5,
        ),
    );
    put(
        "quiet_speech_12s",
        clip(
            12.0,
            -55.0,
            -45.0,
            speech,
            &[(1.0, 0, 5.0), (7.0, 2, 4.5)],
            6,
        ),
    );
    put(
        "quiet_speech_-52_12s",
        clip(
            12.0,
            -60.0,
            -52.0,
            speech,
            &[(1.0, 0, 5.0), (7.0, 2, 4.5)],
            7,
        ),
    );
    put(
        "speech_short_6s",
        clip(6.0, -60.0, -33.0, speech, &[(0.5, 1, 3.5)], 8),
    );
    put(
        "speech_25s",
        clip(
            25.0,
            -60.0,
            -33.0,
            speech,
            &[(1.0, 0, 5.0), (9.0, 1, 4.0), (17.0, 2, 5.0)],
            9,
        ),
    );
    // Very short clips: how few seconds still give a steady room and speech level.
    for secs in [2.0, 3.0, 4.0, 5.0] {
        put(
            &format!("len_{secs:.0}s"),
            clip(secs, -60.0, -33.0, speech, &[(0.4, 1, secs * 0.4)], 30),
        );
    }
    // Mostly digital silence: 6 s of zeros then 3 s of content.
    let mut z = vec![0.0f32; 6 * RATE];
    z.extend(clip(3.0, -60.0, -33.0, speech, &[(0.3, 0, 1.4)], 31));
    put("zeros_6s_then_3s", z);
    // Short fragments of speech (cut mid-word) over room noise, at two room levels and levels.
    for (k, (room, level, takes)) in [
        (-60.0, -33.0, [0.7, 1.2]),
        (-55.0, -36.0, [1.0, 1.6]),
        (-50.0, -33.0, [0.8, 1.4]),
        (-60.0, -45.0, [1.0, 1.5]),
    ]
    .iter()
    .enumerate()
    {
        for i in 0..3 {
            put(
                &format!("frag{k}_{i}"),
                clip(
                    10.0,
                    *room,
                    *level,
                    speech,
                    &[(1.5, i, takes[0]), (6.0, (i + 1) % 3, takes[1])],
                    20 + k as u64 * 3 + i as u64,
                ),
            );
        }
    }
    // Speech and a long quiet end: the shape the guard must keep judging as before.
    put(
        "tail_20s",
        clip(20.0, -60.0, -33.0, speech, &[(1.0, 0, 5.0)], 10),
    );
}

fn stats(audio: &Path, transcript: Option<&Path>) {
    let samples = decode_16k_mono(audio).unwrap();
    let secs = samples.len() as f64 / RATE as f64;
    let f = RATE / 50;
    let mut lv: Vec<f32> = samples
        .chunks_exact(f)
        .filter(|c| !cm_ai::audio::is_digital_silence(c))
        .map(|c| 10.0 * (c.iter().map(|s| s * s).sum::<f32>() / f as f32 + 1e-12).log10())
        .collect();
    lv.sort_by(f32::total_cmp);
    let (room, speech) = if lv.is_empty() {
        (0.0, 0.0)
    } else {
        (lv[lv.len() / 10], lv[lv.len() * 9 / 10])
    };
    let reference = guard::Reference::new(&samples);
    println!(
        "audio {secs:.1}s non_silent {:.1}s room {room:.1} dB speech {speech:.1} dB range {:.1} dB reference {}",
        lv.len() as f64 * 0.02,
        speech - room,
        reference.is_some()
    );
    let Some(t) = transcript else { return };
    let t: Transcript = serde_json::from_slice(&std::fs::read(t).unwrap()).unwrap();
    let Some(r) = reference else {
        println!("words {} (no reference)", t.words.len());
        return;
    };
    for l in guard::lines(&t.words) {
        let w = &t.words[l.clone()];
        let low = w
            .iter()
            .filter(|x| x.confidence < guard::LOW_CONFIDENCE)
            .count();
        let (a, b) = (w[0].start.as_secs_f64(), w[w.len() - 1].end.as_secs_f64());
        let mean = w.iter().map(|x| x.confidence).sum::<f32>() / w.len() as f32;
        println!(
            "line {a:.1}-{b:.1}s words {} low {low} mean_conf {mean:.2} margin {:?} speech_share {:?} doubtful {} unsupported {}",
            w.len(),
            r.margin(a, b),
            r.speech_share(a, b),
            guard::is_doubtful(w, MediaTime::ZERO, &r),
            guard::is_unsupported(w, MediaTime::ZERO, &r),
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("make") => {
            let speech: Vec<Vec<f32>> = option(&args, "--speech")
                .expect("--speech")
                .split(',')
                .map(|p| decode_16k_mono(Path::new(p)).unwrap())
                .collect();
            make(Path::new(&option(&args, "--out").expect("--out")), &speech);
        }
        Some("stats") => stats(
            Path::new(&option(&args, "--audio").expect("--audio")),
            option(&args, "--transcript").as_deref().map(Path::new),
        ),
        _ => eprintln!("usage: short_guard_probe make|stats ..."),
    }
}
