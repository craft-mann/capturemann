# Whisper's own safeguards on quiet Vietnamese (0.2.7, M2d item 3)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro, GPU on. Not "verified".
- **Question:** the shipped decode sets none of whisper.cpp's safeguards itself (the defaults apply: temperature fallback with step 0.2, entropy threshold 2.4, average log-probability threshold -1.0, no-speech threshold 0.6). Does changing them help on the quiet, gated unique Vietnamese clips where transcripts lose words?
- **Setup:** Turbo, `asr_bench` built with a temporary environment hook (not in the repository), clips `viu_a`, `viu_b`, `viu_h` at -45 and -52 dBFS gated (6 cells) plus `viu_a` clean (7 runs per setting), before the coverage re-read in the pipeline was relevant to the comparison (same pipeline in all rows).

| Setting | Mean WER | Mean coverage | Whisper calls (7 runs) |
|---|---|---|---|
| defaults | 31.1 % | 86.1 % | 14 |
| temperature step 0 (no fallback) | 31.3 % | 86.1 % | 13 |
| average log-probability threshold -0.5 | 29.1 % | 86.5 % | 14 |
| no-speech threshold 0.3 | 31.1 % | 86.1 % | 14 |
| entropy threshold 2.0 | 31.1 % | 86.1 % | 14 |
| all three stricter | 29.1 % | 86.5 % | 14 |

**Result:** only the stricter log-probability threshold changed anything (-2.0 WER points, +0.4 coverage); the others are identical to the defaults. With one run per cell and the spread of earlier same-input runs (several points), this is not a finding. **No change to the decode.** The maintainer's real case (a quiet full sentence written as one word of 0.94 confidence) is not something these thresholds act on: the word is confident.

Not measured: a data set where Whisper writes text over silence (hallucination), which the no-speech threshold exists for; none of the public clips provoked it (12 s of room noise before speech produced no text at all).
