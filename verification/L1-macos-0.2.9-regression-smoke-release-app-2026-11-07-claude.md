# L1 macos 0.2.9 regression report (smoke)

- **Date:** 2026-11-07T00:12:45Z to 2026-11-07T00:12:58Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit unknown, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261107T001245Z --who cm-regress via regress.sh --verdict PARTIAL`, tier `smoke`
- **Result:** 12 passed, 0 failed, 2 skipped of 14 cases
- **SHA-256 of the files under test:**
  - `capturemann` `6e8b4a4c3794488e98866545a8ac5c10518c18f57d0c7ba9fba5014f5af4cbc5`
  - `cm-mcp` `50cbc86c1ec03adcd3de3cbe7786358c58042a62b5ef8c3d27a3f68924eded8a`
  - `cm-daemon` `a272d3d793619b3333214cdf61c896bed421af0e400a465443711629de6ba31d`
  - `cm-desktop` `900af6be3d132b1012aaa0ec9a4a7ce0f9b0d97cf48d6d975b43743a3625fa36`
  - `cm-overlay` `c6de698a112af795a97063a53055277d61070e6d1a0786f7b5b9eaad1dd7fee0`
  - `ffmpeg` `971e0179ed134cfa59df6a2d9735e8aa6edb2b8c90e394610e972f68d4b10d63`
  - `ffprobe` `6da79db5fb7b9a9e64b9f266c1072a52174ab03bb06da823be94749253b1a910`
  - `llama-server` `a90441179dce4eb46cea5df8c98cb69a4231f8edb095dfa9b92b567946084478`

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

- `pkg.files` (342 ms): capturemann: 4026496 bytes; cm-mcp: 9570736 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon; cm-desktop: 105675024 bytes; cm-overlay: 5657136 bytes; licence: Some("/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel/CaptureMann.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/private/tmp/claude-502/-Users-trungtran-source-r...
- `pkg.startup` (1107 ms): capturemann --version: exit 0 (0x00000000) in 22 ms; cm-mcp --bogus: exit 2 (0x00000002) in 22 ms; cm-daemon --bogus: exit 2 (0x00000002) in 620 ms; cm-overlay --help: exit 0 (0x00000000) in 16 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 20 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 21 ms; llama-server --version: exit 0 (0x00000000) in 22 ms
- `pkg.diagnostics` (1318 ms): version 0.2.9; daemon pid 10450 answered after 300 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1255 ms): daemon pid 10574 answered after 291 ms; pid 10574, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1760 ms): daemon pid 10654 answered after 304 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1156 ms): daemon pid 10761 answered after 297 ms; daemon pid 10805 answered after 303 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (743 ms): daemon pid 10849 answered after 294 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; 20 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: CaptureMann, ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Neo, Notes, Slack, System Information, Telegram, Terminal, WeChat; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (708 ms): daemon pid 10889 answered after 295 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (784 ms): daemon pid 10959 answered after 293 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (744 ms): daemon pid 11001 answered after 298 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1269 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 11069 answered after 303 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
