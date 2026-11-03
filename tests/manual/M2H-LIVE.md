# M2H-LIVE: transcribe while recording, recording safety gate (needs the maintainer's consent)

**Do not run this without the maintainer's consent and a time slot.** It records the microphone for 20 minutes twice.
The feature is wired (`crates/cm-daemon/src/ai/live.rs`): an audio recording is followed by a background task that reads
each finished piece of `mic.denoised.wav` with 2 Whisper threads and saves `transcript.live.json` in the bundle; at Stop the
ordinary `transcribe` job resumes from it. Tested with scripted engines only (`tests/ai_live.rs`, `tests/live_recording.rs`);
this script is the hardware gate it must pass before it may be on by default.

Purpose: show that transcribing finished pieces while an audio recording runs does not make the recording's audio path late.
If it does, ship the feature off by default (decision rule in the plan).

## Setup
1. A Mac with the build to test (`make bundle-dev`), a speech model installed (Large v3 Turbo), `ai.auto_transcribe` on.
2. Nothing else heavy running; on AC power; note the machine, macOS version and free RAM.
3. A source of steady speech near the microphone (a podcast on a speaker) so Whisper has work to do; keep the level constant.
4. Two runs, same room, same source, same length: **A** with `ai.transcribe_while_recording` off (`capturemann settings set ai.transcribe_while_recording false`), **B** with it on (the default when `ai.auto_transcribe` is on). The saved state appears as `transcript.live.json` in the bundle folder once 10 minutes are on disk; a screen recording never writes one.

## Steps (each run)
1. Start an audio recording (Record sheet, audio only, microphone). Note the time.
2. In a second terminal sample CPU, GPU and memory once a second for the whole run, for example
   `top -l 0 -s 1 -stats pid,command,cpu,mem | grep -E "cm-desktop|cm-daemon|llama"` and `sudo powermetrics --samplers gpu_power -i 1000`
   (or Activity Monitor's GPU history). Save the output next to the report.
3. Let it run 20 minutes without touching the machine. In run B, look for `transcript.live.json` in the bundle folder after 11 minutes
   (the first piece is read once more than 10 minutes are on disk, the next ones about every 5 minutes after that); no job is listed for it.
4. Stop. Time from Stop to "transcript ready" (the `transcript.ready` event or the library row): note it.
5. Open the bundle's `manifest.json`.

## What to compare (A against B)
- `tracks.mic.details.gaps` (dropouts) and `drift_ppm`: both must stay inside the 50 ms budget of REC-2; B must not have
  more gaps than A.
- The mic file length against the wall clock of the recording (within the session clock tolerance).
- Peak and mean CPU of the recorder process; GPU use; memory; whether the audio callback ever reports underruns
  (`recording.health` events).
- Stop-to-transcript time: B must be under 30 s for a 20 minute recording (the plan's target for 1 hour is also under 30 s).
- The transcripts of A and B: `diff` the words (they must be equal; if not, list the differences).

## Pass criteria
No new gaps, drift inside the budget, no underrun events in B, Stop-to-ready under 30 s in B, equal words. Write the report
in `verification/` from the template in its README (say "verified on hardware (as reported)" only with it).
