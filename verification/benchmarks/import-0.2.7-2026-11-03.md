# Import time (0.2.7, M3 S5)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro, internal SSD (the volume had 5.5 GB free of 926 GB, so the free-space guard was exercised too), release build of `cm-daemon`. Not "verified".
- **Question:** how long does `import_media` take per GB for a video that is used as it is, for a file that needs an editing copy, and for long audio?
- **Method:** `cargo run --release -p cm-daemon --example import_bench -- --dir <scratch> --big-secs 300` makes four fixtures with the `ffmpeg` on `PATH` (a synthetic test pattern and a sine tone; no screen, camera or microphone, no private file), imports each through the real daemon (the whole job: probe, copy into staging, editing copy when needed, sound decode to `mic.wav`, verify, install) and times the call to the finished job. The FFmpeg is the Homebrew build (the editing copy used VideoToolbox, like the shipped build); the shipped build was not used.

## Results

| File | Status | Size (MB) | Duration (s) | Import time (s) | Seconds per GB |
|---|---|---|---|---|---|
| memo.wav (15 s voice memo) | ready | 1.3 | 15 | 0.3 | n/a (fixed cost) |
| talk.m4a (1 h of AAC) | ready | 31.8 | 3600 | 5.9 | n/a (decode-bound) |
| big.mp4 (1080p30 H.264, 30 Mbit/s, 300 s) | ready | 1128.2 | 300 | 10.3 | 9.1 |
| vfr.mkv (1080p, variable frame rate, 60 s) | prepared | 93.5 | 60 | 12.1 | n/a (encode-bound) |

- **A video that is used as it is** costs about 9 s per GB on this machine, which is the copy plus the checks (the first and last 2 s decoded, the duration read back, the sound decoded). At 30 Mbit/s that is 1 s of work per 29 s of video.
- **Long audio** is bound by decoding the AAC to 48 kHz 32-bit float, not by its size: an hour takes about 6 s (roughly 600x real time) and writes about 0.7 GB of `mic.wav`. A per-GB figure would mislead (185 s per GB of AAC); read it as 1.6 s per audio hour.
- **Preparing for editing** (hardware H.264 at 0.15 bit per pixel per frame, 30 fps, 1080p) took 12.1 s for a 60 s clip, about 5x real time including the copy and the checks; a 52 min call would take roughly 10 min. It needs a hardware encoder: without one the file fails with `no_hardware_encoder`.
- **A 15 s memo** takes 0.3 s, almost all of it the fixed costs (two `ffprobe` runs, `ffmpeg` start-up).

## What this does not show

- One run per cell and one machine; a USB or network volume, a spinning disk and an Intel Mac will be slower in the copy, and the editing copy on an Intel Mac depends on its video encoder.
- Synthetic content: a real phone clip has B-frames and a different bit rate profile, so the decode checks and the preparing time may differ. HEVC sources, 4K and rotation were not timed.
- The first run's disk cache is warm for the copy (the fixtures were just written); a cold copy is bound by the disk.
- The time to transcribe after import is the transcription's own (see the other reports).
