# Spike results

Spike code lives in `spikes/` (its own Cargo workspace, not part of the product build). Run with `cargo run --manifest-path spikes/Cargo.toml -p spike-shot` from the repo root. Windows results are pending for every spike (no Windows machine yet).

Fill this in as you run each spike. A spike passes only when it passes on **both**
Windows and macOS. If it fails, pick the fallback and note why.

## Machines

| | Windows | macOS |
| --- | --- | --- |
| Model / CPU / GPU |  | MacBook, Apple M4 Pro |
| OS version | | macOS 26.6.2 (25G83) |
| Displays (resolution, scale) |  | Built-in 3024x1964 px, scale 2.0 (notch) |
| FFmpeg version | | |

## S1 `spike-shot`

**Pass:** every display captured at physical size; warm capture < 150 ms per 4K display;
windows listed with titles and captured with the right content.

| Check | Windows | macOS |
| --- | --- | --- |
| Display sizes correct (paste table row) | | |
| Warm capture ms (largest display) | | |
| Windows captured / attempted | | |
| PNGs look right (sharp, correct content) | | |
| **Result** |  | Pass (reported Oct 19) |

Fallback if it fails: native per-OS screenshot calls (WGC single frame, ScreenCaptureKit screenshot).

## S2 `spike-overlay`

**Pass:** window maps 1:1 to the capture ("1:1 mapping" line); a crop of a known UI
element (e.g. a 1 px divider) matches what you dragged to the pixel on a 1x and a 2x
display; overlay visible < 150 ms after start.

| Check | Windows 1x | Windows 2x/150% | macOS Retina | macOS external 1x |
| --- | --- | --- | --- | --- |
| 1:1 mapping line |  |  | Yes, offset (0, 0), 3024x1964 |  |
| Crop pixel-exact |  |  | Yes, 420x649 drag = 420x649 crop |  |
| Overlay visible ms |  |  | 82 hotkey path (capture 67 + frame 15); 162 cold |  |
| Menu bar / taskbar covered |  |  | Yes, incl. notch strip |  |
| **Result** | | | Pass | |

Fallback if it fails: per-monitor overlay windows; `wgpu` rendering if software
rendering is too slow at 4K.

## S3 `spike-record`

**Pass:** 60 s and 10 min at 1080p30 with ≤ 1% late frames and no growing lag; audio
drift ≤ 50 ms; flash-vs-beep offset ≤ 50 ms by eye; CPU < 30% on a mid-range laptop.

| Run | OS | Encoder | Late % | Max / final lag ms | Audio drift ms | Flash vs beep | CPU % | Result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 60 s | Windows | libx264 | | | | | | |
| 60 s | Windows | hw | | | | | | |
| 10 min | Windows | best | | | | | | |
| 60 s | macOS | libx264 | | | | | | |
| 60 s | macOS | hw | 0.06 | 52.8 / 2.7 | +0.2 | +21 ms audio after video (auto-measured, 21 pairs, no drift) | (pending) | Pass, CPU pending |
| 10 min | macOS | best | | | | | | |

Fallback if it fails: let FFmpeg capture the screen itself for v0.1
(`-f ddagrab` / `gdigrab` on Windows, `-f avfoundation` on macOS) and keep native
capture for v0.2.

## S4 `spike-cursor`

**Pass:** event timestamp within 5 ms of the posted time; no dropped clicks (every down has
an up, 20/20); cursor image + hotspot extracted (including Retina scale); CPU negligible.
Run: `cargo run --manifest-path spikes/Cargo.toml -p spike-cursor -- record --secs 10`
(or `selfcheck`, which synthesizes events with CGEventPost and moves the real cursor).

| Check | Windows | macOS |
| --- | --- | --- |
| Event tap / hook installs, permission reported gracefully |  | Tap created (Input Monitoring granted); denied path prints `permission_missing` and falls back to 120 Hz position polling (denied path not exercised) |
| Timestamp error vs. posted time (ms) |  | Not measured: CGEventPost needs Accessibility, which this terminal lacks (silently dropped); zero events reached the tap |
| Dropped clicks (down/up seen / posted) |  | Not measured (same reason) |
| Cursor image + hotspot + scale |  | Works: I-beam 18x36 px at 2.0x, hotspot (8,18); arrow rep 280x400 px at 10x, hotspot (50,50); PNGs checked by eye. Uses deprecated `NSCursor.currentSystemCursor` |
| Idle CPU (release, tap + 30/4 Hz image poll) |  | 0.36% of one core over 10 s (cursor idle) |
| **Result** |  | Partial: cursor images and CPU pass; timestamp accuracy and click completeness unverified |

