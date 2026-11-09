# GitHub release text for 0.3.0 (draft, not published)

Prepared the night of Nov 8 to 9, 2026 by the night orchestrator, from `specs/plans/release-0.3.0-plan.md`,
`specs/plans/linux-parity-matrix.md`, `git log v0.2.9..HEAD` and the stored reports in `verification/` (`L1-linux-0.3.0-*`).
**Nothing here is final until the maintainer has read it.** Every `<FILL: ...>` is a number or a decision that only exists
on the release day (checksums, signatures, APT host and key fingerprint). Wording rule (CLAUDE.md): no "verified" or "passed"
without a stored report; lines without a report say "not run". Steps: `release-day-0.3.0.md`.

- **Tag:** `v0.3.0` (never reuse a tag). **Title:** `CaptureMann 0.3.0: Linux catches up`.
- **Type:** draft, then **pre-release** (the version is 0.x). Linux only: no Windows or macOS file.
- **Files to attach** (x86_64 from the lab laptop, aarch64 from the VM, built from commit `6cdcb033`): 
  `capturemann-0.3.0-linux-x86_64.tar.gz`, `capturemann_0.3.0_amd64.deb`, `capturemann-0.3.0-linux-aarch64.tar.gz`,
  `capturemann_0.3.0_arm64.deb`, each with `.sha256` and `.minisig`, and `SHA256SUMS.txt`.

---

## Release body

# CaptureMann 0.3.0: Linux catches up

Until now Linux was an early build (0.2.6: screenshots, the editor, local AI, the CLI and MCP; recording was hidden). **0.3.0 brings
Linux to what Windows and macOS have:** you can record the screen, the microphone, system audio and the camera, import and
export video on a computer without a hardware encoder, and use the tray, the editor and the agent tools as on the other
systems. A few things differ by design or are not possible on Linux; they are listed below, with the reason. This release is
**Linux only** (Ubuntu 24.04 or newer, x86_64 and aarch64). Windows and macOS stay at 0.2.9.

## What you can do on Linux now

- **Record the screen** (display, region, and on X11 a window) at the display's resolution, 30 or 60 fps, with pause and
  resume, a countdown, a banner in the Studio window (timer, pause, mute, discard, restart, stop) and a red tray icon.
  On GNOME (X11 and Wayland) CaptureMann records through GNOME's screen-cast service **without the "Share Screen" dialog**; on
  other desktops it uses the XDG desktop portal and your desktop asks. **Because GNOME's dialog is skipped, CaptureMann's own
  confirmation for agent-started recordings, its recording indicator and GNOME's orange recording dot are what tell you a
  recording is running.** A crash loses at most about one fragment (2 seconds); the next start recovers the take.
- **Hardware H.264 where the GPU has it** (VA-API; Intel Quick Sync works, including drivers that only offer a fixed
  quantiser). Without it, recording, import and export use **software VP9**, and the result, the CLI and the toast say
  "VP9, not H.264" and why. VP9 is royalty-free; no software H.264 encoder ships (as before).
- **Record the microphone and system audio**, with the microphone list read from PipeWire.
- **Record the camera** (Video4Linux2: MJPEG and YUYV webcams) as its own track, with a camera picker and a check in Settings.
- **Cursor track on X11** (position, clicks, scroll) for the editor's cursor styling.
- **Import** MP4, MOV, MKV and the other formats and **export** MP4, platform presets, GIF and WebM, with or without a GPU encoder.
- **Window screenshots on Wayland (GNOME):** through the desktop's own picker; CaptureMann uses the file the shell saved.
- **Tray, shortcuts, Open with** (the desktop entry offers CaptureMann for importable media), idle-sleep guard while recording,
  Auto-redact (uses the system `tesseract`), speaker labels and the built-in text model: as listed in the parity table in
  `specs/plans/linux-parity-matrix.md`.
- **Install:** a tarball and a `.deb` for x86_64 and aarch64, minisign signatures, and a signed APT repository tool
  (published: the assets of the GitHub release tagged `apt`; key fingerprint `516C CD4F C7B3 5167 8803  F71B DEB1 BB54 FAEF 557D`; install lines at the end of the notes; `docs/development/linux-apt.md`).


## What is different or missing on Linux (and why)

- **No live webcam bubble during a recording.** The portal and the screen-cast service cannot leave one window out of the
  capture, so a floating bubble would end up in the video. The camera is recorded as its own track and **composited at
  export**; the setup bubble before the take works.
- **No floating HUD**; a banner in the Studio window and the tray icon instead (the HUD would be in the video on Wayland).
- **Other apps' windows cannot be excluded** from a recording (no desktop allows it); the Privacy page says so.
- **Window recording:** X11 only. On Wayland a window has no id, and GNOME 46's window source gives a monitor-sized frame
  (the window at the top left on transparency) that needs the buffer's crop metadata; **not built**.
- **Cursor track: X11 only.** On Wayland the pointer is drawn in the picture and there are no clicks.
- **Global shortcuts: X11 only.** GNOME 46 on Wayland has no GlobalShortcuts portal; set a keyboard shortcut in the
  desktop's settings that runs `cm-desktop --launcher` or `cm-desktop --capture region`. The KDE portal path is written, not run.
