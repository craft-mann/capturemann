# Release comparison, v0.2.6 vs 0.2.7 (PARTIAL: first rows, 2026-11-04)

Status: partial. Rows done: PAT 5-min, hop-84s, two short clips. Queued in this order: FLEURS 4-min, English far-field (AMI ES2004b, TS3003a, IS1009b, PriMock day1, day3), syn4, noise set, Nov 2 (45 min), 1 h (0.2.7 only), Data Platform 88 min (0.2.7 only). Counts only for private audio. "Measured on this Mac" (M4 Pro class, Metal), one run per cell (decodes repeat identically).

## Setup (reproducible by command)
- 0.2.6: worktree of tag v0.2.6, `cargo build --release -p cm-ai --example bench026` (the example file is not in the tag: it calls `transcribe_pieces` and writes the transcript). 0.2.7: this branch, `asr_bench --lift` (the daemon's per-call `Configured` wrapper with the lift) with `--transcript-out`.
- Harness: `python3 tests/tools/compare_releases.py run --out DIR --rows ROWS.json` then `table`; scoring by `rescore` (folded and exact WER with tone marks kept; loops, dense repeats, bare-token runs, sounds). Each decode through `/tmp/cm-gpu`.
- Models: large-v3-turbo and large-v3 from the app's models folder; language vi / en per row; no glossary; full file decoded as the app does (pieces of about 5 minutes), scored by word time.
- Cross-check: bench026 on the full PAT file equals the coordinator's v0.2.6 daemon run (Large v3 70/33/25 exact, Turbo 111/100/22).

## Rows (WER % folded / exact, with sub/del/ins of the exact score)

| Row | 0.2.6 Turbo | 0.2.7 Turbo | 0.2.7 Large v3 |
|---|---|---|---|
| PAT 5-min (private, 1,000 ref words, seeded from a Large v3 transcript) | 22.7 / 23.3 (111/100/22) | 22.7 / 23.3 (111/100/22) | 12.3 / 12.8 (70/33/25) |
| hop-84s (private, 298 words, window 89 to 173 s of a 330 s clip; the reference is an older Large v3 output the maintainer called acceptable) | 96.3 / 97.3 (32/258/0) | 28.9 / 29.2 (47/31/9) | 0.7 / 0.7 (0/1/1) |
| short vi 15 s (made-up row s_vi1) | 20.0 / 24.0 | 20.0 / 24.0 | 20.0 / 24.0 |
| short en 20 s | 4.3 / 4.3 | 4.3 / 4.3 | 4.3 / 4.3 |

Loops, dense repeats, bare-token runs and sounds are 0 in every row above. Speed (audio seconds per wall second, measured while other agents shared the GPU, so only a rough guide): Turbo 5 to 12, Large v3 2 to 7. Peak memory: Turbo about 2.0 to 2.2 GB, Large v3 about 3.7 to 3.8 GB.

## What the first rows show
- PAT: 0.2.7 Turbo equals 0.2.6 Turbo word for word in count (the lift regression is fixed, see the regression report); Large v3 equals the 0.2.6 level.
- hop-84s: 0.2.6 Turbo leaves most of the window empty (40 of about 300 words). Over the whole 330 s clip it writes 46, 61, 15, 15, 15, 28 words per 30 s for the first three minutes where 0.2.7 writes 46, 62, 80, 108, 107, 72: Turbo skipped about 100 s of speech in 0.2.6 and 0.2.7 recovers it (the re-read of speech that got no words, and the gap fill). 0.2.7 Turbo then scores 29 % on that window. This is the largest user-visible gain in this comparison, on one private stretch.
- hop-84s with Large v3 is 0.7 %: that number means nothing about accuracy, the reference is a stored Large v3 transcript. Do not use it to rank Large v3.
- The two short clips are identical across releases and models in the scored counts.

## Not shown yet / limits
Public rows are still running. Speaker count was not rerun (only old stored diarization.json exists for the private bundles). No insertion-over-silence number for private audio (no marked silent spans). One recording per language for the Vietnamese rows. The PAT and hop references both come from machine transcripts, so differences of a few points between Turbo and Large v3 are biased toward Large v3.
