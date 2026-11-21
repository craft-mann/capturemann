# QA 0.3.6 Linux aarch64, commit 96b85253 (2026-11-21)

Machine: aarch64 Ubuntu 24.04.5 VM under QEMU/HVF on the maintainer's Mac, headless, glibc 2.39, rustc 1.99.1. Built by `packaging/linux/release-build.sh --src ~/wt/main034 --commit 96b852537cbf9aebc62efe44477fee8f56fcb99d --jobs 3` from a git bundle of main (head 96b85253). Cargo target dir was kept from earlier runs (incremental, 3m16s for the product crates; FFmpeg and llama.cpp steps ran in 2 min each, so they were mostly cached by the build scripts). The speaker model came from the mirror release over the network (sherpa step OK, no cache workaround needed).

## Files (unsigned)
- capturemann-0.3.6-linux-aarch64.tar.gz  314ed7da8cfbc45ce692a0a00354223eb4073aaf8b76d7fdd075846b42749d2d (149,740,166 bytes)
- capturemann_0.3.6_arm64.deb  8703938a0782fbb8a3a2a96b9427d7b7b921e9ede6f9462e19ac4fa3d2a0e6db (105,002,028 bytes)
Sums checked again on the Mac after copy (shasum -c SHA256SUMS.txt: both OK). Copied to /Users/trungtran/release-0.3.6/linux-aarch64/.

## Build
All steps passed. Only warning: vendor/whisper-rs-sys build.rs:137 "variable does not need to be mutable" (unused mut, harmless, in the vendored crate). check-package.sh: PASS for the tarball and PASS for the .deb (all ldd checks resolve, programs run, ffprobe, llama-server).

## cm-regress (built from the same commit; package = tarball installed with install.sh)
- standard: 42 passed, 0 failed, 22 skipped of 64
- full: 44 passed, 0 failed, 36 skipped of 80
- .deb install (linux.deb_install, run alone): 1 passed, 0 failed
Same counts as the 0.3.5 aarch64 run (42, 44). Failures: none.

## Skips
- 19 desktop-session cases (targets, shots, windows, recording on a display, overlay, etc.): headless VM.
- 2 need a whisper model / speaker models dir (not supplied; no model download).
- VA-API case (vainfo not installed); FFmpeg-absent message case (system FFmpeg cannot be hidden); tray icon, hotkey overlay, DMG, Windows silent install: not implemented for Linux or other system; the .deb case is skipped in the tarball run (it ran on its own with the .deb); confirmation-gate case needs a display.
(Full per-case list in the reports.)

## Not run
Any desktop-session behaviour on aarch64, hardware tier, camera/mic/sound, second monitor, GPU, Wayland/KDE, real whisper and speaker model checks, 3-hour soak. Files are unsigned.
