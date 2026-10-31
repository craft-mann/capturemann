# L1 report: screen and audio recording on Linux in a headless VM

- **Date:** 2026-10-31
- **Who:** Claude (run for the maintainer; nobody watched the screen; the portal consent was not needed because a restore token from the L0 run was reused)
- **Machine:** QEMU virtual machine on a MacBook Pro (Apple M4 Pro), 4 vCPUs shared with other jobs, 8 GB, **aarch64**, Ubuntu 24.04.5, GNOME Shell 46 started with `--headless --wayland --virtual-monitor 1280x720` (software rendering, no `/dev/dri`, no `/dev/video*`), xdg-desktop-portal 1.18.4, xdg-desktop-portal-gnome 46.2, PipeWire 1.0.5 with WirePlumber. Audio: the "Dummy Output" sink (system audio) and a null `Audio/Source/Virtual` node created with `pw-cli` (microphone, silence).
- **Build:** release build of `crates/cm-capture/examples/record_linux.rs` at the commit that adds this file (`git rev-parse --short HEAD` after the commit), `CARGO_INCREMENTAL=0`.
- **FFmpeg:** `ffmpeg version 6.1.1-3ubuntu5` (the distribution's, GPL build with libvpx; development use only, not the product's bundled FFmpeg).
- **Fixture:** `weston-simple-shm` animating, the pointer moved in a circle through Mutter RemoteDesktop (`/tmp/wiggle.py`) so frames keep flowing; for audio `pw-play` of a 12 s stereo file with a 100 ms 1 kHz beep at 1, 3, 5, 7 and 9 s into the file.

This is **not** real hardware: no GPU, no hardware encoder, 720p, a virtual 60 Hz monitor, simple content (low encode cost). It shows that the Linux source, the session clock, the audio sources, the constant-frame-rate pipeline and the software VP9 encoder work together; it says nothing about 1080p/4K, DMA-BUF, VA-API/NVENC or other desktops.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1. Pure parts and fakes on macOS and in the VM (`cargo test -p cm-encode -p cm-capture -p cm-audio --lib`) | PASS | VM (release): cm-audio 80, cm-capture 97, cm-encode 33 passed. macOS (debug): cm-audio 86 passed, cm-capture and cm-encode all passed. New tests: `PtsMapper`, `frame_from_buffer`, token stores, source start/idle/stop with a scripted backend, `CpuFrame`, the VP9 encoder (gaps, duplicate slot, full queue, no frames). The VP9 test needs an FFmpeg with libvpx-vp9 and prints SKIPPED without one. |
| 2. Record 10 s, 30 fps, `--audio`, restore token | PASS | Screen: `delivered` 563 buffers from the compositor (about 56/s), `written` 301 frames, 0 dropped, 0 late, 0 repeated, 0 stalls, `first_frame_ms` 77. File: VP9 (`ffprobe` codec_name vp9), 1280x720, 301 frames, 6 fragments (five of 2.0 s), 5.4 MB (8 Mb/s target on static-ish content). No consent dialog (token reused, saved back through `FileTokenStore`). |
| 3. Frame gaps (ffprobe packet pts of `screen.mp4`) | PASS | 301 packets, pts 0.000 to 10.000 s, every gap exactly 33.33 ms (min and max equal), no missed slot. The compositor's own cadence is 17.3 ms median (L0). |
| 4. Same at 60 fps, video only | PASS | 600 frames written from 573 delivered: 27 repeated (the virtual monitor sends about 57 fps, the pipeline repeats the last frame to keep CFR), 0 dropped, 0 late. |
| 5. CPU of the whole recording, `/usr/bin/time` (includes the FFmpeg child) | measured | 30 fps: user 2.18 s + sys 0.82 s over 10.09 s wall = 0.30 core, max RSS 164 MB. 60 fps: 4.14 + 1.57 s over 10.06 s = 0.57 core, 165 MB. Content is nearly static, so a busy screen costs more. |
| 6. System audio (monitor of the default sink) | PASS | `system.wav`: 48 kHz stereo, 485,376 frames (10.112 s), no gaps, 0 resyncs, `max_offset_ms` 1.85, measured drift -12.1 ppm. Beeps found in the file at 1.042, 3.042, 5.042, 7.042, 9.042 s: intervals exactly 2.000 s (no drift over 8 s). First sample at session time 38 ms. |
| 7. Microphone (null source, silence) | PASS | `mic.wav`/`mic.denoised.wav`: 48 kHz mono, 484,352 frames (10.09 s), no gaps, 0 resyncs, `max_offset_ms` 2.62, drift +2.8 ppm. First sample at session time 43 ms. |
| 8. Track starts against the session clock | measured | screen first frame at 8,000 ticks (66.7 ms), mic 5,124 (42.7 ms), system 4,560 (38.0 ms); all positive (after the origin). Ends: screen 1,208,970, mic 1,216,004, system 1,218,000 ticks (10.07 to 10.15 s). |
| 9. A/V sync by flash and beep (AUD-1/REC-1 method) | not done | The VM has no source that flashes on a known beat. The audio-only check (beeps 2.000 s apart, in the right place after the `pw-play` start: first beep at session time 1.080 s, i.e. `pw-play` began playing about 80 ms after the session origin, plausible for its start-up) does not prove the video side. To do on real hardware or with a flash client. |
| 10. `cargo deny check` | PASS | "advisories ok, bans ok, licenses ok, sources ok". No new crate in `Cargo.lock`: `pipewire` 0.10.1 (MIT, with `pipewire-sys`, `libspa`, `libspa-sys`) was already in the lock through `xcap`; the change adds the edges cm-capture -> pipewire and cm-audio -> pipewire (Linux only) and the `screencast` feature of `ashpd` (MIT). One existing "yanked" warning (`clipboard-win`) is unrelated. |
| 11. `python3 scripts/check_target_os.py`, `cargo fmt`, `cargo clippy -D warnings` (macOS) | PASS | no new `target_os` lines outside `sys/` (and examples). Clippy on Linux was not run in the VM (build warnings are clean). |

## Acceptance criteria

None claimed. This is groundwork for REC-1 to REC-4 and AUD-1/AUD-2 on Linux (`specs/platform-acceptance.md` rows stay open).

## Problems found

- `cm-portal-gnome.service` (the VM's hand-started portal backend) had died with SIGSEGV at 05:27 (it was running a Screenshot request from another job) and every ScreenCast `create_session` then failed with "StartServiceByName ... Timeout". I started the unit again with `systemctl --user start cm-portal-gnome`; recording worked afterwards. Whoever debugs the Screenshot hang should know the backend can crash.
- The track details written by the recorder say `"codec": "h264"` and `"cursor": true` although the file is VP9 and the cursor is only in the pixels: `Codec` has no VP9 and the recorder takes both from the plan. To fix before the manifest is trusted on Linux.
- The wired mic path still uses `cpal` (ALSA) by default on Linux (`cpal_mic_factory`); the PipeWire factories exist (`pipewire_mic_factory`, `pipewire_monitor_factory`) and `SystemAudioRecorder::new` uses the PipeWire monitor on Linux, but the app and daemon were not touched (outside this task).
- `needs_reopen` (follow a default-device change) is not implemented for PipeWire.
- The bundled FFmpeg has no VP9 decoder: a VP9 master could not be edited or exported by the product until the decoder is added to the FFmpeg build (see `specs/plans/linux-recording-plan.md`, R8).

## Verdict

PARTIAL. Source, clock, audio, CFR pipeline and software VP9 work end to end in a headless VM; hardware encoders, real GPU, sync by flash and beep, and the product wiring are not done.
