# Release 0.2.3 plan: speaker detection and transcript improvement

Status: **in progress** (Oct 29, 2026): Stage 0 tools built (`measure`, `baseline`, clip script), English benchmark stored in `verification/benchmarks/`, Stage 1 items 1 and 2 done for English (voice model and threshold swapped); Vietnamese deferred. Drafted Oct 28, 2026. Scope set by the maintainer: speaker detection
improvement and transcript improvement. Translation is not in this build. Rules that apply:
general techniques and options only, no phrase lists or fixes for one recording (open source);
nothing is called "verified" without a stored report in `verification/`; no GPL in shipped
binaries; every new option has an MCP parameter and a CLI flag; UI text through `cm_i18n`.

## Why measure first

0.2.2 measured repetition (8-word groups repeating), not accuracy. Speaker labels have no accuracy
figure at all (AI-8.AC1 to AC3 open). Every idea below is a guess until there is a number, so
Stage 0 comes before any tuning and decides what the later stages are.

## Stage 0: measurement tools and a reference set

- **WER tool** (`cm-ai` example or ignored test, not shipped): word error rate between a
  transcript and a hand-corrected text, with the same folding as `pieces::checks::fold`
  (case, diacritics, `đ`), reported per excerpt. Output: WER, and counts of substitutions,
  insertions, deletions.
- **DER tool**: diarization error rate from a hand-made reference turn list (RTTM-like text:
  start, duration, speaker), with a 0.25 s collar, speaker mapping by best overlap. Reports
  missed speech, false alarm, speaker confusion, and the speaker count found against true.
- **Reference set** (the maintainer provides; stays outside the repo, only numbers and a
  description go in `verification/`): a few excerpts of 3 to 5 minutes, hand-corrected and
  hand-labelled, covering at least: Vietnamese and English, 2 and 3+ speakers, headset and
  in-the-room, clean and noisy. Without these there is no accuracy claim.
- Output: `verification/SPEAKERS-WER-DER-baseline-<date>.md` with the 0.2.2 numbers as the
  baseline. Every later stage reports against it.

## Stage 1: speaker detection

Known weak points in the code today (`cm-ai/src/speaker_engine.rs`):

1. The embedding model is **CAM++ English VoxCeleb**. The maintainer's recordings are largely
   Vietnamese; a model trained on other languages may separate voices worse. Evaluate other
   embedding models that sherpa-onnx 2.0.1 can load (candidates to check, licence and size
   first, none assumed: multilingual 3D-Speaker CAM++ / ERes2Net, WeSpeaker ResNet). Any model
   must be permissively licensed and pinned by SHA-256 like the current two.
2. `AUTO_THRESHOLD` (1.2), `FOLD_MIN_SECS` (8 s) and `FOLD_FRACTION` (4 %) were tuned on **one**
   English recording. Re-tune on the reference set by sweep, and prefer a rule that adapts
   (for example choose the cluster count from the merge-distance gap) over a single constant.
3. The speaker count is a hint the user must give for good results. Improve "Not sure":
   estimate the count, show it with how confident it is, and make "Find again with N people"
   one click.
4. Word-to-speaker assignment uses the word midpoint. Check boundary errors (a word split by a
   turn change) and short back-channel turns ("ừ", "yeah") that tidy-up may be erasing
   (`FLICKER_SECS`, `MERGE_GAP_SECS`).
5. Use the transcript's own pauses: speaker changes mostly happen at pauses, so snapping turn
   boundaries to the nearest pause in the word timings is a general technique.
6. One blocking engine call means progress is 0 then 1 and a long file looks stuck. Run the
   engine in windows (like transcript pieces) so progress and cancel work, then join speakers
   across windows by voice-embedding distance. Embeddings stay inside the call (AI-8.AC6).
7. Both-track recordings (microphone and system audio are separate tracks): use the track as a
   strong prior (the microphone track is mostly "me") before clustering. This is general, not
   specific to any recording.

Done when: DER on the reference set is measured and the chosen settings beat the 0.2.2 baseline
on the set (not on one file), the number is stored, and AI-8 status text gives the figure.

## Stage 2: transcript improvement

Order by expected gain per risk, each judged by the Stage 0 WER and by the repeat metrics from
0.2.2 (they must not get worse):

1. **Voice-activity detection** (Silero v5.1.2 `ggml-silero-v5.1.2.bin`, 885,098 bytes, supported
   by whisper.cpp): skip silence and noise so Whisper does not invent text there. Confirm the
   licence (MIT expected, to check) and pin by SHA-256 before anything ships.
2. **Original microphone as default** or chosen per recording: 0.2.2 data showed fewer loops with
   the original audio but ~1,700 words the other read missed. Decide by WER, not by loops.
3. **Second reading beyond loops**: today the other audio is read only when a piece loops.
   Option: read low-confidence pieces from both audios and keep the better by confidence and
   agreement. Costs time, so it would be an option ("Higher accuracy"), not the default.
4. **Beam search** as the same "Higher accuracy" option; measure WER gain against time.
5. **Proofreading pass** with the built-in text model: fix obvious recognition errors using the
   glossary and context, constrained so it cannot add or remove content (check by word-level
   diff, reject changes beyond a small edit distance). Off by default, undoable, and shows what
   it changed. Only if Stage 0 shows enough gain to justify it.
6. **Glossary and prompt use**: check how the glossary prompt carries across pieces and whether
   a short context from the previous piece helps WER.

Done when: WER on the reference set is measured for each kept option, defaults are chosen by
those numbers, options appear in MCP (`transcribe` parameters), CLI flags and the Re-generate
panel, and rejected ideas are written down with their numbers.

## Not in 0.2.3

Translation, the first-use setup sheet, Windows and Linux, and Ollama / LM Studio.

## Carried-over checks (small, do alongside)

- Intel DMG test on a real Intel Mac (`tests/manual/INTEL-0.2.1.md`), needs a device.
- First real run of `release.yml` on GitHub.
- Find the flaky `cm_daemon` test (failed once under CPU load in a report run).
- Dependabot Actions-pin PR.

## Release steps

Same as 0.2.2: version bump and changelog, `make release` for arm64 and `CM_ARCH=x86_64`, tests,
`cargo deny`, Trivy, reports in `verification/`, draft release, download-back check, Homebrew
cask. The owner pushes.
