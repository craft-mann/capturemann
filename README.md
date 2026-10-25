<h1 align="center">CaptureMann</h1>

<p align="center">
Free, open-source screenshots and screen recording with a webcam bubble, local AI captions,<br>
and every feature available to AI agents through MCP and a CLI.
</p>

<p align="center">
<a href="https://github.com/craft-mann/capturemann/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/craft-mann/capturemann/actions/workflows/ci.yml/badge.svg"></a>
<a href="LICENSE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue"></a>
<img alt="Platform: macOS 13+" src="https://img.shields.io/badge/platform-macOS%2013%2B-lightgrey">
</p>

> **Status: 0.1.0, first release (macOS 13+, Apple Silicon only).** Windows follows later. Most
> results are tested with scripted fakes, plus checks on real devices by the owner on one Mac; there
> are few stored hardware reports. What is still unverified is tracked in
> [`specs/04-delivery-plan.md`](specs/04-delivery-plan.md) under "Verification debt".

## Features

- **Screenshots** of a display, a window or a region, with an annotation editor and Auto-redact for
  emails, keys, phone numbers and IPs.
- **Screen recording** up to the display's native resolution (4K/5K) at 30 or 60 fps, with a webcam
  bubble, microphone (noise suppression), system audio and cursor telemetry. Pause, resume, and
  crash recovery that loses at most two seconds.
- **Editor**: trim, cut, split, speed, backgrounds, zoom regions, restylable cursor, 9:16 and 1:1
  reframe. Export to MP4, platform presets, GIF or WebM.
- **Local AI**: Whisper transcripts and captions (SRT, VTT, TXT, or burned in), a caption editor, and
  zoom and silence-cut suggestions. Models download only when you press Download.
- **Library** of self-contained capture bundles, stored in a folder you choose.
- **Share**: save to local folders or Google Drive, publish to YouTube (private by default).
- **For agents**: 53 MCP tools plus resources and prompts, and an identical `capturemann` CLI. Risky
  actions (start recording, publish, delete) need an on-screen confirmation.
- **Private by design**: no telemetry, and no network traffic unless you connect a destination or
  download a model. You bring your own Google credentials; secrets live in the macOS Keychain.
- English and Vietnamese interface.

## Install

Requires a Mac with Apple Silicon and macOS 13 or later. Intel Macs are not supported.

1. Download `CaptureMann-<version>-macos-arm64.dmg` and its `.sha256` file from
   [Releases](https://github.com/craft-mann/capturemann/releases).
2. Check the download (in the folder with both files):
   `shasum -a 256 -c CaptureMann-<version>-macos-arm64.dmg.sha256` must print `OK`.
3. Open the DMG and drag CaptureMann to Applications.

The release notes say whether a build is notarized. A build that is **ad hoc signed and not
notarized** is blocked by Gatekeeper at first launch: right-click the app and choose Open, or run
`xattr -dr com.apple.quarantine /Applications/CaptureMann.app`. Updates are manual: download the new
DMG. To build it yourself instead, see [building from source](docs/building.md). More on verifying a
download: [docs/releasing.md](docs/releasing.md#how-users-verify-a-download).

## Quick start

```sh
make run-bundle                 # builds, signs and opens target/bundle/CaptureMann.app
# the CLI and MCP server are inside the bundle, in Contents/MacOS
target/bundle/CaptureMann.app/Contents/MacOS/capturemann capture screenshot
```

Connect an AI agent over MCP (stdio):

```json
{ "mcpServers": { "capturemann": { "command": "/path/to/CaptureMann.app/Contents/MacOS/capturemann", "args": ["mcp"] } } }
```

See [`specs/02-agent-interface.md`](specs/02-agent-interface.md) for the tools, permission tiers and
the CLI.

## Build from source

Rust 1.90, Xcode command line tools and `cmake`:

```sh
cargo test --workspace --locked
make bundle
```

Full instructions, the bundled LGPL FFmpeg and macOS permissions: [`docs/`](docs/README.md).

## Documentation

- [`docs/`](docs/README.md): building, permissions, plan, design, spikes
- [`specs/`](specs/README.md): product spec, technical design, agent interface, integrations, features
- [`CONTRIBUTING.md`](CONTRIBUTING.md), [`SECURITY.md`](SECURITY.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md),
  [`CHANGELOG.md`](CHANGELOG.md)

## Contributing

Contributions are welcome, especially new storage providers, publishing platforms and translations.
Read [`CONTRIBUTING.md`](CONTRIBUTING.md) first. Security problems: please report them privately, see
[`SECURITY.md`](SECURITY.md).

## Links

CaptureMann is a [CraftMann](https://www.craftmann.app/) app.
[Contact](https://www.craftmann.app/capturemann/) ·
[Privacy policy](https://www.craftmann.app/capturemann/privacy/) ·
[Source](https://github.com/craft-mann/capturemann)

## License

Apache-2.0, see [LICENSE](LICENSE). Third-party notices: [NOTICE](NOTICE) and
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). The bundled FFmpeg is an unmodified LGPL build.

The Apache-2.0 licence covers the code. It does not grant rights to the CaptureMann or CraftMann names or logos (`design/brand`); forks should use their own name and marks.
