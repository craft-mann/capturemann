# Release comparison, v0.2.6 vs 0.2.7 (2026-11-04)

Status: complete for the rows below (the partial first version of this report is replaced). Counts only for private audio. Measured on this Mac, Metal, one run per cell (decodes repeat identically); speeds were taken while other agents shared the GPU, so they are rough.

## Setup (reproducible by command)
- 0.2.6: worktree of tag v0.2.6, `cargo build --release -p cm-ai --example bench026` (the example file is not in the tag: it calls `transcribe_pieces` and writes the transcript). 0.2.7: this branch, `asr_bench --lift` (the daemon's per-call `Configured` wrapper with the lift) with `--transcript-out`.
- Harness: `python3 tests/tools/compare_releases.py run --out DIR --rows ROWS.json` then `table`; scoring by `rescore` (folded and exact WER with tone marks kept; loops, dense repeats, bare-token runs, sounds). Each decode through `/tmp/cm-gpu`.
- Models: large-v3-turbo and large-v3 from the app's models folder; language vi / en per row; no glossary; full file decoded as the app does (pieces of about 5 minutes), scored by word time.
- Cross-check: bench026 on the full PAT file equals the coordinator's v0.2.6 daemon run (Large v3 70/33/25 exact, Turbo 111/100/22).

## Results (exact WER % with tone marks kept, sub/del/ins in brackets; folded WER after the semicolon)

Private rows: pat_5min (first 300 s of the 23-minute PAT file, 1,000 reference words), hop_84s (89 to 173 s of a 330 s clip of the Data Platform meeting, 298-word reference that is an older Large v3 output: Large v3 numbers on it mean nothing), nov2_45min and data_platform_88min (no reference: counters only).  rows have no usable WER (the clips are noise with a few speech spans): their metric is words written outside the speech spans. viu_1h and data_platform_88min were run for 0.2.7 only.

| Row | metric | 0.2.6 Turbo | 0.2.7 Turbo | 0.2.7 Large v3 |
|---|---|---|---|---|
| pat_5min | exact WER % (sub/del/ins); folded | 23.3 (111/100/22); 22.7 | 23.3 (111/100/22); 22.7 | 12.8 (70/33/25); 12.3 |
| pat_5min | counters | 922 w, loops 0, dense 0, bare 0, sounds 0 | 922 w, loops 0, dense 0, bare 0, sounds 0 | 992 w, loops 0, dense 0, bare 0, sounds 0 |
| pat_5min | speed, peak memory | 9x, 2.1 GB | 8x, 2.2 GB | 4x, 3.8 GB |
| hop_84s | exact WER % (sub/del/ins); folded | 97.3 (32/258/0); 96.3 | 29.2 (47/31/9); 28.9 | 0.7 (0/1/1); 0.7 |
| hop_84s | counters | 40 w, loops 0, dense 0, bare 0, sounds 0 | 276 w, loops 0, dense 0, bare 0, sounds 0 | 298 w, loops 0, dense 0, bare 0, sounds 0 |
| hop_84s | speed, peak memory | 6x, 2.0 GB | 5x, 2.1 GB | 2x, 3.8 GB |
| short_s_vi1 | exact WER % (sub/del/ins); folded | 24.0 (4/2/0); 20.0 | 24.0 (4/2/0); 20.0 | 24.0 (4/2/0); 20.0 |
| short_s_vi1 | counters | 23 w, loops 0, dense 0, bare 0, sounds 0 | 23 w, loops 0, dense 0, bare 0, sounds 0 | 23 w, loops 0, dense 0, bare 0, sounds 0 |
| short_s_vi1 | speed, peak memory | 7x, 1.9 GB | 12x, 1.9 GB | 7x, 3.7 GB |
| short_s_en2 | exact WER % (sub/del/ins); folded | 4.3 (2/0/0); 4.3 | 4.3 (2/0/0); 4.3 | 4.3 (2/0/0); 4.3 |
| short_s_en2 | counters | 47 w, loops 0, dense 0, bare 0, sounds 0 | 47 w, loops 0, dense 0, bare 0, sounds 0 | 47 w, loops 0, dense 0, bare 0, sounds 0 |
| short_s_en2 | speed, peak memory | 26x, 1.9 GB | 11x, 1.9 GB | 5x, 3.7 GB |
| fleurs_viu_a | exact WER % (sub/del/ins); folded | 11.8 (46/5/10); 10.3 | 11.8 (46/5/10); 10.3 | 10.8 (35/7/14); 10.1 |
| fleurs_viu_a | counters | 522 w, loops 0, dense 0, bare 0, sounds 0 | 522 w, loops 0, dense 0, bare 0, sounds 0 | 524 w, loops 0, dense 0, bare 0, sounds 0 |
| fleurs_viu_a | speed, peak memory | 11x, 2.0 GB | 12x, 2.0 GB | 5x, 3.8 GB |
| fleurs_viu_b | exact WER % (sub/del/ins); folded | 9.1 (46/3/3); 7.4 | 9.1 (46/3/3); 7.4 | 12.6 (56/12/4); 11.2 |
| fleurs_viu_b | counters | 571 w, loops 0, dense 0, bare 0, sounds 0 | 571 w, loops 0, dense 0, bare 0, sounds 0 | 563 w, loops 0, dense 0, bare 0, sounds 0 |
| fleurs_viu_b | speed, peak memory | 14x, 2.0 GB | 14x, 2.0 GB | 2x, 3.8 GB |
| fleurs_viu_c | exact WER % (sub/del/ins); folded | 14.8 (65/7/13); 13.9 | 14.8 (65/7/13); 13.9 | 17.4 (54/6/40); 16.8 |
| fleurs_viu_c | counters | 582 w, loops 0, dense 0, bare 0, sounds 0 | 582 w, loops 0, dense 0, bare 0, sounds 0 | 610 w, loops 0, dense 0, bare 0, sounds 0 |
| fleurs_viu_c | speed, peak memory | 12x, 2.0 GB | 14x, 2.0 GB | 1x, 3.8 GB |
| ami_ES2004b | exact WER % (sub/del/ins); folded | 24.7 (55/186/20); 24.7 | 24.7 (55/186/20); 24.7 | 27.0 (74/181/30); 27.0 |
| ami_ES2004b | counters | 890 w, loops 0, dense 0, bare 0, sounds 0 | 890 w, loops 0, dense 0, bare 0, sounds 0 | 905 w, loops 0, dense 0, bare 0, sounds 0 |
| ami_ES2004b | speed, peak memory | 15x, 2.0 GB | 16x, 2.0 GB | 5x, 3.7 GB |
| ami_TS3003a | exact WER % (sub/del/ins); folded | 41.2 (42/31/14); 41.2 | 41.7 (41/31/16); 41.7 | 37.9 (34/31/15); 37.9 |
| ami_TS3003a | counters | 194 w, loops 2, dense 12, bare 0, sounds 0 | 196 w, loops 1, dense 10, bare 0, sounds 2 | 198 w, loops 1, dense 0, bare 0, sounds 0 |
| ami_TS3003a | speed, peak memory | 16x, 2.0 GB | 10x, 2.0 GB | 5x, 3.7 GB |
| ami_IS1009b | exact WER % (sub/del/ins); folded | 31.0 (51/239/15); 31.0 | 30.2 (66/214/17); 30.2 | 29.6 (36/242/13); 29.6 |
| ami_IS1009b | counters | 759 w, loops 0, dense 0, bare 0, sounds 0 | 786 w, loops 0, dense 0, bare 0, sounds 0 | 754 w, loops 0, dense 5, bare 0, sounds 0 |
| ami_IS1009b | speed, peak memory | 16x, 2.0 GB | 15x, 2.0 GB | 3x, 3.8 GB |
| primock_day1_consultation01 | exact WER % (sub/del/ins); folded | 21.6 (58/120/19); 21.6 | 21.6 (58/120/19); 21.6 | 25.5 (74/124/34); 25.5 |
| primock_day1_consultation01 | counters | 810 w, loops 0, dense 0, bare 0, sounds 0 | 810 w, loops 0, dense 0, bare 0, sounds 0 | 821 w, loops 0, dense 0, bare 0, sounds 0 |
| primock_day1_consultation01 | speed, peak memory | 20x, 2.0 GB | 18x, 2.0 GB | 6x, 3.8 GB |
| primock_day3_consultation01 | exact WER % (sub/del/ins); folded | 20.3 (46/54/40); 20.3 | 17.2 (40/61/18); 17.2 | 17.0 (46/54/17); 17.0 |
| primock_day3_consultation01 | counters | 676 w, loops 1, dense 25, bare 0, sounds 0 | 647 w, loops 0, dense 7, bare 0, sounds 0 | 653 w, loops 0, dense 5, bare 0, sounds 0 |
| primock_day3_consultation01 | speed, peak memory | 17x, 2.0 GB | 10x, 2.0 GB | 7x, 3.8 GB |
| syn4 | exact WER % (sub/del/ins); folded | 6.2 (46/10/0); 6.2 | 6.2 (46/10/0); 6.2 | 12.7 (58/11/45); 12.7 |
| syn4 | counters | 890 w, loops 0, dense 0, bare 0, sounds 0 | 890 w, loops 0, dense 0, bare 0, sounds 0 | 934 w, loops 0, dense 0, bare 0, sounds 0 |
| syn4 | speed, peak memory | 23x, 2.0 GB | 22x, 2.0 GB | 3x, 3.8 GB |
| noise_00_pink_40 | words outside speech; coverage | cov 1.000, outside 8 | cov 1.000, outside 6 | cov 1.000, outside 1 |
| noise_00_pink_40 | counters | 27 w, loops 0, dense 0, bare 0, sounds 0 | 29 w, loops 0, dense 0, bare 0, sounds 0 | 19 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_00_pink_40 | speed, peak memory | 10x, 1.9 GB | 4x, 1.9 GB | 8x, 3.7 GB |
| noise_01_pink_45 | words outside speech; coverage | cov 0.964, outside 7 | cov 0.964, outside 0 | cov 0.928, outside 15 |
| noise_01_pink_45 | counters | 26 w, loops 0, dense 0, bare 0, sounds 0 | 16 w, loops 0, dense 0, bare 0, sounds 0 | 32 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_01_pink_45 | speed, peak memory | 19x, 1.9 GB | 35x, 1.9 GB | 10x, 3.7 GB |
| noise_02_pink_50 | words outside speech; coverage | cov 1.000, outside 12 | cov 1.000, outside 12 | cov 1.000, outside 12 |
| noise_02_pink_50 | counters | 30 w, loops 0, dense 0, bare 0, sounds 0 | 30 w, loops 0, dense 0, bare 0, sounds 0 | 25 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_02_pink_50 | speed, peak memory | 20x, 1.9 GB | 14x, 1.9 GB | 10x, 3.7 GB |
| noise_03_brown_40 | words outside speech; coverage | cov 1.000, outside 0 | cov 1.000, outside 0 | cov 1.000, outside 14 |
| noise_03_brown_40 | counters | 18 w, loops 0, dense 0, bare 0, sounds 0 | 18 w, loops 0, dense 0, bare 0, sounds 2 | 28 w, loops 0, dense 0, bare 0, sounds 2 |
| noise_03_brown_40 | speed, peak memory | 24x, 1.9 GB | 18x, 1.9 GB | 5x, 3.7 GB |
| noise_04_brown_45 | words outside speech; coverage | cov 0.919, outside 0 | cov 0.919, outside 0 | cov 0.938, outside 12 |
| noise_04_brown_45 | counters | 26 w, loops 0, dense 0, bare 0, sounds 0 | 26 w, loops 0, dense 0, bare 0, sounds 0 | 33 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_04_brown_45 | speed, peak memory | 35x, 1.9 GB | 20x, 1.9 GB | 9x, 3.7 GB |
| noise_05_brown_50 | words outside speech; coverage | cov 0.973, outside 6 | cov 0.957, outside 0 | cov 1.000, outside 14 |
| noise_05_brown_50 | counters | 31 w, loops 0, dense 0, bare 0, sounds 0 | 25 w, loops 0, dense 0, bare 0, sounds 0 | 40 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_05_brown_50 | speed, peak memory | 14x, 1.9 GB | 32x, 1.9 GB | 7x, 3.7 GB |
| noise_06_pink_40 | words outside speech; coverage | cov 0.976, outside 0 | cov 0.976, outside 0 | cov 0.957, outside 16 |
| noise_06_pink_40 | counters | 18 w, loops 0, dense 0, bare 0, sounds 0 | 18 w, loops 0, dense 0, bare 0, sounds 0 | 33 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_06_pink_40 | speed, peak memory | 12x, 1.9 GB | 9x, 1.9 GB | 8x, 3.7 GB |
| noise_07_pink_45 | words outside speech; coverage | cov 0.989, outside 7 | cov 0.923, outside 17 | cov 0.939, outside 0 |
| noise_07_pink_45 | counters | 14 w, loops 0, dense 0, bare 0, sounds 0 | 39 w, loops 0, dense 0, bare 0, sounds 0 | 26 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_07_pink_45 | speed, peak memory | 6x, 1.9 GB | 8x, 1.9 GB | 12x, 3.7 GB |
| noise_08_pink_50 | words outside speech; coverage | cov 0.906, outside 0 | cov 0.906, outside 0 | cov 0.987, outside 2 |
| noise_08_pink_50 | counters | 32 w, loops 0, dense 0, bare 0, sounds 0 | 32 w, loops 0, dense 0, bare 0, sounds 0 | 23 w, loops 0, dense 0, bare 0, sounds 0 |
| noise_08_pink_50 | speed, peak memory | 30x, 1.9 GB | 30x, 1.9 GB | 11x, 3.7 GB |
| nov2_45min | counters | 8378 w, loops 3, dense 35, bare 0, sounds 0 | 8695 w, loops 0, dense 0, bare 0, sounds 0 | 9138 w, loops 0, dense 0, bare 0, sounds 0 |
| nov2_45min | speed, peak memory | 11x, 2.3 GB | 9x, 2.3 GB | 3x, 4.0 GB |
| viu_1h | exact WER % (sub/del/ins); folded | - | 11.7 (646/146/136); 10.7 | 17.4 (712/136/539); 16.4 |
| viu_1h | counters | - | 7951 w, loops 0, dense 0, bare 0, sounds 0 | 8366 w, loops 0, dense 0, bare 0, sounds 1 |
| viu_1h | speed, peak memory | - | 14x, 2.6 GB | 2x, 4.2 GB |
| data_platform_88min | counters | - | 18765 w, loops 0, dense 0, bare 1, sounds 1 | 18828 w, loops 0, dense 0, bare 0, sounds 0 |
| data_platform_88min | speed, peak memory | - | 7x, 2.5 GB | 2x, 4.0 GB |


## What the table shows
- Same text on 0.2.6 and 0.2.7 Turbo on the clean and short rows (FLEURS, syn4, PriMock day1, AMI ES2004b, the shorts): the decode is unchanged there. No public row is worse on 0.2.7 Turbo except AMI TS3003a by 0.5 point (41.2 to 41.7: one word).
- 0.2.7 Turbo is better where the 0.2.6 pipeline failed: PriMock day3 20.3 to 17.2 (dense repeats 25 to 7, loop 1 to 0), AMI IS1009b 31.0 to 30.2 (25 more words), hop-84s 97.3 to 29.2 (speech the 0.2.6 pass skipped, recovered by the re-read of speech that got no words), Nov 2 loops 3 to 0 and dense repeat words 35 to 0.
- Large v3 is better only on the private Vietnamese far-field row (PAT 12.8 against 23.3) and AMI TS3003a / IS1009b / day3 slightly; it is worse than Turbo on FLEURS b and c, syn4, PriMock day1, AMI ES2004b and the 1 h FLEURS file (17.4 against 11.7), and 3 to 5 times slower with 1.9 times the memory.
- Noise set, words outside speech (9 clips): 0.2.6 Turbo 40, 0.2.7 Turbo 35, 0.2.7 Large v3 86. The lift change moves single clips both ways (noise_01 7 to 0, noise_07 7 to 17).
- Sounds: 0.2.7 writes a sound ([noise]/[unclear]) where it would otherwise invent words: 2 on AMI TS3003a, 2 on noise_03, 1 each on Nov 2 and the Data Platform meeting.

## Careful re-reads (, beam 5, no temperature fallback on windows the pipeline reads again): partial measurement, setting stays OFF
Measured with the examples  and  with , Turbo, the app's own path ( over the lift), .

| Check | Baseline | Careful re-reads |
|---|---|---|
| Public silence set (mac-synth set2, 36 Vietnamese clips, 1,619 s of reference silence): units over silence (per hour) | 78 (173) | 78 (173) |
| Same set, words over silence | 896 | 979 |
| hop-84s window exact WER (sub/del/ins), 15 decode calls both | 29.2 (47/31/9) | 30.5 (52/24/15) |
| PAT 5-min clip exact WER | 23.7 | 23.7 (no re-read happened, 1 call) |
| FLEURS viu_a / viu_b / viu_c exact | 11.8 / 9.1 / 14.8 | identical (no re-reads) |
| syn4 | 6.2 | identical |
| Decode time | hop clip 31 s | hop clip 55 s (about 1.8 times; the clips without re-reads show the machine noise: 12 against 30 s on PAT under contention) |

Result: on the Vietnamese silence set the careful re-reads remove none of the invented units and add 83 words, and the hop window gets 1.3 points worse. The beam search that cleaned the noise set in the parameter search does not change the units that are made by the gap re-reads here. The setting stays off (the code, tests and flag are in place). Not measured: the silence set for Turbo English and Large v3 Vietnamese, the raw-track clips 103 and 14 through the real daemon, English far-field WER with the flag, decode time per hour of audio on an idle machine.

## Real daemon check of the raw-microphone choice (Nov 2, Turbo, language vi, both tracks stored)
 was on in this check; main now defaults it OFF (maintainer decision, noise-reduced copy stays the default). Raw read: 8,254 words, 0 loop runs, 1 sound, 689 caption lines. Noise-reduced copy: 7,726 words, 1 loop run, 1 sound, 651 caption lines (6.8 % fewer words).

## Left for 0.2.8
- Decide the raw-microphone default with a second consented recording that has both tracks and a reference; measure Large v3 on the denoised/raw pair.
- Careful re-reads: silence set for Turbo English and Large v3 Vietnamese; clips 103 and 14 through the daemon; timing on an idle machine; a narrower trigger (re-read only windows the gate marks) instead of all re-reads.
- Remaining word-time stacks (no room between neighbours): 32 to 111 words per meeting; needs a rule that may shift the neighbour.
- Speaker counts for the three private meetings (only old stored diarization exists); insertion-over-silence on private audio (needs the marked silent spans).
- Full-file route for the hop-84s row and a Large v3 row with an independent reference.
- The lift: only really quiet audio is lifted (peak 12 dB under full scale); the possible Turbo gain from a lift on PAT (about 3 points on one file) is given up; a per-region lift (W6) is the way to get it back.
