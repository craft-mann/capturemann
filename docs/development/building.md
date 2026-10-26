# Building from source

Needs Rust 1.95 or newer (the oldest compiler the GPUI dependency builds with) (`rust-version` in `Cargo.toml`; `rust-toolchain.toml` selects the current stable toolchain), the Xcode command line tools on macOS and `cmake`
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
purpose and one at a time (commands in [`CLAUDE.md`](../../CLAUDE.md)). Manual test scripts are in
[`tests/manual/`](../../tests/manual); reports of runs go in [`verification/`](../../verification/README.md).

## Build and run the app (macOS)

```sh
make ffmpeg          # builds the LGPL FFmpeg from pinned sources (about 10 minutes, arm64)
make bundle          # target/bundle/CaptureMann.app (CM_NO_FFMPEG=1 skips FFmpeg)
make run-bundle      # same, and opens it
make bundle-dev      # same as bundle, under the bundle id dev.capturemann.app.dev (see below)
make bundle-release  # same, and fails unless FFmpeg and its licence files are bundled
make licenses        # regenerates THIRD_PARTY_LICENSES.md (after Cargo.lock changes)
```

Run the app as a bundle, not as a bare binary, so macOS permissions are granted to "CaptureMann"
rather than to the terminal that launched it. Signing, the stable local identity and notarization
are in [`packaging/macos/README.md`](../../packaging/macos/README.md); the FFmpeg build is in
[`packaging/ffmpeg/README.md`](../../packaging/ffmpeg/README.md).

## A development build next to an installed release

macOS ties the Screen Recording grant to the bundle id **and** the signing requirement. A release
(notarized, Developer ID) and a build from this checkout (local identity or ad hoc) both use
`dev.capturemann.app` by default, so whichever you open second fails the other's grant: the log shows
`Failed to match existing code requirement`, and the permissions screen says "not granted" although
the switch is on. Build your own copies with `make bundle-dev` (bundle id `dev.capturemann.app.dev`)
and each keeps its own grants. The Keychain item (service `dev.capturemann.app`) is still shared, so
the first launch of a differently signed copy asks once for the login password: choose **Always
Allow**.

## Permissions on a development Mac

See [macOS permissions](../user/macos-permissions.md).

## Logging

Library and app code reports diagnostics through the `log` facade (`log::warn!`, `log::info!`, ...). The binaries that start a process (`cm-desktop`, `cm-overlay`, `cm-daemon`) install a small stderr logger (`cm_core::logging`). By default the `cm_*` crates log at `info` and everything else (GPUI, wgpu, ...) at `warn`. `RUST_LOG` overrides it: a bare level applies to every crate, `target=level` pairs to one.

```sh
RUST_LOG=debug cargo run -p cm-daemon                  # everything at debug
RUST_LOG=warn,cm_daemon=debug cargo run -p cm-daemon   # quiet, except the daemon
RUST_LOG=cm_daemon::watcher=off make run-bundle        # silence one module
```

Command output that is the product (CLI results, the MCP stdio protocol, `--help` text, the opt-in `CM_TRACE` and `CM_OVERLAY_TRACE` JSON lines) is still printed directly. Never log tokens, secrets or the contents of a user's files.
