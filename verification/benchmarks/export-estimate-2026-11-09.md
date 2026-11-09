# Export size estimate against the real size (0.3.1, EXP-5.AC6)

- **Date:** 2026-11-09. **Status: PASS on the decided pass line of the upper-bound version** ("the real size is at or below the bound shown on every sample": 18 of 18). The first version of the estimate, a range of -15 % to +20 %, **failed** (12 of 18 outside; kept below as history). Measured once per cell on an Apple M4 Pro (macOS 26.6, hardware H.264 through VideoToolbox, GPU compositor), with the FFmpeg that ships (`packaging/ffmpeg/out`, 9.0.2, LGPL build). Not "verified".
- **Test:** `crates/cm-daemon/tests/estimate_vs_real.rs` (`#[ignore]`; run with `CM_ESTIMATE_REPORT=<file> packaging/ffmpeg/with-bundled-ffmpeg.sh cargo test -p cm-daemon --test estimate_vs_real --locked -- --ignored --nocapture`). Bound: `cm_media::estimate::estimate` (formula only, no probe). Real: `cm_media::export_quick`, the real exporter, at the recording size, 720p and 480p.
- **The samples are synthetic**, made with `cargo run -p cm-daemon --example make_fixture -- --content static|typical|motion --size 1920x1080 --secs 20 --no-camera` (FFmpeg `lavfi` sources, microphone and system audio included): *static* = flat panels and lines with one 256 x 144 animated region; *typical* = the same screen with a moving window over about 30 % of the frame; *motion* = a full-frame moving pattern with grain. They are **not real screen recordings**, and no screen recording was run for this report.

## What the bound is (maintainer decision of Nov 9, 2026: option 2, "Up to N MB")

Bound = (video bit-rate **ceiling** + audio bit rate) x length x 1.05, rounded **up** to two significant figures. The ceilings are the exporter's own functions, one place each:

- hardware H.264: `graph::videotoolbox_bitrate`, 12 Mbps at 1080p30 scaled by area (the `-b:v` of the encoder);
- VP9 (WebM, MP4 on Linux without H.264): `graph::vp9_cap_bitrate`, 6 Mbps at 1080p30 scaled by area, given to `libvpx-vp9` as `-b:v` next to `-crf 32` (constrained quality; before this change the exporter used `-b:v 0 -crf 32`, no ceiling). A WebM "fit under N MB" is also held at this ceiling.

## Result (bound version)

| content | format | size | pixels | bound shown | real | real / bound | real / nominal | length est. / real |
|---|---|---|---|---|---|---|---|---|
| static | mp4 | recording | 1920×1080 | Up to 33 MB | 0.96 MB at or below | 0.03 | 0.032 | 20.0 s / 20.0 s |
| static | mp4 | 720p | 1280×720 | Up to 15 MB | 0.81 MB at or below | 0.05 | 0.059 | 20.0 s / 20.0 s |
| static | mp4 | 480p | 854×480 | Up to 6.8 MB | 0.67 MB at or below | 0.10 | 0.105 | 20.0 s / 20.0 s |
| static | webm | recording | 1920×1080 | Up to 17 MB | 0.83 MB at or below | 0.05 | 0.055 | 20.0 s / 20.0 s |
| static | webm | 720p | 1280×720 | Up to 7.3 MB | 0.66 MB at or below | 0.09 | 0.096 | 20.0 s / 20.0 s |
| static | webm | 480p | 854×480 | Under 5 MB | 0.55 MB at or below | 0.16 | 0.170 | 20.0 s / 20.0 s |
| typical | mp4 | recording | 1920×1080 | Up to 33 MB | 18.03 MB at or below | 0.55 | 0.592 | 20.0 s / 20.0 s |
| typical | mp4 | 720p | 1280×720 | Up to 15 MB | 9.39 MB at or below | 0.63 | 0.680 | 20.0 s / 20.0 s |
| typical | mp4 | 480p | 854×480 | Up to 6.8 MB | 5.22 MB at or below | 0.77 | 0.814 | 20.0 s / 20.0 s |
| typical | webm | recording | 1920×1080 | Up to 17 MB | 5.99 MB at or below | 0.35 | 0.393 | 20.0 s / 20.0 s |
| typical | webm | 720p | 1280×720 | Up to 7.3 MB | 2.49 MB at or below | 0.34 | 0.361 | 20.0 s / 20.0 s |
| typical | webm | 480p | 854×480 | Under 5 MB | 1.38 MB at or below | 0.41 | 0.431 | 20.0 s / 20.0 s |
| motion | mp4 | recording | 1920×1080 | Up to 33 MB | 31.03 MB at or below | 0.94 | 1.018 | 20.0 s / 20.0 s |
| motion | mp4 | 720p | 1280×720 | Up to 15 MB | 13.87 MB at or below | 0.92 | 1.004 | 20.0 s / 20.0 s |
| motion | mp4 | 480p | 854×480 | Up to 6.8 MB | 6.31 MB at or below | 0.93 | 0.984 | 20.0 s / 20.0 s |
| motion | webm | recording | 1920×1080 | Up to 17 MB | 14.84 MB at or below | 0.87 | 0.974 | 20.0 s / 20.0 s |
| motion | webm | 720p | 1280×720 | Up to 7.3 MB | 5.78 MB at or below | 0.79 | 0.837 | 20.0 s / 20.0 s |
| motion | webm | 480p | 854×480 | Under 5 MB | 2.46 MB at or below | 0.72 | 0.768 | 20.0 s / 20.0 s |

