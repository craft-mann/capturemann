# L1 windows 0.2.9 regression report (hardware)

- **Date:** 2026-11-08T01:02:16Z to 2026-11-08T01:02:29Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit ae61845f, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261108T010215Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,camera --only rec.camera --commit ae61845f`, tier `hardware`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `18a95748ff29971c44f0227ed03580137decfb5dcd51b807000d7c8e9e0253b8`
  - `cm-mcp.exe` `c1665f768a7249cf1febea50b8c52f26fd838641ea917adeb103d79123acc823`
  - `cm-daemon.exe` `aa13dc180d57ba60772883899bfe2642328a65360f694b3faeb9289cc6c032ed`
  - `cm-desktop.exe` `dbf5fc09437efd909e2c21b3213f0b89b168b844e97930c491e6d2d34f474682`
  - `cm-overlay.exe` `92a8325b84d64c039d70f0fdee61f343afaf89e1013e86c5ed32935a7f99bcd0`
  - `ffmpeg.exe` `b94bead6469a51a9e44db2f06a4daea3efb90686e937262ea2569c304cf31b7e`
  - `ffprobe.exe` `7f13cb52988e7589090d2afe5f1a81dad1656d21578843f282a2ad241db54ad8`
  - `llama-server.exe` `340b747fd5cd59e27a402c8e0d68bd03376cc2d33026c2c14d1818fd97c23d7b`

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

- `rec.camera` (9868 ms): daemon pid 15972 answered after 320 ms; started rec_01a1b8ad7f0d_0001; export job job_cdd81071505f2b1a: done; camera 1280x720 h264 measured 26.3 fps, 5.63 s; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
