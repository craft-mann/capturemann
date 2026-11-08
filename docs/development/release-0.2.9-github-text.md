# GitHub release text for 0.2.9 (draft, not published)

Prepared Nov 6, 2026 from `specs/plans/release-0.2.9-plan.md`, `git log v0.2.8..HEAD` and the 0.2.8 text
(`release-0.2.8-github-text.md`). **Nothing here is final until the regression reports exist.** Every `<FILL: ...>` below is
a placeholder to fill from a stored report in `verification/`; if there is no report for a line, delete the line or write
"not run". Never write "passed" or "verified" without a report (CLAUDE.md wording rule). Updated Nov 8, 2026: the table
below is filled from the reports in `verification/` (`L1-windows-0.2.9-*`, `L1-macos-0.2.9-*`, `VOC-1-soak-*`); what is still
a `<FILL>` is a decision or a number that only exists on the release day (checksums, notarization, winget, the manual runs
by the maintainer). Steps: `release-day-0.2.9.md`.

- **Tag:** `v0.2.9` (never reuse a tag). **Title:** `CaptureMann 0.2.9: fixes on Windows and macOS`.
- **Type:** draft, then **pre-release** (the version is 0.x). One tag, released when both platforms have a stored
  regression run (decision of Nov 7, 2026: Linux moved to 0.3.0).
- **Files to attach:**
  - Windows: `capturemann-0.2.9-windows-x64-setup.exe`, `capturemann-0.2.9-windows-x64.zip`, their two `.sha256` files and
    `SHA256SUMS.txt` (from `packaging/windows/release-files.sh`). Not code-signed.
  - macOS: `CaptureMann-v0.2.9-macos-arm64.dmg` and `CaptureMann-v0.2.9-macos-x86_64.dmg` with their `.sha256` files (made by
    the `Release` workflow; signed, notarized and stapled if the notarization secrets are set, otherwise say "not notarized").

---

## Release body

# CaptureMann 0.2.9: fixes on Windows and macOS

0.2.9 is a bug-fix release. It fixes five problems reported on 0.2.8 and earlier, polishes a few edges, and has **files for
Windows 11 (x64) and macOS 13+ (Apple Silicon and Intel)**. The one addition: **you can drag the recording HUD to where you
want it and collapse it** (see "Improved"). **macOS users also get everything that came in 0.2.8** (see
"Also new for macOS" below), because 0.2.8 was Windows only. Linux stays at 0.2.6; the next Linux release is 0.3.0.

## Fixed

1. **Reveal in the export toast** opens the folder that holds the exported file, with the file selected (File Explorer and
   Finder). On Windows the
   button opened the wrong folder.
2. **Screenshot annotation editor, Vietnamese text.** Typing with Telex or VNI in the Text tool now reaches the annotation
   (the tool leaves composition to the system input method), and the text is drawn and exported with its accents.
   **New text** starts at size 24, in white, with no outline; change the colour and size in the panel as before. Existing
   annotations are not changed. Typing also keeps working after you change the colour or size with the text box open.
   Typing through a real Telex or VNI input method has **not** been run by the test suite: the steps in
   `tests/manual/ANN-1-IME.md` are a manual run by the maintainer: <FILL: result of the maintainer's run on Windows and
   macOS, or "not run">. The Vietnamese glyphs of the bundled font are covered by a test.
3. **The Models page no longer says "Mac" on Windows.** The one-line model descriptions were written for the Mac
   ("every Mac", "8 GB Macs") and were English-only. They are now neutral ("every computer", "8 GB of memory") and are
   translated, including Vietnamese. A test fails if a model description in the catalogues names a platform.
4. **The Speakers tab** gives the speaker list the free height of the panel and scrolls when there are many speakers (before,
   about one and a half rows were visible on a laptop screen). The note "Voices are compared in memory only. No voiceprint is
   saved, and people are never recognised across recordings." moved to **Settings, Privacy**, as a fifth fact; nothing about
   how voices are handled changed. At the smallest window (720 x 480) the panel scrolls as a whole and nothing is cut. With
   one or two speakers the list is only as tall as its rows.
5. **Play on Windows no longer passes the file name through `cmd`.** A capture whose title contained `&`, `^` or `%` could
   make Windows run an extra command when you pressed Play. The file is now opened through File Explorer with the path
   quoted. (A bug found in a review of the changes since 0.2.8; no report of it being used.)

## Improved

- **Recording HUD: drag it, collapse it.** While recording (screen or audio) you can drag the HUD by its background, and a
  new button collapses it to Stop, the timer and Pause (and the agent chip when an agent started the recording). The
  position (per display) and the collapsed state are remembered; if the display is unplugged the HUD comes back to a
  visible place. Agents cannot change this setting.
- **Windows 11: the recording HUD no longer shows a grey frame** around its empty strip, and has no white line on its top edge.
- **Thumbnails** of screenshots and recordings (library, published list, publish sheet, recording details, window picker)
  follow the rounded corner of their frame; before, a cropped picture kept square corners that stuck out of the border.
