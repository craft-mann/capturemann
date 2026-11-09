# estimate_vs_real on macOS arm64 (Apple Silicon, VideoToolbox), main d62549ec, Nov 10, 2026

Command: CM_ESTIMATE_REPORT=... packaging/ffmpeg/with-bundled-ffmpeg.sh cargo test -p cm-daemon --test estimate_vs_real --locked -- --ignored --nocapture (179 s). Result: 1 passed, 0 failed; real size at or below the bound on all 18 samples (highest real/bound 0.94, mp4 motion at the recording size). Synthetic recordings, one Mac. Windows and Linux not run (Windows blocked by Smart App Control; Linux VM has no hardware H.264 encoder).

| content | format | size | pixels | bound shown | real | real / bound | real / nominal | typical (information) | real / typical | length est. / real |
|---|---|---|---|---|---|---|---|---|---|---|
| static | mp4 | recording | 1920×1080 | Up to 33 MB | 0.96 MB at or below | 0.03 | 0.032 | About 5.7 MB | 0.17 | 20.0 s / 20.0 s |
| static | mp4 | 720p | 1280×720 | Up to 15 MB | 0.81 MB at or below | 0.05 | 0.059 | Under 5 MB | 0.27 | 20.0 s / 20.0 s |
| static | mp4 | 480p | 854×480 | Up to 6.8 MB | 0.67 MB at or below | 0.10 | 0.105 | Under 5 MB | 0.40 | 20.0 s / 20.0 s |
| static | webm | recording | 1920×1080 | Up to 17 MB | 0.83 MB at or below | 0.05 | 0.055 | Under 5 MB | 0.83 | 20.0 s / 20.0 s |
| static | webm | 720p | 1280×720 | Up to 7.3 MB | 0.66 MB at or below | 0.09 | 0.096 | Under 5 MB | 1.07 | 20.0 s / 20.0 s |
| static | webm | 480p | 854×480 | Under 5 MB | 0.55 MB at or below | 0.16 | 0.170 | Under 5 MB | 1.27 | 20.0 s / 20.0 s |
| typical | mp4 | recording | 1920×1080 | Up to 33 MB | 18.03 MB at or below | 0.55 | 0.592 | About 31 MB | 0.58 | 20.0 s / 20.0 s |
| typical | mp4 | 720p | 1280×720 | Up to 15 MB | 9.39 MB at or below | 0.63 | 0.680 | About 14 MB | 0.67 | 20.0 s / 20.0 s |
| typical | mp4 | 480p | 854×480 | Up to 6.8 MB | 5.22 MB at or below | 0.77 | 0.814 | About 6.5 MB | 0.80 | 20.0 s / 20.0 s |
| typical | webm | recording | 1920×1080 | Up to 17 MB | 5.99 MB at or below | 0.35 | 0.393 | About 16 MB | 0.37 | 20.0 s / 20.0 s |
| typical | webm | 720p | 1280×720 | Up to 7.3 MB | 2.49 MB at or below | 0.34 | 0.361 | About 7.0 MB | 0.36 | 20.0 s / 20.0 s |
| typical | webm | 480p | 854×480 | Under 5 MB | 1.38 MB at or below | 0.41 | 0.431 | Under 5 MB | 0.42 | 20.0 s / 20.0 s |
| motion | mp4 | recording | 1920×1080 | Up to 33 MB | 31.03 MB at or below | 0.94 | 1.018 | About 31 MB | 1.00 | 20.0 s / 20.0 s |
| motion | mp4 | 720p | 1280×720 | Up to 15 MB | 13.87 MB at or below | 0.92 | 1.004 | About 14 MB | 0.99 | 20.0 s / 20.0 s |
| motion | mp4 | 480p | 854×480 | Up to 6.8 MB | 6.31 MB at or below | 0.93 | 0.984 | About 6.5 MB | 0.97 | 20.0 s / 20.0 s |
| motion | webm | recording | 1920×1080 | Up to 17 MB | 14.84 MB at or below | 0.87 | 0.974 | About 16 MB | 0.93 | 20.0 s / 20.0 s |
| motion | webm | 720p | 1280×720 | Up to 7.3 MB | 5.78 MB at or below | 0.79 | 0.837 | About 7.0 MB | 0.83 | 20.0 s / 20.0 s |
| motion | webm | 480p | 854×480 | Under 5 MB | 2.46 MB at or below | 0.72 | 0.768 | Under 5 MB | 0.75 | 20.0 s / 20.0 s |

Quality at the recording size (information, not a pass line):

- typical mp4: 0.116 bits per pixel, SSIM 0.9964, PSNR 44.16 dB (export)
- typical webm: 0.038 bits per pixel, SSIM 0.9870, PSNR 27.32 dB (export); uncapped reference: 0.044 bits per pixel (6.8 MB), SSIM 0.9901, PSNR 27.34 dB
- motion mp4: 0.200 bits per pixel, SSIM 0.8574, PSNR 34.89 dB (export)
- motion webm: 0.095 bits per pixel, SSIM 0.8257, PSNR 25.22 dB (export); uncapped reference: 0.358 bits per pixel (55.7 MB), SSIM 0.8436, PSNR 25.26 dB
