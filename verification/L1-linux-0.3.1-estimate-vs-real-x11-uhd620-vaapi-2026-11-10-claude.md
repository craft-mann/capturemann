# L1 linux 0.3.1 EXP-5.AC6 estimate vs real size (PARTIAL)

- **Date:** 2026-11-10. **Machine:** ThinkPad T490, Ubuntu 24.04, Intel UHD 620 (VA-API H.264 encode works), x86_64.
- **Build:** commit 2f0fd2e8 (test built on the laptop with `--no-default-features`); exports by the FFmpeg of the CI tarball (package run 42743388645) through `packaging/ffmpeg/with-bundled-ffmpeg.sh`; fixtures by the system FFmpeg.
- **Command:** `CM_FFMPEG_OUT=<package>/lib/capturemann CM_ESTIMATE_REPORT=... packaging/ffmpeg/with-bundled-ffmpeg.sh cargo test -p cm-daemon --test estimate_vs_real --no-default-features --locked -- --ignored --nocapture` (593 s).
- **Result: FAILED, 15 of 18 samples within the bound.** Synthetic recordings only. All three misses are MP4 on the high-motion recording (MP4 here is VA-API H.264, not the VP9 table the test header expects for a machine without hardware H.264): 1080p 117.97 MB vs bound 33 MB, 720p 22.52 MB vs 15 MB, 480p 6.82 MB vs 6.8 MB. WebM rows (software VP9) all within the bound (motion at most 0.87 of the bound). Likely cause (not confirmed): the VA-API path runs in constant-quality mode (`caps.vaapi_cqp`) with no rate cap, so a high-motion master is uncapped.

| content | format | size | pixels | bound shown | real | real / bound | real / nominal | typical (information) | real / typical | length est. / real |
|---|---|---|---|---|---|---|---|---|---|---|
| static | mp4 | recording | 1920×1080 | Up to 33 MB | 0.72 MB at or below | 0.02 | 0.024 | About 5.7 MB | 0.13 | 20.0 s / 20.0 s |
| static | mp4 | 720p | 1280×720 | Up to 15 MB | 0.63 MB at or below | 0.04 | 0.046 | Under 5 MB | 0.21 | 20.0 s / 20.0 s |
| static | mp4 | 480p | 854×480 | Up to 6.8 MB | 0.58 MB at or below | 0.09 | 0.091 | Under 5 MB | 0.34 | 20.0 s / 20.0 s |
| static | webm | recording | 1920×1080 | Up to 17 MB | 0.82 MB at or below | 0.05 | 0.054 | Under 5 MB | 0.82 | 20.0 s / 20.0 s |
| static | webm | 720p | 1280×720 | Up to 7.3 MB | 0.66 MB at or below | 0.09 | 0.095 | Under 5 MB | 1.06 | 20.0 s / 20.0 s |
| static | webm | 480p | 854×480 | Under 5 MB | 0.53 MB at or below | 0.16 | 0.167 | Under 5 MB | 1.24 | 20.0 s / 20.0 s |
| typical | mp4 | recording | 1920×1080 | Up to 33 MB | 9.84 MB at or below | 0.30 | 0.323 | About 31 MB | 0.32 | 20.0 s / 20.0 s |
| typical | mp4 | 720p | 1280×720 | Up to 15 MB | 4.53 MB at or below | 0.30 | 0.328 | About 14 MB | 0.32 | 20.0 s / 20.0 s |
| typical | mp4 | 480p | 854×480 | Up to 6.8 MB | 2.69 MB at or below | 0.40 | 0.420 | About 6.5 MB | 0.41 | 20.0 s / 20.0 s |
| typical | webm | recording | 1920×1080 | Up to 17 MB | 5.98 MB at or below | 0.35 | 0.393 | About 16 MB | 0.37 | 20.0 s / 20.0 s |
| typical | webm | 720p | 1280×720 | Up to 7.3 MB | 2.48 MB at or below | 0.34 | 0.359 | About 7.0 MB | 0.35 | 20.0 s / 20.0 s |
| typical | webm | 480p | 854×480 | Under 5 MB | 1.37 MB at or below | 0.40 | 0.428 | Under 5 MB | 0.42 | 20.0 s / 20.0 s |
| motion | mp4 | recording | 1920×1080 | Up to 33 MB | 117.97 MB **ABOVE** | 3.57 | 3.870 | About 31 MB | 3.81 | 20.0 s / 20.0 s |
| motion | mp4 | 720p | 1280×720 | Up to 15 MB | 22.52 MB **ABOVE** | 1.50 | 1.630 | About 14 MB | 1.61 | 20.0 s / 20.0 s |
| motion | mp4 | 480p | 854×480 | Up to 6.8 MB | 6.82 MB **ABOVE** | 1.00 | 1.065 | About 6.5 MB | 1.05 | 20.0 s / 20.0 s |
| motion | webm | recording | 1920×1080 | Up to 17 MB | 14.83 MB at or below | 0.87 | 0.973 | About 16 MB | 0.93 | 20.0 s / 20.0 s |
| motion | webm | 720p | 1280×720 | Up to 7.3 MB | 5.79 MB at or below | 0.79 | 0.839 | About 7.0 MB | 0.83 | 20.0 s / 20.0 s |
| motion | webm | 480p | 854×480 | Under 5 MB | 2.45 MB at or below | 0.72 | 0.765 | Under 5 MB | 0.74 | 20.0 s / 20.0 s |

Quality at the recording size (information, not a pass line):

- typical mp4: 0.063 bits per pixel, SSIM 0.9956, PSNR 43.52 dB (export)
- typical webm: 0.038 bits per pixel, SSIM 0.9951, PSNR 43.00 dB (export); uncapped reference: 0.044 bits per pixel (6.8 MB), SSIM 0.9900, PSNR 27.37 dB
- motion mp4: 0.759 bits per pixel, SSIM 0.9429, PSNR 38.53 dB (export)
- motion webm: 0.095 bits per pixel, SSIM 0.8402, PSNR 35.03 dB (export); uncapped reference: 0.359 bits per pixel (55.9 MB), SSIM 0.8438, PSNR 25.27 dB
