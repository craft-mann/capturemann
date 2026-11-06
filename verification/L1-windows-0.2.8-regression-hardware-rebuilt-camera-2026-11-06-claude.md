# L1 windows 0.2.8 regression report (hardware)

- **Date:** 2026-11-06T12:24:05Z to 2026-11-06T12:24:16Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit e54cc665, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261106T122404Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera --only rec.camera --commit e54cc665`, tier `hardware`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `cd6278fa32965137536745eb2da1dd9c4ebb2b810add56825a7e89574d90b5eb`
  - `cm-mcp.exe` `df01d78104b136eb976c7dcee714fe0844cfbf844f3b9d4878cb150cfe96f46e`
  - `cm-daemon.exe` `38edeb5a82a3e99cc1ab448afba089684a3043e1f7f6c44745911de655a04f38`
  - `cm-desktop.exe` `b02d3c4c0639858cba2a025c0cc8b9608b3428ff0ae9b62452a17547f75e4511`
  - `cm-overlay.exe` `8350a3e65ba06ac16a5822596fe3eaf89a4ef26841d03af081da26c23b28036a`
  - `ffmpeg.exe` `c0132821863d88ce0f49c568e522eff69741b2714e1a575ac5977b56384dd8e5`
  - `ffprobe.exe` `173b980b6e91c37123fde110367f432604ddb78b405f5533d707a6daa38172b3`
  - `llama-server.exe` `eec30348ea81f2bd4109c98ef13a09a0120218d0d78953ab9f2b784069f12eda`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `rec.camera` | PASS | a camera track keeps at least 24 fps in a lit room |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| CAM-1 | PASS | `rec.camera` |
| CAM-2 | PASS | `rec.camera` |
| bug class: camera 10 fps in low light | PASS | `rec.camera` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `rec.camera` (9206 ms): daemon pid 12748 answered after 321 ms; started rec_01a1b0d0faa4_0001; export job job_3e0edb1ec532113c: done; camera 1280x720 h264 measured 26.1 fps, 5.51 s; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
