# L1 linux 0.2.6 regression report (smoke)

- **Date:** 2026-11-06T14:15:07Z to 2026-11-06T14:15:19Z
- **Who:** claude-orchestrator
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: n/a
- **Build:** commit unknown, package folder `/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier smoke --out /home/trung/lab/regress-out/20261106T141506Z --who claude-orchestrator --verdict PARTIAL`, tier `smoke`
- **Result:** 9 passed, 3 failed, 2 skipped of 14 cases
- **SHA-256 of the files under test:**
  - `capturemann` `0b686eba9fb5a8055ffd9326f40aef8330e3e4d0f9fdab0b9fe3e9d312c46b75`
  - `cm-mcp` `bf4ce173a625f22e526827ef3dbb0d82582ca7bacb0230a73f7a7e98933a588c`
  - `cm-daemon` `07f613d3303af7e7a97d9575bb060729cf655f8ffdf163afed4b94a86a3ca3bd`
  - `cm-desktop` `eafc57097cdca0b8339b9b3f21d94183e143288a190a1c8f24a1f29cf3de7bc1`
  - `cm-overlay` `ff6cc670214d44dfc3e1e226497cd6a6971872b5ab4f6ec54012485a1ad3fb7c`
  - `ffmpeg` `a50fdb123b89d9122b62da6c5030b6bb5e09eb9d8df7f07bd8cff3e8f91ef846`
  - `ffprobe` `c5bc5542f0cb02501a2431a6f474d5672a569401ec654c4a44c660919d5ca188`
  - `llama-server` `eb1ea7337742b604ecf1702486f21683fb2f065ad86cfd054e517170f31a1c62`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | FAIL | the package holds the programs, FFmpeg, the helper and the licences: no LICENSE file; no THIRD_PARTY_LICENSES file |
| `pkg.startup` | PASS | every program starts without a crash (illegal instruction, fast fail) |
| `pkg.diagnostics` | FAIL | diagnostics reports app, system, models and engines, FFmpeg and the helper: the llama-server line does not say `bundled with this build` |
| `daemon.lifecycle` | PASS | daemon starts on a private endpoint, answers, stops, and the endpoint is gone |
| `daemon.library_created` | PASS | a fresh library is created and lists no captures; no helper program runs while idle |
| `settings.roundtrip` | PASS | settings set and get round trip, and survive a daemon restart |
| `targets.list` | PASS | targets lists displays and windows with readable names and real sizes |
| `shot.display` | SKIP | a display screenshot is a valid PNG of the display's physical size: needs --allow screen |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | FAIL | tools/list matches the tool table of the spec (no drift either way): tools the spec lists as built but the server lacks: get_cursor_track, normalize_transcript, get_normalized_transcript, import_media, get_platform_rules |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `zz.crash_events` | SKIP | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run: no crash log is read on Linux yet |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT-1 | PASS | `daemon.lifecycle`, `settings.roundtrip` |
| AGT-2 | FAIL | `mcp.handshake`, `mcp.tools_list` |
| AI-7 | PASS | `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| CAP-1 | PASS | `targets.list` |
| CAP-1.AC1 | not run | `shot.display` |
| CAP-3 | PASS | `targets.list` |
| CAP-4: export_image | PASS | `lib.manifest` |
| DIAG-1 | FAIL | `pkg.diagnostics` |
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
| specs/02-agent-interface.md tool table | FAIL | `mcp.tools_list` |

## Problems found

### `pkg.files`: the package holds the programs, FFmpeg, the helper and the licences

- no LICENSE file; no THIRD_PARTY_LICENSES file

Evidence:

```text
capturemann: 4249672 bytes
cm-mcp: 9240448 bytes
cm-daemon: 93259440 bytes
cm-desktop: 144953024 bytes
cm-overlay: 10249136 bytes
licence: None, third party: None, notice: None
note: no README in the package folder
```

### `pkg.diagnostics`: diagnostics reports app, system, models and engines, FFmpeg and the helper

- the llama-server line does not say `bundled with this build`

Evidence:

```text
version 0.2.6
daemon pid 3579 answered after 282 ms
- video encoder: software VP9 through ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg
- speech model speaker-segmentation (6 MB)
- speech model speaker-embedding (29 MB)
- ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg
- llama-server at /opt/capturemann/lib/capturemann/llama-server
- text recognizer: tesseract at /usr/bin/tesseract (tesseract 5.3.4)
- speaker engine built in: true
```

### `mcp.tools_list`: tools/list matches the tool table of the spec (no drift either way)

- tools the spec lists as built but the server lacks: get_cursor_track, normalize_transcript, get_normalized_transcript, import_media, get_platform_rules

Evidence:

```text
daemon pid 4131 answered after 282 ms
74 tools
```

## Not run

- needs --allow screen (1): shot.display
- no crash log is read on Linux yet (1): zz.crash_events

## Evidence of passing cases

- `pkg.startup` (426 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 16 ms
- `daemon.lifecycle` (1260 ms): daemon pid 3641 answered after 281 ms; pid 3641, 81 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1597 ms): daemon pid 3706 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1042 ms): daemon pid 3783 answered after 282 ms; daemon pid 3842 answered after 282 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `targets.list` (764 ms): daemon pid 3939 answered after 282 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 1 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Gnome-terminal; log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (749 ms): daemon pid 3992 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (810 ms): daemon pid 4056 answered after 282 ms; server capturemann 0.2.6, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1264 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 4189 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (335 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
