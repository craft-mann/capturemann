# L1 windows 0.3.4 regression report (hardware)

- **Date:** 2026-11-16T01:02:19Z to 2026-11-16T01:02:30Z
- **Who:** Claude
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 011b5000, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261116T010219Z --who Claude --verdict PARTIAL --allow screen,camera --only rec.camera --commit 011b5000`, tier `hardware`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `ead08e8b3ead6f5fb17ba98fd75e7c512f361eebdea0a28df81956897d809182`
  - `cm-mcp.exe` `14468b52c449a48a9218b4cb65a310b6a0144e50d212a4223e4dd528350a18c3`
  - `cm-daemon.exe` `f96ac2014d0141b86c6b3710cf8d53dc2a624aabdcc7a63f28092307e8801dc0`
  - `cm-desktop.exe` `46211dc2723e2b5e0c05b8be132c24285ddc31d0bf5f82e52241a9c4635187ee`
  - `cm-overlay.exe` `c5e2e4a606495300481e942f0aa68500d6b1ff278a5ee46decaecb0f38c135d1`
  - `ffmpeg.exe` `fac8b792db7bed42aff11860d9049f9ef9b4067346db52106a6ec5c3eb50d837`
  - `ffprobe.exe` `3fe6dd1ed834fe612d2f76cd19ca340fa1ce97c6a66e86764ed482342ecaeefa`
  - `llama-server.exe` `feb878e6896b32671343fce5e25d243099e231d148dbbca2ee016bcecca7d293`

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

- `rec.camera` (8855 ms): daemon pid 10076 answered after 320 ms; started rec_01a1e1e064c4_0001; export job job_e234deeb0514401a: done; camera 1280x720 h264 measured 25.9 fps, 5.52 s; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
