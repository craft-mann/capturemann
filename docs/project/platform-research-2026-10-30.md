# Cross-platform plan input: lessons from MangoDisk and the CaptureMann platform inventory

> Research annex of `specs/plans/release-0.3-0.4-platforms-plan.md`. Written by a research agent from a shallow clone (commit `4285d46`); items marked **(assumption)** were not confirmed. MangoDisk is GPL-3.0: we copy ideas only, never code.

Date: Oct 30, 2026. Research only; nothing in the CaptureMann repo was edited, no cargo build, no hardware test. MangoDisk was cloned shallow to `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/c1ffbd32-6ef9-4f67-930a-2efe499f29b8/scratchpad/mangodisk-src` (commit `4285d46`, Oct 27, 2026) and only read. Paths starting `md:` are relative to that clone.

Wording rule: nothing below is "verified". Facts read from the two repos are cited with a path. Anything about a platform API, a third-party licence, a CI runner or a vendor programme that I did not read in a source in this session is marked **(assumption)** and must be checked before it goes into a spec.

---

## 1. MangoDisk at a glance

| Item | Finding (source) |
| --- | --- |
| What it is | Disk-space analysis and cleanup app, Tauri 2 shell + Vue front end + Rust core (`md:Cargo.toml`, `md:src-tauri/Cargo.toml`, `md:AGENTS.md`) |
| Licence | **GPL-3.0-only** (`md:Cargo.toml`, GitHub API `license: GPL-3.0`). Its code cannot be copied into CaptureMann (Apache-2.0). Copy ideas and shell/YAML patterns that are our own rewrite, not code. |
| Maturity | 5,011 stars, created Aug 1, 2026, last push Oct 27, 2026, v1.2.1, 12 open issues; a release every 4 to 7 days (`gh api`, `gh release list`, Oct 30, 2026) |
| Platforms shipped | macOS (DMG + Homebrew cask), Windows (NSIS `.exe`, portable `.exe`, winget `MangoDisk.MangoDisk`, PowerShell one-liner), Linux x64 and arm64 (`.deb`, `.AppImage`) (`md:README.md` lines 231-300; release asset list from `gh release view`) |
| Platform bugs seen in issue titles (open and closed, `gh issue list --state all`) | Linux AppImage blank window on Arch because bundled `libwayland-client` shadowed the system one (fixed in the build script, see 2.4); a locale missing from the Linux AppImage selector; Windows CPU-temperature driver install failure `0x800B0109`; Windows cleanup hang; macOS FIFO hang. So: real Linux/Windows runtime bugs appeared after release even with this CI. |
| What is public | Only one workflow: `md:.github/workflows/cross-platform-check.yml`. The release, signing, notarization and update-server tooling is **private** (`md:AGENTS.md` "do not commit ... private release tooling"; `md:README.md` line 378: local builds lack signing, notarization and update metadata). So we can learn its CI and bundling checks, not its signing pipeline. |

---

## 2. (a) What MangoDisk does well that we should copy

### 2.1 One platform crate with contracts, three adapters
- `md:src-tauri/crates/mangodisk-platform/src/contracts/*.rs` define OS-neutral traits and result types (volumes, startup items, processes, file read, privacy, system settings...). `linux/`, `macos/`, `windows/` hold one implementation each. `md:.../src/current.rs` picks the adapter with `#[cfg(target_os = ...)] pub use ... as CurrentPlatform` and offers `current_platform()`.
- `md:src-tauri/crates/README.md` states the rule: the Tauri crate "does not own platform policy"; core owns use cases; CLI and GUI are sibling adapters.
- Per-OS dependencies live in `[target.'cfg(...)'.dependencies]` blocks of the platform crate (`md:.../mangodisk-platform/Cargo.toml`), so a Linux build never compiles `objc2` or `windows`.
- **For us:** CaptureMann already has the idea (traits plus scripted fakes, `CLAUDE.md` "Headless crates"), but the OS split is scattered: 55 files use `target_os` (`grep -rl target_os apps crates`), with `cfg` blocks inside `cm-audio/src/system.rs`, `cm-core/src/clock.rs`, `cm-daemon/src/config.rs`, `apps/desktop/src/permissions.rs`, etc. Recommendation: per crate, one `sys/` (or `mac/`, `win/`, `linux/`) module with the same trait the macOS code already implements, and a single `cfg` at the factory function (as `cm-audio::system::system_audio_factory` does). A new crate `cm-platform` for the cross-cutting small things (host clock, sleep inhibitor, config/data/video dirs, login item, open-settings link, memory probe) would replace the scattered `cfg` blocks in `cm-core/clock.rs`, `cm-daemon/recording/sleep.rs`, `cm-daemon/config.rs`, `cm-ai/model.rs`, `apps/desktop/login_item.rs`.

### 2.2 CI: a three-OS (four-runner) PR gate that runs the real build
`md:.github/workflows/cross-platform-check.yml`:
- Matrix `windows-latest, macos-latest, ubuntu-22.04, ubuntu-22.04-arm`, `fail-fast: false`, on every PR (not only main).
- Linux dev packages installed up front in one `apt-get` step; `Swatinem/rust-cache@v2`; pnpm cache through setup-node.
- Same checks on every OS (`pnpm check`, `cargo test --workspace`), then a **no-bundle native build** (`tauri build --no-bundle`) so linking is tested on each OS without making installers.
- Windows-only step compiles and executes the NSIS installer hook in a silent test installer (`md:src-tauri/nsis/test-installer-hooks.ps1`, `md:src-tauri/nsis/README.md`).
- Linux-only step builds the real `.deb` + AppImage on every PR and runs structural checks on them; artifacts are uploaded only on manual dispatch (7 days, no compression).
- Ubuntu 22.04 is pinned on purpose: glibc 2.35 baseline (`md:scripts/linux-bundles.md`).
- **For us:** add `windows-latest` back (the step is already sketched in `.github/workflows/ci.yml`, comment "Add windows-latest back") and add `ubuntu-22.04` (+ `ubuntu-22.04-arm` later) as `check` matrix entries from the first stage of each milestone, as compile-and-fake-test gates, before any backend works. Keep `fail-fast: false`. Note our matrix has `macos-latest` only and `msrv`, `bundle-macos`, `licenses` jobs are macOS or Ubuntu fixed.

### 2.3 Windows installer: a tested upgrade path
- NSIS via Tauri bundler, per-user install (`installMode: currentUser`), 16 installer languages, WebView2 minimum version pinned (`md:src-tauri/tauri.conf.json` bundle.windows).
- Hook script `md:src-tauri/nsis/installer-hooks.nsh` keeps the previous install directory on silent updates, and `test-installer-hooks.ps1` tests it in CI. This came from a real upgrade bug (readme describes two-install state).
- `md:src-tauri/windows/app.manifest` + comment in `md:src-tauri/build.rs`: the app manifest is `asInvoker`, because elevating the whole GUI with a different admin account broke its data directory. Elevation only at the capability boundary.
- **For us:** test the installer's upgrade and uninstall path in CI on a Windows runner (install silently, check files and registry, upgrade, uninstall) - this is cheap and catches the exact class of bug MangoDisk hit. Keep the recorder `asInvoker`; nothing we do needs admin.

