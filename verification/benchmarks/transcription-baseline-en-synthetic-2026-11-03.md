# Transcription baseline, English, synthetic speech (0.2.7, M2a and M2d)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, one machine, synthetic speech only. Not "verified": no manual script is involved.
- **Machine:** Apple M4 Pro, 14 cores, 48 GB, macOS 26.6.2, GPU on (Metal).
- **Build:** branch `track/mac-ai`, release build of `crates/cm-ai/examples/asr_bench.rs` (commit `6ba37ac`); product code unchanged since 0.2.6 (the only product-side change on the branch is the three `pending_pin` catalogue rows, which are not offered).
- **Question:** before any change to transcription (re-read cost, uncovered speech, repeats, broken tokens), what are word error rate (WER), real-time factor (RTF), the number of Whisper calls and the M2d counters on a fixed English set, per model?

## 1. Short answer

- On four clean synthetic clips (4 to 14 minutes, 1 to 6 voices) the mean WER is **6.0 % for Large v3 Turbo**, **5.9 % for Medium** and **11.1 % for Large v3**. Turbo and Medium ran at 17.8x to 28.3x real time with one Whisper call per piece (no re-reads).
- **Large v3 re-reads a lot on this audio:** 3 to 11 Whisper calls for 1 to 3 pieces, i.e. **2.0x to 3.0x the audio read** (`reread_factor`), RTF 2.6 to 4.2 (Turbo: about 20). Its extra words (169 insertions on the 14-minute clip) are the repeats that the loop check had to cut and that remained. This is the cost of "performance.md item 3" measured: on this set a re-read pass roughly triples the time of the Large v3 path. It says nothing yet about real speech, where Turbo is the default.
- M2d counters on this set: **coverage 99.3 % to 100 %** (Turbo, Medium, Large), **0** U+FFFD words, **0** foreign-script words, **0** spread-out repeats. Synthetic clean speech does not reproduce the three failure kinds; they are reproduced by the unit tests in `crates/cm-ai/src/failure_kinds/tests.rs` (three `#[ignore]` tests name the planned fixes and fail today).

## 2. Setup and how to repeat it

```sh
export SHERPA_ONNX_LIB_DIR=<packaging/sherpa/out/lib>      # the build links the speaker libraries
cargo build --release -p cm-ai --example asr_bench
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn1 --speakers 1 --minutes 4  --seed 1
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn3 --speakers 3 --minutes 4  --seed 2
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn4 --speakers 4 --minutes 5  --seed 3
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn6 --speakers 6 --minutes 14 --seed 4
target/release/examples/asr_bench --audio <dir>/syn1.wav --reference <dir>/syn1.txt --speech <dir>/syn1.speech.json \
  --model <ggml-large-v3-turbo.bin> --model-id large-v3-turbo --language en --label syn1
```

`asr_bench` runs the app's own `transcribe_pieces` over a counting wrapper of `WhisperTranscriber` (greedy, one candidate, as shipped) and prints one JSON line: WER with its parts, RTF (audio seconds / wall seconds, model load excluded), `whisper_calls`, `audio_read_secs` and `reread_factor` (audio the model was given in total / audio length), the pieces report (`split`, `repeats_dropped`), speech coverage, the M2d counters and peak memory. No model or audio is downloaded; the models are the ones installed in `~/Library/Application Support/CaptureMann/ai/models`. The other-audio (noise-reduced vs original microphone) second reading of the app is not part of this harness (`other_audio` is 0 here).

## 3. Data

Synthetic only. `tests/tools/make_synthetic_meeting.py` writes sentences built from word lists and speaks them with macOS `say` voices (Daniel, Karen, Samantha, Rishi, Moira, Tessa), 0.3 to 1.6 s of silence between turns, numbers written out as words. The text is ours; the audio stays outside the repository. No private recording was used or read. Public clips with reference text are proposed, not downloaded: see [`public-corpora-proposal-0.2.7.md`](public-corpora-proposal-0.2.7.md).

| Clip | Length | Voices | Turns | Reference words |
|---|---|---|---|---|
| syn1 | 240 s | 1 | 17 | 710 |
| syn3 | 244 s | 3 | 27 | 739 |
| syn4 | 312 s | 4 | 30 | 900 |
| syn6 | 844 s (3 pieces) | 6 | 75 | 2,481 |

