# Release 0.2.4 plan: performance and diagnostics

Status: **draft, revision 3** (Oct 29, 2026), nothing built. The release has two named parts, and nothing
else ships in it: **(1) performance** (measure, fix the top two findings, write down the rest) and
**(2) diagnostics** (a log file and "Copy diagnostics", so users report problems without sharing audio).
Rules that apply: general techniques only, no one-recording fixes (open source); nothing is called
"verified" without a stored report; measurements go in `verification/benchmarks/`; no telemetry and no
network traffic; UI text through `cm_i18n`; every option has an MCP parameter and a CLI flag; keep new code
platform-neutral (Windows is next). The plan in revision 2 (Vietnamese proxies, windowed speaker runs,
transcript options) is parked in "Deferred" at the end.

## Why these two

- **Diagnostics:** issue 16 (Intel crash) was solvable only because the reporter attached a macOS crash
  log. Everything else we print is lost: the logger in `crates/cm-core/src/logging.rs` writes only to
  stderr, and an app opened from Finder or the Dock has no visible stderr. There are 79 `log::` calls
  already, and `eprintln!` outside tests and examples (for example the `cm-encode:` reasons in
  `crates/cm-encode/src/mac.rs`) that bypass the logger.
- **Performance:** we have single numbers (88 minutes of speaker finding took 6 minutes and 4.4 GB; a
  report on an 11-minute meeting took 84 s to translate), but no table by model, length and chip, and no
  list of where the time goes. The 2.5-hour recordings are where users will notice. We fix what the
  numbers show, not what we guess.

## Part 1: diagnostics (small, independent, build first)

Goal: a user with a problem opens Settings, About, presses **Copy diagnostics**, and pastes a block of text
into an issue. It contains no audio, no transcript text, no file names of recordings and no tokens.

### 1.1 A log file

- Extend `cm_core::logging` (used by `cm-desktop`, `cm-daemon` and `cm-overlay`) with a second sink: a file
  `capturemann.log` in a `logs/` folder of the config directory (`cm_daemon::config::config_dir()` already
  honours `CAPTUREMANN_CONFIG_DIR`, so a second instance logs to its own folder). One function builds the
  path from the config dir, never a hard-coded `~/Library` path.
- **Rotation:** keep the current file plus the previous two, each capped at 2 MB (about 6 MB total, fixed;
  no setting). Rotate at start and when the cap is reached. Writes are line-buffered and flushed on `warn`
  and `error`, so the last lines before a crash are on disk.
