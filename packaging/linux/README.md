# Linux packaging (early build)

`packaging/linux/build-linux.sh` builds `capturemann-<version>-linux-<arch>.tar.gz` (and a `.sha256`) for
`x86_64` and `aarch64`, from `cargo build --release --no-default-features`. The workflow
`.github/workflows/linux-package.yml` runs it by hand on both architectures. Status of what was run and where:
`verification/L1-packaging-linux-2026-10-31-claude.md`.

## Decision: a tarball with an install script (v0)

| Option | For | Against | Verdict |
| --- | --- | --- | --- |
| **tar.gz + `install.sh` + `.desktop` + icons** | One file for every distribution and both CPUs; no root (`~/.local`); trivial to build in CI and to inspect; same shape as the Intel Mac "early build"; the app finds `ffmpeg` next to its own executable | The user installs the system libraries; no automatic updates; no menu entry until `install.sh` ran | **Primary for v0** |
| `.deb` | Declares dependencies, `apt install ./file.deb` pulls them; removable | Ubuntu/Debian only, one build per release of the distribution (t64 library names differ between 22.04 and 24.04), root needed, signing and a repository later | Second step, built from the same staged tree (`dpkg-deb`), once the dependency list has survived real machines |
| AppImage | One file, runs anywhere | Bundling GTK, Mesa, Vulkan, PipeWire and portal libraries is fragile for a GPU app that talks to the desktop portal and the Secret Service; needs FUSE 2 (not on Ubuntu 24.04 by default); a bundled FFmpeg and libva clash with the host drivers | Not for v0 |
| Flatpak | Sandbox with portals, store | Recording, global shortcuts, the CLI/MCP binaries on the host `PATH` and the Unix socket all need holes in the sandbox; manifest, runtime and review work | Later (the portal code paths are already the right ones for it) |

Reasons in short: the product is a desktop app plus a CLI and an MCP server that agents start from a shell,
so unpacking into a normal prefix with symlinks in `bin/` suits it better than a sandbox; and there are no
users yet who would be hurt by installing five apt packages by hand.

## What is in the tarball

```
capturemann-<version>-linux-<arch>/
  install.sh                          --prefix P (default ~/.local), --uninstall
  README.txt
  lib/capturemann/{cm-desktop,cm-overlay,capturemann,cm-mcp[,ffmpeg,ffprobe,FFmpeg/]}
  share/applications/dev.capturemann.app.desktop
  share/icons/hicolor/{16..512}x.../apps/dev.capturemann.app.png, scalable/apps/dev.capturemann.app.svg
  share/doc/capturemann/{LICENSE,NOTICE,THIRD_PARTY_LICENSES.md,CHANGELOG.md,LIBRARIES.txt}
```

`install.sh` copies `lib/capturemann/` to `<prefix>/lib/capturemann/`, links `cm-desktop`, `cm-overlay`,
`capturemann` and `cm-mcp` into `<prefix>/bin/`, writes the `.desktop` file with the real `Exec` path and
refreshes the desktop and icon caches. All programs stay in one folder because `cm-media` looks for
`ffmpeg` next to the real executable (symlinks are resolved) before `PATH`. `--uninstall` removes only what it
installed; the library and settings are never touched. The icons are copied, not redrawn, from
`design/brand/app-icon` (the full-bleed set, which `design/CLAUDE.md` assigns to Linux).
`THIRD_PARTY_LICENSES.md` is generated for the Linux crate set (`CM_LICENSE_TRIPLES` in
`packaging/macos/gen-third-party.sh`); if the crates are not fetched the script falls back to the checked-in
macOS one and says so.

## Building

```sh
sudo apt install build-essential cmake pkg-config nasm curl xz-utils libva-dev \
  libasound2-dev libxkbcommon-x11-dev libxkbcommon-dev libwayland-dev libxcb1-dev libx11-xcb-dev libvulkan-dev \
  libfontconfig-dev libssl-dev libpipewire-0.3-dev libclang-dev libgtk-3-dev libayatana-appindicator3-dev \
  libxdo-dev libgbm-dev libegl-dev libdbus-1-dev libdrm-dev desktop-file-utils      # nasm: x86_64 only
packaging/ffmpeg/build-linux.sh                       # optional, ~10 min: LGPL FFmpeg into packaging/ffmpeg/out-linux-<arch>
CM_FFMPEG_BUNDLE=packaging/ffmpeg/out-linux-$(uname -m) CM_REQUIRE_FFMPEG=1 packaging/linux/build-linux.sh
```

Output: `packaging/linux/out/` (git-ignored). Environment: `CM_SKIP_BUILD=1`, `CM_FFMPEG_BUNDLE`,
`CM_REQUIRE_FFMPEG=1`, `CM_LINUX_OUT`, `CM_CARGO_JOBS`, `CM_SKIP_LICENSES=1`, `CM_LINUX_ALLOW_MISSING`
(testing only). Build each architecture on its own machine (the hosted `ubuntu-24.04` and
`ubuntu-24.04-arm` runners); there is no cross-build. The speaker engine has no Linux libraries, so the
build uses `--no-default-features` and "Find speakers" answers that the build has no speaker engine.

The script refuses to package a program that links a library on a deny list (x264, x265, fdk-aac, readline,
gmp, libav*, ...), and prints the exact libraries and the glibc version the programs need.

