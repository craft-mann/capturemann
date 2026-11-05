# Windows lab: why the installed package transcribes slowly, round 2 (PARTIAL: cause found in source, fix not yet rebuilt or re-measured)

Status: **PARTIAL**. ThinkPad T490 (i7-8665U, Windows 11), lock `C:\lab\lab.lock` taken as `perf2-agent` and released at the end.
Package: CI run 42061424705 (commit bfffe35e), installed silently over the laptop's install. Synthetic speech, no personal data.
The run used a second daemon (own config, library and pipe under `C:\lab\perf2`, removed afterwards); the maintainer's cm-desktop was left running.

## Measured

- `speech engine CPU features: WHISPER : COREML = 0 | OPENVINO = 0 | CPU : SSE3 = 1 | SSSE3 = 1 | AVX = 1 | AVX2 = 1 | F16C = 1 | FMA = 1 | BMI2 = 1 | REPACK = 1 |`
  So the GGML environment flags DO reach CMake (AVX2, FMA, F16C, BMI2 on). Hypothesis 1 of the earlier report is ruled out.
- ggml-small, 50 s clip, ai job through `capturemann import --transcribe`, RUST_LOG=debug:
  `perf: transcribe: 303.7 s wall, peak 0.9 GB; plan 0.0 s (1), whisper 303.6 s (1), tokens 0.0 s (1)`, about 5.8 cores busy (CPU 1093 s at 189 s).
  CI test build on the same laptop: 32 to 40 s. Still about 9x slower.
- The `capturemann diagnostics` call must reach a daemon of the new build: the maintainer's running cm-desktop embeds the old daemon (its exe was locked and
  not replaced by the installer), so the line only showed with a second daemon started from the new `cm-daemon.exe`.

## Cause (best hypothesis, from the source of the crates; the CI log has no CMake output to confirm it)

The C and C++ of ggml is compiled with **no optimisation** in the package build (no `/O2`, no `/DNDEBUG`), but optimised in the test build.

1. `whisper-rs-sys` 0.15 `build.rs` sets `config.profile("Release")` (so cmake-rs runs `cmake --build --config Release`) and defines
   `CMAKE_BUILD_TYPE` = `Release` in a release build, `RelWithDebInfo` when its build script has debug assertions (the dev profile of the test build).
2. `cmake` 0.1.58 `lib.rs` (about lines 745 to 775): with the Visual Studio generator (the default on MSVC; no `CMAKE_GENERATOR` is set in either workflow)
   it defines `CMAKE_C_FLAGS_<UPPER(CMAKE_BUILD_TYPE)>` and `CMAKE_CXX_FLAGS_...` as the cc-rs flags **minus every `-O`/`/O` flag**, unless that variable is already defined.
   That replaces CMake's default `/O2 /Ob2 /DNDEBUG`.
3. Package build: build type `Release` and config `Release` agree, so the Release flags lose `/O2`: unoptimised ggml (intrinsics not inlined, ~10x slower). `+crt-static` only adds `-MT`.
4. Test build: the overridden variable is `..._RELWITHDEBINFO`, a configuration that is never built (config stays `Release`), so the real Release flags keep `/O2`. The fast test build is fast by accident.

This fits the numbers: all cores busy, same instructions available, ~9 to 11x more time, no Rust-side cause (threads, priority, DTW, decode all ruled out in the first report).

## Fix (commit on branch `win-perf2`, not pushed)

`packaging/windows/build-windows.ps1` sets `CMAKE_C_FLAGS_RELEASE` and `CMAKE_CXX_FLAGS_RELEASE` to `/O2 /Ob2 /DNDEBUG /MT` (`/MD` with `CM_WINDOWS_CRT_STATIC=0`) before cargo;
whisper-rs-sys forwards `CMAKE_*` environment variables as defines and cmake-rs then leaves the variables alone. The existing `cargo clean -p whisper-rs-sys` forces the rebuild.
Not checked here (no MSVC): the script only parses by reading.

## Must be rebuilt and re-measured

1. Windows package build (CI, `windows-package.yml`) from the fix commit. Confirm it from the build output if possible (whisper-rs-sys CMake output with `cargo build -vv`, look for `/O2` in the ggml compile lines).
2. Same 50 s clip with ggml-small, RUST_LOG=debug, `perf: transcribe ... whisper N s`: expected about 33 to 40 s. If still slow, next suspects in order: `+crt-static` (`CM_WINDOWS_CRT_STATIC=0`), then the sherpa/onnxruntime libraries sharing the process.
3. llama-server is built separately (MSYS2, `-DCMAKE_BUILD_TYPE=Release`, Ninja-like single config), not affected.