- **Level:** the file records `cm_*` at `info` and everything else at `warn` (today's defaults); `RUST_LOG`
  still overrides. Add a timestamp (UTC, ISO 8601) and the process name (`desktop`, `daemon`, `overlay`)
  to each line; stderr output keeps its present form.
- **Panics and aborts:** install a panic hook that writes the message, location and a backtrace to the log.
  A Rust abort or an uncaught Objective-C exception cannot be logged, so the next start writes one line,
  "previous run did not exit cleanly", found with a small `running` marker file removed on clean exit
  (the same idea as `recording.lock`). This turns "it crashed" into something visible in the next report.
- **Move the stragglers:** change the non-test `eprintln!` calls that report problems to `log::warn!` or
  `log::error!` (the `cm-encode` reasons first). Leave CLI output and usage text alone.
- **What must never be logged** (reviewed line by line for the existing 79 calls, then enforced by a test
  that logs sample records through the redaction function): OAuth tokens, API keys, authorization headers,
  transcript or caption text, report text, the contents of agent tool parameters, and full paths under the
  user's home folder. Paths are written with the home folder replaced by `~`; recording and project names
  are replaced by their ids. A single `redact` function in `cm-core` does this and is applied when the
  file sink and Copy diagnostics read lines.

### 1.2 Copy diagnostics

- The only place in the app UI is a button in Settings, About (the page in `apps/desktop/src/ui/settings/general.rs`), and the same text
  from `capturemann diagnostics` and an MCP read-only tool `get_diagnostics` (same name, same text; every
  user action has a tool). It is read-only, so it is a Free tier tool, and it returns text only; nothing is
  sent anywhere.
- **Contents, in order:**
  1. App: name, version, build number, bundle id, how it was started (app bundle or binary).
  2. System: macOS version, chip and architecture, memory, number of cores, free disk space on the library
     volume, display count and scale factors.
  3. Permissions: Screen Recording, Microphone, Camera, Input Monitoring, Accessibility as granted or not
     (read-only probes the app already has).
  4. Models and engines: which speech, text and speaker models are installed with sizes and the pinned
     ids, whether bundled FFmpeg, `llama-server` and the speaker engine are present and their versions,
     the GPU in use by Whisper.
  5. Settings that change behaviour (a short allow-list, for example frame rate, encoder choice, noise
     reduction, languages you speak, auto speakers), not the whole file. Destination names are replaced by
     their kinds (`local`, `drive`, `youtube`).
  6. Library: number of captures and total size, never names.
  7. Recording plan of the last recording (`used_plan` from its manifest: which encoder settings were
     applied or dropped), because issue 16 was about exactly that.
  8. The last 200 log lines at `warn` and above, redacted, and the "previous run did not exit cleanly"
     line if there is one. Optionally, the most recent performance summary (Part 2).
- A preview before copying: the text is shown in a read-only box so the user can see what leaves the app
  (the user copies it; the app does not send it). A second button, **Reveal log folder**, for users who
  want to attach the files. A line in the sheet states "Nothing is sent. Check it before you paste."
- Document it in `specs/features` (new ID, `DIAG-1`, acceptance criteria below), in the privacy notes of the
  About page and in `docs/` for users.

### 1.3 Acceptance and tests

- **DIAG-1.AC1** Given a fresh start, then `logs/capturemann.log` exists and holds the start line with the
  version; given 3 MB of log lines, then the folder holds at most three files of at most 2 MB each.
- **DIAG-1.AC2** Given a record with a bearer token, a recording name, a transcript sentence and a path under
  the home folder, then none of the first three appears and the path starts with `~` in the file and in
  Copy diagnostics.
- **DIAG-1.AC3** Given a panic in a test thread, then the message and a backtrace are in the file.
- **DIAG-1.AC4** Given a `running` marker from a previous run, then the next start logs "did not exit
  cleanly" once and removes the marker; a clean exit leaves no marker.
- **DIAG-1.AC5** Given "Copy diagnostics" with no network, then the text is produced, includes every section
  above, and no network call is made (a test with the network layer scripted to fail).
- **DIAG-1.AC6** `get_diagnostics` and `capturemann diagnostics` return the same text as the button, and
  `set_settings` cannot disable the log.
- Tests are headless (a temporary config dir, a fake clock for rotation); the About button gets a manual
  script `tests/manual/DIAG-1.md` (steps, what the text must not contain, a sample with a real token typed
  into a note to prove it is redacted). The file sink must not write during other tests unless a test sets
  its own directory.
- A cost note: the file sink writes only lines that pass the level filter, so there is no measurable effect
  on recording; Part 2 includes a check that logging at `info` costs less than 1 % of a transcription.

Done when: the six ACs pass, the manual script has a stored run, and a deliberately broken run (an encoder
setting the platform rejects, on this Mac) produces a diagnostics text that shows the reason without a
crash log.

## Part 2: performance

Method: **measure first, then fix the two biggest findings, then write down the rest.** No change to a
default or an algorithm without a stored before and after number.

### 2.1 A benchmark harness (headless, repeatable)

- One example, `cm-ai` `perf` (beside `measure`, `baseline`, `lang_probe`), and a script that drives it, run
  on a quiet Mac with the power cable in and nothing else heavy open (the harness records the thermal and
  power state, free memory and other CPU load, and refuses to start under load).
- Inputs: synthetic or public audio so the table can be reproduced by anyone and no private recording is
  stored: the existing English clips joined to lengths of 10 minutes, 1 hour and 2.5 hours (a general
  concatenation tool, seeded), plus the maintainer's 2 long recordings and several short ones (from
  `CM_PERF_AUDIO`, not published as audio, only as numbers). Real recordings are the main rows because
  they are the audio users have; the joined clips are the reproducible ones. Short files are also joined
  to make the 1-hour length when a long file is missing for a language. `cm-daemon` `make_fixture` supplies the recording-side fixture.
- Output: one JSON line per run (what, model, input minutes, wall seconds, user and system CPU seconds,
  peak resident memory, peak GPU memory if available, phase timings) and a Markdown table in
  `verification/benchmarks/performance-<chip>-<date>.md` made from them. Each row carries machine (chip,
  memory, macOS), build (version, commit, release profile) and number of runs (3, median shown, spread
  shown).
- Peak memory is read from the OS (`task_info` resident size high-water mark on macOS) by one small
  function behind a trait with a fake, so it can be used in tests and on Windows later.
- **Phase timers in the product itself** (cheap `Instant` spans logged at `debug`, and summed into a
  "last run" record kept in memory and shown in Copy diagnostics): decode and resample, voice-activity
  step, Whisper (per 30 s window), word building, speaker run, report chunks, model load. This is what
  answers "where does the time go" on a user's Mac, so Parts 1 and 2 share it.

### 2.2 What to measure (the table)