Notes: the tap callback only stamps and sends to a writer thread. `CGEventGetTimestamp` is
checked at runtime for mach ticks vs ns. Positions are scaled from points to physical px of
the main display only (multi-display and `outside: true` are not done). Cursor images are
detected by polling, so a shape change is stamped up to 33 ms late. The Windows code
(`WH_MOUSE_LL` + `GetCursorInfo`, QPC stamps) is written but was never compiled or run.

Fallback if it fails: poll position at 120 Hz (`CGEventCreate` / `GetCursorPos`) and take
clicks from the capture library or a lower-rate poll of button state.

## S5 `spike-ui-iced` and `spike-ui-gpui`

**Pass:** one framework covers the editor canvas, library grid, settings, publish sheet, tray, hotkeys and native dialogs; pure Rust, Apache-2.0-compatible; macOS 13+ and Windows 10; idle RSS < 100 MB (NFR-5); keyboard-only operation and screen-reader labels (NFR-13); a custom preview surface fed by a wgpu texture; dark Overlay vs flat Studio theming (docs/development/design-surfaces.md); stable enough to ship.

Prototype (each): sidebar + preview area (black letterbox) + 12-card library grid + primary "Export" button with a "Cmd+E" keycap, docs/development/design-surfaces.md dark tokens, a 640x360 CPU-generated animated plasma as the preview. Run: `cargo run --release --manifest-path spikes/Cargo.toml -p spike-ui-iced` (or `-p spike-ui-gpui`; `SPIKE_STATIC=1` stops the animation). Numbers are release builds on macOS (Apple M4 Pro, 1000x680 window), read with `ps` after 3, 8 and 15 s.