### 2.4 Linux packaging checks that encode lessons
- `md:scripts/build-linux-bundles.sh` pins the AppRun download by SHA-256 per architecture, sets `LINUXDEPLOY_EXCLUDED_LIBRARIES=libwayland-*.so*` (the Arch blank-window bug), replaces AppRun with a project script (`md:scripts/appimage-apprun.sh`) to isolate bundled GLib from host GIO modules, builds `deb,appimage`.
- `md:scripts/check-linux-bundle.py` opens the finished `.deb` and AppImage and asserts architecture, glibc symbol ceiling, launcher permissions, no stray Wayland libraries; `md:scripts/test-linux-bundle.py` tests the checker itself.
- `md:scripts/linux-bundles.md` states honestly: "Building and checking ELF dependencies alone does not verify those runtime paths. Validate on Ubuntu 22.04 and a newer Mesa/Wayland system."
- **For us:** copy the idea (a script that inspects the final package: ELF arch, max GLIBC version, no bundled libwayland/libGL/libvulkan, executable bits, desktop file and icon present). We do the same for macOS in `packaging/macos/verify-release.sh`, so a `packaging/linux/verify-release.py` and `packaging/windows/verify-release.ps1` (signature, version resource, installer contents) follow our own convention. Do not bundle GPU userspace libraries; exclude them like MangoDisk excludes Wayland.

### 2.5 Distribution surface
- Same release carries DMG, NSIS, portable exe, deb, AppImage for x64 and arm64, plus CLI-only downloads for macOS and Windows (`gh release view` asset list). Homebrew cask + custom tap, winget, a PowerShell one-liner and a bash one-liner (`md:README.md`).
- **For us:** we already have the Homebrew tap (`craft-mann/homebrew-tap`) and a CLI (`capturemann`, `cm-mcp`). The CLI/MCP binaries are a cheap early Windows/Linux deliverable (the daemon, library, jobs, edit and export logic is headless), see 5.2 "CLI-first".

### 2.6 Process discipline worth copying
- `md:AGENTS.md`: "If a platform is unavailable, document the unvalidated scope instead of claiming support from local results." Same as our wording rule; also "Never hide a compatibility or performance regression with an allow attribute".
- `md:src-tauri/crates/README.md`: benchmarks on Windows describe workload and environment, not just numbers; ordinary-user vs elevated tested separately.
- Small CI check scripts under `md:scripts/` (`check-i18n-usage.mjs`, `check-source-architecture.mjs`, `check-product-identity.mjs`). We already have `cm-i18n check`; a similar "no `target_os` outside the platform modules" lint would stop the scatter described in 2.1 from regrowing (assumption: a simple grep script in CI is enough).

---

## 3. (b) What does not transfer to a screen recorder, and why

| MangoDisk choice | Why it does not transfer |
| --- | --- |
| Tauri + web view (WebView2 on Windows, WebKitGTK 4.1 on Linux) | CaptureMann's GUI is GPUI (`apps/desktop/Cargo.toml`: `gpui-pre =0.3.9`), the overlay is winit + softbuffer. No web view, so no WebView2 runtime, no WebKitGTK, no GTK3 packages to bundle. Their Linux packaging pain (GIO modules, libwayland, WebKit sub-processes) is mostly web-view related. Ours will be GPU/Wayland/X11/IME related instead. |
| Tauri bundler and updater (`createUpdaterArtifacts`, minisign pubkey, endpoint on their site) | Our macOS packaging is custom (`packaging/macos/*.sh`), and updates are manual by maintainer decision (`docs/project/plan.md`, M7 decisions Oct 25, 2026: no network call from the app). Do not adopt an updater in 0.3/0.4 unless the maintainer reverses that decision. |
| The product does file-system work only | They need no capture API, no hardware encoder, no audio devices, no camera, no real-time clock sync. Their hardest platform code is NTFS enumeration and privileged helper services (`md:src-tauri/crates/mangodisk-privileged`, PawnIO driver). Nothing in that applies. We have no privileged helper and should keep it that way. |
| Taskbar/tray panel work (`md:src-tauri/src/resident/taskbar_display/*`, XAML bridge in C++) | Specific to their resident monitor. Our tray is `tray-icon` + `global-hotkey` (`apps/desktop/src/platform.rs`). |
| Permissions: they need file-access/Full Disk Access style grants | We need screen, camera, microphone, input-monitoring prompts that differ per OS and per display server (section 4). |
| CI never touches a GPU, a display, a camera or a mic | Our hardest tests are hardware tests (`#[ignore]`, `tests/manual/*.md`). A hosted-runner matrix gives compile + fakes only. Their "validated on a Windows environment" workflow is a human, not CI. |
| arm64 Windows/Linux builds (`ubuntu-22.04-arm`) | Cheap for them (pure Rust + web view). For us each arch needs its own FFmpeg, sherpa-onnx, llama-server and whisper builds (section 4.9). |
| Release signing | Not public. Their README says official builds are signed; we get no recipe from the repo. |

---

## 4. (c) CaptureMann platform inventory

Legend: **Built mac** = implemented and (as reported) run on the maintainer's Mac; **Stub** = compiles on Windows, returns `not_implemented`; **Hook** = Windows code exists, compile-checked only (`crates/cm-cursor/src/windows.rs` header: "never run on Windows"); **None** = nothing for that OS (falls into a generic `cfg(not(...))` branch).

### 4.1 Capture, encode, audio, camera, cursor