| Area | Runs | Reported |
|---|---|---|
| **Transcription by model** | Large v3, Large v3 Turbo, Medium, Small (whichever are offered) on 10 min, 1 h, 2.5 h | real-time factor (audio seconds per wall second), peak memory, model load time, share of time in decode, in VAD, in Whisper, with GPU on and off |
| **Transcription settings** | threads (the current rule against 4, 6, all performance cores), noise reduction on and off, flash attention on and off if the build has it | RTF and memory; an effect on words must be checked with the 0.2.2 repeat check so a faster setting does not change the text |
| **Speaker finding by window** | the whole file against windows of 10, 15 and 20 minutes (a throw-away experiment harness; see Deferred for the product work) on 30, 60 and 88 minutes | time, peak memory; this chooses a window size and says whether bounded memory is real |
| **Report on long recordings** | each template (`summary`, `minutes`, ...) on a 1-hour and 2.5-hour transcript with the default text model | time, peak memory of `llama-server`, number of chunks, tokens per second |
| **Translation** | 1-hour and 2.5-hour transcripts to one language | time, memory, batch size effect, retries |
| **Export** | the M4 parity fixture at 10 min and 1 h: video export and audio export | encode time as a multiple of duration, memory |
| **App** | cold start to first window, library open with 1,000 captures, editor open of a 1-hour recording, idle CPU and memory with the app open for 10 minutes | milliseconds, MB, % CPU |
| **Recording overhead** | a 10-minute `record_e2e` run: dropped frames, CPU and memory of the app while recording (only when the maintainer asks for the hardware run) | the number the product spec NFR already states |

Everything is on this Mac (Apple Silicon). The Intel build is measured only if the maintainer or the
reporter of issue 16 runs the same script and sends the Markdown table (the table is short on purpose, so
it can be pasted into an issue). A result from another machine is labelled with its chip, never merged.

### 2.3 Choosing the top two

After the table exists, rank findings by **user-visible cost = (time or memory over what the user would
accept) x (how often the action happens)**, and take the two highest that have a general, low-risk fix.
The plan does not guess them, but these are the candidates the code already hints at, in the order I would
check:

1. **Speaker finding: memory and no early stop.** 88 minutes took 4.4 GB and 6 minutes, and Cancel waits
   for the run to end because sherpa-onnx 2.0.1 has no stop. If the window experiment shows memory is
   proportional to length, the product fix is windowed runs with linking (parked below). If it shows the
   peak is a single large allocation, a cheaper fix may exist (for example feeding the engine 16 kHz mono
   `f32` once without copies; check the copies between `audio.rs` and the engine).
2. **Transcription pipeline overlap:** decode and resample of the next part while Whisper works on the
   present one, if the phase timers show audio preparation is more than a few percent of the total.
3. **Whisper threads and GPU usage:** `set_n_threads` and the load fallback chain in
   `crates/cm-ai/src/whisper.rs` should match the chip (performance against efficiency cores); the table
   decides.
4. **Report and translation chunking:** fewer, larger prompts or reuse of the prompt prefix on
   `llama-server` (its prompt cache), if the 2.5-hour runs are dominated by prefill.
5. **Model load time and duplicate loads:** a model loaded again for each job (transcribe, then language
   probe, then re-generate) instead of kept for a short idle time. The memory cost of keeping it is
   weighed with the numbers.
6. **App-side costs:** library open and editor open if the table shows hundreds of milliseconds.

Rules for a fix: it must not change the transcript (the 0.2.2 repeat check and a word-for-word comparison
of before and after on the benchmark clips; a speed-up that changes words is a quality change and is
judged as one), must not raise peak memory by more than it saves elsewhere, is general (no special case for
a recording length or a language), and keeps cancel working. Each fix gets a before-and-after row in the
same benchmark file and a regression test that would catch the slowdown coming back (a counted-work test
with fakes, such as "audio is converted once", not a timing test, which is flaky).

### 2.4 The rest is written down

Everything not fixed goes into `docs/project/performance.md` (new): the table, the ranked list with the
number that justifies each item, what was tried and rejected, and the budget each action should meet
(for example "transcribe at least 8x real time with Large v3 Turbo on an M-series Mac", numbers set from the
first table, not before). Later releases and the Windows work reuse the harness and the budgets.

### 2.5 Acceptance

- **PERF-1.AC1** The harness runs from a clean checkout with one command and produces the Markdown table
  for the rows above; a re-run on the same machine is within 10 % on wall time (median of 3).
- **PERF-1.AC2** Peak memory and the phase timings of the last run appear in Copy diagnostics (shared with
  DIAG-1).
- **PERF-1.AC3** The two chosen fixes each show a stored before and after improvement of at least 15 % in
  the metric they target (time or peak memory), with identical transcript text on the benchmark clips.
