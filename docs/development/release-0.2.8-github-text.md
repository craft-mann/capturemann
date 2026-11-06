# GitHub release text for 0.2.8 (final text, not published)

Written Nov 5, 2026 from the stored reports in `verification/` (all PARTIAL: one agent, one laptop, nothing seen by a
person unless a line says so). It replaces `windows-0.2.8-release-notes-draft.md`. Paste "Release body" into the release
page. Only two things are left to fill: the two checksums (`<sha256 zip>`, `<sha256 installer>`, from the package run
you ship) and the winget line (after the pull request is merged). Lines marked **[confirm]** need a maintainer
decision; delete the marker. Nothing here has been posted. Steps: `windows-0.2.8-release-checklist.md` and
`release-day-0.2.8.md`.

- **Tag:** `v0.2.8` (never reuse a tag). **Title:** `CaptureMann 0.2.8: recording comes to Windows`.
- **Type:** draft, then **pre-release** (the version is 0.x).
- **Files to attach:** `capturemann-0.2.8-windows-x64-setup.exe`, `capturemann-0.2.8-windows-x64.zip`, their two
  `.sha256` files and `SHA256SUMS.txt` (made by `packaging/windows/release-files.sh`). Windows files are not signed
  (decision O4, Nov 5, 2026). **[confirm]** No macOS or Linux files are attached: `main` has no code change since
  0.2.7, and the shared changes below reach the Mac and Linux with their next builds (0.2.9 is Linux). If a DMG is
  built anyway, add the DMG lines and the CHANGELOG section at the end.

---

## Release body

# CaptureMann 0.2.8: recording comes to Windows

The Windows build now **records**: the screen, the cursor, the microphone, system audio and the camera. It also imports
video and audio, exports with a bundled FFmpeg, finds sensitive text with the recognizer built into Windows, and ships the
speaker engine and the local text engine. It is still an early Windows build. It was tested on **one laptop** (a Lenovo
ThinkPad T490, Intel UHD 620 graphics, Windows 11), the limits are listed below, and the files are **not signed**.

## What is new on Windows

- **Recording** of a display, a window or a region, with the cursor, microphone, system audio and camera; pause and
  resume; a crash or a killed process leaves a recording that is recovered at the next start. The screen is captured with
  Windows.Graphics.Capture and encoded by the Media Foundation hardware H.264 encoder.
- **System audio** through WASAPI process loopback, which leaves out CaptureMann's own sound on a recent Windows 10 or 11
  (older versions fall back to the plain loopback, which includes it).
- **Import** of video and audio files (editing copies are made by the hardware H.264 transform).
- **Export** with the bundled LGPL FFmpeg 9.0.2: the quick export, the YouTube 1080p preset, vertical, square, WebM and GIF.
  H.264 files are tagged BT.709.
- **Auto-redact** with `Windows.Media.Ocr`: no extra program to install. It reads the languages installed in Windows.
- **Speaker labels** (sherpa-onnx engine) and **local transcription and the built-in text model** (whisper.cpp built for a
  portable AVX2 CPU, `llama-server` v0.7.0, the same pin as on macOS). The two speaker-label models are bundled
  (`Resources\Models\speaker`; pyannote segmentation-3.0, MIT, and CAM++ from 3D-Speaker, Apache-2.0; about 35 MB; the
  licences and notices ship with them). Speech models are downloaded or imported by you, as on the Mac.
- **Installer** (per user, no administrator rights) and the zip; the `capturemann` command line and the `cm-mcp` server.
- **Names and sizes that were wrong in 0.2.6** are fixed: displays are named ("Display 1", not "Unknown Monitor 65537"),
  packaged apps show their real names (Notepad, Settings, Windows Terminal), and a window screenshot has the size the picker
  listed (the app now runs per-monitor DPI aware). The interface uses Windows' own words (File Explorer, Recycle Bin,
  Credential Manager, notification area).
