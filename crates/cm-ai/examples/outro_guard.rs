//! Counts what the unsupported-line guard (`cm_ai::guard`, AI-1.AC12) would drop from a stored
//! transcript, without changing anything. Prints numbers and time ranges only, never text.
//!
//! `outro_guard <transcript.json> <audio.s16le> [a-b ...]` where the audio is the recording's microphone
//! as raw 16 kHz mono 16-bit samples (`ffmpeg -i mic.wav -ac 1 -ar 16000 -f s16le out.raw`).

use cm_ai::guard;
use cm_ai::transcribe::Transcript;
use cm_core::MediaTime;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, transcript, audio, ranges @ ..] = &args[..] else {
        eprintln!("usage: outro_guard <transcript.json> <audio.s16le>");
        std::process::exit(2);
    };
    let t: Transcript = serde_json::from_slice(&std::fs::read(transcript).expect("transcript"))
        .expect("transcript.json");
    let raw = std::fs::read(audio).expect("audio");
    let samples: Vec<f32> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
        .collect();
    let lines = guard::lines(&t.words).len();
    let (kept, dropped) = guard::drop_unsupported(t.words.clone(), &samples, MediaTime::ZERO);
    println!(
        "words {} lines {} audio {:.0}s reference {} -> dropped {} lines, {} words",
        t.words.len(),
        lines,
        samples.len() as f64 / 16_000.0,
        guard::Reference::new(&samples).is_some(),
        dropped.len(),
        t.words.len() - kept.len()
    );
    for d in &dropped {
        println!(
            "  {:.1}-{:.1}s, {} words",
            d.start.as_secs_f64(),
            d.end.as_secs_f64(),
            d.words
        );
    }
    // Extra arguments `a-b` (seconds): the margin over the room of that stretch.
    if let Some(r) = guard::Reference::new(&samples) {
        for x in ranges {
            if let Some((a, b)) = x.split_once('-') {
                let (a, b): (f64, f64) = (a.parse().expect("seconds"), b.parse().expect("seconds"));
                println!("  range {a:.1}-{b:.1}s margin {:?} dB dB", r.margin(a, b));
            }
        }
    }
    // Lines near the rule: every word sure (0.97+), 3+ words, 1 s+, within 15 dB of the room.
    // Numbers only: margin over the room in dB, words, lowest confidence, time range.
    if let Some(r) = guard::Reference::new(&samples) {
        for l in guard::lines(&t.words) {
            let w = &t.words[l];
            let (a, b) = (w[0].start.as_secs_f64(), w[w.len() - 1].end.as_secs_f64());
            let low = w.iter().map(|x| x.confidence).fold(1.0f32, f32::min);
            let m = r.margin(a, b).unwrap_or(99.0);
            if w.len() >= 3 && b - a >= 1.0 && low >= guard::SURE && m < 15.0 {
                println!(
                    "  near {a:.1}-{b:.1}s {} words low {low:.2} margin {m:.1} dB",
                    w.len()
                );
            }
        }
    }
}