- **PERF-1.AC4** Every other finding is in `docs/project/performance.md` with its number.
- **PERF-1.AC5** The logging of Part 1 costs less than 1 % of the time of a transcription run (measured
  with `info` on and off).

Done when: AC1 to AC5 hold and the table is stored. If no fix reaches 15 % without risk, that is a result:
the release ships with diagnostics and the performance document only, and says so.

## Order and size (rough)

1. Diagnostics 1.1 and 1.2 (about 3 days): log file, redaction, panic hook and marker, About button, CLI,
   MCP tool, ACs and the manual script. Build first because the phase timers of Part 2 feed it and because
   it makes any problem in the rest of the work visible.
2. Benchmark harness and first table (about 2 days): phase timers, memory function, the `perf` example,
   the long inputs. Run overnight; the first full table is the gate for what comes next.
3. Rank, pick two, fix, re-measure (about 4 days, depending on the finding).
4. Write `docs/project/performance.md`, the quality reports, the status page and the release (the
   maintainer publishes as before).

Decisions (maintainer, Oct 29, 2026): `get_diagnostics` is an MCP tool (read-only, Free tier; the tool count
goes from 73 to 74); the log caps are 3 files of 2 MB; "Copy diagnostics" lives only in Settings, About
(not in the menu bar item); the maintainer's several short audio files and 2 long ones may be used for the
benchmarks, none is sensitive. The audio stays outside the repo (a folder named by `CM_PERF_AUDIO`, never
committed); the stored report names each file by a short id, its length and its language, and holds numbers
only.

## Not in 0.2.4

Everything in "Deferred", translation, the first-use setup sheet, Ollama and LM Studio, and Windows and
Linux.

## Deferred (parked, not cancelled)

From revision 2, to be picked up after this release or when the maintainer decides:

- **Evidence without Vietnamese recordings:** a public-corpus fetcher (Common Voice, FLEURS, VIVOS; licences
  to check), a seeded mixer that builds conversations with known ground truth (gaps, overlap, noise,
  language switches), a voice-print same/different-speaker test, a reference-free health report, and a
  baseline table where every Vietnamese row says "proxy". The benchmark concatenation tool of Part 2 is
  the first piece of the mixer.
- **Speaker detection:** windowed runs with global linking by voice prints (progress, cancel between
  windows, bounded memory), a count rule that uses the linked prints, the confidence UI, boundary tidy-up
  and the "me" prior for both-track recordings. Part 2 measures whether the window size matters; the
  product work waits for that number and for the evidence tools above.
- **Transcript quality:** Silero VAD (`ggml-silero-v5.1.2.bin`, licence and SHA-256 to confirm), mixed
  language handling, Whisper decoding settings, original microphone against noise-reduced, a "higher
  accuracy" option, model choice per language. Judged on WER; the proofreading pass stays out until
  Vietnamese references exist.
- **Reliability:** the Objective-C exception audit (capture, camera, audio, overlay), the real Intel Mac
  result for issue 16 (the log file of Part 1 makes it easier to get), the flaky `cm_daemon` test, the
  first real run of `release.yml` on a fork, the Dependabot pins.
- **Filler policy** (um, uh, "ừ", "ờ") and hand-corrected Vietnamese references, which are welcome at any
  time and block nothing.

## After 0.2.4: Windows

The next major ship is a Windows release. What it needs, so 0.2.4 does not make it harder:

- **Platform pieces behind the existing traits** (capture: Windows Graphics Capture; encoder: Media
  Foundation hardware encoders; audio: WASAPI including loopback for system audio; cursor; camera;
  `keyring` with the Windows credential store). `cm-capture`, `cm-encode` and `cm-cursor` have
  `windows.rs` modules, `cm-camera` has an `unsupported.rs` for other systems and `cm-audio` goes through
  `cpal`, so the state of each is to be checked first; the spikes marked "pending on Windows" in
  `docs/project/spikes.md` come first, because a spike passes only if it passes on both systems.
- **Native libraries for the speaker engine and Whisper on Windows**: sherpa-onnx static libraries (the
  `fetch-macos.sh` equivalent), whisper.cpp with CPU first and then a GPU backend, FFmpeg (LGPL) and
  `llama-server` builds, all pinned by SHA-256.
- **Packaging and signing**: an installer (MSI or MSIX), Authenticode signing, a reputation plan for
  SmartScreen, and the Windows twin of `make release`.
- **CI**: a `windows-latest` job that builds and tests the headless crates, so macOS-only assumptions
  (for example `std::os::unix::fs::symlink` in examples and tests) are found early.
- **What 0.2.4 does for it:** the log file, the diagnostics text, the memory function and the benchmark
  harness are platform-neutral behind small traits (paths from the config dir, OS facts from one module
  with a macOS implementation and a stub), so the Windows release gets its bug reports and its performance
  table from day one.
