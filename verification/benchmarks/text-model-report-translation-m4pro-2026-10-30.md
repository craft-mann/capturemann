# Reports and translation with the built-in text model, 11 minutes to 2.5 hours (PERF-1)

Date: Oct 30, 2026. Status: **measured** (one run per cell, one machine). Not "verified": no manual script is
involved, and nothing here is a promise for other recordings, models or Macs.

## 1. Question and short answer

*How long do the two text-model actions (a meeting report, and a translation of the captions) take on a
long recording, how much memory does the helper program use, and where does it break?*

- **Reports scale about linearly with length and finish.** `summary` took 9 s for 11 minutes, 67 s for 1 hour
  and 178 s for 2.5 hours. `minutes` (a longer, structured answer) took 22 s, 128 s and 362 s. No run
  dropped an item or failed (0 items dropped for missing a citation, 6 of 6 runs finished).
- **Memory of `llama-server` rises with length**, from 3.2 GB (11 min) to 5.0 GB (2.5 h), because the
  model gets one prompt per piece and the reports are written in 1, 4 and 9 pieces.
- **Translation works on a real 88-minute recording** (Vietnamese to English with the text model): 1,271
  lines in 691 s (7.9 s of work per minute of audio), `llama-server` peak 6.5 GB, 5 lines left untranslated.
  Whisper's own translate task (Large v3) took 719 s for the same recording.
- **It failed on two stand-in transcripts** (the 11-minute text repeated to 1 h and 2.5 h, English to
  Vietnamese): `output_too_long` at 79 % and 31 %, no partial result kept. One reply ran to the 4,096-token
  output limit after a 562-token prompt, which looks like the model repeating itself. Because the real
  recording of 88 minutes did not fail, the repeated text is the likely cause; it is not proven, and the
  handling gap (reports split on this error, translation does not) remains.

## 2. Setup

