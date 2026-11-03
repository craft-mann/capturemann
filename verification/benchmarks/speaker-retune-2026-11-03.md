# Speaker finding: threshold and window shift on real conversations (0.2.7, M2b)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro; AMI and PriMock57 clips (CC BY 4.0), downloaded outside the repository with `tests/tools/make_reference_clips.py`; the private 45-minute meeting is read in place, counts only. Not "verified".
- **Question:** the clustering distance (`AUTO_THRESHOLD`, shipped 1.0) and the segmentation `window_shift_ratio` (shipped 0.1): is any other pair better on real conversations without making the private meeting's speaker count worse? The criterion (maintainer, Nov 3): do not worsen the real-meeting count; improve or hold DER on the held-out set and on the long AMI meetings. Windowing (over 20 minutes, windows of 10 minutes) stays on.

## 1. Short answer
**No pair meets the criterion except the shipped one (1.0, 0.1). The defaults stay as they are.** The pair (1.0, 0.2) is the interesting alternative: on the private meeting it finds **6 speakers (the 6 people the maintainer's labels name; 1.0 and 0.1 finds 7)**, whole run and windows agree on 89.5 % of speech time (82.8 % with 1.0 and 0.1), it runs in half the time and memory is lower; on the held-out set it is better (DER 18.4 % against 19.7 %); but it is worse on the tuning set (28.7 % against 23.6 %) and on the long AMI meetings (windowed DER 15.4 % against 11.5 %). The pairs with distance 0.8 improve DER on the held-out set (12.9 % and 14.7 %) but find 7 to 8 speakers on the private meeting and disagree most between whole and windowed (36 % to 58 %).

## 2. Data
- Tuning set, 8 clips of 3 to 5 minutes: AMI `EN2002b`, `TS3003a`, `ES2004b`, `ES2004c` (4 speakers, overlapping speech) and PriMock57 day1..4 `_consultation01` (2 speakers). Held-out set, 8 clips: AMI `TS3003b`, `IS1009a`, `IS1009b`, `IS1009d` and PriMock57 day1..4 `_consultation05`. Caveats: `TS3003b` has the same speakers as `TS3003a` (tuning); PriMock57 doctors recur within a day.
- Long set: three real AMI meetings, `EN2002b` (1,557 s), `ES2004c` (1,535 s), `ES2004b` (1,898 s), 4 speakers each.
- The private meeting: 2,678 s, the maintainer's labels name 6 participants. Whole run and windows are compared by best matching (disagreement in speech time); there is no turn-level truth.
- References: the datasets' turn lists; DER with a 0.25 s collar.

## 3. Results

Mean DER over the clips (engine seconds for the whole set):

| Distance, shift | Tuning DER | Tuning s | Held-out DER | Held-out s | Long, windowed DER | Long, whole DER | Long windowed time (s, 3 meetings) |
|---|---|---|---|---|---|---|---|
| 1.0, 0.1 (shipped) | **23.6 %** | 214 | 19.7 % | 134 | **11.5 %** | 22.1 % | 279 |
| 1.0, 0.2 | 28.7 % | 52 | 18.4 % | 54 | 15.4 % | 15.4 % | 129 |
| 0.8, 0.1 | 22.5 % | 102 | 14.7 % | 107 | 15.1 % | 23.8 % | 245 |
| 0.8, 0.2 | 21.8 % | 52 | **12.9 %** | 55 | 17.5 % | 22.0 % | 137 |

Speakers found on the long meetings (truth 4 each; windowed): 1.0/0.1: 6, 4, 4; 1.0/0.2: 5, 4, 4; 0.8/0.1: 6, 5, 5; 0.8/0.2: 7, 4, 6. Windowed beats whole for the shipped pair on all three meetings (EN2002b 21.5 against 23.9 %, ES2004b 6.9 against 36.6 %, ES2004c 6.2 against 6.0 %), and a single meeting can swing 30 points between settings, so differences of a few points on three meetings are weak evidence.

The private meeting (the maintainer's 6 participants), whole run against 600-second windows, then the two compared:

| Distance, shift | Found (whole / windowed) | Different speaker for this share of the speech | Whole run: time, memory | Windowed: time, memory |
|---|---|---|---|---|
| 1.0, 0.1 (shipped) | 7 / 7 | 17.2 % | 119 s, 830 MB | 124 s, 575 MB |
| 1.0, 0.2 | **6 / 6** | **10.5 %** | 60 s, 565 MB | 65 s, 493 MB |
| 0.8, 0.1 | 8 / 8 | 58.5 % | 119 s, 1,371 MB | 127 s, 685 MB |
| 0.8, 0.2 | 7 / 8 | 36.3 % | 60 s, 821 MB | 66 s, 530 MB |

Short rows: a 15 s cut of three clips takes about 0.4 s and finds 1 speaker under every setting. Memory on the long and private audio is lower with windows in every cell (for example 575 against 830 MB).

## 4. Decision and what is open
- **Defaults unchanged: 1.0 and 0.1**, windowing on (the comparison of windows against the whole run is in favour of windows on all three real long meetings for the shipped pair).
- Offered for the maintainer to choose: (1.0, 0.2) is twice as fast (about 60 s against 120 s for the 45-minute meeting), gets the private meeting's count right and agrees most between whole and windowed, at the price of 5 DER points on the tuning clips and 4 on the long meetings. This is a speed choice, not a measured gain in accuracy.
- Open: the right count over the 16 short clips (shipped 4 of 8 tuning and 7 of 8 held-out); the count rule is still not solved. 16 short clips, 3 long meetings and 1 private meeting, one run each, no spread; single clips swing by tens of points, so none of the differences above is large against that.
