# Architecture overview

A one-page map for someone new to the code. The design in full is
[`specs/01-technical-design.md`](../../specs/01-technical-design.md); the rules and commands are in
[`CLAUDE.md`](../../CLAUDE.md).

## The idea

Every feature is an **action** on one bus, owned by `cm-daemon`. The desktop app, the overlay
windows, the `capturemann` CLI and the MCP server are clients of that bus, so a person and an AI agent
get the same state, undo history and confirmation prompts. The desktop app embeds the daemon in its own
process; the CLI and MCP server reach a running daemon through `cm-ipc` (JSON-RPC over a local socket).

```
 apps/desktop ──┐  (embeds the daemon)        capturemann (cm-cli) ─┐
 apps/overlay ──┤                             cm-mcp ───────────────┤  cm-ipc
                ▼                                                   ▼
            ┌───────────────────── cm-daemon: action bus ─────────────────────┐
            │ confirmation gate · per-client policy · jobs · events · settings │
            └──┬────────┬─────────┬─────────┬─────────┬──────────┬────────────┘
         cm-capture  cm-audio  cm-camera  cm-cursor  cm-library  cm-media / cm-render
         cm-encode                                   cm-storage / cm-publish / cm-auth / cm-ai
```

A capture is a folder (`.cmshot` or `.cmrec`) of media tracks plus `manifest.json`; the SQLite index only
points at it. Masters are read-only after recording, and every edit is a revision in the manifest.

## Crates

| Crate | Role |
|---|---|
| `cm-core` | Shared primitives: `MediaTime`, geometry, documents, actions, undo, the shared error codes |
| `cm-daemon` | The action bus, recording engine, JSON-RPC server, confirmation gate, settings |
| `cm-ipc` | Client side of the daemon's local IPC |
| `cm-cli`, `cm-mcp` | The `capturemann` command line and the MCP server (stdio and local HTTP) |
| `cm-capture` | Screenshots, the screen recording track, session clock, disk guard |
| `cm-camera`, `cm-audio`, `cm-cursor` | The camera, microphone and system audio, and cursor tracks |
| `cm-encode` | Hardware encoders and the fragmented-MP4 writer |
| `cm-library` | Capture bundles, manifest, SQLite index, search, thumbnails, crash recovery |
| `cm-render` | Annotations, Auto-redact and the `wgpu` compositor used by preview and export |
| `cm-media` | FFmpeg sidecar: exporters and the continuous preview decoder |
| `cm-ai` | Whisper transcripts and captions, model store, zoom and cut suggestions |
| `cm-jobs` | Persistent job queue (export, transcribe, upload, publish) |
| `cm-auth`, `cm-storage`, `cm-publish` | OAuth and keychain, storage providers (local, Google Drive), publish providers (YouTube) |
| `cm-i18n` | UI languages: catalogues, `tr!` macros, locale-aware formatting |
| `apps/desktop`, `apps/overlay` | The GPUI shell (library, editor, settings) and the region picker and bubble windows |

Crates under `crates/` must be testable without a display. Anything that touches hardware sits behind a
trait with a scripted fake, so the default `cargo test --workspace` never needs a screen, camera or
microphone.

## Where to look

| I want to... | Start at |
|---|---|
| add or change an action | `crates/cm-daemon/src/action/` (`registry.rs` lists the actions), then its MCP tool (`cm-mcp`) and CLI command (`cm-cli`) |
| change what a feature must do | the matching file in [`specs/features/`](../../specs/features/), with its acceptance criteria |
| change recording | `crates/cm-daemon/src/recording/engine/`, then the track crate |
| change the editor | `apps/desktop/src/ui/` (views) and `apps/desktop/src/recedit/` (edit model) |
| add a UI string | [`locales/README.md`](../../locales/README.md) |
| add a storage or publish provider | `cm-storage` or `cm-publish`, and [`CONTRIBUTING.md`](../../CONTRIBUTING.md) |
| check a feature by hand | [`tests/manual/`](../../tests/manual/README.md) |
