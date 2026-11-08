# L1 macos 0.2.9 regression report (smoke)

- **Date:** 2026-11-08T01:17:27Z to 2026-11-08T01:17:40Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 53a3cb4e, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel6/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel6/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261108T011726Z --who cm-regress via regress.sh --verdict PARTIAL --commit 53a3cb4e`, tier `smoke`
- **Result:** 12 passed, 0 failed, 2 skipped of 14 cases
- **SHA-256 of the files under test:**
  - `capturemann` `ac33aaf1b350a43ca95591b415f50096808156520f53a34b53e4e59e2dccaccc`
  - `cm-mcp` `df2df224d6e8cb9defc3d5c62e8a75c9f4b623e5fe68f66e1bae9299121c4398`
  - `cm-daemon` `a03c732bb18275db18adaeb08ec88e27b087b11773fc72ce37866a7ecb9aa474`
  - `cm-desktop` `72a4352701c566e5a864b6c547ca8e3b8ba5edd9c2145974c4f54ddd91c0b777`
  - `cm-overlay` `2428f236b1e301daab04275439f9fecfff1d66fadf76df15aefa0638da77be4f`
  - `ffmpeg` `fd4fb21978726bc356a9d113d81de3a56ec028da8adf18a753b8eb4febef9203`
  - `ffprobe` `cb65a5d80f376c25ae2b293c56b263ee81488c50cf63c76d8449879d5f20fd34`
  - `llama-server` `9214af6070458059eeaf77c70ff30453d62a51b5fc7a38bdc16ce2484c5d6113`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | PASS | the package holds the programs, FFmpeg, the helper and the licences |
| `pkg.startup` | PASS | every program starts without a crash (illegal instruction, fast fail) |
| `pkg.diagnostics` | PASS | diagnostics reports app, system, models and engines, FFmpeg and the helper |
| `daemon.lifecycle` | PASS | daemon starts on a private endpoint, answers, stops, and the endpoint is gone |
| `daemon.library_created` | PASS | a fresh library is created and lists no captures; no helper program runs while idle |
| `settings.roundtrip` | PASS | settings set and get round trip, and survive a daemon restart |
| `targets.list` | PASS | targets lists displays and windows with readable names and real sizes |
| `shot.display` | SKIP | a display screenshot is a valid PNG of the display's physical size: needs --allow screen |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | PASS | tools/list matches the tool table of the spec (no drift either way) |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: not implemented for macos |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT-1 | PASS | `daemon.lifecycle`, `settings.roundtrip` |
| AGT-2 | PASS | `mcp.handshake`, `mcp.tools_list` |
| AI-7 | PASS | `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| CAP-1 | PASS | `targets.list` |
| CAP-1.AC1 | not run | `shot.display` |
| CAP-3 | PASS | `targets.list` |
| CAP-4: export_image | PASS | `lib.manifest` |
| DIAG-1 | PASS | `pkg.diagnostics` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest` |
| LINUX-1 | not run | `linux.session` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-licence | PASS | `pkg.files` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: window name garbled | PASS | `targets.list` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| platform-acceptance | not run | `linux.session` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |

## Problems found

- None in the cases that ran.
## Not run

- needs --allow screen (1): shot.display
- not implemented for macos (1): linux.session

## Evidence of passing cases

- `pkg.files` (351 ms): capturemann: 4026496 bytes; cm-mcp: 9570736 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon; cm-desktop: 105519984 bytes; cm-overlay: 5657136 bytes; licence: Some("/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel6/CaptureMann.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/private/tmp/claude-502/-Users-trungtran-source-...
- `pkg.startup` (1671 ms): capturemann --version: exit 0 (0x00000000) in 19 ms; cm-mcp --bogus: exit 2 (0x00000002) in 96 ms; cm-daemon --bogus: exit 2 (0x00000002) in 928 ms; cm-overlay --help: exit 0 (0x00000000) in 60 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 19 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 80 ms; llama-server --version: exit 0 (0x00000000) in 115 ms
- `pkg.diagnostics` (1257 ms): version 0.2.9; daemon pid 70292 answered after 293 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1244 ms): daemon pid 70384 answered after 291 ms; pid 70384, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1797 ms): daemon pid 70465 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1099 ms): daemon pid 70603 answered after 293 ms; daemon pid 70620 answered after 286 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (723 ms): daemon pid 70664 answered after 293 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; 20 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Calendar, ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Neo, Notes, Slack, System Information, Terminal, WeChat, ‎WhatsApp; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (702 ms): daemon pid 70731 answered after 293 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (750 ms): daemon pid 70773 answered after 293 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (725 ms): daemon pid 70842 answered after 291 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1251 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 70884 answered after 292 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
