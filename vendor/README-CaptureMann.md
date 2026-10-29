# vendor/

Trimmed copies of two crates, used as path dependencies of `cm-ai` (feature `sherpa`). They are not
workspace members (`exclude = ["vendor"]` in the root `Cargo.toml`), so `clippy` and `test` do not run
on them, while `cargo fmt --all`, `cargo deny` and `make licenses` still read them.

| Directory | Upstream | What changed |
| --- | --- | --- |
| `sherpa-onnx-sys` | `sherpa-onnx-sys` 2.0.1 on crates.io (Apache-2.0, https://github.com/k2-fsa/sherpa-onnx) | `build.rs` is new: it only links the static libraries in `SHERPA_ONNX_LIB_DIR` and fails with a clear message when they are missing; no download, no unpacking. The four build-dependencies (`ureq`, `zip`, `bzip2`, `tar`) and the `shared` feature are removed. `src/` (the FFI declarations) is unchanged apart from rustfmt layout and one addition in `offline_speaker_diarization.rs`: the declaration of `SherpaOnnxOfflineSpeakerDiarizationProcessWithCallback` (already in the static library, Oct 29, 2026). |
| `sherpa-onnx` | `sherpa-onnx` 2.0.1 on crates.io (Apache-2.0) | `Cargo.toml`: the `static`/`shared` features are gone and the sys crate is a path dependency. `src/` is unchanged apart from rustfmt layout and `OfflineSpeakerDiarization::process_with_progress` in `offline_speaker_diarization.rs` (progress callback of the diarization run, Oct 29, 2026; the library ignores a request to stop, see the doc comment). |

Why: the upstream build script downloaded a prebuilt archive during `cargo build` (no checksum) and
unpacked it with crates whose licences (CDLA-Permissive-2.0, bzip2-1.0.6) the project did not want in
its dependency graph. The archive is now fetched by `packaging/sherpa/fetch-macos.sh` (`make sherpa`),
pinned by SHA-256.

`cargo fmt --all` also formats path dependencies, so the sources were laid out with the project's
rustfmt (line breaks and indentation only; upstream uses a different rustfmt configuration). Keep them
formatted, do not hand-edit them.

Both directories keep upstream's `LICENSE` (Apache-2.0). They are listed in `NOTICE` and in
`THIRD_PARTY_LICENSES.md`.

To update: copy the new version of both crates from the cargo registry, re-apply the two changes above
(the diff against the crates.io files is the table), update the archive pin in
`packaging/sherpa/fetch-macos.sh`, run `make sherpa` and
`cargo test -p cm-ai --test real_diarize -- --ignored --nocapture` (see the test's header).
