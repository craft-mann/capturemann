# L1 report: Linux package completed (speaker engine, llama-server, .deb)

- **Date:** 2026-10-31
- **Who:** Claude (agent), for the maintainer
- **Machine:** QEMU VM on an Apple Silicon Mac, Ubuntu 24.04.5 aarch64, 4 CPUs (2 build jobs, shared with other agents). The `.deb` was installed in a clean `ubuntu:24.04` linux/arm64 container (Docker Desktop on the Mac, `--memory 3g`). **No x86_64 machine: nothing here ran on x86_64.** No real desktop, no GPU.
- **Tree:** branch `worktree-agent-a0ae999814913cd00`, commits `9a9de87` (speaker engine), `9187394` (llama-server), `185a418` (.deb), plus the documentation commit that holds this report. Release profile, default features (`sherpa` on), `cargo build --release --locked -j2`, `GGML_NATIVE=OFF`. The target directory was seeded with the release directory of another tree, so `whisper-rs-sys` was **not** rebuilt with `GGML_NATIVE=OFF` (the CMake flag only applies to a fresh build; the tarball is for testing the packaging, not a release candidate).
- **Inputs:** sherpa-onnx v2.0.1 aarch64 archive and the two speaker models (`make sherpa` steps, SHA-256 checked by the scripts); llama.cpp v0.7.0 built by `packaging/llama/build-linux.sh` (12,724,224 bytes); FFmpeg 9.0.2 LGPL built by `packaging/ffmpeg/build-linux.sh` in the VM (VAAPI on).
- **Outputs tested:** `capturemann-0.2.5-linux-aarch64.tar.gz` (228,257,219 bytes, sha256 `918e60304fcfbbb4779c641d9947dc97fdbcec67a776fc45001a07596b399f90`) and `capturemann_0.2.5_arm64.deb` (155,482,732 bytes, xz, sha256 `cceb9f3637bd58789dde23a3ea9a4e06dd6ce4cdc0e302ba12ea90a0c2346a8b`, 939,632 KiB installed). `cm-desktop` is about 700 MB because the release profile keeps debug info.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 `fetch-models.sh` on Linux (it was macOS-only, so `make sherpa` would have failed on Linux after the libraries) | PASS | now uses `sha256sum` or `shasum`; downloaded both models in the VM, pins matched; re-run on macOS from the cached files, also PASS |
| 2 Release build of `cm-desktop`, `cm-overlay`, `capturemann`, `cm-mcp` with the sherpa libraries | PASS | 3 m 44 s on a seeded target dir |
| 3 No sherpa or ONNX Runtime `.so` in NEEDED of the programs | PASS | `objdump -p cm-desktop`: no `sherpa`/`onnx` entry; the deny list in `build-linux.sh` now also rejects `libsherpa` and `libonnxruntime` |
| 4 "Find speakers" path compiled in | PASS | `cm-desktop` contains `SherpaOnnxCreateOfflineSpeakerDiarization` and not the "has no speaker engine" text (the no-sherpa binary of the other tree is the reverse); `build-linux.sh` now checks this and stops on a mismatch; the installed app's diagnostics print `speaker engine built in: true` and list both speaker models |
| 5 Licences: ONNX Runtime `ThirdPartyNotices.txt` | PASS | it was already in `packaging/sherpa/licenses/` and in the generated `THIRD_PARTY_LICENSES.md` (the "still missing" notes were stale); the Linux `libonnxruntime.a` holds version 1.28.2, the version of that file. The tarball and `.deb` also have `share/doc/capturemann/SHERPA-ONNX/` (notes, fetch record, MIT text, notices). The x86_64 library's ONNX Runtime version was not inspected |
| 6 `build-linux.sh` with `CM_REQUIRE_SHERPA`, `CM_REQUIRE_SPEAKER_MODELS`, `CM_REQUIRE_LLAMA`, `CM_REQUIRE_FFMPEG` | PASS | Linux `THIRD_PARTY_LICENSES.md` generated (no fallback warning); `lib/Resources/Models/speaker/`, `lib/capturemann/{llama-server,Llama/,ffmpeg,ffprobe,FFmpeg/}`, `SHERPA-ONNX/` all in the tarball |
| 7 `CM_REQUIRE_LLAMA=1` without a bundle | PASS (refused) | message names `packaging/llama/build-linux.sh` |
| 8 A dynamic program given as `CM_LLAMA_BUNDLE` (`/bin/ls`) | PASS (refused) | "needs a library outside the allowed list: libselinux.so.1" |
| 9 `llama-server` library check | PASS | NEEDED: `ld-linux-aarch64.so.1`, `libc.so.6`, `libm.so.6`; MIT licence file and the vendored `jsonhpp` licence present; `--version` prints `0.7.0-dev` in the container |
| 10 `install.sh` into a prefix | PASS | models and `Llama/` copied (checked `lib/Resources/Models/speaker`, `lib/capturemann/Llama`); the run was used for step 11 |
| 10b `install.sh --uninstall` after the change | FAIL, fixed, **not re-run** | the first run left `share/applications/dev.capturemann.app.desktop` behind (my edit had moved that path onto the wrong line); the line was corrected afterwards but the uninstall was not run again |
| 11 Installed `cm-desktop` under Xvfb, then `capturemann diagnostics` | PASS | `speech model speaker-segmentation (6 MB)`, `speaker-embedding (29 MB)`, `llama-server at <prefix>/lib/capturemann/llama-server`, `speaker engine built in: true`. This shows the models and the helper are **found**; it does not run a speaker or text job |
| 12 Speaker job with the packaged models, text model with `llama-server` | not run | the speaker engine was run with real models in `verification/L1-local-ai-linux-2026-10-31-claude.md` (steps 11, 12); no GGUF text model was available and none was downloaded |
| 13 `build-deb.sh` from the tarball | PASS | xz, about 4 min on the shared VM; `Depends:` has `libc6 (>= 2.39)` read from the build, the verified library list, and `libva2, libva-drm2` because FFmpeg is bundled; `Recommends:` gnome-keyring or kwalletmanager, xdg-desktop-portal, pipewire |
| 14 `lintian` | reported, 4 kinds left | `dir-or-file-in-opt` (by design), `unstripped-binary-or-object` (debug info; `CM_DEB_STRIP=1` exists, not tested), `embedded-library` libavfilter/libavutil/libswscale (the bundled LGPL FFmpeg), `no-manual-page`. The Markdown change log first produced about 260 format warnings and a missing-changelog error: fixed by shipping a Debian-format `changelog.gz` plus `CHANGELOG.md.gz`, and a copyright year |
| 15 Clean container, `apt install ./capturemann_0.2.5_arm64.deb` **with Recommends** (first pass, a deb without FFmpeg and without the final changelog) | PASS | 369 packages set up (gnome-keyring, xdg-desktop-portal, pipewire, ffmpeg come in); `capturemann --version` = `capturemann 0.2.5`; all programs `0 not found`; `cm-desktop` alive under Xvfb; a region screenshot (300x200) was taken; `apt remove` and `purge` clean |
| 16 Clean container, `apt install --no-install-recommends ./capturemann_0.2.5_arm64.deb` (final deb) | PASS | 135 packages set up from `Depends:` alone; ldd of `cm-desktop`, `cm-overlay`, `capturemann`, `cm-mcp`, `ffmpeg`, `ffprobe`, `llama-server`: `0 not found`; `capturemann --version` ok; bundled `ffmpeg -version` = FFmpeg 9.0.2; `llama-server --version` ok; `/usr/bin/*` symlinks and `Exec=/usr/bin/cm-desktop` right; `cm-desktop` alive after 15 s under Xvfb; diagnostics list both speaker models, the bundled ffmpeg, the bundled `llama-server`, `speaker engine built in: true`; a region screenshot was taken |
| 17 `apt remove capturemann`, then `apt purge` | PASS | after remove: 0 package files left, `/opt/capturemann` gone, only dpkg's own `capturemann.list` and `.postrm` and the `rc` status; after purge: `dpkg-query` finds nothing and no path with `capturemann` in its name remains outside `/tmp`. The maintainer scripts ran with neither `update-desktop-database` nor `gtk-update-icon-cache` installed (they skip), so the cache refresh itself was **not** exercised |
| 18 Install on a real Ubuntu or Debian desktop, menu entry, icon cache | not run | no desktop |
| 19 Debian 13 (`.deb` Depends names) | not run | the names are those of Ubuntu 24.04; Debian 13 is expected to match (same t64 names) but was not tried |
| 20 `sign-linux.sh` with the `.deb` pattern | not run | `minisign` is not installed here; the change is a glob and `bash -n` passes |
| 21 `linux-package.yml` (new `make sherpa`, llama, `.deb` steps) | not run | YAML parses; never executed on GitHub, no workflow was triggered |
| 22 Anything on x86_64 | not run | no machine: `fetch-linux.sh` x86_64 path, FFmpeg/llama/whisper builds with AVX2 flags, the x86_64 `.deb` |
| 23 `CM_DEB_STRIP=1`, `CM_DEB_COMPRESS=zstd` | not run | `gzip -1` and `xz -6` were run |

