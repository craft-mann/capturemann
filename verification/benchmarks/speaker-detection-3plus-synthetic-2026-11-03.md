# Speaker detection baseline for 3 to 6 speakers, synthetic voices (0.2.7, M2b)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, one machine, synthetic voices only. Not "verified".
- **Machine:** Apple M4 Pro, 14 cores (10 performance), 48 GB, macOS 26.6.2.
- **Build:** branch `track/mac-ai` (commit `6ba37ac`), release build of `crates/cm-ai/examples/baseline.rs` (DER) and `perf.rs` (time, memory); sherpa-onnx 2.0.1, pyannote segmentation 3.0, 3D-Speaker CAM++ zh-en advanced (the shipped pair, `packaging/sherpa/out/models`). The vendored sherpa wrapper is unchanged.
- **Question:** what are DER, found speaker count, time and memory of the shipped settings (clustering threshold 1.0, `window_shift_ratio` 0.1) on meetings with 3, 4 and 6 speakers, before any change?

## 1. Short answer

- On three synthetic meetings (3, 4 and 6 text-to-speech voices) the shipped threshold 1.0 scores **DER 26.7 %, 44.0 % and 36.7 % (mean 35.8 %)** and found the right count in **0 of 3** (it found 2, 2 and 4). A threshold of **0.8 scored 0.1 %, 1.9 % and 18.3 % (mean 6.8 %)** with the right count in 2 of 3 (3, 4 and 5 found for 3, 4 and 6 voices); 1.2 merged everybody (mean 68.9 %, 1 speaker found).
- With the **true count given**, the app's known-count path scored 26.7 %, 18.3 % and 18.3 % (mean 21.1 %) and found 2, 3 and 5 speakers for 3, 4 and 6: giving the count did not guarantee that count.
- **Time:** 15 to 20x real time with the count unknown (12.1 s for 244 s, 18.9 s for 312 s, 55.5 s for 844 s) and 10 to 16x with the count given (the clustering step is a second run); **peak memory 252 to 342 MB** (844 s clip).
- Read with care: synthetic voices are clean, steady and very different from each other, which is why 0.8 does so much better here than it did on the real clips of the 2026-10-29 report (where 1.0 was best). This baseline says the **harness works for 3 to 6 speakers and where the shipped setting stands on it**; it does not say 0.8 is better for people.

## 2. Setup and how to repeat it

```sh
export SHERPA_ONNX_LIB_DIR=<packaging/sherpa/out/lib>
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn3 --speakers 3 --minutes 4  --seed 2   # also syn4 (4, 5 min, seed 3), syn6 (6, 14 min, seed 4)
cargo build --release -p cm-ai --examples
target/release/examples/baseline <dir with syn*.wav + syn*.rttm.txt> --sherpa <packaging/sherpa/out/models> --thresholds 0.8,1.0,1.2
target/release/examples/perf speakers <dir>/syn6.wav --sherpa <models> [--people 6] --label syn6   # time, peak memory, found count
```

The environment names for the real-model path are in `CLAUDE.md` (`CM_SHERPA_MODELS`, `CM_DIARIZE_AUDIO`, `CM_DIARIZE_SPEAKERS`; ignored test `real_diarize`). DER uses a 0.25 s collar, reference speakers mapped one to one to found ones by shared speech time (`measure::diarization_errors`).

**How the speaker count was chosen** (plan M2d item 5): in `baseline` the unknown-count columns run the app's automatic path at the given threshold; the "true count" column passes the number of voices; in `perf`, "unknown" is the app's automatic path (threshold 1.0, folding of tiny clusters, a second engine run capped at 8 when more than 8 are found), "known" is `--people N` (the automatic step after a recording).

## 3. Data

Synthetic only (`tests/tools/make_synthetic_meeting.py`, macOS `say` voices Daniel, Karen, Samantha, Rishi, Moira, Tessa; turns of one voice, no overlap, 0.3 to 1.6 s gaps). Reference turns are the exact generated turn boundaries. No private recording used. Public 3+ speaker material is proposed in [`public-corpora-proposal-0.2.7.md`](public-corpora-proposal-0.2.7.md) (AMI, already scripted) and not downloaded. No tuning/held-out split is needed yet because nothing was tuned; when the threshold is retuned (week 2) the three clips here are the tuning set and held-out material must be separate.

| Clip | Length | Voices | Turns |
|---|---|---|---|
| syn3 | 244 s | 3 | 27 |
| syn4 | 312 s | 4 | 30 |
| syn6 | 844 s | 6 | 75 |

## 4. Results

DER (found speakers in brackets), count unknown, by threshold; the last column gives the true count:

| Clip | Voices | 0.8 | 1.0 (shipped) | 1.2 | True count given |
|---|---|---|---|---|---|
| syn3 | 3 | 0.1 % (3) | 26.7 % (2) | 60.9 % (1) | 26.7 % |
| syn4 | 4 | 1.9 % (4) | 44.0 % (2) | 65.1 % (1) | 18.3 % |
| syn6 | 6 | 18.3 % (5) | 36.7 % (4) | 80.6 % (1) | 18.3 % |
| mean | | 6.8 % (count right 2/3) | 35.8 % (0/3) | 68.9 % (0/3) | 21.1 % |

Time and memory of the app path (`perf speakers`, one process per row):

| Clip | Count | Wall s | x real time | Found | Turns | Peak MB |
|---|---|---|---|---|---|---|
| syn3 | unknown | 12.1 | 20.2 | 2 | 25 | 252 |
| syn3 | known (3) | 14.9 | 16.4 | 2 | 25 | 253 |
| syn4 | unknown | 18.9 | 16.4 | 2 | 29 | 264 |
| syn4 | known (4) | 20.7 | 15.0 | 3 | 29 | 277 |
| syn6 | unknown | 55.5 | 15.2 | 4 | 71 | 336 |
| syn6 | known (6) | 84.9 | 9.9 | 5 | 74 | 342 |

The windowing and memory plan (performance.md items 2 and 4) is judged against these rows: wall time, peak MB and DER at the same threshold.

## 5. What this does not show

- **Synthetic voices.** Distinct, clean and stationary: the threshold optimum differs from real speech (the 2026-10-29 report found 1.0 best on real clips and 0.8 worse). A retune on these clips alone would be wrong; they are a regression check and a scale check for 3 to 6 speakers.
- Overlap, noise and far-field microphones are absent. Memory of 342 MB at 14 minutes is lower than the 1.2 GB at 60 minutes in performance.md; length scaling beyond 14 minutes was not measured here.
- One run per cell, no spread; three clips; a single machine; Intel not measured.
- English text only, but the speaker engine does not read the words.

## 6. Decision and follow-up

- Baseline stored. Week 2 (windowed runs, no second engine run, `window_shift_ratio`, threshold) reports DER, time and peak MB next to these rows on the same clips and on separate held-out public clips (AMI test clips of the earlier report, once approved to re-fetch).
- Open: public material with 5 to 6 speakers (AMI has 3 to 4); a longer synthetic clip (60 min) for memory.
