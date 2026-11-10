# L1 windows 0.3.1 regression report (full)

- **Date:** 2026-11-10T01:34:26Z to 2026-11-10T01:35:45Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 97ea4b8a, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\20261110T013425Z --who cm-regress via regress.sh --verdict PARTIAL --allow sound --only ai.transcribe,ai.speakers,installer.silent --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --speaker-models C:\lab\r6\spk --installer C:\lab\capturemann-0.3.1-windows-x64-setup.exe --commit 97ea4b8a`, tier `full`
- **Result:** 3 passed, 0 failed, 0 skipped of 3 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `d40155a757d7c177017455a13875125a8e3bac5e9b1055d263f74efe58b2b753`
  - `cm-mcp.exe` `3a8303e022941b85705c9937c733c0ba7ccabe4e26ddce09f372a6f38f19d79f`
  - `cm-daemon.exe` `980c2daed95b4c820ffca6fcd2aec5220a653300f4929ba5b2a106461c9c5190`
  - `cm-desktop.exe` `cd747a0120331b534ea328afa90b6e6eb2b15d82ac8f7ada187752b2a5e7457d`
  - `cm-overlay.exe` `75d7aabb5a4cf140896f878eda997a3d62c1bccba58c4aad7b188f10b56bdfe0`
  - `ffmpeg.exe` `d4eb242a9a247b61c759b8e2b1e39f6fa761c12c9b228480e201e9ebf3c2fc9c`
  - `ffprobe.exe` `48d7c62a4bd062c6bfa7c8f2c9cf364426cbbf00dd7c852e2cca8f2c0bcb2fa6`
  - `llama-server.exe` `5e7ea405d049d122a7ab47347fe248a6b3152897c678d0b3c3b16f78a59800e8`
  - `capturemann-0.3.1-windows-x64-setup.exe` `3fe6623b831251ec57a174ee7260f4fca41301a7ac361d901ecc2ab2c8b9ad87`

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

- `ai.transcribe` (22113 ms): daemon pid 11156 answered after 321 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-10T01:34:29.74Z", "depends_on": [], "elapsed_ms": 19688, "error": null, "eta_ms": null, "finished_at": "2026-11-10T01:34:49.431Z", "held": false, "inputs": { "a...; library entry after transcription: {"broken":false,"created":"2026-11-10T01:34:29.6440998Z","duration":6015000,"height":null,"id":"rec_06GW65X69KFA28S3","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"importe...
- `ai.speakers` (39454 ms): daemon pid 10292 answered after 320 ms; speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-10T01:35:16.286Z", "depends_on": [], "elapsed_ms": 6732, "error": null, "eta_ms": null, "finished_at": "2026-11-10T01:35:29.624Z", "held": false, "inputs": { "b...; speakers listed: 2; speakers on the words: {"1", "2"}; log hygiene: 1 WARN line(s) in the case's logs
- `installer.silent` (14652 ms): 1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]; saved before: 8 shared key(s), Start menu folder yes, user PATH yes; installer: exit 0 (0x00000000) in 10484 ms; installed: capturemann 0.3.1; daemon pid 11344 answered after 319 ms; uninstaller: exit 0 (0x00000000) in 225 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
