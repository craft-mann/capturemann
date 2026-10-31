# L1 report: Linux package on a clean Ubuntu 24.04 (runtime dependency list)

- **Date:** 2026-10-31
- **Who:** Claude (agent), for the maintainer
- **Machine:** Docker Desktop on an Apple Silicon Mac, `docker run --platform linux/arm64 ubuntu:24.04` (a fresh container each time, `--no-install-recommends`); not the QEMU VM, which has the build libraries installed.
- **Package under test:** `capturemann-0.2.5-linux-aarch64.tar.gz` (164 MB, built in the VM, release, `--no-default-features`, no bundled FFmpeg)
- **Fixture:** none; the screenshot is a 300x200 region of an Xvfb screen

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Install only the "runtime" apt list of `packaging/linux/README.md` (required line + `libva2 libva-drm2` + the old optional `libayatana-appindicator3-1 libxdo3`), unpack, `install.sh` | PASS | apt exit 0 |
| 2 `ldd` of cm-desktop, cm-overlay, capturemann, cm-mcp: "not found" | PASS | 0 for all four |
| 3 Same with ONLY the required line (no libva, no optional line) | PASS | 0 "not found" for all four, so the list is sufficient; `libva*` is only for the bundled FFmpeg |
| 4 `capturemann --version` | PASS | `capturemann 0.2.5` |
| 5 `cm-desktop` under Xvfb (added `xvfb xauth dbus dbus-x11` for the test only), `dbus-run-session`, 12 s | PASS | process alive. Log: no xinput pointers, no DRI3 (software EGL), no portal and no a11y bus in the container (warnings) |
| 6 `capturemann capture screenshot --no-start --region 0,0,300,200` against it | PASS | `Captured shot_... (300x200 px, region)`, bundle in `~/Videos/CaptureMann` |
| 7 Bare `ubuntu:24.04` (nothing installed): `install.sh` | PASS | installs, then prints each missing library with its apt package and one `apt install` line (new `check-runtime.sh`); exit 0 |
| 8 Run the printed `apt install` line, then `check-runtime.sh` again | PASS | "All libraries the programs link are present"; the script only notes that `libvulkan.so.1` is absent (loaded at run time, not by `ldd`) |
| 9 `dpkg -S` of every soname in the script's table, in the container | PASS | all 31 sonames resolve to the package named in the script (`libglib2.0-0t64`, `libssl3t64`, `libgtk-3-0t64`, ...) |

## Findings and changes

- The README list had no missing library. Two entries were wrong in the other direction: `libayatana-appindicator3-1` and `libxdo3` are not in the DT_NEEDED list of `cm-desktop` and no `dlopen` string refers to them (`strings`), so they were removed from the README ("loaded when present" was a guess). `cm-desktop` NEEDED: freetype, xkbcommon, xkbcommon-x11, glib/gobject/gio, gtk-3, gdk-3, pango, cairo, cairo-gobject, gdk-pixbuf, stdc++, asound, ssl, crypto, EGL, gbm, xcb, pipewire, gcc_s, libm, libc.
- `libgtk-3-0t64` pulls Wayland, X11, fontconfig, drm, dbus, cairo through apt; the shorter list in `check-runtime.sh` shows what `ldd` alone needs.
- New `packaging/linux/check-runtime.sh` (also shipped in the tarball by `build-linux.sh`, run at the end of `install.sh`): `ldd` on every program, a soname-to-package table made by `dpkg -S`, `apt-file` as a fallback for sonames not in the table.
- `libvulkan1` and `mesa-vulkan-drivers` were installed in all runs; whether the app starts without them was not tested (it rendered through software EGL under Xvfb). They stay in the list.

## Not run

x86_64, a real desktop, Wayland, Fedora/Debian, glibc older than 2.39 (expected to fail), the tarball rebuilt with `check-runtime.sh` inside (the container used the script from the work tree next to the old tarball).

## Verdict

PASS for the stated scope (Ubuntu 24.04 aarch64, clean container): the required list is complete and the app starts and serves a screenshot with only those packages.