| Check | iced 0.14.0 | GPUI (`gpui-pre` 0.3.9) |
| --- | --- | --- |
| Builds on this machine (CLT only, no Xcode) | Yes, as is | Not as is: `gpui-pre-apple` build.rs runs `xcrun metal`. Fixed with the `runtime_shaders` feature (Metal shaders compiled at run time) |
| Exact failure | n/a | `cargo::error=metal shader compilation failed: xcrun: error: unable to find utility "metal", not a developer tool or in PATH` (gpui-pre-apple 0.3.9 build.rs). Cost to fix with the full toolchain: install Xcode (about 10 GB) plus `xcodebuild -downloadComponent MetalToolchain`; or use `runtime_shaders` (one feature flag, small startup cost) |
| Second build gotcha | none | Without the `font-kit` feature on `gpui-pre-platform` the window opens but no text is drawn anywhere (verified by screenshot); with it, text is correct |
| Idle RSS, static window (MB) | 92.7 to 95.5 (5 runs) | 73.5 to 75.9 (5 runs) |
| RSS, animated preview (CPU plasma, 60 Hz) (MB) | 94 to 95.5 | 77 to 79 |
| CPU, static / animated | 0.0% / 20 to 23% | 0.0 to 0.7% / 32 to 35% (plasma generation on the main thread dominates; not a framework figure) |
| First frame (process start to first render/pipeline), ms | 147 to 188 warm (735 once, cold) | 103 to 149 warm (448 once, cold) |
| Release binary | 12.9 MB | 13.0 MB |
| Dependency crates | 405 | 612 |
| Preview from a wgpu texture | Yes, first-class: `iced::widget::shader` gives the widget `&wgpu::Device/Queue` and a render pass on the app's own wgpu 27 device. Prototype uploads a texture each frame and draws it with a WGSL fullscreen triangle. The compositor must use the same wgpu major version (27) | No. GPUI renders with Metal (macOS) and DirectX 11 (Windows); wgpu is used only on Linux. The only external-pixel element is `surface()`, which takes a `CVPixelBuffer` on macOS and does not exist on Windows. Prototype uses `img(Arc<RenderImage>)` (CPU upload through the atlas, letterboxed correctly) |
| Accessibility (NFR-13) | None in 0.14. AccessKit work (iced #552, PRs #1849, #3111) is open and not merged; no `accesskit` in the dependency tree | In-tree AccessKit: `role()`, `aria_label()`, focus, actions (`a11y.rs` example); `accesskit` 0.24 and `accesskit_macos` in the tree. Active (macOS fixes in Sep 2026, open PR #65740 on Oct 19). Not tested with VoiceOver or Narrator here |
| Keyboard-only | `keyboard::listen` subscription and focusable buttons; no focus ring styling out of the box | Actions + key bindings, `tab_stop`, `focus_visible` styles (used for the docs/development/design-surfaces.md focus ring) |
| Theming per docs/development/design-surfaces.md | Per-widget style closures; all tokens reproduced | Tailwind-style builder; all tokens reproduced, incl. 16% brand tint |
| Fonts / tabular numbers | cosmic-text (OpenType features limited; verify `tnum`, `ss03`) | CoreText / DirectWrite (verify `tnum`, `ss03`) |
| Backdrop blur (overlay.panel) | Not provided | Not provided (overlay is in apps/overlay anyway) |
| Tray, global hotkeys | Not in iced: `tray-icon` and `global-hotkey` crates on the winit loop | Not in GPUI: no tray API (dock menu and app menus only); needs `tray-icon` on the platform thread, untested with GPUI's own run loop |
| Native file dialogs | `rfd` | Built in: `cx.prompt_for_paths` / `prompt_for_new_path` |
| License (own crate) | MIT | Apache-2.0 |
| Non-MIT/Apache transitive licenses | Zlib, BSD, ISC, BSL-1.0, CC0, dual LGPL/GPL choices (`r-efi`, `self_cell`) with a permissive option | Same, plus MPL-2.0 (`cbindgen` build-time, `dwrote`, `option-ext`: file-level copyleft, needs a notice and cargo-deny allowance) and Unicode-3.0 |
| Release cadence | 0.14.0 on 2025-12-07, none since (10 months); `master` very active (commits through Oct 17, 2026) | `gpui-pre` weekly snapshots of zed (0.3.9 on 2026-10-12); the `gpui` crate itself is stuck at 0.2.2 (Oct 2025). No API stability promise; edition 2024 |
| Windows 10 | winit + wgpu (DX12/Vulkan); not run | Win32 + DirectX 11 + DirectWrite; Zed ships on Windows; not run |
| **Result** | Fails NFR-13 (no accessibility); RSS within 5 MB of the NFR-5 budget before any real UI | Passes NFR-13 and NFR-5; fails the wgpu-texture requirement as is |

Not verified: Windows (both), VoiceOver/Narrator behavior of GPUI's AccessKit tree, whether GPUI `tnum`/`ss03` features work, tray and global hotkeys on either, a 1080p60 preview through GPUI (CPU upload cost measured only at 640x360), zero-copy from a wgpu Metal texture to GPUI's `surface()` (IOSurface-backed `CVPixelBuffer`) and any Windows equivalent, and long-run memory growth. The animated CPU figures include plasma generation and are not framework overhead. RSS will rise with a real editor, thumbnails and the compositor's textures.

Fallback if GPUI's preview path is too costly: iced 0.14 (or its master) for the shell with an AccessKit fork (COSMIC's) accepted as a risk, or GPUI with the preview as a separate native child surface fed by the compositor.

### S5 follow-up: 1080p60 preview through GPUI (Oct 22, 2026)

**Status: tested on this Mac, no stored report.** Numbers below are from the author's runs of `cargo run --release -p cm-desktop --example preview_bench -- [surface|cpu] [seconds]` (MacBook Pro, Apple M4 Pro, macOS 26, built-in 3024x1964 Retina display, 1280x720 point window, so the 1080p frame is scaled down). Source: synthetic NV12 frames from a worker thread paced at 60 fps (`apps/desktop/src/preview/`: `SyntheticSource` with a 32-bit frame counter and a moving bar, `PresentStats`, `PreviewSurface`). No real decoder, no compositor, no capture permissions. Windows: not run, and the `surface` path does not exist there. Other builds and agents were running on the machine during the runs, so CPU figures vary between runs (surface 17% and 28% for 20 s and 8 s runs).

