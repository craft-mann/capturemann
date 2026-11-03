# Vietnamese: Large v3, Medium and Turbo on FLEURS, and a first look at a Vietnamese Zipformer (0.2.7, M2j steps 1 and 5)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro, 48 GB, GPU on; the models installed on this Mac (no download for step 1). Not "verified".
- **Data:** FLEURS vi_vn test, unique sentences only (see the correction in `vietnamese-fleurs-baseline-2026-11-03.md`), CC BY 4.0: four ~4-minute clips (`viu_a/b/c` and the held-out `viu_h`), four short clips of 9 to 26 s (`s_u1..4`) and a 1-hour row (`viu_1h`, 3,463 s). Read speech, one speaker per utterance: it cannot show far-field, overlap or English terms inside Vietnamese. The private meeting reference (M2j step 2) has not been supplied yet, so nothing here is about meetings.
- **Build:** branch `track/mac-ai`, `asr_bench` with the current pipeline (broken-word repair, gap re-read), greedy decode as shipped, language `vi`.

## 1. Short answer (step 1)

**Keep Large v3 Turbo for Vietnamese.** On the four 4-minute clips Turbo's WER is 10.6 % (mean) against 12.3 % for Large v3 and 18.0 % for Medium; on the 1-hour row 10.7 % against 17.8 % (Large v3) and 17.3 % (Medium). Large v3 is 2.5x to 11x slower than Turbo (3.6x real time at one hour against 20.3x), uses 3.7 to 4.7 GB against 2.0 to 2.7 GB, and re-reads: 5 and 6 Whisper calls for single 4-minute pieces (re-read factor 2.1 and 2.6) and 50 calls at one hour (1.93x). Medium is as fast as Turbo and clearly less accurate. Neither older model is a better Vietnamese reader than the default on this data.

## 2. Results (WER % / coverage % / x real time / Whisper calls)

| Clip | Length | Turbo | Large v3 | Medium |
|---|---|---|---|---|
| s_u1 | 26 s | 35.9 / 100 / 28.0 / 1 | 28.2 / 100 / 12.8 / 1 | 30.8 / 100 / 23.8 / 1 |
| s_u2 | 9 s | 4.5 / 100 / 10.9 / 1 | 9.1 / 99.4 / 6.4 / 1 | 27.3 / 100 / 10.8 / 1 |
| s_u3 | 17 s | 2.1 / 95.1 / 20.1 / 1 | 2.1 / 97.2 / 8.5 / 1 | 4.2 / 100 / 15.7 / 1 |
| s_u4 | 12 s | 6.1 / 100 / 15.8 / 1 | 6.1 / 100 / 7.0 / 1 | 6.1 / 100 / 12.6 / 1 |
| viu_a | 243 s | 10.3 / 98.5 / 23.8 / 1 | 10.1 / 97.3 / 9.5 / 1 | 12.4 / 98.1 / 19.4 / 1 |
| viu_b | 243 s | 7.4 / 98.6 / 26.9 / 1 | 11.2 / 97.4 / 4.1 / 5 | 21.4 / 96.9 / 17.5 / 1 |
| viu_c | 253 s | 13.9 / 99.9 / 25.1 / 1 | 16.8 / 98.7 / 2.3 / 6 | 21.4 / 94.1 / 16.3 / 6 |
| viu_h (held out) | 249 s | 10.8 / 95.8 / 24.3 / 2 | 11.2 / 97.2 / 5.8 / 3 | 16.8 / 93.9 / 18.3 / 1 |
| mean of the four 4-minute clips | | **10.6** / 98.2 | 12.3 / 97.7 | 18.0 / 95.8 |
| viu_1h | 3,463 s | **10.7** / 97.3 / 20.3 / 16 | 17.8 / 97.2 / 3.6 / 50 | 17.3 / 93.1 / 16.8 / 31 |

Peak memory: Turbo 1.9 to 2.7 GB, Medium 1.9 to 2.7 GB, Large v3 3.6 to 4.7 GB. The short clips are single utterances (the 26 s one has a high WER for every model: one hard sentence in 39 words; with 9 to 26 s of audio, 4 words moves WER by 10 points).

## 3. A Vietnamese transducer through the linked sherpa-onnx recognizer (step 5, research only)

Nothing shipped, nothing added to the repository except the measurement example `crates/cm-ai/examples/zipformer_probe.rs` (decodes single utterances with `OfflineRecognizer`, which is already linked for speakers). The two archives were downloaded to a scratch folder outside the repository.

| | `sherpa-onnx-zipformer-vi-30M-int8-2026-02-09` | `sherpa-onnx-zipformer-vi-int8-2025-04-20` |
|---|---|---|
| Source | `hynt/Zipformer-30M-RNNT-6000h` (Hugging Face), about 6,000 hours | `zzasdf/viet_iter3_pseudo_label`, about 70k hours (per the sherpa-onnx docs) |
| **Licence on the card** | **`cc-by-nc-nd-4.0`** (the card shows the identifier only) | `apache-2.0` (the card's README is empty: no training data, no description) |
| Size | archive 26.4 MB (encoder.int8 26 MB, decoder 4.9 MB, joiner.int8 1.0 MB) | archive 60.2 MB (encoder.int8 68 MB, decoder 5.0 MB, joiner.int8 1.0 MB) |
| Token times | yes: the offline result has `timestamps` for every token (checked on 70 of 70 utterances) | yes |
| Punctuation, casing | none (lower-case text, no punctuation) | none |
| Speed (4 CPU threads, utterance by utterance) | 174x to 181x real time | 135x to 137x real time |

FLEURS WER on the same unique-sentence utterances as `viu_a/b/c/h` (greedy search, WER folded like the others): 30M model **7.9 / 9.5 / 15.1 / 8.7 %** (mean 10.3 %), 2025 model **7.9 / 9.5 / 16.7 / 10.6 %** (mean 11.2 %), against Turbo 10.3 / 7.4 / 13.9 / 10.8 % (mean 10.6 %). Both transducers are as accurate as Turbo on read speech, 5 to 7 times faster than Turbo on the GPU although they ran on four CPU threads (memory not measured). Many of their errors are insertions (word splits), not examined.

**Licence verdict:** the 30M model is licensed **CC BY-NC-ND 4.0** on its Hugging Face card: non-commercial and no derivatives. The int8 files the app would need are a derivative, and a free app that bundles or downloads them still redistributes the weights; this model cannot be used by CaptureMann. The 2025 model's card says Apache-2.0, but its README is empty and says nothing about the 70k hours of "pseudo label" training data (provenance, consent, terms); the sherpa-onnx page does not state a licence either. **Neither can be adopted on this evidence.** Before anything else the maintainer (or a lawyer) would need the data terms of the 70k model; any model also needs a pinned URL, SHA-256 and approval.

What this does not show: read speech only; no far-field, overlap, noise or English terms; the transducers give no punctuation or casing (Whisper does); no longer-than-utterance decoding was tried (long audio would need chunking at pauses).

## 4. Decision and follow-up

- Step 1: no change; Turbo stays the Vietnamese default.
- Step 2 (private reference), step 3 (glossary and initial prompt) wait for the corrected reference text; the numbers go here, the text stays with the maintainer.
- PhoWhisper medium and large (step 4) only if a gap remains after steps 2 and 3.
