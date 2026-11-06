# Transcription: how it works today and what goes wrong (0.2.7 review brief)

**Date:** 2026-11-03. **Purpose:** a self-contained description of CaptureMann's local transcription pipeline and the open quality problems, so a reviewer without the repository can suggest solutions. Everything here is from the code on `main` (`dfeaf10`) and the reports in `verification/benchmarks/`. Numbers are measured on one Apple M4 Pro (48 GB), one run per cell, and are **not** "verified" in the project's sense.

**Constraints that limit solutions**

- Open source (Apache-2.0). **No phrase lists, no per-language blocklists, no fixes for one recording.** Only general techniques and options.
- Everything runs locally (no cloud ASR). No GPL code in shipped binaries. Models are downloaded by the user; the app never ships weights.
- macOS (Apple Silicon first) is the 0.2.7 target. Windows and Linux follow in 0.2.8 and 0.3.0.
- Every claim about quality needs a stored report.

---

## 1. What the app records

A recording is a bundle folder with one file per track: `screen`, `camera`, `mic`, `system` (system audio), `cursor`. An audio-only recording has `mic` (and optionally `system`). The mic track is stored twice: the raw file and a noise-reduced copy (the default for transcription). Meetings are typically recorded with the user's laptop microphone (far-field for other people in the room) plus system audio (remote participants).

## 2. The pipeline, step by step

Code: `crates/cm-ai/src/{recording,audio,pieces,whisper,vad,speakers,speaker_engine,sounds,failure_kinds,glossary,caption}.rs`; orchestration in `crates/cm-daemon/src/ai/`.

1. **Audio selection.** `recording.rs` picks the mic track (the noise-reduced copy by default, the raw file when the user asks for "original audio"), and optionally mixes the system track in (`mix_at`). Timeline: the files have pauses removed; words are mapped back to **session time** (`file_to_session`).
2. **Decode.** FFmpeg (an LGPL sidecar) decodes to mono 16 kHz `f32` (`audio::decode_16k_mono`). About 230 MB of RAM per hour.
3. **Cutting into pieces (`pieces.rs`).** Audio up to 600 s is one piece. Longer audio is cut at the quietest 100 ms within ±20 s of every 300 s. Reason: Whisper conditions each 30 s window on the previous text, so one bad window can poison all following ones (on Oct 28 an 88-minute Vietnamese recording came back with one phrase repeated 2,480 times from 5:35 to the end, decoded at high confidence). Each piece is a separate run, so a loop cannot cross a boundary, and the glossary prompt starts every piece.
4. **Whisper decode (`whisper.rs`).** whisper.cpp through `whisper-rs`, Metal on macOS. Greedy sampling (`best_of: 1`), `suppress_blank`, `suppress_nst`, token timestamps on, DTW alignment for word times, `initial_prompt` from the user's glossary (capped at about 224 tokens), language fixed by the user or detected from the first piece (a list such as `vi,en` can be given). **Whisper's own safeguards are at their defaults** (temperature fallback step 0.2, entropy threshold 2.4, average log-prob threshold -1.0, no-speech threshold 0.6). Models offered: tiny, base, small, medium, large (v3), **large-v3-turbo** (the default), plus three PhoWhisper entries (Vietnamese fine-tunes, not yet downloadable). Words are built from tokens (`build_words`): a token with a leading space starts a word, `[BLANK_AUDIO]`-style annotations are dropped, per-word confidence is the mean token probability.
5. **Checks and repairs per piece (`pieces.rs`, `pieces/checks.rs`, `failure_kinds.rs`).**
   - **Loop detection** (`is_degenerate`, `find_runs`): a unit of up to 8 tokens repeated many times. A looping piece is cut in half at a quiet point and read again (up to 3 levels); if it still loops, repeats are collapsed (`collapse_repeats`, `drop_soft_repeats`).
   - **Broken-token repair:** words with stray characters (non-text code points) are re-read; if the re-read is better it replaces them, otherwise the stray characters are stripped or the word dropped.
   - **Coverage re-read (`fill_gaps`):** the VAD finds speech-like stretches of at least 3 s that have no word within 0.5 s, or regions where words fill under 25 % of the region; each is read again on its own (at most 20 per piece). A re-read is kept when its words average at least 0.4 confidence.
   - **Repeated-sentence rule (new in 0.2.7):** the same sentence 3 or more times in a row, or 5 or more times within 60 s, is re-read alone with no prompt. If the re-read is not a loop it replaces the stretch; if the re-read agrees (about the same count, at most 6 repeats, at most 7 words/s) the repeat is kept as really said; otherwise the span becomes a sound entry (`[noise]` when there is energy but no speech evidence, `[unclear]` when speech evidence exists, `sounds.rs`). Sound entries live in `transcript.json` beside `words`, so captions, reports, translation and search never see them.
   - **Other-audio fallback:** a bad piece can be read from the other source (mic ↔ system) when the lengths match.
