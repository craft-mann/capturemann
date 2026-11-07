# L1 windows 0.2.9 regression report (hardware)

- **Date:** 2026-11-07T12:34:28Z to 2026-11-07T12:34:39Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit ae61845f, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261107T123427Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,camera,mic --only rec.camera --commit ae61845f`, tier `hardware`
- **Result:** 0 passed, 1 failed, 0 skipped of 1 cases
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
| `rec.camera` | FAIL | a camera track keeps at least 24 fps in a lit room: the camera ran at 10.1 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun) |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| CAM-1 | FAIL | `rec.camera` |
| CAM-2 | FAIL | `rec.camera` |
| bug class: camera 10 fps in low light | FAIL | `rec.camera` |

## Problems found

### `rec.camera`: a camera track keeps at least 24 fps in a lit room

- the camera ran at 10.1 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun)

Evidence:

```text
daemon pid 14716 answered after 320 ms
started rec_01a1b600d807_0001
export job job_4e04e6e88b32ba01: done
camera 1280x720 h264 measured 10.1 fps, 5.54 s
```

## Not run

- Every selected case ran.

## Evidence of passing cases


## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
