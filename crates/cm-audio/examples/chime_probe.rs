//! Runs the system audio check once for real: plays a 0.6 s chime through `/usr/bin/afplay` and
//! reports whether system audio capture heard it. It makes a sound and starts a system audio
//! capture (Screen Recording permission), so run it by hand; it never prompts for permission.
//!
//!   cargo run -p cm-audio --example chime_probe

use cm_audio::{
    run_check, screen_recording_permission, system_audio_factory, AfplayPlayer, ChimeConfig,
    ChimeOutcome,
};

fn main() {
    println!("screen recording: {:?}", screen_recording_permission());
    let mut n = 0u32;
    let outcome = run_check(
        &AfplayPlayer,
        system_audio_factory(Vec::new()),
        &ChimeConfig::default(),
        &mut |peak| {
            n += 1;
            let segs = (((peak + 60.0) / 60.0).clamp(0.0, 1.0) * 16.0).round() as usize;
            println!("{:>3} {:>7.1} dBFS {}", n, peak, "#".repeat(segs));
        },
    );
    match outcome {
        ChimeOutcome::Heard { peak_dbfs } => println!("HEARD (peak {peak_dbfs:.1} dBFS)"),
        ChimeOutcome::NotHeard => println!("NOT HEARD"),
        ChimeOutcome::NoPermission => println!("NO PERMISSION"),
        ChimeOutcome::Failed(m) => println!("FAILED: {m}"),
    }
}
