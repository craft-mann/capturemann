# AI-5 manual script: translation (Whisper to English with large-v3, and text-model translation)

Covers what the scripted tests cannot: Whisper's translate task on a real Vietnamese recording
with the real `large-v3` model, the quality of the English lines, the turbo rule on a real model,
and (when the text-model engine is wired) translation to Vietnamese with a local model. The line
rules, the re-wrap and split, the engine and model choice (including the turbo rule) and the
stored translation are covered by `cargo test -p cm-ai translate` and `cargo test -p cm-daemon
--test ai_translate`. Spec: `specs/features/07-ai.md`, AI-5 (AC1 to AC5). **Status when this script
was written:** the Whisper engine is built and tested with fakes only; no real `large-v3` run has
happened. Steps marked **[text model]** need the text-model translator wired to the AI-7 provider
(check `capturemann ai translate --help` and the spec's "AI-5 as built": until then they fail with
`no_text_model`), a running Ollama or LM Studio (see `AI-7.md`) and a model; the default model is
undecided (spike S8), so use the largest your Mac runs and write down which.

**Privacy:** nothing is recorded by this script beyond the test clip. Use a clip you own; the audio
stays on this Mac. The model download in step 1 is the only network use (it happens only when you
press Download); turn the network off before step 2 (Wi-Fi off) to prove the rest is offline.
Delete the test library afterwards.

## Setup

1. Own instance: `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET` pointing at
   scratch folders (never your library). Use the dev bundle if you test the app.
2. A **2-minute Vietnamese recording** with clear speech (you reading a news article aloud, or a
   clip you have the rights to) and a **hand-made English reference** of what was said, written
   before you run the translation. Record it with the Audio mode (⌥⇧V, microphone) or add it as a
   `.cmrec`; the `make_fixture` tool has no speech and cannot be used.
3. A 2-minute **English** recording for the text-model steps.
4. Models: `capturemann ai models` should list `large-v3` (about 3.1 GB, "Best accuracy and
   translation to English") and `large-v3-turbo` (about 1.6 GB, "Recommended: fast and accurate, not for
   translation"). Download them from the AI page (press Download; size and checksum are shown
   first), or `capturemann ai models download large-v3 --wait` and the same for `large-v3-turbo`,
   or import files you already have with `capturemann ai models import <id> <file>`.

## 1. Models and labels

- Open the AI page. Expected: `large-v3` is labelled for translation to English and
  `large-v3-turbo` says it is not for translation; neither downloads without your press.
- Pass when both are listed with size and licence before any download, and the file you downloaded
  matches the catalogue checksum (a mismatch must refuse the file). If the `large-v3-turbo`
  entry still says its checksum is to be pinned, report that as a defect.

## 2. Vietnamese to English with Whisper (AC1), network off

1. `capturemann ai models use large-v3`. Transcribe: `capturemann ai transcribe ID --language vi
   --wait`.
2. Turn the network off. `capturemann ai translate ID --to en --engine whisper --wait`.
3. Expected: the job finishes; `translations/en.json` is in the bundle with `engine` `whisper` and
   `model` `large-v3`; `capturemann ai captions get ID --language en` prints the lines.
4. Check the lines against the AI-1 rules: at most 2 rows of 42 characters, between 1 and 7
   seconds, none empty, times increasing and inside the recording.
5. Review: read every English line against your reference and mark each **conveys the meaning**
   (yes or no). Names and numbers count.
- Pass (AC1) when `en.json` exists, all lines meet the rules and **at least 90% of lines** convey
  the source meaning. Write the count (for example 41 of 44) and the job time down. A miss is a
  finding about Whisper's quality, not a code failure: report it with the lines.

## 3. The turbo rule (AC2)

1. `capturemann ai models use large-v3-turbo`, then `capturemann ai translate ID --to en --engine
   whisper --wait` again.
2. Expected: the result names `large-v3` as the model (not turbo) and the lines are English.
3. Remove `large-v3` (`ai models rm large-v3`, confirm in the app), install `medium`, run again:
   the result names `medium`. Remove `medium` too and run again: the call fails with
   `translation_model_missing` and **nothing is downloaded** (watch a network monitor).
- Pass when turbo is never used for translation and the neither-case downloads nothing. Put
  `large-v3` back afterwards.

## 4. Edits survive a re-translate (AC5)

1. In the caption editor choose English in the language switcher, edit one line, Save.
2. Edit an unrelated line of the original captions and press Translate again.
- Pass when the edited English line is still your text (the result counts it in `kept_edits`;
  `capturemann ai translate ID --to en --wait --json`).

## 5. English to Vietnamese with a text model (AC3, AC4) **[text model]**

1. Start Ollama or LM Studio and set the text model on the AI page (Test passes, `AI-7.md`).
   Add the glossary term `CaptureMann` and make sure the English recording says it at least twice.
2. `capturemann ai translate ID2 --to vi --engine text-model --wait` (or Translate... in the
   Transcript tab, Vietnamese).
3. With a network monitor on: traffic only to `127.0.0.1`.
4. Compare `captions get ID2 --language vi` with `captions get ID2`: each translated line keeps
   its source line's start and end (or a split of them); the printed `untranslated` count is 0
   (a flagged line keeps the English text).
5. Look for "CaptureMann" in the Vietnamese lines: unchanged wherever the source has it.
- Pass when the times match, only loopback traffic is seen and the term is unchanged. Rate the
  Vietnamese yourself (natural, correct, names kept) and write the percentage down; it feeds S8.
- With provider None the call fails with `no_text_model` and the app says where to set one up.

## 6. Errors

- `ai translate ID --to vi --engine whisper`: refused (Whisper translates to English only).
- `--to vi` on the Vietnamese recording and `--to en` on an English one: refused with
  `same_language`.
- A recording with no transcript: `not_found` with the way to transcribe.

## 7. Exports per language

- `capturemann export ID --captions srt --language en` (or the footer buttons with English chosen)
  writes `<title> - captions.en.srt`; `vtt` and `txt` likewise (`<title>.en.txt`). Open them:
  English text, the edit list applied.

## 8. In the app

- Open the recording, Transcript tab: the language switcher lists the original and English
  (translated); Translate... starts the job, a chip shows while it runs and a toast says it is
  ready. Check English, Vietnamese and `CAPTUREMANN_PSEUDO_LOCALE=1` for clipped labels.

## Not covered here

AC6 (YouTube caption tracks, PUB-2) and AC7 (Confirm with a remote provider) are not built; text
models are local only (maintainer decision, Oct 26, 2026).

## Report

Store the run in `verification/` (template in its README): the Mac, the models and checksums, the
counts from steps 2 and 5, and any line you rated wrong. Say "verified on hardware" only with that
report. Until then AI-5 is "tested with fakes".
