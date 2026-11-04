# L1 Windows 0.2.8: night log (Nov 4-5, 2026, Claude, overnight orchestrator)

Running log, newest at the bottom. Machine: Lenovo ThinkPad T490, Intel UHD 620 (driver 31.0.101.2137), Windows 11 Pro 25H2
build 26200.9711, one 1920x1080 panel at 150 %. Times are local (UTC+7). Nothing here is "verified" unless a stored
report names it; these are observations.

- 22:15 start. State: branch `track/windows` at c007500. Package run 41854759015 (c007500) and Windows tests run
  41855785408 (c007500) started; the test programs of 41855785408 were unpacked to `C:\lab\tests` (31 programs).
- 22:35 hardware test programs run in the logged-in session (see the topic reports). `cm-render__ocr_real` with
  `CM_OCR_REAL=1`: real Windows.Media.Ocr finds the 2 e-mails and the key. `cm-encode__mf_real`: 150 frames, 3
  fragments. `cm-capture__real_display`: 4 of 5 pass; the window test fails (image 960x485 against bounds 1426x721).
  Cause: a process without a DPI manifest gets scaled `GetWindowRect` values. Fixed in 9dc56ad (per-monitor DPI
  awareness requested by the capture crate and by `cm-daemon`).
- 22:36 MISTAKE: I ran `cm-audio__hardware.exe system_audio --ignored`; the filter also matched
  `system_audio_hears_a_chime_played_by_another_process`, so one short chime played on the laptop's speakers. No
  further audio test was run with a loose filter. (The maintainer asked for no audible tests overnight.)
- 22:40 `cm-camera__windows_camera`: the camera is found; the 6 s recording test got 62 frames (about 10 fps) at 720p
  and fails its 120-frame check; a diagnostic test that prints the native modes was added (7ea...).
- 22:43 Package run 41854759015 failed at "Portable zip..." because `ffmpeg -version | Select-Object -First 2`
  leaves a non-zero `$LASTEXITCODE` in PowerShell 7 (the same ffmpeg.exe runs fine on the laptop: exit 0).
  Fixed in the workflow; rust-cache now keeps its cache on failure. The zip of that run was still uploaded
  (`if: always()`) and unpacked on the laptop as `C:\lab\pk1` for the first checks.
- 22:50 to 23:10 (the "numbers" below are runs on the laptop unless noted) camera 6 s test: about 10 fps at 720p; the
  Windows tests programs unpacked again after each fix (runs 41855785408, 41864428269, 41869678741, 41880383062,
  41885987311, 41892011764, 41895032977).
- 23:10 Maintainer's instruction arrived: no sound at all. The default output of the laptop was already muted at volume 0
  (`C:\lab\mute.ps1`, Core Audio endpoint; the saved state was left unchanged and removed at the end). No audible test was
  run after that. Skipped for this reason: the loopback/chime tests of `cm-audio__hardware`, AUD-1/AUD-2 audible checks,
  "does process loopback really leave out CaptureMann's sound".
- 23:20 root causes found and fixed (details in the hardware report): window DPI (9dc56ad), Quick Sync NV12 (commit
  "Media Foundation H.264 takes NV12"), the GitHub Windows package check (PowerShell 7 exit code), "Basic Render Driver"
  scrub, stage logging for the stop crash.
- 23:35 Mistake: I pushed once (980fb5a... 3a10b2a) before the pre-push run of that moment had finished; it passed
  afterwards. No other push skipped the checks.
- 00:20 stop crash found: `wgc stop` ran twice (log line printed twice), fixed in the WGC capture (2820e55 + later).
- 00:50 package run 41880377274 (4ad1c12): all CI steps green, including the app window and the silent install; the
  start-up panic (tokio pipe) was found in the failed run 41869669940 and fixed in 4ad1c12.
- 01:00 to 01:15 gate recording (553 s) with a ticking clock: 54 % late frames; fixed in fa4d68d (event-driven
  sources) and re-measured: 0 late.
- 01:35 app started on the laptop; recording in the app was hidden (`recording_supported()` false on Windows):
  `CM_WINDOWS_RECORDING=1` added (3a10b2a).
- 02:00 process loopback system audio written; first run: heap corruption 0xC0000374 (PROPVARIANT drop); fixed (31d0562);
  then 299 packets in 3 s with nothing playing.
- 03:05 final package run 41897523579 (31d0562) green; re-check on the laptop; installer installed and uninstalled.
- 03:15 laptop cleaned: recordings, screenshots, configs and test libraries deleted (a screenshot of the desktop shows a
  personal photo from the Settings window; none was stored in the repository), no cm-* process running, output mute state
  restored (it was muted).
