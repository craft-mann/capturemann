# sherpa-onnx libraries (speaker engine, AI-8)

`cm-ai` links sherpa-onnx (and the ONNX Runtime inside it) statically for "Find speakers". The
libraries come from the project's own release archive, pinned by SHA-256, and are fetched by one
script; the Cargo build never downloads anything.

```text
make sherpa        # = bash packaging/sherpa/fetch-macos.sh   (macOS arm64; curl, shasum, tar)
                   # on Linux: bash packaging/sherpa/fetch-linux.sh   (aarch64 or x86_64; curl, sha256sum, tar)
cargo build        # .cargo/config.toml points SHERPA_ONNX_LIB_DIR at packaging/sherpa/out/lib
```

- **Who must run `make sherpa`:** every developer once, and CI before `cargo clippy` / `cargo test` /
  `make bundle` (the `check`, `msrv` and `bundle-macos` jobs of `ci.yml` and the release job do).
  Without it the build stops in `vendor/sherpa-onnx-sys/build.rs` with a message saying so.
- **No libraries on this machine:** `cargo build --no-default-features -p cm-desktop` (and
  `-p cm-daemon`, `-p cm-ai`) builds without the `sherpa` feature. "Find speakers" then answers
  that the build has no speaker engine. `cargo test --workspace --no-default-features` runs the
  rest of the tests.
- **Another location:** set `SHERPA_ONNX_LIB_DIR=/path/to/lib` (the `lib` folder of a
  `sherpa-onnx-v2.0.1-*-static-lib` archive); an exported variable wins over `.cargo/config.toml`.
- **Offline:** put `sherpa-onnx-v2.0.1-osx-arm64-static-lib.tar.bz2` into `packaging/sherpa/.downloads/`
  (or set `CM_SHERPA_SRC_DIR`); the script checks it against the pinned SHA-256 and downloads nothing.
- **Upgrading:** change the version, URL, SHA-256 and size at the top of `fetch-macos.sh`, vendor the
  matching `sherpa-onnx` and `sherpa-onnx-sys` crates again (see `vendor/README-CaptureMann.md`), run
  `make sherpa` and the real-model test (`cargo test -p cm-ai --test real_diarize -- --ignored`).
- **Linux:** `fetch-linux.sh` pins `sherpa-onnx-v2.0.1-linux-aarch64-static-lib.tar.bz2` and
  `-linux-x64-static-lib.tar.bz2` (same libraries as macOS; `build.rs` adds `stdc++ m pthread dl`).
  See `verification/L1-local-ai-linux-2026-10-31-claude.md`.
- **Windows:** not done. Each needs its own archive name, SHA-256 and a `LIBS` check
  (the archives differ: Windows uses `.lib` files, and the dynamic C++ runtime changes).
- **Signing:** nothing new. The libraries are static; the executable that contains them is signed as
  before (no extra dylib, framework or helper).

## Licences

The archive holds only `lib/*.a`. sherpa-onnx is Apache-2.0 (text in `vendor/sherpa-onnx/LICENSE`, listed
in `NOTICE` and `THIRD_PARTY_LICENSES.md`); ONNX Runtime is MIT (`licenses/onnxruntime-MIT.txt`, also in
`THIRD_PARTY_LICENSES.md`). ONNX Runtime's `ThirdPartyNotices.txt` (v1.28.2, the version string inside both
the macOS and the Linux aarch64 `libonnxruntime.a`) is `licenses/ThirdPartyNotices-onnxruntime.txt`; the
generator (`packaging/macos/gen-third-party.sh`, also used for the Linux tarball) adds it to the generated
licence file. The Linux tarball and `.deb` also carry `share/doc/capturemann/SHERPA-ONNX/` (the notes, the two
ONNX Runtime texts and the fetch record). The x86_64 archive's ONNX Runtime version was not inspected.

## Speaker models (shipped inside the app)

`make sherpa` also runs `fetch-models.sh`: it fetches (curl, cached in `.downloads/`, SHA-256 and size
checked, offline when the files are cached) the pyannote segmentation-3.0 archive and the CAM++ English
model, unpacks the archive with the system `tar` (build time only), and writes
`out/models/{pyannote-segmentation-3-0.onnx,campplus-sv-en-voxceleb-16k.onnx,BUILD-INFO.txt,LICENSES/}`.
`packaging/macos/bundle.sh` copies the two files into `Contents/Resources/Models/speaker/`
(`CM_NO_SPEAKER_MODELS=1` skips; `make bundle-release` requires them). For a dev run of `cargo run`
set `CM_SPEAKER_MODELS_DIR=packaging/sherpa/out/models`. The licence texts also live in
`models-licenses/` (read by `make licenses`). Changing a model: update the pins in `fetch-models.sh`
and `crates/cm-ai/data/speaker_models.json` together.
