# Vietnamese baseline on FLEURS vi_vn: Large v3 Turbo and PhoWhisper-small (0.2.7, M2a/M2d/M2e)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, one machine. Not "verified".
- **Machine:** Apple M4 Pro, 14 cores, 48 GB, macOS 26.6.2, GPU on.
- **Build:** branch `track/mac-ai`, release build of `crates/cm-ai/examples/asr_bench.rs`; the shipped transcription path (greedy, timestamps on) unchanged.
- **Question:** before any change, how do Large v3 Turbo (the default) and a PhoWhisper-small converted for whisper.cpp do on Vietnamese read speech, in WER, speed, speech coverage and broken-token counts?

## 1. Short answer

- **Turbo on Vietnamese read speech: WER 20.8 %, 24.8 %, 22.7 % (mean 22.8 %)** on three 4-minute clips, 9.4x, 13.5x and 7.6x real time. **Speech coverage is only 85.4 % to 89.6 %**: 18 to 28 seconds of speech per clip got no words (2 to 3 stretches), and the re-read machinery ran **2.0x to 3.2x** the audio (3 to 9 Whisper calls for a single 4-minute piece, 1 to 4 splits). The first M2d failure kind (speech without words) and the re-read cost (performance.md item 3) both show on public Vietnamese data, with no private recording involved.
- **PhoWhisper-small, converted with `packaging/phowhisper/convert.sh` and run with the app's current settings, is not usable:** mean WER **60.9 %** (mostly deleted words: it dropped the start of a 9-second test sentence), 6.6x to 11.1x real time, 0.8 GB peak memory. With a no-timestamps decode (an experiment, not committed) it still scored **69 % to 74 %**: it repeats the first sentence of the clip and glues words together (`vàtuyết`). So the conversion works, but the model needs model-specific decoding before it can be compared fairly; the comparison with Turbo is therefore **not yet made**.
- Other counters: 0 U+FFFD words and 0 spread-out repeats for Turbo; PhoWhisper (default settings) produced 1 U+FFFD word and 17 to 19 words under 5 % confidence per clip, plus 1 to 2 foreign-script words in two clips.

## 2. Setup and how to repeat it

```sh
# data: FLEURS vi_vn test split (see section 3); keep it outside the repository
python3 -I tests/tools/concat_utterances.py --tsv <list.tsv> --out <dir> --name vi_a --secs 240 --skip 0    # vi_b: --skip 25, vi_c: --skip 50
cargo build --release -p cm-ai --example asr_bench
target/release/examples/asr_bench --audio <dir>/vi_a.wav --reference <dir>/vi_a.txt --speech <dir>/vi_a.speech.json \
  --model <ggml-large-v3-turbo.bin> --model-id large-v3-turbo --language vi --label vi_a
# PhoWhisper: PYTHON=python3.12 OUT=<dir> packaging/phowhisper/convert.sh small   (then --model <dir>/ggml-phowhisper-small.bin)
```

`list.tsv` is `path<TAB>normalized transcription` for each test wav (FLEURS column 4: lowercase, no punctuation). The WER fold removes case, diacritics and punctuation, so tone marks are not scored (a wrong tone on a right syllable is not an error here: this makes the Vietnamese WER optimistic).

## 3. Data

**FLEURS** `vi_vn`, test split (Hugging Face `google/fleurs`, `data/vi_vn/test.tsv` and `audio/test.tar.gz`, 857 utterances, 544 MB), licence CC BY 4.0, Conneau et al., "FLEURS: Few-shot Learning Evaluation of Universal Representations of Speech" (2022). Read Wikipedia-style sentences, one speaker per utterance. Nothing else was downloaded (no Common Voice, no VIVOS). Audio stays outside the repository; only numbers are stored.

Three clips of about 4 minutes, from utterances joined with 0.6 s of silence, in file order and without sharing an utterance: `vi_a` (utterances 0 to 19, 248 s, 552 words), `vi_b` (25 to 43, 243 s, 532 words), `vi_c` (50 to 67, 240 s, 569 words). Nothing was tuned, so there is no tuning/held-out split yet; when settings are chosen from these clips, a different part of the test split must be used to confirm.

## 4. Results

