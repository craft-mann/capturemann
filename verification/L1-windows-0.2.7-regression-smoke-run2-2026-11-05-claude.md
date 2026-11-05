# L1 windows 0.2.7 regression report (smoke)

- **Date:** 2026-11-05T10:13:15Z to 2026-11-05T10:13:39Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --out C:\lab\regress-out\20261105T101312Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen`, tier `smoke`
- **Result:** 13 passed, 0 failed, 0 skipped of 13 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `54de4cef1252c28df3f049a30a5c0a9626b7368dc39fd8f273363cc4588ab4a8`
  - `cm-mcp.exe` `22f296cffe660a2ef8f39cb1b256bd603cf6f91a1a96265e8fd3a0dc73f88bd3`
  - `cm-daemon.exe` `c869a197f3994b75e80f9cf3a4464d4f80e9cb897f6a6a8fdf8eab958ebb5b1b`
  - `cm-desktop.exe` `288606b6e3e0aebeef1842de7efafff19b4f1d05e2250b2f31a5cd22874ef767`
  - `cm-overlay.exe` `01983fc20f23ed6a6a1d5f2c0312858513e3a7b3ae8064baef613b5c652c064d`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`

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
| `shot.display` | PASS | a display screenshot is a valid PNG of the display's physical size |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | PASS | tools/list matches the tool table of the spec (no drift either way) |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged by Windows during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT-1 | PASS | `daemon.lifecycle`, `settings.roundtrip` |
| AGT-2 | PASS | `mcp.handshake`, `mcp.tools_list` |
| AI-7 | PASS | `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| CAP-1 | PASS | `targets.list` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-3 | PASS | `targets.list` |
| CAP-4: export_image | PASS | `lib.manifest` |
| DIAG-1 | PASS | `pkg.diagnostics` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest` |
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
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `pkg.files` (631 ms): capturemann.exe: 4557824 bytes; cm-mcp.exe: 8084992 bytes; cm-daemon.exe: 57344000 bytes; cm-desktop.exe: 95361536 bytes; cm-overlay.exe: 2787840 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (884 ms): capturemann --version: exit 0 (0x00000000) in 32 ms; cm-mcp --bogus: exit 2 (0x00000002) in 34 ms; cm-daemon --bogus: exit 2 (0x00000002) in 34 ms; cm-overlay --help: exit 0 (0x00000000) in 19 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 34 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 35 ms; llama-server --version: exit 0 (0x00000000) in 35 ms
- `pkg.diagnostics` (1160 ms): version 0.2.7; daemon pid 5252 answered after 319 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\llama-server.exe (bundled with this build, llama.cpp v0.7.0); - text recognizer: windows...
- `daemon.lifecycle` (1969 ms): daemon pid 12208 answered after 319 ms; pid 12208, 84 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (2078 ms): daemon pid 4544 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1844 ms): daemon pid 10952 answered after 320 ms; daemon pid 10236 answered after 316 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (1204 ms): daemon pid 11960 answered after 321 ms; display 7931569 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 3 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Settings, Windows Terminal, cm-desktop; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1472 ms): daemon pid 14432 answered after 320 ms; display 7931569 1920x1080; PNG 1920x1080; capture shot_06GTPCR4WY9AMRH1; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1228 ms): daemon pid 13404 answered after 320 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1324 ms): daemon pid 13884 answered after 324 ms; server capturemann 0.2.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1240 ms): daemon pid 14688 answered after 320 ms; 77 tools; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (2093 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 3404 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