- **Notifications:** the coloured stripe on the left of a toast no longer sticks out of the card's rounded corners.

The Windows test job on CI is also changed to run faster (it still takes about 48 minutes, against a 20 minute target); that
is internal and changes nothing you download.

## Security

GitHub code scanning (Trivy and Dependabot) reported 15 open alerts, all in one file: `packaging/phowhisper/requirements.txt`,
the Python requirements of a script the maintainer runs by hand to convert PhoWhisper checkpoints. **Nothing in the app, the
installers, the zip, the DMGs uses that file, so nothing in the files you download is affected and
there is nothing for you to update.** The pins were raised (torch 2.15.0, transformers 5.22.1 and their companions) and the
`small` model was converted again; the result is a byte-identical model file (same SHA-256), so the Vietnamese benchmark
still applies (`packaging/phowhisper/README.md`). One transformers alert (alert 8) has no patched version yet: the maintainer
dismisses it as "not used in shipped code" or it closes when a fix is published <FILL: state of the alert list on the
release day>. The converter README says to convert only the three pinned VinAI revisions in a throwaway virtual environment.

## Also new for macOS (from 0.2.8, which was Windows only)

macOS was last released at 0.2.7. Shared changes from 0.2.8:

- Two new agent tools with CLI commands: `get_cursor_track` (`capturemann cursor`) and `get_platform_rules`
  (`capturemann platform-rules`). The MCP server now lists 79 tools.
- `capturemann settings set` is followed by the running app at once (a `settings.changed` event), and the capture delay is
  read for each capture.
- A recording whose encoder cannot keep up is marked with an `encoder_overloaded` warning in its manifest and the drop
  counters are honest; a recording area that is too small is `invalid_input` (`recording_area_too_small`, smallest side
  18 px), not a missing encoder; a microphone that delivers only silence is reported after 4 s (`mic_silent`); the daemon
  publishes `camera.reconnected` when a camera comes back.
- Error and help texts name your system (Windows or Mac) instead of always saying "Mac".
- `ai transcribe` answers `no_model` when no speech model is installed, instead of taking a folder of speaker models for one.
- Vietnamese: the interface text was reviewed by the maintainer on Nov 6, 2026, except 86 entries that are still drafts.
- `cm-regress`, the release regression suite, now drives macOS packages as well as Windows
  (`docs/development/regression.md`).

The Windows-only items of 0.2.8 (recording on Windows, Auto-redact with Windows.Media.Ocr, installer, zip) are in the 0.2.8
release notes and are unchanged here.

## What was and was not run per platform

**To be filled from the stored reports; delete a cell's claim if its report does not exist.** All runs are by an agent on
one lab machine per platform unless a line says a person looked. A PARTIAL report is not "verified".

