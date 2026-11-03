# Transcription review, steps 1 to 3: baselines, cheap decode options, Silero VAD (0.2.7)

- **Date:** 2026-11-03. **Status:** measured; Apple M4 Pro, Large v3 Turbo, greedy decode as shipped, the glossary-style prompt **on** (`Zed Nim, Blorp, Qux`: made-up terms, so no phrase of any language), one set of clips per row. Each cell was run **3 times and the three runs were identical** on this machine (min, median and max are equal in every cell, so one number is given: Metal gave no run-to-run difference here). Not "verified".
- **Source:** the outside review in the repo root and `specs/plans/transcription-pipeline-review-brief.md`. Rules kept: no phrase lists, no hardcoding, no hardware or microphone, tests with made-up tokens.

## 1. Short answer
- **None of the options beats the baseline on all five sets, so none is kept as it is.** Closest: **high-pass plus loudness normalisation (`--agc`)** wins on three sets (4-minute FLEURS 10.8 to 10.4 %, English 16.0 to 7.1 %, noise set 8.4 to 7.6 %) and loses on one (short FLEURS 12.1 to 12.9 %); it needs a second look at per-piece gain before it is wired.
- **`n_max_text_ctx = 0`** (the carry-over switch the review should have named; `no_context` is already on in whisper.cpp 1.8 and only clears the prompt at the start of a call) removes the harm of the prompt on English (16.0 to 6.2 %) and is slightly better on 4-minute FLEURS (10.2 %), but **makes the noise set worse (8.4 to 11.2 % WER, outside-speech words 0 to 6)**.
- **Flash attention is worse everywhere** (noise set: 77 words written over non-speech against 0; English 9.0 %; FLEURS 11.5 %). Not kept.
- **Silero VAD, each merged region its own call, with the review's far-field parameters** is **worse on 4-minute FLEURS (13.8 % against 10.8 %, coverage 92.9 % against 98.8 %, 72 calls against 4)** and equal or better elsewhere (English 16.0 to 6.3 %, noise set 8.3 %, outside-speech words 0). Not kept as the default; the regions cut real speech on read FLEURS at threshold 0.32 (a smaller `min_silence` or higher padding is the next thing to try).
- **The public noise set does not reproduce invented text:** the baseline wrote **0** words outside the speech span over all nine clips (pink or brown noise at -40 to -50 dBFS, one 5 to 12 s FLEURS utterance at -30 dBFS) and 0 loops. The hallucination of the real meeting needs real room audio; this set can only show that an option makes it worse (flash attention did: 77 words). The noise was generated with FFmpeg (`anoisesrc`) because MUSAN and DEMAND (large downloads) were not fetched.
- **Finding: the prompt hurts English** with unrelated terms: the same synthetic English meeting reads 6.2 % without a prompt and 16.0 % with `Zed Nim, Blorp, Qux` (3 words outside the speech). The glossary prompt should be tested on real terms, not assumed harmless.

## 2. Setup and data
- `asr_bench` (`--prompt`, `--n-max-text-ctx`, `--flash-attn`, `--agc`, `--vad <model>`; counters `outside_speech_words` = words more than 0.5 s from every speech span, `loop_runs` = a sentence 3 times in a row, `dense_repeat_words`). Driver and clips as in the other reports.
- Sets (clips): `fleurs4min` 4 clips of 4 minutes (FLEURS vi_vn, unique sentences, CC BY 4.0); `fleurs_short` 4 clips of 9 to 26 s; `english` the synthetic 5-minute 4-voice meeting; `english_short` the 20 s clip; `noise` 9 clips of 30 s (`tests/tools/make_noise_burst_set.py`).
- Silero VAD: `ggml-org/whisper-vad`, commit `9ffd54a1e1ee413ddf265af9913beaf518d1639b`, file `ggml-silero-v5.1.2.bin`, **885,098 bytes, SHA-256 `29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf`**, licence MIT (repository card), URL `https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v5.1.2.bin`. (`ggml-silero-v6.2.0.bin`, same size, SHA-256 `2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`, exists beside it; it was not measured.) Nothing is bundled or downloaded by the app.