| Area | macOS today (file) | Windows today | Windows target | Linux today | Linux target |
| --- | --- | --- | --- | --- | --- |
| Screenshots, window list, thumbnails | `xcap` + `sck_shot.rs` (ScreenCaptureKit, macOS 14+ excludes own windows) in `cm-capture`; `xcap` is declared for `macos` and `windows` only (`crates/cm-capture/Cargo.toml` line 25) | `xcap` path compiles; never run | keep `xcap` (GDI/DXGI) or Windows.Graphics.Capture still frames; own-window exclusion via `WDA_EXCLUDEFROMCAPTURE` on our windows (already coded in `apps/overlay/src/platform.rs`, compile-only) | None (xcap not enabled) | `xdg-desktop-portal` Screenshot / ScreenCast through PipeWire on Wayland; X11 via `xcap`/XGetImage. Region pick on Wayland has to go through the portal dialog or a layer-shell surface (4.6). |
| Screen recording | ScreenCaptureKit `SCStream` -> IOSurface sample buffers (`crates/cm-capture/src/record/sck.rs`, plain `objc2`) | **Stub** `record/windows.rs` (`WgcSource`) | **Windows.Graphics.Capture** (Win10 1903+; D3D11 textures; borderless on Win11 via `IsBorderRequired`, **assumption** on exact OS floors). Crates: `windows` (MIT/Apache) or `windows-capture` (MIT, **assumption**). Win10 22H2 is the product floor (`docs/project/status.md`). | None | **PipeWire via xdg-desktop-portal ScreenCast** (works on GNOME/KDE/wlroots Wayland and X11 sessions). Crates: `ashpd` (portal client), `pipewire` (PipeWire bindings, MIT). DMA-BUF frames. Needs a user prompt on every session on GNOME unless the portal restore token is used (**assumption**). |
| Hardware H.264/HEVC encode, fMP4 writer | `AVAssetWriter` + VideoToolbox, CMAF segmenter, 2 s fragments (`crates/cm-encode/src/mac.rs`); shared `fmp4.rs`, `writer.rs`, `settings.rs`, `encoder.rs` traits | **Stub** `cm-encode/src/windows.rs` (`HardwareEncoder::create` returns `not_implemented`) | **Media Foundation H.264 MFT** (picks NVENC/Quick Sync/AMF via the OS; hardware MFT with `MF_TRANSFORM_ASYNC`, D3D11 device manager). Feed to the shared fMP4 writer (assumption: `fmp4.rs` can mux raw H.264 samples; check). Patent licensing for H.264 is carried by the OS component, consistent with the "no software H.264 shipped" rule (`CLAUDE.md`). | None | **VA-API** (Intel/AMD, Mesa, MIT) via `libva` through FFmpeg or direct; **NVENC** via NVIDIA driver (`nv-codec-headers` MIT, runtime loaded). Policy risk: Fedora/RHEL Mesa builds disable H.264/HEVC encode in VA-API (**assumption**; "freeworld" packages restore it). With the "no software H.264" rule a Linux machine without a hardware H.264 encoder cannot record. See open decision D-L2 (allow VP9/AV1 software or hardware as a Linux-only fallback). |
| System audio | `SckAudioSource` (`cm-audio/src/sys/mac_sck.rs`), excludes own bundle id | `cm-audio/src/system.rs` already calls `CpalSource::open_default_output_loopback()` on Windows (cpal WASAPI loopback); no own-process exclusion (`let _ = &exclude_bundle_ids`) | WASAPI loopback (exists via cpal). For parity with `exclude_bundle_ids`, Windows 10 2004+ **process-loopback exclude** (`ActivateAudioInterfaceAsync` with `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`, **assumption**) | falls to `not_implemented` | PipeWire monitor source of the default sink (capture the "monitor" node); PulseAudio-compat also works. Own-app exclusion is hard; can be done by capturing a null sink we route to (**assumption**). |
| Microphone | `cpal 0.18` (`cm-audio/src/cpal_source.rs`) + AVFoundation permission probe (`permission.rs`, `sys/mac_permission.rs`) | cpal (WASAPI) code is cross-platform; permission probe is `cfg(not(macos))` placeholder | cpal WASAPI; probe Windows microphone privacy switch (registry `CapabilityAccessManager\ConsentStore\microphone`, **assumption**) | cpal ALSA/PipeWire-ALSA/Jack backends | cpal (ALSA) works; PipeWire-native later. Flatpak: portal. |
| Camera | AVFoundation (`cm-camera/src/mac/*`), IOSurface frames | `unsupported.rs` ("Windows camera backend (Media Foundation) is a later slice") | Media Foundation source reader (crate `windows`, or `nokhwa` MIT, **assumption**); fragment writer in `cm-camera/src/writer.rs` | `unsupported.rs` | V4L2 (`v4l` crate, MIT, **assumption**) or the PipeWire camera portal in Flatpak. |
| Cursor telemetry | `cm-cursor/src/macos/*`, NSCursor image + event tap | **Hook** `cm-cursor/src/windows.rs` (WH_MOUSE_LL + GetCursorInfo), 439 lines, compile-only; needs per-monitor-v2 DPI awareness | finish + run S4 | `cm-cursor/src/platform.rs` fallback | X11: XInput2 raw events. **Wayland: no global pointer position or clicks for ordinary apps.** The ScreenCast portal can deliver cursor position + bitmap as stream metadata (`cursor_mode = Metadata`, **assumption**); global clicks need libei/evdev (needs input-group permission, **assumption**). Expect click effects and "editable cursor" reduced on Wayland. |
| Host clock | `clock_gettime(CLOCK_UPTIME_RAW)` (`cm-core/src/clock.rs`); non-mac branch is a placeholder "monotonic since first call" | placeholder | `QueryPerformanceCounter`; frames from WGC carry `SystemRelativeTime` (QPC-based, **assumption**) | placeholder | `CLOCK_MONOTONIC`; PipeWire buffer timestamps use the same clock domain (**assumption**). The single-clock design (`CLAUDE.md` Recording) holds only if each source's timestamp is converted from its own domain. |
| Idle-sleep inhibitor | IOKit assertion (`cm-daemon/src/recording/sleep.rs`, `cfg(macos)`); non-mac returns a no-op | no-op | `SetThreadExecutionState(ES_CONTINUOUS|ES_SYSTEM_REQUIRED|ES_DISPLAY_REQUIRED)` | no-op | logind `Inhibit` over D-Bus or portal `Inhibit`. |
| Exclude own windows from capture (HUD, bubble, countdown, toasts) | `NSWindow.sharingType = none` (`apps/overlay/src/platform.rs`, `apps/desktop/src/platform_window.rs`), SCK pid exclusion | `apps/overlay/src/platform.rs` has `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`, compile-only; `apps/desktop/src/platform_window.rs` non-mac `apply` returns `Err("... only built for macOS so far")` | `WDA_EXCLUDEFROMCAPTURE` (Win10 2004+, **assumption** on build number) applied through GPUI's raw window handle | none | **No equivalent on Linux** (**assumption**). Mitigations: record a display region that does not include the HUD, or window-capture mode; bubble is composited at export already (`CLAUDE.md`), so only the live HUD/countdown/toasts are affected. Open decision D-L3. |

### 4.2 UI shell, windows, tray, shortcuts, clipboard