| Platform | Package (CI run, commit) | Smoke | Standard | Hardware | Report files |
| --- | --- | --- | --- | --- | --- |
| Windows 11 x64 (ThinkPad T490, Intel UHD 620) | installer from the Windows package run 42363837840, commit `ae61845f` (installed silently on the lab laptop) | 12 passed, 2 skipped | 49 passed, 7 skipped | 58 passed, **1 failed**, 18 skipped: `rec.camera` measured 10.1 fps in the evening (a low-light camera mode) and passed in daylight (9.9 s run, Nov 8); the full tier: 56 passed, 14 skipped | `verification/L1-windows-0.2.9-regression-{smoke,standard,hardware,full}-pkg-ae61845f-*`, `...-hardware-camera-again{1,2}-*` (the evening failures), `...-hardware-camera-day-*` |
| macOS arm64 (the maintainer's Mac, Apple M4 Pro, macOS 26.6.2) | the notarized DMG built on the maintainer's Mac on Nov 8, 2026 after commit `53a3cb4e` (a local `make release`), app copied out of the DMG for the runs | 12 passed, 2 skipped | 44 passed, 12 skipped | 53 passed, 0 failed, 24 skipped (screen, microphone, system audio and camera ran with the maintainer's consent); the DMG install check passed | `verification/L1-macos-0.2.9-regression-{full-dmg,smoke-app,standard-app,hardware-app}-53a3cb4e-*` |
| macOS x86_64 (Intel) | built and checked on Apple Silicon only | not run on Intel hardware | not run | not run | none |

**One hour of audio recording** (the case for long meetings), one run per platform, mic only on the Mac, mic plus system audio
on Windows, nobody speaking: 3602.6 s recorded on the Mac and 3601.6 s on Windows, no gaps, no resyncs, the app's memory
stayed flat (Mac 113 to 135 MB; Windows 42 to 43 MB after the first minute) and it did not crash. **Disk use is large: about
1.4 GB per hour on the Mac and 4.1 GB per hour on Windows** (raw mic, denoised mic and system audio are each stored as
32-bit float). Reports: `verification/VOC-1-soak-macos-1h-*`, `VOC-1-soak-windows-1h-*`. The Mac run and the Windows run used
the packages built before the HUD change.

The macOS runs above are on the DMG built on the maintainer's Mac. The DMGs attached to this release are built again by the
`Release` workflow from the tag: <FILL: result of the checks on the downloaded draft DMG (`releasing.md`), or "not rerun">.

Bug fixes: Reveal (EXP-4.AC5) and the text defaults (ANN-1.AC7, AC8) have unit tests with fakes; the Speakers tab layout and
the hint wording have layout and catalogue tests; the HUD drag and collapse have tests for the positions and the stored
state (REC-5.AC16, AC17). **The HUD, the thumbnails and the toast were seen by eye on the Windows laptop** (screenshots
of the demo recording and toast; Nov 7, 2026, not stored in the repository). On the Mac the maintainer looked at the
thumbnails and said the corners are right; the HUD on macOS has not been seen by eye (`tests/manual/HUD-1.md`:
<FILL: result of the maintainer's run, or "not run">).
CI on the tag commit (`cargo test --workspace` on macOS and Windows): <FILL: link or "see Actions">.

### Not run (same as 0.2.8 unless a report says otherwise)

- **Windows:** Windows 10 22H2; SmartScreen on a real browser download; a second monitor with mixed scaling
  (`tests/manual/DPI-WIN.md`); NVIDIA and AMD hardware encoders and drivers (only Intel UHD 620 was used); the Samsung
  monitor cases.
- **macOS:** the Intel DMG on a real Intel Mac (no Intel Mac is available; the Intel crash of issue 16 is not confirmed
  fixed on hardware); cases that were skipped: `shot.window`, `redact.window_text`, `rec.each_monitor`,
  `desktop.hud_click_through`, `desktop.recording_placement`, `desktop.settings_applied` (not built for macOS), a second monitor
  and a 4K external monitor, transcription and speaker steps (no models given to the run), and the long-recording soak case.
- **Windows (also):** the same second-monitor, installer-silent, transcription and speaker cases were not run.
- **All:** the invented outro-style sentence in transcripts of quiet stretches is still a known fault (no transcription
  mitigation in this release); playback follows the system's default output device.

## Install

- **Windows 11, x64:** installer `capturemann-0.2.9-windows-x64-setup.exe` (per user, no administrator rights) or the zip
  `capturemann-0.2.9-windows-x64.zip` (unpack anywhere, run `cm-desktop.exe`; `capturemann.exe` is the command line).
  winget: `winget upgrade CaptureMann.CaptureMann` <FILL: only once the winget package is accepted; pull request `<URL>`>.
  The files are **not code-signed**; a browser download may show a SmartScreen warning (More info, then Run anyway).
  SHA-256: installer `<FILL>`, zip `<FILL>`. Check with `certutil -hashfile <file> SHA256`.
- **macOS 13+:** `CaptureMann-v0.2.9-macos-arm64.dmg` (Apple Silicon) or `CaptureMann-v0.2.9-macos-x86_64.dmg` (Intel);
  <FILL: "signed and notarized" or "not notarized">. Or `brew upgrade --cask craft-mann/tap/capturemann`
  (`brew install --cask craft-mann/tap/capturemann` for a first install). SHA-256: arm64 `<FILL>`, x86_64 `<FILL>`.
  Check with `shasum -a 256 -c <file>.sha256`.
- **Linux:** no 0.2.9 files. Linux stays at 0.2.6; the next Linux release is 0.3.0.

Report problems at the issue tracker with the output of `capturemann diagnostics`.

## Licences

Apache-2.0. The packages carry `THIRD_PARTY_LICENSES.md`. FFmpeg is an LGPL build (no GPL code, no libx264); its source
offer is in the package. The Rust dependency set did not change from 0.2.8 (`Cargo.lock` differs only in the workspace
versions).

---

## CHANGELOG section (add under `## [Unreleased]`; `release.yml` reads it for the macOS run)

```
## [0.2.9] - YYYY-MM-DD

0.2.9 is a bug-fix release for Windows and macOS. macOS also receives the changes of 0.2.8 (Windows only). Linux stays at 0.2.6; the next Linux release is 0.3.0.

### Fixed
- Reveal in the export toast opens the folder of the exported file with the file selected.
- Annotation editor: Vietnamese text typed with Telex or VNI reaches the Text tool and is exported with its accents; new
  text starts at size 24, white, without an outline.
- The Models page no longer says "Mac" on Windows; the model descriptions are neutral and translated.
- The Speakers tab list scrolls and has a flexible height; the voiceprint note moved to Settings, Privacy.
- Play on Windows opens the file through File Explorer with a quoted path (a capture title with `&`, `^` or `%` could make
  Windows run an extra command).

### Improved
- Recording HUD: drag it by its background and collapse it; position and state are remembered.
- Windows 11: no grey frame or white line around the recording HUD.
- Thumbnails and notification stripes follow the rounded corners of their frame.

### Security
- The PhoWhisper converter requirements (`packaging/phowhisper/requirements.txt`, used only by a maintainer script, not
  shipped) were raised to patched torch and transformers; nothing in the shipped files changed.
```
