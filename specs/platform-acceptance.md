# Windows and Linux acceptance rows

One row per requirement in `features/`. The acceptance criteria (AC) and IDs in the feature files are
unchanged and apply to macOS 13+ and Windows 10 22H2+ unless a criterion names one system (`README.md`,
"Platforms"). This file says, per requirement, **what changes on Windows and on Linux**, **which stage of
`plans/release-0.3-0.4-platforms-plan.md` builds it**, and **how it is checked**. "Shared" means the code has no
OS-specific part, so its Windows and Linux check is the CI run (fakes) plus the manual script named in the
feature file. A row is a plan, not a status: the status line of each feature stays the source of truth, and
nothing here counts as verified without a report in `verification/`.

Stages: W0–W6 are 0.3.0 (Windows), L0–L4 are 0.4.0 (Linux). Linux is still open (the maintainer has not
decided D-L1 to D-L5), so Linux cells name the intended mechanism, not a commitment. Wayland limits are in the
plan (no custom full-screen overlay on GNOME; window lists and shortcuts only through portals; the HUD cannot hide
itself from capture; the tray needs an extension on GNOME).

## Screenshot, annotation, library, launcher

| ID | Windows | Linux |
| --- | --- | --- |
| CAP-1 | W1. Display, window and region capture through `xcap`; the picker overlay is a borderless topmost window per display; physical-pixel output checked at 100 %, 150 % and mixed DPI. Own windows are hidden with `WDA_EXCLUDEFROMCAPTURE`. Checked by `tests/manual/CAP-1.md` and `CAP-1-overlay.md` on the primary machine. | L1. **X11 path built** (`xcap`; display, region and titled-window capture checked on Xvfb in a VM, CI runs them; a Wayland session takes display and region screenshots through the portal Screenshot interface, see `verification/L1-wayland-screenshot-2026-10-31-claude.md`). **Wayland targets (decided Oct 31, 2026):** windows have no ids and cannot be listed, so `list_targets` reports `capabilities` (`list_windows`, `capture_window`, `capture_region`, `pick`); a window id answers `not_implemented` naming the picker; `capture_screenshot { target: { pick: true } }` (CLI `capture screenshot --pick`) calls the portal with `interactive: true`, so the compositor's own window / area / screen picker chooses; it needs a person at the screen (Esc is `user_declined`, 2 minutes without an answer is `confirmation_timeout`, a desktop without a picker is `not_implemented`) and keeps the tier of `capture_screenshot`. Displays are the monitors Mutter reports over D-Bus (`org.gnome.Mutter.DisplayConfig`, no permission) cropped from the one desktop picture, with each monitor's own scale; other compositors report one "Desktop" display (KDE and wlroots monitor layout not done). Region = crop of the picture (CLI, MCP) or the `cm-overlay` freeze-then-select window; window mode of `cm-overlay` on Wayland uses the portal picker. Report: `verification/L1-capture-targets-linux-2026-10-31-claude.md`. Run on GNOME Wayland, KDE Wayland and one X11 session. **L1 windows (Oct 31, 2026, QEMU VM, `verification/L1-windows-linux-2026-10-31-claude.md`):** Linux has no per-window exclusion from capture, so the app takes its own toast stack off the screen for a capture and restores it ~1.5 s later (`capture_cloak`, `studio_window::kept_windows` is the hook the engine calls before every display capture and target listing); checked on X11 (the toast is not in the image); on GNOME Wayland the hide runs but the pixel check was inconclusive in the shared VM session. The library window stays in the capture on purpose. |
| CAP-2 | W1. `global-hotkey` (RegisterHotKey) with the Ctrl+Alt+Shift defaults in the feature file; `tray-icon`; Launch at login is a per-user Run entry written only by an installed copy (the macOS rule: a dev binary never registers). | L1. GlobalShortcuts portal (**not offered by GNOME 46's portal 1.18**, so global hotkeys there need another route; re-check on newer GNOME and KDE); tray through StatusNotifier (needs an extension on GNOME, the launcher window is the fallback); autostart `.desktop` file (written, tested with fakes). **L1 (Oct 31, 2026, QEMU VM, report `verification/L1-desktop-shell-linux-2026-10-31-claude.md`):** `cm-desktop` starts and shows its window on X11 and on GNOME Wayland; the tray icon is a StatusNotifierItem served by the app itself over D-Bus with `zbus` (no GTK): exported and registered, menu and click routing tested against a stand-in watcher in the VM (report `verification/L1-tray-linux-2026-10-31-claude.md`), not yet seen in a real panel (KDE, GNOME with the AppIndicator extension); stock GNOME has no watcher, so the app shows a toast and the launcher window is the fallback; the shell's screenshot works on X11 through the overlay and the CLI, and the CLI path works on Wayland through the portal. **Hotkeys (Oct 31, 2026, same VM, report `verification/L1-clipboard-hotkeys-linux-2026-10-31-claude.md`):** X11 uses `global-hotkey` (XGrabKey) and works (Ctrl+Alt+Shift+Space opened the launcher). In a Wayland session the app asks the GlobalShortcuts portal (`ashpd`), tested only against a fake portal on a private bus; on GNOME 46 the interface is missing, nothing is registered, and Settings -> Shortcuts plus a start-up toast say so and give the fallback: a desktop custom shortcut that runs `cm-desktop --launcher` or `cm-desktop --capture region|window|display` (a second launch hands the request to the running app over `instance.sock`). Not tried: KDE, newer GNOME with the portal, a real keyboard. |
| CAP-3 | W1. Shared drawing code; checked with the picker at mixed DPI. | L1. Where a custom overlay exists: X11, and Wayland for the region mode (`cm-overlay` opens a fullscreen window per monitor over the frozen portal picture; no highlight of windows, which Wayland cannot list; window mode uses the compositor's picker). |
| CAP-4 | W1. Clipboard gets the image as PNG and DIB; default destination is shared. | L1. **Built (Oct 31, 2026; report `verification/L1-clipboard-hotkeys-linux-2026-10-31-claude.md`):** `arboard` with the X11 selection (the app keeps one clipboard handle for its whole life, because the selection lives in the owning process and was lost when the handle was dropped) and the `wayland-data-control` protocol (wlroots, KDE). GNOME's Mutter has no data-control protocol: there the X11 selection through XWayland is used (Mutter hands it to Wayland apps; not tried in the VM, whose shell runs without XWayland). When no clipboard can be reached, `wl-copy` or `xclip` is run if installed, else the copy fails with a message naming what to install; `--copy`, the editor's Copy and the post-capture toast then say "not copied" instead of claiming success. Only the running app can copy (a one-shot process would lose an X11 selection on exit); the headless `cm-daemon` has no clipboard and says so. Checked on X11 (Xvfb) with `xclip`; the Wayland positive path is untried. |
| Window frame (all app windows) | macOS and Windows draw the frame; nothing changes. | L1. **Built Oct 31, 2026** (`ui::csd`, `platform_window::normal_window_options`): where the compositor draws no title bar (GNOME Wayland) the Studio window gets an app-drawn 36 px bar with minimise, maximise/restore and close, drag to move, double-click to maximise, right-click window menu, and 6 px resize strips on the edges and corners; KDE and X11 window managers keep their own frame (GPUI `window_decorations()` decides at render time). Window id and class are `dev.capturemann.app` (the `.desktop` file name) and X11 gets the app icon. Pop-ups: X11 `WindowKind::PopUp` is an override-redirect notification window (above, no taskbar, no focus); Wayland has no always-on-top or no-focus request, so toasts are cards inside the main window while it exists. Checked in a VM on X11 (openbox) and headless GNOME 46 Wayland, see `verification/L1-windows-linux-2026-10-31-claude.md`; KDE, fractional scaling, HiDPI, a real GPU and rounded corners or a shadow are not done. |
| CAP-5 | Shared. | Shared. |
| CAP-6 | Not scheduled (P2); decide per OS when CAP-6 starts. | Same. |
| ANN-1, ANN-3, ANN-4 | Shared (the annotation model and renderer have no OS part). W1 run of `tests/manual/ANN-2.md` covers the editor on Windows. | Shared; L1 run. |
| ANN-2 | Shared. Blur and pixelate are checked against golden pixels in CI; W1 manual run. | Shared; L1 run. |
| LIB-1 | Shared. The watcher polls folder signatures; the Windows CI run exercises it (added Oct 30, 2026: fresh folder time, no cached directory entry). | Shared. |
| LIB-2 | W1. Default folder is `Videos\CaptureMann`; check a OneDrive-redirected Videos folder and long paths (0.3.x). Free space comes from `GetDiskFreeSpaceExW`. | L1. XDG video dir, `statvfs` free space. Delete goes to the freedesktop.org Trash (`cm_platform::trash`, checked on Linux). |
| LIB-3 | Shared. | Shared. |
| LNC-1 | W1. Hotkey opens the launcher window; focus returns to the previous app. | L1. Focus return is limited on Wayland. |

## Recording, camera, audio, voice

| ID | Windows | Linux |
| --- | --- | --- |
| REC-1 | W2. Windows.Graphics.Capture source; Media Foundation H.264 hardware encoder into the shared fragmented-MP4 writer. Hardware tests on the primary machine; no hardware encoder gives `hardware_encoder_unavailable` (checked in a VM without a GPU). | L2. Portal ScreenCast through PipeWire (**stream, timestamps and cursor metadata checked on GNOME 46 in a VM**, see `verification/L0-portal-screencast-2026-10-31-claude.md`); VA-API or NVENC. **L1 product path (Oct 31, 2026, QEMU VM, report `verification/L1-recording-product-linux-2026-10-31-claude.md`):** `start_recording` and `capturemann record start/stop` record the whole screen through the portal and PipeWire into software VP9 (D-R2, D-R3; the restore token is stored in the config folder), with the cursor in the picture; window, region and camera answer `not_implemented`; a vanished shared screen stops the recording with reason `screen_halted`. 10 s at 30 fps: 301 frames, constant 33.33 ms gaps, 0.27 core. No hardware encoder, no real GPU. **App UI (Oct 31, 2026, VM, `verification/L1-recording-ui-linux-2026-10-31-claude.md`, behind `CM_LINUX_RECORDING=1`):** the Record sheet offers the display only (window, region and webcam are dimmed with a reason), explains the desktop's share dialog before the first start and blocks Start with a plain line when PipeWire or FFmpeg is missing; start, countdown, pause, mute, stop and a stream loss were driven through the app in the headless GNOME session and the recording shows in the library with its picture. Not run: a real desktop, KDE, Intel/AMD/NVIDIA GPUs. |
| REC-2, REC-3 | W2. 1080p30 is the gate (30 minutes with all tracks on the primary machine); 1080p60, 1440p and 4K are measured and reported, not gated. | L2. 30-minute 4K30 gate on one machine. |
| REC-4 | Shared (the session removes the pause gap); W2 run. | Shared; L2 run. |
| REC-5 | W1. The HUD and stop control are excluded from capture with `WDA_EXCLUDEFROMCAPTURE`. Whether Windows 10 22H2 shows a capture border, and whether it lands in the recording, is an open question for spike S6. | L2. The HUD cannot hide itself from capture on Wayland (nor on X11 through the portal), so on Linux **the HUD, the countdown and the brackets are not floating windows**: the controls are a banner in the Studio window (timer, pause, mute, discard, stop; countdown with skip and cancel), which opens by itself if it was closed, next to the tray icon (red state, Stop and Pause in its menu; needs a StatusNotifier host) and the desktop's own screen-share indicator (GNOME shows it in the top bar for every portal screen share; the app cannot turn it off). X11 can opt back into the floating HUD with `CM_LINUX_FLOATING_HUD=1` (it is then in the video). AC5 (no recording with the indicator hidden) holds through the banner and the system indicator; a desktop with no indicator, no tray host and a hidden Studio window (a compositor like wlroots without a panel) has none of them: not possible to guarantee on Wayland. Built Oct 31, 2026 (VM; no real desktop). |
| REC-6 | W2. Own windows via `WDA_EXCLUDEFROMCAPTURE`; excluding another app's window is to be confirmed by the S6 run before the criterion is promised. | L2. Not possible on Wayland for other apps; document per desktop. |
| REC-7 | W3. Low-level mouse hook for clicks and position, stamped by QPC; check against SmartScreen and antivirus behavior (0.3.x). | L2. Portal cursor metadata (**delivered on every frame in the GNOME 46 VM test** once the consumer asks for it with a size range); reduced click effects. |
| REC-8 | W2. `recording.lock` owner lock uses `File::try_lock` (works on Windows, tested in CI); a kill test on the primary machine. | L2. Same lock. |
| REC-9 | W3 (after the overlay). Topmost transparent window per display. | L2. Limited on Wayland. |
| CAM-1, CAM-2 | W3. Media Foundation camera; live bubble excluded from capture; camera is its own track. | L2. V4L2 camera. |
| CAM-3, CAM-4 | W3 (picker, preview and mirror use the same camera layer); CAM-3 layout editing is shared. | L2. |
| CAM-5 | Decided at W3 from the macOS implementation (which parts are OS-specific). | Decided at L2. |
| AUD-1 | W3. WASAPI microphone with the device picker and level meter. | L2. PipeWire or ALSA. |
| AUD-2 | W3. WASAPI loopback with own-process exclusion (process loopback needs Windows 10 build 19041 or later, which 22H2 is); 50 ms sync budget measured against QPC. | L2. Monitor source of the sink. **L1 (Oct 31, 2026, VM):** system audio is the monitor of the default output through PipeWire, recorded through the daemon and CLI with the screen (beeps 2.000 s apart over 10 s); PipeWire is also the default microphone source on Linux. A/V sync by flash and beep and `needs_reopen` are open. |
| AUD-3, AUD-4, AUD-6 | Follow the macOS implementation; any OS-specific part is decided at W3. Shared otherwise. | Same at L2. |
| VOC-1 | W3. Same recorder as AUD-1; `tests/manual/VOC-1.md` Windows run (consent, kill, 3-hour soak). The idle-sleep guard is a Windows power request (added Oct 30, 2026). | L2. |
| VOC-2 | Shared editor; W4 run with the Windows FFmpeg. | L3. |
| VOC-3 | W5 (needs whisper on Windows). | L3. |

## Editing, export

| ID | Windows | Linux |
| --- | --- | --- |
| EDT-1 … EDT-5 | Shared logic. W4: the DX12 compositor; the `parity` test and `tests/manual/EDT-1.md` to `EDT-5.md` runs on Windows hardware. Software renderers (WARP) skip the speed bound but not the pixel checks. | L3. Vulkan compositor (lavapipe in CI), `parity` on hardware. |
| EXP-1 | W4. Windows LGPL FFmpeg build with a self-check like macOS; the encoder is `h264_mf` (or a hardware vendor encoder), no libx264. | L3. Linux LGPL FFmpeg. |
| EXP-2 | Shared (presets). | Shared. |
| EXP-3 | W4. GIF and WebM through the bundled FFmpeg's component list. | L3. |
| EXP-4 | Shared (jobs). | Shared. |

## AI, redaction

| ID | Windows | Linux |
| --- | --- | --- |
| AI-1 | W5. whisper CPU build; GPU paths are not promised. | L3. Early build (Oct 31, 2026): whisper.cpp builds from source with the default `whisper-rs` features (CPU, no OpenMP, no libgomp) and transcribes on aarch64 Ubuntu 24.04 in the VM (`verification/L1-local-ai-linux-2026-10-31-claude.md`); x86_64 untested; `build-linux.sh` pins a fixed CPU baseline (`GGML_NATIVE=OFF`). |
| AI-2, AI-3, AI-4, AI-6 | Shared (text logic). | Shared. |
| AI-5 | W5, with the translation model. | L3. |
| AI-7 | W5. `llama-server` Windows build as a helper program, found as on macOS. | L3. `packaging/llama/build-linux.sh` builds it (CPU, static, optional Vulkan) and the engine finds it next to the executable or on `PATH`; built in the VM, not yet run with a model, not yet in the tarball. |
| AI-8 | W5. sherpa-onnx Windows build; also depends on spike S7. | L3. `packaging/sherpa/fetch-linux.sh` pins the v2.0.1 static libraries for aarch64 and x86_64; speaker finding ran on aarch64 in the VM with the real models (2 speakers in a two-voice clip); x86_64 libraries are pinned but never linked or run. |
| AI-9 | W5. | L3. |
| RED-1 | W5. `Windows.Media.Ocr` recognizer; the rules and the benchmark stay recognizer-agnostic. | L3. OCR choice; the button is hidden with a documented reason if none ships. |
| RED-2, RED-3 | Shared (rule data). | Shared. |

## Storage, publishing

| ID | Windows | Linux |
| --- | --- | --- |
| STO-1 | W1. Drive letters, UNC paths and removable drives; the unplug case is covered by an `icacls` stand-in in CI. | L1. XDG paths, mounts. |
| STO-2 … STO-5 | Shared; tokens go to Windows Credential Manager through `keyring` (W0 smoke test, passes on the CI runner; the vault is stored in parts there). | Shared; Secret Service through `keyring` works with an unlocked keyring (CI runs it against `gnome-keyring`); with none running it answers `platform_rejected` (`no_keyring`) saying what to start; the opt-in `CAPTUREMANN_SECRETS=file` (or a `secrets_backend` file holding `file` in the config folder) keeps the secrets in an owner-only, unencrypted `secrets.json` (decided Oct 31, 2026). Tested with fakes and temp files; the real no-keyring path is not run on a Linux desktop yet. |
| PUB-1 | W5. Credentials in Windows Credential Manager; `tests/manual/PUB-1.md` on Windows. | L3. |
| PUB-2 … PUB-7 | Shared provider code; one Windows run of `PUB-2.md`. | Shared. |

## Permissions, agents, diagnostics

| ID | Windows | Linux |
| --- | --- | --- |
| PRM-1 | W1. No screen-recording permission exists; microphone and camera privacy switches are Settings pages the app links to; there is no input-monitoring grant. Rows that do not apply are not shown. | L1. Portal prompts (screen, camera), no global grant. **Built (Oct 31, 2026, tested with fakes and in the VM, not on real hardware):** the setup screen has an OS-correct intro for Linux (no per-app prompts) and lists what the system lacks (session, desktop portal with ScreenCast v4+ and Screenshot, PipeWire, Secret Service, VAAPI) from `cm_platform::stack`; the same facts are in Copy diagnostics and `capturemann diagnostics`. Manual script: `tests/manual/LINUX-1.md`. |
| AGT-1 | W0. Named-pipe endpoint (`\\.\pipe\capturemann`) behind the same `Endpoint` type; the CLI and `cm-mcp` run on Windows early (D-W6). The Windows CI job runs the pipe tests. | W0-equivalent: Unix socket under `$XDG_RUNTIME_DIR`. |
| AGT-2 | W1. The confirmation dialog is a desktop window; shared logic. | L1. |
| AGT-3 | Shared. | Shared. |
| DIAG-1 | W1. Logs under `%APPDATA%\CaptureMann`; 0.3.x adds GPU, driver and OS build to the report. | L1. XDG paths; add GPU and driver. |
