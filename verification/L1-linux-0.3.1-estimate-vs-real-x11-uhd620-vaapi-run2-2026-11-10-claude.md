# L1 linux 0.3.1 EXP-5.AC6 estimate vs real size, run 2 after the constant-quality bound (PARTIAL)

- **Date:** 2026-11-10. **Machine:** ThinkPad T490, Ubuntu 24.04, Intel UHD 620 (VA-API H.264, constant quality), x86_64.
- **Build:** commit f79fbf11 (`--no-default-features`); exports by the FFmpeg of the CI tarball (package run 42743388645) through `packaging/ffmpeg/with-bundled-ffmpeg.sh`; same command as run 1 (`L1-linux-0.3.1-estimate-vs-real-x11-uhd620-vaapi-2026-11-10-claude.md`), 600 s.
- **Result: PASSED, 18 of 18 samples within the bound** (run 1: 15 of 18). MP4 on this machine now counts the video at 4.0 times the table (`CQP_WORST_CASE`) and shows no typical size ("none"); WebM rows are unchanged. The worst MP4 sample (motion, 1080p, 117.97 MB) is 0.91 of its bound (130 MB). Synthetic recordings only; the factor is a worst case measured on this test, not a guarantee for noisier real screens. MP4 bounds are loose for quiet screens (0.01 to 0.10 of the bound), the accepted trade-off.

| content | format | size | pixels | bound shown | real | real / bound |
|---|---|---|---|---|---|---|
| static | mp4 | recording | 1920×1080 | Up to 130 MB | 0.72 MB | 0.01 |
| static | mp4 | 720p | 1280×720 | Up to 57 MB | 0.63 MB | 0.01 |
| static | mp4 | 480p | 854×480 | Up to 26 MB | 0.58 MB | 0.02 |
| static | webm | recording | 1920×1080 | Up to 17 MB | 0.82 MB | 0.05 |
| static | webm | 720p | 1280×720 | Up to 7.3 MB | 0.66 MB | 0.09 |
| static | webm | 480p | 854×480 | Under 5 MB | 0.53 MB | 0.16 |
| typical | mp4 | recording | 1920×1080 | Up to 130 MB | 9.84 MB | 0.08 |
| typical | mp4 | 720p | 1280×720 | Up to 57 MB | 4.53 MB | 0.08 |
| typical | mp4 | 480p | 854×480 | Up to 26 MB | 2.69 MB | 0.10 |
| typical | webm | recording | 1920×1080 | Up to 17 MB | 5.98 MB | 0.35 |
| typical | webm | 720p | 1280×720 | Up to 7.3 MB | 2.48 MB | 0.34 |
| typical | webm | 480p | 854×480 | Under 5 MB | 1.37 MB | 0.40 |
| motion | mp4 | recording | 1920×1080 | Up to 130 MB | 117.97 MB | 0.91 |
| motion | mp4 | 720p | 1280×720 | Up to 57 MB | 22.52 MB | 0.40 |
| motion | mp4 | 480p | 854×480 | Up to 26 MB | 6.82 MB | 0.26 |
| motion | webm | recording | 1920×1080 | Up to 17 MB | 14.83 MB | 0.87 |
| motion | webm | 720p | 1280×720 | Up to 7.3 MB | 5.79 MB | 0.79 |
| motion | webm | 480p | 854×480 | Under 5 MB | 2.45 MB | 0.72 |
