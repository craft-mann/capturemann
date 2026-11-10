//! Counts what the unsupported-line guard (`cm_ai::guard`, AI-1.AC12) would drop from a stored
//! transcript, without changing anything. Prints numbers and time ranges only, never text.
//!
//! `outro_guard <transcript.json> <audio.s16le>` where the audio is the recording's microphone
//! as raw 16 kHz mono 16-bit samples (`ffmpeg -i mic.wav -ac 1 -ar 16000 -f s16le out.raw`).

use cm_ai::guard;
use cm_ai::transcribe::Transcript;
use cm_core::MediaTime;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, transcript, audio] = &args[..] else {
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
}
