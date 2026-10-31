# Linux packaging (early build)

`packaging/linux/build-linux.sh` builds `capturemann-<version>-linux-<arch>.tar.gz` (and a `.sha256`), and
`packaging/linux/build-deb.sh` turns that tarball into `capturemann_<version>_<amd64|arm64>.deb` (see "The .deb"), for
`x86_64` and `aarch64`, from `cargo build --release` (with the speaker engine when `make sherpa` has run, else `--no-default-features`). The workflow
`.github/workflows/linux-package.yml` runs it by hand on both architectures. Status of what was run and where:
`verification/L1-packaging-linux-2026-10-31-claude.md`.

## Decision: a tarball with an install script (v0)

| Option | For | Against | Verdict |
| --- | --- | --- | --- |
| **tar.gz + `install.sh` + `.desktop` + icons** | One file for every distribution and both CPUs; no root (`~/.local`); trivial to build in CI and to inspect; same shape as the Intel Mac "early build"; the app finds `ffmpeg` next to its own executable | The user installs the system libraries; no automatic updates; no menu entry until `install.sh` ran | **Primary for v0** |
| `.deb` | Declares dependencies, `apt install ./file.deb` pulls them; removable | Ubuntu/Debian only, one build per release of the distribution (t64 library names differ between 22.04 and 24.04), root needed, signing and a repository later | **Built** (`build-deb.sh`, from the tarball, with `dpkg-deb`); the dependency list is the one proved on a clean container, not yet on real machines |
| AppImage | One file, runs anywhere | Bundling GTK, Mesa, Vulkan, PipeWire and portal libraries is fragile for a GPU app that talks to the desktop portal and the Secret Service; needs FUSE 2 (not on Ubuntu 24.04 by default); a bundled FFmpeg and libva clash with the host drivers | Not for v0 |
| Flatpak | Sandbox with portals, store | Recording, global shortcuts, the CLI/MCP binaries on the host `PATH` and the Unix socket all need holes in the sandbox; manifest, runtime and review work | Later (the portal code paths are already the right ones for it) |

Reasons in short: the product is a desktop app plus a CLI and an MCP server that agents start from a shell,
so unpacking into a normal prefix with symlinks in `bin/` suits it better than a sandbox; and there are no
users yet who would be hurt by installing five apt packages by hand.

## The .deb

```sh
packaging/linux/build-linux.sh                         # first: the .deb is made from this tarball
packaging/linux/build-deb.sh                           # newest capturemann-*-linux-$(uname -m).tar.gz in packaging/linux/out
sudo apt install ./capturemann_<version>_<amd64|arm64>.deb      # pulls the libraries; remove: sudo apt remove capturemann
```

* Layout: the programs and everything next to them (`ffmpeg`, `llama-server`, the licence folders, the speaker models
  under `lib/Resources/Models/speaker`) are in `/opt/capturemann/lib/...`, exactly as in the tarball, because the
  app finds FFmpeg, `llama-server` and the models relative to its own real path (symlinks are resolved). `/usr/bin/{cm-desktop,cm-overlay,capturemann,cm-mcp}`
  are symlinks to them; the `.desktop` file (with `Exec=/usr/bin/cm-desktop`) and the icons are in `/usr/share`;
  documents and the `copyright` file (DEP-5; Apache-2.0 plus the bundled components) in `/usr/share/doc/capturemann`.
* `Depends:` is the verified list under "What the user needs" (`libc6 (>= 2.39)` read from the build, GTK 3, ALSA,
  xkbcommon, Wayland, EGL/gbm/drm, X11/xcb, fontconfig/freetype, OpenSSL 3, D-Bus, PipeWire, Vulkan loader and
  `mesa-vulkan-drivers | vulkan-icd`, plus `libva2 libva-drm2` when FFmpeg is bundled). `Recommends:` gnome-keyring or
  kwalletmanager (a Secret Service), xdg-desktop-portal, pipewire, and `ffmpeg` when none is bundled.
* Maintainer scripts: `postinst` and `postrm` only refresh the desktop-entry and icon caches when the tools exist.
  The user's library and settings are never touched, also not by `purge`.
