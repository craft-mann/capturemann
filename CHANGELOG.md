# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project follows [Semantic Versioning](https://semver.org/) once it reaches 1.0.

## [Unreleased]

## [0.2.5] - 2026-10-30

0.2.5 is about what Auto-redact finds: 21 kinds of sensitive text instead of 4, chosen from the editor, the command line or an agent, with a benchmark that measures it. Benchmark results comparing 0.2.3 and 0.2.5 are added to the release notes from the stored reports in `verification/benchmarks/`.

### Benchmark results (Auto-redact, 0.2.3 to 0.2.5)

Measured with Apple Vision on an Apple M4 Pro (macOS 26.6.2), on 300 synthetic screenshots made by the project from a fixed seed: 211 images in the `dev` layouts and 89 in four `held_out` layouts that were read once after everything was fixed. Report: [`redaction-0.2.5-final-2026-10-30.md`](https://github.com/craft-mann/capturemann/blob/v0.2.5/verification/benchmarks/redaction-0.2.5-final-2026-10-30.md).

| | dev: 0.2.3 | dev: 0.2.5 | held-out: 0.2.3 | held-out: 0.2.5 |
|---|---|---|---|---|
| Kinds looked for | 4 | 20 | 4 | 20 |
| Items scored | 1,367 | 2,275 | 436 | 1,410 |
| Found | 92.2 % | 95.3 % | 94.5 % | 98.6 % |
| Precision | 68.8 % | 92.0 % | 69.2 % | 94.4 % |
| False alarms per image | 2.725 | 0.417 | 2.056 | 0.831 |
| Still readable after the blur | 4.2 % | 2.1 % | 3.4 % | 0.5 % |

- The two versions score different sets of items: 0.2.3 cannot find the 16 kinds it did not know, so "found" is not like for like.
- The images are synthetic and the rules were written for their formats, so these are upper bounds, not real-world accuracy, and a blurred screenshot is not proof that nothing sensitive remains.
- False alarms on screenshots with nothing to find are 0.746 (`dev`) and 0.889 (`held_out`) per image, above our 0.2 target. The generic long-token rule (unchanged) is the main cause, for example full git hashes.
- Private-key blocks are the weakest default kind with Apple Vision (65.5 % of key lines covered on `dev`; the held-out layouts have none). Bare ten-digit numbers without separators and letters-only passwords are not found.
- Windows and Linux have no recognizer yet.

### Added

- **Auto-redact finds 21 kinds instead of 4.** Besides emails, tokens, phone numbers and IPv4 addresses it now proposes payment cards (Luhn, issuer prefix, length) with the expiry date or security code beside them (`card_extra`, found by position, no label needed), Social Security numbers (issuable ranges, masked forms skipped), ITINs, EINs, ABA routing numbers, IBANs, NPIs, DEA numbers, Medicare Beneficiary Identifiers, US passport numbers, VINs, IMEIs, IPv6 and MAC addresses, PEM private-key blocks, `NAME=value` and `"name": "value"` secrets, and URLs that carry a password or a secret in the query. Still only proposals: nothing is applied until you accept, and it runs on your Mac with no network. The MCP tool `suggest_redactions` takes the new kind names in `kinds` (the command line has no `--kinds` flag yet). Details: [`docs/user/redaction.md`](docs/user/redaction.md), spec `specs/features/17-redaction.md` (RED-1 to RED-3).
- **Choose which kinds Auto-redact looks for.** The editor's Auto-redact card has **What to look for** (a switch per group, a chip per kind, Defaults, Search again), the command line has `capturemann annotate <id> --suggest-redactions --kinds ssn,card`, and the MCP tool has `kinds`. The editor chooser is tested with fakes and has not been seen by eye.
- **A benchmark for Auto-redact**: `cargo run -p cm-render --example redaction_bench` builds a synthetic set of 300 screenshots with exact ground truth from a seed and scores recall, precision, false alarms and the end-to-end leak rate (blur, render, read again), per kind and per standard. Reports (Apple Vision, Apple M4 Pro, 2026-10-30): the [final run](https://github.com/craft-mann/capturemann/blob/v0.2.5/verification/benchmarks/redaction-0.2.5-final-2026-10-30.md) with the `dev` and `held_out` layouts (read once), the [0.2.3 baseline](https://github.com/craft-mann/capturemann/blob/v0.2.5/verification/benchmarks/redaction-0.2.3-baseline-2026-10-30.md) and an interim run. Synthetic numbers are an upper bound, not real-world accuracy. A manual cross-check on real screenshots (counts only) is `tests/manual/RED-1.md`, not run yet.
- **The README and the user guide list the kinds from the rule data.** The table between the `redaction-kinds` markers is generated, and a test (`crates/cm-render/tests/redaction_docs.rs`) fails when it differs from the code. New: the README section "Find personal data before you share a screenshot", `docs/user/redaction.md` and `docs/development/redaction.md`.

### Changed

- **`npi` is off by default** (30 % of its proposals were real on the synthetic dev split: a ten-digit number passes its check digit one time in ten); name it in `kinds` to turn it on.
- **Phones, IP addresses and tokens.** US (NANP) phone numbers with two or more separators get higher confidence (0.7), an extension is part of the span, and dates, times, addresses, versions and long digit runs are tested not to be read as phone numbers. IPv6 is found beside IPv4 and loopback and unspecified addresses are no longer proposed. The token rule knows more provider key shapes and bearer credentials; the generic mixed letter-and-digit rule stays at confidence 0.6. On the synthetic dev split with Apple Vision, precision went from 68.8 % to 90.9 % and false alarms per image from 2.725 to 0.550, while the set scored grew from 1,367 to 2,288 items (the two figures score different sets).
- Known limits, stated in the README: formats and checksums only (no names, addresses or dates), text the recognizer cannot read stays, bare ten-digit Vietnamese mobile numbers and letters-only passwords are not found, private-key blocks are partly missed with Apple Vision, and there is no Windows or Linux recognizer. Not a compliance tool.

### Known limitations

- Auto-redact finds formats and check digits, not names, addresses, dates of birth or text next to a label (a CVV or expiry with no card number on screen is missed). Bare ten-digit numbers without separators (for example Vietnamese mobile numbers) and passwords made only of plain letters are not proposed.
- Text the recognizer cannot read cannot be found. Auto-redact needs a recognizer, so it works on macOS only (Apple Vision); Windows and Linux have none yet.
- The false-alarm target (below 0.2 per screenshot with nothing to find) is not met: 0.746 and 0.889 on the synthetic dev and held-out layouts. `npi` is off by default.
- The benchmark is synthetic; the check on real screenshots (`tests/manual/RED-1.md`) has not been recorded. The editor's kind chooser was tried by hand but not recorded in a report.
- The Intel fix for issue 16 (0.2.3, 0.2.4) is still not confirmed on a real Intel Mac.

## [0.2.4] - 2026-10-30

0.2.4 is about finding out where time goes and making problems easier to report. Two speed changes, a log file with a Copy diagnostics button, and measured numbers for the speech, speaker and text models.

### Fixed

- **Intel Macs: a track that throws no longer aborts the app** (GitHub issue 16, second report: 0.2.3 still crashed). The 0.2.3 catch covered the encoder set-up only; the crash report shows the abort in another thread that `RecordingSession` starts, with a macOS exception unwinding out of it. Every call into a track (start, pause, resume, stop) now runs inside an Objective-C exception catch: an optional track that throws is dropped with a warning and the recording goes on, a required one fails the start cleanly. The track that threw is named in the log.
- **Encoder rejects the display size: record smaller instead of failing.** When the hardware encoder rejects the full size with every codec and setting, the recorder now tries 2160p, 1440p and 1080p heights of the same shape (at most three) and the capture is configured for the size that works. The manifest keeps the size asked for in `downscaled_from`. The reasons the encoder gave go to the log file (they went only to standard error, which nobody sees when the app starts from Finder).

### Added

- **Diagnostics.** Each process (the app, the daemon, the overlay) writes a log file in `<config>/logs` (3 files of 2 MB, oldest dropped). Bearer tokens, recording names, transcript text and the home folder are left out (paths start with `~`), a panic is written with its backtrace, and a start that follows a crash logs "did not exit cleanly" once. **Settings, About, Diagnostics, Copy diagnostics** puts a text report on the clipboard: versions, system, permissions, models, a few settings, library counts, the last warnings and the last runs with their time and peak memory. The same text comes from the new MCP tool `get_diagnostics` (tool 74, read-only) and `capturemann diagnostics`. Nothing is sent anywhere. Not yet checked by hand (`tests/manual/DIAG-1.md`).
- **A benchmark harness**: phase timers and peak memory in the app's own work, the `cm-ai` `perf` example and `tests/tools/perf_matrix.py` (one process per configuration, waits for a quiet machine). Two reports in `verification/benchmarks/`: speech and speaker finding on an Apple M4 Pro (three models, thread counts, 1 to 88 minutes), and reports and translation with the built-in text model (11 minutes to 2.5 hours). The notes, the ranked list of what was not done and what was rejected are in `docs/project/performance.md`. None of it ships in the app.

### Changed

- **Speaker finding starts with the automatic transcription** instead of after it, and the two run side by side. An 11-minute recording takes 37 s for both instead of 52.6 s, an 88-minute one 483 s instead of 587 s, with the same transcript text. The transcript itself arrives about 20 % later while both run (11 min: 24.6 s to about 30 s; 88 min: 370 s to 458 s). Turn Auto speakers off to get the old timing.
- **Speaker finding uses up to 8 threads on Apple Silicon** (the number of performance cores) instead of 4: 17 to 21 % faster, identical speaker turns (11 minutes: 33.9 s to 28.0 s). Other computers keep `min(4, cores)`.

### Known limitations

- Translation with the built-in text model stopped with `output_too_long` on two repeated-text stand-ins of 1 and 2.5 hours and kept nothing; a real 88-minute recording translated fine. Reports split a piece on that error, translation does not yet. See `docs/project/performance.md`.
- Export, app start and library timings, and a repeat of every row three times, are not measured yet. The numbers come from one Mac.
- The speaker count with an unknown number of people can run the engine twice on very long recordings (88 minutes: 265 s plus 263 s). Memory of speaker finding grows with length (1.2 GB at 1 hour, 3.5 GB at 2.5 hours).

## [0.2.3] - 2026-10-29

0.2.3 is about speaker detection, plus the Intel Mac recording crash and the fixes found while testing 0.2.2.

### Fixed

- **Intel Macs: starting a screen recording no longer aborts the app** (GitHub issue 16, MacBookPro16,1 with an Intel UHD 630). AVFoundation rejected the encoder settings by throwing an Objective-C exception, which unwound into Rust and killed the process, leaving the "Currently Sharing" indicator up. The encoder set-up (and every frame append) now runs inside an exception catch. A rejected plan is tried again without the optional compression properties, then (for HEVC up to 4096 px wide) as H.264; the manifest records the codec the file really holds. When nothing works the recording says the Mac has no usable hardware encoder instead of crashing, and the reason AVFoundation gave is written to the system log. Not yet confirmed on the reporter's Mac.
- **A short Vietnamese recording came out as Indonesian** ("Hai pinta Recalling screen dan bantuan Jemaah..."). Whisper's language detector listens to the first 30 seconds and, on an 8-second clip, rated Indonesian 55 % and Vietnamese 24 %. A new setting, **Languages you speak** (AI page, `ai.languages`, for example `vi, en`), makes the detector choose only among those; the same clip then reads "kiểm tra Recalling screen". Also `language: "vi,en"` on the `transcribe` tool and `capturemann ai transcribe --language vi,en`. Detection now skips the silence before the first sound. Until the setting is filled in nothing changes: Whisper still picks among all its languages. To repair an old transcript, set the languages and Re-generate it.
- Reveal in Finder on a recording or an audio capture failed with "not a screenshot".
- Cancel on a report left "Writing the report 0%" on screen for ever; the card now clears once the job has stopped.

### Changed

- **Speaker detection uses a better voice model.** The English-only CAM++ model is replaced by the 3D-Speaker CAM++ Chinese-English model (Apache-2.0, 28 MB), and the clustering threshold for an unknown number of people is 1.0 instead of 1.2. On 6 public English conversation clips held back while choosing, the diarization error rate fell from 54 % to 17 % (51 % for calling everyone one speaker) and the speaker count was right in 5 of 6 clips, against 0 of 6. English only: Vietnamese is not measured yet. Details and caveats: `verification/benchmarks/speaker-detection-2026-10-29.md`. Speakers found by 0.2.2 stay as they are until you run Find speakers again.
- Finding speakers shows real progress instead of 0 % until it ends. Cancel still takes effect when the run ends (the speaker library ignores a request to stop).

### Added

- **Languages you speak** (AI page, setting `ai.languages`): see the transcript fix above. The `transcribe` tool and `capturemann ai transcribe` take a list such as `vi,en`.
- A benchmark folder, `verification/benchmarks/`, with the speaker-detection benchmark (setup, public data, results, what it does not show) and the tools behind it: word and diarization error rates (`cm-ai` `measure`), the baseline runner (`baseline`), a draft-reference maker for your own recordings, `tests/tools/make_reference_clips.py` for PriMock57 and AMI clips, and `lang_probe`, which prints Whisper's language probabilities. None of it ships in the app.

### Known limitations

- Speaker accuracy is measured on 14 English conversation clips only. Vietnamese and other languages are not measured, so the accuracy criteria of speaker labels stay open. The speaker count is still a suggestion: 3 of 8 tuning clips and 1 of 6 held-out clips got it wrong, usually one speaker too many.
- Finding speakers on a very long recording uses a lot of memory (about 4.4 GB for 88 minutes) and Cancel takes effect only when the run ends: the speaker library ignores a request to stop. Running it in windows would fix both and is not done.
- The Intel recording fix is untested on a real Intel Mac. If every retry is rejected, recording still fails (with an error instead of a crash); the reason is in the system log.
- The language list only helps once it is filled in; with the field empty a short clip can still be heard as a related language.
- The known limitations of 0.2.2 still apply (no transcript word error rate for Vietnamese, the second reading only on loops, longer runs for long recordings).

## [0.2.2] - 2026-10-28

0.2.2 is about transcripts of long recordings, plus the fixes found while testing 0.2.1. Translation and speaker detection are unchanged and get their own release.

### Fixed

- A long recording no longer fails its report with `invalid_model_output`. When one piece's notes did not fit in the model's reply limit, the JSON was cut off and the retry had no room left; the piece is now split in half (or, for the merge, the notes are kept side by side). Seen on a 2.5-hour Vietnamese recording. 0.2.0 and 0.2.1 have the problem.
- A long recording is no longer transcribed as one phrase repeated thousands of times, and its transcript is checked for the other ways Whisper goes wrong. A Large v3 run on an 88-minute Vietnamese recording came back with one sentence 2,480 times from 5:35 to the end: Whisper writes each 30-second window on the text of the one before, and one bad window carried through the rest. A recording over 10 minutes is now transcribed piece by piece (cut at the quietest spot near every 5 minutes), so a loop cannot pass from one piece to the next, the glossary applies to every piece instead of fading out after the first minutes, and the language found in the first piece is used for the others. Every piece is checked for exact loops and for soft loops (the same sentences coming round with small changes, which an exact-repeat check misses: the noise-reduced reading of that recording had about 8 minutes of them). A bad piece is read again from the other audio of the recording (the original microphone or the noise-reduced copy, whichever was not read) and the better reading is kept; one that is still bad is split in half and done again, and what survives has its repeats taken out. Recordings under 10 minutes are one piece and get the same checks. 0.2.0 and 0.2.1 have the problem.
- A transcription, speaker or report run that the daemon already has is now shown in the window. After a restart the daemon picks up queued and interrupted jobs by itself, and nothing told the window, so the Transcript and Speakers tabs showed nothing while a long transcription was running. The app now reads the daemon's job list at start and whenever an editor opens, and shows those runs with their progress.
- A long transcription no longer starts over from 0% when a recording interrupts it. While you record, the daemon holds background jobs; the transcription had no saved progress, so when the hold ended it began again (seen at 7x% on an 88-minute recording). It now saves its progress after every finished piece and goes on from the last one.
- A one-line field (the rename box, search fields) now scrolls to follow the caret: a long text is no longer cut at the right edge, and the arrow keys reach the whole text.
- The audio editor of a long recording plays again right away. Mixing the preview of an 88-minute recording took about 45 seconds and 12 GB of memory (the exporter's loudness measurement and true-peak limiter ran over the whole mix), and Play was silent until it finished; it now takes under a second and about half the memory. Exports are unchanged.
- The sidebar highlights Audio, not Recordings, while an audio recording is open in the editor.
- The sidebar shows the exact version (for example `v0.2.2`), the same as Settings > About, instead of only major.minor.
- After a long recording the Transcript and Speakers tabs now say that the transcript and the speaker labels are being made, with the progress, and that nothing needs pressing. The automatic transcription was invisible to the editor (the new `transcript.started` event tells it), so the tabs looked idle for minutes.

### Added

- **Original microphone** for transcription: the Re-generate panel has an Audio choice, Noise-reduced (the default) or Original microphone. Noise reduction can remove soft sounds that Whisper needs, so a transcript that misses words may read better from the original. Also `original_audio` on the MCP `transcribe` tool and `capturemann ai transcribe --original-audio`. A recording made without noise reduction is not affected.
- A **Re-generate** button in the Transcript tab of the audio editor: pick another installed speech model and the language, and make the transcript again (the same panel as the recording details page, including what happens to your caption edits). A different model can give a better transcript.

### Changed

- Report exports leave out the citation times by default: Markdown, TXT and Copy no longer print `[mm:ss]` after every item. A new box "Include citation times" under the report (and `include_times` / `capturemann ai report export --times`) adds them. The marked moments keep their times; the report view still shows every citation and seeks.

### Known limitations

- The transcript checks look at repetition, not at accuracy: no word error rate was measured, and the improvement is shown on one 88-minute Vietnamese recording read with Large v3. Other languages, models and recordings are untested.
- The second reading only happens when a piece loops. Speech that one reading simply misses (the noise-reduced copy can drop quiet or distant speakers) is not recovered; "Original microphone" in Re-generate is the way to get it.
- Transcribing a long recording takes longer when pieces are read twice (about 17 minutes for the 88-minute test on an M4 Pro with Large v3), and longer still on slower Macs. Interrupted runs resume from the last finished piece.
- The Whisper translate task and speaker detection are not checked or split into pieces yet.
- The Intel build is still not tested on a real Intel Mac (see 0.2.1).
- The known limitations of 0.2.0 still apply.

## [0.2.1] - 2026-10-28

0.2.1 adds Intel Mac support and nothing else: no new feature and no redesign.

### Added

- A separate download for Intel Macs, `CaptureMann-v0.2.1-macos-x86_64.dmg` (macOS 13 or later), next
  to the unchanged Apple Silicon one. Each download holds one chip only, so the Apple Silicon app does
  not grow. Homebrew picks the right one for your chip. **The Intel build has not been tested on a real
  Intel Mac yet**: it was built and checked on an Apple Silicon Mac (including running it under
  Rosetta). Recording sources, the hardware encoder, permissions and the speed of the speech and text
  models on Intel hardware are untested; expect the models to be slow on older Intel Macs.

### Changed

- The built-in text engine runs on the CPU on Intel Macs (Metal is used on Apple Silicon).
- The release workflow builds both downloads and is started by hand; pushing a tag no longer starts it.

### Known limitations

- The Intel build has not been tested on a real Intel Mac (see above). It is built on Apple Silicon,
  so the first reports from Intel hardware may find problems that this release does not know about.
- The built-in text engine and the speech models run on the CPU on Intel Macs; the default model
  choices were made for Apple Silicon and have not been measured on Intel.
- Everything in the 0.2.0 list below still applies.

## [0.2.0] - 2026-10-27

0.2.0 (voice and meetings). The maintainer sets the date when it is cut. Everything below is built and
tested with scripted fakes; the real speech, speaker and text engines were also run through the CLI on
one 11-minute meeting (Oct 27, 2026). The maintainer used the latest build on one Mac and reported that
recording, screenshots and audio work, and accepted the speaker split on one meeting as working, but
no hardware run is stored in `verification/` yet. See Known limitations.

### Added

- Audio-only recording: a third capture mode next to screenshots and screen recordings. The Record
  sheet has a Screen / Audio switch with five presets (In the room, Online meeting, Lecture or
  talk, Product demo, Voice note), the microphone, system audio or both, a compact HUD with a mic
  meter, Pause, Mute, Marker and Discard, ⌥⇧V to start or stop and ⌥⇧M to add a marker. The
  microphone alone never asks for Screen Recording. A consent line ("Let everyone know they're
  being recorded.") shows before recording. Recordings appear under a new Audio item in the library.
- Markers: add a moment while recording (or later with a time) and see it in the editor and in
  reports. `add_marker` and `capturemann record marker`.
- An editor for audio recordings: a "now speaking" card, lanes for markers, speakers and one
  waveform per track, cuts and suggested cuts, and a Transcript tab with speaker turns and languages.
- Export audio: M4A or WAV, the mix or one track, loudness evened out to -16 LUFS for M4A, tags,
  an optional copy to a folder; also "Export audio only" for screen recordings.
  `capturemann export <id> --audio`.
- Translation: captions to English with Whisper (`large-v3`, offline) and to other languages with the
  built-in text model; one translation per language, edited and exported like the original.
  `capturemann ai translate`.
- A text engine built into CaptureMann (`llama-server`, MIT, bundled in the app) and a model list on
  the AI page, downloaded on request and run on this Mac: Gemma 4 E2B is the default and recommended
  on every Mac, Gemma 4 26B-A4B (about 15 GB) is a higher-quality option shown only on Macs with 32 GB
  of memory or more, Qwen3.5 4B is optional (best for translation into other languages) and Qwen3.5 2B
  is the smallest. A model that needs more memory than this Mac has free shows a warning. Press Use to
  turn it on; there is a Test button and a Remove that deletes the file. There is no remote or cloud
  option, and no Ollama or LM Studio (a server of your own is a later item). The glossary now lives on
  the AI page.
- Reports from a transcript with the built-in text model: meeting minutes, interview notes, one-on-one
  notes, lecture notes, product recap and summary, with times on every item that jump to that
  moment, editing, regenerating and export as Markdown or text. `capturemann ai report`.
- Speaker labels (who spoke when), renaming, merging and reassigning speakers, and speaker names
  in exports and reports. The two speaker models (about 36 MB) are included in the app, so there is nothing to download or
  set up and it works offline. Accuracy has not been measured; the maintainer found it working on one
  meeting and more testing with other people is planned. They are found automatically after each
  transcript (Settings > AI > "Find speakers after each transcript", on by default), using "How many
  people?" from the Record sheet (`start_recording` `people`, `capturemann record start --audio --people N`)
  or the preset's guess; a recording that already has speakers is left alone.
- Whisper models `large-v3` (translation to English) and `large-v3-turbo` (fast transcription, not
  for translation). Large v3 Turbo is the recommended speech model on the AI page.
- The Report view: every item has time chips that jump to the moment, owners, action-item checkboxes,
  a column with the cited moment and a player, Copy, and export to Markdown or TXT with choices to tick.
  After a transcript a toast asks "Write a report?". While a report is written the progress bar follows
  the model (see Fixed).
- An audio recording opens its editor straight away and has no details page; the editor's top bar has
  Reveal in Finder where screen recordings have Recording details.
- The Vietnamese strings added for this release were read through by the maintainer.

### Changed

- The sidebar version badge follows the build (it shows v0.2), instead of a fixed v0.1.
- Release size: the app is about 180 MB. The AI models are not in it and download only when you press
  Download (about 4.4 GB for Large v3 Turbo plus Gemma 4 E2B); only the two small speaker models are inside.

### Fixed

- The progress bar of a report (and of a long translation batch) moved only between model calls, so a normal meeting sat at 0 % and then jumped to done. The text model's answer is now streamed and the bar follows the tokens as they arrive; it still never goes back and reaches 100 % only at the end, and cancelling stops the stream.
- A missing FFmpeg component during audio export is reported as `job_failed` with a reason that
  names the component (it was an `internal` error).

### Known limitations (0.2.0)

- Speaker accuracy has not been measured against labelled recordings (spike S7 is a technical Go
  only); it worked on one 11-minute English meeting for the maintainer, and testing with users and the
  team is planned over the coming weeks. Gemma 4 E2B is the default text model; its quality on your own
  recordings is still being checked (spike S8).
- No first-use setup sheet yet (one place to download the speech and text models when you first
  record); it comes in 0.2.1. Until then the AI page is where models are downloaded.
- Live transcript while recording (VOC-3) is not included; it comes in a later 0.2.x.
- Intel Macs are not supported yet (Apple Silicon and macOS 13+ only); Windows and Linux follow later.
- Ollama and LM Studio (a text model from a server of your own) are not offered; they are a later item.
- The bundled LGPL FFmpeg (built with `make ffmpeg` on Oct 27, 2026) has the `wav` muxer and the
  `pcm_s24le` encoder that WAV export needs; the export itself still awaits its hardware check.
- Hardware reports: the audio recording and the audio export checks (VOC-1, VOC-2) were run by the maintainer on one Mac and reported PASS on Oct 27, 2026 (`verification/VOC-1-2026-10-27-maintainer.md`, `VOC-2-2026-10-27-maintainer.md`); measured numbers (call sync, soak memory and drift, loudness) were not stored, and the AI checks (AI-5, AI-7, AI-8, AI-9) have no stored report yet. What is known otherwise is the maintainer's own use on one
  Mac and the CLI runs above. Manual scripts are in `tests/manual/`.
- A library from 0.1.0 is upgraded when 0.2.0 opens it (the index moves from version 2 to 3; bundles are
  not changed). This was tried on a synthetic 0.1.0 library only; make a copy of a large real library
  before you open it with 0.2.0, and do not open it with 0.1.0 afterwards.

## [0.1.0] - 2026-10-26

First public release. macOS 13+ on Apple Silicon only; Intel Macs follow after 0.1.0, then Windows and Linux.

### Added

- Screenshots of a display, a window or a region, with an annotation editor (arrows, shapes, text,
  pen, highlight, blur) and Auto-redact for emails, keys and tokens, phone numbers and IPv4 addresses.
  The window picker (⌥⇧2) shows live thumbnails of the open windows.
- Screen recording up to the display's native resolution at 30 or 60 fps, with a webcam bubble
  (with its own toolbar for size, shape, mirror and camera off), microphone, system audio and cursor
  telemetry; pause, resume, restart, discard, and crash recovery that loses at most two seconds.
  The start sheet, countdown, recording HUD and a menu-bar icon that shows when the app needs
  attention (for example a missing permission or a failed upload).
- Recording editor: trim, cut, split, speed, background and padding, zoom regions (with a label),
  cursor styling and click effects, audio mix, 9:16 and 1:1 reframe. Changes are non-destructive and
  undoable. Export to MP4, platform presets (YouTube 1080p, 1440p, 4K, vertical, square), GIF and
  WebM through a bundled LGPL FFmpeg.
- Local AI: Whisper transcripts and captions (SRT, VTT, TXT, or burned into the video), a caption
  editor (split at the cursor, find and replace, play this line), a glossary for names and terms,
  and suggestions for zoom regions and for cutting silences and filler words. Models download only
  when you press Download. Word times come from the model's DTW alignment and never start before
  the first sound, so captions line up with the speech even when a recording opens with silence.
- Library of `.cmshot` and `.cmrec` bundles with search and Uploaded and Published filters; bundles
  added or changed outside the app are picked up.
- Storage to local folders and Google Drive (your own Google client), and publishing to YouTube
  (private by default, asks for confirmation every time).
- Every feature as an MCP tool (55 tools, plus resources and prompts) and a `capturemann` command
  line, with per-client permission tiers and on-screen confirmation for recording, publishing and
  deleting.
- Per-tool agent permissions (Settings, Agents & CLI): mutating tools can be set to Allow or Ask first, and starting a recording can be allowed without asking; publish and delete always ask.
- English and Vietnamese interface (Settings, General, Language).
- No telemetry; no network traffic unless you connect a destination or download a model.
- Release builds are a signed and notarized DMG with a SHA-256 file on GitHub Releases.

### Known limitations

- macOS 13 or later on Apple Silicon (arm64) only. Intel Macs and Windows are not supported yet;
  Windows is planned after this release.
- Minimized windows are not listed in the window picker.
- The release DMG is signed with a Developer ID certificate and notarized by Apple, so it opens
  without a Gatekeeper warning. A build you make yourself from source is signed with a local
  identity or ad hoc and is not notarized: on first launch right-click the app and choose Open, or
  run `xattr -dr com.apple.quarantine /Applications/CaptureMann.app`. Each ad hoc build has a
  different signature, so macOS asks for the Screen Recording permission again after you install
  a new one.
- Updates are manual: download the new DMG from GitHub Releases. There is no update check.
- Only Google Drive and YouTube are offered. OneDrive and Dropbox are built but shown as "Coming
  soon"; TikTok and Facebook Pages are not included. Contributions are welcome.
- AI titles, descriptions and chapters are not included; there are no API keys and no text model in
  this release.
- Most features are tested with scripted fakes. Hardware paths were checked by the maintainer on one Mac
  (Apple M4 Pro, macOS 26) and are not stored as reports; the 60 fps self-test, long soaks and sync
  measurements are not done. Drive and YouTube were not run against a real account in automated
  tests. See `specs/04-delivery-plan.md`, "Verification debt".