| Item | Value |
|---|---|
| Machine | Apple M4 Pro, 14 cores (10 performance), 48 GB, macOS 26.6.2, on AC power |
| Build | commit `50f5eed`; `cm-daemon` release build; `llama-server` from the dev bundle (`0.7.0-dev`, llama.cpp, Metal, Apple clang 21) |
| Model | Gemma 4 E2B Instruct, Q4_0, 2.84 GB (the app's default text model), context 16,384 tokens, one slot (`--parallel`), all layers on the GPU |
| Daemon | a second, isolated `cm-daemon` (own library, config and socket); the maintainer's library and app were not used |
| Other load | the app and a virtual machine were left as they were; no other benchmark ran at the same time |

How a cell was measured:

- **Time**: wall clock around `capturemann ai report <id> --template <t> --wait` and
  `capturemann ai translate <id> --to vi --engine text-model --wait`. It includes queueing, the server
  start (the server stays loaded between runs, so only the first run paid for it), the model's work, the
  citation check and writing the result.
- **Memory**: the largest sum of resident memory of all `llama-server` processes, sampled once a second
  (`ps`). It excludes the daemon and can miss a peak shorter than one second.
- **Pieces** (`chunks`): the count the report job returns.
- **Speeds** (section 4.3) are the totals the server itself logged over all 844 calls of the session.

Repeat it (all on a scratch library; nothing is downloaded):

```sh
# 1. a library with one transcribed recording, an isolated daemon and the model symlinked in
CAPTUREMANN_LIBRARY=<lib> CAPTUREMANN_CONFIG_DIR=<cfg> CAPTUREMANN_SOCKET=/tmp/cmb/s.sock \
  CM_LLAMA_SERVER=<llama-server> target/release/cm-daemon &
# 2. longer stand-ins: python3 -I tests/tools/text_bench_tile.py <lib> <bundle.cmrec> 60 150
#    then rebuild the index (JSON-RPC method rescan_library on the socket)
# 3. for each id and template, time:  capturemann --socket /tmp/cmb/s.sock ai report <id> --template minutes --wait
# 4. translation: capturemann --socket ... ai translate <id> --to vi --engine text-model --wait
```

## 3. Data

- **Source recording**: a private 11.3-minute English meeting with two speakers, 1,581 words, 202 caption
  lines (transcribed by Whisper large-v3-turbo). The recording and its text stay with its owner; only
  numbers are published.
- **Long inputs are a stand-in.** `tests/tools/text_bench_tile.py` repeats that transcript (and its speaker
  turns) end to end, shifting the times, until it covers 60 and 150 minutes (8,465 and 21,099 words). This
  gives realistic length, piece counts and prompt sizes, but **not** realistic content: the same sentences
  recur every 11 minutes. It cannot say anything about summary quality, and it may change how the model
  behaves (section 5).
- **The real long recording** (section 4.2b) is a private 88.0-minute Vietnamese meeting with several
  speakers, transcribed on the same day with Whisper large-v3-turbo (language `vi`, 14,381 words). Its text
  went through the local model only; none of it is stored here.

## 4. Results

### 4.1 Report generation (`generate_report`), English transcript, default text model

| Input | Pieces | `summary` s | `minutes` s | `llama-server` peak, `summary` | peak, `minutes` |
|---|---|---|---|---|---|
| 11.3 min (1,581 words) | 1 | 9.4 | 21.7 | 3.19 GB | 3.28 GB |
| 60 min (8,465 words, stand-in) | 4 | 66.6 | 128.4 | 3.60 GB | 3.99 GB |
| 150 min (21,099 words, stand-in) | 9 | 178.2 | 361.6 | 4.76 GB | 5.00 GB |

Items dropped for a missing citation: 0 in every run. Names reassigned: 0. All six jobs ended `done`.

Derived, in seconds of work per minute of audio: `summary` 0.83 (11 min), 1.11 (60 min), 1.19 (2.5 h);
`minutes` 1.92, 2.14, 2.41. One hour of audio therefore waits about 1 to 2 minutes, and the cost per minute
grows slowly with length because each added piece also adds a merge step.

### 4.2 Translation to Vietnamese (`translate_captions`, text model)

| Input | Result | Time | `llama-server` peak | Progress when it stopped |
|---|---|---|---|---|
| 11.3 min, 202 lines | done, 0 lines untranslated | 72.2 s | 4.59 GB | 100 % |
| 60 min (stand-in) | **failed**, `output_too_long` | 431.5 s | 6.00 GB | 79 % |
| 150 min (stand-in) | **failed**, `output_too_long` | 421.9 s | 6.08 GB | 31 % |

### 4.2b Translation of the real 88-minute recording (Vietnamese to English)

| Step | Result | Time | Notes |
|---|---|---|---|
| Transcribe (Whisper large-v3-turbo, `vi`) | 14,381 words | 346 s | 15.3x real time; the earlier benchmark run of the same audio gave 14,360 words in 370 s |
| Translate with the text model (Gemma 4 E2B) | done, 1,271 lines, 5 untranslated | 691 s | `llama-server` peak 6.54 GB; 7.9 s per minute of audio |
| Translate with Whisper's translate task (Large v3) | done, 4,052 lines, 0 untranslated | 719 s | 7.3x real time; lines are Whisper's own segments, so the count differs |

The two engines write the same file, so the second run replaced the first; quality was not compared.

For the 11-minute run the rate is 6.4 s of work per minute of audio: about 3 times `minutes` and 8 times
`summary` of the same recording, because the whole text is written out again in another language.

### 4.3 Server speed (all calls of the session)

| Measure | Value |
|---|---|
| Prompt processing | 318,485 tokens in 307 s, about 1,040 tokens/s (median call 1,090 tokens/s) |
| Generation | 117,088 tokens in 1,391 s, about 84 tokens/s (80.8 tokens/s in the failing reply) |

Generation dominates: a report spends most of its time writing, not reading. That is why `minutes` costs
about twice `summary` for the same input.

## 5. What this does not show

- **Real recording: one language pair and one recording.** Vietnamese to English with the text model
  worked; English to Vietnamese on a real long transcript was not run.
- **One run per cell, one machine.** The first plan asked for the median of 3 with a spread; these cells
  have no spread, so differences under about 10 % mean nothing. Re-run before using any single number.
- **Stand-in long inputs.** Repeated text can make a model repeat itself, so the two translation failures
  probably come from the input. They were not repeated, and no real long transcript failed, so "translation
  fails on long recordings" is **not** shown; what is shown is that one reply can reach the output limit and
  that the job then fails without keeping the finished batches.
- **One model and one language pair** (Gemma 4 E2B, English to Vietnamese). The larger text models in the
  catalogue (Qwen3.5 4B) were not measured; they will be slower and use more memory.
- Memory is sampled once a second and covers `llama-server` only. The daemon, the app and Whisper add to it
  when they run at the same time.
- Quality was not judged; only time, memory and whether the job finished.
- Not measured: Intel Macs, battery power, a Mac with 8 or 16 GB (the 6 GB peak above may not fit next to
  other apps on a 16 GB Mac).

## 6. Decision and follow-up

- Reports need no performance fix: 1 hour waits about 1 to 2 minutes, and memory stays under the 5 GB the
  model entry in `text_models.json` should be checked against (`ram_mb` is an estimate; these runs are
  the first measurement of it: 3.2 to 5.0 GB for reports, up to 6.1 GB for translation).
- **Translation robustness stays open but is lower priority.** The real 88-minute recording translated, so
  the failure needs a repeating input to show up so far. Still worth doing, cheaply: split a batch when a
  reply reaches the output limit (as reports do), keep finished batches, and add a counted-work test with a
  scripted model that returns an over-long reply.
- Translation to Vietnamese of a real long English transcript, and a second run of the 88-minute
  translation, would show how stable the result is.
- Repeat the table three times per cell, and add the 8 GB and 16 GB Macs, before using it as a budget.
