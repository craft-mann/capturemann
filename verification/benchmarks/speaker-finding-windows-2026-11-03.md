# Speaker finding: windows, no second run, window shift (0.2.7, M2b)

- **Date:** 2026-11-03. **Status:** measured on synthetic voices only, one run per cell, Apple M4 Pro, the shipped models. Not "verified". Timings of some cells were taken while other work ran on the machine and are marked.
- **Build:** branch `track/mac-ai`, `perf speakers` (time, peak memory, turns) and `measure der` (DER, 0.25 s collar).
- **Question:** (1) can the second engine run for "more speakers than the maximum" be replaced by folding, (2) do windowed runs for long audio keep the answer, cap memory and make Cancel work, (3) is another `window_shift_ratio` or link distance better?

## 1. Short answer

- **No second run for a maximum.** Speakers beyond the maximum (the ones with least speech) are given to the nearest kept speaker (`fold_to_n`). Same DER as running all the audio again with the exact maximum (`syn6`, 6 voices: 62.1 % against 62.2 % at a maximum of 2, 49.1 % against 49.1 % at 3), in 38 s against 79 s and 54 s against 91 s (the second run costs a whole run again). The 0.2.6 behaviour stays selectable (`Tuning::second_run_for_max`) for such measurements.
- **Windows.** Audio over 20 minutes is cut at quiet spots into windows of about 10 minutes; every window is run on its own and the speakers are linked by voice prints (mean of the prints of a speaker's six longest turns, agglomerative linking by cosine distance 0.5, two speakers of one window never linked; a known count merges down to that count). On a **1 hour, 6 voice synthetic meeting** the 10 minute windows gave the **same turns and DER as the whole run (34.1 %, 5 of 6 found)** in 184 s against 268 s; peak memory 594 MB against 599 MB (**no memory gain measured at one hour on this clip**; the earlier report's 1.2 GB at one hour came from a real recording). Windows of 5 minutes were worse (48.2 %, 4 found), and the link distance (0.35, 0.5, 0.65) made no difference on this clip. **Cancel** is noticed between windows (unit test) instead of at the end of a whole run.
- **Window shift 0.2** (shipped 0.1): DER equal on 3 of 4 clips, better on `syn6` (31.4 % against 36.7 %) and 1.4x to 3x faster (timings noisy), but 0.3 broke `syn4` (1 speaker, 65 %). **Not changed:** the evidence is synthetic voices, which the 2026-10-29 report showed behave differently from real conversations.

## 2. Setup and how to repeat

```sh
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn60 --speakers 6 --minutes 60 --seed 9 --max-gap 2.5     # also syn3/syn4/synp3/syn6 as in the other reports
cargo build --release -p cm-ai --examples
target/release/examples/perf speakers <dir>/syn60.wav --sherpa <models> [--window-secs 600] [--link 0.5] [--shift 0.2] [--max-people 2] [--second-run] --turns-out t.txt
target/release/examples/measure der <dir>/syn60.rttm.txt t.txt
```

## 3. Results

Maximum of speakers (automatic run, more voices than the maximum):

| Clip | Maximum | Fold (new) | Second run (0.2.6) |
|---|---|---|---|
| syn6 (6 voices, 844 s) | 2 | DER 62.1 %, 38.1 s | DER 62.2 %, 79.1 s |
| syn6 | 3 | DER 49.1 %, 53.5 s | DER 49.1 %, 90.6 s |
| syn4 (4 voices, 312 s) | 3 | 44.0 %, 17.3 s (no fold needed) | 44.0 %, 15.8 s |
| synp3 (3 voices, 305 s) | 2 | 16.7 %, 13.8 s | 16.7 %, 13.7 s |

1 hour (`syn60`, 3,608 s, 6 voices):

| Setting | Wall s | Found | Peak MB | DER |
|---|---|---|---|---|
| whole run (0.2.6) | 267.8 | 5 | 599 | 34.1 % |
| windows of 600 s, link 0.5 | 183.8 | 5 | 594 | 34.1 % |
| windows of 300 s | 185.7 | 4 | 568 | 48.2 % |
| windows of 600 s, link 0.35 | 438.2 (machine busy) | 5 | 545 | 34.1 % |
| windows of 600 s, link 0.65 | 443.3 (machine busy) | 5 | 622 | 34.1 % |

Window shift (the shipped value is 0.1):

| Clip | 0.1 | 0.2 | 0.3 |
|---|---|---|---|
| syn3 | 26.7 %, 28.6 s | 26.7 %, 15.7 s | 26.7 %, 42.0 s (busy) |
| syn4 | 44.0 %, 22.7 s | 44.0 %, 9.5 s | 65.1 %, 5.7 s (1 speaker) |
| synp3 | 16.7 %, 15.3 s | 16.7 %, 8.3 s | 16.7 %, 5.3 s |
| syn6 | 36.7 %, 74.7 s (busy) | 31.4 %, 23.9 s | 44.2 %, 14.6 s |

Short and medium rows: 15 to 27 s clips are below the window length and take the old path unchanged (20 to 27 s synthetic clips found 1 speaker in the earlier short-clip report, not changed here); one hour is in the table above. Unit tests: `speaker_engine::windowed` (speakers linked across windows with differently numbered clusters, a known count kept, cancel between windows, no prints or short audio run whole, cuts in quiet spots) and the changed `speaker_engine::tests`.

## 4. What this does not show

Only text-to-speech voices that are distinct and steady; memory at one hour was the same with and without windows here, so the cap is not demonstrated; the linking distance has not been tested on real conversations (overlap, similar voices) where it matters; two 6-voice clips found 4 to 5 speakers either way. Retuning the threshold and `window_shift_ratio` needs real conversations with a tuning set and a separate held-out set (AMI and PriMock57 clips of the 2026-10-29 report: CC BY 4.0, not downloaded this week). **No default was changed except the two structural ones above (no second run; windows above 20 minutes).**