## 3. Results (mean over the clips of the set)

| Set | Baseline WER / coverage / words outside speech / calls | `n_max_text_ctx` 0 | Flash attention | High-pass + loudness | Silero VAD regions |
|---|---|---|---|---|---|
| fleurs4min (4 clips) | 10.8 % / 98.8 % / 0 / 4 | **10.2** / 98.1 / 0 / 4 | 11.5 / 98.2 / 0 / 4 | **10.4** / 98.6 / 0 / 4 | 13.8 / 92.9 / 0 / 72 |
| fleurs_short (4) | 12.1 / 99.1 / 0 / 4 | 12.1 / 98.8 / 0 / 4 | 12.1 / 100 / 0 / 4 | 12.9 / 98.8 / 0 / 4 | 13.0 / 98.5 / 0 / 5 |
| english (1) | 16.0 / 98.8 / 3 / 3 | **6.2** / 100 / 2 / 1 | 9.0 / 100 / 7 / 1 | **7.1** / 100 / 3 / 3 | **6.3** / 100 / 2 / 16 |
| english_short (1) | 4.3 / 100 / 1 / 1 | 4.3 / 100 / 1 / 1 | 4.3 / 100 / 1 / 1 | 4.3 / 100 / 1 / 1 | 4.3 / 100 / **0** / 2 |
| noise (9) | 8.4 / 97.7 / 0 / 10 | 11.2 / 97.7 / **6** / 10 | 14.6 / 98.9 / **77** / 10 | **7.6** / 97.7 / 0 / 9 | 8.3 / 98.7 / 0 / 10 |

Loop counters (`loop_runs`, `dense_repeat_words`) were 0 in every cell: no public clip loops. Numbers are means of the per-clip WER; "words outside speech" and "calls" are sums over the clips.

## 4. Step 4 inputs (signals on the real meeting, counts only)
The hallucinated stretch at 1:34 does not reappear in a new decode of the meeting (neither with the plain decoder nor with the pipeline), so the gate can only be designed from the stored transcript. For the 8.4 s unit at 94.7 s (6 words, mean word confidence 0.66): raw level -30.2 dBFS (the meeting's speech is -20 to -41), **speech share by our energy rule 0.42**, **by Silero 0.03**, speaker engine 2.3 s of its 8.4 s. Silero share by unit over the whole transcript (1,515 units): under 0.1 for 81, 0.1 to 0.25 for 50, 0.25 to 0.5 for 119, 0.5 to 0.75 for 204, above 0.75 for 1,061. Units with a share near 0 include quiet real speech (a unit at 103.7 s of 5.5 s has Silero 0, energy 0.05 raw and 0.69 on the noise-reduced track, speaker engine 3.9 s). **So a Silero share under 0.1 alone would flag about 5 % of the transcript, among them real quiet speech; it has to be combined with at least no-speech probability, the words per second of speech-like time, and the re-read.** Whisper's own per-segment no-speech probability and mean log-probability are now carried through the pipeline (`Transcript.segments`, never stored) so the gate can read them directly; the gate itself is not built.

## 5. What this does not show
Nine synthetic noise clips never provoked the failure, so the options are judged on the cost they add elsewhere; one English clip and 4+4 FLEURS clips per row; the three repeats were identical, which says nothing about other machines. Silero thresholds were the review's, not tuned. No step was run on the private reference (not supplied yet).

## 6. Decision and follow-up
No option is switched on. Candidates for the next round, each to be judged on all five sets: loudness normalisation per piece, `n_max_text_ctx = 0` together with a hallucination gate (since the noise set got worse), Silero with a lower `min_silence` and wider padding, and the step 4 gate fed by `Transcript.segments`.
