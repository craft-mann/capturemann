# Typical export size against the real size, on real recordings (0.3.1, EXP-5.AC10 and AC11)

- **Date:** 2026-11-09. **Status: PASS on the proposed pass line** (real within a factor 1.5 of the typical size on at least 80 % of held-out samples, never above the bound): **31 of 36 (86 %)**, every sample at or below the bound. Per format the held-out result is MP4 12 of 12, WebM 19 of 24 (79 %). Not "verified": one Mac, one user's captures, one hardware encoder; reproduced on this Mac only.
- **Machine:** Apple M4 Pro (macOS 26.6), hardware H.264 through VideoToolbox, GPU compositor, the FFmpeg that ships (`packaging/ffmpeg/out`, 9.0.2, LGPL build), debug builds of `cm-daemon` and `capturemann` from this branch, exports through the real daemon and CLI (`capturemann export <id> --format ... --size ... --wait`) on an isolated library of copies (`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET` set apart; the maintainer's library was never opened). The copies and the exports were deleted afterwards.
- **What was measured:** the maintainer agreed to the use of copies of their own captures. Only sizes, lengths, pixels and bit rates were recorded; no frame, title, transcript or other content appears here or in any log of this work. Recordings are R1 to R10.
- **Why:** the bound of `export-estimate-2026-11-09.md` is honest but loose on real screens: the maintainer's 8:00 recording (R1 here) showed "Up to 69 MB" for a 480p WebM and came out at 14.2 MB (237 kbps, 21 % of the bound). Decision (option B): a best guess shown as the main number ("About 14 MB"), the bound kept as the safety ("up to 69 MB"), from a **cheap content measure that needs no decoding**: the master screen track's bytes per second (file size over length), plus the chosen frame and codec. No probe, no trial encode.

## Method

- **Captures (metadata only).** 10 of the 43 captures with a screen track, chosen for spread of master bit rate, length and frame, by metadata alone. 8 are the recorder's own (HEVC masters, `quality: high`, 27 to 40 Mbps asked of the encoder; the recorder on this Mac records HEVC, not H.264 as the brief assumed), 2 are **imported** H.264 files (R1 and R2; no `quality` on the track). 5 carry a camera bubble, which the export composites. The other 33 screen captures were not used (camera clips of 2 to 42 s much like R9 and R10, all HEVC masters of 0.1 to 5.9 Mbps); the 22 audio-only captures have no screen track and get no typical size.
- **Slices, by the maintainer's instruction:** a 60 s kept range set in the copy's edit list (`edits[0]`, `version: 2`, `in`/`out` in session ticks) so the real exporter ran on exactly that minute; two slices per recording of 3 min or more (at 25 % and 65 %), one slice from the middle for 1 to 3 min, the whole clip under 1 min. 12 slices, 3 exports each (WebM 480p, WebM 1080p, MP4 at the recording size): 36 exports, 554 MB, 9 minutes of export time in all. Nothing was exported in full; R1's full 480p WebM (14.2 MB) is the maintainer's own export, kept as a reference.
- **Master rate in the slice:** `ffprobe -show_packets` over the slice's time range (packet sizes, no decoding), so the table separates the master's average over the whole file (what the product uses) from its rate in the exported minute.
- **Audio** per capture from the manifest (mono microphone: Opus 64 / AAC 128 kbps; stereo system audio: 96 / 192; none: 0) and taken out of the real size before fitting the video rule; the typical size in the product adds it back.

| R | master | frame @ fps | length | master MB | master kbps | audio | camera | slices |
|---|---|---|---|---|---|---|---|---|
| R1 | imported H.264 | 2218×1440 @ 30 | 8:00 | 42.3 | 705 | mic mono | no | a (2:00–3:00), b (5:12–6:12) |
| R2 | imported H.264 | 1920×1080 @ 16 | 42:29 | 199.0 | 624 | mic mono | no | a (10:37–11:37), b (27:37–28:37) |
| R3 | recorded HEVC high | 3024×1964 @ 30 | 0:35 | 77.6 | 17 912 | system stereo | no | all |
| R4 | recorded HEVC high | 3024×1964 @ 30 | 0:45 | 13.9 | 2 450 | system stereo | no | all |
| R5 | recorded HEVC high | 3024×1780 @ 30 | 0:17 | 6.2 | 2 865 | none | no | all |
| R6 | recorded HEVC high | 3840×2160 @ 30 | 1:03 | 18.3 | 2 326 | mic mono | yes | mid (60 s) |
| R7 | recorded HEVC high | 3024×1964 @ 30 | 1:09 | 33.1 | 3 848 | mic mono | yes | mid (60 s) |
| R8 | recorded HEVC high | 3024×1964 @ 30 | 1:01 | 19.2 | 2 523 | mic mono | yes | mid (60 s) |
| R9 | recorded HEVC high | 3840×2160 @ 30 | 0:14 | 9.9 | 5 891 | mic mono | yes | all |
| R10 | recorded HEVC high | 3024×1964 @ 30 | 0:24 | 4.9 | 1 648 | mic mono | yes | all |

## The rule (`cm_media::estimate`, one place)

```text
typical video bps = K[codec] × budget × master_bps × (export area / master area) ^ 0.9
                    clamped to [0.003 bit/pixel/frame, the codec's ceiling (the bound's rate)]
typical bytes     = (typical video bps + audio bps) × length × 1.02, nearest two figures, ≤ max_bytes
```

- `K` = **2.0 for hardware H.264**, **0.30 for VP9** (`TYPICAL_K_H264`, `TYPICAL_K_VP9`): at the master's own frame, the H.264 export spends about twice the recorder master's bytes (the exporter asks for 12 Mbps at 1080p and the master is a quiet HEVC file), the VP9 export at CRF 32 about a third.
- `budget` = 1 for the recorder's own master, **3.8 for an imported file** (`TYPICAL_IMPORT_FACTOR`; the screen track has no `quality`): the two imports spend far fewer bytes on the same picture than a master that is asked for 30 to 40 Mbps and lands at 2 to 18. Without this factor the rule fits 50 % within 1.5× (the imports land at 2 to 2.8× the guess, the recorded ones at 0.4 to 0.9×). **This constant rests on two files of one user and is the weakest part of the rule;** the alternative reading, that it is an H.264-against-HEVC difference rather than an import-against-recorder one, cannot be excluded with this data (both imports are H.264, all recorder masters HEVC). The recorder on Windows, Linux and Intel Macs writes H.264 masters at its own high bit-rate request, which is why the rule keys on the recorder's `quality` and not on the codec.
- Area exponent 0.9 (`TYPICAL_AREA_EXP`): the least-squares optimum in log space over 0.3 to 1.0; 0.8 and 1.0 give the same held-out counts (86 %). A smaller frame keeps a little more detail per pixel.
- The floor (0.003 bit per pixel per frame) never binds on these samples; it is under the synthetic static screen of the bound test (0.004 to 0.02) and only keeps a near-empty master from a zero guess. The ceiling binds once (R3, MP4: a video playback whose master is 17.9 Mbps).
- Fitted on the 36 exports with the **whole-file master rate** (the product's measure), closed form per codec (geometric mean of real / predicted), the import factor iterated with them; constants then rounded.

## Result

Real and typical in kbps of the exported file (audio included); "real / bound" is against the shown bound with its 5 % margin. The master rate in the slice is next to the whole-file rate the rule used.

| R | slice | master kbps whole / slice | format | size | pixels | ceiling kbps | real kbps | real / bound | typical kbps | real / typical |
|---|---|---|---|---|---|---|---|---|---|---|
| R1 | a | 705 / 632 | webm | 480p | 740×480 | 1028 | 226 | 0.20 | 179 | 1.26 |
| R1 | a | 705 / 632 | webm | 1080p | 1664×1080 | 5200 | 519 | 0.09 | 554 | 0.94 |
| R1 | a | 705 / 632 | mp4 | recording | 2218×1440 | 18483 | 3950 | 0.20 | 5596 | 0.71 |
| R1 | b | 705 / 1093 | webm | 480p | 740×480 | 1028 | 305 | 0.27 | 179 | 1.70 |
| R1 | b | 705 / 1093 | webm | 1080p | 1664×1080 | 5200 | 671 | 0.12 | 554 | 1.21 |
| R1 | b | 705 / 1093 | mp4 | recording | 2218×1440 | 18483 | 5456 | 0.28 | 5596 | 0.98 |
| R2 | a | 624 / 784 | webm | 480p | 854×480 | 1186 | 247 | 0.19 | 234 | 1.05 |
| R2 | a | 624 / 784 | webm | 1080p | 1920×1080 | 6000 | 704 | 0.11 | 791 | 0.89 |
| R2 | a | 624 / 784 | mp4 | recording | 1920×1080 | 12000 | 4022 | 0.32 | 4968 | 0.81 |
| R2 | b | 624 / 1418 | webm | 480p | 854×480 | 1186 | 217 | 0.17 | 234 | 0.93 |
| R2 | b | 624 / 1418 | webm | 1080p | 1920×1080 | 6000 | 652 | 0.10 | 791 | 0.82 |
| R2 | b | 624 / 1418 | mp4 | recording | 1920×1080 | 12000 | 4802 | 0.38 | 4968 | 0.97 |
| R3 | all | 17912 / 18074 | webm | 480p | 740×480 | 1028 | 378 | 0.32 | 532 | 0.71 |
| R3 | all | 17912 / 18074 | webm | 1080p | 1662×1080 | 5194 | 855 | 0.15 | 1965 | 0.43 |
| R3 | all | 17912 / 18074 | mp4 | recording | 3024×1964 | 34370 | 25782 | 0.71 | 35253 | 0.73 |
| R4 | all | 2450 / 2454 | webm | 480p | 740×480 | 1028 | 139 | 0.12 | 157 | 0.88 |
| R4 | all | 2450 / 2454 | webm | 1080p | 1662×1080 | 5194 | 441 | 0.08 | 353 | 1.25 |
| R4 | all | 2450 / 2454 | mp4 | recording | 3024×1964 | 34370 | 5086 | 0.14 | 5194 | 0.98 |
| R5 | all | 2865 / 2905 | webm | 480p | 816×480 | 1133 | 191 | 0.16 | 83 | 2.30 |
| R5 | all | 2865 / 2905 | webm | 1080p | 1834×1080 | 5731 | 585 | 0.10 | 357 | 1.64 |
| R5 | all | 2865 / 2905 | mp4 | recording | 3024×1780 | 31150 | 7389 | 0.23 | 5845 | 1.26 |
| R6 | mid | 2326 / 2360 | webm | 480p | 854×480 | 1186 | 98 | 0.07 | 113 | 0.87 |
| R6 | mid | 2326 / 2360 | webm | 1080p | 1920×1080 | 6000 | 375 | 0.06 | 270 | 1.39 |
| R6 | mid | 2326 / 2360 | mp4 | recording | 3840×2160 | 48000 | 5764 | 0.11 | 4876 | 1.18 |
| R7 | mid | 3848 / 3899 | webm | 480p | 740×480 | 1028 | 170 | 0.15 | 159 | 1.07 |
| R7 | mid | 3848 / 3899 | webm | 1080p | 1662×1080 | 5194 | 522 | 0.09 | 466 | 1.12 |
| R7 | mid | 3848 / 3899 | mp4 | recording | 3024×1964 | 34370 | 11327 | 0.31 | 7980 | 1.42 |
| R8 | mid | 2523 / 2542 | webm | 480p | 740×480 | 1028 | 107 | 0.09 | 126 | 0.85 |
| R8 | mid | 2523 / 2542 | webm | 1080p | 1662×1080 | 5194 | 394 | 0.07 | 328 | 1.20 |
| R8 | mid | 2523 / 2542 | mp4 | recording | 3024×1964 | 34370 | 6795 | 0.19 | 5277 | 1.29 |
| R9 | all | 5891 / 5974 | webm | 480p | 854×480 | 1186 | 160 | 0.12 | 186 | 0.86 |
| R9 | all | 5891 / 5974 | webm | 1080p | 1920×1080 | 6000 | 588 | 0.09 | 583 | 1.01 |
| R9 | all | 5891 / 5974 | mp4 | recording | 3840×2160 | 48000 | 10522 | 0.21 | 12148 | 0.87 |
| R10 | all | 1648 / 1654 | webm | 480p | 740×480 | 1028 | 92 | 0.08 | 105 | 0.88 |
| R10 | all | 1648 / 1654 | webm | 1080p | 1662×1080 | 5194 | 335 | 0.06 | 237 | 1.41 |
| R10 | all | 1648 / 1654 | mp4 | recording | 3024×1964 | 34370 | 4538 | 0.13 | 3492 | 1.30 |

- **Every sample is at or below the bound** (AC6's pass line still holds on real recordings: 0.06 to 0.71 of it). Real screens of this user land at **6 to 38 % of the bound**, the video playback (R3) at 15 to 71 %: the bound alone is 3 to 16 times the real size for them, which is what the typical size fixes.
- **In-sample** (the rounded constants, whole-file master rate): median real / typical **1.01**, 32 of 36 within 1.5×, 34 within 2×, worst 2.30 (R5, 480p WebM) and 0.43 (R3, 1080p WebM). With the slice's own master rate instead: 28 of 36 within 1.5× (the whole-file average predicts better than the exported minute's own rate, because the encoder's output follows the recording's general activity more than the minute's).

### Held-out errors (leave one recording out)

Each recording predicted with constants fitted on the other nine (the exponent 0.9 fixed); ratio real / typical of its rows (480p WebM, 1080p WebM, MP4; slice a then b):

| R | constants without it (K H.264, K VP9, import) | real / typical |
|---|---|---|
| R1 | 2.16, 0.30, 3.34 | 1.37, 1.05, 0.74; **1.85**, 1.36, 1.03 |
| R2 | 2.03, 0.31, 4.27 | 0.95, 0.78, 0.71; 0.83, 0.72, 0.85 |
| R3 | 2.16, 0.34, 3.47 | **0.64**, **0.39**, 0.73 |
| R4 | 2.04, 0.31, 3.74 | 0.87, 1.21, 0.96 |
| R5 | 1.94, 0.29, 4.05 | **2.42**, **1.73**, 1.31 |
| R6 | 1.99, 0.31, 3.79 | 0.85, 1.36, 1.19 |
| R7 | 1.95, 0.30, 3.87 | 1.06, 1.11, 1.46 |
| R8 | 1.98, 0.31, 3.78 | 0.83, 1.16, 1.30 |
| R9 | 2.07, 0.32, 3.69 | 0.83, 0.96, 0.84 |
| R10 | 1.97, 0.31, 3.82 | 0.87, 1.38, 1.32 |

- **Median 1.03, 31 of 36 (86 %) within 1.5×, 34 of 36 (94 %) within 2×; worst 2.42× and 0.39×.** The constants barely move when a recording is left out (K H.264 1.94 to 2.16, K VP9 0.29 to 0.34), except the import factor (3.3 to 4.3), which depends on the two imports.
- The five misses: R5 (a recording with no audio track whose WebM is 1.7 to 2.4× the guess at both sizes; its MP4 is fine), R3 (a video playback: VP9 at CRF 32 compresses it far better than the rule expects from its 17.9 Mbps master, 0.39 to 0.64×), and R1's busier slice at 480p (1.85×; its quieter slice 1.37×).
- **Without the import factor** the same procedure gives 50 % within 1.5× and 83 % within 2×: the rule needs some account of how the master's bytes were spent.

### The reference (R1 in full) and the slice spread

- The maintainer's full export of R1 (480p WebM, 8:00) was **14.2 MB**; the rule from the whole-file master says **10.7 MB, "About 11 MB"** (1.33× under), against "Up to 69 MB" before. Its two one-minute slices came out at 226 and 305 kbps (the full file averages 237), so the slices bracket the real average.
- **A one-minute slice can mislead by up to 1.4×:** R1's two slices differ by 0.72 to 0.77× (MP4, 1080p and 480p WebM), R2's by 0.84 to 1.14×; the master's own rate in the slice differs from the whole-file average by 0.9 to 2.3× (R2's second minute is 1418 kbps against 624 average, yet its exports are not larger). The fit used the whole-file rate and each slice as its own sample.

## Limits (what this does and does not say)

- **One Mac, one user, one encoder.** Apple M4 Pro, VideoToolbox H.264, `libvpx-vp9` of the shipped FFmpeg. Not calibrated: Windows (Media Foundation, NVENC, QSV, AMF), Linux (VA-API, VP9 in MP4), Intel Macs, so the rule there is a guess carried over; the bound is still what holds.
- **Recorder masters at one setting.** All eight are HEVC at `quality: high`; a recorder master in H.264 (Intel Macs, Windows, Linux) or at a lower quality setting spends its bytes differently and is not calibrated (the manifest's `bitrate_bps` could normalise this later; no data to fit it).
- **Two imports**, both H.264 at 0.6 to 0.7 Mbps; other imports (ProRes, phone HEVC at 50 Mbps, a 10 Mbps screen recording from another tool) land somewhere between the two cases, and the ceiling clamp catches the large ones.
- Not measured: 60 fps, exports of 1440p and 4K frames (the masters were 4K, the exports at most the recording size of 3024×1964 or 3840×2160 in MP4), 720p and 480p MP4, a camera bubble at other layouts (the five camera recordings fit the rule without a camera term, 0.83 to 1.46×), clips with speed changes or cuts beyond one kept range, the first and last seconds of a recording.
- The pass line (80 % within 1.5×) is met at 86 % over all samples and at 79 % for WebM alone; a stricter line (1.3×) would fail (69 %).

## How to run it again

The exports were driven by scripts outside the repository (`cal-*.sh`, `cal-*.py` in the agent's scratchpad: copy with `cp -c -R`, set the slice, `capturemann export --wait`, `ffprobe -show_packets` for the slice rate, a least-squares fit in log space with leave-one-recording-out); they held no content and were not kept. The stored tests are the unit test with scripted master sizes (`cm_media::estimate`, `the_typical_size_follows_the_master_rate_and_never_exceeds_the_bound`), the daemon, CLI and MCP tests with a 705 kbps master file, and `crates/cm-daemon/tests/estimate_vs_real.rs`, which keeps the bound's pass line on the synthetic samples and now prints the typical size as information (not run again for this report: the bound did not change).