- **Diagnostics** (`capturemann diagnostics`) lists the Windows build, each graphics adapter with its driver, the recognizer,
  FFmpeg, `llama-server` and the speaker engine, and says what this build includes.

## Also in this release (all platforms; the Mac and Linux get it with their next builds)

- Two new agent tools, with CLI commands: `get_cursor_track` (`capturemann cursor`) and `get_platform_rules`
  (`capturemann platform-rules`). The MCP server now lists 79 tools.
- `capturemann settings set` is followed by the running app at once (a `settings.changed` event), and the capture delay is
  read for each capture.
- A recording whose encoder cannot keep up is marked with an `encoder_overloaded` warning in its manifest, and the drop
  counters are honest; a recording area that is too small is `invalid_input` (`recording_area_too_small`), not a missing
  encoder; the daemon publishes `camera.reconnected` when a camera comes back.
- Error and help texts name your system (Windows, Linux or Mac) instead of always saying "Mac".

## Fixes found on the test laptop

Every recording ended in a crash at stop (a stop that ran twice); the app panicked at start (a named pipe created outside a
runtime); Quick Sync refused the video format used for import copies and exports (NV12 is used now); a slowly changing screen
was reported as 54 % late frames; the area window swallowed clicks meant for the recording bar (it is click-through now); the
countdown, bar and area appear on the monitor being recorded; the window picker and the helper overlay end themselves
when the app is gone; `settings set` did not reach the running app; "Microsoft Basic Render Driver" was shown as
"Basic [redacted] Driver" in diagnostics; the Windows Terminal showed as "Windows Terminal Host"; helper programs no longer
open a console window; the CLI no longer hangs when it starts the daemon.

## Known limitations

- **Not run:** Windows 10 22H2; a second monitor with mixed scaling (steps in `tests/manual/DPI-WIN.md`); SmartScreen on a
  real browser download.
- **Hardware:** tested on one ThinkPad T490 with an Intel UHD 620 only. **NVIDIA and AMD hardware encoders and drivers are
  untried.** A computer without a hardware H.264 encoder cannot record (the message says so).
- **Camera:** replugging a camera into a **different USB port** may give it another device id; the recorder reopens the same
  id only, so it may not resume. In a dark room the camera may drop its frame rate (10 fps in a dark room, 29.8 to 30.0 fps at
  720p in a lit room).
- **Second monitor:** on a 3840x2160 monitor at 150 % next to the panel, screenshots and a 10 s recording matched the listing
  and the window picker works on both monitors. The launcher and the start sheet always open on the primary display, and the
  recording bar and the camera bubble cannot be dragged or snapped on Windows. A 4K recording on the UHD 620 could not keep
  up in an earlier test (it now carries an `encoder_overloaded` warning; the case was not run again).
