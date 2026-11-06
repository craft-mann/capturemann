# L1 linux 0.2.9 regression report (smoke)

- **Date:** 2026-11-06T17:30:28Z to 2026-11-06T17:30:40Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: n/a
- **Build:** commit unknown, package folder `/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --out /home/trung/lab/regress-out/20261106T173027Z --who cm-regress via regress.sh --verdict PARTIAL`, tier `smoke`
- **Result:** 11 passed, 1 failed, 2 skipped of 14 cases
- **SHA-256 of the files under test:**
  - `capturemann` `cf059d0f373a147816b10fac2599fe79a7ae1d36474bf179a709841ed731b0c9`
  - `cm-mcp` `fdaf0f0b9ae5e835d70857cb499a5512ecb3666652dfd60b310006250b582ead`
  - `cm-daemon` `0df2ab34585d65eb62e1813de5ac926a8be8641a9fd1cea78d6b207f3254fafb`
  - `cm-desktop` `8b95538f254b532d2c58a2bf5d505e6d0318582748b84ee763952741def78e84`
  - `cm-overlay` `ffe785f5cee771d7b0c5838fbe268ea4154bcd82f554feba0fa5dd03eb640435`
  - `ffmpeg` `1b928d7b7170b240a6efbafae9b1416812b85a921d54604a7a4bd5f06a91c400`
  - `ffprobe` `45dbb102c7babc4d45eef0b4365fbd2b08fabf55f448f6c9278a65d07780eae7`
  - `llama-server` `d693fb41e91a3329006276137757a600c8861e7a501468898dfa6fcb2ec10132`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | FAIL | the package holds the programs, FFmpeg, the helper and the licences: no LICENSE file; no THIRD_PARTY_LICENSES file |
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
| `zz.crash_events` | SKIP | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run: no crash log is read on Linux yet |

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
| NFR-licence | FAIL | `pkg.files` |
| PKG-contents | FAIL | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: illegal instruction | not run | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | not run | `zz.crash_events` |
| bug: window name garbled | PASS | `targets.list` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| platform-acceptance | PASS | `linux.session` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |

## Problems found

### `pkg.files`: the package holds the programs, FFmpeg, the helper and the licences

- no LICENSE file; no THIRD_PARTY_LICENSES file

Evidence:

```text
capturemann: 4476440 bytes
cm-mcp: 9411520 bytes
cm-daemon: 96017008 bytes
cm-desktop: 147930952 bytes
cm-overlay: 10305552 bytes
licence: None, third party: None, notice: None
note: no README in the package folder
```

## Not run

- needs --allow screen (1): shot.display
- no crash log is read on Linux yet (1): zz.crash_events

## Evidence of passing cases

- `pkg.startup` (428 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 16 ms
- `pkg.diagnostics` (1065 ms): version 0.2.9; daemon pid 19907 answered after 282 ms; - video encoder: software VP9 through ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg; - llama-server at /opt/capturemann/lib/capturemann/llama-server (bundled with this build, llama.cpp v0.7.0); - text recognizer: tesseract at /usr/bin/tesseract (tesseract 5.3.4); - speaker engine built in: true; - spe...
- `daemon.lifecycle` (1261 ms): daemon pid 19971 answered after 282 ms; pid 19971, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1565 ms): daemon pid 20036 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1041 ms): daemon pid 20112 answered after 282 ms; daemon pid 20171 answered after 282 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (732 ms): daemon pid 20268 answered after 282 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: ; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (648 ms): daemon pid 20321 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (805 ms): daemon pid 20385 answered after 282 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (789 ms): daemon pid 20460 answered after 282 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1261 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 20520 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (350 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