| Item | macOS today | Windows | Linux |
| --- | --- | --- | --- |
| UI toolkit | GPUI `gpui-pre =0.3.9`, features `runtime_shaders`, `font-kit` (`apps/desktop/Cargo.toml`); Metal. Preview uses `surface()` with `CVPixelBuffer` (`apps/desktop/src/preview/*`, macOS only) | GPUI uses Win32 + DirectX 11 + DirectWrite (`docs/project/spikes.md` S5); no `surface()`, preview via CPU upload path ("Windows has no `surface()`", S5 follow-up; 1080p ~36% of one core on M4 Pro on macOS, Windows unmeasured). Hello-world GPUI on Windows has **not been run** (spikes.md "Not verified: Windows (both)") | GPUI on Linux uses wgpu (Vulkan) per S5 table; Wayland + X11 backends (**assumption**: both supported by the `gpui-pre-platform` snapshot; check). Client-side decorations on GNOME/Wayland and IME are the two known soft spots (**assumption**). |
| Overlay (region picker) | winit 0.30 + softbuffer, `NSWindow` at screen-saver level (`apps/overlay/src/platform.rs`) | winit borderless + `WDA_EXCLUDEFROMCAPTURE`; S2 DPI cases (150%, mixed DPI) not run | X11: override-redirect/fullscreen works. **Wayland: a normal client cannot position or stack itself above the panel; fullscreen-on-one-output is possible, true overlays need `wlr-layer-shell` (wlroots, KDE) which GNOME/mutter does not offer** (**assumption**). On GNOME rely on the ScreenCast/Screenshot portal's own picker, and crop after capture. |
| Tray | `tray-icon 0.26` + native menu (muda) (`apps/desktop/src/platform.rs`) | works via Shell_NotifyIcon (crate) | `tray-icon` on Linux needs GTK + libayatana-appindicator at build and runtime; GNOME shows no tray without an extension (**assumption**) - need an always-reachable launcher window as the fallback (the LNC-1 launcher exists). |
| Global hotkeys | `global-hotkey 0.9` (`apps/desktop/src/platform.rs`); macOS default chords in `shortcuts.rs` line 18 | RegisterHotKey, supported by crate | X11 supported by crate; **Wayland not supported by the crate** (**assumption**). Wayland route: `org.freedesktop.portal.GlobalShortcuts` (KDE yes, GNOME 48+ yes, wlroots varies; **assumption**) via `ashpd`; the user binds keys in the compositor dialog, so the app cannot force `Alt+Shift+2`. CLI/MCP-driven capture and a tray/launcher button remain the always-works path (a strength of the "one action bus" design). |
| Clipboard | `arboard` non-mac + native on mac (`apps/desktop/src/clipboard.rs`) | `arboard` (note `cargo deny` warning: `clipboard-win` 5.4.1 yanked, `docs/project/status.md` line 73) | `arboard` X11 + Wayland (wl-clipboard feature); images need `wayland-data-control` support (**assumption**). |
| Login item | LaunchAgent plist (`apps/desktop/src/login_item.rs`), non-mac returns `Ok(())` | not implemented | HKCU `Run` key (or `startup` shortcut) | not implemented | `~/.config/autostart/*.desktop`; Flatpak: Background portal. |
| Open OS settings page for a permission | `open x-apple...`; Windows branch `cmd /C start ms-settings:` exists in `apps/desktop/src/permissions.rs`; Linux branch is a no-op | present | none / portal handles consent |
| Permission probes | macOS TCC for screen, mic, camera, input monitoring (`permissions.rs`, `--demo-permissions windows` fake scenario exists) | Windows scenario exists as a fake only; no probe code (**WGC needs no consent**; camera/mic have a privacy switch) | None. Wayland portals ask per session; X11 has no prompts. Probe becomes "is the portal reachable". |
| Secrets | `keyring 4.2` default features: Keychain (macOS), Credential Manager (Windows) (`crates/cm-auth/Cargo.toml` comment) | works with default features | Needs the `sync-secret-service` / `async-secret-service` feature and a running Secret Service (GNOME Keyring / KWallet); headless or minimal desktops have none, so define a fallback (D-L5). **Assumption** on the exact feature names in keyring 4.x. |
| IPC | Unix socket in a 0700 runtime dir (`crates/cm-ipc/src/endpoint.rs`) | Named-pipe client/server branch **already written** with `tokio::net::windows::named_pipe` (`endpoint.rs` line 194); tests are `cfg(unix)` only | Unix socket under `XDG_RUNTIME_DIR` (works as is, **assumption** the path logic does not assume macOS). |
| Paths | `~/Library/Application Support`, `~/Movies` (`cm-daemon/src/config.rs`, `cm-ai/src/model.rs`) | `%APPDATA%` branch present for models; check config and video dirs | XDG branch present; `~/Videos` used when not macOS. |
| Library bundles | `cm-library` uses `cfg(unix)` for file permissions and fsync (`rec.rs` lines 152-175, 1013-1023); `bundle.rs` has `target_os` | non-unix branches exist; NTFS rename-over-open-file and long-path behaviour untested | unix branch applies. |

### 4.3 Rendering and export

| Item | Today | Windows | Linux |
| --- | --- | --- | --- |
| wgpu compositor | `cm-render` wgpu 29 with `metal` feature on macOS, `dx12` on `cfg(windows)` (`crates/cm-render/Cargo.toml` lines 32-40); no Linux target block | DX12 declared; headless adapter on CI runners is **assumption** (WARP software adapter) | add `vulkan` feature; CI headless via lavapipe (Mesa software Vulkan; **assumption**) |
| Preview parity test | `cargo test -p cm-desktop --test parity` needs FFmpeg + GPU, else SKIPPED | would SKIP on a runner without adapter; run on hardware | same |
| FFmpeg sidecar (LGPL) | `make ffmpeg` builds macOS arm64 only (`packaging/ffmpeg/build-macos.sh`), pinned tarballs, SHA-256 | Need an LGPL Windows x64 build: `h264_mf` (Media Foundation) + optionally `h264_nvenc`/`h264_qsv`/`h264_amf` (need `nv-codec-headers`, `libvpl`, `AMF` headers, all MIT, **assumption**: buildable without `--enable-nonfree` and still LGPL), libvpx, libopus. Build with MSYS2/MinGW or MSVC. `quick.rs` already lists encoder candidates `h264_mf`, nvenc, qsv, amf (`docs/project/plan.md` line 78). | LGPL Linux build with `h264_vaapi`, `h264_nvenc`, libvpx, libopus; or rely on the distro FFmpeg for `.deb`/`.rpm` (licence and codec set vary per distro; **assumption**) and a bundled static build for AppImage/Flatpak. |

### 4.4 AI and OCR

