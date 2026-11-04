# Can invented text be vetoed? Perturbation, cross-model, copy and signature tests on the raw track

Date: 2026-11-04. Apple M4 Pro, shared GPU. Turbo and Large v3, Vietnamese, prompt off, each suspect span decoded on its own with 1 s of context on each side by the new example `invented_probe` (shipped quiet-input lift on). Counts and similarities only; no text from the private meetings is stored here. Wording: "measured".

## 1. Question and short answer

*Does a parameter-light signal (perturbation agreement, cross-model agreement, the noise-reduced copy, the runtime silence signature, timing) remove the two confirmed invented units (cough clip 14, noise clip 103) and none of the 19 real quiet-speech units, so that the raw-track default becomes safe?* **No rule does.** Several catch both invented units; every one of them also flags real speech (5 to 9 of 19), or fits only because it was chosen after seeing the 21 points.

## 2. Setup and commands

```text
cargo run --release -p cm-ai --example invented_probe -- --raw <mic.wav> --copy <mic.denoised.wav> \
    --spans <[[start,end,label],...]> --turbo <ggml> --large <ggml> --language vi --signature-secs 30
python3 tests/tools/... (the analysis was a scratch script; the rules and numbers are below)
```

Each span: the isolated Turbo decode of the raw audio (`orig`), Turbo at 6 dB lower gain (`quiet6`), shifted 150 ms (`shift150`), with noise at 10 % of the window RMS (`noise`), Large v3 on the raw audio (`large`), Turbo on the noise-reduced copy (`copy`). Similarity of two texts = 1 minus word edit distance over the longer word count (case and punctuation folded, tone marks kept); `pert_mean` and `pert_min` are the mean and minimum over the three perturbations. Signature: the text each model writes for the 30 s window of the recording whose 90th-percentile 1 s level is lowest; a unit "matches" when its word bigrams are in the signature's.

Data: the 2 invented and 19 real units of the maintainer's three batches (15 in Nov 2, 4 in the Data Platform meeting), and 60 spans of the public silence set (30 invented from the synthetic silence, 30 real speech, vi, 8 s excerpts joined by 2 s of digital zero).

## 3. Results

### Real meetings (2 invented, 19 real)

| Signal | invented (14, 103) | real range | AUC (invented higher) | Behaviour on 14 and 103 |
|---|---|---|---|---|
| (A) agreement under perturbation, mean | 0.77, 0.67 | 0.13 to 0.85 | 0.70 | text is **stable**, not unstable |
| (A) agreement, minimum over the three | 0.32, 0.00 | 0.00 to 0.74 | 0.29 | one perturbation destroys it |
| (B) Turbo against Large v3 | 0.32, 0.00 | 0.00 to 0.87 | 0.18 | the models disagree |
| (B) Turbo raw against Turbo copy | 0.32, 1.00 | 0.00 to 0.30 | 1.00 | the copy decode reproduces 103 and half of 14; the copy drops real quiet speech |
| (C) overlap with the recording's own quietest 30 s | 0.95, 1.00 | 0.00 to 1.00 | 0.96 | see below |
| (D) words per second (earlier report) | 3.3, 3.7 | 1.7 to 7.6 | 0.21 | no |

* **(C) is poisoned by speech**: the quietest 30 s of Nov 2 (at 110 s) holds 78 words of speech, so its text is mostly real speech, and one real unit shares its bigrams (real clip 20, 0.67 to 0.75 overlap on its pipeline text). On the Data Platform meeting the quietest 30 s (at 313 s) is the noise stretch itself and the signature is 11 words, an outro-style sentence. A signature only works when the recording has a truly speech-free stretch; these two do not guarantee it. On the pipeline text of the raw run, a 0.5 bigram margin flags 103 and clip 20 and misses 14 (the raw run wrote nothing there).
* **(A) goes the opposite way on the real instances**: the invented text is stable under gain, shift and noise (mean 0.77 and 0.67), as the audio-driven text of clear speech is. The prior is robust to small changes.
* **(B)** cross-model disagreement does catch both (0.32, 0.00), as on the public set, but 6 of 19 real units are also at or under 0.35.
* **(D)** forced-alignment confidence (DTW token times) was not run: the cheap version, words per second against the speaker's norm, does not separate (AUC 0.21), and the real instances have ordinary timing.

### Rules

| Rule | real data: invented caught | real flagged | public set: invented caught | real flagged |
|---|---|---|---|---|
| cross <= 0.35 (models disagree) | 2 / 2 | 6 / 19 | 24 / 30 | 1 / 30 |
| min perturbation agreement <= 0.35 | 2 / 2 | 9 / 19 | 26 / 30 | 2 / 30 |
| both of the above (the coordinator's two-vote example) | 2 / 2 | **5 / 19** | 23 / 30 | 1 / 30 |
| perturbation mean < 0.5 and models disagree | 0 / 2 | 5 / 19 | 14 / 30 | 0 / 30 |
| perturbation stable (mean >= 0.6) and models disagree (<= 0.35), found by search | 2 / 2 | 0 / 19 (191 rectangles of this kind exist) | 10 / 30 | 1 / 30 |

* The last rule is the only one with 0 of 19, but it was found by searching 2-signal rectangles after seeing the points, its margin is thin (real clip 101 is at 0.67 and 0.33, invented 14 at 0.77 and 0.32), and it fails leave-one-out (thresholds fitted on one invented unit do not catch the other). It is the opposite of the plan's hypothesis (prior text *changes* under perturbation) and the public set shows the plan's direction instead (invented 0.52 median, real 0.94; cross-model 0.00 against 0.89), so the synthetic and the real instances disagree about what invented text looks like. Not implemented.
* The public set separates invented from real with every disagreement signal (AUC 0.12 to 0.14 for agreement), as it did for the evidence signals: the synthetic failure is *not voice-like*; the real instances sit on faint noise that two models both turn into speech-like text.

## 4. What this does not show

n = 2 invented units on the real recordings. The 19 "real" units are the ones the three batches selected for weak evidence, so they are the hard negatives; clear speech would be easy. The isolated decode is not the decode in context (a unit that the isolated decode writes differently, such as clip 7 whose isolated text equalled the signature sentence while the maintainer heard a few real words, is counted as real because the region has speech). Large v3 gives only one more opinion; the cost (4 Turbo decodes and 1 Large decode of a short span per suspect unit) was not measured as seconds per hour.

## 5. Decision and follow-up

1. **No veto was implemented** (`crates/cm-ai/src/invented.rs` is not written): the acceptance condition (both invented, 0 of 19 real, not fitted after the fact) is not met. The raw default is therefore not made safe by any rule measured here; until a veto exists, the raw track brings the invented sentence back over background noise (clip 103) while recovering quiet speech.
2. The cheapest honest mitigation that follows from the data: on the raw track, treat a unit as suspect when **the models disagree** (cross <= 0.35) and mark it `[unclear]` rather than delete it; this costs 6 of 19 faint-speech units their words (they become sounds) and removes both invented ones. That trade is a product decision for the maintainer.
3. More labels would decide it: a third batch of 8 clips (111 to 118, raw-track units that the copy has no text for and with weak evidence) is in `~/Movies/CaptureMann-review/silent-clips-3/`.
