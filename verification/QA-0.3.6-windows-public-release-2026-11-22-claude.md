# QA 0.3.6, Windows 11, the PUBLIC release files, installed (PARTIAL)

- **Date:** 2026-11-22 (UTC+7). **Who:** Claude (post-release agent), driven from the Mac. **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 (build 26200), one display, user `trung` on the console.
- **Files:** downloaded on the laptop with `curl.exe` from https://github.com/craft-mann/capturemann/releases/download/v0.3.6/ . Sizes: setup 92063594 bytes, zip 121700198 bytes.
- **Hashes** (Get-FileHash on the laptop, equal to `SHA256SUMS.txt` and to the expected values): setup `60a4c5bfd9f5bc913bc90c284218a32a5ebc919b7a023b0e391e562b71e92ddb`, zip `e02dd69d94adaa6231a0db16f76e7a2fb78715cb889360724d82cbe3ac11a7c5`. Identical to the CI files tested on Nov 22 (`QA-0.3.6-windows-ci-package-1778431b-*`).
- **Before:** nothing CaptureMann installed (no uninstall entry, no folder, no CaptureMann process, no lock).

## Signing
Get-AuthenticodeSignature: setup `NotSigned`; `capturemann.exe` and `cm-desktop.exe` `NotSigned`; the zip is not a signable file (`UnknownError`, normal). The files are unsigned as documented. The mark-of-the-web (Zone.Identifier) that a browser adds cannot be reproduced by curl (the file has only the `:$DATA` stream), so what SmartScreen says to a browser download was not seen (not run).

## Install and zip
- Silent install (`/S /D=C:\lab\q36p\inst`): exit 0; folder holds doc, FFmpeg, Llama, Resources, app.ico, capturemann.exe, cm-daemon.exe, cm-desktop.exe, cm-mcp.exe, cm-overlay.exe, ffmpeg.exe, ffprobe.exe, llama-server.exe, path.ps1, README.txt, uninstall.exe. Uninstall entry `CaptureMann 0.3.6`, InstallLocation the lab folder. `capturemann --version` = `capturemann 0.3.6`.
- Portable zip unzipped into its own folder (top folder `capturemann-0.3.6-windows-x64`): `capturemann.exe --version` = `capturemann 0.3.6`; `--help` prints the full command list. File list equals the installed one except `uninstall.exe`; the five programs have the same SHA-256 as the installed ones (capturemann a445d5b6..., cm-desktop 945a153b..., cm-daemon 98cba5a4..., cm-mcp 242a8647..., cm-overlay d4ac6448...).
- The zip was not run through cm-regress separately (`--package-dir` on the unzipped folder not run; the files are byte-identical to the installed ones, which were tested).

## Regression (`cm-regress`, installed files, `--allow screen` only, `--commit 11061ddf`)
`C:\lab\tests\cm-regress.exe` is the build of 96b85253; the tag's code differs from it only in Windows-only bar code that cm-regress does not exercise. Private dirs per case (cm-regress's own). Whisper model `ggml-small.bin`, speech `speech2.wav`, the speaker models from the install, full tier with the downloaded setup as `--installer`.

| Tier | Passed | Failed | Skipped | Total |
| --- | --- | --- | --- | --- |
| standard | 57 | 0 | 7 | 64 |
| full | 68 | 0 | 12 | 80 |

Skips: `linux.*` (7 in standard; 9 in full with `linux.deb_install`, `linux.tarball_install`), `installer.dmg`, `desktop.recording_banner` (not implemented for Windows), `desktop.recording_placement` (second monitor). `ai.transcribe`, `ai.speakers`, `installer.silent`, `desktop.hotkey_overlay`, `zz.crash_events` passed; no time-limit rerun was needed. Reports: `L1-windows-0.3.6-public-installed-regression-{standard,full}-2026-11-22-claude.{md,json}`.

## Desktop app and Quick Capture bar (installed cm-desktop, private dirs)
Private `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, pipe `\\.\pipe\q36p`, `CAPTUREMANN_SECRETS=file` under `C:\lab\q36p\app`; started from a scheduled task in the console session; keys by `keybd_event`, windows by UI Automation, state from `quick_capture.json`.
- App started, stayed alive for the whole run; `desktop.log`: `desktop 0.3.6 started`, studio window opened; no crash event in the Application log.
- Ctrl+Alt+Shift+C: bar (1323x161) opened and became the foreground window (pid of cm-desktop; the studio was foreground before).
- `2`: kind screenshot > video. `1`: video > screenshot. `Ctrl+1`: no visible change (kind and bar unchanged, library not raised; the default presets file has one preset or the same kind, so a change would not show here; not investigated). `O`: collapsed true, bar 717 wide. `Esc`: bar closed, foreground back to the studio window. A second `Esc` did nothing harmful.
- Log line at the bar's close: `ERROR gpui-pre-0.3.9/src/window.rs:2017: window not found` right after `quick_capture window ... closed itself after 7.3s`. Cosmetic as seen (no visible effect), but an ERROR line in the log; I do not know if earlier builds print it.

## Isolation
Before/after listing of the laptop user's default library locations and `%APPDATA%\CaptureMann` (176 lines, with the SHA-256 of `settings.json`): **0 differing lines**.

## Cleanup
Silent uninstall (`uninstall.exe /S`): exit 0; uninstall entry gone, Start menu folder gone; `C:\lab\q36p` (downloads, install, portable, app dir, reports), my scripts (`q36p-*.ps1`), `lab.lock` removed; no scheduled task or CaptureMann process left. `C:\wt` and `C:\lab\tests` kept. Laptop not rebooted, still on Windows (`lab.sh status`: up: windows).

## ISSUES
None blocking. Observations: the ERROR log line above; Ctrl+1 showed no visible effect in this private config (no check that a preset was available).

## Not run
Windows 10, SmartScreen / browser mark-of-the-web, second monitor, mixed DPI, NVIDIA/AMD encoders, the hardware tier, camera/microphone/sound, the soak, regress against the unzipped folder, the library-raise check for Ctrl+1 with a saved preset.

## Verdict
PARTIAL. The public files match the published hashes, install, run from the zip, and pass standard 57/0 and full 68/0; the bar keys work.
