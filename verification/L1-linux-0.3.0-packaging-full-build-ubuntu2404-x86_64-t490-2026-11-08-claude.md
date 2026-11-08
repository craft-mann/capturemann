# L1 Linux 0.3.0 packaging: full from-source package build on x86_64 (ThinkPad T490)

- **Date:** 2026-11-08
- **Who:** Claude (AI agent, packaging track), nothing run by a person
- **Machine:** the lab ThinkPad T490 (Intel UHD 620), Ubuntu 24.04, X11 session; `nice -n 19`, 3 jobs, shared with other agents; about 55 minutes in all
- **Build:** branch `track/linux-packaging` on top of main 45ef59b1 (package files carry the workspace version 0.2.9); run from `~/wt/pkg`, nothing installed, the lab's installed CaptureMann untouched
- **Status:** PARTIAL (one machine; no Docker; the APT flow was not run on x86_64; the app was not started with a display)

| Step | Result | Note |
| --- | --- | --- |
| `fetch-linux.sh` / `fetch-models.sh` (sherpa-onnx libs, speaker models) | PASS | pinned SHA-256 |
| `packaging/ffmpeg/build-linux.sh` on x86_64 (first time) | PASS | LGPL, VAAPI on, self-check ok |
| `packaging/llama/build-linux.sh` on x86_64 | PASS | llama.cpp v0.7.0, static, CPU |
| `build-linux.sh` with `CM_REQUIRE_SHERPA/SPEAKER_MODELS/LLAMA/FFMPEG=1` (cargo release build included) | PASS | speaker engine compiled in; tarball 140 MB, glibc 2.39 |
| `build-deb.sh` | PASS | `.deb` 106 MB, ships LICENSE |
| `check-package.sh` on tarball and `.deb` | PASS | 8 ELF files x86-64; licences; programs start; ldd resolves |
| VA-API probe inside `check-package.sh` (bundled FFmpeg, `h264_vaapi`, one 256x256 frame) | PASS | on the T490 (Intel iGPU) |
| `cm-regress --only pkg.files,pkg.startup` against the tarball's `lib/capturemann` and the `.deb` unpacked with `dpkg -x` | PASS | before the case fix the `.deb` failed "no LICENSE file" |
| `llama-server` present, runs (`--version`) and its MIT text ships | PASS | a text-model download and run was NOT done (smallest model 1.27 GB) |
