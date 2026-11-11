# L1 windows 0.3.1 regression report (full)

- **Date:** 2026-11-10T15:48:52Z to 2026-11-10T15:50:31Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 5e1a0d9d, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\20261110T154850Z --who cm-regress via regress.sh --verdict PARTIAL --allow sound --only ai.transcribe,ai.speakers,installer.silent --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --speaker-models C:\lab\r6\spk --installer C:\lab\capturemann-0.3.1-windows-x64-setup.exe --commit 5e1a0d9d`, tier `full`
- **Result:** 3 passed, 0 failed, 0 skipped of 3 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `a87e36b81baa0a7078126ce91b8ed7d9527b88fd7f767d2a20370980d14feea4`
  - `cm-mcp.exe` `757c3caa79efda6460f08ee972914b9660fc74bf95cf2d0a0b03f6ec9fb01701`
  - `cm-daemon.exe` `5c0ebe887f5677d6380ed5040e0d4b77f57d97b12828ab8c598a5b40146563fd`
  - `cm-desktop.exe` `6b902bb4516fee04d695aed3475d50269a9ab7eb53cd753262994948492f098b`
  - `cm-overlay.exe` `af0891b58c48adf3c1f1a2fbd705374f6fb1ce18456bb537af5186e7054f7f36`
  - `ffmpeg.exe` `d4eb242a9a247b61c759b8e2b1e39f6fa761c12c9b228480e201e9ebf3c2fc9c`
  - `ffprobe.exe` `48d7c62a4bd062c6bfa7c8f2c9cf364426cbbf00dd7c852e2cca8f2c0bcb2fa6`
  - `llama-server.exe` `5e7ea405d049d122a7ab47347fe248a6b3152897c678d0b3c3b16f78a59800e8`
  - `capturemann-0.3.1-windows-x64-setup.exe` `e352619477f41d433b150f79f0c714fadd31694364ee72d1954550d679919898`

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

- `ai.transcribe` (28027 ms): daemon pid 3020 answered after 322 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-10T15:48:56.489Z", "depends_on": [], "elapsed_ms": 25587, "error": null, "eta_ms": null, "finished_at": "2026-11-10T15:49:22.077Z", "held": false, "inputs": { "...; library entry after transcription: {"broken":false,"created":"2026-11-10T15:48:56.4033657Z","duration":6015000,"height":null,"id":"rec_06GWC9FB1EDM52XH","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"imported...
- `ai.speakers` (48232 ms): daemon pid 7644 answered after 336 ms; speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-10T15:49:52.456Z", "depends_on": [], "elapsed_ms": 8914, "error": null, "eta_ms": null, "finished_at": "2026-11-10T15:50:10.966Z", "held": false, "inputs": { "b...; speakers listed: 2; speakers on the words: {"1", "2"}; log hygiene: 1 WARN line(s) in the case's logs
- `installer.silent` (18565 ms): 1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]; saved before: 8 shared key(s), Start menu folder yes, user PATH yes; installer: exit 0 (0x00000000) in 13151 ms; installed: capturemann 0.3.1; daemon pid 1224 answered after 320 ms; uninstaller: exit 0 (0x00000000) in 331 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
