# REC-WIN manual script: screen recording on Windows (0.2.8, gate)

Covers what the unit tests cannot: Windows.Graphics.Capture plus the Media Foundation hardware H.264
encoder on a real machine. The logic around them (time mapping, crop, NV12 conversion, the MP4
muxer, the key-frame clock) is tested on any OS (`cargo test -p cm-encode -p cm-capture --lib`).
Run on the lab laptop (ThinkPad T490, Intel UHD 620) in the logged-in session. Reports go in
`verification/` (template in its README), naming the machine, the Windows build, the GPU and the
app build; say "verified" only with one. Delete the recordings afterwards.

## 0. Encoder alone (no screen)

`cargo test -p cm-encode --test mf_real -- --ignored --nocapture --test-threads=1`
(from a build machine; or run the test binary from the CI artifact).
Pass: 150 frames in, 150 out, a clean file, fragments of about 2 s, every fragment starts at a
sync sample. Prints `SKIPPED` on a machine with no hardware H.264 encoder (a VM).
The kept file is not stored; for a look at the output use the recording of section 1: h264 High,
1920x1080, 30 fps.

## 1. The 10-minute gate (task 5)

1. Start the app, choose the main display, 1080p (or Source on a 1920x1080 panel), 30 fps, screen
   and microphone, then record 10 minutes while playing a video with a running clock on screen and
   clapping or beeping now and then.
2. Stop. Open the recording: it plays in sync (clap and beep within 1 frame / 40 ms of the picture).
3. `capturemann library` / the bundle's `screen.mp4`: `ffprobe -show_entries stream=nb_frames,
   r_frame_rate,duration` gives 18000 frames, 30/1, 600 s (late frames: see the manifest's track
   stats; NFR-2 allows 2 %).
4. Fragments: `ffprobe -show_packets` flags `K` about every 2 s.
5. CPU / GPU use during the run (Task Manager): note them in the report.

## 2. Pause and resume

Record 30 s, pause 10 s, resume 30 s. Pass: the file is 60 s long, continuous, the clock on screen
jumps 10 s at the join.

## 2b. HUD clicks and the area border (win-hudclick; NOT run on hardware)

Record the display on the external monitor, then on the panel. Pass, on each: click Pause on the HUD (the
recording pauses, the HUD shows it; click Resume); the recorded area border must not block clicks to other apps
on the recorded monitor (click a window behind it: it takes the click and comes to the front); the border is still
drawn and the HUD stays above it. If not, send the `recording windows` lines of `desktop.log` (hwnd, rect, ex-style,
z-order of the area window, the bubble and the HUD).

## 3. Crash recovery (taskkill)

1. Record the display. After 30 s, in another shell: `taskkill /F /IM cm-desktop.exe` (and
   `cm-daemon.exe` if it runs separately).
2. Start the app again. Pass: the unfinished recording is offered for recovery, and after recovery
   the file holds everything up to the last complete 2 s fragment (at most about 2 s lost) and plays.

## 4. Targets

- A window (Notepad): the picture is the visible frame, no black border, size equal to the picker's.
  Resize it while recording: the recording keeps its size (the picture is scaled).
- A region 1280x720 of the display: the file is 1280x720 and shows that region.
- Close the recorded window mid-recording: the recording stops cleanly and keeps what it has.
- Cursor on / off follows the setting.

## 5. No hardware encoder

In a VM without a GPU: starting a recording fails with `hardware_encoder_unavailable` and leaves no
bundle. (The message text still names a Mac; see the report notes.)
