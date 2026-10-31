# Linux recording plan (L1: screen + audio)

Status: design note, 2026-10-31. Covers how a Linux recording maps onto `ScreenRecorder<Source, Encoder>`
(`crates/cm-capture/src/record/`), what was built without a GPU in the Ubuntu 24.04 arm64 VM, and what is
left. Linux is still open at the maintainer (D-L1 to D-L5 in `specs/platform-acceptance.md`); this note names
mechanisms, it does not commit the product to Linux. Evidence so far: `verification/L0-portal-screencast-2026-10-31-claude.md`
(portal + PipeWire, 57.6 fps, a PTS on every frame, restore token) and
`verification/L1-recording-linux-2026-10-31-claude.md` (this work).

## 1. The mapping

| macOS piece | Trait / type | Linux piece |
| --- | --- | --- |
| `SckSource` | `record::ScreenSource` (`check_permission`, `start(plan, sink)`, `stop`; events `Frame{host_ns, frame}`, `Idle`, `Stopped`) | `PipeWireScreenSource<B, T>`: portal ScreenCast session (ashpd) + a PipeWire video stream. `B: PwBackend` is the OS part (real: `PortalPipeWire`, fake: scripted), `T: TokenStore` keeps the restore token (the caller picks the place; nothing is hard-coded). |
| IOSurface frame | `ScreenSource::Frame` | `cm_encode::CpuFrame`: BGRx/BGRA/RGBx/RGBA/NV12 bytes, size, stride, behind an `Arc` (cheap `Clone`, the recorder keeps the last frame to repeat it). DMA-BUF frames are a later `Frame` variant. |
| `mach_absolute_time` stamp | `host_ns` on `cm_platform::clock::monotonic_ns` | That is `CLOCK_MONOTONIC` on Linux, and PipeWire's `SPA_META_Header.pts` is on the same clock (the L0 spike saw a constant offset to local arrival). `PtsMapper` uses the PTS when it is within 500 ms of arrival, else the arrival time (and counts it), and never goes backwards. |
| `AVAssetWriter` + VideoToolbox | `cm_encode::VideoEncoder` (`append`, `finish`, `check`, `fragments_written`) | See section 3. Built and tested: `FfmpegVp9Encoder` (rawvideo on stdin, libvpx VP9, fragmented MP4 out). Hardware: VA-API / NVENC later, behind the same trait. |
| `MicRecorder`, `SystemAudioRecorder` | `cm_audio::AudioSource` (`format`, `start(sink)`, `stop`, `needs_reopen`) and `SourceFactory` | `PipeWireAudioSource` in `cm-audio` (`sys/linux_pipewire.rs`): mic = default source, system audio = monitor of the default sink (`stream.capture.sink = true`). Same `SourceSink::push(host_ns, f32 interleaved)` contract as `CpalSource`. `system_audio_support()` reports `pipewire_monitor`. |
| `SystemScreenRecorder::system(plan)` | alias in `record/mod.rs` | Wired on Oct 31, 2026 as `cm_capture::linux_screen_recorder` (`sys/linux_recorder.rs`, used by the daemon's `recording/sys/linux.rs`); the app's Record entry points stay hidden on Linux unless `CM_LINUX_RECORDING=1`. It is `ScreenRecorder<PipeWireScreenSource<PortalPipeWire, FileTokenStore>, <chosen encoder>>`. |

Things the macOS code assumes that Linux changes:

* **Consent.** There is no permission query. The portal shows its dialog on the first `start`; the restore token
  (persist mode "until revoked") makes later starts silent. `check_permission` therefore returns `Ok` and the
  dialog is the permission flow; a declined dialog is `permission_missing(screen)`. A revoked token makes the
  portal ask again (a dialog during a scripted agent start: needs the existing Confirm-tier handling and a
  clear message, see risks).
* **Target selection.** The compositor's dialog picks the monitor or window, not `CapturePlan.target`. The plan's
  source size comes from the first negotiated format, so `source_size` in the plan is a hint only.
* **Frames only on damage.** A still screen sends no buffers. The pipeline already repeats the last frame to keep
  a constant frame rate and has `Idle` for "unchanged"; the PipeWire source emits `Idle` from a 250 ms timer
  while no frame arrives so the stall watchdog does not fire on a quiet desktop.
* **Cursor.** Cursor mode "embedded" for now (the cursor is in the pixels). The metadata cursor (proven in L0)
  feeds `cm-cursor` later; the screen track must then use "hidden".
* **Excluding our own windows (REC-6)** cannot be done by the compositor on Wayland. Bubble and HUD are
  composited at export (the bubble already is); HUD windows will appear in a full-screen capture. Decide per
  window with the overlay work; not part of this task.

## 2. Audio

`cm-audio` already has `cpal` (ALSA host on Linux). It was not used because: (a) the ALSA host only reaches
PipeWire through the `pipewire-alsa` plug-in, which is not always installed; (b) system audio needs the monitor
of the default sink, which ALSA does not expose; (c) timestamps: cpal's ALSA capture time is the stream time,
PipeWire gives `pw_stream_get_time` on the monotonic clock. A native PipeWire source (the `pipewire` crate, MIT,
the same one the screen source uses) covers mic and monitor with one code path and no libasound/libpulse
requirement. PulseAudio-only systems are served by `pipewire-pulse` on every current distro; plain PulseAudio
and JACK are not supported (stated limit).

Timestamps: the capture time of a buffer is `pw_time.now` (monotonic ns at the last graph cycle) minus the
bytes still queued in the stream converted to time (`pw_time.queued`/`buffered`, `delay`). The recorder's
existing resync (`DEFAULT_RESYNC_MS`) absorbs the rest. `needs_reopen` returns true when the default sink or
source changed (metadata `default.audio.sink` / `default.audio.source`), so a Bluetooth headset connecting is
followed like a WASAPI default-output change.

## 3. Encoder: what is legal and what is testable

Rules in force: no GPL/AGPL in shipped binaries; the shipped FFmpeg is LGPL with no `libx264`; no software
H.264 encoder ships (OpenH264 was removed on Oct 24, 2026, maintainer decision (c): Cisco's patent grant covers
only Cisco's own binaries, so a source build carries the exposure; see `docs/project/plan.md` and
`specs/features/03-recording.md`, edge cases). `docs/project/platform-research-2026-10-30.md` already says
"Linux relies on hardware via VA-API/NVENC".

| Option | Legal for the product | Testable in the VM (no GPU) | Notes |
| --- | --- | --- | --- |
| **VA-API** H.264/HEVC/AV1 via FFmpeg `h264_vaapi` (LGPL, dynamic libva) | Yes, same position as VideoToolbox: the encoder is the user's driver and hardware (Intel iHD/i965, AMD Mesa radeonsi). Mesa's patent position is the distro's. | No: no `/dev/dri`. Fakes only. | Needs the bundled LGPL FFmpeg built with `--enable-vaapi` (libva is MIT, loaded at run time). In-process with the `cros-libva` crate (BSD-3) means writing the H.264 bitstream headers (SPS/PPS/slice) by hand, weeks of work, or a Rust packer; FFmpeg as a child process is the fast route. |
| **NVENC** via FFmpeg `h264_nvenc` / `hevc_nvenc` | Yes (ffnvcodec headers MIT, driver loaded at run time, the proprietary driver is the user's). | No. | Best coverage on NVIDIA desktops; same wrapper. |
| **V4L2 stateful (m2m)** | Yes | No (`/dev/video*` absent) | Raspberry Pi / ARM SoCs, Qualcomm. Rare on desktops; skip until a request. FFmpeg `h264_v4l2m2m`. |
| **Software VP9 (libvpx, BSD-3 + Google patent grant) in MP4 or WebM** | **Yes**, LGPL FFmpeg + libvpx is already the shipped export build (`packaging/ffmpeg/build-macos.sh` includes libvpx and libopus) | **Yes**, runs on the VM CPU | The testable path. Measured numbers in the verification report. Costs CPU: real-time VP9 at 1080p30 needs roughly 3 to 4 fast cores (`-deadline realtime -cpu-used 6..8`); 4K60 is not realistic in software. |
| Software AV1 (SVT-AV1 BSD-3-Clause-Clear + AOM patent licence, libaom BSD-2) | Yes (royalty-free by AOM licence) | Yes | Real-time SVT-AV1 at 1080p30 is feasible on 4+ cores but heavier than VP9 at equal quality, and decoders are rarer (YouTube accepts it, older editors do not). Candidate only if VP9 disappoints. |
| Software H.264 (OpenH264, x264) | x264 is GPL: **no**. OpenH264 source build: patent exposure, the Oct 24 decision removed it. | Yes | **Not proposed.** If the maintainer wants a no-GPU H.264 fallback on Linux, the option recorded on Oct 24 is Cisco's prebuilt binary, downloaded once after consent. Flagged as decision D-R1. |

Recommendation: encoder order on Linux = hardware H.264/HEVC (VA-API, then NVENC) when the driver accepts the
plan, else **VP9 in fragmented MP4** by libvpx (software, truthful message: "no hardware encoder; recording in
VP9, expect higher CPU use"), else fail with `hardware_encoder_unavailable` (the existing error; its text names a
Mac and needs a Linux variant). This keeps REC-2's "no software H.264" rule and gives every Linux box a working
recorder. It does change the macOS statement "no software fallback in v0.1" for Linux only: decision D-R2.

**Container.** The screen track file stays `screen.mp4` and stays fragmented (2 s fragments, REC-8 recovery
`truncate_to_last_fragment` works on any fMP4), so the bundle layout, the manifest and recovery do not change.
VP9 in MP4 (`vp09`) plays in Chrome, Firefox and Safari 14+; WebM stays an export format. FFmpeg's `mp4` muxer with
`frag_keyframe+empty_moov+default_base_moof` and `-frag_duration 2000000` writes it.

**Decode side (a hidden cost).** The editor and exporter decode masters with FFmpeg; the bundled build
deliberately has no VP9 or Opus *decoder* (see CLAUDE.md, `make ffmpeg`). Recording VP9 masters needs the native
`vp9` decoder (LGPL, built in) added to the component list; `packaging/` is outside this task. Hardware masters
(H.264) have no such issue.

**Process vs in-process.** The built encoder runs the FFmpeg binary as a child (`CM_FFMPEG`, next to the
executable, `PATH`) and feeds raw frames over a pipe. That departs from "FFmpeg only after recording" in
CLAUDE.md: on macOS and Windows the encoder is in-process. Reasons for it on Linux: VA-API and NVENC both need
FFmpeg's wrappers or a large amount of codec code, and a pipe costs one memcpy of the frame. Cost: a child
process to supervise (a crashed child = a halted recording, the fragments written so far stay valid), and
about 220 MB/s of pipe traffic at 720p60 (1080p60 is about 500 MB/s; fine on a modern kernel, measured in the
report). Alternative for the software path: link libvpx in-process (`env-libvpx-sys`/`vpx-encode` crates, BSD)
and write fMP4 with the existing `FragmentedMp4Writer` (needs a `vp09` init segment builder). Decision D-R3.

## 4. What was built (VM-testable, no GPU)

* `cm-encode`: `CpuFrame`, `PixelFormat`; `FfmpegVp9Encoder` (VideoEncoder) with a bounded queue, CFR duplicate
  filling, stride repacking, and tests (skipped without an FFmpeg that has libvpx-vp9).
* `cm-capture` `record::pipewire`: `PtsMapper`, `frame_from_buffer` (stride/format checks), `TokenStore` +
  `MemoryTokenStore`/`FileTokenStore`, `PwBackend` trait, `PipeWireScreenSource` implementing `ScreenSource`,
  and a scripted backend. All pure Rust and tested on macOS. The Linux part is `sys/pipewire_linux.rs`
  (`PortalPipeWire`: portal via ashpd + PipeWire stream, adapted from `spikes/spike-portal-capture`; a
  stub that answers `not_implemented` on other systems, so callers compile everywhere).
* `cm-audio` `sys/linux_pipewire.rs`: `PipeWireAudioSource` (mic, sink monitor); pure helpers (sample
  conversion, timestamp, device naming) tested everywhere.
* Dependencies (Linux only, `[target.'cfg(target_os = "linux")'.dependencies]`): `pipewire` 0.10 (MIT), its
  `pipewire-sys`/`libspa`/`libspa-sys` (MIT), `bindgen` at build time (BSD-3), `ashpd` with the `screencast` feature
  (MIT, already used for screenshots). Build needs `libpipewire-0.3-dev` and `libclang`; `cargo deny` result in
  the verification report.

## 5. Risks

| # | Risk | Mitigation |
| --- | --- | --- |
| R1 | No hardware encoder on many VMs/old GPUs; software VP9 too slow for 4K60 or on weak CPUs. | Plan fallback by size (`smaller_plans` already exists); cap software recording at 1080p30; say so in the UI. |
| R2 | DMA-BUF buffers: GPU compositors prefer them; the CPU path costs a copy and may be refused by some compositors on high resolutions. | Offer MemFd/MemPtr first (what L0 measured), add DMA-BUF import into VA-API later (zero copy). Measure on real GPU. |
| R3 | Restore token revoked or not honoured (KDE and GNOME differ; Flatpak permission store vs host). | Treat as "dialog again"; surface a typed error; never loop. Test on KDE 6 and GNOME 46/48. |
| R4 | Dialog during an agent-started recording. | Confirm-tier already requires an on-screen user; document that the first run per token needs a click. |
| R5 | PipeWire PTS wrong on some compositors (0 or non-monotonic). | `PtsMapper` fallback to arrival time, counted in track details. Test on wlroots/KDE. |
| R6 | A/V sync: the audio stamp estimate (`pw_time`) has ~10 ms error; Bluetooth latency is not reported. | Existing `stamp_offset_ms` setting + the clap/flash measurement of the manual scripts (REC-1, AUD-1 on Linux). |
| R7 | The FFmpeg child dies or stalls (back pressure). | Bounded queue returns `NotReady` (counted as late frames by the pipeline), `check()` reports a dead child as `job_failed`; fragments already written are recoverable. |
| R8 | VP9 masters undecodable by the bundled FFmpeg. | Add the `vp9` decoder to `build-macos.sh`-style lists (and the Linux build script) before shipping the fallback. |
| R9 | X11 sessions: no portal needed, `xcap` works for screenshots, but recording needs XShm/XComposite or the portal too (GNOME on X11 offers the portal). | Use the portal on X11 as well when present; native X11 capture is out of scope. |
| R10 | Headless-VM numbers do not transfer: no GPU, 720p, 60 Hz virtual monitor. | The report says so; real-GPU runs are a manual script (`tests/manual/REC-1-linux.md`, to write). |
| R11 | `pipewire` crate API churn and the libclang build dependency on user build machines. | Pin `=0.10.x`; document in `CONTRIBUTING`; CI image already has libclang for bindgen? (check). |
| R12 | Global stop/start hotkeys do not exist on GNOME 46 (no GlobalShortcuts portal). | Not recording-specific; launcher/tray route in the L1 plan. |

## 6. Task breakdown (can be split across agents)

Disjoint files per task; all behind the traits above.

1. **Encoder wiring** (cm-encode, packaging): done for VP9. Next: VA-API (`h264_vaapi`) and NVENC variants of the
   child-process encoder (same struct, other args and `-vaapi_device`/`-init_hw_device`), selection logic and
   `hardware_encoder_unavailable` text for Linux. Needs a machine with a GPU for the tests that matter.
2. **Zero-copy path**: DMA-BUF negotiation in the PipeWire source (modifiers), a `Frame` variant, VA-API import.
   Needs a real GPU.
3. **In-process libvpx encoder** (D-R3): `vp09` init segment in `cm-encode::fmp4::synth` style, packet-to-fragment
   mapping, tests against a software decoder in CI.
4. **System wiring**: `SystemScreenRecorder` for Linux in `record/mod.rs`, `cm-daemon` backend construction,
   token store location (`~/.local/share`/XDG state), settings for "recording indicator" (Wayland shows its own).
5. **Audio**: finish device list for Linux (`list_audio_devices` through PipeWire registry), default-device
   follow (`needs_reopen`), mute/denoise unchanged; manual script `AUD-1-linux.md`.
6. **Cursor track** from PipeWire cursor metadata (L0 proved it) into `cm-cursor`; the screen stream switches to
   cursor mode "hidden".
7. **Window and region targets**: portal source types, crop of a monitor stream for regions.
8. **Recovery/soak**: `recording.lock` and `kill -9` on Linux (REC-8), a 30 min software-VP9 soak, disk-full.
9. **Docs/acceptance**: update `specs/platform-acceptance.md` rows REC-1..REC-8 for Linux, `docs/project/spikes.md`.

## 7. Decisions for the maintainer (decided Oct 31, 2026)

* **D-R1** Software H.264 fallback on Linux: **no.** (Cisco's binary after consent stays a later option.)
* **D-R2** Software VP9 (in MP4) as the Linux fallback when no hardware encoder is usable: **yes**, labelled
  as a software encoder. Changes "no software fallback" for Linux only; macOS is unchanged.
* **D-R3** Linux encoding through an FFmpeg child process behind `VideoEncoder`: **yes** for the early build, as a
  deliberate exception to "FFmpeg only after recording" that applies to Linux only. An in-process libvpx and
  VA-API encoder can replace it later without touching callers. The macOS recorder does not use any of the new
  types (`CpuFrame`, `FfmpegVp9Encoder`, the PipeWire source); they compile there for their tests only.
* **D-R4** Add the VP9 decoder to the bundled LGPL FFmpeg (with D-R2): **yes.**
* **D-R5** Minimum stack for the early build: **PipeWire only**, portal v4+ with persist. Extend later.

## 8. Result of the VM run and what it did not settle

`verification/L1-recording-linux-2026-10-31-claude.md`: 10 s at 30 fps, 1280x720, headless GNOME 46, restore
token (no dialog): 301 frames in `screen.mp4` (VP9, fragmented MP4), every gap exactly 33.33 ms, no late
or dropped frames, 0.30 core of CPU in total (ffmpeg included); at 60 fps 600 frames, 0.57 core, 27 repeated frames
(the virtual monitor delivers 57 fps). System audio through the sink monitor: five 1 kHz beeps 2.000 s apart
in the file. Mic: PipeWire null source (silence), 2.8 ppm drift. Not settled: A/V sync by eye or by a flash
and a beep (no flash source in the VM), anything on a real GPU, 1080p and above, DMA-BUF, and windows/regions.
Known wrinkles to fix when wiring the product: the track details say `codec: h264` (the plan's codec) while the file
is VP9 (`Codec` has no VP9 variant yet, `used_plan()` cannot say so); `cursor: true` is reported but the cursor is
only in the pixels.

**Update (Oct 31, 2026, product path):** the wrinkles above are fixed: the recorder describes a VP9 track with
`cm_core::describe_video_track` (codec `vp9`, encoder, `software`, `cursor_source`). The daemon, CLI and MCP record on
Linux (`verification/L1-recording-product-linux-2026-10-31-claude.md`, partial).
