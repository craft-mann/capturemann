# VOC-1 one-hour audio recording (soak), Windows: report

- **Date:** 2026-11-07
- **Who:** claude (run by an agent on the lab laptop while it was free; the maintainer asked for it for the BA team's meeting recordings)
- **Machine:** ThinkPad T490 (hostname thinkpadt490), Windows 11 Pro, microphone "Microphone Array (Realtek(R) Audio)", system audio loopback; nothing was played or spoken during the hour
- **Build:** the 0.2.9 Windows package (installer, workflow run 42326007966, commit `69e57536`) installed silently in `%LOCALAPPDATA%\Programs\CaptureMann`; `cm-desktop.exe` started in the logged-in session with its own config folder and library (`C:\lab\audio-soak`, never the real library). Does not include the HUD change.
- **How:** `capturemann record start --audio --preset online` (microphone and system audio), `agents.tool_policy.start_recording = allow` in the scratch settings, one `record status` and one process sample per minute for 60 minutes, then `record stop`. Script: `win-audio-soak.ps1` (scratchpad, not stored in the repo).
- **Status:** PARTIAL (one machine, one hour, no speech or playback content, no sleep or microphone loss during the hour).

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Start from the CLI (no dialog) | PASS | live after 146 ms |
| 60 minute samples | PASS | the app answered every minute and never exited |
| App memory | PASS | 42 to 43 MB for minutes 2 to 60 (86 MB in the first minute), no growth |
| Stop and save | PASS | `duration_s` 3601.62, `paused_ms` 0 |
| Length | PASS | mic 3601.47 s (172,870,560 frames at 48 kHz), system 3601.60 s |
| Gaps, resyncs, drift | PASS | mic: gaps 0, resyncs 0, drift -3.7 ppm, max offset 13.5 ms; system: gaps 0, resyncs 0, drift 0 |

## Problems found

- **Disk use is large:** the hour wrote **4.15 GB** (`mic.wav`, `mic.denoised.wav` and `system.wav`, 1318.9 MB each, 32-bit float stereo at 48 kHz): about 69 MB per minute for the `online` preset. A three-hour meeting needs about 12 GB. Say so in the release text, and consider whether a smaller sample format should be a later change.
- The log line `version:` was empty: the file version field of `cm-desktop.exe` is not filled in on Windows (the installer and the app report 0.2.9 elsewhere). Not checked further.

## Not run

Transcription and speaker steps on this recording, a person listening to the result, a real meeting with speech on both sides, sleep or lid close, a USB headset unplugged, a laptop on battery.
