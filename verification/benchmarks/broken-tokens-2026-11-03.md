# Broken words: U+FFFD, foreign script, near-zero confidence (0.2.7, M2d item 3)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro, 48 GB, GPU on. Not "verified".
- **Build:** branch `track/mac-ai`; "before" is the release `asr_bench` built before the change, "after" the one with it.
- **Question:** does a general rule keep U+FFFD, foreign-script words on low confidence and hopeless stretches out of the transcript, without changing the rest?

## 1. The change

- `Job::repair` (`pieces.rs`), after each piece: the *suspect* words are those with nothing left but stray characters (U+FFFD, controls, zero-width, private use), a letter of a script the language does not use on confidence under 0.2 (`failure_kinds::is_broken`), and runs of three or more words under 0.1 confidence. A window of 1 s around each suspect stretch is read again on its own (no prompt); the new reading replaces the old when it has nothing broken and is not less certain (or is empty where the old words were all hopeless). What is still broken is cleaned (stray characters removed from a word) or dropped. Counters: `broken_reread`, `broken_reread_kept`, `broken_dropped` (in `PiecesReport` and the checkpoint).
- `caption::build_captions` removes stray characters from words and skips words that are nothing else, so an **existing** transcript (written before this change) no longer shows U+FFFD in its captions. Foreign-script words already stored stay until the transcript is made again (the captions do not know the language).
- General rules on characters and confidence; no word lists.

## 2. Results

PhoWhisper-small run with the app's settings on the three FLEURS clips is the only source of broken words in the public sets (Turbo has none, and 12 s of room noise at -55 or -70 dBFS before the speech produced no text at all, with identical WER before and after: 13.2 % and 18.6 % on `vi_a` / `vi_b`).

| Clip | Version | WER | Words | U+FFFD | Foreign script | Words under 5 % | Broken and doubtful | Whisper calls | Stretches read again (kept) | Words dropped or cleaned |
|---|---|---|---|---|---|---|---|---|---|---|
| vi_a | before | 60.7 % | 341 | 1 | 0 | 17 | 0 | 3 | | |
| vi_a | after | 60.7 % | 338 | 0 | 0 | 13 | 0 | 4 | 1 (1) | 1 |
| vi_b | before | 59.2 % | 394 | 1 | 2 | 17 | 2 | 1 | | |
| vi_b | after | 58.1 % | 383 | 0 | 0 | 12 | 0 | 4 | 3 (3) | 1 |
| vi_c | before | 62.9 % | 362 | 0 | 1 | 19 | 0 | 1 | | |
| vi_c | after | 62.4 % | 359 | 0 | 1 | 13 | 0 | 3 | 2 (2) | 0 |

U+FFFD 2 to 0; broken-and-doubtful 2 to 0; the one remaining foreign-script word (vi_c) has a confidence of 0.2 or more and stays by design. WER did not get worse (60.7 / 59.2 / 62.9 to 60.7 / 58.1 / 62.4). The cost is 1 to 3 extra Whisper calls on short windows.

Unit tests (`failure_kinds/tests.rs`): the former ignored test `broken_tokens_are_never_shown_as_text`, `a_broken_stretch_is_read_again_and_a_clean_reading_replaces_it`, `captions_never_show_stray_characters`, and the character-class tests.

**The maintainer's Nov 2 meeting (counts only, nothing else read):** its stored transcript has 3 words with U+FFFD, 5 foreign-script words and 102 words under 5 % confidence, 6 of them both broken and under 20 %. With this build the 3 U+FFFD no longer reach the captions; the 5 foreign-script words and the rest of the weak-signal text (for instance Latin letters with diacritics from another language, which no script rule can tell apart) go only if the transcript is made again, and the rerun is not measured on that recording.

## 3. What this does not show

- Turbo produced no broken words on any public clip, so the rule is exercised on PhoWhisper's output and on constructed word lists, not on the weak-signal start of the private file. Whether the re-read replaces the garbage there is unmeasured.
- Low-confidence real words in a normal sentence are left to the doubtful underline; only hopeless runs (three words under 10 %) are re-read, and they are replaced only when the second reading is not less certain.
- One run per cell, three clips.