| Path | New frames shown per second | Gaps over 33 ms (a missed 60 fps slot) | Gaps over 25 ms (one 120 Hz refresh late) | Process CPU (all threads, one core = 100%) | RSS |
| --- | --- | --- | --- | --- | --- |
| `surface()`: IOSurface-backed NV12 (`420f`) `CVPixelBuffer`, GPU sampled through a CoreVideo Metal texture cache | 60.0 (20 s), 59.6 (8 s) | 0 (20 s), 3 (8 s) | 47 of 1224 | 17% (20 s), 28% (8 s); the source thread's fill costs ~1 ms per frame | 265 MB, flat between the 8 s and 20 s runs |
| `img()`: NV12 to BGRA on the CPU (3.8 ms per frame on the worker), 8 MB atlas upload per frame | 60.0 (20 s) | 0 (20 s) | 42 of 1223 | 36% (20 s), 38% (8 s) | 112 MB |

What the runs show:
- **GPUI can show a steady 1080p60 stream on macOS.** Both paths reach 60 fps with no visible hitch in the 20 s runs; the display refreshes at up to 120 Hz, so about half of the redraws carry no new frame and a 60 fps frame sometimes lands one refresh (25 ms) late. That jitter is quantization, not a missed frame.
- **`surface()` is the cheaper path** (about half the CPU of the upload path, and the pixels never pass through the CPU after the decoder). It needs a `CVPixelBuffer` in NV12 full range: GPUI's Metal renderer asserts `420YpCbCr8BiPlanarFullRange` and panics on any other format, so the wgpu compositor's BGRA output cannot be handed over as is. Options, none tried: render the preview in NV12, convert on the GPU, or read back and use the CPU path. Whether a wgpu Metal texture can be shared with an IOSurface-backed `CVPixelBuffer` without a copy is unproven.
- **The CPU path works but has two traps.** The frame must be BGRA, and every replaced image must be removed with `cx.drop_image(old, Some(window))` from inside `render`; with `None` the window being drawn is out of `App.windows`, nothing is freed, and RSS reached 8 GB in 22 s (8 MB retained per frame). Cost scales with pixels: 4x at 4K, so a preview should be rendered at 1080p or less.
- **Windows has no `surface()`.** The preview there is the CPU upload path (or a child surface), unmeasured on Windows. At 1080p it cost about 36% of one core on the M4 Pro.

**Recommendation: GPUI stays (go) for the editor preview on macOS, using `surface()`; do not switch to iced for this reason.** Conditions before M4 relies on it: (1) show the same numbers with a real decoded stream and the real compositor (this test used synthetic frames), (2) decide how the compositor's output reaches an NV12 `CVPixelBuffer`, (3) run the Windows CPU path on a Windows 10 machine, (4) store a report (`verification/`) from a run on a quiet machine. If (2) forces a CPU readback on macOS, the `img()` path at 1080p (about 36% of a core) is the fallback and still holds 60 fps here.

**Follow-up, decode side (Oct 22, 2026, timing probe on this Mac, not a stored report).** `cm_media::decode` streams a test pattern (`testsrc2`, H.264 software-encoded fixtures, 30 fps) through one long-lived FFmpeg process to a 1280-wide preview (`cargo test -p cm-media --test decode --release -- --ignored --nocapture`). Release build, Apple M4 Pro: 1080p source to RGBA 975 fps sequential, to NV12 1,866 fps; 4K source to RGBA 523 fps, to NV12 723 fps; first picture 41 to 100 ms; a scrub (restart at a new time) 45 to 130 ms. Real screen recordings (HEVC from VideoToolbox, real content) were not measured, and the editor window with this decoder, the compositor and the audio clock has not been run by eye, so condition (1) above is half answered: the decoder is far faster than 60 fps, the end-to-end frame cost in the window is unmeasured.

## S6 `spike-encode`

