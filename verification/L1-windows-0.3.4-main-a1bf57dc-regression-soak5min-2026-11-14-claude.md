# L1 windows 0.3.4 regression report (hardware)

- **Date:** 2026-11-14T14:21:52Z to 2026-11-14T14:26:57Z
- **Who:** Claude
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit a1bf57dc, package folder `C:\lab\q034\pkg`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --only rec.soak --soak-minutes 5 --package-dir C:\lab\q034\pkg --out C:\lab\q034\regress-soak --allow screen,mic,sound --who Claude --verdict PARTIAL --commit a1bf57dc`, tier `hardware`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `aa57327a1c8f3d3a2794cb26e0de1745727329a49f501d027e5f0d6e26ef1f79`
  - `cm-mcp.exe` `52a2ce61d52fd492d79b39e6e28fd74c081a4241f3b06b08172084cb6454938f`
  - `cm-daemon.exe` `caedf8fcd89f868094ac3523249dd7056069178661bc66fad4f8bafcea14d837`
  - `cm-desktop.exe` `57a46743dd7d2113bbc60b311d5ac98b1bb4e9de3c57265e942abb46178d6373`
  - `cm-overlay.exe` `84e4f6f0c715383794bc142330087eee31eb7862e6e8bc37a16cfc55f052b099`
  - `ffmpeg.exe` `fac8b792db7bed42aff11860d9049f9ef9b4067346db52106a6ec5c3eb50d837`
  - `ffprobe.exe` `3fe6dd1ed834fe612d2f76cd19ca340fa1ce97c6a66e86764ed482342ecaeefa`
  - `llama-server.exe` `feb878e6896b32671343fce5e25d243099e231d148dbbca2ee016bcecca7d293`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `rec.soak` | PASS | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| NFR-2 | PASS | `rec.soak` |
| REC-WIN gate | PASS | `rec.soak` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `rec.soak` (302814 ms): daemon pid 15276 answered after 319 ms; started rec_01a1da6faf55_0001; 300 s: state recording recorded Some(Number(300356)) ms late Some(Number(0.0))%; soak: h264 1920x1080 avg Some(30.0) 300.7 s; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