## What the user needs (runtime)

Built on Ubuntu 24.04, so the programs need **glibc 2.39 or newer**: Ubuntu 24.04 and later, Debian 13,
Fedora 40 and later. Ubuntu 22.04 and Debian 12 are too old. Only the C library, `libgcc_s` and `libm` are linked by
`capturemann`, `cm-mcp` and `cm-overlay`. `cm-desktop` (GPUI, GTK for the tray and dialogs, PipeWire for the
portal capture, ALSA for audio) links the libraries below. This list comes from `ldd` on a debug build of
`cm-desktop` from another branch in the QEMU VM (Ubuntu 24.04 arm64); see "Known problems" for why a release
`cm-desktop` was not available in this tree.

On Ubuntu 24.04 (apt pulls the transitive ones):

```sh
sudo apt install libgtk-3-0t64 libasound2t64 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 \
  libwayland-cursor0 libwayland-egl1 libegl1 libgbm1 libdrm2 libx11-6 libxcb1 libfontconfig1 libfreetype6 \
  libssl3t64 libdbus-1-3 libpipewire-0.3-0t64 libvulkan1 mesa-vulkan-drivers
# only for the bundled FFmpeg (it links libva, which Mesa desktops already have):
sudo apt install libva2 libva-drm2
# loaded when present (tray icon, input automation, secrets, screen capture on Wayland, export):
sudo apt install libayatana-appindicator3-1 libxdo3 gnome-keyring xdg-desktop-portal ffmpeg
```

`libvulkan1` and a Vulkan driver (`mesa-vulkan-drivers`, or the vendor's) are for the GPU compositor and the
UI (software rendering through lavapipe works but is slow). A Secret Service provider (gnome-keyring or KWallet)
must be running to store tokens; without one, connecting a Google account fails cleanly. `ffmpeg` is only needed
when the tarball has none of its own (Export needs FFmpeg 5.1 or newer; Ubuntu 24.04 has 6.1).

## Licence check

* `cargo deny check` is the check for the Rust crates (CI runs it; result in the verification report).
* The shipped programs link only system libraries (`libc`, `libm`, `libgcc_s`, and for `cm-desktop` the LGPL,
  MIT or similar system libraries listed above: GTK, GLib, Cairo, Pango and the X11/Wayland libraries are LGPL or
  MIT and are dynamically linked from the user's system; none is GPL). `build-linux.sh` enforces the deny list.
* No `ffmpeg` library is linked; FFmpeg is a separate program.

## FFmpeg for Linux (LGPL)

`packaging/ffmpeg/build-linux.sh` follows `build-macos.sh`: the same pinned sources (FFmpeg 9.0.2, libvpx 1.17.0,
libopus 1.6.1, SHA-256 checked), the same component lists and the same self-check (LGPL configuration, required
encoders, decoders, filters, only `libc`, `libm`, `libz` and `libva` linked; built and self-checked on aarch64 Ubuntu 24.04 in the VM, not yet on x86_64), for the host's architecture,
into `packaging/ffmpeg/out-linux-<arch>/` with `LICENSES/`, `SOURCE-OFFER.txt` and `BUILD-INFO.txt`.
`build-linux.sh` here ships those files in `lib/capturemann/FFmpeg/`.

**The H.264 gap.** macOS uses VideoToolbox. Linux has no hardware H.264 encoder that is always present, and
no software one may ship (x264 is GPL; OpenH264 was removed from the product for the patent exposure of a
source build). The Linux build therefore enables **VAAPI** (`h264_vaapi`, `hevc_vaapi`; Intel and AMD GPUs,
through the system's `libva` and driver, `sudo apt install libva2 intel-media-va-driver` or `mesa-va-drivers`)
when `libva-dev` is installed at build time, and nothing else. On a machine without a VAAPI device MP4 (H.264)
export fails, while WebM (VP9 + Opus), GIF and audio export work. Whether the exporter in `cm-media` chooses
`h264_vaapi` and uploads frames with `hwupload` is not implemented there yet (it names `h264_videotoolbox` and
`libx264`); that is exporter work, not packaging. NVIDIA (NVENC) needs `nonfree` headers and is not shipped.

## Known problems of the first Linux builds

* `cm-desktop` did not compile in this branch: `cargo build --release -p cm-desktop` stops in `ashpd 0.13.13`
  with "You can't enable both async-io & tokio features at once". `cm-capture` asks for `ashpd/tokio` (the
  Wayland screenshot portal) while `gpui-pre-linux` pulls `oo7/async-std`, which turns on `ashpd/async-io`. Whoever
  owns `cm-capture` has to select one runtime (the `async-io` feature is the one GPUI needs). Until then no release
  tarball can contain the app; `CM_LINUX_ALLOW_MISSING=cm-desktop` only lets the packaging be tried.
* The window's `app_id` in the app is `app.capturemann.desktop` while the desktop file is `dev.capturemann.app`
  (the login item and the macOS bundle id use the latter). `StartupWMClass` is set to the former so the taskbar
  groups the window; on Wayland the shell matches the `app_id` to the desktop file name, so the app should use
  `dev.capturemann.app` as `app_id`.
* Nothing here was run on a real desktop or on x86_64 hardware; see the verification report.
