# L1 Windows 0.2.8: hardware and CLI checks on the lab laptop

- **Date:** 2026-11-04 22:30 to 2026-11-05 about 02:00 (UTC+7)
- **Who:** Claude (AI agent), driving the laptop from a Mac over ssh and through the logged-in desktop session
  (`C:\lab\win-run-in-session.ps1`). Nobody watched the screen; all results are from program output and files.
- **Machine:** Lenovo ThinkPad T490, Intel UHD Graphics 620 (driver 31.0.101.2137), Windows 11 Pro 25H2 build
  26200.9711, one 1920x1080 panel at 150 %, Realtek microphone array, Integrated Camera (USB UVC). Output muted
  at volume 0 for the whole run (see "Sound").
- **Builds:** CI "Windows package" zips (the portable zip, `C:\lab\pk1`: run 41854759015, commit c007500; `C:\lab\pk2`:
  run 41869669940, commit 2820e55) and "Windows tests" programs of runs 41855785408 (c007500), 41864428269 (9f391d0),
  41869678741 (2820e55), 41880383062 (4ad1c12). The runs that carry later fixes are named in the log.
- **FFmpeg:** the zip's own `ffmpeg.exe` 9.0.2 (LGPL, `h264_mf`, libvpx, libopus), run with `-hw_encoding 1` where noted.
- **Daemon:** `cm-daemon.exe` started in the interactive session with `CAPTUREMANN_CONFIG_DIR=C:\lab\cfg-night`,
  `CAPTUREMANN_LIBRARY=C:\lab\lib-night`, `CAPTUREMANN_SOCKET=\\.\pipe\cm-night`, `CAPTUREMANN_SECRETS=file`
  and `agents.tool_policy = {"start_recording": "allow"}` in that config (a headless daemon has no confirmation UI).
- **Status:** PARTIAL (see "Not run"). Nothing from the desktop app window itself is in this report.

## Results

