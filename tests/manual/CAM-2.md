# CAM-2 manual script: camera track, camera/screen sync (`cm-camera`)

Covers what CI cannot: a real camera, real optics, real sync. Everything else (retiming, pause
gaps, device removal, fragment cadence, mirror metadata) is covered by `cargo test -p cm-camera`
with the fake camera.

**Privacy:** every step that turns the camera on records you. Run them yourself, keep each run
short (the tools cap at 10 s), and delete the output folders afterwards. Nothing here prompts for
camera permission: grant Camera to the terminal app first (System Settings > Privacy & Security >
Camera), restart the terminal, and check with `sync_probe list` (prints `Authorized`).

## 1. Hardware smoke test (about 7 s, camera on)

```
CM_CAMERA_TEST_YES=1 cargo test -p cm-camera --test mac_camera -- --ignored --nocapture
```

Pass when: the printed `details` show `size` 1920x1080 (or the camera's native size when it has
no 1080p mode), `b_frames: false`, 2 s fragments (`fragments.max_s` about 2.0), `stats.dropped`
0, `mirror` as configured; the test itself checks that `ffmpeg` decodes the file without errors,
that frame count equals `stats.written`, that PTS are strictly increasing and that a preview ran
on the same capture session while recording. The recording is deleted by the test.

## 2. Delivery latency (5 s, camera on, nothing stored)

```
cargo run -p cm-camera --example sync_probe -- latency --yes --secs 5
```

Prints median/p95/max of "host clock at the callback minus the sample PTS" and the PTS interval
distribution. Reference run (MacBook Pro Camera, built-in, 1080p30, macOS 26): median 73.5 ms,
p95 76.8 ms, interval median 33.3 ms. This is pipeline delay. It tells you nothing about whether
the PTS is correct: that is step 3.

## 3. Optical sync (the 50 ms criterion, NFR-3 / CAM-2.AC2)

Goal: learn how far the camera's timestamps are from real time, then set
`CameraConfig::latency_compensation_ms` (`DEFAULT_LATENCY_COMPENSATION_MS` in `types.rs`).

Setup: a flashing window in front of the camera, recorded from a different place than it is shown
(the camera must see the screen: tilt the screen so the camera sees it, or use a mirror/second
display). Dim room lighting helps the camera's exposure.

1. Terminal A, start the flash window (640x480 window, black/white about once a second for 8 s,
   not fullscreen, 0.5 Hz, safe for photosensitive viewers; it takes focus when it opens):
   `cargo run -p cm-camera --example flash_window -- --out /tmp/flashes.json --secs 8`
2. Terminal B, within about 1.5 s of the window opening, record the camera (point it at the
   window; `--secs` 10 is the maximum):
   `cargo run -p cm-camera --example sync_probe -- record --yes --out /tmp/cam-sync --secs 10`
3. Analyze (reads files only, no camera):
   `cargo run -p cm-camera --example sync_probe -- analyze --camera /tmp/cam-sync/camera.mp4 --meta /tmp/cam-sync/probe.json --flashes /tmp/flashes.json --display-latency-ms 16`
4. Delete `/tmp/cam-sync` (it contains your image) and `/tmp/flashes.json`.

Reading the result: both tools stamp with the same host clock (`CLOCK_UPTIME_RAW`). `analyze`
matches each logged colour change (host time of the `setBackgroundColor` call) with the brightness
step in the camera file's time (session time when nothing was paused, as in this probe). The raw figure includes the display's own latency (a
vsync plus panel response, 8 to 20 ms; the default assumption is 16 ms, change it with
`--display-latency-ms`), which is subtracted in the line "camera error as recorded". Positive
means the camera track is late. Each edge is only resolved to about plus/minus one half frame
(17 ms at 30 fps); the flash period is 1.013 s so the phase walks across the frame grid, and the
**median over 6 or more edges** is the figure to use (the min/max spread of up to a frame is
expected). If the median error is outside +/-50 ms, or you want it near 0, set
`latency_compensation_ms` to the printed suggestion and repeat step 2 to confirm. Run it for each
camera type you support (built-in, a USB webcam, Continuity Camera, a virtual camera): the offset
is a property of the device, so the right long-term home is a per-device table keyed by
`CameraInfo.id`/model.

Pass for CAM-2.AC2 (start): |median error| <= 50 ms after compensation, on built-in and one
external camera. The 30-minute figure needs the soak below.

## 4. Camera against the screen track (needs `cm-capture`)

When the screen track exists, point the camera at a flashing window shown on the recorded
display, record the session, and compare the two files directly:

```
cargo run -p cm-camera --example sync_probe -- compare --screen <bundle>/screen.mp4 --camera <bundle>/camera.mp4
```

The result is camera minus screen for the same flash (includes the display latency seen by the
camera, since the screen track sees the pixels before they are lit). Compare it against the
step-3 figure for the same camera; they should agree within about 20 ms.

## 5. Soak (30 minutes, NFR-3 "no growth over 60 min")

Record 30 minutes with the camera in a normal session. Pass when `details.delivery_latency_ms`
median over the last minute is within 10 ms of the first minute (compute from two short
`latency` runs at the start and end, or log from the recorder), `stats.dropped` is below 0.1%
of `delivered`, `stats.late` is below 1%, and the 30-minute repeat of step 3 stays within 50 ms.

## 6. Edge cases (manual)

| Case | Steps | Pass when |
| --- | --- | --- |
| Device removed (CAM-1 edge case) | Record with a USB webcam, unplug it after 5 s, replug after 5 s, stop | `stop` succeeds; `camera.mp4` plays up to the unplug; `details.warnings` has `camera_disconnected` (and `camera_restored` if frames returned); the screen recording is unaffected. **Reconnect is not implemented on macOS in this slice**, so no `camera_restored` is expected yet |
| Camera busy | Start a call in Zoom/Teams with the same camera, start a camera recording | `device_busy` (exit code 5) or, for cameras the OS shares, a normal recording |
| Permission denied | Revoke Camera for the terminal, run `sync_probe latency --yes` | prints the `permission_missing(camera)` message with the System Settings path; nothing crashes |
| Pause/resume | Record camera, pause 5 s, resume | `details.pauses` shows the gap (session time); `ffprobe` shows continuous PTS with no jump (the 5 s are subtracted from file time, like the screen and audio files), the file is about 5 s shorter than the wall clock, and fragments stay about 2 s; no frames inside the gap |
| Device list | `sync_probe list` with a USB webcam, an iPhone (Continuity) and OBS virtual camera attached | each appears with the right `kind`; the default is flagged. Continuity needs `NSCameraUseContinuityCameraDeviceType` in the bundle Info.plist to report as `continuity` rather than `external` |
