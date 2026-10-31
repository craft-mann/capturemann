# L1 report: Linux packaging (early build)

- **Date:** 2026-10-31
- **Who:** Claude (agent), for the maintainer
- **Machine:** QEMU VM on an Apple Silicon Mac, Ubuntu 24.04 aarch64, 4 CPUs, 8 GB. No x86_64 machine, no real desktop, no GPU.
- **Build:** branch `windows-step0` worktree, release profile, `--no-default-features`, `cargo build -j2` (cm-cli, cm-mcp, cm-overlay)
- **FFmpeg:** built by `packaging/ffmpeg/build-linux.sh`: FFmpeg 9.0.2 (LGPL), libvpx 1.17.0, libopus 1.6.1, VAAPI on (libva 1.20)
- **Fixture:** none; packaging only

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Build cm-cli, cm-mcp (release) | PASS | 58 s on top of the shared dependencies |
| 2 Build cm-overlay (release) | PASS | 49 s; links only libc, libm, libgcc_s |
| 3 Build cm-desktop (release, with the others) | FAIL | `ashpd 0.13.13`: "You can't enable both async-io & tokio features at once". `cm-capture` enables `ashpd/tokio`, `gpui-pre-linux` -> `oo7/async-std` enables `ashpd/async-io`. Not a packaging fault; needs a decision in `cm-capture` |
| 4 `build-linux.sh` (CM_LINUX_ALLOW_MISSING=cm-desktop) | PASS | tarball 29 MB without FFmpeg, 38 MB with; `.sha256` checks; Linux THIRD_PARTY_LICENSES generated |
| 5 `install.sh --prefix` then `capturemann --version` | PASS | prints `capturemann 0.2.5`; symlinks, `.desktop` Exec path rewritten |
| 6 `desktop-file-validate` | PASS | one hint about categories, fixed afterwards (`AudioVideo;Video;Recorder;`) |
| 7 `install.sh --uninstall` | PASS | only `mimeinfo.cache` (made by update-desktop-database) is left |
| 8 `packaging/ffmpeg/build-linux.sh` | PASS | self-check: LGPL, required encoders/decoders/filters, NEEDED = libc, libm, libz, libva, libva-drm |
| 9 FFmpeg smoke: VP9 WebM encode of 1 s; encoders list | PASS | `h264_vaapi`, `hevc_vaapi`, `libopus` present; no VAAPI device in the VM, so H.264 was not run |
| 10 Tarball with `CM_FFMPEG_BUNDLE`, `CM_REQUIRE_FFMPEG=1` | PASS | `FFmpeg/{SOURCE-OFFER.txt,BUILD-INFO.txt,LICENSES/}` inside; the installed `ffmpeg -version` runs |
| 11 `cargo deny check` (Mac, all targets, Cargo.lock as of this commit) | PASS | advisories ok, bans ok, licenses ok, sources ok |
| 12 Linked libraries of the shipped programs | PASS | capturemann, cm-mcp, cm-overlay: libc, libm, libgcc_s only; glibc 2.39 needed. `cm-desktop` (debug build from another worktree): GTK 3, GLib, Cairo, Pango, X11/xcb, Wayland, EGL, gbm, drm, ALSA, PipeWire, OpenSSL, dbus, fontconfig, freetype and their dependencies; all LGPL/MIT/BSD system libraries; no GPL |
| 13 The workflow `linux-package.yml` | not run | YAML parses (jobs: package; trigger: workflow_dispatch). Never executed on GitHub |
| 14 x86_64 build of anything | not run | no machine; `nasm` and the `x86_64-linux-gcc` libvpx target are untested |
| 15 Run `cm-desktop` from the installed tarball | not built | step 3 |

## Acceptance criteria

None of the specs has an acceptance criterion for packaging yet (see `specs/platform-acceptance.md` for the Linux plan).

## Problems found

- `cm-desktop` does not build in release on Linux in this tree (step 3). The tarball must not be published until it does.
- The window `app_id` is `app.capturemann.desktop`; the desktop file is `dev.capturemann.app` (Wayland shells match the two by name). `StartupWMClass` covers X11 only.
- The bundled FFmpeg links `libva.so.2` and `libva-drm.so.2` (hard dependency); `libva2 libva-drm2` are in the apt list.
- Export code in `cm-media` names `h264_videotoolbox` and `libx264`; it has no `h264_vaapi` path, so MP4 export on Linux needs work there.
- The `ffmpeg.org` download reset the connection once in the VM; the script now retries on every error.

## Verdict

PARTIAL. Packaging scripts, install and uninstall, LGPL FFmpeg build and licence checks work on aarch64 in a VM. Waiting on: a release `cm-desktop` (step 3), x86_64 (step 14), the CI workflow (13), a run on a real X11 and Wayland desktop (15).

## Follow-up run (Oct 31, 2026, commit f662986's parent tree, aarch64 VM)

`packaging/linux/build-linux.sh` (no bundled FFmpeg, `--no-default-features`, release) built
`capturemann-0.2.5-linux-aarch64.tar.gz`, 164 MB, now **with `cm-desktop`** (the `ashpd` runtime conflict is fixed).

| Step | Result | Notes |
| --- | --- | --- |
| `sha256sum -c` | PASS | |
| Unpack and `install.sh --prefix /tmp/pk/inst` | PASS | the notes about PATH and a missing FFmpeg print |
| `capturemann --version` | PASS | `capturemann 0.2.5` |
| `ldd cm-desktop`: unresolved libraries | PASS | none (the VM has the build libraries installed, so this does not prove the runtime package list) |
| `cm-desktop` starts on Xvfb (X11) from the installed copy | PASS | window process alive after 8 s; log: no xinput pointers (Xvfb), no DRI3 (software rendering) |
| `capturemann capture screenshot --no-start --region` against the running app | PASS | 300x200 PNG saved to the library |
| `.minisig` signing and verification with `sign-linux.sh` (throwaway key) | PASS | a changed file fails verification |

Not run: x86_64, a clean machine with only the runtime packages, a real desktop, Wayland from the installed copy.
