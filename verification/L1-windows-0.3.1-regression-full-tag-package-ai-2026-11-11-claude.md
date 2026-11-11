# L1 windows 0.3.1 regression report (full)

- **Date:** 2026-11-11T02:54:00Z to 2026-11-11T02:55:21Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 4d72fa9a, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\20261111T025359Z --who cm-regress via regress.sh --verdict PARTIAL --allow sound --only ai.transcribe,ai.speakers,installer.silent --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --speaker-models C:\lab\r6\spk --installer C:\lab\capturemann-0.3.1-windows-x64-setup.exe --commit 4d72fa9a`, tier `full`
- **Result:** 3 passed, 0 failed, 0 skipped of 3 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `c608d9ac92519d8ebf38ff9fa2e71bac747b0800d515f06a06fc6207340120d7`
  - `cm-mcp.exe` `f48a26026ea12568f172d850a4860c1da18014cde19c22647d293bc5686dec50`
  - `cm-daemon.exe` `7919cc3b329bc3ee5269cd603ae14bc4223f1b6dd316865b9e7d87cae5e0c614`
  - `cm-desktop.exe` `756f9149dadfec52e982c90b8cc9ed67d4fc7378e4d0cf10e40b67aefd2c21dd`
  - `cm-overlay.exe` `2ac0ef304c6ca2acea7b6dfdf5204ebbd3517e14f3b933c0a72de2229ca7b89c`
  - `ffmpeg.exe` `d4eb242a9a247b61c759b8e2b1e39f6fa761c12c9b228480e201e9ebf3c2fc9c`
  - `ffprobe.exe` `48d7c62a4bd062c6bfa7c8f2c9cf364426cbbf00dd7c852e2cca8f2c0bcb2fa6`
  - `llama-server.exe` `5e7ea405d049d122a7ab47347fe248a6b3152897c678d0b3c3b16f78a59800e8`
  - `capturemann-0.3.1-windows-x64-setup.exe` `d9ffd95665117c1c635e749df27c2f8dbc36d11844aa716abdfe738bf947cc78`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | PASS | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models) |
| `installer.silent` | PASS | silent install into a temp folder, the programs run, silent uninstall leaves nothing |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AI-1 | PASS | `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
| AI-8 | PASS | `ai.speakers` |
| O7 installer | PASS | `installer.silent` |
| bug: Transcribe illegal instruction (AVX-512) | PASS | `ai.transcribe` |
| windows-package.yml silent install | PASS | `installer.silent` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `ai.transcribe` (22080 ms): daemon pid 5512 answered after 322 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-11T02:54:04.54Z", "depends_on": [], "elapsed_ms": 19651, "error": null, "eta_ms": null, "finished_at": "2026-11-11T02:54:24.193Z", "held": false, "inputs": { "a...; library entry after transcription: {"broken":false,"created":"2026-11-11T02:54:04.4276962Z","duration":6015000,"height":null,"id":"rec_06GWH1PXS8XPMWAA","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"imported...
- `ai.speakers` (39778 ms): daemon pid 6200 answered after 319 ms; speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-11T02:54:50.765Z", "depends_on": [], "elapsed_ms": 6778, "error": null, "eta_ms": null, "finished_at": "2026-11-11T02:55:04.646Z", "held": false, "inputs": { "b...; speakers listed: 2; speakers on the words: {"1", "2"}; log hygiene: 1 WARN line(s) in the case's logs
- `installer.silent` (15245 ms): 1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]; saved before: 8 shared key(s), Start menu folder yes, user PATH yes; installer: exit 0 (0x00000000) in 10647 ms; installed: capturemann 0.3.1; daemon pid 7116 answered after 319 ms; uninstaller: exit 0 (0x00000000) in 257 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
