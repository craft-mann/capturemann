# The text engine that ships inside CaptureMann

`make llama` (`packaging/llama/build-macos.sh`) builds `llama-server` from llama.cpp (MIT) so that
the built-in text model (AI-7, provider `embedded`) needs no other program. `make bundle`
copies it into `CaptureMann.app/Contents/MacOS`, signs it with the app's identity and puts its
licence files and build record in `Contents/Resources/Llama/`.

It is a **helper program**, started by the app on `127.0.0.1` and never linked into it: whisper.cpp
(in `cm-ai`) and llama.cpp both embed ggml, and two copies in one process clash. The app finds it
the way it finds FFmpeg: `CM_LLAMA_SERVER` (a file or a folder), next to the executable, then
`PATH` (`cm_ai::text_model::engine::locate_helper`). The engine manager, its idle stop and its
failure reasons (`engine_missing`, `engine_failed`, `not_enough_memory`) are in
`crates/cm-ai/src/text_model/engine/`.

## Before the first build

`LLAMA_TAG` and `LLAMA_SHA256` at the top of the script are placeholders and the script refuses to
run while they are. Pick a release, compute `shasum -a 256` of its source tarball, write both in.
Then check the options the app passes (`engine_args` in the engine module) against
`llama-server --help` of that release; the script's self-check fails if one is missing.

## What the build guarantees

Static, Metal on, Accelerate, arm64 only (macOS 13+). `LLAMA_CURL` and `LLAMA_OPENSSL` are off, so
the server cannot download a model or call out by itself; the app gives it a local GGUF file and
nothing else. The self-check fails on any library that is not part of macOS, on curl or OpenSSL,
and on a missing licence. Output (git-ignored): `packaging/llama/out/{llama-server,BUILD-INFO.txt,LICENSES/}`.

Needs the Xcode command line tools, `cmake` (`brew install cmake`), `curl` and `shasum`; no Python.

Bundling flags (see `packaging/macos/bundle.sh`): `CM_LLAMA_BUNDLE` (a binary to bundle),
`CM_NO_LLAMA=1` (skip), `CM_REQUIRE_LLAMA=1` (fail without it; `make bundle-release` sets it).

## Windows

`packaging/llama/build-windows.sh` (an MSYS2 MINGW64 shell, `pacman -S --needed mingw-w64-x86_64-gcc
mingw-w64-x86_64-cmake mingw-w64-x86_64-ninja curl tar`) builds `llama-server.exe` from the same pinned
tag and checksum as the macOS script (it reads them from `build-macos.sh` and stops while they are
`PENDING_PIN`). CPU only (AVX2, FMA, F16C, BMI2; OpenMP off), libstdc++, libgcc and winpthread static;
`CM_LLAMA_VULKAN=ON` adds Vulkan (needs `vulkan-1.dll` at run time, untested here). The self-check fails
on any DLL outside the Windows system set, on curl or OpenSSL, and on a missing licence or option.
Output (ignored by version control): `packaging/llama/out-windows-x64/{llama-server.exe,BUILD-INFO.txt,LICENSES/}`.

`packaging/windows/build-windows.ps1` bundles that folder next to the programs when it exists
(`CM_LLAMA_BUNDLE` names another; `CM_REQUIRE_LLAMA=1` makes its absence an error) and puts the build
record and licences in `Llama\`. **Not run on Windows yet, and not in the CI workflows** (the CI job
would build llama.cpp on every package run; add it once the maintainer wants it in the zip).