6. **Glossary (`glossary.rs`).** After decoding, each "heard as" spelling is replaced by the user's term (word-based, case-insensitive, diacritics must match).
7. **Speakers (`speaker_engine.rs`, `speakers.rs`).** Independent of the words. sherpa-onnx v2.0.1: pyannote segmentation + CAM++ voice embeddings, clustering with `AUTO_THRESHOLD = 1.0` (higher: fewer speakers), `WINDOW_SHIFT_RATIO = 0.1`. Audio over 20 minutes is cut into windows linked by voice prints. Words get a speaker when read (`assign_words`, by time overlap), so the transcript file never changes. Headset mode skips diarization for the mic.
8. **Captions (`caption.rs`).** Lines are built from words (`build_captions`, `normalize_lines` repairs zero-length lines from words stacked on one instant); edited lines are stored separately.
9. **Live mode (M2h, audio recordings only, off by default until a 20-minute hardware check).** The same pieces are cut and read while the recording is still being written, so Stop only has to read the last piece (`pieces/live.rs`, `transcript.live.json` checkpoints).
10. **Other consumers:** "Suggest cuts" (silence + filler words, with a speech veto from transcript words and speaker segments), reports and translation through a local Gemma 4 E2B (`llama-server`). Both depend on the transcript being right.

## 3. What works (measured)

- **English:** business-level on the maintainer's check; good on the synthetic and public English clips (`transcription-baseline-en-synthetic-2026-11-03.md`).
- **Vietnamese read speech** (FLEURS vi_vn, unique sentences): Turbo mean WER about 10.6 % on 4-minute clips and 10.7 % on a 1-hour row at about 20× real time. Large v3 is 12.3 % / 17.8 %, 3-10× slower; Medium 18.0 % / 17.3 %. Turbo stays the default (`vietnamese-models-2026-11-03.md`).
- A Vietnamese Zipformer transducer (sherpa-onnx) is as accurate on read speech (10.3 %, 5-7× faster) but its licence is CC BY-NC-ND (unusable) or unclear (the 70k-hour model), and it gives no punctuation or casing.

## 4. The problems (what a reviewer should solve)

The failing case is a **real Vietnamese team meeting** (about 1 hour, 6 known participants, far-field laptop mic plus system audio, Vietnamese with English technical terms mixed in, overlapping speech, quiet and distant talkers). The meeting audio and its transcripts are private; the numbers below come from the maintainer's checks and the agent's runs on it.

### P1. Hallucinated text over quiet or non-speech audio

Whisper writes **text that nobody said**: typical "video outro" lines in the training data (an invitation to subscribe to a channel, thanks for watching, see you next time). It appears at quiet stretches (for example about 1:34 in the meeting), is confident enough to pass every current check, and is **promotional content** in a business transcript. The current rules cannot catch it: the sentences are not repeated, and the confidence is not low. Whisper's own no-speech threshold did not act on the clips tried so far; none of the public clips provoked this failure (12 s of room noise before speech produced no text), so there is **no public test set that reproduces it**.

Ideas already planned: use the no-speech probability and average log-prob per segment together with the VAD ratio and the level relative to the speaker's normal level, then re-read or mark as `[unclear]`/`[noise]`. Open: which signal separates a quiet real sentence from a hallucination, without a phrase list.

### P2. Repeated short phrases with high confidence

The same short word or sentence appears again and again, with confidence about 0.94, **over audio that contains real speech** (the maintainer listened: other words were said). In the stored transcript there were 4 runs (78 words), for example 16 repeats of one 2-word phrase over 26.6 s, and 18 words in 0.9 s (physically impossible). Switching to Turbo produced dozens of consecutive repeats in another segment. The audio under the repeats is not silent (-29 to -37 dBFS), so a level check alone cannot catch it. The structural rule in §2.5 fixes most of it. On a new run with the fix on Turbo, one run of 18 words remained (a 6-word sentence three times, at a plausible rate). **Caveat:** with the fix switched off in the agent's own run the loops reproduced less strongly, so the improvement cannot be attributed to the rule alone, and the maintainer's own run still showed repeats with the first build of the rule.

### P3. Far-field, overlapping, code-switched Vietnamese

Read speech is fine (about 10 % WER); the meeting is not. We have no number yet because the **hand-corrected reference** (a 5-minute stretch) has not been supplied. Suspected causes, none yet measured on the meeting:

- the mic is far from most speakers, so words are quiet and Whisper either skips them (coverage gaps, partly handled by the re-read) or invents text;
- English terms inside Vietnamese sentences (Whisper is run with one language; a list `vi,en` only picks the language of the first piece);
- overlap between speakers (one decode stream, no separation);
- a noise-reduced mic copy may remove soft consonants Vietnamese needs (the "original audio" option exists but nobody has measured which is better on this meeting).

### P4. Wrong or garbled tokens

Words with non-text characters (for example mixed-script junk inside a Vietnamese word) appeared in the first real meeting transcript. The repair (re-read, strip, drop) removes them, but a stripped word may leave a hole. The cause (a decoder emitting a partial UTF-8 byte sequence for Vietnamese diacritics, or a broken sub-word) is not proven.

