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
| Local AI | whisper (CPU), speaker engine (sherpa Linux libs, pinned; in the package with its models), llama-server (built and in the package); no text model run | `L1-local-ai-linux`, `L1-packaging-complete-linux` |
| Recording (hidden) | PipeWire + software VP9 through the daemon/CLI; stream-loss handling; **in the app behind `CM_LINUX_RECORDING=1`**: Record sheet (display only), banner in the Studio window instead of a floating HUD, the desktop's share dialog explained, readable errors, library picture, Play and Reveal with a message when nothing opens the file (VM only) | `L1-recording-linux`, `L1-recording-product-linux`, `L1-vp9-export-linux`, `L1-recording-ui-linux` |
| Packaging | tarball (speaker engine, models, `llama-server`, FFmpeg, licences) + `install.sh` + `check-runtime.sh` + minisign signing; `.deb` from the tarball; clean Ubuntu container proof of tarball and `.deb` (apt install, run, remove, purge) | `L1-packaging-linux`, `L1-packaging-clean-linux`, `L1-packaging-complete-linux` |

## Before calling it releasable

1. **CI green on the final commit** (macOS, Ubuntu, Windows; the PR must not be a draft for the Windows job). The
   Ubuntu job now builds with the speaker engine (`make sherpa`); that change has not run on GitHub yet.
2. **Package workflow run on GitHub** (Actions > Linux package) for x86_64 and aarch64, with FFmpeg, the speaker
   engine and `llama-server` bundled and the `.deb` built from the tarball (done in the aarch64 VM by hand, see
   `L1-packaging-complete-linux`; the workflow steps for `make sherpa`, `llama-server` and the `.deb` have not run on
   GitHub, and `packaging/ffmpeg/build-linux.sh`, `build-linux.sh` and `build-deb.sh` have not run on x86_64).
   The workflow passes `CM_REQUIRE_SHERPA=1`, `CM_REQUIRE_SPEAKER_MODELS=1`, `CM_REQUIRE_LLAMA=1`, `CM_REQUIRE_FFMPEG=1`,
   so a missing piece fails the job instead of producing a smaller package.
3. **Real-machine run of `tests/manual/LINUX-1.md`** on the T490: Ubuntu 24.04, X11 and Wayland, with a report in
   `verification/`. Things only a real session can show: the `--pick` picker, the Wayland clipboard, the tray in a
   panel (GNOME AppIndicator extension, KDE), the title bar on a real GNOME, HiDPI/fractional scale, multi-monitor,
   VA-API (`vainfo`) and the VP9 recorder's CPU use on a UHD 620.
4. **Sign** the tarballs: `packaging/linux/sign-linux.sh` (needs the maintainer's minisign key and `minisign.pub`
   committed, see its header). Publish as a GitHub pre-release by hand.
5. Release notes: early build, recording hidden in the app (`CM_LINUX_RECORDING=1`; see the recording UI report), MP4 export needs a VA-API GPU or falls back to VP9, glibc 2.39+,
   `sudo apt install ./capturemann_<version>_<arch>.deb` (or the tarball and the apt packages), the text model is a
   download on the AI page (no model file ships), how to verify the download.
6. **Decide on the size**: `cm-desktop` is about 700 MB because the release profile keeps debug info, so the tarball is
   about 230 MB and the `.deb` about 155 MB (900 MB installed). `build-deb.sh` has `CM_DEB_STRIP=1` (`strip --strip-debug`
   on the four programs); stripping in the release profile (`[profile.release] debug`/`strip`) is a `Cargo.toml`
   decision for the maintainer (crash reports and backtraces keep function names with `strip = "debuginfo"`).
7. **Maintainer address** for the `.deb`: `CM_DEB_MAINTAINER` defaults to "The CaptureMann contributors
   <noreply@users.noreply.github.com>"; set a real one for a release.

## Known gaps and open items (none block the early build)

- Recording UI (with `CM_LINUX_RECORDING=1`, default still off): the controls are a banner in the Studio window (no
  floating HUD, countdown disc or brackets: they would be in the video, and Wayland cannot place them); the tray icon
  and the desktop's own screen-share indicator also show it. A real desktop (GNOME with the AppIndicator extension, KDE)
  has not been seen yet. The Start sheet keycaps ("Ctrl Alt Shift R") overflow the footer and the line "Start or stop
  from anywhere" is wrong on GNOME 46 Wayland (no GlobalShortcuts portal). Settings -> Capture & recording still offers
  H.264/HEVC and quality choices that mean nothing for the software VP9 recorder. The permissions page lists the Camera.
- Wayland: no always-on-top (toasts are in-window cards), no window list, no GlobalShortcuts on GNOME 46; the capture
  cloak (hide own toasts for a capture) was inconclusive on Wayland in the shared VM session.
- Light panels get the dark tray icon; XDG autostart from an AppImage would point at a temporary mount.
- Fixed since the first list (tested with fakes and, for the audio stamps, in the VM, see
  `verification/L1-audio-drift-linux-2026-10-31-claude.md`): `video_dir()` reads `XDG_VIDEOS_DIR` from
  `user-dirs.dirs` and the desktop app asks it too; one opener helper (`cm_platform::opener`) names `xdg-utils` when
  `xdg-open` is missing; the overlay copies a picked colour through `wl-copy`/`xclip` first (they outlive the overlay)
  and logs a failure; keycaps say "Super"; Settings -> Shortcuts shows "Set by your desktop: ..." from the portal's
  answer (the portal path itself still needs a desktop that has the GlobalShortcuts portal to be seen working).
- xcap's `current_monitor` compares `y` with the monitor's `x` (upstream bug, not changed).
- ONNX Runtime's `ThirdPartyNotices.txt` is not missing any more: it was already in `packaging/sherpa/licenses/` and in
  the generated `THIRD_PARTY_LICENSES.md` (v1.28.2, the version inside the macOS and the Linux aarch64 libraries; the
  x86_64 library was not inspected). The tarball and the `.deb` also carry it in `share/doc/capturemann/SHERPA-ONNX/`.
- `cm-ai` looks for the speaker models in `<executable dir>/../Resources/Models/speaker` (a macOS bundle path), so on
  Linux the packages put them in `lib/Resources/Models/speaker` next to `lib/capturemann`. A Linux-specific lookup
  (for example `share/capturemann/models/speaker`, or `$XDG_DATA_DIRS`) would be tidier but is a `crates/cm-ai` change.
- The `.deb` installs under `/opt/capturemann` (symlinks in `/usr/bin`) for that reason; `lintian` reports
  `dir-or-file-in-opt` (not an archive package), `unstripped-binary-or-object`, `embedded-library` (the bundled LGPL
  FFmpeg) and `no-manual-page`. No apt repository, no RPM, no signature inside the `.deb` (minisign signs the file).
- The system-audio track showed a 21 ms resync and a -2229 ppm figure in the product run; not investigated.
- The 21 ms resync and -2229 ppm in the system-audio track came from stamping at a graph quantum change; fixed
  (graph time instead of callback time). Real-device clock drift, a microphone that reports a delay and the picture/sound
  offset with the new stamps are not measured.
- Windows: two branches are parked (`parked/windows-packaging`, `parked/windows-ci-real`), never run on Windows.
