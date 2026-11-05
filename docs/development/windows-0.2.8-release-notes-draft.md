# Windows 0.2.8: release notes (DRAFT, not published)

**Superseded on Nov 5, 2026 by `release-0.2.8-github-text.md` (final text) and `release-day-0.2.8.md` (steps).** Kept for history.

Draft of Nov 5, 2026 for the maintainer. Facts only, taken from `docs/project/status.md`,
`specs/plans/release-0.2.8-plan.md` ("Known limitations of the Windows 0.2.8 release") and the stored reports in
`verification/` (`L1-windows-0.2.8-hardware-2026-11-05-claude.md`, `L1-windows-0.2.8-round2-2026-11-05-claude.md`,
`L1-windows-0.2.8-morning-summary-2026-11-05-claude.md`; all PARTIAL). Wording rule: nothing is called "verified";
the hardware claims are "tested on one ThinkPad T490 (Intel UHD 620, Windows 11)". Fill the placeholders at release
and delete any line a later run did not support. Nothing here has been posted. Steps: `windows-0.2.8-release-checklist.md`.

---

# CaptureMann 0.2.8 for Windows

Recording comes to Windows. The Windows build now records the screen, the cursor, the microphone, system audio and
the camera, imports video and audio, and finds sensitive text with the recognizer built into Windows. It is an early
Windows build: it was tested on one laptop, and the list of what is not covered is below. The files are **not
signed**: Windows SmartScreen may warn before it runs a file downloaded in a browser.

## What is in the Windows build

- **Recording** of a display, a window or a region, with the cursor, the microphone, system audio and the camera, pause
  and resume, and recovery after a crash. Capture uses Windows.Graphics.Capture; video is encoded by the Media Foundation
  hardware H.264 encoder.
- **System audio** through WASAPI process loopback, which leaves out CaptureMann's own sound on a recent Windows 10 or 11
  (older ones fall back to the plain loopback, which includes CaptureMann's sound).
- **Import** of video and audio files, with editing copies made by the hardware H.264 transform.
- **Export** with the bundled LGPL FFmpeg 9.0.2: the quick export, the YouTube 1080p preset, WebM and GIF.
- **Auto-redact** with the recognizer built into Windows (`Windows.Media.Ocr`): e-mail addresses, keys and IP addresses
  were found in the test run.
- **Speaker labels** with the bundled sherpa-onnx speaker engine, and **local transcription and the built-in text
  model** through the bundled `llama-server` (v0.7.0, the same pin as on macOS).
- Screenshots, the editor and annotations, the library, the `capturemann` command line and the `cm-mcp` server, in
  English and Vietnamese, as in the early build. Secrets stay in the Windows credential store.

## What is not there, or limited

- **Not run:** Windows 10 22H2; a second monitor with mixed scaling (steps in `tests/manual/DPI-WIN.md`); SmartScreen on
  a real browser download.
- **Hardware:** tested on one ThinkPad T490 with an Intel UHD 620 only. NVIDIA and AMD hardware encoders and drivers are
  untried.
- **Camera:** replugging a camera into a different USB port may give it another device id; the recorder reopens the same
  id only, so it may not resume. A camera in a dark room may drop its frame rate (10 fps was measured in a dark room;
  29.8 to 30.0 fps at 720p in a lit room).
- **Multi-monitor:** the launcher and the start sheet always open on the primary display, and the HUD and the camera
  bubble cannot be dragged or snapped on Windows. On a second monitor (3840x2160 at 150 %) the screenshots and a
  10 s recording matched the listing, but the countdown, the HUD and the area brackets were placed as if that monitor
  started at 0,0.
- **Text recognition:** only the languages installed in Windows are recognized; en-US was the one checked.
- **Preview and export differ:** the preview-against-export comparison test (`cm-desktop__parity`) fails on Windows
  (mean error about 7 against a limit of 3).
- **System audio timing:** a flash and beep test put the recorded beep about 87 ms after the flash with no drift; that
  includes the test's own latency, so sync within 40 ms is not shown.
- **Transcription speed:** the `medium` model ran at 0.63 times real time on the CPU (8 threads); there is no
  Metal-style GPU path on Windows. The text-model download rows are still unpinned (`pending_pin`) until the maintainer
  pins them. **[confirm before release]**
- **Not signed:** see "Install". The region picker could not be seen or driven by the automated runs.

## Install

Windows 11, x64. Windows 10 is not tested.

- **Installer** `capturemann-0.2.8-windows-x64-setup.exe`: per user, no administrator rights. SHA-256 `<sha256 installer>`.
- **Zip** `capturemann-0.2.8-windows-x64.zip`: unpack anywhere and run `cm-desktop.exe`; `capturemann.exe` is the
  command-line client. SHA-256 `<sha256 zip>`.
- **winget** (once the package is accepted): `winget install CaptureMann.CaptureMann`. **[add after the winget pull
  request is merged]**
- Check a download with `certutil -hashfile <file> SHA256` and compare it with the sum above.

**Windows may show a SmartScreen warning for a file downloaded in a browser** because the files are not code-signed
(decision of Nov 5, 2026: no signing while CaptureMann is 0.x). Choose **More info, then Run anyway**. Installing with
`winget install CaptureMann.CaptureMann` avoids it.

## Tested on

- Automated tests on Windows (GitHub's `windows-latest`): build, the package, a smoke script, silent install and
  uninstall. Reports in `verification/` (PARTIAL: a CI runner is not a physical PC).
- **One ThinkPad T490 (Intel UHD 620, Windows 11, build 26200.9711)**, driven by an agent over ssh and the desktop
  session, nothing seen by a person: screen recording with the microphone, cursor, pause, resume, window and region,
  `kill -9` recovery, import, export, Auto-redact, speaker labels, and a 621 s run with screen, microphone, system audio,
  camera and cursor at 1080p30 (0.011 % late screen frames, keyframes every 2.0 s, daemon CPU about 5.5 % of the
  machine). Reports: `verification/L1-windows-0.2.8-hardware-2026-11-05-claude.md` and
  `verification/L1-windows-0.2.8-round2-2026-11-05-claude.md` (PARTIAL).
- Not tested: Windows 10, ARM, mixed-DPI monitors, NVIDIA or AMD graphics.

## Known issues

Report problems at the issue tracker with the output of `capturemann diagnostics`.

## Licences

Apache-2.0. The packages carry `THIRD_PARTY_LICENSES.md`; the FFmpeg is an LGPL build, no GPL code is in the binaries.

## What the maintainer still has to do

- Decide the open point marked **[confirm before release]** and the text-model rows still `pending_pin`.
- Run the package workflow on the release commit, fill `<sha256 zip>` and `<sha256 installer>`, and publish the
  release; submit the winget manifest. All steps are in `docs/development/windows-0.2.8-release-checklist.md`.
- Optionally look at what no one has seen by eye (the region picker, SmartScreen on a real download, a second monitor).
