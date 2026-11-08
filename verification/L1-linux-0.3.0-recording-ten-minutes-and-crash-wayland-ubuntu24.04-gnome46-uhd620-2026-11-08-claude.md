# Linux recording, 10-minute A/V run and kill -9 recovery (release 0.3.0 gate, Wayland part)

- Date: Nov 8, 2026, 21:50 to 22:12 Vietnam time. Machine: lab ThinkPad T490, Ubuntu 24.04, GNOME Shell 46.0, **Wayland session**, Intel UHD 620, PipeWire.
- Build: branch `track/linux-recording` (commits 8213ed99, 4bd55698, 2ce8d977, 62486390, 6c92a401), **release** build of `cm-daemon` and `capturemann`, `--no-default-features` (no speaker engine; irrelevant here). FFmpeg: the system `/usr/bin/ffmpeg` (h264_vaapi, libx264 used only to make the stimulus clip).
- Encoder used: hardware H.264 through VA-API (constant quality: the driver has no bit-rate control). Recording path: GNOME's screen-cast service, no dialog.
- Scripts: `scripts/testlab/rec-live-av.sh`, `rec-av-analyze.py`, `rec-crash.sh`, `rec-hold.sh`.
- **Status of the gate: PARTIAL. Wayland measured; X11 not measured** (see "Not run"). The `CM_LINUX_RECORDING` switch stays until X11 is measured.

## Stimulus and method

A test clip (a white flash and a 1 kHz beep together every 2 s, 640x360, played in a window by `ffplay`, 90 % of the screen) plays on the laptop while the product records the whole display with `capturemann record start --size 720p --countdown 0 --mic default --system-audio`. `rec-av-analyze.py` finds the flash in `screen.mp4` (its timeline plus the track's `file_start` from the manifest = session clock) and the beep onset in `system.wav` (the monitor of the output: no air path) and in `mic.wav` (the room microphone hears the loudspeaker, 1 kHz band-passed). It prints flash minus beep onset: negative means the picture is first. The clip's own flash-to-beep offset is not zero by design of any player, so read the numbers as "offset of this setup" and the per-minute medians as the drift.

## 10-minute recording, screen + microphone + system audio

| | |
| --- | --- |
| Length | 600.63 s (`stop` answer); screen.mp4 600.53 s, 18017 frames (30 fps, 18009 written), mic and system audio 600.54 s and 600.56 s |
| Screen track | delivered 15108 frames, written 18009 (repeats on a still screen), dropped 8, late 37; no `encoder_overloaded` warning; no warnings at all |
| Mic / system | 0 dropped; 3 / 13 late buffers |
| Flash vs beep, system audio | 250 of 252 flashes matched; **mean -2.3 ms, median -5.9 ms, sd 23.5 ms** (the video frame grid is 33 ms), min -40.6, max +72.7 |
| Per-minute median, system | -34.6, +6.0, -6.0, -19.2, +26.1, -7.2, -5.9, +32.7, -0.6 ms: no drift over 9 minutes |
| Flash vs beep, microphone (room) | 182 of 276 onsets matched; median +0.6 ms, per minute -37.7, +22.5, +0.2, -9.7, +5.3, -28.1, +1.0, +35.2, -4.4; sd 132 ms because 94 onsets were room noise or missed (read the median and the per-minute medians) |
| Mic onset minus system onset | n 165, median -4.8 ms |

The stimulus stopped after about 8.5 minutes (252 flashes): the maintainer's quiet hours began at 22:00 and `rec-hush.sh` muted the output and stopped the player at 21:59:55. The recording went on to 600 s in silence; it is counted in the lengths above but not in the sync statistics.
Result: **a 10-minute screen + microphone + system-audio recording plays back in sync on Wayland (GNOME 46)** within the 33 ms frame grid and without drift, over the 8.5 minutes that had a stimulus. This is one run on one machine.

## kill -9 recovery (REC-8 AC1), Wayland, screen + microphone, silent

`rec-crash.sh 20`: record, `kill -9` the daemon after 20 s, start the daemon again.

- Before the kill the bundle held `recording.lock`, `screen.mp4` (262 kB), `mic.wav`, `mic.denoised.wav`.
- The next daemon start logged `recovered recording ... (lost up to 2000 ms)`; the bundle had a `manifest.json`, no `recording.lock`, `revisions[0].action` `recover`, `recovered.lost_ms_max` 2000.
- Recovered lengths: screen **20.03 s**, microphone **20.05 s**, manifest duration 20.03 s, for a recording that ran about 20.3 s: **well under 3 s lost**. (AC2, sync of the recovered bundle: not measured.)

## Not run

- **X11 session, 10 minutes**: the lab was switched to Wayland by the desktop agent before this run and the clip needs sound, which is not allowed after 22:00. The first X11 numbers are a 30 s debug-build run (flash-to-beep offset not meaningful there, the player's own latency was not separated; `verification/` has no report for it) and are not evidence. To run: log in to X11, `bash ~/lab/rec-ten.sh 600` before 22:00 or with a silent stimulus.
- Window recording on Wayland (stated gap, see the targets report), second monitor, scale 2, the 30-minute soak, system audio from a player other than `ffplay`.
