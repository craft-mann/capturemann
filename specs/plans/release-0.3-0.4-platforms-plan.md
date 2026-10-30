# Platform plan: 0.3 Windows, 0.4 Linux (and their stabilization releases)

Status: **draft, revision 1** (Oct 30, 2026), nothing built. Set by the maintainer: **after 0.2.5 there are no
new features**; the work is to extend the supported platforms. 0.3.0 = Windows, 0.3.x = Windows stabilization,
0.4.0 = Linux, 0.4.x = Linux stabilization. Input: `docs/project/platform-research-2026-10-30.md` (the
inventory of every macOS-only part and the lessons from MangoDisk, a Rust app that ships on macOS, Windows and
Linux). Wording rule: a box is checked only on the platform it belongs to, and nothing is "verified" without a
stored report in `verification/`; the research annex marks every unconfirmed platform fact as an assumption.

## What we take from MangoDisk (ideas only: it is GPL-3.0, we copy no code)

1. **One place for OS code.** It has a platform crate with OS-neutral traits and one adapter per OS, chosen by a
   single `cfg`. We have 55 files with `target_os`. Plan: a `cm-platform` crate for the small cross-cutting parts
   (host clock, sleep inhibitor, config/data/video folders, login item, open-settings link, memory probe), one
   `sys/` module per capture crate behind the trait the macOS code already implements, and a CI check that
   `target_os` appears only in those modules.
2. **A multi-OS PR gate from the start.** Matrix `windows-latest`, `macos-latest`, `ubuntu-22.04`,
   `fail-fast: false`, rust-cache, the same fmt/clippy/test on each, run on every PR, before any backend works.