- **Tray:** needs a StatusNotifier host; stock GNOME needs the AppIndicator extension (otherwise a toast and the launcher
  window are the fallback).
- **Camera track:** software VP9 only.
- **Auto-redact** on Linux uses the system `tesseract` (recall is lower than the Windows and macOS recognisers on the synthetic
  benchmark); the editor button is dimmed with the install command when it is missing.
- **Vietnamese:** 117 new Linux strings are machine-written drafts.
- **YouTube preset without a hardware encoder:** VP9 + AAC in an MP4, labelled; a real WebM/Opus file is planned.

## How it was tested

Reports are in `verification/` (`L1-linux-0.3.0-*`); all are marked PARTIAL. Tested on **one** computer: ThinkPad T490, Intel UHD
620, Ubuntu 24.04.5, GNOME 46, X11 and Wayland, built-in webcam, plus an aarch64 Ubuntu 24.04 VM (headless: no recording, no
desktop cases). `cm-regress` on the first build of these files (commit `5301d4b8`, extracted tarball unless noted), passed / failed / skipped. The files above were rebuilt from `6cdcb033` (only the "early build" wording, the cursor-rule move to `cm-platform`, `pw_registry.rs` starting its helper through `cm_platform::process::command`, and Linux-only gating of two desktop details differ): <FILL: result of the smoke and standard runs on the rebuilt files>.

| Run | Result |
| --- | --- |
| X11, smoke | 13 / 0 / 1 |
| X11, standard | 49 / 0 / 7 |
| X11, hardware tier (screen, microphone, camera; no sound) | 58 / 0 / 20 of 78 |
| X11, full tier with the tarball installer case | 57 / 0 / 14 |
| X11, full tier with the `.deb` installer case | 56 / 0 / 15 (the `.deb` case itself was skipped there because 0.2.9 was installed; run on its own after `apt remove` and `purge`: 1 / 0 / 0) |
| X11, standard on the installed `.deb` (an `apt` upgrade from 0.2.9 first) | 49 / 0 / 7 |
| X11, `rec.soak` 10 minutes (screen and microphone; constant 30 fps and 2 s key frames checked) | 1 / 0 / 0 |
| GNOME 46 Wayland, smoke | 13 / 0 / 1 |
| GNOME 46 Wayland, standard | 47 / 0 / 9 |
| GNOME 46 Wayland, hardware tier (no sound) | 55 / 0 / 23 of 78 |
| aarch64 VM (headless), standard / full | 34 / 0 / 22 and 35 / 0 / 36 |

All 17 failures of the 0.2.6/0.2.9 Linux run have an outcome in `verification/L1-linux-0.3.0-triage-2026-11-09-claude.md`.
10-minute recording gate on X11 (silent: 18013 frames, none dropped, 300 of 300 flashes, tracks end within 24 ms) and on
Wayland (screen + microphone + system audio: flash-to-beep offset median -5.9 ms, no drift); `kill -9` recovery lost less
than one second on Wayland.

## Not run

- Any other computer or GPU: **AMD and NVIDIA** hardware encoders, a second monitor or a display scale other than 1, mixed DPI.
- **KDE, wlroots (Sway, Hyprland), Fedora/Arch/Debian** (Ubuntu 24.04 only), Flatpak and Snap (unsupported).
- **Sound cases** (system audio recording, the chime player, the speakers) in the final regression: quiet hours.
- The flash-to-beep offset on X11, a 30-minute soak, X11 window recording from the CLI on hardware, Wayland window recording.
- A second USB webcam, low light, replugging, a camera without access (video group).
- The APT repository on GitHub's runners and with a real key, the Linux package workflow on GitHub (never run there),
  `ubuntu` versions other than 24.04.
- The 3 manual scripts for IME typing (IBus Telex/VNI) on the final build.

## Files

| File | SHA-256 |
| --- | --- |
| `capturemann-0.3.0-linux-x86_64.tar.gz` | `6f6904df654fadbd7c74772f9ee9bfa6e595dc2da2ee60bda2cf473bd7dc166f` |
| `capturemann_0.3.0_amd64.deb` | `d2b928d359f706bff6ce05124fca29f0fbb0f4c6d8685a9933600ba5e543e311` |
| `capturemann-0.3.0-linux-aarch64.tar.gz` | `41b4782e31d80ce5a47f0adac28065d357c09f769cbc578657dd507ee2d60463` |
| `capturemann_0.3.0_arm64.deb` | `4e024e03801764a1ca1df56b1af09990bfcae12b30fdc0dffb1b41ada1b65e0b` |

Minisign public key: `packaging/linux/minisign.pub`; public key `RWTeQv8ogB2CCuZ7j0rr3MovdHod4QATmVn+I329YC8pomOC6R9Rxc2k` (verify: `minisign -Vm <file> -P <that key>`).
Install: `sudo apt install ./capturemann_0.3.0_amd64.deb` (or the tarball and `install.sh`).
