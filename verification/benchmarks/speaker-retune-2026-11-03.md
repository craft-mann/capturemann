# Speaker finding: retune on real conversations (0.2.7, M2b)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro; AMI and PriMock57 clips (CC BY 4.0), downloaded outside the repository with `tests/tools/make_reference_clips.py`. Not "verified".
- **Question:** are the clustering distance (`AUTO_THRESHOLD`, shipped 1.0), the segmentation `window_shift_ratio` (shipped 0.1) and the 20-minute window rule right on real conversations?

## 1. Short answer
**New defaults: clustering distance 0.8 and window shift 0.2.** On the tuning set (8 clips) the shipped pair scored mean DER 23.6 %; 0.8 with 0.2 scored 21.8 % in half the time. On the **held-out set (8 other clips)**, run once with the chosen settings next to the shipped ones: **DER 12.9 % against 19.7 %**, 66 s against 200 s for the eight clips, right count in 6 of 8 against 7 of 8. Over all 16 clips: 17.4 % against 21.7 %. Windowed runs on three real 26 to 32 minute AMI meetings (4 speakers each) scored **better than the whole run in all three** (DER 34.5, 6.7, 11.3 % against 37.5, 13.5, 15.1 %).

## 2. Data
- Tuning set (8 clips of 3 to 5 minutes): AMI headset mixes `EN2002b`, `TS3003a`, `ES2004b`, `ES2004c` (4 speakers each, overlapping speech) and PriMock57 consultations day1..4 `_consultation01` (2 speakers).
- Held-out set (8 clips): AMI `TS3003b`, `IS1009a`, `IS1009b`, `IS1009d` and PriMock57 day1..4 `_consultation05`. Caveats: `TS3003b` is the same group of speakers as `TS3003a` of the tuning set, and PriMock57 doctors recur within a day; the speaker series `IS1009` is new.
- Long set: AMI `EN2002b` (1,557 s), `ES2004c` (1,535 s), `ES2004b` (1,898 s), 4 speakers each.
- References: the datasets' own turn lists; DER with a 0.25 s collar.

## 3. Results (mean DER over the clips of a set; total seconds of engine time)

| Distance, shift | Tuning DER | Tuning s | Right count (tuning) | Held-out DER | Held-out s | Right count (held-out) |
|---|---|---|---|---|---|---|
| 0.8, 0.1 | 22.5 % | 89 | 7/8 | 14.7 % | 143 | 4/8 |
| 0.8, 0.2 | **21.8 %** | **47** | 6/8 | **12.9 %** | **66** | 6/8 |
| 0.9, 0.1 | 19.5 % | 95 | 6/8 | 16.9 % | 151 | 5/8 |
| 0.9, 0.2 | 26.1 % | 48 | 4/8 | not run | | |
| 1.0, 0.1 (shipped) | 23.6 % | 124 | 4/8 | 19.7 % | 200 | 7/8 |
| 1.0, 0.2 | 28.7 % | 48 | 3/8 | not run | | |
| 1.1, 0.1 | 33.7 % | 99 | 3/8 | not run | | |
| 1.1, 0.2 | 37.8 % | 48 | 3/8 | not run | | |

Long meetings (new defaults), DER / seconds / found speakers, whole run against windows of 600 s and 900 s:

| Meeting | Whole | 600 s windows | 900 s windows |
|---|---|---|---|
| EN2002b (26 min) | 37.5 %, 53 s, 5 | 34.5 %, 49 s, 7 | 40.8 %, 35 s, 7 |
| ES2004c (26 min) | 15.1 %, 61 s, 5 | 11.3 %, 56 s, 6 | 7.5 %, 97 s, 4 |
| ES2004b (32 min) | 13.5 %, 52 s, 5 | 6.7 %, 67 s, 4 | 10.0 %, 72 s, 5 |

Peak memory 369 to 517 MB for both. 15-second clips (a minute cut from three clips): about 0.4 s, 1 speaker found by every setting (the engine does not find a second speaker in 15 s of these conversations; unchanged by this retune).

**The private 45-minute meeting** (no truth, so agreement only), whole run against 600-second windows: with the old defaults 7 speakers found by both and 17.2 % of the speech time given to different speakers after best matching; **with the new defaults 7 against 8 found and 36.3 % disagreement**, windows 63 s against 61 s and 516 MB against 822 MB. The new defaults give the two ways less in common on this recording, while on the three AMI meetings with a truth the windows were the better of the two; which one is right on the private meeting cannot be said without a truth.

## 4. Decision and what is open
- `AUTO_THRESHOLD` 1.0 to 0.8 and `WINDOW_SHIFT_RATIO` 0.1 to 0.2 (changed in `speaker_engine.rs`). The window rule (over 20 minutes, windows of 10 minutes, link distance 0.5) stays on: better than the whole run on all three real long clips, with the caveat above.
- Open: the right count is 6 of 8 (held-out) against 7 of 8 before; a count rule is still not solved. Clips are 3 to 5 minutes, so the tuning says little about the 20-minute rule except through the three long clips. 16 clips and 3 meetings, one run each, no spread.
