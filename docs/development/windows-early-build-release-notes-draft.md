# Windows early build: release notes (DRAFT, not published)

Draft of Nov 2, 2026 for the maintainer. Fill the lines marked **[after the laptop test]** from the reports of the
ThinkPad run (`verification/`), and delete any claim that run did not support. Nothing here has been posted. The
process is `docs/development/releasing.md`.

---

# CaptureMann 0.2.6 for Windows (early build)

The first Windows build. It is an early build: screenshots, annotation, the library, the command-line client and the
MCP server work; **recording is not available yet**. The files are **not signed**: Windows SmartScreen will warn
before it runs them (more info, run anyway).

## What works

- **Screenshots** of a region, a window or a whole display. Window screenshots are cut to the window's visible frame.
- **Editor and annotations**, copy and save, the library with search and bundles, in English and Vietnamese.
- **`capturemann` command line and the `cm-mcp` server** for agents; they start their own background daemon (on a named
  pipe) when the app is not running.
- Secrets (sign-in tokens, keys) go in the Windows credential store.

## What does not work yet

- Screen and audio recording, the webcam, and MP4 export of recordings.
- Auto-redact: there is no Windows text recognizer yet. It works when `tesseract` is installed
  (`winget install UB-Mannheim.TesseractOCR`); the app says so when it is missing.
- Speaker labels and the bundled FFmpeg are not in this build (the speaker engine is built for macOS and Linux only).
- No code signing yet: SmartScreen warns.
- The app names the window picker shows for packaged apps (Notepad, Settings) are the program's path or host name.

## Install

Windows 11, x64 (the runner and the test laptop). Windows 10 is not tested.

- **Installer** `capturemann-0.2.6-windows-x64-setup.exe`: per user, no administrator rights.
- **Zip** `capturemann-0.2.6-windows-x64.zip`: unpack anywhere and run `cm-desktop.exe`; `capturemann.exe` is the
  command-line client.
- Check the download with the `.sha256` file (`certutil -hashfile <file> SHA256` and compare).

## Tested on

- Automated tests on Windows (GitHub's `windows-latest`, Windows Server 2025): build, clippy, the full test suite,
  real-display tests (display and region sizes, window capture equals its bounds), a smoke script on the packaged
  files, silent install and uninstall of the installer: all passed. Report:
  `verification/L1-windows-package-ci-2026-11-01-claude.md` (PARTIAL: a CI runner is not a physical PC).
- A Lenovo ThinkPad T490 (Intel UHD 620, 1920x1080 at 150 %) with Windows 11 Pro: the zip and the installer install,
  start and uninstall cleanly; display and window screenshots, the window picker, the result toast and the tray icon
  work; the smoke script passes. Not run: SmartScreen on a real download, two monitors, Windows 10, region capture and
  the editor on this machine. Report: `verification/L1-windows-thinkpad-2026-11-02-claude.md` (PARTIAL).
- Not tested: Windows 10, ARM, high-DPI mixes of displays, a second monitor.

## Known issues

Report problems at the issue tracker with the output of `capturemann diagnostics`.

## Licences

Apache-2.0. The packages carry `THIRD_PARTY_LICENSES.md`; no GPL code is in the binaries.
