# L1 report: local AI on Linux (whisper, speaker engine, text engine build)

- **Date:** 2026-10-31
- **Who:** Claude (agent), for the maintainer
- **Machine:** QEMU VM on an Apple Silicon Mac, Ubuntu 24.04.5 aarch64, 4 CPUs (2 build jobs), 8 GB shared with other agents. No x86_64 machine, no GPU.
- **Build:** debug profile (`cargo test -p cm-ai`, `cargo build -p cm-daemon`), default features (`sherpa` on), `CARGO_INCREMENTAL=0`
- **Models and audio (not downloaded; copied from this Mac):** `ggml-medium.bin` (1.5 GB, from the maintainer's CaptureMann folder), the two speaker `.onnx` files from `packaging/sherpa/out/models`, two synthetic voices made with macOS `say` (Samantha, Daniel) joined into a 32 s clip, and the first voice alone (7.3 s). No text model (GGUF) was available or downloaded.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 whisper-rs 0.16 / whisper-rs-sys 0.15 build from source, default features | PASS | CPU only; no `cuda`, `vulkan`, `openblas` or `openmp` feature; `GGML_OPENMP=OFF` is the crate's default, so ggml uses its own thread pool |
| 2 Transcribe with `ggml-medium` on the CPU (`real_model`, `CM_WHISPER_GPU=0`, language en) | PASS | 7.3 s of audio in 12.1 s (0.60x real time, 2 test jobs sharing 4 CPUs with other agents, debug build of the Rust side); text exact: "Hello everyone, welcome to the meeting. Today we will talk about the release plan for the new screenshot tool and what is still left to do."; peak RSS 2.07 GB for the whole test run |
| 3 Cancel path of the real model (`real_model_cancel_works_and_a_normal_run_is_not_aborted`) | PASS | |
| 4 Load and refusal paths without a model (the unit tests in `cm-ai`) | PASS | part of the 245 unit tests below |
| 5 `cargo test -p cm-ai` (no ignored tests) | PASS | 245 + 14 + 6 + 22 tests, 0 failed |
| 6 Threads | PASS | `available_parallelism()` for whisper (`whisper.rs`), at most 4 for the speaker engine |
| 7 Libraries of the cm-ai test binary and of `cm-daemon` (default features) | PASS | NEEDED: libc, libm, libgcc_s, libstdc++, libssl, libcrypto, libasound, libpipewire, and for `cm-daemon` also libEGL, libgbm, libwayland, libxcb (GPUI). **No libgomp, no OpenBLAS.** glibc 2.39 needed (built on Ubuntu 24.04). libstdc++/libgcc_s are GPL-3.0 with the GCC Runtime Library Exception (system libraries, allowed); OpenSSL 3 is Apache-2.0 |
| 8 whisper.cpp with `GGML_NATIVE=OFF` (what `build-linux.sh` now exports) | PASS (configure only) | `whisper-rs-sys` rebuilt in a separate target dir: CMake cache has `GGML_NATIVE:BOOL=OFF`. Not run, and the x86_64 AVX2 flags were not built |
| 9 `packaging/sherpa/fetch-linux.sh` (aarch64) | PASS | archive `sherpa-onnx-v2.0.1-linux-aarch64-static-lib.tar.bz2`, 21,076,492 bytes, SHA-256 `b6c6090e...c031d` checked before unpacking; 11 libraries copied; BUILD-INFO and NOTICE written |
| 10 `fetch-linux.sh` for x86_64 | not run | archive `sherpa-onnx-v2.0.1-linux-x64-static-lib.tar.bz2` (22,694,241 bytes, SHA-256 `55cfadf6...70871`) was downloaded in the VM and its member list checked (same 11 libraries plus the 2 unused ones); the script path with `CM_ARCH=x86_64` was not run and nothing was linked |
| 11 Link `cm-ai` and `cm-daemon` with the `sherpa` feature (static sherpa-onnx + ONNX Runtime) | PASS | `vendor/sherpa-onnx-sys/build.rs` needed no change: it already adds `stdc++ m pthread dl` on Linux. No sherpa or ONNX Runtime `.so` in NEEDED |
| 12 Speaker engine with the real models (`real_diarize`, 2 speakers) | PASS | 32.3 s clip: 2 speakers, 4 turns, 1.7 s (RTF 0.052); 14.6 s and 15.5 s; no turn under 0.5 s; the stop-from-callback test also passed. The clip is synthetic text-to-speech, so this shows the engine runs and separates two voices, not its accuracy |
| 13 `packaging/llama/build-linux.sh` (llama.cpp v0.7.0, aarch64) | PASS | pinned tag and SHA-256 read from `build-macos.sh` (both already pinned there, not placeholders); tarball checked; 12,724,224 bytes; NEEDED: libc, libm, ld-linux only (static libstdc++/libgcc, no libgomp, no curl/OpenSSL); MIT licence file plus `vendored-LICENSE-jsonhpp.txt` kept; `--version` prints `0.7.0-dev`; the 8 options the app passes are in `--help` |
| 14 Pending pin refusal | PASS | with `LLAMA_TAG="PENDING_PIN"` in a copy of `build-macos.sh` the Linux script stops with the "not pinned yet" message, exit 1, nothing downloaded |
| 15 Engine lookup on Linux | PASS (code and unit tests) | `locate_helper` already searched `CM_LLAMA_SERVER`, the executable's folder and `PATH` on every OS; `current_exe()` resolves the install symlinks to the `lib` folder, so a helper next to the real programs is found. `available_ram_mb` reads `MemAvailable` |
| 16 Run `llama-server` with a model, text model, translation, reports | not run | no GGUF on this machine; nothing was downloaded |
| 17 Vulkan build (`CM_LLAMA_VULKAN=ON`) | not run | needs `libvulkan-dev` and `glslc`; the self-check logic for it was not exercised |
| 18 x86_64 anything | not run | no machine |
| 19 `packaging/linux/build-linux.sh` end to end with the sherpa libraries | not run | `cm-desktop` takes too long for a shared VM; the branch logic was read and `bash -n` checked. The cm-ai/cm-daemon builds above cover the sherpa link |
| 20 `cargo clippy`, `cargo fmt` | not run | the only Rust change is a comment in `engine/mod.rs` |

## For the maintainer

- Nothing needs pinning for llama.cpp: v0.7.0 and its SHA-256 are already in `packaging/llama/build-macos.sh`, and the Linux script reuses them. `crates/cm-ai/data/text_models.json` has no `pending_pin` entries.
- Nothing to download for the sherpa libraries (the script does it). ONNX Runtime's `ThirdPartyNotices.txt` is still the open item from `packaging/sherpa/README.md`.
- To run a text model on Linux: build the helper (`make llama`), set `CM_LLAMA_SERVER`, put a GGUF in the AI page, and run `tests/manual/AI-7.md` there.
- CI: the Linux job needs a `make sherpa` step before clippy, test and the package build (see the lead's note), or it keeps building `--no-default-features`.
- The tarball does not contain `llama-server` yet; `install.sh` and `build-linux.sh` would need a `CM_LLAMA_BUNDLE` step like `bundle.sh`.
- The Linux binary needs glibc 2.39 (Ubuntu 24.04 or newer) because it is built there.
