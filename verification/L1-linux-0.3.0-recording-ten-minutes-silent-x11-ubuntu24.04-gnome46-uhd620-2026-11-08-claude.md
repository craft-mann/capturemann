# Linux recording, 10-minute silent run on X11 (release 0.3.0 gate, X11 part)

- Date: Nov 8, 2026, 22:50 to 23:01 Vietnam time. Machine: lab ThinkPad T490, Ubuntu 24.04, GNOME Shell 46.0, **X11 session** (`loginctl` Type x11), Intel UHD 620, PipeWire.
- Build: branch `track/linux-recording` after commit 262e81e2 (flush every fragment), **release** `cm-daemon` and `capturemann` (`--no-default-features`). FFmpeg: system `/usr/bin/ffmpeg` (`h264_vaapi`). Encoder: hardware H.264 through VA-API, constant quality. Path: GNOME's screen-cast service, no dialog.
- Scripts: `scripts/testlab/rec-ten-quiet.sh`, `rec-live-av.sh` (`SILENT=1`: `ffplay -an`), `rec-av-analyze.py`.
- **A/V offset: not measured on X11. Quiet hours (no sound after 22:00 Vietnam time).** The offset is measured on Wayland only (`L1-linux-0.3.0-recording-ten-minutes-and-crash-wayland-*`). This run measures length, frame, drop and cross-track agreement.

## Stimulus

The same clip as the Wayland run, played without sound: a window of 90 % of the screen turns white for 100 ms every 2 s. Recorded: the whole display with `--size 720p --countdown 0 --mic default --system-audio`, the microphone hearing the quiet room.

## Results (600 s)

| | |
| --- | --- |
| `stop` answer | 600.49 s; warnings: none |
| Tracks | screen, **cursor**, mic, system audio |
| Screen | `screen.mp4` 600.40 s, 18013 frames written (30 fps), 20923 delivered by the desktop, **0 dropped**, 11 late; no `encoder_overloaded` |
| Flashes found in the video | **300 of 300** (one per 2 s for 600 s): the picture timeline is continuous |
| Mic | 28142 buffers, 600.36 s, 0 dropped, 0 late |
| System audio | 28143 buffers, 600.36 s, 0 dropped, 0 late (no beep played, so no onsets; the file has a peak of 1.0, not examined) |
| Track ends on the session clock (ticks of 1/120000 s) | screen 72055810, cursor 72055810, mic 72057790, system 72059020: within 24 ms of each other after 10 minutes, no drift between files |
| Track starts | screen 66 ms, mic 123 ms, system 129 ms after the session origin (as the manifest records them) |
| Cursor | `cursor.jsonl` and `cursors/` present (XInput2 raw, `cm-cursor`); the mouse did not move, so one event |

An earlier 10-minute run an hour before (22:36) had the same numbers (600.475 s, 18013 frames, 0 dropped, 8 late) but its scripts forced a stale Wayland environment into the daemon, so the product treated the session as Wayland (no cursor track); that is why this run was repeated with the right environment.

## Not run

- The flash-to-beep offset on X11 (quiet hours). A 30 s debug-build run earlier in the day on X11 is not evidence.
- Window recording on X11 by the CLI (area crop is unit-tested; not run on hardware), second monitor, scale 2, the 30-minute soak.
