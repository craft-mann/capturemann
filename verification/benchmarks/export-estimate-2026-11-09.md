# Export size estimate against the real size (0.3.1, EXP-5.AC6)

- **Date:** 2026-11-09. **Status: FAIL on the decided pass line** ("the shown range contains the real size on every sample"): 12 of 18 samples are outside the range with the final table (15 of 18 with the first table). Measured once per cell on an Apple M4 Pro (macOS 26.6, hardware H.264 through VideoToolbox, GPU compositor), with the FFmpeg that ships (`packaging/ffmpeg/out`, 9.0.2, LGPL build). Not "verified".
- **Test:** `crates/cm-daemon/tests/estimate_vs_real.rs` (`#[ignore]`; run with `CM_ESTIMATE_REPORT=<file> packaging/ffmpeg/with-bundled-ffmpeg.sh cargo test -p cm-daemon --test estimate_vs_real --locked -- --ignored --nocapture`). Estimate: `cm_media::estimate::estimate` (formula only, no probe). Real: `cm_media::export_quick`, the real exporter, at the recording size, 720p and 480p.
- **The samples are synthetic**, made with `cargo run -p cm-daemon --example make_fixture -- --content static|typical|motion --size 1920x1080 --secs 20 --no-camera` (FFmpeg `lavfi` sources, microphone and system audio included): *static* = flat panels and lines with one 256 x 144 animated region; *typical* = the same screen with a moving window over about 30 % of the frame; *motion* = a full-frame moving pattern with grain. They are **not real screen recordings**, and no screen recording was run for this report.

## Result with the final table (H.264 = the rate the exporter asks for; VP9 rows = the typical sample)

| content | format | size | pixels | estimate | real | real / middle of the estimate | length est. / real |
|---|---|---|---|---|---|---|---|
| static | mp4 | recording | 1920×1080 | About 25–37 MB | 1.0 MB **OUTSIDE** | 0.03 | 20.0 s / 20.0 s |
| static | mp4 | 720p | 1280×720 | About 11–17 MB | 0.8 MB **OUTSIDE** | 0.06 | 20.0 s / 20.0 s |
| static | mp4 | 480p | 854×480 | About 5.4–7.7 MB | 0.7 MB **OUTSIDE** | 0.11 | 20.0 s / 20.0 s |
| static | webm | recording | 1920×1080 | About 5.0–7.2 MB | 0.6 MB **OUTSIDE** | 0.09 | 20.0 s / 20.0 s |
| static | webm | 720p | 1280×720 | Under 5 MB | 0.5 MB **OUTSIDE** | 0.17 | 20.0 s / 20.0 s |
| static | webm | 480p | 854×480 | Under 5 MB | 0.5 MB **OUTSIDE** | 0.27 | 20.0 s / 20.0 s |
| typical | mp4 | recording | 1920×1080 | About 25–37 MB | 18.0 MB **OUTSIDE** | 0.60 | 20.0 s / 20.0 s |
| typical | mp4 | 720p | 1280×720 | About 11–17 MB | 9.4 MB **OUTSIDE** | 0.69 | 20.0 s / 20.0 s |
| typical | mp4 | 480p | 854×480 | About 5.4–7.7 MB | 5.2 MB **OUTSIDE** | 0.82 | 20.0 s / 20.0 s |
| typical | webm | recording | 1920×1080 | About 5.0–7.2 MB | 6.3 MB inside | 1.06 | 20.0 s / 20.0 s |
| typical | webm | 720p | 1280×720 | Under 5 MB | 2.7 MB inside | 0.89 | 20.0 s / 20.0 s |
| typical | webm | 480p | 854×480 | Under 5 MB | 1.5 MB inside | 0.87 | 20.0 s / 20.0 s |
| motion | mp4 | recording | 1920×1080 | About 25–37 MB | 31.0 MB inside | 1.03 | 20.0 s / 20.0 s |
| motion | mp4 | 720p | 1280×720 | About 11–17 MB | 13.9 MB inside | 1.02 | 20.0 s / 20.0 s |
| motion | mp4 | 480p | 854×480 | About 5.4–7.7 MB | 6.3 MB inside | 0.99 | 20.0 s / 20.0 s |
| motion | webm | recording | 1920×1080 | About 5.0–7.2 MB | 40.6 MB **OUTSIDE** | 6.83 | 20.0 s / 20.0 s |
| motion | webm | 720p | 1280×720 | Under 5 MB | 7.3 MB **OUTSIDE** | 2.47 | 20.0 s / 20.0 s |
| motion | webm | 480p | 854×480 | Under 5 MB | 2.9 MB **OUTSIDE** | 1.69 | 20.0 s / 20.0 s |

"Middle" is the formula's value before the -15 % / +20 % range. The "Under 5 MB" label is shown when the top of the range is at most 5 MB.

## First table (VP9 rows guessed at 0.3 to 2.6 Mbps before this run)

Same samples; only the WebM rows differed: static 0.23 to 0.47 of the middle, typical 1.5 to 2.5, motion 2.9 to 16, all 9 WebM samples outside. The MP4 rows are the table above.

## What holds and what does not

- **Hardware H.264 (MP4):** the estimate is the rate the exporter requests (`graph::videotoolbox_bitrate`, 12 Mbps at 1080p30, scaled by area) plus audio. VideoToolbox spends the whole request only on busy pictures: the *motion* samples are inside at all three sizes (0.99 to 1.03 of the middle). A *typical* screen comes out at 0.60 to 0.82 of the middle (below the range), a *static* one at 0.03 to 0.11. The request is a ceiling, not a prediction.
- **VP9 (WebM, and MP4 on Linux without H.264):** the exporter uses constant quality (CRF 32), which has no ceiling, so the size follows the picture: 0.56 MB (static) to 40.6 MB (motion) for the same 20 s at 1080p, a ratio of 70. The table rows match the *typical* sample (inside at all three sizes, 0.87 to 1.06); static is far below and motion far above.
- **Length:** the estimated length after cuts and In/Out equals the real length on every sample (20.0 s), so that part of the formula is right.
- A range of -15 % / +20 % is a window of 1.4x. The sizes of one recording at one setting differ by 30x to 70x between pictures, so **no formula over length, frame and codec alone can pass the line** while the encoders keep their adaptive rate control. I did not widen the range, change the label, or fit the table to one sample.

## Options (maintainer decision, not made here)

1. **A cheap content measure** (the plan's "bring the probe back"): scale the table by the recording's own measured rate, for example the screen master's bytes per second from the file size (no ffprobe, no decode). The master comes from the same kind of hardware encoder, so for H.264 it is a close proxy. It has to be calibrated on **real** masters on all three systems: this report's masters are libx264 CRF fixtures and say nothing about VideoToolbox, Media Foundation or VA-API masters.
2. **Change what the label promises:** "Up to N MB" for hardware H.264 (the request is a true ceiling there; the motion rows sit at it) and nothing firmer for VP9. This changes the design's range and label, so it needs the design owner.
3. **Make the exporter's size predictable** (constant bit rate for H.264; constrained quality with a bit-rate cap for VP9): static screens would get much larger files than today. Not recommended.

Not measured: Windows (Media Foundation, NVENC, QSV, AMF), Linux (VA-API, and VP9 in MP4), 60 fps, 1440p and 4K sources, a camera bubble, clips longer than 20 s, real screen recordings.
