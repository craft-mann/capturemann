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

## S7 `spike-diarize`

**Status (Oct 27, 2026): technical Go on macOS arm64; accuracy still open.** The engine and models shipped in 0.2.0 (inside the app) and the maintainer accepted the speaker split on one 11-minute English meeting as working; accuracy (DER, word attribution, 4-person and 30-minute runs, a 16 GB Mac) is to be tested with users and the team over the coming weeks, so no accuracy figure is claimed. The text below is the Oct 27 spike log.

**Pass (plan S7):** AI-8.AC1 and AC2 targets (at least 90% of words correct on a 10-minute two-person interview; DER at most 20% and at most 3 minutes for 30 minutes of audio on a 16 GB M-series Mac), at most 600 MB peak memory, a licence that allows mirroring.

**What was run (Oct 27, 2026, macOS, Apple Silicon only; Windows pending):** the official `sherpa-onnx` crate 2.0.1 (offline speaker diarization API: pyannote segmentation-3.0 -> CAM++ embeddings -> fast clustering), wrapped in `spikes/spike-diarize` (`diarize`, `timeline`, `split` commands). Candidate B (pyannote community-1) and the Nemotron challenger were **not** run (not in the permitted downloads). Input: the maintainer's sample meeting `Meeting with Michael.m4a`, a copy converted to 16 kHz mono WAV with the bundled FFmpeg (not stored in the repo). The audio is **675.7 s (11 min 16 s)**, English, speech until 667.4 s. Run on the M4 Pro (48 GB RAM, so the 16 GB case is not tested), release build, `--threads 4` unless noted, models loaded in 0.13 to 0.38 s.

| Check | Windows | macOS |
| --- | --- | --- |
| Builds from Rust, runs offline | | Yes. Models read from local files, no network at run time. Build needs the prebuilt static library (see below) |
| Default settings (threshold 0.5, no `num_clusters`) | | **29 speakers**, 190 turns, 35.8 s. 3 clusters hold 17 to 22% of the audio each, the remaining 26 are fragments (0.3 to 34 s). Unusable without a hint |
| Threshold sweep (no `num_clusters`) | | 0.7: 16 speakers; 0.8: 12; 0.9: 10; 1.0: 6 (two big ones with 55% and 27% of the audio plus four small ones of 1 to 17 s). The threshold is not a quick fix; only `num_clusters` gave the expected count |
| `num_clusters` 2 | | 2 speakers, 101 turns: speaker 0 401 s (60% of the audio, 77 turns), speaker 1 180 s (27%, 24 turns). 35.1 s, RTF 0.052 |
| `num_clusters` 3 | | 3 speakers, 114 turns: 378 s (57%), 181 s (27%), and a third with only 24.8 s (4%) in 12 turns (0.3 to 7 s; the two longest, 6.1 s at 128 s and 7.1 s at 155 s, sit between turns of speaker 0). 40.7 s, RTF 0.060. The third speaker may be a real third voice or a split of the main speaker: needs listening |
| Wall time / real-time factor | | 4 threads: 35.1 to 40.7 s for 675.7 s = RTF 0.052 to 0.060 (about 94 s for 30 minutes if it scales linearly; **not measured**, and clustering time grows faster than linearly with the number of segments). 1 thread (`num_clusters` 2): 112.7 s = RTF 0.167 (about 5 minutes for 30 minutes, over the AC2 limit if the 16 GB Mac had to run on one thread). One run of the threshold 0.7 case, done while two other runs shared the CPU, took 99.5 s: it shows the sensitivity to load |
| Peak memory (`/usr/bin/time -l`) | | Peak footprint 382 MB (default), 331 MB (2 clusters), 323 MB (3 clusters), 286 MB (1 thread); maximum resident set 432, 362, 338 and 319 MB. Under the 600 MB limit for 11 minutes of audio; **how it grows with a 30 minute or longer recording is unmeasured** |
| Binary (release) | | `spike-diarize` 24.9 MB with the spikes' `debug = 1`, **17.2 MB stripped**. It links only system libraries (`libc++`, `libSystem`, `Foundation`, `CoreFoundation`); the static ONNX Runtime is inside it |
| Cold build | | 20.4 s (fresh target directory, crates already in the cargo cache; includes downloading the 20.8 MB archive and compiling 143 crates, most of them for the build script only). Incremental rebuild of the spike: 0.7 s |
| Result | | Pipeline works and is fast and small enough. Accuracy is unmeasured |