### P5. Speaker count on the real meeting

Retuned clustering (threshold 0.8, shift 0.2) improved real English meetings (held-out DER 19.7 % → 12.9 %, 3× faster) but on the Vietnamese meeting found 7-8 speakers where there are 6, and the disagreement between whole-file and windowed runs rose from 17 % to 36 %. The shipped values (1.0 and 0.1) were restored; a four-combination measurement (threshold × shift) on the tuning set, a held-out set, three long AMI meetings and the meeting is running.

### P6. Waiting time

A one-hour recording takes minutes after Stop (Turbo about 20× real time on an M4 Pro, much slower on Intel Macs). Live transcription while recording is built but gated on a 20-minute hardware check. The targets are short audio (about 15 s) and medium audio (about 1 hour); 3-5 hours is out of scope for now.

### P7. Vietnamese-specific model options

PhoWhisper (VinAI, trained on 844 hours of Vietnamese) small, medium and large are listed in the model catalogue but not downloadable (the ggml conversions are not published; the small one is converted and waiting). The decoding of PhoWhisper through whisper.cpp comes last. No numbers on the meeting yet.

## 5. What we have tried (and the result)

| Attempt | Result |
|---|---|
| Whisper's stricter thresholds (temperature fallback off, log-prob -0.5, no-speech 0.3, entropy 2.0) on quiet gated Vietnamese | Only log-prob -0.5 changed anything (-2 WER points, one run per cell: within noise). No decode change. |
| Large v3 vs Turbo vs Medium on FLEURS | Turbo best or equal, much faster. |
| Piece cutting at 300 s | Removes the single-loop-poisons-everything failure. |
| Coverage re-read of speech with no words | Recovers many dropped words; gated by confidence 0.4. |
| Repeated-sentence structural rule | Fixes most spread-out repeats; one 18-word run remained in the agent's run. |
| Retuned speaker clustering | Better on English, worse on the Vietnamese meeting; reverted. |
| Vietnamese Zipformer (sherpa-onnx) | Accurate on read speech but licence blocks it. |

## 6. Questions for the reviewer

1. **Hallucination detection without phrase lists.** What per-segment or per-token signals (no-speech probability, avg log-prob, compression ratio, token entropy, timestamps pattern, VAD ratio, level vs speaker norm) best separate hallucinated text from quiet real speech in Whisper-large-v3-turbo output? Is there a known, general, open-source technique (for example VAD pre-segmentation so Whisper never sees non-speech, `condition_on_previous_text=false`, per-segment re-decode with temperature fallback, compression-ratio checks) that we should adopt, and what are its costs in whisper.cpp?
2. **Should we run VAD first and decode only speech regions?** (Silero-style VAD, or the pyannote segmentation we already link for speakers.) What are the risks for far-field Vietnamese (clipped word starts, lost quiet words), and how should regions be padded and merged?
3. **Code-switching.** For Vietnamese with English technical terms, is there a better approach than one fixed language (per-segment language detection, a glossary prompt, a multilingual model setting)?
4. **Far-field audio.** Is noise reduction before Whisper helping or hurting here? Which front-end (gain normalisation, dereverberation, a different denoiser, loudness-adaptive chunking) is worth testing, and how to evaluate it without a public far-field Vietnamese set?
5. **Evaluation without private data.** How do we build a public, reproducible test for P1 and P2 (synthetic far-field mixes of FLEURS with room noise, reverb and overlap; are there public Vietnamese meeting corpora under a usable licence)?
6. **PhoWhisper through whisper.cpp.** Known pitfalls in converting the Hugging Face weights to ggml (tokenizer, positional embeddings, timestamp tokens), and whether it is likely to beat Turbo on far-field meetings.
7. **Speed.** Beam vs greedy, flash attention, quantised models, batching of pieces, running pieces in parallel on Apple Silicon: what is the best speed gain per accuracy lost?
8. **Anything in the pipeline above that is wrong or missing**, such as a known Whisper failure mode we do not guard against.

## 7. Where to look

- Code: `crates/cm-ai/src/whisper.rs` (decode params), `pieces.rs` and `pieces/checks.rs` (pieces, loops, repair, gap re-read), `failure_kinds.rs` (counters), `vad.rs` (speech regions), `sounds.rs` (`[unclear]`/`[noise]`), `speaker_engine.rs` (speakers).
- Reports (`verification/benchmarks/`): `whisper-safeguards-2026-11-03.md`, `repeated-sentences-2026-11-03.md`, `coverage-reread-2026-11-03.md`, `broken-tokens-2026-11-03.md`, `vietnamese-models-2026-11-03.md`, `vietnamese-fleurs-baseline-2026-11-03.md`, `level-aware-transcription-2026-11-03.md`, `short-clips-2026-11-03.md`, `speaker-retune-2026-11-03.md`, `transcription-baseline-en-synthetic-2026-11-03.md`.
- Plan: `specs/plans/release-0.2.7-plan.md` (M2, M2d to M2j).
