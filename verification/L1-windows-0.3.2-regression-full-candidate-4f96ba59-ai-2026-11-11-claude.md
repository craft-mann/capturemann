# L1 windows 0.3.2 regression report (full)

- **Date:** 2026-11-11T15:15:22Z to 2026-11-11T15:16:38Z
- **Who:** win-regress
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 4f96ba59, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\20261111T151520Z --who win-regress --verdict PARTIAL --only ai.transcribe,ai.speakers,installer.silent --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --speaker-models C:\lab\r6\spk --installer C:\lab\capturemann-0.3.2-windows-x64-setup.exe --commit 4f96ba59`, tier `full`
- **Result:** 2 passed, 1 failed, 0 skipped of 3 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `d5f1b03ac2ffa2779da16f6c332b27e5a33be1b3545a6648aa2faaaf1a8dbcfe`
  - `cm-mcp.exe` `4125b5b09aac28601d2cff625d92d62356267935764474f626a578a04476a1e4`
  - `cm-daemon.exe` `9a7db3874ad61d61254d444a565a8c503b349672768a00f54eead1cb34adfe20`
  - `cm-desktop.exe` `9f9dbfb9c5da6a550ee575de54aa0441837ead957c21bcf772c23c7330a09b83`
  - `cm-overlay.exe` `5b0343becd6d44b87e407e49756366c522f5fb30cee13efa84ce122c0cdcbbd0`
  - `ffmpeg.exe` `d4eb242a9a247b61c759b8e2b1e39f6fa761c12c9b228480e201e9ebf3c2fc9c`
  - `ffprobe.exe` `48d7c62a4bd062c6bfa7c8f2c9cf364426cbbf00dd7c852e2cca8f2c0bcb2fa6`
  - `llama-server.exe` `5e7ea405d049d122a7ab47347fe248a6b3152897c678d0b3c3b16f78a59800e8`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | PASS | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models) |
| `installer.silent` | FAIL | silent install into a temp folder, the programs run, silent uninstall leaves nothing: could not start the program: The system cannot find the file specified. (os error 2) |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AI-1 | PASS | `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
| AI-8 | PASS | `ai.speakers` |
| O7 installer | FAIL | `installer.silent` |
| bug: Transcribe illegal instruction (AVX-512) | PASS | `ai.transcribe` |
| windows-package.yml silent install | FAIL | `installer.silent` |

## Problems found

### `installer.silent`: silent install into a temp folder, the programs run, silent uninstall leaves nothing

- could not start the program: The system cannot find the file specified. (os error 2)

Evidence:

```text
1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]
saved before: 8 shared key(s), Start menu folder yes, user PATH yes
```

## Not run

- Every selected case ran.

## Evidence of passing cases

- `ai.transcribe` (26599 ms): daemon pid 1700 answered after 321 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-11T15:15:25.453Z", "depends_on": [], "elapsed_ms": 24234, "error": null, "eta_ms": null, "finished_at": "2026-11-11T15:15:49.691Z", "held": false, "inputs": { "...; library entry after transcription: {"broken":false,"created":"2026-11-11T15:15:25.3454542Z","duration":6015000,"height":null,"id":"rec_06GWPBCQBEPHAD7Q","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"imported...
- `ai.speakers` (44965 ms): daemon pid 9492 answered after 321 ms; speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-11T15:16:19.849Z", "depends_on": [], "elapsed_ms": 7517, "error": null, "eta_ms": null, "finished_at": "2026-11-11T15:16:35.365Z", "held": false, "inputs": { "b...; speakers listed: 2; speakers on the words: {"1", "2"}; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