"Nominal" is (ceiling + audio) x length before the margin and the rounding; real / nominal is the overshoot of the encoder over its own request.

- **Static screens land far under the bound** (3 % to 16 % of it): the bound is a ceiling, a quiet screen is far below it. That is the accepted trade-off of the "Up to" label. Typical screens land at 34 % to 77 %, busy ones at 72 % to 94 %.
- **Overshoot.** Hardware H.264 (VideoToolbox) overshot its own request by at most **1.8 %** (motion, 1080p: 1.018; 720p 1.004; 480p 0.984). VP9 constrained quality stayed under its cap on every sample (at most 0.974 of nominal, on the motion sample); the nominal includes audio and the container. The 5 % margin (`BOUND_MARGIN`) covers the measured 1.8 % with room; it is in the bound, not hidden. Other hardware encoders (Media Foundation, NVENC, QSV, AMF, VA-API) were not measured and may overshoot more.
- The estimated length after cuts and In/Out equals the real length on every sample (20.0 s).

## Quality of the capped WebM and the H.264 rate (information, not a pass line)

Against the master's frames (SSIM and PSNR, same size, the export starts at the master's first frame), FFmpeg on PATH. The uncapped reference is `ffmpeg -i master -c:v libvpx-vp9 -b:v 0 -crf 32 ...` (the exporter's settings before the cap), re-encoded straight from the master.

- typical, WebM at the recording size: capped 0.038 bits per pixel (6.0 MB), SSIM 0.9870, PSNR 27.32 dB; uncapped 0.044 bits per pixel (6.8 MB), SSIM 0.9901, PSNR 27.34 dB. The cap barely binds here; the small difference is the encoder's own variation (the capped export goes through the compositor, the reference does not).
- motion, WebM at the recording size: capped 0.095 bits per pixel (14.8 MB), SSIM 0.8257, PSNR 25.22 dB; uncapped **0.358 bits per pixel (55.7 MB)**, SSIM 0.8436, PSNR 25.26 dB. The cap cuts the file to a quarter and costs 0.018 SSIM and 0.04 dB PSNR on this sample.
- H.264 for comparison: typical 0.116 bits per pixel, SSIM 0.9964, PSNR 44.16 dB; motion 0.200 bits per pixel, SSIM 0.8574, PSNR 34.89 dB.
- Honest limits: the motion sample is full-frame grain and a moving test pattern, which no encoder can reproduce at these rates (SSIM 0.84 uncapped), so the numbers say the cap costs little *on content that was already near the encoder's limit*, not that the picture is good. The PSNR of the typical WebM (27 dB against 44 dB for H.264) is low in both capped and uncapped exports and is the CRF 32 setting, unchanged by this work. I did not look at the pictures; no real screen recording was used.

## History: the range version (-15 % to +20 %), failed

Same samples and the VP9 rows at the typical sample (before the cap, `-b:v 0 -crf 32`): 12 of 18 outside the range.

| content | format | size | estimate (range) | real | real / middle of the range |
|---|---|---|---|---|---|
| static | mp4 | recording | About 25–37 MB | 1.0 MB **OUTSIDE** | 0.03 |
| static | mp4 | 720p | About 11–17 MB | 0.8 MB **OUTSIDE** | 0.06 |
| static | mp4 | 480p | About 5.4–7.7 MB | 0.7 MB **OUTSIDE** | 0.11 |
| static | webm | recording | About 5.0–7.2 MB | 0.6 MB **OUTSIDE** | 0.09 |
| static | webm | 720p | Under 5 MB | 0.5 MB **OUTSIDE** | 0.17 |
| static | webm | 480p | Under 5 MB | 0.5 MB **OUTSIDE** | 0.27 |
| typical | mp4 | recording | About 25–37 MB | 18.0 MB **OUTSIDE** | 0.60 |
| typical | mp4 | 720p | About 11–17 MB | 9.4 MB **OUTSIDE** | 0.69 |
| typical | mp4 | 480p | About 5.4–7.7 MB | 5.2 MB **OUTSIDE** | 0.82 |
| typical | webm | recording | About 5.0–7.2 MB | 6.3 MB inside | 1.06 |
| typical | webm | 720p | Under 5 MB | 2.7 MB inside | 0.89 |
| typical | webm | 480p | Under 5 MB | 1.5 MB inside | 0.87 |
| motion | mp4 | recording | About 25–37 MB | 31.0 MB inside | 1.03 |
| motion | mp4 | 720p | About 11–17 MB | 13.9 MB inside | 1.02 |
| motion | mp4 | 480p | About 5.4–7.7 MB | 6.3 MB inside | 0.99 |
| motion | webm | recording | About 5.0–7.2 MB | 40.6 MB **OUTSIDE** | 6.83 |
| motion | webm | 720p | Under 5 MB | 7.3 MB **OUTSIDE** | 2.47 |
| motion | webm | 480p | Under 5 MB | 2.9 MB **OUTSIDE** | 1.69 |

Cause: hardware H.264 and constant-quality VP9 adapt their size to the picture (0.56 MB for a static screen to 40.6 MB for motion in WebM at 1080p, a ratio of 70), and a window of 1.4x cannot cover that. The first VP9 table (guessed at 0.3 to 2.6 Mbps) failed all 9 WebM samples. The maintainer chose to change what the label promises (an upper bound) and to make the bound true by capping the encoder.

Not measured: Windows (Media Foundation, NVENC, QSV, AMF), Linux (VA-API, and VP9 in MP4), 60 fps, 1440p and 4K sources, a camera bubble, clips longer than 20 s, real screen recordings.
