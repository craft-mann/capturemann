# Windows lab: why the installed package transcribes slowly (PARTIAL, cause not found)

Status: **PARTIAL**. Measured on the ThinkPad T490 (i7-8665U, 4 cores / 8 threads, Windows 11 Pro, Intel UHD 620, CPU only, AC power, High performance plan).
Lock `C:\lab\lab.lock` taken as `perf-agent` and released at the end. No sound, no display change, no personal data (synthetic speech
made by `win-make-speech.ps1`, 50.1 s, 116 words), `ggml-small.bin` downloaded for the run and removed afterwards, the maintainer's `ggml-medium.bin` untouched.

## Numbers (same 50.1 s clip, ggml-small unless noted)

| Program | What it runs | Wall time | CPU-seconds per second |
|---|---|---|---|
| `cm-ai__real_model.exe` (CI test build, commit 283bde5a), `transcribe()` once, 8 threads | one `whisper_full` | 34.3 s, 32.3 s, 39.7 s (RTF 0.65 to 0.79) | 8.0 |
| same, ggml-medium | one `whisper_full` | 114.2 s (RTF 2.28) | n/a |
| installed package 0.2.7 (`cm-daemon.exe` built 2026-11-05 10:32 UTC, package run 42010089424), via `capturemann import --transcribe`, 6 threads (background job) | whole job; `perf:` line: whisper 376.3 s of 376.5 s, plan 0.0, tokens 0.0 | 350 s and 376 s | 6.0 |
| same, `ai.background_priority=false` in settings.json (8 threads) | same | 389 s | n/a |
| same, daemon started by hand with `Start-Process` (not by the CLI) | same | 392 s | 7.3 |
| same, model id without a DTW preset (`small-nodtw`, a catalog override pointing at the same file) | same | 371 s | n/a |

So the package is **11x slower than the CI test build in the single `whisper_full` call itself** (376 s against 33 s), and uses about 6x more CPU time for it.
The 4x-audio-length figure of the report is confirmed (and 7 to 8x with this clip on this day).

## Ruled out (measured or read in source)

- **Thread count / background priority.** 6 vs 8 threads makes no difference (389 s with the setting off). `job_threads()` is 8 - 2 = 6 here; Windows has no thread-class lowering (`priority.rs` does nothing off macOS).
- **Windows power throttling (EcoQoS).** `GetProcessInformation(ProcessPowerThrottling)` is control 0 / state 0 for cm-daemon, cm-desktop and the test exe. Priority class Normal.
- **How the daemon is started** (CLI auto-start or `Start-Process`): same time.
- **Machine state / thermals / other work.** The test build was re-run before and after the daemon runs (34.3, 32.3, 39.7 s); CPU idle 1 to 2 % before; the desktop app was idle.
- **DTW token timestamps** (the app passes model id `small`, the test passes `real`, so only the app turns DTW on): the same model file under an id with no DTW preset took 371 s. On macOS (CPU only, medium) DTW on/off is 4.58 vs 4.40 s.
- **Decode settings.** Defaults are greedy (`best_of 1`); `ai.careful_rereads` (beam 5) is off by default; the clip is one piece (under `LONG_AUDIO_SECS`), one whisper call, no re-reads (`whisper ... (1)` in the perf line).
- **CMAKE_BUILD_TYPE.** `whisper-rs-sys` 0.15 sets `Release` explicitly in a release build (`RelWithDebInfo` only when its build script has debug assertions, which is the CI test build).
- **GGML flags in the script.** `build-windows.ps1` sets GGML_NATIVE=OFF, AVX, AVX2, FMA, F16C, BMI2 ON, AVX512 OFF before `cargo`; whisper-rs-sys forwards `GGML_*` environment variables to CMake, and ggml's MSVC branch turns `GGML_AVX2` into `/arch:AVX2` plus the FMA/F16C defines. The installed daemon was built after the portable-baseline commit a3246356 (it is an ancestor of the package run's commit 668eef82).

## What is left (not proven)

The slowdown is inside ggml/whisper.cpp code as built into the package, not in CaptureMann's Rust logic. The remaining differences between the fast test build and the slow package build are:
1. **Rust profile**: test = dev (opt-level 0), package = release (`debug = 1`); the C++ is built by CMake in both, but through `cc`/`cmake` flags derived from the profile.
2. **`-C target-feature=+crt-static`** in `RUSTFLAGS` (package only): `/MT` for everything, passed to the CMake build too.
3. **Whether the AVX2 flags really took effect in the package.** A crude instruction scan of the two executables is not conclusive (both contain AVX2/F16C code, and sherpa-onnx/onnxruntime add AVX-512 code of their own). `whisper_print_system_info()` is the decisive check, and nothing printed it.

## Source change (commit on branch `win-perf`)

- `cm_ai::whisper::system_info()` returns whisper.cpp's `AVX = 1 | AVX2 = 1 | FMA = 1 | F16C = 1 | ...` string; it is logged at info level when a model loads, and `capturemann diagnostics` has a new line `speech engine CPU features: ...`.
  After the next package build this shows in one command whether the shipped ggml has AVX2/FMA/F16C.
- Checked on macOS only: `cargo fmt --check`, `cargo check -p cm-ai -p cm-daemon --no-default-features`. The msvc target cannot be checked here (whisper-rs-sys needs the MSVC C++ toolchain). **Needs a Windows package build (CI) and a re-measure.**

## Re-measure after the next package build

1. `capturemann diagnostics` on the laptop: read `speech engine CPU features`. If `AVX2 = 0` or `FMA = 0` or `F16C = 0`: the GGML environment variables do not reach the CMake build of `cm-daemon` (fix in `build-windows.ps1`, for example pass them as `CMAKE_*`-style flags, or drop the `cargo clean -p whisper-rs-sys` and check which build script output is linked).
2. If all are 1 and it is still slow: build a package variant without `+crt-static` (`CM_WINDOWS_CRT_STATIC=0`, `-Crt md` sherpa libraries) and compare; then a variant whose whisper-rs-sys is built with `RelWithDebInfo` (the CI test build's setting).
3. Same measurement as here: 50 s synthetic clip, `RUST_LOG=debug`, read the `cm_core::perf: perf: transcribe ... whisper N s` line from `<config dir>\logs\daemon.log`; the fast reference is 33 s for ggml-small and 114 s for ggml-medium.
