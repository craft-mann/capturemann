# L1 windows 0.3.6 regression report (full)

- **Date:** 2026-11-20T11:51:31Z to 2026-11-20T12:01:41Z
- **Who:** Claude QA agent
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 8a746f6b, package folder `C:\lab\q36f\pkg`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --only ai.transcribe --timeout-scale 3 --out C:\lab\q36f\regress-out\r3-transcribe --who Claude QA agent --verdict PARTIAL --allow screen --package-dir C:\lab\q36f\pkg --commit 8a746f6b --whisper-model C:\lab\q36f\models\ggml-medium.bin`, tier `full`
- **Result:** 0 passed, 1 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `c9b7357285e38ee50a58b8cb28f49256d203552b75ae063437be9a065d8e5894`
  - `cm-mcp.exe` `3d3356d5cf8068f45326d6795def698858f5a5c8c438d148e786d006235855e6`
  - `cm-daemon.exe` `06fd9568bae61f99d7c3047c5580e5371bbf5a1b0c22a0f04dd1f52f3fb4a580`
  - `cm-desktop.exe` `509b9944fed9ec6d044510a3863471339776321ac24012ae99ecd13977ae3f58`
  - `cm-overlay.exe` `2a9caa12b859a05ec5a0347b508b965cacb8f1d63a08e990e0b16943cb0b2a3a`
  - `ffmpeg.exe` `fac8b792db7bed42aff11860d9049f9ef9b4067346db52106a6ec5c3eb50d837`
  - `ffprobe.exe` `3fe6dd1ed834fe612d2f76cd19ca340fa1ce97c6a66e86764ed482342ecaeefa`
  - `llama-server.exe` `feb878e6896b32671343fce5e25d243099e231d148dbbca2ee016bcecca7d293`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `ai.transcribe` | FAIL | a transcription with a real model runs to the end without a crash: transcribe failed: {"error":{"code":"busy","message":"the job is still running after 600 s; it keeps running in the daemon (job job_0a88d21421e3113f)","data":{"job_id":"job_0a88d21421e3113f","s... |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AI-1 | FAIL | `ai.transcribe` |
| AI-3 | FAIL | `ai.transcribe` |
| bug: Transcribe illegal instruction (AVX-512) | FAIL | `ai.transcribe` |

## Problems found

### `ai.transcribe`: a transcription with a real model runs to the end without a crash

- transcribe failed: {"error":{"code":"busy","message":"the job is still running after 600 s; it keeps running in the daemon (job job_0a88d21421e3113f)","data":{"job_id":"job_0a88d21421e3113f","state":"running","timed_out":true},"exit_code":6}}

Evidence:

```text
daemon pid 14676 answered after 320 ms
transcribe: exit 6 (0x00000006) {"error":{"code":"busy","message":"the job is still running after 600 s; it keeps running in the daemon (job job_0a88d21421e3113f)","data":{"job_id":"job_0a88d21421e3113f","state":"running","timed_out...
```

## Not run

- Every selected case ran.

## Evidence of passing cases


## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
