# Transcription review: the option stack against every set (0.2.7)

- **Date:** 2026-11-03. **Status:** measured, Apple M4 Pro, Large v3 Turbo, Vietnamese for the Vietnamese sets, the made-up prompt `Zed Nim, Blorp, Qux` on except where a row says "no prompt". The runs of a cell were identical on this machine in every case that was repeated three times (steps 1 to 3, see `review-steps-1-3-2026-11-03.md`); the later rows are one run each. Not "verified".
- **New row, private:** `meeting-5min`: the first 300 s of the maintainer's corrected reference meeting (`Meeting_with_PAT_TLSG`, 1,000 words, 2 speaker labels; the audio is the imported track, `mic.wav` and `original.m4a` decode to the same samples, so the raw-against-processed row is **identical** (22.7 % both)). Only numbers appear here; the engine finds **2 speakers** in these 5 minutes, as many as the reference labels.
- **Rule used:** a stack is kept only if it improves the meeting WER and makes no public set worse. Insertions and deletions are counted apart; coverage and loops are in the tables of the earlier reports (loops were 0 in every cell of every run here).

## 1. Short answer
**Shipped by default: the quiet-input lift, and nothing else.** Audio whose RMS is under -26 dBFS is lifted toward -20 dBFS with at most +15 dB of gain, per decode call (a piece or a re-read); louder audio is left alone. It improves the meeting (no prompt 23.1 to 22.7 %, **with the prompt 25.6 to 22.4 %**), the 4-minute FLEURS clips (10.8 to 10.4 %) and the noise set (8.4 to 7.6 %), and changes no other set. Real-time factor is unchanged (4-minute FLEURS 25.7x against 25.5x; meeting 23.7x against 22.4x).

**Not shipped (each is a setting with an off switch, off by default):**
- **The hallucination gate** (segment no-speech probability, voice-activity share against energy, timing collapse, with the re-read passing the same gate): it removes the invented words of the noise set, but costs the real sets. With it on the meeting goes from 21.0 to 23.5 to 25.4 % and the 4-minute FLEURS from 10.7 to 11.3 to 12.0 %, whichever single signal is left on; the Silero opinion did not rescue it.
- **No carry-over of text (`n_max_text_ctx` 0)** with the lift: the best meeting row (**21.0 %**, deletions 92 down to 59) and the English row (16.0 to 6.2 %: it drops the prompt), but the noise set gets worse (12.0 %, 10 words over the silent parts) and only the gate could undo that, which fails above. It also silently turns the glossary prompt off for the decode.
- **Silero regions as the decode unit:** worse on the meeting (26.5 to 27.6 % against 23.1 %) and on 4-minute FLEURS with the v5 file (13.4 to 13.9 % against 10.8 %); the v6.2.0 file gave 9.7 % on FLEURS but 25.9 % on the meeting.
- **Flash attention** (77 invented words on the noise set), **the high-pass** (short FLEURS 12.1 to 12.9 %).

## 2. Table (WER % / words written outside the speech span / meeting insertions and deletions)