**How the output looks (read from the turn list and a text timeline, not heard):**

- Boundaries are plausible against the audio: the first speech starts at 2.6 s, the last turn ends at 667.4 s of 675.7 s, long monologues are kept whole (for example 46 s from 168 s and 45 s from 399 s as one turn each) and the end of one speaker's turn usually lines up with the next speaker's start (for example 63.02 s and 77.98 s, 127.98 s, 134.03 s). Speaker 0 and 1 alternate through the first ~9.5 minutes; the last ~1.5 minutes are mostly one speaker.
- Short segments: with `num_clusters` 2 or 3, 27 of 101 and 30 of 114 turns are shorter than 1 s (back-channels such as "yeah" are plausible, but some are single-frame flickers of 0.30 to 0.47 s inside one speaker's talk, for example `2.63 to 3.10`, `4.01 to 4.35`). `min_duration_on` 0.3 s and `min_duration_off` 0.5 s are the library defaults; the spike takes them as options. Speaker changes with a gap under 0.5 s: 24 (2 clusters) and 34 (3 clusters), mostly real hand-offs. Rapid A-B-A flips over a few seconds do occur around 63 to 78 s and 304 to 340 s; whether they are real interjections needs listening.
- Overlap: the pyannote model is a powerset model, so two speakers can be reported at once; the output contains overlapping turns of different speakers, 17.5 s in total with 2 clusters and 19.7 s with 3 (the default run: 28.0 s). Overlapping turns are not merged by the library, so the AI-8 rule "louder speaker wins, line marked overlap" has to be applied by `cm-ai` on the words.
- Speaker numbering is the library's cluster order, not first speech. `cm-ai` must renumber ("Speaker 1..N by first speech").
- **No DER, no word attribution:** there is no reference. Do not read "plausible" as accurate.

**Listening check for the maintainer (needs your ear):** `spikes/spike-diarize/scratch/listen.sh "<audio>" [num_clusters|auto] [out_dir]` converts to 16 kHz mono, diarizes, prints the timeline (`timeline.txt`: one row per speaker, `#` = talking, plus the turn list) and writes `speaker_N.wav`, each as long as the recording with the other speakers muted, so a time in the timeline is the same time in every file. Build first with `cargo build --release -p spike-diarize --manifest-path spikes/Cargo.toml`. Outputs of the runs above (not in the repo): `/private/tmp/claude-502/s7-models/listen-k2/` and `/private/tmp/claude-502/s7-models/listen-k3/` (turn JSONs of every run are in `/private/tmp/claude-502/s7-models/out/`). Things to check: (1) are speaker 0 and 1 the two main voices, all through; (2) is the 3-cluster third speaker a real person (12 turns, 24.8 s, at 119, 128, 147, 155, 324, 328, 341, 385, 545, 564, 568 and 661 s); (3) do the sub-second flickers and the A-B-A flips at 63 to 78 s and 304 to 340 s sound like real interjections; (4) are the starts and ends of the long turns right.

**Licence and `cargo deny` (main `deny.toml`, not edited):** `cargo deny --config deny.toml check licenses sources bans` on the spike (offline, advisories not run) gives `sources ok`, `bans ok` (duplicate-version warnings only), `licenses FAILED` with **two** licences that are not on the allow list, both only in the crate's **build-script** dependencies (they are not linked into the binary): `bzip2-1.0.6` (`libbz2-rs-sys` via `zip` via `sherpa-onnx-sys`) and `CDLA-Permissive-2.0` (`webpki-roots`, 2 versions, via `ureq`). Both are permissive (bzip2 licence is BSD-like; CDLA-Permissive-2.0 is for the data in the root certificate list). Options for the maintainer: add both to `allow` with a comment "build-time only, via sherpa-onnx-sys", or vendor/patch a `sherpa-onnx-sys` whose build script does not download (removes `ureq`, `zip`, `bzip2`). Everything linked into the binary is MIT/Apache-2.0/Zlib/Unicode (`sherpa-onnx` and `sherpa-onnx-sys` Apache-2.0; the others are `serde`, `serde_json`, `anyhow`, `indexmap`, `hashbrown`, `foldhash` Zlib, `memchr` Unlicense-or-MIT taken as MIT, `zmij` MIT); `r-efi` (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`) is Windows-only build-script graph and is satisfied by MIT. No GPL/AGPL crate. The static libraries inside the C archive are not covered by `cargo deny`: **the release archive contains no licence or notice files** (only `lib/*.a`), so `THIRD_PARTY_LICENSES.md` must get the sherpa-onnx (Apache-2.0) and ONNX Runtime (MIT, with its third-party notices) texts from their repositories; ONNX Runtime's bundled third-party licences were not read in this spike.

**Static linking, build script and reproducibility:** the crate's default feature `static` links `libonnxruntime.a` (75.1 MB of the 206 MB of static libraries in the archive) and the sherpa-onnx libraries into the executable; the linker drops the rest (final stripped binary 17.2 MB). No `.dylib` or `.framework` is shipped or needed, so there is nothing extra to sign or notarize; the usual hardened-runtime signing of the executable is enough (not tested: no signing was done here). The build script (`sherpa-onnx-sys` 2.0.1) at build time: (1) with nothing set it **downloads** `https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-osx-arm64-static-lib.tar.bz2` into `<target>/sherpa-onnx-prebuilt/` with no checksum check; (2) `SHERPA_ONNX_ARCHIVE_DIR=<dir>` uses `<dir>/<archive name>` and does not touch the network (tested: builds); (3) `SHERPA_ONNX_LIB_DIR=<dir>` links the `lib/` folder you give it and skips both download and extraction (tested: builds, no download). For CI and `make bundle` use (2) or (3) with the archive pinned by SHA-256 (below), checked by our script before the build (the crate does not verify it); a Mac without the archive and without network fails the build. Intel Macs, Windows and Linux need their own archive names (`osx-x86_64`, `win-x64`, ...) and were not checked.

**Pins for the maintainer to confirm (downloaded Oct 27, 2026; hashes computed here):**

| File | URL | Size (bytes) | SHA-256 |
| --- | --- | --- | --- |
| Static library archive | `https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-osx-arm64-static-lib.tar.bz2` | 20,829,454 | `d7a9b14f337958e7373c8c650f046df33d210bbaff2f33cb8322dda663ee5e1c` |
| Segmentation archive | `https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2` | 6,958,444 | `24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488` |
| `model.onnx` inside it (used) | (from the archive) | 5,992,913 | `220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079` |
| `model.int8.onnx` inside it (not run) | (from the archive) | 1,540,506 | `d582f4b4c6b48205de7e0643c57df0df5615a3c176189be3fc461e9d18827b5d` |
| Embedding model | `https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx` | 29,596,978 | `357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b` |
| Segmentation `LICENSE` (MIT, "Copyright (c) 2022 CNRS") | (in the archive) | 1,061 | `14d7016ad68e7394d6e6b78d96cc2ae431c905287b89674cfdf021e79e62b8ba` |

The segmentation archive's `README.md` only says the models are converted from `huggingface.co/pyannote/segmentation-3.0`; the embedding file comes without a licence file (its Apache-2.0 licence comes from the ModelScope record and the 3D-Speaker repository, read in the research note, not from the download). Crates: `sherpa-onnx` and `sherpa-onnx-sys` `=2.0.1` (`Cargo.lock` of the spikes workspace). The pins above are the facts of this download; they are not yet in any product code.

**Not verified, and how to finish:**

1. **Accuracy.** No ground truth: no DER, no word attribution, AC1 and AC2 targets untested. Listen with `listen.sh` (above); for numbers, hand-label the turns of a 10-minute two-person recording and the 30-minute four-person fixture and score them (the plan's S7 sets).
2. **One recording only,** 11 minutes, English, 2 to 3 speakers, unknown microphone and room. Four-plus speakers, Vietnamese, headset/system-audio mixes and noisy rooms are untested.
3. **16 GB machine and 30 minutes+.** Peak memory and time were measured on a 48 GB M4 Pro with 11 minutes of audio only; run a 30-minute file on the 16 GB Mac before claiming AC2 or the 600 MB limit. Also check 1 hour for memory growth.
4. **Candidates B (pyannote community-1 with VBx) and the Nemotron diarization model** were not run.
5. **Clustering threshold calibration:** the library's `threshold` scale is unclear and 1.0 still over-split here; either calibrate on several recordings or rely on the user's `num_speakers` hint and a "merge speakers" fix-up (AI-8 already has merge) and treat the automatic count as a suggestion.
6. **Windows and Intel Mac** builds, and signing/notarization of an executable that contains the library (static, so expected to be unremarkable).

**Done afterwards (Oct 27, 2026): the build-script route.** Instead of allowing the two licences, `sherpa-onnx` and `sherpa-onnx-sys` 2.0.1 are vendored in `vendor/` with a `build.rs` that only links the libraries in `SHERPA_ONNX_LIB_DIR` (no download, no `ureq`, `zip`, `bzip2`, `tar`), and the archive is fetched by `packaging/sherpa/fetch-macos.sh` (`make sherpa`) with the SHA-256 above checked first. `cargo deny --offline check licenses bans sources` passes without any new exception (advisories were not run: no network); `cargo tree` shows none of bzip2, libbz2-rs-sys, zip or webpki-roots. The engine and the model rows are in `cm-ai` (`speaker_engine`, `data/speaker_models.json`); see AI-8 "Real engine" in `specs/features/07-ai.md`. First product run on the same 675.7 s sample (release, M4 Pro, 4 threads): `num_speakers` 2: 2 speakers, 101 turns, 34.7 s; 3: 3 speakers, 114 turns, 34.5 s (third speaker 24.8 s in 12 turns, as in the spike); automatic (threshold 1.2, tiny clusters folded): 2 speakers, 101 turns, 35.0 s; peak footprint 283 to 293 MB. Same caveats: no ground truth.

**Recommendation:** keep sherpa-onnx (static) + pyannote segmentation-3.0 + CAM++ as the AI-8 runtime and model set: it is the smallest, fastest route, runs offline from Rust, is permissive-licensed, and needs no dylibs. Before building the model part: (a) decide the two `cargo deny` licence allowances (or a patched `-sys` crate); (b) pin the archive and models by SHA-256 as above and verify the archive before building; (c) design AI-8 around a `num_speakers` hint (the default threshold over-splits, so without a hint the automatic count is not usable: offer "How many people?" and a merge step, and consider capping or merging tiny clusters); (d) renumber speakers by first speech and resolve overlapping turns in `cm-ai`; (e) run the listening check and the 30-minute 16 GB run, and only then mark S7 passed or switch to candidate B.

## S8 text model for reports and translation (resolved Oct 27, 2026)

**Result:** resolved by the Cowork research (`docs/research/local-model-and-licence-check-2026-10-27.md`, `docs/research/text-model-pins-2026-10-27.md`) and the tests of Oct 27, 2026: **Gemma 4 E2B** is the default (best score in the small-model test, English and Vietnamese); Gemma 4 26B-A4B (Macs with 32 GB or more), Qwen3.5 4B (best for translation into other languages) and Qwen3.5 2B are the other choices, all Apache-2.0, run by a bundled `llama-server`. Tested through the CLI on one 11-minute meeting: report with Gemma 24 s, translation to Vietnamese 84 s with 0 lines left untranslated; a 45-minute transcript (the meeting chained four times) went through the report in 3 chunks in 164 s with 0 items dropped. **Still to keep checking:** Vietnamese quality and long-recording quality on other recordings (the 45-minute run repeats content, so it checks chunking and time, not new topics), the review set of `tests/manual/AI-9.md`, and the `ram_mb` figures on a Mac with little memory.

## Decision

| Spike | Go / fallback | Notes |
| --- | --- | --- |
| S1 | Go on macOS; Windows pending | Passed on macOS (reported Oct 19); spike code in `spikes/spike-shot` |
| S2 | Go on macOS; Windows and 1x display pending | Fullscreen keeps windows below the notch (3024x1898): use a borderless window at screen-saver level with the full screen frame |
| S3 | Go on macOS 60 s; 10-min soak and Windows pending | Product build needs -rpath /usr/lib/swift in its own build.rs; mono WAV must be mapped to mono before AAC; SCK at 27 fps with moving content |
| S5 | **Decided Oct 20, 2026: GPUI (`gpui-pre`) for apps/desktop, pinned to an exact version**, conditional; iced if the preview path fails | GPUI is the only one with accessibility (NFR-13) and fits the memory budget (74 vs 93 MB idle), but has no wgpu texture embedding: first task in M0 is a 1080p60 preview via `surface()` (macOS) / CPU upload / child surface. Build needs `runtime_shaders` + `font-kit` features without Xcode; pin an exact `gpui-pre` version. Windows pending |
| S6 | **Conditional Go on macOS (decided Oct 20, 2026)**; Windows pending | Screen path: `AVAssetWriter` + `SCStream` GPU surfaces, HEVC or H.264, CMAF segment writer for 2 s fMP4 (not `movieFragmentInterval`); 0.26 to 0.33% late at 3024x1964 and 3840x2160 60 fps without camera. With the camera on (Oct 20, 20 s runs, 3024x1964 @ 60 HEVC): 0.83% screen late, 0.17% camera late, 14% of one core, ~270 MB; the same setup failed once at 7.0% late / 55.8 fps (cause not found: background load or display refresh suspected). Margin to the 1% gate is thin, so 60 fps is offered only after a self-test and 30 fps is the default (REC-3). Camera delivery lags screen by ~75 ms; the 50 ms sync criterion is unproven (needs the flash-vs-camera test). Set SCK interval to 1.1 x fps, B-frames off, retime samples to ticks since a shared origin. Bubble hidden by `sharingType = none` and `SCContentFilter` exclusion (use both). Still open: repeated 5-minute runs with and without camera, 30-minute soak, 60 Hz and true 4K displays, worst-case content, Windows |
| S7 | **Runtime and models: technical Go on macOS arm64 (Oct 27, 2026); quality NOT decided**; Windows, accuracy and the 16 GB machine pending | `sherpa-onnx` 2.0.1 (static) + pyannote segmentation-3.0 + CAM++ EN VoxCeleb runs offline from Rust: 11.3 min of speech in 36 s (RTF 0.052, 4 threads), peak 323 to 431 MB, 17.2 MB stripped binary, no extra dylibs. No DER or word attribution was measured (no ground truth, one recording). The default clustering threshold 0.5 gave 29 speakers on a 2 to 3 person recording, so the product must pass `num_clusters` (from the "How many people?" hint) or a calibrated threshold (about 1.0 gave 6, still too many). Needs the maintainer's listening check, a 30-minute and a 4-person recording and a 16 GB Mac before AI-8.AC1 to AC3 are called passed. `cargo deny` needs two new licences allowed (build-only crates) or a different build route |
| S8 | **Resolved (Oct 27, 2026): Gemma 4 E2B** | See the S8 section above; Vietnamese and long-recording quality to keep checking |
