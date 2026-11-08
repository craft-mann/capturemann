# L1 Linux 0.3.0 camera on hardware (PARTIAL)

- **Date:** 2026-11-08 (about 21:20 to 22:40 Vietnam time), no sound played
- **Distribution / desktop / session:** Ubuntu 24.04.5 LTS, GNOME, Wayland session (auto-login), kernel 7.0.0-42-generic
- **Machine / camera:** ThinkPad T490 (x86_64, Intel UHD 620); built-in "Integrated Camera: Integrated C" (Azurewave, USB, UVC), nodes `/dev/video0` (pictures) and `/dev/video1` (metadata, not listed)
- **Build:** branch `track/linux-camera` (cm-camera Linux backend, V4L2 ioctls, zune-jpeg for Motion JPEG, software VP9 writer through FFmpeg 6.1.1), release profile
- **Who:** Claude (camera agent); nobody was in front of the camera, the room light was not controlled

## What ran

`cargo test --release -p cm-camera --test linux_camera -- --ignored --nocapture --test-threads=1` (5 tests) and
`cm-regress --dev --tier hardware --only rec.camera --allow screen,camera` against the same build (daemon, CLI, portal screen recording with a stored consent).

| Check | Result |
| --- | --- |
| list cameras (CAM-4) | 1 camera, id `/dev/v4l/by-id/usb-Azurewave_Integrated_Camera_0001-video-index0`, kind built_in, 10 modes up to 1280x720 at 30 fps, metadata node not listed |
| steady frames (CAM-2) | 90 frames in 3 s at 720p, worst delivery latency 28 ms, capture stamps rise and never lead delivery |
| first picture | decoded (Motion JPEG), not black |
| camera track file | 6 s, 180 frames written, 0 dropped, 4 fragments of about 2 s, VP9 in fragmented MP4 (FFmpeg appends a 124-byte `mfra` index after the last fragment) |
| preview joins a recorder | preview at 10 fps got 30 frames in 3 s while the recorder kept 120 frames; recorder not disturbed when the preview left |
| unplug and replug (CAM-1 edge) | USB device unbound and bound again through sysfs: events `Disconnected` then `Reconnected`, pictures resumed (60 frames before, 243 after) |
| `rec.camera` through the daemon (CAM-1, CAM-2) | PASS: 1280x720, 30.0 fps, 5.27 s; stored report in the lab (`report-hardware.md`), 1 WARN line in the case's log |

## Not run

- Low light, USB webcams other than the built-in one, a second camera, VA-API or NVENC for the camera track (the camera track is software VP9), the sync flash test (CAM-2 50 ms) against the screen track, the live bubble and Settings page in the running app, X11 session, Wayland KDE, Ubuntu 22.04.
- In a debug build the Motion JPEG decoder is far too slow (about 4 fps); only release builds were measured.

Nothing here is "verified" until a person has read it and says so.
