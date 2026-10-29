//! Which language does Whisper think this audio is in, and what does each reading say? A
//! diagnostic for "I spoke Vietnamese and the transcript is in another language" (not shipped).
//!
//! ```text
//! cargo run --release -p cm-ai --example lang_probe -- <ggml model> <audio file> [lang ...]
//! ```
//!
//! Prints the ten most likely languages from Whisper's own detector (the first 30 s), then the
//! transcript with `auto` and with each language given (default `vi en`).

#![allow(clippy::unwrap_used)]

use std::path::Path;

use cm_ai::audio::decode_16k_mono;
use cm_ai::pieces::transcribe_pieces;
use cm_ai::transcribe::Pcm16k;
use cm_ai::whisper::WhisperTranscriber;
use cm_core::MediaTime;
use whisper_rs::{WhisperContext, WhisperContextParameters};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(model), Some(audio)) = (args.first(), args.get(1)) else {
        eprintln!("usage: lang_probe <ggml model> <audio file> [lang ...]");
        std::process::exit(2);
    };
    let samples = decode_16k_mono(Path::new(audio)).unwrap();
    println!("audio {:.1} s", samples.len() as f64 / 16_000.0);

    let ctx = WhisperContext::new_with_params(model, WhisperContextParameters::default()).unwrap();
    let mut state = ctx.create_state().unwrap();
    state.pcm_to_mel(&samples, 4).unwrap();
    let (best, probs) = state.lang_detect(0, 4).unwrap();
    let mut ranked: Vec<(usize, f32)> = probs.iter().copied().enumerate().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!(
        "detected: {}",
        whisper_rs::get_lang_str(best).unwrap_or("?")
    );
    for (id, p) in ranked.iter().take(10) {
        println!(
            "  {:>3}  {:5.1} %",
            whisper_rs::get_lang_str(*id as i32).unwrap_or("?"),
            p * 100.0
        );
    }
    drop(state);
    drop(ctx);

    let t = WhisperTranscriber::load(Path::new(model), "probe", true).unwrap();
    let mut langs: Vec<String> = args.iter().skip(2).cloned().collect();
    if langs.is_empty() {
        langs = vec!["vi".into(), "en".into()];
    }
    langs.insert(0, "auto".into());
    for l in langs {
        let (out, _) = transcribe_pieces(
            &t,
            Pcm16k {
                samples: &samples,
                start: MediaTime::ZERO,
            },
            Some(&l),
            None,
            &mut |_| true,
        )
        .unwrap();
        let text: String = out
            .words
            .iter()
            .map(|w| w.text.trim())
            .collect::<Vec<_>>()
            .join(" ");
        println!("[{l} -> {}] {text}", out.language);
    }
}
