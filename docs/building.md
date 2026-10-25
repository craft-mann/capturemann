# Building from source

Needs Rust 1.90 (`rust-toolchain.toml` pins it), the Xcode command line tools on macOS and `cmake`
(the speech engine compiles whisper.cpp: `brew install cmake`). Tests that export video also need an
FFmpeg with `libx264` on `PATH` (`brew install ffmpeg`) to make their test recordings. **End users
install nothing**: the app ships its own LGPL FFmpeg.

## Check and test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
```

Hardware tests are `#[ignore]`d. They record your screen, camera or microphone, so run them only on
purpose and one at a time (commands in [`CLAUDE.md`](../CLAUDE.md)). Manual test scripts are in
[`tests/manual/`](../tests/manual); reports of runs go in [`verification/`](../verification/README.md).

## Build and run the app (macOS)

```sh
make ffmpeg          # builds the LGPL FFmpeg from pinned sources (about 10 minutes, arm64)
make bundle          # target/bundle/CaptureMann.app (CM_NO_FFMPEG=1 skips FFmpeg)
make run-bundle      # same, and opens it
make bundle-release  # same, and fails unless FFmpeg and its licence files are bundled
make licenses        # regenerates THIRD_PARTY_LICENSES.md (after Cargo.lock changes)
```

Run the app as a bundle, not as a bare binary, so macOS permissions are granted to "CaptureMann"
rather than to the terminal that launched it. Signing, the stable local identity and the notarization
TODO are in [`packaging/macos/README.md`](../packaging/macos/README.md); the FFmpeg build is in
[`packaging/ffmpeg/README.md`](../packaging/ffmpeg/README.md).

## Permissions on a development Mac

See [macOS permissions](macos-permissions.md).
