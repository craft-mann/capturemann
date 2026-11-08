# L1 linux 0.3.0 regression report (smoke)

- **Date:** 2026-11-08T20:45:37Z to 2026-11-08T20:45:52Z
- **Who:** night orchestrator
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 5301d4b8, package folder `/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --out /home/trung/lab/regress-out/20261108T204536Z --who night orchestrator --verdict PARTIAL --package-dir /home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann --commit 5301d4b8`, tier `smoke`
- **Result:** 13 passed, 0 failed, 1 skipped of 14 cases
- **SHA-256 of the files under test:**
  - `capturemann` `2d6346b93480161ec442ec26ebc099e9d4dbc0e7351944bd6a20cdabf32adf60`
  - `cm-mcp` `0baca1bbeb63be2a930c584db209039e45488366172ff1364f1e3477a30215ca`
  - `cm-daemon` `c546c08a0fd110d225cb636f5376fdc4fde0d1ae1e65a77b37846688e76727d0`
  - `cm-desktop` `b6e76b9e36b3ad2d464a217b1f4faf138dbdae05be24101de707d931905daa6c`
  - `cm-overlay` `fbe5ecfd0192d3026e69c5a9e0c8d0d521e363328009b2f33812d1c953503494`
  - `ffmpeg` `5da034620e64147d7e2ddf849ba258e34996e45849a7ea4777a3eef145e07ccc`
  - `ffprobe` `af190fa91ddd6d97b6e4426461526f3950509454a42c1e91d55b4c57cb78bc13`
  - `llama-server` `9b6a30cc527aa50e6f1c309f103f7fdb87c944a13d3382e733fee7c1d881ca5c`

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
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
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
| LINUX-1 | PASS | `linux.session` |
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
| platform-acceptance | PASS | `linux.session` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |

## Problems found

- None in the cases that ran.
## Not run

- needs --allow screen (1): shot.display

## Evidence of passing cases

- `pkg.files` (318 ms): capturemann: 5013560 bytes; cm-mcp: 9943360 bytes; cm-daemon: 97857840 bytes; cm-desktop: 150193016 bytes; cm-overlay: 10845728 bytes; licence: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../shar...
- `pkg.startup` (445 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 30 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 16 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 16 ms; llama-server --version: exit 0 (0x00000000) in 16 ms
- `pkg.diagnostics` (1190 ms): version 0.3.0; daemon pid 1220320 answered after 282 ms; - GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0; - video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffmpeg at ~/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - llama-server at ~/...
- `daemon.lifecycle` (1258 ms): daemon pid 1220413 answered after 281 ms; pid 1220413, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1739 ms): daemon pid 1220527 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1060 ms): daemon pid 1220640 answered after 282 ms; daemon pid 1220699 answered after 282 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (1297 ms): daemon pid 1220796 answered after 282 ms; display 1 'Built-in display' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":false,"list_windows":false,"pick":true}; apps: ; log hygiene: 2 WARN line(s) in the case's logs
- `lib.manifest` (750 ms): daemon pid 1220866 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (801 ms): daemon pid 1220930 answered after 282 ms; server capturemann 0.3.0, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (795 ms): daemon pid 1221007 answered after 282 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1269 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 1221067 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (338 ms): session wayland desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
