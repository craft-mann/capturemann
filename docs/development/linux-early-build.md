# Linux early build: where it stands and what is left

Written Oct 31, 2026. Scope of the early build (maintainer, Oct 31, 2026): screenshots, annotation, library, local AI,
CLI and MCP. Recording is built as a prototype and hidden in the app (`CM_LINUX_RECORDING=1` turns it on). Everything
below was tested in an aarch64 Ubuntu 24.04 VM (headless GNOME 46 Wayland, Xvfb + openbox for X11) or on CI; nothing
has run on a real Linux desktop yet. Test lab for that: `scripts/testlab/` (ThinkPad T490, Intel UHD 620).

## Built (tested in the VM or on CI, reports in `verification/L1-*`)

| Area | State | Report |
| --- | --- | --- |
| Screenshots, X11 | display, region, titled windows (Xvfb in CI) | `L0-xcap-linux`, `L1-capture-targets-linux` |
| Screenshots, Wayland | portal screenshot; region by freezing the picture; window/region by the compositor's picker (`--pick`; its success path never ran headless) | `L1-wayland-screenshot`, `L1-capture-targets-linux` |
| Desktop app | starts on X11 and GNOME Wayland; client-side title bar on Wayland; app id `dev.capturemann.app` | `L1-desktop-shell-linux`, `L1-windows-linux` |
| Tray | StatusNotifierItem over D-Bus (no GTK); tested against a stand-in watcher only | `L1-tray-linux` |
| Clipboard, hotkeys | X11 clipboard and XGrabKey hotkeys work; Wayland clipboard unproven; no GlobalShortcuts portal on GNOME 46 (toast + `cm-desktop --launcher` fallback) | `L1-clipboard-hotkeys-linux` |
| Secrets | `no_keyring` error; `CAPTUREMANN_SECRETS=file` opt-in | acceptance row STO-2..5 |
| Local AI | whisper (CPU), speaker engine (sherpa Linux libs, pinned), llama-server build script; no text model run | `L1-local-ai-linux` |
| Auto-redact | Text recognizer = the system `tesseract` program as a helper (not linked, not bundled); button dimmed with the install command when missing; synthetic benchmark on aarch64, dev split; editor not seen on a desktop | `L1-auto-redact-linux` |
| Recording (hidden) | PipeWire + software VP9 through the daemon/CLI; stream-loss handling | `L1-recording-linux`, `L1-recording-product-linux`, `L1-vp9-export-linux` |
| Packaging | tarball + `install.sh` + `check-runtime.sh` + minisign signing; clean Ubuntu container proof | `L1-packaging-linux`, `L1-packaging-clean-linux` |

## Before calling it releasable

1. **CI green on the final commit** (macOS, Ubuntu, Windows; the PR must not be a draft for the Windows job). The
   Ubuntu job now builds with the speaker engine (`make sherpa`); that change has not run on GitHub yet.
2. **Package workflow run on GitHub** (Actions > Linux package) for x86_64 and aarch64, with FFmpeg bundled
   (`packaging/ffmpeg/build-linux.sh`, not built on x86_64 yet). The sherpa libraries and `llama-server` are not in the
   tarball yet (`build-linux.sh` uses sherpa when `packaging/sherpa/out/lib` exists; `CM_REQUIRE_SHERPA=1` makes
   that an error when missing; no `llama-server` bundle step).
3. **Real-machine run of `tests/manual/LINUX-1.md`** on the T490: Ubuntu 24.04, X11 and Wayland, with a report in
   `verification/`. Things only a real session can show: the `--pick` picker, the Wayland clipboard, the tray in a
   panel (GNOME AppIndicator extension, KDE), the title bar on a real GNOME, HiDPI/fractional scale, multi-monitor,
   VA-API (`vainfo`) and the VP9 recorder's CPU use on a UHD 620.
4. **Sign** the tarballs: `packaging/linux/sign-linux.sh` (needs the maintainer's minisign key and `minisign.pub`
   committed, see its header). Publish as a GitHub pre-release by hand.
5. Release notes: early build, no recording in the app, speaker finding only if the sherpa libraries are bundled,
   MP4 export needs a VA-API GPU or falls back to VP9, glibc 2.39+, the apt packages, how to verify the download.

## Known gaps and open items (none block the early build)

- Auto-redact needs `sudo apt install tesseract-ocr` (not in the tarball, not a package dependency yet); recall is clearly below Apple Vision's on the synthetic set; x86_64 and a real desktop are untested; Windows will use `Windows.Media.Ocr` later.
- Wayland: no always-on-top (toasts are in-window cards), no window list, no GlobalShortcuts on GNOME 46; the capture
  cloak (hide own toasts for a capture) was inconclusive on Wayland in the shared VM session.
- `cm_platform::dirs::video_dir()` ignores xdg-user-dirs; `cm-overlay`'s picked-colour copy can be lost on X11
  without a clipboard manager; keycaps say "Win" for Super; portal-chosen shortcut triggers are not shown in Settings;
  light panels get the dark tray icon; XDG autostart from an AppImage would point at a temporary mount.
- xcap's `current_monitor` compares `y` with the monitor's `x` (upstream bug, not changed).
- ONNX Runtime's `ThirdPartyNotices.txt` is still missing from the licence notes (sherpa README).
- The system-audio track showed a 21 ms resync and a -2229 ppm figure in the product run; not investigated.
- Windows: two branches are parked (`parked/windows-packaging`, `parked/windows-ci-real`), never run on Windows.