**Pass (M0 gate):** 4K60 (or the display's native size at 60 fps) screen + 1080p camera recorded in-process with hardware encoders to 2-second fragmented MP4, frames passed as GPU surfaces with no CPU copy; <= 1% late frames; screen/camera sync within 50 ms; the camera bubble absent from the screen track.
Run (macOS, repo root; outputs in `spike-out/encode/`, git-ignored):

```text
cargo build --release --manifest-path spikes/Cargo.toml -p spike-encode
./spikes/target/release/spike-encode preflight         # prints which permission is missing and how to grant it
./spikes/target/release/spike-encode verify-bubble     # bubble-absent test with controls (needs Screen Recording)
./spikes/target/release/spike-encode run --secs 60     # HEVC, native 3024x1964, 60 fps, camera if authorised
./spikes/target/release/spike-encode run --secs 60 --codec h264 --width 3840 --height 2160
```

Design as built (macOS): `SCStream` delivers IOSurface-backed `CMSampleBuffer`s on a dispatch queue; each is retimed (a new `CMSampleBuffer` around the same pixel buffer, PTS = host-clock ns converted to 120,000/s ticks since a shared origin `t0`) and handed to `AVAssetWriterInput.appendSampleBuffer`, whose VideoToolbox output settings do the hardware encode. This process never maps the pixels (only `verify-bubble` locks single frames, on purpose). The camera is `AVCaptureSession` + `AVCaptureVideoDataOutput` feeding a second `AVAssetWriter` started at the same `t0`. Bindings are plain `objc2-*` crates (no Swift bridge). Windows (WGC + Media Foundation MFT + `WDA_EXCLUDEFROMCAPTURE`) is not written: it cannot be built or run here.

**Measured on macOS (MacBook Pro, Apple M4 Pro, built-in display 3024x1964 @ scale 2, 120 Hz; screen only, camera not run):**

| Run (HEVC unless noted) | Size @ fps | Late % | Achieved fps | Max / final lag above median ms | Process CPU (of one core) | RSS max MB | Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 60 s HEVC native | 3024x1964 @ 60 | 0.28 | 59.84 | 10.2 / +0.2 | 4.2% | 123 | Pass |
| 60 s H.264 native (90 Mbps target) | 3024x1964 @ 60 | 0.31 | 59.97 | 10.7 / +0.3 | 3.8% | 131 | Pass |
| 60 s HEVC 2160p | 3840x2160 @ 60 | 0.31 | 59.98 | 12.8 / +0.1 | 4.2% | 90 | Pass |
| 60 s H.264 2160p | 3840x2160 @ 60 | 0.33 | 59.97 | 12.4 / -0.2 | 4.0% | 82 | Pass |
| 30 s HEVC native, BGRA capture | 3024x1964 @ 60 | 0.72 | 59.71 | 11.4 / +0.0 | 6.3% | 148 | Pass (closer to the limit than 420v) |
| 300 s HEVC native | 3024x1964 @ 60 | 0.26 | 59.93 | 14.1 / +3.6 | 4.1% | 133 | Pass, no lag growth |
| 10 min / 30 min soak, Windows | | | | | | | not run |

How "late" is counted (same spirit as S3): a frame is late if it reached the writer more than one frame interval behind the steady-state delivery lag (median of host clock now minus PTS), or never made its 60 Hz slot (the larger of the shortfall against the expected count and the PTS-gap misses, plus not-ready drops and append failures). "Lag" is that delivery lag relative to the median; "final" is the mean of the last second, so a value near 0 means no growth.

| Check | Windows | macOS |
| --- | --- | --- |
| Screen Recording / Camera permission | | Screen Recording granted to Terminal.app (preflight). Camera: **not determined**, so the camera track was not run |
| Capture at native size, 60 fps, GPU surfaces | | Yes: 3024x1964 (points x `pointPixelScale`), 420v (NV12) or BGRA, IOSurface buffers straight into the writer |
| In-process hardware encode | | HEVC (`hvc1`, Main) and H.264 (High) via `AVAssetWriter` VideoToolbox settings. Encoding runs in `VTEncoderXPCService` (about 2.4% of one core), so it is not in the process CPU figure; `top` also showed WindowServer at about 75% (the animated test window plus the 120 Hz display), `replayd` 1.4% |
| 2 s fragmented MP4 | | Yes, with the segment delegate (`outputFileTypeProfile` CMAF + `preferredOutputSegmentInterval` 2 s): moov first, one `moof` per 2.00 s (min 1.92, max 2.02 s, measured from `tfdt`), 151 fragments in 300 s; `ffprobe` reads it, full decode with FFmpeg has no errors. The plain `AVAssetWriter` `movieFragmentInterval` (mp4 and mov) did **not** fragment: one `mdat` and the `moov` at the end, not crash-safe |
| Tracks on one clock (120,000 ticks/s) | | Yes: media timescale 120000, first screen PTS about 0.09 s after `t0`, file `start_time` matches. Camera track untested |
| Camera 1080p H.264 second track | | Built, **not run** (permission). Camera frame stamping, 8 Mbps H.264 writer and the bubble preview layer are unverified |
| Bubble excluded from capture | | Yes (`verify-bubble`, full 3024x1964 BGRA frame, bubble = solid magenta circle, control first): control 100.0% magenta in the bubble area; `sharingType = none` alone 0.0%; `SCContentFilter` window exclusion alone 0.0%; both 0.0%. Also checked on the recorded HEVC file (frame at mid-run, decoded with FFmpeg): exclude none 100.0% present; sharing, filter, both 0.0% |
| Pause | | Not built (pause only stops appending; not part of this spike) |
| **Result** | | Screen path passes the gate at native 60 fps and 2160p60 (0.26 to 0.33% late, 4% of one core, under 150 MB). Camera, camera/screen sync and a real-content run are unverified |

Findings that change the design:

- **Frame pacing:** with `minimumFrameInterval` = exactly 1/60 on this 120 Hz display ScreenCaptureKit delivered 54.3 to 55.3 fps (2.8 to 3.8% late; gaps of 25 ms, i.e. three 120 Hz vsyncs instead of two). Setting the interval to 1/66 (1.1 x the target) gives 59.8 to 60.0 fps; 1/62 gave 59.1 to 59.6. Use 1.1 x fps as the SCK interval and let the encoder frame rate stay at 60. On a 60 Hz display this is untested.
- **B-frames:** with frame reordering on, the CMAF segment writer fails after a few seconds (`-11800`, underlying `-16364`) at both 3024x1964 and the default settings; `AllowFrameReordering = false` fixes it. Cost: some compression efficiency; measured sizes are with B-frames off.
- **Timestamps:** in segment mode the writer does not rebase to the session start, and ScreenCaptureKit leaves the sample duration invalid; both must be set on the retimed sample buffer (PTS since `t0`, duration = 1 frame). Retiming creates a new sample buffer that shares the same pixel buffer: no pixel copy.
- **Bubble:** on macOS 26.6 `NSWindow.sharingType = .none` still hides the window from ScreenCaptureKit display capture, and so does `SCContentFilter` exclusion by window id; using both costs nothing and covers older or changed behaviour. The bubble window uses level 1000 (screen-saver), `ignoresMouseEvents`, `canJoinAllSpaces`.
- **Content realism:** the test screen is a 900x600 window of rotating coloured bars (Core Animation render server, so every 120 Hz vsync changes) plus whatever is on screen. Bitrates (21 to 30 Mbps HEVC at 4K-class, below the 60 Mbps target) reflect that content, not worst case; a real 4K60 gameplay or video capture will load the encoder harder. Unmeasured.
- **Build fix for the `spike-record` link failure (S3 binary):** root cause is not the Swift runtime but the search path. `apple-metal`'s Swift bridge force-loads `swiftCompatibility56` and `swiftCompatibilityPacks`; the dependencies' build scripts add `-L <xcode-select -p>/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx`, which does not exist with Command Line Tools only. Those static libraries are in `/Library/Developer/CommandLineTools/usr/lib/swift/macosx`. Fix in `spikes/spike-record/build.rs`: also emit `cargo:rustc-link-search=native=<xcode-select -p>/usr/lib/swift/macosx` (next to the existing `-rpath /usr/lib/swift`). `cargo build -p spike-record` now links and starts (the `CoreAudioTypes` message stays a harmless warning). The product build needs the same two lines in its own `build.rs` if it keeps `screencapturekit`. `spike-encode` avoids the problem altogether by using `objc2-*` bindings and no Swift bridge (no rpath or search path needed); that is the recommended route for the product too, at the cost of writing the delegate classes by hand (about 150 lines here).

Not verified, and how to finish:

1. **Camera track, camera/screen sync (50 ms), bubble over the live camera.** Grant Camera access to the app that runs the command: System Settings > Privacy & Security > Camera, enable Terminal (or iTerm / VS Code). If Terminal is not listed, run once from Terminal.app: `./spikes/target/release/spike-encode preflight` then `./spikes/target/release/spike-encode run --secs 20 --request-camera` and click Allow; quit and reopen Terminal if needed. Then run: `./spikes/target/release/spike-encode run --secs 60 --camera on --bubble camera` and, for the marker variant that is machine-checked, `--bubble marker`. Check: camera late % <= 1, camera first-PTS offset and delivery-latency difference printed under `--- sync`, and the `ffprobe` start times of both files. Camera PTS is taken to be on the host clock (assumed, not yet seen: the run prints the camera's median host-now minus PTS, which should be a few tens of ms, not negative or seconds). The optical sync figure (flash on screen vs the same flash in the camera) needs a second device or a mirror and was not attempted.
2. **Soak:** `--secs 1800` (about 6.5 GB) for the 30-minute NFR-2 run, with webcam, mic and system audio added by the product.
3. **Other displays:** a 60 Hz external display and a true 4K display (the `1.1 x fps` interval was only measured on this 120 Hz panel). A mid-range Windows NVENC machine for the whole spike.
4. **Worst-case content** (full-screen video or game at 4K60) for encoder load and bitrate.

Fallback if the camera track or 4K60 on a weaker Mac fails: record 2160p30 or native 30 fps (the code path is the same, `--fps 30`), HEVC instead of H.264 to cut bandwidth, camera at 720p (`--camera-res 720`). Keep the CMAF segment writer: the plain-file writer with `movieFragmentInterval` does not fragment. If hardware encode misbehaves, `SCRecordingOutput` (macOS 15+) is the next alternative for the screen track.

## Decision

| Spike | Go / fallback | Notes |
| --- | --- | --- |
| S1 | Go on macOS; Windows pending | Passed on macOS (reported Oct 19); spike code in `spikes/spike-shot` |
| S2 | Go on macOS; Windows and 1x display pending | Fullscreen keeps windows below the notch (3024x1898): use a borderless window at screen-saver level with the full screen frame |
| S3 | Go on macOS 60 s; 10-min soak and Windows pending | Product build needs -rpath /usr/lib/swift in its own build.rs; mono WAV must be mapped to mono before AAC; SCK at 27 fps with moving content |
| S5 | **Decided Oct 20, 2026: GPUI (`gpui-pre`) for apps/desktop, pinned to an exact version**, conditional; iced if the preview path fails | GPUI is the only one with accessibility (NFR-13) and fits the memory budget (74 vs 93 MB idle), but has no wgpu texture embedding: first task in M0 is a 1080p60 preview via `surface()` (macOS) / CPU upload / child surface. Build needs `runtime_shaders` + `font-kit` features without Xcode; pin an exact `gpui-pre` version. Windows pending |
| S6 | **Conditional Go on macOS (decided Oct 20, 2026)**; Windows pending | Screen path: `AVAssetWriter` + `SCStream` GPU surfaces, HEVC or H.264, CMAF segment writer for 2 s fMP4 (not `movieFragmentInterval`); 0.26 to 0.33% late at 3024x1964 and 3840x2160 60 fps without camera. With the camera on (Oct 20, 20 s runs, 3024x1964 @ 60 HEVC): 0.83% screen late, 0.17% camera late, 14% of one core, ~270 MB; the same setup failed once at 7.0% late / 55.8 fps (cause not found: background load or display refresh suspected). Margin to the 1% gate is thin, so 60 fps is offered only after a self-test and 30 fps is the default (REC-3). Camera delivery lags screen by ~75 ms; the 50 ms sync criterion is unproven (needs the flash-vs-camera test). Set SCK interval to 1.1 x fps, B-frames off, retime samples to ticks since a shared origin. Bubble hidden by `sharingType = none` and `SCContentFilter` exclusion (use both). Still open: repeated 5-minute runs with and without camera, 30-minute soak, 60 Hz and true 4K displays, worst-case content, Windows |
