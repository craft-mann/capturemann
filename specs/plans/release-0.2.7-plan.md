# Release 0.2.7 plan: macOS stabilisation and faster, more accurate AI

Status: **released Nov 4, 2026** (tag `v0.2.7` on `6e992cd`; entry in `docs/project/status.md`, notes in `CHANGELOG.md`). What was built, measured and left out is in `docs/research/transcription-quality-study-0.2.7.md` and `specs/plans/transcription-quality-plan-0.2.7.md`; what moves on is in `specs/plans/release-0.2.8-plan.md` ("Transcription track"). **Outcome against this plan:** M1 (logs and window lifetime), M2 (stored reports for every claim), M2d to M2j (broken tokens, repeats, skipped speech, `[unclear]`/`[noise]`, word times; PhoWhisper is **not** shipped: the ggml conversion exists but is not uploaded or pinned), M2h (transcription while recording: built, **off by default**, the 20-minute hardware check was not run) and M3 (Import) shipped; the invented outro-style text, the raw-microphone default and the invented-text gate did **not** ship (measured, see the study). Original text of the plan (drafted Nov 3, 2026, right after 0.2.6) follows. Scope set by the maintainer on Nov 3, 2026:

- **0.2.7 is macOS only:** stabilisation, plus faster and more accurate **transcription**, **speaker detection** and
  **report writing**.
- **Windows catch-up moves to 0.2.8, Linux catch-up to 0.2.9** (kept below under "Later releases"). The 0.2.7 release
  does not change the Windows and Linux early builds; they stay at their 0.2.6 state and the release text says so.
- **Import media (Nov 3, 2026, maintainer): videos and audio files imported into the library ship in 0.2.7** as a third
  track, M3 below (design boards I0 to I4 in `capturemann-redesign 6/`, text in `import-media-design.md`).
- The work is split into three tracks that run **at the same time** in separate git worktrees, each driven by its own
  agent (see "How the agents work").

Rules that apply to every track (from `CLAUDE.md` and the maintainer): general techniques only, no phrase lists or
one-recording fixes (open source); **nothing is called "verified", "accurate" or "faster" without a stored report in
`verification/`** (benchmark reports in `verification/benchmarks/`); no GPL/AGPL in shipped binaries; `target_os` only
in platform code (`python3 scripts/check_target_os.py`); every user-facing action has an MCP tool and a CLI command;
UI text through `cm_i18n`; one change per commit with the two trailer lines; no failing tests merge to `main`; the
maintainer pushes `main`, signs, publishes and notarizes.

## Where 0.2.6 left things (inputs to this plan; the Linux and Windows rows feed 0.2.8 and 0.2.9)

| Area | State at 0.2.6 | Source |
| --- | --- | --- |
| macOS Intel | Issue 21: Record panel, webcam bubble and (before 0.2.6) the export toast stay on top on an Intel Mac; only the toast wording is fixed, the stuck windows are unexplained, the reporter was asked to retest 0.2.6 and send the log | issue 21 |
| Transcription | Whisper large-v3-turbo on CPU/GPU; long recordings are cut into pieces, looping pieces are split and re-run (29 Whisper calls for 17 pieces on 88 min, cost unmeasured); Vietnamese is not measured | `docs/project/performance.md` |
| Speaker detection | 17 % diarization error on held-back English clips; Vietnamese not measured; an unknown speaker count can run the engine twice (88 min: 265 s plus 263 s); memory grows with length (0.6 GB at 10 min, 3.5 GB at 150) and Cancel is ignored until the run ends; `window_shift_ratio` 0.1 is the main cost | `docs/project/performance.md`, `verification/benchmarks/speaker-detection-2026-10-29.md` |
| Reports and translation | Built-in text model (Gemma 4 E2B); an 88 min Vietnamese-to-English translation took 691 s and 6.5 GB; repeated-text stand-ins failed with `output_too_long` and lost their partial result; report accuracy never measured | `docs/project/performance.md` |
| Linux | `.deb`/tarball early build, no recording (code exists behind `CM_LINUX_RECORDING=1`, never seen on a real desktop); Wayland window capture and global shortcuts have no path on GNOME 46; aarch64 only in a VM | `docs/development/linux-early-build.md`, `verification/LINUX-1-*` |
| Windows | early build, unsigned, no recording, no text recognizer, no speaker engine, no bundled FFmpeg; picker app names, 1 px size difference, "Unknown Monitor 65537"; SmartScreen and Windows 10 never run | `verification/L1-windows-thinkpad-2026-11-02-claude.md`, `specs/plans/release-0.3-0.4-platforms-plan.md` |

## Track M1: macOS stabilisation (agent `mac-stability`)

Goal: nothing a user can see gets stuck, and failures leave a report.