| Item | macOS today | Windows target | Linux target |
| --- | --- | --- | --- |
| OCR for Auto-redact (ANN-2.AC4) | Apple Vision `VNRecognizeTextRequest` (`crates/cm-render/src/ocr/apple.rs`); other OS get a stub that answers `not_implemented` and the editor hides the button (`ocr/mod.rs`) | `Windows.Media.Ocr` (OS API, no extra licence; recognizes only languages with an installed OCR language pack, so Vietnamese depends on the user's installed language features, **assumption**) | No OS OCR. Options (all **assumption**, not evaluated): Tesseract (Apache-2.0; needs `libtesseract` + traineddata, system package or bundle) via a binding crate; `ocrs` (pure Rust, MIT/Apache-2.0, models downloaded separately, accuracy for Vietnamese diacritics unknown); an ONNX OCR model through the `onnxruntime` already pulled in by `sherpa-onnx` (would need a second runtime config). |
| PII detection (0.2.5 work in progress, untracked `crates/cm-render/src/ocr/rules/`) | text rules after OCR (`detect.rs`) are platform-neutral if they stay behind `TextRecognizer` | reuse | reuse; only the recognizer is per OS. Keep the 0.2.5 rules free of Apple types so 0.3/0.4 only add recognizers. |
| Whisper (`whisper-rs 0.16`, feature `metal` on mac, `cm-ai/Cargo.toml` line 48) | Metal | CPU build works; GPU (Vulkan/CUDA) needs SDK at build time (**assumption**); decide CPU-only for 0.3 | CPU; Vulkan later |
| Speaker engine (`sherpa-onnx` 2.0.1 vendored, static libs from `make sherpa`, `packaging/sherpa/fetch-macos.sh`, macOS arm64 only) | works (S7 macOS) | needs MSVC static libs of the same release (S7 says Windows pending), watch CRT `/MT` vs `/MD` mismatch (**assumption**) | needs Linux static libs; check the glibc baseline |
| `llama-server` (`packaging/llama/build-macos.sh`, still `pending_pin` placeholders) | Metal build | CPU/Vulkan build script needed | CPU/Vulkan build script needed |
| Memory probe for model choice (`cm-ai/src/text_model/engine/mod.rs` line 771) | `vm_stat` | returns `None` | `/proc/meminfo` present |

### 4.5 Licence risks (shipped binaries must not carry GPL/AGPL; `CLAUDE.md` Constraints)
1. **MangoDisk itself is GPL-3.0**: read, do not copy code. Same for any snippet from its NSIS hooks or scripts; re-derive from public docs.
2. FFmpeg on Windows/Linux: must be built LGPL (no `--enable-gpl`, no `libx264`, no `--enable-nonfree`). `bundle.sh` already refuses GPL builds on macOS; the equivalents for Windows and Linux must be written. NVENC/QSV/AMF wrappers inside FFmpeg are LGPL-compatible only with headers/loaders that are not nonfree (**assumption** - confirm FFmpeg's licence note for each flag before enabling).
3. OpenH264: stays out (decision Oct 24, 2026). Windows uses the OS Media Foundation H.264 encoder; Linux relies on hardware via VA-API/NVENC. A distro that ships an FFmpeg with `libx264` (GPL) must not be redistributed by us; a `.deb` that merely `Depends:` on the distro `ffmpeg` package is a packaging decision for the maintainer (the package is not our binary, but it is the GPL one most distros ship, **assumption**; legal read needed).
4. GTK/WebKit are not involved (we do not use a web view). `tray-icon` on Linux links GTK3 (LGPL, dynamic) and libayatana-appindicator (LGPL/GPL variants, **assumption**): dynamic linking to system libraries is normally acceptable; check each against `deny.toml` and the packaging policy.
5. PawnIO-style signed drivers (MangoDisk) are exactly what we must not do. We install no driver and no service.
6. `whisper-rs` Unlicense exception, sherpa-onnx Apache-2.0, ONNX Runtime MIT: already handled; the Windows/Linux prebuilt archives must be re-checked (`cargo deny`, `THIRD_PARTY_LICENSES.md` regeneration per OS).
7. Tesseract (Apache-2.0) and its traineddata (Apache-2.0) are compatible; `ocrs` models' licences must be checked (**assumption**).
8. Windows redistributables: statically link the C runtime or ship the VC++ redistributable terms (MS licence allows redistribution); not a GPL issue but needs an NOTICE entry.

### 4.6 Wayland and desktop-environment limits (summary for the Linux plan)
- Screen capture only through the portal/PipeWire; the compositor, not the app, shows a selection dialog. No silent capture, no custom full-screen region overlay on GNOME.
- Window capture lists come from the portal picker; our own "window picker" palette (R19/R20, `docs/project/plan.md` line 119) cannot enumerate other apps' windows on Wayland (**assumption**), so it works on X11 only. On Wayland the Window tile defers to the portal picker.
- No always-on-top, no window positioning (HUD, bubble, countdown placement) for ordinary clients on GNOME; wlroots/KDE can do it through layer-shell. Webcam bubble placement is our own feature (live bubble) and would behave differently.
- Global shortcuts: portal only (4.2).
- Tray: extension dependent on GNOME.
- Cursor and click telemetry: see 4.1.
- Flatpak/sandbox: everything goes through portals; this is the cleanest Wayland story and a reason to make Flatpak the primary Linux channel (D-L1).

### 4.7 What is already cross-platform or close (reduces work)
`cm-core` (except the clock placeholder), `cm-library`, `cm-jobs`, `cm-storage`, `cm-publish`, `cm-auth` (loopback OAuth + `open`), `cm-media` (FFmpeg-driven), `cm-i18n`, `cm-daemon` (IPC, action bus, config dirs for XDG and `%APPDATA%`), `cm-cli`, `cm-mcp`, and most of `cm-render` (tiny-skia annotation, wgpu compositor). The macOS-only surface is: capture backends, encoder, system audio, camera, cursor, OCR recognizer, own-window hiding, permission probes, login item, sleep inhibitor, clock, preview `surface()`, packaging scripts, `make ffmpeg|llama|sherpa`. Counted from `grep`: 55 source files mention `target_os`; the two stubs named in the status are `cm-capture/src/record/windows.rs` and `cm-encode/src/windows.rs`, plus `cm-camera/src/unsupported.rs`.

---

## 5. (d) Draft milestone plan

Principles carried over from the repo: platform order is Intel Mac (done, 0.2.1) then Windows then Linux (`specs/04-delivery-plan.md` line 34); boxes are checked on the platform they belong to; "tested with fakes" vs "verified on hardware (as reported)" wording; no software H.264; no telemetry; one action bus, so CLI/MCP get each OS first.

### 5.1 Pre-work that belongs in 0.2.5 or just after (cheap, no OS needed)
1. Add `windows-latest` and `ubuntu-22.04` to the `check` job as build + clippy + fake-test gates; mark failing crates `cfg`-gated, not skipped. Decide which crates compile on Linux today (probably `cm-capture` fails because `xcap`/`sck` are gated; fix with a Linux stub like the Windows one).
2. Introduce the `cm-platform` consolidation (2.1) as a pure refactor on macOS with no behaviour change; add the "no `target_os` outside platform modules" lint.
3. Keep 0.2.5 PII rules recogniser-agnostic (4.4).
4. Write spec stubs: per-feature "Windows/Linux" acceptance criteria columns, so `AREA-n.ACm` IDs stay stable (no renumbering).

### 5.2 Release 0.3.0 - Windows 10 22H2 / 11, x64 first

Scope proposal: screenshots, recording (screen + mic + system audio + webcam + cursor), editor, export, local AI, Drive/YouTube, CLI + MCP. Windows on ARM and 4K60-with-camera targets come after 0.3.x if no hardware is available (D-W2).

**Stage W0 - Foundations and spikes (2-3 weeks of the maintainer's 15-20 h/week, assumption).**
- Get Windows hardware (D-W1). Done when: one Windows 10 22H2 and one Windows 11 machine reachable, ideally one with NVIDIA and one with Intel/AMD iGPU.
- CI: Windows `check` job green (fmt, clippy `-D warnings`, test with fakes, `cargo deny`). `cargo check --target x86_64-pc-windows-msvc` already used by the authors of `cm-cursor/src/windows.rs`.
- Spikes S1-S6 on Windows (the table in `docs/project/spikes.md` has the empty Windows columns): S1 shot, S2 overlay with 100% / 150% / mixed-DPI, S3 recording soak, S4 cursor accuracy, S5 GPUI on Windows (window, tray, hotkeys, IME, accessibility, 1080p60 CPU-upload preview), S6 WGC + MF encode 4K60 + camera. Done when: each has a stored report in `verification/` (template in its README) and a Go / conditional Go / No-go in `spikes.md`. Fallback for S5 if GPUI on Windows is poor: iced (already prototyped).
- Windows host clock (QPC), Windows paths, named-pipe IPC tests (the pipe branch exists, tests are `cfg(unix)`), `keyring` Credential Manager smoke test.

**Stage W1 - Screenshots, overlay, library, tray, hotkeys.** CAP-1..5, ANN-1..3, LIB-1/2 on Windows. `xcap` or WGC stills; overlay on all displays with correct DPI; `WDA_EXCLUDEFROMCAPTURE` on HUD/overlay/bubble via GPUI raw handle (replace the `Err` branch in `apps/desktop/src/platform_window.rs`); clipboard PNG/DIB; login item (Run key); `Shortcuts` defaults with Ctrl/Alt instead of Cmd; Windows permission scenario becomes real (camera/mic privacy links). Done when: `tests/manual/CAP-1.md`, `CAP-1-overlay.md`, `ANN-2.md`, `PRM-1.md` have Windows runs stored in `verification/`; Auto-redact button hidden where no recogniser.

**Stage W2 - Recording engine.** WGC source + MF hardware encoder + shared fMP4 writer; QPC clock; pause/crash recovery (`recording.lock`, fragment size 2 s); disk guard. Done when: `record_real`-style ignored hardware tests pass on both machines; 10-minute soak; 4K30 for 30 minutes on the NVIDIA machine (M3 gate equivalent); hardware-encoder-missing case returns `hardware_encoder_unavailable` (REC-2.AC6) on a VM without a GPU.

**Stage W3 - Audio, camera, cursor.** WASAPI loopback with own-process exclusion, mic, MF camera + live bubble, hook-based cursor; sync budgets (NFR-3, 50 ms camera/screen) measured. Done when: `REC-1.md`, `REC-5.md`, `REC-7.md`, `CAM-1.md`, `CAM-2.md`, `AUD-1.md`, `VOC-1.md` have Windows runs; IME script (`I18N-IME.md`, Telex and VNI) passes with the built-in Windows Vietnamese keyboard and with Unikey (TSF path noted in the plan).

**Stage W4 - Edit, export, preview.** DX12 compositor on real GPUs; preview over the CPU upload path (measure; if > budget, evaluate child surface / D3D11 shared texture); Windows LGPL FFmpeg build (`packaging/ffmpeg/build-windows.*`) with `h264_mf` + vendor encoders + libvpx + libopus, self-check script equal to macOS (licence, components, linked libs). Done when: `parity` test passes on Windows hardware; `EXP-1/2`, `EDT-1..5`, `PLAY-1`, `CAM-3` Windows runs stored; every shipped binary passes the no-GPL check.

**Stage W5 - AI and publishing on Windows.** `Windows.Media.Ocr` recogniser (Vietnamese availability documented), whisper CPU path, sherpa-onnx Windows static libs (S7 on Windows), `llama-server` Windows build, model paths under `%APPDATA%`. `PUB-1/2` with the Windows credential store. Done when: `AI-5/7/8/9.md`, `PUB-1/2.md` have Windows runs; `FIRST-RUN` equivalent for a clean Windows 10 VM.

**Stage W6 - Packaging, signing, release.**
- Installer: pick one primary (D-W3). Candidates: NSIS (what MangoDisk ships; scriptable, hook tests as in 2.3), WiX/MSI (enterprise deployment, winget supports both), MSIX (clean uninstall, Store path, but sandbox/identity constraints with screen capture and sidecar exes, **assumption**). Recommended starting point: per-user NSIS or MSI built in CI plus a portable ZIP, `asInvoker` manifest.
- Code signing (D-W4), options and trade-offs (**all assumptions to confirm**): (1) Azure Artifact Signing (formerly Trusted Signing): low cost, signs in CI, but eligibility is limited by country/organisation type; (2) an OV or EV certificate from a CA, which since 2023 must have its key on hardware (token or cloud HSM), a recurring cost and SmartScreen reputation still builds slowly for OV; (3) SignPath Foundation free signing for open-source projects (requires acceptance and a public build pipeline); (4) Microsoft Store (Store signs MSIX, SmartScreen friendlier, individual developer fee); (5) ship unsigned for 0.3.0-beta with a documented SmartScreen warning and publish SHA-256 files as already done for DMGs. Decision needed before W6 because it changes the installer format and the CI.
- Distribution: GitHub Releases + winget manifest PR to `microsoft/winget-pkgs` (installer URL + SHA-256, **assumption** on moderation time), Scoop bucket optional, the existing Homebrew tap unaffected.
- CI: `release.yml` gets a `windows` job (build, installer, sign, verify, upload, `.sha256`) that mirrors the macOS job; `release.yml` has never run on GitHub (`docs/project/plan.md` M7 row), so test the whole workflow on a fork first (already an open item).
- Installer tests in CI: silent install, files present, uninstall leaves user library alone (2.3).
- Docs: `docs/user-guide` Windows page, SmartScreen note, Windows permissions page parallel to the macOS one.
- Done when: a signed (or explicitly unsigned) installer installs, records 5 minutes with all tracks, exports, uninstalls on a clean Windows 10 22H2 and a Windows 11 VM/machine, with a report in `verification/RELEASE-0.3.0-*.md`; `status.md` Platforms line updated; `THIRD_PARTY_LICENSES.md` regenerated for Windows.

### 5.3 Release 0.3.x - Windows stabilisation
- Cadence idea from MangoDisk: small frequent releases (they ship every 4-7 days); for us a 1-2 week cadence while issues arrive (assumption).
- Work items (ordered by risk): GPU/driver matrix (NVIDIA, Intel Arc/UHD, AMD, hybrid laptops, remote desktop sessions), multi-monitor mixed DPI, HDR displays (HDR capture tone-mapping, **assumption**), DRM/protected content blank frames, high-refresh displays, antivirus false positives on an unsigned/new-reputation exe, long paths and OneDrive-redirected Videos folder, sleep/resume and lid close, UAC/elevated windows not capturable from a non-elevated recorder (**assumption**), Windows on ARM decision, installer upgrade path, Narrator accessibility check (NFR-13), Vietnamese IME edge cases.
- Diagnostics: reuse the 0.2.4 diagnostics export so Windows bug reports contain GPU/driver/OS build; add an issue-template field for OS build and GPU.
- Done when (0.3.1 / "stable" bar, proposal): two consecutive point releases with no data-loss bug (lost or unplayable recording), the 30-minute 4K30 + webcam gate re-run on two GPU vendors with reports stored, signing/SmartScreen state documented, winget package accepted.

### 5.4 Release 0.4.0 - Linux

Proposal: Wayland-first on GNOME and KDE (Ubuntu 22.04/24.04, Fedora current, plus Arch as community), X11 sessions supported where the same code path works. Architectures: x64 first; arm64 only if hardware exists (D-L6).

**Stage L0 - Foundations and spikes.** CI `ubuntu-22.04` green (apt packages for `libasound2-dev`, `libxkbcommon`, `libwayland`, `libx11`/`xcb`, GTK3 + appindicator for tray, `libpipewire-0.3-dev`, `libdbus-1-dev`, `libssl`), cache as in 2.2. Spikes on real Linux sessions: (1) GPUI on Wayland and X11 (window, tray, IME with IBus and fcitx5 for Vietnamese, HiDPI fractional scaling, accessibility), (2) portal ScreenCast via `ashpd` + `pipewire` receiving DMA-BUF frames at 4K60, (3) VA-API and NVENC encode of those frames to fMP4, (4) cursor metadata from the portal, (5) GlobalShortcuts portal on GNOME and KDE, (6) layer-shell on KDE/wlroots for the overlay. Done when: reports in `verification/`, and the decisions D-L1..D-L5 are made from the results.
**Stage L1 - Screenshots and library.** Portal Screenshot + X11 path; region selection UX per DE; clipboard; XDG paths; `keyring` Secret Service with fallback; autostart `.desktop`. Done when: `CAP-1.md`, `ANN-2.md` Linux runs on Ubuntu GNOME (Wayland), KDE Plasma (Wayland), and one X11 session.
**Stage L2 - Recording.** Portal screencast source, VA-API/NVENC encoder, PipeWire clock mapping, system audio monitor, mic, V4L2 camera, cursor from portal metadata (X11 raw input as bonus). Done when: ignored hardware tests pass on Intel, AMD and NVIDIA machines; 30-minute 4K30 gate on one; the "no hardware H.264" case surfaces `hardware_encoder_unavailable` (or the chosen fallback).
**Stage L3 - Edit, export, AI.** Vulkan compositor (lavapipe in CI), LGPL FFmpeg for Linux, OCR choice implemented (4.4), whisper/sherpa/llama Linux builds. Done when: `parity` on Linux hardware; Auto-redact works or the button is hidden with a documented reason.
**Stage L4 - Packaging and release.**
- Formats (D-L1): Flatpak (Flathub; portals are native, sandbox permissions for `--device=dri`, `--socket=pipewire`; Flathub builds from source offline, so use the existing `vendor/` plus `cargo vendor`; bundling LGPL FFmpeg, llama-server and sherpa inside the Flatpak; **assumption** on Flathub review of sidecar binaries), AppImage (as MangoDisk; FUSE2 dependency on Ubuntu 22.04+ is a known user friction, **assumption**), `.deb` (cargo-deb or fpm; `Depends` list), `.rpm` (cargo-generate-rpm), Snap and AUR community-maintained only.
- Build baseline: Ubuntu 22.04 runner for glibc 2.35 as MangoDisk does; verify with a script like `check-linux-bundle.py` (2.4); do not bundle `libwayland-*`, `libGL*`, `libvulkan*`, Mesa.
- Signing: Flatpak/Flathub signs; for `.deb`/`.rpm` publish detached GPG signatures and an apt/dnf repository only if the maintainer wants to run one (D-L7); AppImage `zsync` update info optional. Checksums for all.
- `release.yml` gets `linux` jobs; install smoke test in containers (Ubuntu 22.04, 24.04, Fedora) for `.deb`/`.rpm` with `--version` and CLI export of a fixture (`make_fixture` example) - that is headless and fits CI.
- Done when: install on clean Ubuntu GNOME Wayland, Fedora KDE Wayland and an X11 session; record 5 minutes with all tracks, export, uninstall; report `verification/RELEASE-0.4.0-*.md`.

### 5.5 Release 0.4.x - Linux stabilisation
Distro and compositor matrix (GNOME 44-48, KDE 5/6, Sway/Hyprland best-effort, XFCE/X11), NVIDIA proprietary vs nouveau, portal restore-token behaviour, Flatpak sandbox permissions, fractional scaling, IME (ibus-bamboo, fcitx5-unikey), keyring absence, Wayland shortcut portal coverage, Arch/NixOS reports, arm64 decision. Done when: two point releases without recording-loss bugs and a documented support matrix (supported / best-effort / unsupported per DE) in `docs/`.

### 5.6 Test strategy per OS

| Layer | Runs headless in hosted CI | Needs hardware or a desktop session | Evidence |
| --- | --- | --- | --- |
| Logic crates (`cm-core`, `cm-library`, `cm-jobs`, `cm-storage`, `cm-publish`, `cm-media`, `cm-i18n`, `cm-daemon` with fakes) | Yes on all three OS (matrix, section 2.2); the Windows named-pipe tests must be added | No | CI logs |
| Platform adapters compile | Yes (`cargo check` per target, clippy `-D warnings`) | No | CI |
| FFmpeg export tests | Yes if FFmpeg is installed on the runner (as the macOS job does with brew; choco on Windows, apt on Ubuntu). The shipped LGPL build should also be run through `packaging/ffmpeg/with-bundled-ffmpeg.sh`-style wrapper per OS | No | CI |
| wgpu compositor / parity | Possibly with software adapters (WARP on Windows, lavapipe on Linux; **assumption** both exist on hosted runners) | Real GPU for the parity gate | CI + `verification/` |
| Capture, encode, audio, camera, cursor | No (hosted runners have no camera/mic; screen capture on a runner desktop is **unverified, to try in W0/L0**) | Yes: `#[ignore]` tests run one at a time, with the user's consent (memory note "Recording tests need consent") | `verification/*.md` per template |
| Linux portal flows | Possibly with a headless compositor (mutter `--headless` or sway headless + xdg-desktop-portal + PipeWire in a container; **assumption**, spike it in L0); otherwise manual | Real GNOME and KDE sessions | `verification/` |
| Installers | Install/upgrade/uninstall in a Windows runner; `.deb`/`.rpm`/AppImage inspection and container install in Linux | Clean-machine run before each release | CI + release report |
| IME, accessibility, multi-DPI | No | Manual scripts (`I18N-IME.md` for Windows and Linux variants, new `A11Y-*.md`) | `verification/` |

Hardware access options for the maintainer (who has a Mac only, per repo evidence): a spare Windows laptop with NVIDIA; a cloud Windows VM with a GPU for the NVENC path (hourly cost, remote-desktop capture quirks make it unsuitable for WGC timing tests, **assumption**); Parallels/UTM VMs on the Mac for installer and UI tests only (virtual GPU, no hardware encoder; they do exercise the hardware-encoder-missing path); a Linux PC or VM for Wayland/X11 portal work; a self-hosted GitHub runner on the test machine if the maintainer accepts the security trade-offs for a public repo (do not run PR code from forks on it).

### 5.7 Risks (ranked)
1. **GPUI on Windows/Linux is untested** and has no stability promise (`spikes.md` S5, weekly `gpui-pre` snapshots, exact pin `=0.3.9`). Mitigation: W0/L0 spike first; iced fallback recorded in S5.
2. **No hardware for Windows/Linux** - every acceptance box is manual. Mitigation: D-W1; start with CLI/headless parity.
3. **Linux H.264 encoder availability and the "no software H.264" rule**; Fedora/RHEL VA-API restrictions (assumption).
4. **Wayland capabilities** (4.6): no global shortcuts/overlays/own-window hiding on GNOME; features degrade; product claims in docs must be per-DE.
5. **Preview performance on Windows/Linux without `surface()`**; may force a lower preview resolution or a child surface.
6. **Windows trust**: SmartScreen/antivirus on a new unsigned binary that hooks the mouse (`WH_MOUSE_LL`) and captures the screen looks like spyware to heuristics (assumption). Signing choice (D-W4) is a launch risk, not a polish item.
7. **Sidecar builds multiply** (FFmpeg, sherpa, llama, whisper) x 3 OS x archs, each with licence checks; `release.yml` has never run on GitHub even for macOS.
8. **Clock sync** across source domains (QPC, PipeWire, WASAPI) vs the NFR-3 50 ms budget.
9. **Vietnamese IME** on GPUI for Windows (TSF) and Linux (IBus/fcitx) - the UI has Vietnamese as its second language.
10. **Maintainer capacity** (15-20 h/week): two OS releases back to back; stabilisation releases must be time-boxed.

### 5.8 Open decisions for the maintainer
- D-W1 Hardware: which Windows machine(s) and GPUs; cloud GPU VM for NVENC? self-hosted runner?
- D-W2 Windows scope for 0.3.0: x64 only? ARM64 later? Minimum build (Win10 22H2 as in status).
- D-W3 Installer format: NSIS vs MSI (WiX) vs MSIX; per-user vs per-machine; portable zip.
- D-W4 Code signing route (list in W6) and who owns the account/identity.
- D-W5 Does 0.3.0 wait for Windows OCR and speakers/llama, or ship them in 0.3.x? (Keeping 0.3.0 to capture/record/edit/export is the lowest-risk cut.)
- D-W6 CLI/MCP first: ship `capturemann` + `cm-mcp` for Windows/Linux early (headless fixtures, export, transcript) before the GUI recorder works? (Proposal, pairs with MangoDisk's separate CLI downloads.)
- D-L1 Primary Linux channel: Flatpak first, AppImage + deb as secondary? rpm? who maintains AUR/Nix?
- D-L2 Linux without a hardware H.264 encoder: refuse (consistent with today), or allow a royalty-free codec path (VP9/AV1 via libvpx/SVT-AV1/AOM in an LGPL FFmpeg, into WebM/fMP4) as a Linux-only fallback. Needs a legal/patent read and a change to the "no software encoder" constraint in `CLAUDE.md`.
- D-L3 Own-window hiding on Linux: accept the HUD being in capture (mitigate with region/window mode), or ship the HUD only inside the tray/launcher on Linux.
- D-L4 Linux feature floor: Wayland-first vs X11-first; what is "supported" (GNOME + KDE on Wayland) vs "best effort" (wlroots, XFCE).
- D-L5 Secrets without a Secret Service: encrypted file with a user passphrase, or ask the user to run a keyring.
- D-L6 Linux arm64 and Windows ARM64: yes/no and when.
- D-L7 Run an apt/dnf repository or only GitHub Releases + Flathub?
- D-A Auto-update: still manual (current decision) or add an opt-in "Check for updates" (MangoDisk uses a signed-manifest updater on its own server; Windows silent installers make this attractive but it adds network traffic against the "no network traffic when off" constraint).
- D-B Version numbers: 0.3.0 may be labelled beta for Windows only; same for 0.4.0 Linux (the repo wording is per-platform "verified on ..." in `status.md`).

---

## 6. Files read (for follow-up)
CaptureMann: `CLAUDE.md`; `docs/project/plan.md`, `status.md`, `spikes.md` (S5 to S7 tables and summary); `specs/03-integrations.md` (OAuth/storage, no platform APIs there); `specs/04-delivery-plan.md` (lines 34-54, 87, 127-130); `.github/workflows/ci.yml`, `release.yml` (job structure only); `crates/*/Cargo.toml` target blocks; `crates/cm-encode/src/windows.rs`, `cm-capture/src/record/windows.rs`, `cm-cursor/src/windows.rs` (header), `cm-camera/src/unsupported.rs`, `cm-audio/src/system.rs`, `cm-core/src/clock.rs`, `cm-daemon/src/recording/sleep.rs`, `cm-ipc/src/endpoint.rs`, `cm-render/src/ocr/mod.rs`, `apps/overlay/src/platform.rs`, `apps/desktop/src/{platform,permissions,login_item,platform_window}.rs`. Not read in detail: `packaging/macos/*.sh`, `specs/features/*`, the deferred sections of `plan.md` after line 120.
MangoDisk: `Cargo.toml`, `src-tauri/Cargo.toml`, `tauri*.conf.json`, `.github/workflows/cross-platform-check.yml`, `src-tauri/crates/README.md`, `mangodisk-platform/Cargo.toml` and `current.rs`, `scripts/build-linux-bundles.sh`, `scripts/linux-bundles.md`, `src-tauri/nsis/README.md`, `src-tauri/build.rs`, `AGENTS.md`, `README.md`, GitHub metadata via `gh`. Not read: the Windows privileged service, the Vue front end, `mangodisk-core`.
