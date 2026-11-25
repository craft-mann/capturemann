# QA note: Linux final files of 0.3.7 (commit cc1f281c)

Built with `packaging/linux/release-build.sh --jobs 3` from the clean export of `cc1f281cbdffda4602aa4f70da35712f46ae4cf9` (pushed `public/main`), unsigned (the maintainer signs with minisign). Verdict PARTIAL.

| File | SHA-256 | check-package.sh |
| --- | --- | --- |
| `capturemann-0.3.7-linux-x86_64.tar.gz` | `ab3313dd113e47cd5d2ac877336149335eaa63d783af910210a42e2c2c49f1ee` | PASS |
| `capturemann_0.3.7_amd64.deb` | `ad73f24063c78d572d94b280ea2fa840401c4a8f64d3e974914e86398266c04b` | PASS |
| `capturemann-0.3.7-linux-aarch64.tar.gz` | `5b371bdc30e694fb5e4e743dd36077afcd54368bd71017be2392464430d9e32e` | PASS |
| `capturemann_0.3.7_arm64.deb` | `25997e06f037bffd381d790ec55fc209310b5ecb7eb23676902e287dad5cf91e` | PASS |

Files are in `/Users/trungtran/release-0.3.7/linux-x86_64/` and `linux-aarch64/` (with `.sha256`, `SHA256SUMS.txt`, `BUILD-INFO.txt`; no `debug/`). Hashes re-computed on the Mac after the pull match `SHA256SUMS.txt`.

## cm-regress on these files (0 failed everywhere)

- x86_64, ThinkPad T490, Ubuntu 24.04, GNOME X11, `--allow screen`, screen unlocked: standard 57 passed / 7 skipped of 64; full 65 passed / 15 skipped of 80, with `linux.deb_install` and the five recording cases (`rec.screen_cursor`, `rec.region`, `rec.region_min`, `rec.kill9_recovery`, `desktop.recording_banner`) passing. Reports `L1-linux-0.3.7-cc1f281c-regression-{standard,full}-x11-*`.
- aarch64, headless QEMU VM: standard 42 / 22 skipped of 64; full 44 / 36 skipped of 80; `.deb` install 1 of 1. Reports `L1-linux-aarch64-0.3.7-cc1f281c-regression-{standard,full,deb-install}-*`. Every desktop-session case is skipped.
- The first aarch64 build attempt failed at the llama step (a GitHub download was cancelled: curl 92); a rerun of the same command passed.

## Not run

Wayland, AMD/NVIDIA, KDE, other distributions, a second monitor, sound cases, `ai.transcribe` and `ai.speakers` (no model passed), `linux.tarball_install` in the full run on x86_64 (the run was given the `.deb`), `regress.ffmpeg_missing_wording` (the laptop has a system FFmpeg), the aarch64 desktop session, the soak cases.
