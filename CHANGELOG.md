# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project follows [Semantic Versioning](https://semver.org/) once it reaches 1.0.

## [Unreleased]

First public release (0.0.1) is in preparation. macOS 13+ on Apple Silicon only; Windows follows later.

### Added

- Screenshots (display, window, region) with an annotation editor and Auto-redact.
- Screen recording up to native resolution at 30 or 60 fps with webcam bubble, microphone, system
  audio and cursor telemetry; pause, resume and crash recovery.
- Recording editor: trim, cut, split, speed, background, zoom regions, cursor styling, 9:16 and 1:1
  reframe; export to MP4, platform presets, GIF and WebM through a bundled LGPL FFmpeg.
- Local AI: Whisper transcripts and captions (SRT, VTT, TXT, burned in), a caption editor, glossary,
  zoom and silence/filler-word cut suggestions. Models download only when you ask.
- Library of `.cmshot` and `.cmrec` bundles with search and status filters.
- Storage to local folders and Google Drive; publishing to YouTube (private by default).
- Every feature as an MCP tool (53 tools, plus resources and prompts) and a `capturemann` CLI, with
  permission tiers and on-screen confirmation for agents.
- English and Vietnamese UI.
- No telemetry; no network traffic unless you connect a destination or download a model.
