# L1 macos 0.2.9 regression report (smoke)

- **Date:** 2026-11-07T08:11:07Z to 2026-11-07T08:11:20Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 26f81195, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel3/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel3/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261107T081107Z --who cm-regress via regress.sh --verdict PARTIAL --commit 26f81195`, tier `smoke`
- **Result:** 12 passed, 0 failed, 2 skipped of 14 cases
- **SHA-256 of the files under test:**
  - `capturemann` `2fbe9895c4965ae3d31a763329edbf29035f218921c5414f37b42740524812d1`
  - `cm-mcp` `01af3a1e1913b258559a9f9631b21dbba0a79352e8d59a4b4fbe22e858793340`
  - `cm-daemon` `a03c732bb18275db18adaeb08ec88e27b087b11773fc72ce37866a7ecb9aa474`
  - `cm-desktop` `b80c947bf324999687af977c487d86548ce53ebcc38f2edcc6d96e7f51f1fa11`
  - `cm-overlay` `8b3ab178640b61eb0d5004f1c9384e3d38b83930c9ba1a7d973a213588ef2187`
  - `ffmpeg` `e44c11c00832793d20df1e0defede5237ca3fb55f0bf4b115b0aba943a6086ce`
  - `ffprobe` `6191f65daf15e0031ea0b8acf3848625ff74dbd139ad5f2bc885170dc576dce7`
  - `llama-server` `e257b02c4ae603da159ac7b529dcd9e93ad2daba76695665aee6529c88f34c26`

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

- `pkg.files` (355 ms): capturemann: 4026496 bytes; cm-mcp: 9570736 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon; cm-desktop: 105518544 bytes; cm-overlay: 5657136 bytes; licence: Some("/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel3/CaptureMann.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/private/tmp/claude-502/-Users-trungtran-source-...
- `pkg.startup` (1570 ms): capturemann --version: exit 0 (0x00000000) in 19 ms; cm-mcp --bogus: exit 2 (0x00000002) in 97 ms; cm-daemon --bogus: exit 2 (0x00000002) in 842 ms; cm-overlay --help: exit 0 (0x00000000) in 61 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 19 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 60 ms; llama-server --version: exit 0 (0x00000000) in 116 ms
- `pkg.diagnostics` (1285 ms): version 0.2.9; daemon pid 78253 answered after 292 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1248 ms): daemon pid 78373 answered after 293 ms; pid 78373, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1836 ms): daemon pid 78427 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1112 ms): daemon pid 78561 answered after 292 ms; daemon pid 78578 answered after 293 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (739 ms): daemon pid 78650 answered after 293 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; 20 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Calendar, ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Neo, Notes, Slack, System Information, Telegram, Terminal, WeChat; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (704 ms): daemon pid 78691 answered after 293 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (769 ms): daemon pid 78760 answered after 293 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (735 ms): daemon pid 78804 answered after 293 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1256 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 78849 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