| Check | Result | Evidence |
| --- | --- | --- |
| `capturemann diagnostics` | PASS | Windows 11 Pro 25H2 build 26200.9711; GPU Intel UHD 620 with driver 31.0.101.2137 and the Basic Render Driver flagged as a software renderer; ffmpeg path next to the exe; `llama-server not found ... this build does not include one`; `text recognizer: windows_ocr (available; reads en-US ...; OCR languages installed: en-US)`; `speaker engine built in: true`; permissions mic/screen/camera Authorized. One defect, fixed (below): the GPU line read "Microsoft Basic [redacted] Driver" |
| `--json targets` | PASS | display "Display 1" (not "Unknown Monitor"), 1920x1080, scale 1.5; windows listed with app names "Notepad", "Settings", "Windows Terminal Host" (the exe's FileDescription; the product name is "Windows Terminal") |
| window screenshot size | FAIL then fixed | before: listed 1426x720, saved 960x485 (daemon not DPI aware); after the fix (zip of run 41869669940): listed 1426x720, saved 1426x720 |
| `cm-render__ocr_real` (`CM_OCR_REAL=1`) | PASS | Windows.Media.Ocr found the 2 e-mails and the `sk-proj` key on the synthetic PNG |
| `annotate --suggest-redactions` through the daemon | PASS | on a Notepad window shot at 1426x720: 2 e-mails (0.95), 1 key (0.95), 1 IPv4 (0.5), recognizer `windows_ocr`. The phone number on the 4th line (`+1 415 555 0132`) was not suggested (not investigated). On the old 960x485 shot the key came out at 0.6 and the IP was missed |
| Import `--dry-run` (a.mp4, rotated .mov, vfr .mkv, wav, flac, hevc) | PASS | statuses ready / prepared (rotated; vfr+mkv+pixel format; hevc) / ready / cant_import for flac ("FLAC files are not supported yet.") |
| Import editing copy on the UHD 620 (`h264_mf -hw_encoding 1`) | FAIL then fixed | before: "This computer can't prepare this file for editing (no hardware video encoder)" for all three; ffmpeg said `MFT name: 'Intelr Quick Sync Video H.264 Encoder MFT'`, `format negotiation failed`. Cause: yuv420p input; the transform wants NV12. After the fix (run 41869669940): all imported; copies are H.264, upright 1080x1920 (rotated source), 30 fps constant, 600 frames for 20 s |
| Export with the bundled FFmpeg (`export --preset ...`) | PASS | quick (h264_mf, 20.0 s, 5.4 MB), youtube-1080p (h264_mf), webm (libvpx-vp9, 20.008 s), gif (20.0 s) |
| Import error paths | PASS | file open without sharing: "is open in another program"; read denied by ACL: `permission_missing`, files, Windows wording (Controlled folder access, file picker); a text file named .mp4: "can't be read. It may be damaged ..."; a long accented Vietnamese file name imported |
| `cm-encode__mf_real` | PASS | 150 frames in, 150 out, 3 fragments, 1.99 MB |
| `cm-capture__real_display` | PASS after the DPI fix | before: 4 of 5; the window test failed (image 960x485 against bounds 1426x721) |
| `cm-capture__real_window_thumbs` | PASS | Notepad 1426x721 thumbnail 62 ms; the minimized Windows Terminal answers "Window is minimized" and is still listed |
| `cm-desktop__parity` | SKIPPED | FFmpeg not on PATH for the program (not re-run with `CM_FFMPEG`) |
| `cm-audio__hardware` (devices only) | PASS | default mic "Microphone Array (Realtek(R) Audio)" 48 kHz 2 ch opens without streaming |
| `cm-audio__hardware` system-audio tests | NOT RUN (see Sound) | the chime test ran once by mistake; it reported `Heard -0.49 dBFS`; the other test (silent system) recorded 0 frames: a WASAPI loopback delivers nothing while nothing plays, and the track is then empty (`idle` gap reason) |
| `cm-camera__windows_camera` | PARTIAL | camera listed with 9 sizes; opens; first picture not black; steady frames; but only about 10 fps at 1280x720 (see Problems) |
| Recording (record_e2e, run 41869678741, c. 2820e55): screen + cursor + mic, pause 3 s | PASS | 43 PASS, 0 FAIL, 2 SKIP; screen 1920x1080 H.264 High 30 fps, keyframes every 2.0 s, one pause gap 3.014 s, stop latency 402 ms, process CPU 12 % of one core, RSS 107 MB, mic 48 kHz no drops |
| Recording + camera 720p (record_e2e) | PARTIAL | 55 PASS, 3 FAIL: camera 10.05 fps (asked 30), late frames, camera ends 13 to 100 ms after the stop |
| Recording through the daemon (`record start/pause/resume/stop`), screen + mic + camera | PASS (with the camera caveat) | 21.7 s wall, 5.03 s paused, file 16.63 s, screen keyframes every 2 s, camera 10 fps |
| `kill -9` recovery (`record_e2e --kill9-test`) | PASS | 10 PASS with the zip's ffmpeg/ffprobe on PATH (run 41869678741 programs, zip of run 41869669940): killed at about 7.3 s, recovered 1 bundle, 0.99 s of screen lost, recovered screen.mp4 is H.264 1920x1080 30 fps 6.30 s and decodes with no errors, cursor log parses, lock removed, indexed |
| Stop crash | FAIL then fixed | every recording ended with exit code 0xC0000409 (fast-fail) in the daemon and in `record_e2e`; see Problems |
| 9.2-minute screen + mic recording (the 10-minute gate, shortened) | PARTIAL | 553.5 s (my two overlapping scripts; not 600 s): 1920x1080 H.264 High, 16598 frames (30 fps constant), keyframes every 2.0 s (277, min and max gap 2.0), mic delivered 55329 = written, no drops; daemon CPU about 3.5 % of the machine (8 threads), working set 98 to 196 MB (falling after the first minute). A/V sync not measured. 54 % "late" frames and a `screen_degraded` warning because the screen showed a ticking clock (see Problems) |

## Problems found, with root causes

1. **Window capture 960x485 against listed 1426x721** (also visible in the old daemon: listed 1426x720, saved
   960x485). A process without a DPI manifest (headless daemon, CLI, test program) gets scaled `GetWindowRect`
   values and a smaller `PrintWindow` bitmap; the DWM frame bounds are always physical. Fixed: `ensure_dpi_aware()` in
   `crates/cm-capture/src/sys/win_window.rs` (commit 9dc56ad), called by the capture code and at the start of `cm-daemon`.
2. **Import editing copy and export refused by the Intel Quick Sync MFT** ("format negotiation failed" with yuv420p).
   Fixed: NV12 for `h264_mf` (`Encoder::pix_fmt`, `EncoderFamily::pixel_format`).
3. **Every recording ended with a fast-fail (0xC0000409)**: `WgcCapture::stop()` ran twice (explicitly, then in `Drop`);
   the second `RemoveFrameArrived` on the closed frame pool ended the process. The screen file was complete only up
   to the last fragment. Fixed: `stopped` flag in `crates/cm-capture/src/sys/win_wgc.rs`. Found with stage logging
   (`wgc stop: removing the handlers` printed twice).
4. **`cm-desktop.exe` panicked at start-up** in the CI "app window" step: `there is no reactor running` at tokio's
   `named_pipe.rs:136`. `instance::serve` created the first pipe outside a tokio runtime (the Import feature's
   second-launch pipe). Fixed: the pipe is created inside the serving thread's runtime (commit 4ad1c12). Not yet seen
   running on the laptop (next report).
5. **"Microsoft Basic Render Driver" printed as "Basic [redacted] Driver"** in diagnostics: the HTTP Basic credentials
   scrubber. Fixed in `cm-jobs::scrub` (credentials must look like base64).
6. **54 % late frames and `screen_degraded` for a picture that changes 16 times a second**: the pipeline counted
   gaps shorter than 250 ms between frames as missed slots, but Windows.Graphics.Capture sends frames only when the
   picture changes (video at 24 fps would do the same). Fixed: `ScreenSource::delivers_only_on_change` (commit fa4d68d).
   Not yet re-measured on the laptop.
7. **Camera at about 10 fps at 720p** (manifest `measured_fps` 10.05, NV12 720p30 is the chosen native type). The
   recorded frames have a mean luma of 24 of 255 (a dark room); the camera most likely halves its frame rate in low
   light. A change that preferred MJPG did not help and was reverted. Not resolved; needs a re-run in a bright room
   and, if it persists, a camera-control (auto-exposure priority) investigation.
8. Not fixed: the picker/`targets` app name for Windows Terminal is the exe's FileDescription "Windows Terminal Host".

## Sound

The laptop's default output was already muted at volume 0 when I checked it at 23:10 (state saved in
`C:\lab\mute-state.txt` and left as found). At 22:36, before I knew that, `cm-audio__hardware.exe system_audio ...`
also matched the chime test, which played one short chime; I cannot say whether the output was muted at that moment.
No audible test was run afterwards: skipped for this reason were the loopback/chime tests of `cm-audio__hardware`
and process-loopback work.

## Not run

Desktop app window behaviour on the laptop (start-up, picker, own-window exclusion, tray), speaker labels (no models
installed), transcription speed (no model installed), loopback recording with sound, a second monitor, Windows 10,
SmartScreen, the NSIS installer on the laptop, `cm-desktop__parity` with FFmpeg, A/V sync with a stimulus.