| Model | Clip | WER | Subst / Ins / Del | Wall RTF | Whisper calls | Audio read / length | Coverage | Uncovered | Peak MB | Low-conf words | U+FFFD |
|---|---|---|---|---|---|---|---|---|---|---|---|
| large-v3-turbo | vi_a | 20.8 % | 48 / 12 / 55 | 9.4 | 7 | 2.89 | 89.6 % | 2 regions, 18 s | 2,009 | 0 | 0 |
| large-v3-turbo | vi_b | 24.8 % | 27 / 41 / 64 | 13.5 | 3 | 2.00 | 86.8 % | 2 regions, 18 s | 1,994 | 0 | 0 |
| large-v3-turbo | vi_c | 22.7 % | 28 / 25 / 76 | 7.6 | 9 | 3.19 | 85.4 % | 3 regions, 28 s | 2,001 | 0 | 0 |
| phowhisper-small (default settings) | vi_a | 60.7 % | 98 / 13 / 224 | 6.6 | 3 | 2.00 | 100 % | 0 | 792 | 17 | 1 |
| phowhisper-small (default settings) | vi_b | 59.2 % | 112 / 32 / 171 | 7.6 | 1 | 1.00 | 99.9 % | 0 | 766 | 17 | 1 |
| phowhisper-small (default settings) | vi_c | 62.9 % | 149 / 1 / 208 | 11.1 | 1 | 1.00 | 100 % | 0 | 766 | 19 | 0 |
| phowhisper-small (experiment: no timestamps) | vi_a | 69.9 % | 51 / 0 / 335 | 62.8 | 1 | 1.00 | 97.3 % | | 813 | 0 | 0 |
| phowhisper-small (experiment: no timestamps) | vi_b | 66.7 % | 95 / 2 / 258 | 46.5 | 1 | 1.00 | 99.6 % | | 813 | 0 | 0 |
| phowhisper-small (experiment: no timestamps) | vi_c | 73.8 % | 65 / 0 / 355 | 79.0 | 1 | 1.00 | 100 % | | 813 | 0 | 0 |

Coverage counts a second of speech (the utterance spans) as covered when a word lies within 1 s; "uncovered" is runs of at least 5 s without a word near them. PhoWhisper's coverage is high because it writes words everywhere, not because they are right (its WER is 60 %).

The PhoWhisper file: `ggml-phowhisper-small.bin`, 487,601,984 bytes, sha256 `db8de368e822fce5dcdce7c0c4c80c4b554c27655516dea5ec8c093e22f58785`, converted from `vinai/PhoWhisper-small` revision `a86b604c346caf7148c37512eafe783a16420adb` with whisper.cpp v1.9.5 `convert-h5-to-ggml.py` (float16). `ModelStore::import` accepts it (ignored test `a_converted_phowhisper_file_is_accepted_by_import`). A single 9.4 s FLEURS sentence with the default settings came out as `anderson gần như không bị gián đoạn.` (the first half missing; Turbo wrote the full sentence), and with no timestamps as the complete sentence with glued words. The checkpoint, the Python environment and the tools were deleted after the test; the ggml file stays in a scratch folder, outside the repository.

## 5. What this does not show

- **Read, studio-like sentences concatenated by us**, not a meeting; the real meeting that prompted M2d was noisier and had several speakers. Coverage and re-read counts here are therefore a floor for how bad it can be.
- WER ignores tone marks and punctuation (folded). Tone errors would raise every figure.
- One run, three clips (about 12 minutes, 1,653 words): the spread between clips (20.8 to 24.8 % for Turbo) is as large as most differences we would try to detect.
- The concatenation with short silences may itself provoke Whisper's skipping of the speech after a pause; it is the plausible reason why coverage falls, not shown here. A set of single utterances would separate "the model skips" from "joined clips confuse it".
- PhoWhisper-small only; medium and large were not converted. The failure on PhoWhisper is a statement about this conversion run with the app's decode settings (timestamps on, greedy, `language=vi`), not about the model: it may need no-timestamp decoding, a different tokenizer treatment or an unconverted-faithful path. Word times (DTW alignment heads) were not examined.

## 6. Decision and follow-up

- Baseline for Turbo stored: the M2d "coverage re-run" fix and the re-read-cost work are judged on `coverage` (85.4 to 89.6 %), `reread_factor` (2.0 to 3.2) and WER (20.8 to 24.8 %) on these three clips, with another FLEURS slice held out.
- PhoWhisper (M2e) needs model-specific decoding work before a fair comparison with Turbo; until then it must not be offered and no size becomes the suggested Vietnamese model.
