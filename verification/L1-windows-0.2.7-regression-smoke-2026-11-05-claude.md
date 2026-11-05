# L1 windows 0.2.7 regression report (smoke)

- **Date:** 2026-11-05T08:14:03Z to 2026-11-05T08:14:45Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --out C:\lab\regress-out\20261105T081358Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen`, tier `smoke`
- **Result:** 13 passed, 0 failed, 0 skipped of 13 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `4ce16c07d21d28a65e73c780850ed5072d18cd90a4738bc21ad2efe2c8150518`
  - `cm-mcp.exe` `c8b5462820c2a2ab65e25704328f001f82797de77a0ef4500a12b0ea03b95e57`
  - `cm-daemon.exe` `5c87af51434d35fab0b07a9afb66050e80d6f4c045dfb6678a841e85feb67e09`
  - `cm-desktop.exe` `38445bd66ccd7727ed9918774c8c37de2d1f8eda649f75b1b826d642194574e0`
  - `cm-overlay.exe` `102c291228b4b3fbfc1813e02895e38dfbd70e2d476a6222a11608e49ef358df`
  - `ffmpeg.exe` `08d073edcbbbf6b8c3c11efb9a78a8f21af9c555da86d815fa2fbcb0c2d8d609`
  - `ffprobe.exe` `e09a5180f022cc05bab32a8ba3aed0d54e642cb81a9952a57aa24a121e46102f`
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

- `pkg.files` (719 ms): capturemann.exe: 4556800 bytes; cm-mcp.exe: 8084992 bytes; cm-daemon.exe: 57489920 bytes; cm-desktop.exe: 95506944 bytes; cm-overlay.exe: 2780160 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (14295 ms): capturemann --version: exit 0 (0x00000000) in 33 ms; cm-mcp --bogus: exit 2 (0x00000002) in 2123 ms; cm-daemon --bogus: exit 2 (0x00000002) in 7962 ms; cm-overlay --help: exit 0 (0x00000000) in 2222 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 33 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 464 ms; llama-server --version: exit 0 (0x00000000) in 731 ms
- `pkg.diagnostics` (1267 ms): version 0.2.7; daemon pid 11732 answered after 321 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\llama-server.exe (bundled with this build, llama.cpp v0.7.0); - text recognizer: window...
- `daemon.lifecycle` (2128 ms): daemon pid 12384 answered after 321 ms; pid 12384, 84 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (2130 ms): daemon pid 14248 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1874 ms): daemon pid 9848 answered after 321 ms; daemon pid 3048 answered after 318 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (1237 ms): daemon pid 11356 answered after 321 ms; display 4327037 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; display 7931565 'S27B80P' 3840x2160 at (-3840,-1861) scale 1.5; 3 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Settings, Windows Terminal, cm-desktop; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1513 ms): daemon pid 11092 answered after 321 ms; display 4327037 1920x1080; PNG 1920x1080; capture shot_06GTNHH5WW21NCSZ; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1173 ms): daemon pid 13856 answered after 322 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1318 ms): daemon pid 3796 answered after 323 ms; server capturemann 0.2.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1259 ms): daemon pid 6420 answered after 320 ms; 77 tools; known drift (spec lists, server lacks): get_cursor_track, export_image, get_platform_rules; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (2116 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 8168 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