1. **Issue 21 (Intel).** Reproduce in the pieces we can: the Record panel is closed by the code on Start, the setup
   bubble on Start, the toast on dismiss. Add the missing evidence first: log lines at each close and each window
   creation/destruction (`cm_log`), a "Copy diagnostics" check that lists the live app windows, and a watchdog test
   with a scripted window host that fails to close. Then fix what the logs show. Do not guess a fix without a log.
   Wait for the reporter's 0.2.6 log; ask on the issue if none arrives.
2. **Window lifetime audit.** Every floating surface (panel, bubble, toast, overlay, launcher) gets one owner, one close
   path and a test that a failed close is retried or reported. Applies to Windows and Linux hosts too (shared code in
   `apps/desktop`).
3. **Long-session health.** Changed Nov 3, 2026 (maintainer): the 3-hour soak is **not** a 0.2.7 gate and is not
   scheduled; first the product must handle **short audio (about 15 s)** and **medium audio (1 hour and more)**
   well. The soak plan (`tests/manual/M1-SOAK.md`, `scripts/soak/sample.sh`) stays for later; for 0.2.7 the memory and
   file-handle sampler is run on a 1 hour recording instead, when the maintainer gives a slot.
4. **Intel confirmation.** Ask for one more Intel report or borrow a machine; until then the Intel DMG stays
   "built on Apple Silicon, checked under Rosetta" in every public text.
5. **Errors in the app log.** `desktop.log` shows `RefCell already borrowed` (3 to 47 per session, always in a burst of
   3) followed by `window not found`, nine times between Oct 30 and Nov 2, and once `timed out waiting on app_will_quit`
   (Nov 2). The pattern points at a re-entrant borrow while a window is closing (the same family as issue 21's windows).
   Find which action triggers it (log lines at each window close, see task 1), make the close path not borrow twice,
   and add a test with a scripted window host. The log lines carry no module name, so give them one.
6. Items from the 0.2.6 review that touch macOS only: none other known. Add any found in M1.

Done when: a stored soak report, window-lifetime tests in CI, issue 21 either fixed with a log-backed cause or still
open with the evidence listed.

## Track M2: macOS AI speed and accuracy (agent `mac-ai`)

Everything starts with a **benchmark that exists before the change**, stored under `verification/benchmarks/`, so
every claim is a before/after on the same data. Public wording: "measured on ...", never "accurate".

### Audio length targets (maintainer, Nov 3, 2026)
Every M2 benchmark and fix is checked at two lengths before anything longer: **short (about 15 s)** and **medium
(1 hour and more)**. Short clips: transcription, speaker labels, silence suggestions and reports behave sensibly (no
garbage first words, no "no speech" on a clip with speech, no pauses cut at the edges, a 15 s clip finishes in seconds,
speaker count not forced). Medium: the 1 hour and the 88 minute cases of `docs/project/performance.md`. 3 to 5 hours is
out of scope for 0.2.7. Each report lists its short and medium rows.

### M2a Transcription
- Build a word-error-rate (WER) harness over public clips with reference text (English only, O1; no private recordings in the
  repository). Report WER and real-time factor per model.
- Measure the cost of the repeat re-runs (performance.md item 3) and cut it: detect looping with a general rule,
  re-run only the looping piece with a changed temperature or without the previous text as prompt, not a phrase list.
- Long-recording speed: overlap of pieces, GPU vs CPU split, model choice by length. Target set after the baseline.
- Vietnamese: not in 0.2.7 (O1); the release text lists it as not measured.

### M2b Speaker detection
- **Baseline for 3+ speakers** (today only English, 14 clips); English only (O1).
- Remove the second engine run for unknown speaker counts (performance.md item 1): options are a grouping pass over
  the voice prints, or a first-pass cap that does not need a rerun. Needs a change in the vendored sherpa-onnx
  wrapper (`vendor/sherpa-onnx*`, trimmed copies: keep the no-download build rule).
- Windowed runs to cap memory and make Cancel work (item 2).
- Retune `window_shift_ratio` and the clustering threshold on a larger set than 14 clips (item 4), reporting DER
  next to time for each setting.

### M2c Reports and translation
- Measure report **speed** (tokens per second on M-series and Intel) and **quality**: define a checkable rubric
  (every item links to a timestamp that exists; all speakers named in the transcript appear; no invented names or
  numbers, checked by string match against the transcript) and score a fixed set of transcripts. Report the
  scores; no "accuracy" claim without the report.
- Fix `output_too_long` for translation (performance.md item 5): keep partial work, split the piece and retry
  (the reports path already splits).
- Candidate speed-ups to measure, not assume: prompt/KV cache reuse across pieces, smaller context for short
  pieces. The model stays Gemma 4 E2B (O2); a different quant of it needs a pinned URL and SHA-256 in
  `crates/cm-ai/data/text_models.json` (no `pending_pin` entries ship) and the maintainer's approval.

### M2d Findings from a real Vietnamese meeting (Nov 2, 2026) and the fixes they point to
Input: the maintainer's 45 min meeting `EmeSoft Weekly Meeting Nov 2.cmrec` (6285 words, `vi`, large-v3-turbo, 2 to 6
speakers). Analysed by script on Nov 3, 2026. Findings are about **kinds** of failure; the recording itself is
private and never enters the repository or a test (a synthetic or public clip reproduces each kind). English-only
claims stay (O1), but these failure kinds are language independent, so they are in scope.