* Architecture comes from `uname -m` of the tarball (`x86_64` -> `amd64`, `aarch64` -> `arm64`); the package names are
  those of Ubuntu 24.04 and Debian 13, so build and use it there. `CM_DEB_COMPRESS` (xz, gzip, zstd), `CM_DEB_LEVEL`,
  `CM_DEB_MAINTAINER` and `CM_DEB_STRIP=1` (strip the debug info of the four programs: the tarball's `cm-desktop` is
  about 700 MB because the release profile keeps it) are the knobs; see the script header.
* `lintian` on it reports `dir-or-file-in-opt` (by design: an add-on under `/opt`), `unstripped-binary-or-object`
  (see `CM_DEB_STRIP`), `embedded-library` for the bundled LGPL FFmpeg and `no-manual-page`. It is not meant for the
  Debian archive.
* Proof: `verification/L1-packaging-complete-linux-2026-10-31-claude.md` (clean `ubuntu:24.04` arm64 container:
  `apt install ./file.deb`, run, `apt remove`, `apt purge`; x86_64 not run).

## Verifying a download

Each release file (tarball or `.deb`) has a `.minisig` signature made with the project's minisign key
(`packaging/linux/minisign.pub`; the same public key is on the release page). Check both the checksum and the signature:

```sh
sha256sum -c capturemann-<version>-linux-<arch>.tar.gz.sha256      # or capturemann_<version>_<arch>.deb.sha256
minisign -Vm capturemann-<version>-linux-<arch>.tar.gz -p minisign.pub
```

The maintainer signs with `packaging/linux/sign-linux.sh` (see its header for the key setup).

## What is in the tarball

```
capturemann-<version>-linux-<arch>/
  install.sh                          --prefix P (default ~/.local), --uninstall
  README.txt
  lib/capturemann/{cm-desktop,cm-overlay,capturemann,cm-mcp[,ffmpeg,ffprobe,FFmpeg/]}
  lib/capturemann/llama-server        [+ Llama/{BUILD-INFO.txt,LICENSES/}] the built-in text engine (MIT), with CM_LLAMA_BUNDLE
  lib/Resources/Models/speaker/       the two speaker models + LICENSES/ (cm-ai reads <executable dir>/../Resources/Models/speaker)
  share/applications/dev.capturemann.app.desktop
  share/icons/hicolor/{16..512}x.../apps/dev.capturemann.app.png, scalable/apps/dev.capturemann.app.svg
  share/doc/capturemann/{LICENSE,NOTICE,THIRD_PARTY_LICENSES.md,CHANGELOG.md,LIBRARIES.txt}
  share/doc/capturemann/SHERPA-ONNX/  notes, fetch record, ONNX Runtime MIT text and ThirdPartyNotices (speaker engine builds)
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
make llama                                            # optional, ~5 min: llama-server (MIT, static, CPU) into packaging/llama/out
make sherpa                                           # the speaker engine: sherpa-onnx static libraries + the two models (SHA-256 pinned)
packaging/ffmpeg/build-linux.sh                       # optional, ~10 min: LGPL FFmpeg into packaging/ffmpeg/out-linux-<arch>
CM_REQUIRE_SHERPA=1 CM_REQUIRE_SPEAKER_MODELS=1 CM_LLAMA_BUNDLE=packaging/llama/out CM_REQUIRE_LLAMA=1 \
CM_FFMPEG_BUNDLE=packaging/ffmpeg/out-linux-$(uname -m) CM_REQUIRE_FFMPEG=1 packaging/linux/build-linux.sh
```