| Stack | meeting-5min, no prompt (ins / del) | meeting-5min, prompt | fleurs4min | fleurs_short | english | english_short | noise (9 clips) |
|---|---|---|---|---|---|---|---|
| baseline (0.2.6 decode) | 23.1 (31 / 92) | 25.6 (24 / 125) | 10.8 / 0 | 12.1 / 0 | 16.0 / 3 | 4.3 / 1 | 8.4 / 0 |
| **lift quiet input (shipped)** | **22.7** (56 / 42) | **22.4** (48 / 52) | **10.4** / 0 | 12.1 / 0 | 16.0 / 3 | 4.3 / 1 | **7.6** / 0 |
| lift + high-pass | 22.7 (35 / 71) | 24.5 (48 / 67) | 10.4 / 0 | 12.9 / 0 | 7.1 / 3 | 4.3 / 1 | 7.6 / 0 |
| lift + `n_max_text_ctx` 0 | **21.0** (32 / 59) | 21.0 (32 / 59) | 10.7 / 0 | 11.4 / 0 | 6.2 / 2 | 4.3 / 1 | 12.0 / **10** |
| lift + gate, no voice model | 25.0 (33 / 80) | not run | 12.7 / 0 | 12.1 / 0 | 7.2 / 1 | 4.3 / 1 | 7.6 / 0 |
| lift + `n_max_text_ctx` 0 + gate | 25.4 | 25.4 | 11.3 / 0 | 11.4 / 0 | 6.2 / 4 | 4.3 / 1 | 7.6 / 0 |
| same + voice model opinion | 25.4 | 25.4 | 12.0 / 0 | 11.4 / 0 | 6.2 / 4 | 4.3 / 1 | 7.6 / 0 |
| `n_max_text_ctx` 0 + one gate signal only (voice / no-speech / no-speech at 0.95) | 23.5 | 23.5 | 11.3 to 12.0 / 0 | 11.4 / 0 | 6.2 / 4 | 4.3 / 1 | 11.3 / 10 |
| `n_max_text_ctx` 0 + timing collapse only | 23.5 | 23.5 | 11.3 / 0 | 11.4 / 0 | 6.2 / 4 | 4.3 / 1 | **8.0** / 1 |
| Silero regions, v5.1.2, silence 150 ms, pad 450 ms | 26.5 | 26.1 | 13.9 / 0 | 12.0 / 0 | 6.3 / 2 | 4.3 / 0 | 9.0 / 0 |
| Silero regions, v5.1.2, silence 200 ms, pad 500 ms, threshold 0.25 | 27.6 | 27.2 | 13.4 / 0 | 13.8 / 0 | 6.3 / 2 | 4.3 / 0 | 8.6 / 0 |
| Silero regions, v6.2.0, silence 150 ms, pad 450 ms | 25.9 | 28.6 | **9.7** / 0 | 12.0 / 0 | 6.6 / 0 | 4.3 / 0 | 8.6 / 0 |
| flash attention | not run | not run | 11.5 / 0 | 12.1 / 0 | 9.0 / 7 | 4.3 / 1 | 14.6 / **77** |

"Lift" is the shipped step (below -26 dBFS RMS, target -20, at most +15 dB), except in the baseline row. The English rows with the prompt show the prompt's own harm (16.0 against 6.2 % without it). Speaker count of the meeting-5min: 2 found, 2 in the reference.

## 3. Wired and tested
- `ai.lift_quiet_input` (default on), `ai.hallucination_gate` (off), `ai.vad_opinion` (off), `ai.carry_previous_text` (on = as before); `PipelineSettings` in `cm_ai::pipeline`, applied by the transcribe job and the live follower (a live state made with other options is not reused), `Configured` wrapper around any decoder. Unit tests (made-up tokens only): quiet audio lifted with the cap and loud audio left alone; each switch off gives the old behaviour; the gate marks a no-speech segment, reads it again and replaces it, leaves a believed segment, turns a second failing reading into a sound; energy that the voice model does not hear marks a long segment but quiet speech with voice-model agreement stays; collapsed timing is marked.
- The voice-activity model is a pinned catalogue entry, `crates/cm-ai/data/vad_models.json`, not bundled: `ggml-org/whisper-vad` commit `9ffd54a1e1ee413ddf265af9913beaf518d1639b`, `ggml-silero-v5.1.2.bin`, 885,098 bytes, SHA-256 `29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf`, MIT. (`ggml-silero-v6.2.0.bin`, same size, SHA-256 `2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`.) `silero::vad_store(root)` downloads and checks it like the other models, only on request.

## 4. What this does not show
One private 5-minute clip (a different meeting from the one with the repeated sentences and the 1:34 outro, neither of which reappears in new decodes); the 1-hour row was not re-run for the stack (the lift is one gain pass per call; the gate stays off); runs were identical where repeated, one run each in the last rounds; the gate was tried as designed, not tuned: a looser version of the no-speech and voice-model rules, or a gate applied only to segments under a minimum duration, is the next thing to try, judged on the same table.
