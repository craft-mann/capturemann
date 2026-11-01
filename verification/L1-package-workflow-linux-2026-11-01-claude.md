# L1 package workflow report (Linux, x86_64 and aarch64)

- **Date:** 2026-11-01
- **Who:** Claude (AI agent), nothing run by a person
- **Machine:** GitHub runners `ubuntu-24.04` and `ubuntu-24.04-arm`; the files were then installed in a `ubuntu:24.04` amd64 container (emulated on an Apple Silicon Mac)
- **Build:** `ba05504` (workflow run 41271634263, a temporary push trigger started it; the trigger is removed again in `eff55b8`), release profile, stripped
- **Status:** PARTIAL (no real Linux desktop was involved)

| Step | Result | Note |
| --- | --- | --- |
| Workflow runs on both architectures (FFmpeg and llama-server built, speaker engine linked) | PASS | both jobs green |
| Files: tarball 104.8 MB, `.deb` 81.9 MB (x86_64), plus a separate artifact with the unstripped programs | PASS | debug artifact 848 MB |
| `sha256` files match the downloads | PASS | |
| `apt install ./capturemann_0.2.5_amd64.deb` in a clean `ubuntu:24.04` amd64 container | PASS | exit 0; the programs find all their libraries |
| `capturemann --version`, bundled `ffmpeg` has a VP9 encoder, bundled `llama-server --version` | PASS | |
| Run the app, capture, edit, record on a real desktop | NOT RUN | the ThinkPad T490 test, `tests/manual/LINUX-1.md` |
