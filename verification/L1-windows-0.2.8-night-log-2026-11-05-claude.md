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
