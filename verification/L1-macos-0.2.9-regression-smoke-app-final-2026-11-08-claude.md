# L1 macos 0.2.9 regression report (smoke)

- **Date:** 2026-11-08T00:59:52Z to 2026-11-08T01:00:06Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 1d5f429d, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel5/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel5/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261108T005952Z --who cm-regress via regress.sh --verdict PARTIAL --commit 1d5f429d`, tier `smoke`
- **Result:** 12 passed, 0 failed, 2 skipped of 14 cases
- **SHA-256 of the files under test:**
  - `capturemann` `0ff644b188010ce3eac3c1e81501e038d3062300f888690ca0f7c2e2a8e38a32`
  - `cm-mcp` `df24632de1ab9c9aa08dd85488af2b9dbe0e855e5c9537a1096cf8e7516606fb`
  - `cm-daemon` `a03c732bb18275db18adaeb08ec88e27b087b11773fc72ce37866a7ecb9aa474`
  - `cm-desktop` `c10712ca143ca1ef0ab3ba18a8579d081d2e656522031a288668bb691cc1fb37`
  - `cm-overlay` `aa707edd4ba4be6508dc1b301ca1760ba87ac5224af7564f5bd802e1a03163a8`
  - `ffmpeg` `9881c6972c875e3f52db8923abfe08454681b6cdbfa9bc07d4b83e9433a278c8`
  - `ffprobe` `7a547933d3015bc4920bda1f6bd22df825b779f01f2c68414123105b1d058acb`
  - `llama-server` `ca8f0a0801c3d765e9f48bb0f0cea64e57f890030dc44a4452f8b263891c8840`

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

- `pkg.files` (334 ms): capturemann: 4026496 bytes; cm-mcp: 9570736 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon; cm-desktop: 105519984 bytes; cm-overlay: 5657136 bytes; licence: Some("/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel5/CaptureMann.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/private/tmp/claude-502/-Users-trungtran-source-...
- `pkg.startup` (1587 ms): capturemann --version: exit 0 (0x00000000) in 19 ms; cm-mcp --bogus: exit 2 (0x00000002) in 109 ms; cm-daemon --bogus: exit 2 (0x00000002) in 840 ms; cm-overlay --help: exit 0 (0x00000000) in 57 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 19 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 61 ms; llama-server --version: exit 0 (0x00000000) in 111 ms
- `pkg.diagnostics` (2906 ms): version 0.2.9; daemon pid 45561 answered after 1923 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1223 ms): daemon pid 45715 answered after 288 ms; pid 45715, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1795 ms): daemon pid 45769 answered after 288 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1117 ms): daemon pid 45902 answered after 289 ms; daemon pid 45920 answered after 293 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (708 ms): daemon pid 45993 answered after 293 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; 21 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Calendar, CaptureMann, ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Neo, Notes, Slack, System Information, Terminal, WeChat, ‎WhatsApp; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (684 ms): daemon pid 46033 answered after 293 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (737 ms): daemon pid 46074 answered after 293 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (716 ms): daemon pid 46149 answered after 292 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1232 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 46192 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