- **Smallest recording area:** 18 px per side (16 failed on the test laptop's encoder). Other GPUs may need more.
- **Sound and picture timing:** the recorded system audio lands about 15 to 35 ms after the picture in the best test, but
  that test cannot separate the product from its own stimulus, so "within 40 ms" is **not shown**. Microphone timing was not
  measured.
- **Text recognition:** only languages installed in Windows are recognized; en-US was the one checked.
- **Transcription:** CPU only (no GPU path on Windows). On the test laptop 50 s of speech took about 3 to 4 minutes with the
  `small` model end to end. The two speaker model files are bundled in the package; in the lab test (which used imported copies of
  the same files), two synthetic voices were separated when the count was given, and merged into one when it was left to
  "auto". **[confirm after the CI build: the bundled models are found without importing]**
  **An invented outro-style sentence over quiet stretches is still a known fault** (see the 0.2.7 notes and
  `ai.transcribe_raw_mic`). **[confirm: say this only if no transcription mitigation is merged for 0.2.8]**
- **Region picker:** it could not be seen or driven by the automated runs (the window picker was looked at by the
  maintainer on both monitors).
- **Vietnamese:** the full Vietnamese interface text was reviewed by the maintainer on Nov 6, 2026, except 86 entries (new Windows and Linux wording) that are still marked as drafts.
- **Not signed** (see Install).

## Install

Windows 11, x64. Windows 10 (22H2) is listed as supported but has not been tested on a real device yet; all testing so far was on Windows 11 with an Intel processor.

- **Installer** `capturemann-0.2.8-windows-x64-setup.exe` (per user, no administrator rights). SHA-256 `<sha256 installer>`
- **Zip** `capturemann-0.2.8-windows-x64.zip`: unpack anywhere and run `cm-desktop.exe`; `capturemann.exe` is the command line.
  SHA-256 `<sha256 zip>`
- **winget** (once the package is accepted): `winget install CaptureMann.CaptureMann` **[add after the pull request is merged]**
- Check a download: `certutil -hashfile <file> SHA256` and compare with the sum above (or `SHA256SUMS.txt`).

**Windows may show a SmartScreen warning for a file downloaded in a browser**, because the files are not code-signed
(decision of Nov 5, 2026: no signing while CaptureMann is 0.x). Choose **More info, then Run anyway**. Installing with winget
normally avoids it.

## How it was tested

- GitHub's `windows-latest`: build, the package, a smoke script, silent install and uninstall
  (`verification/L1-windows-package-ci-2026-11-01-claude.md`, PARTIAL).
- One ThinkPad T490 (Intel UHD 620, Windows 11 build 26200), driven by an agent over ssh and the desktop session, with the
  release regression suite `cm-regress` on package run 42010089424: standard tier 48 passed, 0 failed, 1 skipped; hardware
  tier 58 passed, 0 failed, 8 skipped (second monitor, 4K external, transcription, installer and tray cases were not run
  in those runs). Also a 621 s run with screen, microphone, system audio, camera and cursor at 1080p30 (0.011 % late screen
  frames, keyframes every 2.0 s, about 5.5 % of the machine's CPU), `kill -9` recovery, speaker labels and transcription on
  synthetic speech. Reports (all PARTIAL): `verification/L1-windows-0.2.8-*` and `verification/L1-windows-0.2.7-regression-*`.
  **[confirm: the shipped package is a later build than run 42010089424; rerun `regress.sh` on it and update this
  paragraph]**
- Not tested: Windows 10, ARM, mixed-DPI monitors, NVIDIA or AMD graphics.

Report problems at the issue tracker with the output of `capturemann diagnostics`.

## Licences

Apache-2.0. The packages carry `THIRD_PARTY_LICENSES.md`. FFmpeg is an LGPL build (no GPL code, no libx264); its source
offer is in the package.

---

## Optional: CHANGELOG section (only if `release.yml` is used for a macOS build in the same tag)

```
## [0.2.8] - 2026-11-DD

0.2.8 is the Windows release: recording, import, export, Auto-redact, speaker labels and the text engine on Windows
(Windows 11 x64, unsigned, tested on one laptop). macOS and Linux get the shared changes below with their next builds.

### Added
- Windows: recording of screen, cursor, microphone, system audio and camera; Import; export with a bundled LGPL FFmpeg;
  Auto-redact with Windows.Media.Ocr; speaker engine and `llama-server`; NSIS installer and zip.
- MCP tools and CLI commands `get_cursor_track` (`capturemann cursor`) and `get_platform_rules` (`capturemann platform-rules`).
- `settings.changed` and `camera.reconnected` events; `encoder_overloaded` recording warning.
- `cm-regress`, a release regression suite that drives the shipped package.

### Changed
- Messages and help use the system's own words (Windows, Linux, Mac).
- A recording area that is too small is `invalid_input` (`recording_area_too_small`).

### Fixed
- See the Windows release text: stop crash, start-up panic, Quick Sync format, late-frame count, HUD clicks, monitor placement,
  window size at 150 % scaling, display and app names, console windows, CLI hang.
```
