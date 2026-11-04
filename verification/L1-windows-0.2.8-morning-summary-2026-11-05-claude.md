# L1 Windows 0.2.8: morning summary (Nov 5, 2026, Claude, overnight orchestrator)

Branch `track/windows` on `public` (craft-mann/capturemann), nothing else touched: not `main`, no tag, no release, no
signing. Machine: ThinkPad T490, Intel UHD 620 (driver 31.0.101.2137), Windows 11 Pro 25H2 build 26200.9711.
Reports: `L1-windows-0.2.8-hardware-2026-11-05-claude.md` (results, root causes, PARTIAL) and
`L1-windows-0.2.8-night-log-2026-11-05-claude.md` (timeline). Nothing here is called "verified": these are one-laptop
checks by an agent; nobody looked at the screen.

## What to download

- **Package (final): Actions run 41897523579** ("Windows package", commit 31d0562, all CI steps green including app window
  and silent install): artifact `capturemann-windows-x64` = `capturemann-0.2.7-windows-x64.zip` (SHA-256
  `0b510ef789d37267b671b6f1c5377d607ffde430ef924f3842f6a5c6d4dc869f`) and the per-user installer
  (`2a17a59dc834a65ececd2923c18a466be782a2c94de7b87f079ba68812139898`). Unsigned. It is on the laptop in `C:\lab\pk5` (zip,
  installer in `C:\lab`).
- Test programs: "Windows tests" run 41895032977 (31d0562) is unpacked in `C:\lab\tests`. Later commits only add docs and one
  unit-test fix.

## What works on the laptop (details and numbers in the hardware report)

- Diagnostics (GPU, recognizer, ffmpeg, llama-server and speaker-engine lines), `targets` ("Display 1", readable app names,
  listed = saved window size 1426x721), Auto-redact through `Windows.Media.Ocr` (e-mails, a key, an IPv4; en-US only).
- Bundled FFmpeg 9.0.2: export (quick, YouTube 1080p, WebM, GIF); Import with the hardware editing copy (upright, 30 fps
  constant) and its error paths (file open elsewhere, denied by ACL, not media, long Vietnamese name).
- Recording (Windows.Graphics.Capture + Media Foundation hardware H.264): screen + cursor + mic + system audio, pause/resume,
  window and region targets, `kill -9` recovery (0.99 s lost), daemon and app paths; a 9.2-minute 1080p30 screen + mic run
  (constant 30 fps, keyframe every 2.0 s, no dropped mic buffers, daemon CPU about 3.5 % of the machine).
- Desktop app starts, the second launch with a file opens the Import sheet; installer installs and uninstalls cleanly.
- New: system audio through WASAPI process loopback (leaves out CaptureMann's own sound), with the plain loopback as the
  fallback. It opens and delivers (silence included); that it excludes our own sound was NOT checked (no sound allowed).

## Bugs found on real hardware and fixed (each in its own commit)

1. Window screenshots saved at 960x485 while listed 1426x721 (headless daemon not DPI aware).
2. Every recording ended with a fast-fail crash (0xC0000409): WGC `stop()` ran twice. This would have broken every recording.
3. `cm-desktop.exe` panicked at start-up (tokio named pipe created outside a runtime, the Import second-launch pipe).
4. Quick Sync refused yuv420p: import editing copies and exports failed on this GPU ("no hardware video encoder"); NV12 now.
5. 54 % "late" frames and a `screen_degraded` warning for a picture that changes at 16 Hz (WGC sends frames only on change).
6. "Microsoft Basic Render Driver" scrubbed to "Basic [redacted] Driver" in diagnostics.
7. CI: the package check piped ffmpeg into `Select-Object` (PowerShell 7 exit code); a unit test assumed macOS error numbers.
8. My own first process-loopback version corrupted the heap (found by the test on the laptop, fixed before it reached a package).

## Still broken, unexplained or not run

- **Camera records about 10 fps at 720p** (manifest `measured_fps` 10.05; NV12 720p30 is chosen). The frames are dark (mean
  luma 24/255) so the camera's low-light frame-rate drop is the guess; a bright screen in front of the camera did not change it
  and a Motion JPEG preference did nothing (reverted). Needs a test in a lit room.
- The 10-minute gate ran 553 s (two overlapping scripts of mine), no A/V-sync stimulus, no 30-minute run.
- Not run: speaker labels and transcription speed (no model on the laptop and I did not download one), the app's picker/tray/
  hotkey by eye (the picker windows are excluded from capture), SmartScreen, Windows 10, second monitor, `cm-desktop__parity`
  with FFmpeg (needs a full ffmpeg on PATH; the zip's has no lavfi), the `cm-media` tests that make fixtures with lavfi.
- The phone number on the OCR test line was not suggested (an IPv4 was); probably OCR reading, not investigated.
- "Windows Terminal Host" is the app name for Windows Terminal; the "FFmpeg was not found ... Homebrew" text is still the macOS
  wording on Windows (only matters for a build without the bundled copy). Camera hot-plug "Reconnected" and mixed-DPI review
  were not started.
- Recording in the app is hidden unless `CM_WINDOWS_RECORDING=1` (so nothing ships by accident; see O5).

## Sound

The laptop's output was already muted (volume 0) when I looked at 23:10 and I left it so. Before that, at 22:36, a test
filter (`system_audio`) also matched the chime test and played one chime (probably silent: muted). After the instruction no
audible test ran. Skipped because of it: the chime/loopback tests, AUD-1/AUD-2 audible checks, the check that process loopback
excludes CaptureMann's sound. Nothing was played on the Mac.

## Decisions for you

- **O5** ship Windows recording in 0.2.8? Evidence is in the report; open: camera 30 fps, A/V sync, the full 10 minutes, the app's
  picker/hotkeys by eye. Flip `recording_supported()` for Windows (or keep `CM_WINDOWS_RECORDING`) accordingly.
- **O4** SignPath and **O7** installer: the installer works (silent install/uninstall checked on the laptop and in CI); it is unsigned.
- **llama.cpp for Windows:** the pin in `build-macos.sh` is not confirmed for Windows, so no llama build is in CI; the package says
  "this build does not include one".
- **Merging `track/windows` into `main`:** the branch changes shared code (cm-capture pipeline `delivers_only_on_change`,
  `cm-jobs` scrub, `cm-media` pix_fmt for Media Foundation/Quick Sync, `cm-platform` capability switch, a `cm-audio` dependency on
  `windows-core`); the macOS tests pass here (fmt, clippy -D warnings, workspace tests, target_os check, i18n check before every
  push). The `main` Windows CI job still fails on `exp4_ac4_cancel_deletes_partial_output` as before.

## Risks

- Everything rests on one laptop with one GPU. The Quick Sync fix (NV12) was needed here; other encoders (NVIDIA, AMD) are untried.
- Process loopback needs a recent Windows 10/11; older ones fall back to the plain loopback (which includes our own sound).
- Pushes to `public/track/windows` only; the repo's Windows workflows ran about 14 times; I cancelled two queued runs.