## Problems found

- `make sherpa` could not work on Linux: `fetch-models.sh` stopped with "this script only runs on macOS" (the Ubuntu CI job in `ci.yml` calls `make sherpa` and would have failed there; fixed in `packaging/sherpa/fetch-models.sh`, `ci.yml` untouched).
- `cm-ai` looks for the bundled speaker models in `<executable dir>/../Resources/Models/speaker`, a macOS bundle layout. On Linux that is `lib/Resources/Models/speaker` beside `lib/capturemann`, which is why the packages (and the `.deb` under `/opt/capturemann`) have that odd path. A Linux lookup in `crates/cm-ai` would let the `.deb` use `/usr/lib` and `/usr/share`.
- The programs are about 700 MB (debug info); tarball 228 MB, `.deb` 155 MB, 940 MB installed.
- The default `Maintainer:` is a placeholder (`CM_DEB_MAINTAINER`).
- The disk of the VM went down to 5.3 GB free while the seeded target dir (6.7 GB) existed; I deleted the build directories before packaging. Everything under `/home/dev/cm-pkg2*` and the temporary build trees is removed; `lintian` is left installed in the VM. The container (`--rm`) and the `ubuntu:24.04` image I pulled were removed.

## Verdict

PARTIAL. On aarch64 the tarball is complete (speaker engine with models and licences, `llama-server`, FFmpeg) and the `.deb` installs from `Depends:` alone on a clean Ubuntu 24.04, runs, and removes cleanly. Not shown: x86_64, a real desktop, the GitHub workflow, a speaker or text job from the packaged files, signing.
