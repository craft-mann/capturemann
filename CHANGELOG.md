# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project follows [Semantic Versioning](https://semver.org/) once it reaches 1.0.

## [Unreleased]

## [0.1.0] - Unreleased

First public release. macOS 13+ on Apple Silicon only; Windows follows later.

### Added

- Screenshots of a display, a window or a region, with an annotation editor (arrows, shapes, text,
  pen, highlight, blur) and Auto-redact for emails, keys and tokens, phone numbers and IPv4 addresses.
  The window picker (⌥⇧2) shows live thumbnails of the open windows.
- Screen recording up to the display's native resolution at 30 or 60 fps, with a webcam bubble
  (with its own toolbar for size, shape, mirror and camera off), microphone, system audio and cursor
  telemetry; pause, resume, restart, discard, and crash recovery that loses at most two seconds.
  The start sheet, countdown, recording HUD and a menu-bar icon that shows when the app needs
  attention (for example a missing permission or a failed upload).
- Recording editor: trim, cut, split, speed, background and padding, zoom regions (with a label),
  cursor styling and click effects, audio mix, 9:16 and 1:1 reframe. Changes are non-destructive and
  undoable. Export to MP4, platform presets (YouTube 1080p, 1440p, 4K, vertical, square), GIF and
  WebM through a bundled LGPL FFmpeg.
- Local AI: Whisper transcripts and captions (SRT, VTT, TXT, or burned into the video), a caption
  editor (split at the cursor, find and replace, play this line), a glossary for names and terms,
  and suggestions for zoom regions and for cutting silences and filler words. Models download only
  when you press Download.
- Library of `.cmshot` and `.cmrec` bundles with search and Uploaded and Published filters; bundles
  added or changed outside the app are picked up.
- Storage to local folders and Google Drive (your own Google client), and publishing to YouTube
  (private by default, asks for confirmation every time).
- Every feature as an MCP tool (54 tools, plus resources and prompts) and a `capturemann` command
  line, with per-client permission tiers and on-screen confirmation for recording, publishing and
  deleting.
- Per-tool agent permissions (Settings, Agents & CLI): mutating tools can be set to Allow or Ask first, and starting a recording can be allowed without asking; publish and delete always ask.
- English and Vietnamese interface (Settings, General, Language).
- No telemetry; no network traffic unless you connect a destination or download a model.
- Release builds are a DMG with a SHA-256 file on GitHub Releases.

### Known limitations

- macOS 13 or later on Apple Silicon (arm64) only. Intel Macs and Windows are not supported yet;
  Windows is planned after this release.
- Minimized windows are not listed in the window picker.
- Until the Apple Developer ID certificate is in place, builds are signed ad hoc and are not
  notarized: on first launch right-click the app and choose Open, or run
  `xattr -dr com.apple.quarantine /Applications/CaptureMann.app`. Each ad hoc build has a different
  signature, so macOS asks for the Screen Recording permission again after you install a new one.
  The release notes of each version say which kind of build it is.
- Updates are manual: download the new DMG from GitHub Releases. There is no update check.
- Only Google Drive and YouTube are offered. OneDrive and Dropbox are built but shown as "Coming
  soon"; TikTok and Facebook Pages are not included. Contributions are welcome.
- AI titles, descriptions and chapters are not included; there are no API keys and no text model in
  this release.
- Most features are tested with scripted fakes. Hardware paths were checked by the owner on one Mac
  (Apple M4 Pro, macOS 26) and are not stored as reports; the 60 fps self-test, long soaks and sync
  measurements are not done. Drive and YouTube were not run against a real account in automated
  tests. See `specs/04-delivery-plan.md`, "Verification debt".
