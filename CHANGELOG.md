# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project follows [Semantic Versioning](https://semver.org/) once it reaches 1.0.

## [Unreleased]

### Fixed

- A long recording no longer fails its report with `invalid_model_output`. When one piece's notes did not fit in the model's reply limit, the JSON was cut off and the retry had no room left; the piece is now split in half (or, for the merge, the notes are kept side by side). Seen on a 2.5-hour Vietnamese recording. 0.2.0 and 0.2.1 have the problem.

- The audio editor of a long recording plays again right away. Mixing the preview of an 88-minute recording took about 45 seconds and 12 GB of memory (the exporter's loudness measurement and true-peak limiter ran over the whole mix), and Play was silent until it finished; it now takes under a second and about half the memory. Exports are unchanged.
- The sidebar highlights Audio, not Recordings, while an audio recording is open in the editor.
- After a long recording the Transcript and Speakers tabs now say that the transcript and the speaker labels are being made, with the progress, and that nothing needs pressing. The automatic transcription was invisible to the editor (the new `transcript.started` event tells it), so the tabs looked idle for minutes.

### Changed

- Report exports leave out the citation times by default: Markdown, TXT and Copy no longer print `[mm:ss]` after every item. A new box "Include citation times" under the report (and `include_times` / `capturemann ai report export --times`) adds them. The marked moments keep their times; the report view still shows every citation and seeks.

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
