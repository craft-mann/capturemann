# VOC-1 one-hour audio recording (soak), macOS: report

- **Date:** 2026-11-07
- **Who:** claude (run by an agent on the maintainer's Mac while it was not in use; the maintainer asked for it)
- **Machine:** Mac16,8 (Apple M4 Pro), macOS 26.6.2, built-in microphone ("MacBook Pro Microphone"), a quiet room (no speech was played)
- **Build:** the notarized release app `CaptureMann 0.2.9` (arm64 DMG, built Nov 7, 2026 from `main` before `69e57536`), copied out of the DMG to a scratch folder; started as `cm-desktop` with its own config folder, library and socket (the maintainer's data was not touched). Not the build that contains the HUD change.
- **How:** `capturemann record start --audio --preset lecture`, `agents.tool_policy.start_recording = allow` in the scratch settings, one `record status` and one process sample (RSS, CPU) per minute for 60 minutes, then `record stop`. Script: scratchpad `audio-soak.sh` (not stored in the repo).
- **Status:** PARTIAL (one machine, one hour, mic only, no system audio, no speech content, no playback by a person).

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Start the recording from the CLI (no dialog) | PASS | state `recording`, live after 220 ms |
| 60 minute samples while recording | PASS | all 60 samples answered, state `recording` every time, the app never exited |
| App memory | PASS | resident set 112.5 to 134.9 MB, 132.5 MB at minute 1, 115.6 MB at minute 60 (no growth) |
| App CPU | PASS | 3.6 % on average, 7.5 % at most |
| Stop and save | PASS | stop took 20 ms of the stop call, bundle written |
| Length | PASS | `mic.wav` and `mic.denoised.wav` are 3602.6 s (172,925,440 frames at 48 kHz); the recording ran 3600 s plus start and stop |
| Gaps, resyncs, drift | PASS | manifest: `gaps: []`, `resyncs.count: 0`, `drift_ppm: -2.33`, `max_offset_ms: 8.4` (threshold 20) |
| Files read back | PASS | `ffprobe` reads both WAV files (pcm_f32le, 48 kHz, mono) |

## Problems found

- **Disk use:** the hour wrote 1.38 GB (`mic.wav` 692 MB raw plus `mic.denoised.wav` 692 MB, both 32-bit float): about 23 MB per minute. Worth a line in the release notes for users who record long meetings.

## Not run

Transcription and speaker steps on this recording, system audio, a person listening to the result, a Mac that sleeps or loses its microphone during the hour, a laptop on battery.