3. **Installer tests in CI** (silent install, upgrade, uninstall leaves the user's library alone) and an
   `asInvoker` manifest: the recorder needs no administrator rights and installs no driver or service.
4. **A package checker script per OS** (as `packaging/macos/verify-release.sh`): architecture, glibc ceiling,
   no bundled `libwayland`/GPU libraries on Linux, signature and version on Windows.
5. **Honesty about what CI cannot check.** Hosted runners have no camera, microphone or reliable capture, so
   capture, audio, camera, IME and multi-DPI stay manual scripts with stored reports.
6. **CLI first.** `capturemann` and `cm-mcp` are headless; shipping them on Windows and Linux early gives users
   export, transcripts and agent access while the recorder is still being built.

Does not transfer: the web view (we use GPUI), the Tauri updater (updates stay manual, no network when off),
privileged drivers and services, arm64 on day one (each architecture needs its own FFmpeg, sherpa-onnx, llama
and whisper builds).

## Step 0 (before 0.3, no OS needed; fits the end of 0.2.5 or a 0.2.6)

No new features; a refactor and CI only:
1. Windows and Ubuntu jobs in `check` (build, clippy `-D warnings`, fake-based tests, `cargo deny`).
2. `cm-platform` and the per-crate `sys/` split on macOS with no behaviour change; the `target_os` lint.
3. Keep the 0.2.5 redaction rules recognizer-agnostic (they are: only the recognizer is per OS).
4. Per-feature "Windows" and "Linux" acceptance rows in the specs (IDs unchanged).

## 0.3.0: Windows 10 22H2 and Windows 11, x64

Scope: everything in 0.2.x that works on macOS and has an OS API on Windows; items that need new engineering
(Windows OCR, speaker engine, llama) may move to 0.3.x if they threaten the date (D-W5).

| Stage | Content | Done when |
|---|---|---|
| W0 | the primary Windows machine (D-W1); spikes S1 to S6 on Windows (S6 also checks, as assumptions to confirm, whether Windows 10 22H2 shows the yellow capture border and whether it appears in the recording, and the CPU load of a 1080p30 recording on this machine); QPC clock; named-pipe IPC tests; Credential Manager smoke test | each spike has a stored report and Go / conditional Go / No-go in `docs/project/spikes.md`; GPUI on Windows judged (S5), iced is the recorded fallback |
| W1 | Screenshots, overlay (DPI 100/150/mixed), library, tray, hotkeys, `WDA_EXCLUDEFROMCAPTURE`, clipboard, login item, permission links | `CAP-1`, `CAP-1-overlay`, `ANN-2`, `PRM-1` Windows runs stored |
| W2 | Windows.Graphics.Capture source, Media Foundation H.264 hardware encoder into the shared fragmented-MP4 writer, pause and crash recovery | hardware tests pass on the primary machine; 10-minute soak; **30 minutes of 1080p30 with screen, microphone, system audio, webcam bubble and cursor on the primary machine** (a 15 W integrated-graphics laptop); 1080p60, 1440p and 4K are measured and reported, not gated; no hardware encoder gives `hardware_encoder_unavailable` (checked in a VM without a GPU) |
| W3 | WASAPI loopback with own-process exclusion, microphone, Media Foundation camera and live bubble, cursor hook | `REC-1`, `REC-5`, `REC-7`, `CAM-1`, `CAM-2`, `AUD-1`, `VOC-1`, `I18N-IME` Windows runs stored; 50 ms sync budget measured |
| W4 | DX12 compositor, preview upload path measured, Windows LGPL FFmpeg build with a self-check like macOS | `parity` test passes on Windows hardware; `EXP-1/2`, `EDT-1..5`, `PLAY-1`, `CAM-3` runs stored; no GPL in any shipped binary |
| W5 | `Windows.Media.Ocr` recognizer, whisper CPU, sherpa-onnx and llama Windows builds, publishing with the Windows credential store | `AI-5/7/8/9`, `PUB-1/2`, clean-VM first run stored |
| W6 | Installer, signing decision, winget, `release.yml` Windows job tested on a fork first | install, record 5 minutes with all tracks, export, uninstall on Windows 10 22H2 and 11; `verification/RELEASE-0.3.0-*.md` |

## 0.3.x: Windows stabilization

Small releases (about every 1 to 2 weeks while reports arrive, time-boxed). Order of risk: GPU and driver matrix
(NVIDIA, Intel, AMD, hybrid laptops), mixed DPI, HDR, protected-content black frames, high refresh rates,
antivirus and SmartScreen on a new binary that hooks the mouse, OneDrive-redirected folders and long paths,
sleep and lid close, windows of elevated apps, Narrator, Vietnamese IME edge cases, installer upgrades. The
0.2.4 diagnostics export gains GPU, driver and OS build. "Stable" bar (proposal): two consecutive point releases
without a lost or unplayable recording, the 30-minute 1080p30 gate re-run on a second GPU vendor and on Windows 11 with reports, signing
state documented, winget package accepted.

## 0.4.0: Linux (x64; Wayland first on GNOME and KDE, X11 where the same code works)

| Stage | Content | Done when |
|---|---|---|
| L0 | Ubuntu 22.04 CI; spikes on real sessions: GPUI on Wayland and X11 with IBus/fcitx5, xdg-desktop-portal ScreenCast through PipeWire at 4K60, VA-API and NVENC encode, portal cursor metadata, GlobalShortcuts portal, layer-shell on KDE | reports stored; decisions D-L1 to D-L5 made from the results |
| L1 | Portal screenshot and X11 path, region selection per desktop, clipboard, XDG paths, Secret Service with a fallback, autostart | `CAP-1`, `ANN-2` Linux runs on GNOME Wayland, KDE Wayland and one X11 session |
| L2 | Portal screencast source, VA-API/NVENC encoder, PipeWire clock mapping, monitor-source system audio, microphone, V4L2 camera, cursor from the portal | hardware tests on Intel, AMD, NVIDIA; 30-minute 4K30 gate on one; no encoder gives `hardware_encoder_unavailable` (or the chosen fallback) |
| L3 | Vulkan compositor (lavapipe in CI), Linux LGPL FFmpeg, OCR choice, whisper/sherpa/llama Linux builds | `parity` on Linux hardware; Auto-redact works or the button is hidden with a documented reason |
| L4 | Packaging (Flatpak first, then AppImage, `.deb`, `.rpm`), package checker script, container install smoke tests in CI | install on clean Ubuntu GNOME, Fedora KDE and an X11 session; `verification/RELEASE-0.4.0-*.md` |

Wayland limits are written into the user guide per desktop: no custom full-screen overlay on GNOME, window lists
and shortcuts only through portals, our HUD cannot hide itself from capture, cursor and click effects reduced,
the tray needs an extension on GNOME (the launcher window is the fallback).

## 0.4.x: Linux stabilization

Support matrix in `docs/` (supported, best effort, unsupported per desktop): GNOME, KDE, Sway/Hyprland, XFCE on
X11; NVIDIA proprietary versus nouveau; portal restore tokens; Flatpak permissions; fractional scaling; IME;
missing keyring; Arch and NixOS reports. Done when two point releases pass without a recording-loss bug and the
matrix is published.

## Risks (ranked)

1. GPUI is untested on Windows and Linux and has no stability promise (pinned `=0.3.9`).
2. The maintainer has no Windows or Linux hardware, so most acceptance boxes are manual.
3. Linux hardware H.264 availability against our "no software H.264" rule (Fedora/RHEL restrictions).
4. Wayland capability loss; claims in docs must be per desktop.
5. Preview speed without `surface()` on Windows and Linux.
6. SmartScreen and antivirus on a new, possibly unsigned, screen-recording binary.
7. Sidecar builds (FFmpeg, sherpa, llama, whisper) multiply by OS and architecture; `release.yml` has never run on GitHub.
8. Clock sync across QPC, PipeWire and WASAPI against the 50 ms budget.
9. Vietnamese IME in GPUI on Windows (TSF) and Linux (IBus/fcitx).
10. Maintainer capacity for two OS releases back to back: stabilization releases are time-boxed.

## Decisions needed from the maintainer

Windows: **D-W1** hardware: **decided Oct 30, 2026**: the maintainer's ThinkPad (Windows 10 22H2, Intel Core i7-8665U 1.9 GHz, 32 GB RAM, Intel UHD 620 integrated graphics, built-in webcam and microphone) is the primary machine; no dedicated GPU is required (Media Foundation picks Quick Sync, AMD or NVENC itself). Before 0.3.0 is called stable add a second GPU vendor (AMD or NVIDIA, through beta testers or a rented cloud machine) and a Windows 11 check (VM or another PC); a self-hosted runner stays open;
**D-W2** decided Oct 30, 2026: x64 only for 0.3.0, ARM64 later; **D-W3** decided Oct 30, 2026: per-user NSIS installer (no administrator rights, `asInvoker`) plus a portable zip; MSI and MSIX are not used;
**D-W4** decided Oct 30, 2026: check SignPath Foundation eligibility early (in W0; it needs a public build pipeline and acceptance by the programme, to be confirmed), keep Azure Artifact Signing and an unsigned beta with SHA-256 files as fallbacks; who holds the identity is still open;
**D-W5** decided Oct 30, 2026: 0.3.0 is capture, record, edit and export; Windows OCR, speaker finding and llama move to 0.3.x; **D-W6** decided Oct 30, 2026 for Windows: ship `capturemann` and `cm-mcp` early (about W0 plus W4), since they are headless; Linux to be decided in 0.4;
Linux: **D-L1** primary channel (Flatpak first?) and who maintains AUR/Nix; **D-L2** Linux without a hardware
H.264 encoder: refuse, or allow a royalty-free codec path (needs a patent read and a change to the constraint in
`CLAUDE.md`); **D-L3** HUD visible in capture on Linux or only in the tray; **D-L4** Wayland-first and the
definition of "supported"; **D-L5** secrets without a Secret Service (encrypted file with a passphrase?);
**D-L6** Linux arm64 and Windows ARM64; **D-L7** run an apt/dnf repository or only GitHub Releases and Flathub.
Both: **D-A** updates stay manual (current decision) or an opt-in check; **D-B** label 0.3.0 and 0.4.0 as beta.