1. **Speech that got no words.** 29 gaps longer than 8 s add up to 546 s with no words; the speaker engine marks
   329 s of them as speech, and the gaps have normal signal level. Fix to measure: compare speech regions (VAD or the
   speaker engine) with word coverage; re-run uncovered speech regions on their own (a piece that starts at the
   region, no previous text as prompt); report coverage (seconds of speech with words / seconds of speech) in the
   benchmark. Baseline first.
2. **Repeated word with spread-out times.** One word said 7 times, 14 to 39 s apart, each on weak signal and
   high self-confidence, beside the gaps above. The loop detector (`pieces.rs`) only counts words in a row and needs
   8 of a single word, so it misses this. General rule to test: the same short text recurring while the gap before it
   is long and the signal under it is low (energy and speaker-engine agreement), and drop or re-decode such words.
   No phrase list.
3. **Broken characters.** 3 words with U+FFFD or stray symbols and 1 word with a Korean character, 102 words under
   5 % confidence. They sit at the start of the file (first 12 s) and in weak-signal stretches. `join_split_characters`
   already joins split bytes, so what remains is bytes that never form text. Fix to test: a token that decodes to
   U+FFFD, or a script that does not belong to the transcript's language set and has low confidence, is re-decoded
   (temperature fallback) or marked doubtful, never shown as text. Whisper runs greedy with one candidate and no
   fallback or no-speech settings today (`whisper.rs`): measure the fallback parameters (temperature steps,
   no-speech and log-probability thresholds, and a smaller or beam search) on the WER benchmark before changing them.
4. **Short repeated phrases (11 in the meeting)** look like real disfluency ("lý do lý do"). Keep; only the long
   or low-confidence ones are suspect. Do not collapse repeats shorter than the loop rule.
5. **Speakers:** the file has 6 named speakers though the Record sheet said 2 people; the speaker count was set
   afterwards by hand. M2b should report how the count was chosen next to the DER.

### M2e PhoWhisper as a speech model option
- The weights are Hugging Face transformers checkpoints (`vinai/PhoWhisper-{small,medium,large}`, BSD-3-Clause per
  the model card). whisper.cpp needs ggml files, and no whisper.cpp-compatible conversion exists (the one community
  repo is base-size, for another engine). Decision: the maintainer uploads converted files to the project's Hugging Face
  org (`https://huggingface.co/craftmann`, repo name e.g. `craftmann/phowhisper-ggml`, decided Nov 3, 2026); the app pins that commit URL, SHA-256 and byte size in `crates/cm-ai/data/models.json` like every model.
- Work: a reproducible conversion script (whisper.cpp `convert-h5-to-ggml.py` route; pinned tool versions; checks that
  `ModelStore::import` accepts the file and one whisper.cpp run transcribes a short public clip) under `packaging/`,
  a README for the upload, and three catalog entries marked `pending_pin` until the maintainer gives the URLs (they
  are neither offered nor downloadable until then; no `pending_pin` entry ships). NOTICE/licence text for the model.
  Labels and hints through `cm_i18n`; the model picker says it is tuned for Vietnamese.
- Measure first: Vietnamese WER and real-time factor of large-v3-turbo vs PhoWhisper small/medium/large on a public
  corpus, plus the coverage and hallucination counts of M2d, stored under `verification/benchmarks/`. Whether any
  PhoWhisper size becomes the suggested model for Vietnamese is decided from that report, not assumed.