Output: `packaging/linux/out/` (git-ignored). Environment: `CM_SKIP_BUILD=1`, `CM_FFMPEG_BUNDLE`,
`CM_REQUIRE_FFMPEG=1`, `CM_REQUIRE_SHERPA=1`, `CM_REQUIRE_SPEAKER_MODELS=1`, `CM_SPEAKER_MODELS_BUNDLE`, `CM_LLAMA_BUNDLE`, `CM_REQUIRE_LLAMA=1`, `CM_LINUX_OUT`, `CM_CARGO_JOBS`, `CM_SKIP_LICENSES=1`, `CM_LINUX_ALLOW_MISSING`
(testing only). Build each architecture on its own machine (the hosted `ubuntu-24.04` and
`ubuntu-24.04-arm` runners); there is no cross-build. The speaker engine (sherpa-onnx with
ONNX Runtime, static, so no extra `.so`) is built in when `packaging/sherpa/out/lib` exists (`make sherpa`); without it
the build uses `--no-default-features` and "Find speakers" answers that the build has no speaker engine
(`CM_REQUIRE_SHERPA=1` turns that into an error; the workflow sets it). After the build the script checks that
`cm-desktop` really contains the sherpa-onnx C API (or really does not), so a stale binary cannot slip in.
The text engine (AI-7) is `llama-server` from `packaging/llama/build-linux.sh` (llama.cpp, MIT; the tag and SHA-256 are
read from `build-macos.sh`, so both platforms run the same version). `CM_LLAMA_BUNDLE` takes its output folder (or the
file); the script refuses it unless it needs only libc, libm and the loader (no libgomp, curl or OpenSSL, so the
helper cannot reach the network by itself), copies it to `lib/capturemann/` where the engine looks for it (search
order: `CM_LLAMA_SERVER`, the folder of the executable, `PATH`) and puts its licence files in `lib/capturemann/Llama/`.
The same deny-list check as for the other programs runs on it. Without the bundle the built-in text model answers
"engine missing" unless the user has `llama-server` on `PATH`; no model file ships (the user downloads one).

Licences: the sherpa-onnx (Apache-2.0) and ONNX Runtime (MIT, plus its `ThirdPartyNotices.txt`) texts are in
`THIRD_PARTY_LICENSES.md` and again, with the fetch record, in `share/doc/capturemann/SHERPA-ONNX/`.

The script refuses to package a program that links a library on a deny list (x264, x265, fdk-aac, readline,
gmp, libav*, ...), and prints the exact libraries and the glibc version the programs need.

## What the user needs (runtime)

Built on Ubuntu 24.04, so the programs need **glibc 2.39 or newer**: Ubuntu 24.04 and later, Debian 13,
Fedora 40 and later. Ubuntu 22.04 and Debian 12 are too old. Only the C library, `libgcc_s` and `libm` are linked by
`capturemann`, `cm-mcp` and `cm-overlay`. `cm-desktop` (GPUI, GTK for dialogs, PipeWire for the portal capture,
ALSA for audio) links the libraries below. The list was proved on a clean `ubuntu:24.04` arm64 container with
only these packages (`--no-install-recommends`): every program resolves all its libraries and `cm-desktop` ran
under Xvfb and served a CLI screenshot (`verification/L1-packaging-clean-linux-2026-10-31-claude.md`).

On Ubuntu 24.04 (apt pulls the transitive ones):

```sh
sudo apt install libgtk-3-0t64 libasound2t64 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 \
  libwayland-cursor0 libwayland-egl1 libegl1 libgbm1 libdrm2 libx11-6 libxcb1 libfontconfig1 libfreetype6 \
  libssl3t64 libdbus-1-3 libpipewire-0.3-0t64 libvulkan1 mesa-vulkan-drivers
# only for the bundled FFmpeg (it links libva, which Mesa desktops already have):
sudo apt install libva2 libva-drm2
# optional: secrets, screen capture on Wayland, export without a bundled FFmpeg
sudo apt install gnome-keyring xdg-desktop-portal ffmpeg tesseract-ocr   # tesseract-ocr: Auto-redact (a helper program; without it the button is dimmed)
```

`./check-runtime.sh` (in the tarball; `packaging/linux/check-runtime.sh` in the repository) runs `ldd` on every
program, prints each missing library with the apt package that provides it and the `apt install` line, and
`install.sh` runs it at the end. `libayatana-appindicator3` and `libxdo3` are **not** needed: the build neither links
nor loads them (the Linux tray icon is a StatusNotifierItem over D-Bus).

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
export fails, while WebM (VP9 + Opus), GIF and audio export work. The bundled FFmpeg also has the native `vp9` decoder, so VP9-in-MP4 recordings (the software fallback of the Linux
recorder) can be edited and exported. The `cm-media` exporter probes the FFmpeg it finds and picks `h264_vaapi`
(when a VAAPI device exists) or VP9 (`libvpx-vp9`); it never uses `libx264`. NVIDIA (NVENC) needs `nonfree` headers and is not shipped.

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