## 4. Results

| Model | Clip | WER | Subst / Ins / Del | Wall s | RTF | Whisper calls | Audio read / length | Pieces split | Coverage | Peak MB |
|---|---|---|---|---|---|---|---|---|---|---|
| large-v3-turbo | syn1 | 5.8 % | 33 / 0 / 8 | 8.5 | 28.3 | 1 | 1.00 | 0 | 100 % | 1,965 |
| large-v3-turbo | syn3 | 5.4 % | 27 / 1 / 12 | 12.8 | 19.0 | 1 | 1.00 | 0 | 100 % | 1,961 |
| large-v3-turbo | syn4 | 6.2 % | 46 / 0 / 10 | 14.9 | 20.9 | 1 | 1.00 | 0 | 100 % | 1,982 |
| large-v3-turbo | syn6 | 6.8 % | 110 / 0 / 58 | 41.7 | 20.2 | 3 | 1.00 | 0 | 99.3 % | 2,019 |
| medium | syn1 | 5.6 % | 35 / 0 / 5 | 13.5 | 17.8 | 1 | 1.00 | 0 | 100 % | 1,990 |
| medium | syn3 | 5.3 % | 27 / 0 / 12 | 12.2 | 20.0 | 1 | 1.00 | 0 | 100 % | 1,990 |
| medium | syn4 | 6.3 % | 47 / 0 / 10 | 15.9 | 19.5 | 1 | 1.00 | 0 | 100 % | 2,001 |
| medium | syn6 | 6.5 % | 122 / 0 / 40 | 44.5 | 19.0 | 3 | 1.00 | 0 | 100 % | 2,044 |
| large (v3) | syn1 | 8.3 % | 33 / 18 / 8 | 91.5 | 2.6 | 7 | 3.00 | 3 | 100 % | 3,736 |
| large (v3) | syn3 | 10.0 % | 27 / 35 / 12 | 74.5 | 3.3 | 5 | 2.59 | 2 | 100 % | 3,786 |
| large (v3) | syn4 | 12.7 % | 58 / 45 / 11 | 73.4 | 4.2 | 3 | 2.00 | 1 | 99.9 % | 3,784 |
| large (v3) | syn6 | 13.3 % | 124 / 169 / 36 | 289.5 | 2.9 | 11 | 2.03 | 4 | 100 % | 3,855 |

Mean WER over the four clips: Turbo 6.0 %, Medium 5.9 %, Large v3 11.1 %. The WER is the usual one with case, diacritics and punctuation folded; the same figure without hesitation sounds is identical (the synthetic voices make none). Health counters (U+FFFD, foreign script, confidence under 5 %, spread-out repeats): 0 on every row.

## 5. What this does not show

- **Synthetic, clean, one voice per person, no overlap, no room.** These WERs are a harness check and a before/after yardstick for the same clips, not what a person will see on a meeting. Not "accuracy".
- **A good part of the WER is probably style, not recognition:** the reference writes numbers as words and hyphenated compounds ("thirty-three"), Whisper often writes digits; the fold joins the hyphen. This was not examined word by word.
- Large v3's repeats here are on text-to-speech with long, regular sentences; do not read them as how it behaves on speech.
- One run per cell: no spread. The earlier performance report (`performance-m4pro-2026-10-29.md`) had three runs and a spread of under 3 %.
- English only (decision O1 amended on Nov 3: Vietnamese is measured next on a public corpus, not here). No Vietnamese was run this week; see the proposal file.
- The M2d failure kinds were found on a private 45-minute Vietnamese meeting. No number or text from it is stored here. Coverage, spread-out repeats and broken tokens are reproduced only by constructed word lists and signals (unit tests), and are 0 or full on the clean clips above, so the **fixes cannot be judged on this set**: they need a clip where Whisper really skips or loops, which is the public-corpus work.

## 6. Decision and follow-up

- Baseline stored. Week 2 compares against it: a change to re-reads must keep WER within noise and cut `reread_factor` and `whisper_calls` for Large v3; fallback-parameter experiments (M2d-3) run on this harness first.
- Open: a clip that provokes skipped speech on Turbo; the public clips; Vietnamese WER for Turbo against PhoWhisper (M2e).