- Check what depends on the Whisper family: language `vi` handling, DTW word times (alignment heads are not in a
  fine-tune's config for some conversions), the 300 s piece logic, and memory (ram_mb is measured, not guessed).

### M2f Suggest cuts runs by itself
- When the recording editor opens, `suggest_cuts` runs once per revision with silences and fillers both on (fillers
  need a transcript: without one only silences run and the panel says so). Nothing is applied; Accept stays one undo
  step. Setting under Settings to turn the automatic run off (default on); same `suggest_cuts` action, no new
  agent-only behaviour. Runs off the UI thread and is cancelled when the editor closes. Owner: `apps/desktop` editor
  files (assigned after `mac-stability`'s shared-code commits merge, to avoid conflicts).

### M2g Suggested cuts on a quiet meeting recording (seen Nov 3, 2026, maintainer's screenshot)
Opening the 45 min meeting as an audio recording, Suggest cuts showed "1.2 s pause at 0:04 (and 258 more)", with long
amber blocks over stretches that hold speech (the first 15 minutes are mostly amber). Read-only statistics over the
file (the recording stays private; the findings are the kinds):

- The suggestions run on `mic.denoised.wav`. The denoiser pushes quiet stretches to near digital silence, so the
  noise-floor estimate (10th percentile) is about -83 dBFS and the adaptive rule never raises the threshold: the fixed
  default of **-40 dBFS** applies. The recording is quiet (median level -39 dBFS on the denoised track, words around
  -50 dBFS, -40 dBFS on the raw track), so the default sits above a large share of the speech.
- A replica of the rule gives 171 pauses (464 s of 2,678 s), with 150 transcript words (2.4 %) and 91 s of
  speaker-engine speech inside them; the other suggestions of the 259 are filler cuts. Cutting those pauses would
  delete real speech. Because M2f now shows the list by itself, this is visible at once, so it must be fixed before 0.2.7.
- Same recording, same cause family as M2d item 1: the speech that got no words sits in quiet stretches where the
  denoiser lowered the level by 4 to 9 dB.

Work (general techniques, measured before and after, no thresholds tuned to this one file):
1. A level-independent rule: estimate the floor from the **raw** track (or from the level distribution, e.g. the gap
   between the noise mode and the speech mode), set the threshold relative to the speech level, not an absolute -40.
2. A veto from other evidence: a pause is not suggested where the speaker engine or the transcript says someone
   speaks (when they exist).
3. Measure level-aware transcription: peak or loudness normalization of quiet input before Whisper, and whether
   Whisper reads the denoised or the raw track (WER and coverage on synthetic quiet clips made by lowering gain and
   adding a gating denoiser; report both).
4. Test: a synthetic quiet-speech clip (speech at -45 dBFS RMS, room noise -60 dBFS) must not produce suggestions
   inside speech; `silence.rs` tests cover the denoised-floor case.

### M2h Transcribe while recording (audio recordings only; maintainer, Nov 3, 2026)
Goal: a user who stops a long audio recording must not wait minutes for the transcript. Today everything starts at stop
(an 88 minute meeting: about 6 min for the transcript, about 8 min with speakers beside it; `docs/project/performance.md`).
Idea: the recorder already writes `mic.wav` as it goes; transcribe each finished piece (about 5 min, cut at a pause, with
the existing `pieces` logic and its checkpoint) while the meeting is still running, so at stop only the last piece is left.

- **Scope:** `kind: audio` recordings only (a screen recording keeps the end-of-recording start: it already uses the GPU
  and the encoders). Setting `ai.transcribe_while_recording`, default on when `ai.auto_transcribe` is on (16 GB RAM or
  more) and a speech model is installed; off otherwise. Nothing starts without a model, no download.
- **Conflict to resolve first:** AI jobs are held while a recording runs (a started recording holds a running job, see the
  `Checkpoint` comment in `pieces.rs`); that rule protects the recording. M2h must lift it only for this one job, with a
  lower priority: Whisper threads and GPU use limited so the recording's audio path is never late.
- **Rules:** a piece starts only when its end plus the pause-search lookahead (20 s) is on disk; pause (`PauseGap`) and
  mic mute are the same as in the batch path (the media files are compacted at a pause); Discard or restart deletes the
  partial transcript; a crash leaves the checkpoint and `recording.lock` recovery continues from the last finished piece;
  Stop finishes the remaining piece(s), joins the words and runs the same post steps as today (captions, auto speakers,
  report suggestion).
- **Result must equal the batch result:** same audio, same model: word for word, or the differences are listed in the
  report (boundary handling at pieces is the only expected source).
- **Targets (measured, then written down):** at stop, transcript ready in under 30 s for a 1 hour audio recording on an
  M4 Pro (before: about 4 to 5 min); a 15 s clip unchanged (below the piece size it runs at stop, in seconds). Speaker
  finding still needs the whole file; report its time at stop as before, and note windowed speaker runs (M2b) as the way to
  shorten it.
- **Recording safety gate (needs the maintainer's consent and a time slot, hardware):** a 20 minute audio recording
  with the background transcription on, with the mic file checked for dropouts and drift (`details.gaps`, `drift_ppm`,
  the 50 ms budget) against a run with it off; CPU, GPU and memory sampled. If the recording is affected, ship the
  feature off by default and say so.
- **Owner:** `mac-ai` (pieces, whisper) with the named `cm-daemon` files (`ai/dispatch.rs`, `ai/jobs.rs`, the
  recording-finished event hookup); order after M2a's re-read-cost work, before the PhoWhisper decoding.

### M2i Sound markers instead of invented words (maintainer, Nov 3, 2026)
The Nov 2 meeting showed real, quiet speech written as "Cảmơn." (high word confidence, so a confidence threshold alone
does not catch it) and a laugh written as a word. Decision: the transcript must not guess.
- **0.2.7:** `[unclear]` where speech evidence exists (speaker engine or VAD) but no reliable words after the re-read
  (M2d coverage re-run), keeping the speaker and the time; `[noise]` where the span has energy and no speech evidence.
  They are marked as **sounds**, not words (an additive field on the transcript entries, old `transcript.json` files
  still load): shown dimmed in the transcript and caption editor, written as `[unclear]` / `[noise]` in TXT and SRT/VTT
  exports, and **excluded** from reports, translation, glossary suggestions and library text search.
- **Later (0.2.8 candidate):** `[laughter]`, `[whispering]` and other sound labels from an audio-tagging model
  (sherpa-onnx audio tagging is already in `vendor/sherpa-onnx`; needs a model licence check, size, a pinned SHA-256,
  and a measurement on public data such as VocalSound (CC BY 4.0) and MUSAN (CC BY 4.0)). Do not label quiet speech
  "whispering" without that model: it is quiet far-field speech, not whispering.
- Whisper currently runs with `suppress_nst(true)`; whether letting it write its own sound tokens helps is measured,
  not assumed.

### M2j Vietnamese transcript quality (maintainer, Nov 3, 2026: English is business-ready, Vietnamese is not yet)
Known: Turbo reaches 22.8 % WER on clean FLEURS Vietnamese read speech and 29 to 43 % on quiet variants
(`vietnamese-fleurs-baseline`, `level-aware-transcription`); PhoWhisper-small converted to ggml reads 60.9 % with our
current decode settings, because a fine-tune needs its own decoding (no timestamps, word spacing and times). Business
meetings are harder than read speech: several far-field speakers, quiet voices, Vietnamese with English terms.
Order, cheapest first, each step measured and stored before the next (short 15 s and 1 h rows kept):
1. **Models already installed on the maintainer's Mac (Medium, Large v3, Large v3 Turbo), no download:** Vietnamese
   WER, coverage and speed of Large v3 and Medium against Turbo on FLEURS vi and on the private meeting reference
   below. Large v3 is slower and re-reads more on English; its Vietnamese value is the open question.
2. **The private reference (stays outside the repository):** the maintainer corrects about 5 minutes of the Nov 2
   meeting by hand (text only, with the start time); `mac-ai` computes WER against it locally and publishes only numbers.
   FLEURS is read speech and cannot show far-field, multi-speaker or code-switching errors.
3. **Glossary and initial prompt** (exists): measure the effect of names and technical terms, including English terms
   inside Vietnamese, on the private reference.
4. **PhoWhisper medium and large with its own decoding** (no timestamps, word times from the alignment or VAD,
   spacing), only if steps 1 to 3 leave a gap; the maintainer uploads the converted files to `craftmann`
   (`packaging/phowhisper`). Check English terms inside Vietnamese: a Vietnamese fine-tune can lose them.
5. **A Vietnamese transducer through sherpa-onnx** (already linked for speakers): the sherpa-onnx docs list
   `sherpa-onnx-zipformer-vi-30M-int8-2026-02-09` (from `hynt/Zipformer-30M-RNNT-6000h`, about 6,000 hours); a 2025 build is
   said to use about 70k hours. Small and fast, but **licence unverified** (one third-party page says Apache-2.0; read
   the Hugging Face card and the release notes before anything else), and it gives no punctuation or casing. Evaluate only
   after the licence is confirmed; any model needs a pinned URL, SHA-256 and the maintainer's approval.
6. Not planned: fine-tuning our own model; LLM rewriting of the transcript (it would guess).
No Vietnamese claim in the release text without a stored report; English wording stays as measured.

Done when: before/after reports for each of M2a, M2b, M2c are stored, the tests (with fakes) cover the new
splitting/cancel behaviour, and `tests/manual/AI-5/7/8/9.md` have been re-run on the new build.

## Track M3: Import media (agent `mac-import`)

Source: `capturemann-redesign 6/` (untracked at the repo root until the maintainer moves it; boards I0 to I4,
`screens/png/Import*.png`, `source/Import*.dc.html`, `import-media-design.md`). The kit says "not yet in the specs":
M3 starts by writing `specs/features/18-import.md` (feature template of `specs/README.md`, IDs `IMP-n`, acceptance
criteria `IMP-n.ACm`) and adding the tool to `specs/02-agent-interface.md`; nothing is built before the spec meets the
Definition of Ready. Decisions already made by the maintainer (kit, Nov 3, 2026): `import_media` is Free tier;
**Remove the originals** is opt-in, Confirm for agents, original goes to the Trash only after the copy is verified;
files only, no folders.

What the analysis found (the plan for each is the work list below):

1. **The bundled FFmpeg reads only some of the formats the boards list.** Decided Nov 3, 2026 (maintainer): **the FFmpeg
   build is not extended in 0.2.7.** `packaging/ffmpeg/build-macos.sh` has decoders `h264 hevc aac pcm_*` (plus image
   codecs) and demuxers `mov matroska wav image2 rawvideo gif`, so 0.2.7 imports: video MP4, MOV, M4V and MKV with
   H.264 or HEVC video and AAC or PCM audio; audio M4A, AAC-in-MP4 and WAV (PCM). WebM (VP9/Opus), AVI, MP3, AIFF, FLAC,
   OGG, Opus and bare AAC are recognised by the probe as `unsupported_format` with the reason shown on the row ("not
   supported yet"); the sheet's supported-format line lists only what works. The probe uses the bundled `ffprobe`, which
   can still name the codec of a file it cannot decode. Extending the lists (demuxers `avi mp3 aiff flac ogg aac`,
   decoders `vp8 vp9 opus vorbis flac mp3float alac mpeg4`, all LGPL) is a candidate for 0.2.8, with `make ffmpeg` run for both
   architectures.
2. **"Prepared for editing"** (variable frame rate, MKV, AVI, odd codecs) means a constant-frame-rate editing copy.
   No libx264 ships, so the copy is H.264 or HEVC through `h264_videotoolbox` / `hevc_videotoolbox` with the `fps`
   filter (both already built). A hardware encoder is required (as for recording); without it the sheet says so.
3. **Track model.** A bundle's tracks are `screen`, `camera`, `mic`, `system`, `cursor`. An imported video is a
   `recording` bundle whose `screen` track is the master file (or the editing copy); its sound is decoded once to
   `mic.wav` (48 kHz, f32, no denoised copy) so transcription, speakers, mix, mute, export and Suggest cuts work unchanged
   (`raw_file` already falls back to `file`). The UI calls it "Audio". An imported audio file is a `kind: audio` bundle with
   the same `mic.wav`. No new `CaptureKind` (the boards' `kind: video|audio` becomes `recording|audio`); "imported" is
   `source: { path, imported_at, original_name, original_file }` in the manifest. No cursor or camera track: the editor's
   existing unavailable state is reused for Cursor, and the Camera tab is disabled.
4. **Jobs and actions.** `import` is a persisted job (copy, probe, decode, prepare, verify, optionally trash) with
   `job.progress`, cancel and resume; disk space is checked first (`disk_full`); `capture.created` fires per file. `import_media`
   is Free, but `remove_originals: true` is Confirm: the registry has one tier per tool, so a parameter-dependent tier
   (or a second tool name) is needed and must be specified in the interface spec. New error reasons: `unsupported_format`,
   `protected_media`, `unreadable_file`; `permission_missing` gets `data.permission: "files"`; a folder path is
   `invalid_input` with `data.reason: "folder_not_supported"`. MCP tools go from 74 to 75; CLI `capturemann import`.
5. **Files permission.** The app is not sandboxed: macOS asks by itself when a protected folder is read. The daemon must
   turn "Operation not permitted" into `permission_missing` and let the GUI trigger the prompt first; an agent over the CLI
   gets the error. Needs a manual script (a file on Desktop and one on an external drive).
6. **Moving to the Trash** exists in `cm-platform::trash`; the verify step (same size, decodes, duration within 1 frame)
   is new.
7. **UI work in `apps/desktop`:** the import sheet (I2), drop overlay and drag onto the library (I1), library chips and
   an "Imported" badge and progress strip on cards (I3), the editor's "Imported" chip, source line and disabled tabs (I4),
   the Captions empty state, Launcher "Import media", menu-bar "Import file…", ⌘I. **Dock-icon drop and double-click
   open need `CFBundleDocumentTypes` in the app bundle and an open-files event that nothing in `apps/desktop` handles
   today**: platform code in `apps/desktop/src/platform*.rs`. All strings through `cm_i18n` with Vietnamese drafts.
8. **Icons:** `capturemann-icons/` is not in the repo and `design/icons` has none of the three custom ones, but all eight
   (`cm-import`, `cm-import-video`, `cm-import-audio`, `file-x`, `files`, `lock-keyhole`, `folder-input`, `circle-slash`)
   are inline SVGs (24 grid, 2 px stroke, `currentColor`) in `capturemann-redesign 6/source/UiIcon.dc.html`
   (`<sc-if value="{{is.<name>}}">`). The agent extracts them into `design/icons/` with the kit's naming and updates
   `icons.json` and `icon-map`.
9. **Document conflict to raise (spec rule):** the kit's toast spec says 6 s; the maintainer changed toasts to 5 s on
   Nov 3, 2026 (`design/redesign` and `design-surfaces.md` follow 5 s). The new import toasts use 5 s except a running
   import (progress) and failures.
10. **Fit with M2:** an imported 15 s voice memo or a 1 hour interview is exactly the short and medium audio M2 targets;
    "Transcribe after import" uses the M2 fixes, and imported files (often quiet or already normalised) are a second
    real source for the quiet-speech and coverage checks.

Work list, in order (one change per commit, tests with fakes plus FFmpeg fixtures made by the test tools; hardware is not
needed):
- **S0 spec:** `18-import.md`, the interface table, error codes, glossary; the icons into `design/icons`. No change to
  the FFmpeg build (see item 1).
- **S1 engine (`cm-media`, `cm-library`):** probe (streams, duration, rotation, VFR, DRM, HDR flag), format
  classification (Ready / Prepared / Can't import), editing-copy builder, audio decode to `mic.wav`, bundle writer
  with `source`, verify.
- **S2 daemon (`cm-daemon`, `cm-jobs`, `cm-cli`, `cm-mcp`):** `import` job, `import_media` action, events,
  permission mapping, remove-originals with the Confirm tier, CLI and MCP tool, tests with scripted jobs.
- **S3 UI part 1:** library Import… and ⌘I, the sheet with per-row status, after-import options, footer, progress strip
  on cards, failed card, job toast (5 s rules), Imported badge and filter chips.
- **S4 UI part 2:** drag onto the library with the drop overlay, editor Imported chip, source line, disabled tabs,
  Captions empty state, Launcher and menu-bar entries, Dock drop and Open With (platform code).
- **S5 checks:** `tests/manual/IMP-1.md` (a phone HEVC MOV with rotation, a variable-rate MKV, an M4A interview of
  about 1 hour, a 15 s voice memo, a DRM-protected file, a file on Desktop and on an external drive, a folder drop,
  cancel, disk full, remove-originals), a benchmark report of import time per GB and of prepare time, i18n check,
  `design` side-by-side with the PNGs at 1440 px.

Owns: `crates/cm-media` (probe, prepare), `crates/cm-library` (import writer, manifest `source`), `crates/cm-jobs`,
`crates/cm-daemon` (new import files and the registry entry), `crates/cm-cli`, `crates/cm-mcp`, new `apps/desktop` import
files, icons in `design/icons`, specs and manual scripts. Must not touch `crates/cm-ai` (M2) or the
window-lifetime code (`surfaces.rs`); editor and library files shared with `mac-stability` are edited in separate small
commits and merged first-come, rebasing on `main`.

## How the agents work (parallel worktrees)

Following the parallel-agent rules we already use (disjoint files, unique script names, I merge and retest).

```sh
# once per track, from the main checkout, after `git checkout main && git pull --ff-only public main`
git worktree add ../capturemann-wt/mac-ai        -b track/mac-ai
git worktree add ../capturemann-wt/mac-stability -b track/mac-stability
git worktree add ../capturemann-wt/mac-import    -b track/mac-import
```

The `track/*` branches are **temporary work areas**; the maintainer's rule "work on `main`, no feature branches" is
about the shared line of work. Each agent's finished, tested commits are merged to `main` by me (fast-forward or
cherry-pick, keeping the commit messages), never pushed by the agent. Delete each worktree and branch after merging.

| Track | Owns (may change) | Must not touch without asking |
| --- | --- | --- |
| mac-ai | `crates/cm-ai`, `crates/cm-media` (transcription and report paths), `vendor/sherpa-onnx*`, `verification/benchmarks/`, `tests/manual/AI-*.md`, and only the job progress/cancel files in `crates/cm-daemon` named in its brief | `apps/desktop` UI beyond progress and cancel wiring; the rest of `cm-daemon` |
| mac-import | see Track M3 (probe and prepare in `cm-media`, import in `cm-library`, `cm-jobs`, new `cm-daemon` import files, CLI, MCP, new `apps/desktop` import files) | `crates/cm-ai`, `apps/desktop/src/surfaces.rs` |
| mac-stability | `apps/desktop` (window lifetime, diagnostics), `apps/overlay`, `crates/cm-daemon` (recording lifecycle), soak scripts | `crates/cm-ai`, the job progress/cancel files given to mac-ai |

**Shared files with one owner** (agents add entries on their branch, I resolve at merge): `Cargo.lock` and workspace
`Cargo.toml` (me; version stays 0.2.6 until the release commit bumps all to 0.2.7), `locales/capturemann.pot` and
`vi.po` (regenerate with `cargo run -p cm-i18n -- extract` after the merge, not on the branch), `CHANGELOG.md` and
`docs/project/status.md` (me, from the agents' final reports), `THIRD_PARTY_LICENSES.md` (`make licenses` after a
`Cargo.lock` change).

**Hardware on the Mac:** recording tests (including the 3-hour soak in M1) need the maintainer's consent and a time
slot, stay as short as the check allows, and the output is deleted (`CLAUDE.md`). The agents never start one on their
own. The Linux/Windows laptop is not used in 0.2.7.

**Each agent's brief** names the goal, the files it owns, the baseline report to create first, the commands to run
(`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, `python3 scripts/check_target_os.py`; the CI Linux and Windows jobs must stay green), what it must not do (run `make bundle` over
`target/bundle`, push, publish, say "verified" without a report) and the report it hands back: commits, tests run,
reports written, what was not done.

## Order and checkpoints

1. **Week 1:** baselines and reproductions only (M2 benchmarks, M1 logging). No behaviour change merges before its
   baseline report exists. Decisions O1, O2 and O6 are made (see below).
2. **Week 2:** fixes and speed-ups with before/after reports; M1 posts a "shared code done" note when its
   `apps/desktop` window-lifetime commits are merged. "No measurable gain" is an acceptable result for any speed-up: the
   report is stored and the change is not merged.
3. **Week 3:** the soak (with the maintainer's slot), the Intel check, the manual scripts `AI-5/7/8/9` re-run, full test
   run on all three CI OSes (the Linux and Windows jobs must stay green even though nothing new ships for them).
4. **Release:** release text from the reports; macOS packages and DMGs by the pattern of 0.2.6 (draft, user signs, user
   publishes, I check one download, cask script, status entry). The Windows and Linux early builds are not rebuilt
   for 0.2.7 unless a shared-code change forces it; the release text says that they stay at 0.2.6. Version bump to
   **0.2.7** is the release commit.

CI: shared-code commits from M1 touch `apps/desktop`, which Linux and Windows also build, so the CI jobs for both
(`ci:windows` label on a branch) run before merge.

## Decisions (made by the maintainer on Nov 3, 2026)

- **O1** Vietnamese: **English only** in 0.2.7. Baselines, WER and DER cover English; Vietnamese is listed as not
  measured in the release text. No Vietnamese corpus work, no Vietnamese-specific model change.
- **O1 amended (Nov 3, 2026, later):** the maintainer adds **PhoWhisper** (VinAI, BSD-3-Clause, Vietnamese fine-tune
  of Whisper) as downloadable speech models, sizes **small, medium, large**. So Vietnamese is measured after all, with
  open corpora whose licence allows it (Common Voice vi is CC0; VIVOS is non-commercial, so used for a local benchmark
  only, never redistributed); no private recording. Release wording stays "measured on ...". See M2e.
- **Default cuts (Nov 3, 2026):** the editor runs **Suggest cuts automatically** (silences and fillers both on) when it
  opens; the user still accepts, one undo step (AI-3.AC3 stays). See M2f.
- **O2** Text model: **keep Gemma 4 E2B.** M2c measures and speeds up the current model; no second model, no new pin.
  Remove the "larger model" candidate from the speed-up list.
- **O6** Issue 21: **wait for the reporter's 0.2.6 log.** No cutoff and no diagnostics-only build. The logging and
  window-lifetime work (M1) still ships; the release text does not claim a fix for Issue 21 unless a log-backed cause is
  found. If the log arrives late, it goes into 0.2.8 or a 0.2.7.x.
- Windows work may touch `crates/cm-ai` in Windows-only files and separate commits (0.2.8 plan); `mac-ai` is not
  blocked by it, and the maintainer merges one track at a time.

Moved to the 0.2.8 and 0.2.9 plans: O3 (Linux packaging), O4 (Windows signing), O5 (recording on Linux and Windows).

## Risks

- Accuracy work without a good Vietnamese set can only be claimed for English; the release text must say so.
- A change inside the vendored sherpa-onnx wrapper must keep the no-download, no-GPL build rules; it goes in its own
  commit with its own before/after report.
- Windowed speaker runs (M2b) can split one speaker across windows; the gate is that DER on the held-out English clips
  does not get worse than a tolerance set from the baseline. Retuning uses a tuning set and a separate held-out set.
- The report rubric (M2c) is a floor check (timestamps exist, speakers present, no invented names or numbers); it does
  not measure whether claims are right, and the release text must not call it accuracy.
- Two tracks edit near `apps/desktop` and `cm-daemon`; the ownership table gives the shared job files to `mac-ai` by name,
  and M1's shared-code commits merge first, then `mac-ai` rebases.
- The Linux and Windows early builds get no fixes in 0.2.7; any user report of a bug there is logged for 0.2.8 or 0.2.9.

## Later releases (not part of 0.2.7)

Moved out on Nov 3, 2026 so 0.2.7 can stay on macOS. Each has its own plan, owned by a separate session and agent that
runs alongside 0.2.7 on its own worktree (shared code is merged after M1's, see each plan):

- **0.2.8, Windows:** `specs/plans/release-0.2.8-plan.md`
- **0.2.9, Linux:** `specs/plans/release-0.2.9-plan.md`

The one test laptop (Linux and Windows dual boot) is not used by the 0.2.7 tracks. O3, O4, O5 live in those plans.
