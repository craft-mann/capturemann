# L1 windows 0.2.7 regression report (hardware)

- **Date:** 2026-11-05T08:34:50Z to 2026-11-05T08:35:19Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261105T083447Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera,second_monitor --only rec.mic,rec.system_audio,rec.camera,shot.second_monitor`, tier `hardware`
- **Result:** 2 passed, 2 failed, 0 skipped of 4 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `4ce16c07d21d28a65e73c780850ed5072d18cd90a4738bc21ad2efe2c8150518`
  - `cm-mcp.exe` `c8b5462820c2a2ab65e25704328f001f82797de77a0ef4500a12b0ea03b95e57`
  - `cm-daemon.exe` `5c87af51434d35fab0b07a9afb66050e80d6f4c045dfb6678a841e85feb67e09`
  - `cm-desktop.exe` `38445bd66ccd7727ed9918774c8c37de2d1f8eda649f75b1b826d642194574e0`
  - `cm-overlay.exe` `102c291228b4b3fbfc1813e02895e38dfbd70e2d476a6222a11608e49ef358df`
  - `ffmpeg.exe` `08d073edcbbbf6b8c3c11efb9a78a8f21af9c555da86d815fa2fbcb0c2d8d609`
  - `ffprobe.exe` `e09a5180f022cc05bab32a8ba3aed0d54e642cb81a9952a57aa24a121e46102f`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `shot.second_monitor` | PASS | every extra monitor lists and captures at its own physical size |
| `rec.mic` | FAIL | a microphone track is recorded for the length of the recording: left programs running: ffmpeg.exe (1484) |
| `rec.system_audio` | FAIL | system audio contains a tone played by another process: no volumedetect output: [aist#0:0/pcm_f32le @ 000002ba2ebb58c0] Guessed Channel Layout: stereo Input #0, wav, from 'C:\Users\trung\AppData\Local\Temp\cmr4320\3\library\regress recording.cmrec\system.wav'... |
| `rec.camera` | PASS | a camera track keeps at least 24 fps in a lit room |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AUD-1 | FAIL | `rec.mic`, `rec.system_audio` |
| AUD-2 | FAIL | `rec.mic` |
| CAM-1 | PASS | `rec.camera` |
| CAM-2 | PASS | `rec.camera` |
| CAP-1.AC1 | PASS | `shot.second_monitor` |
| DPI-WIN layout A | PASS | `shot.second_monitor` |
| bug class: camera 10 fps in low light | PASS | `rec.camera` |
| bug class: process loopback | FAIL | `rec.system_audio` |

## Problems found

### `rec.mic`: a microphone track is recorded for the length of the recording

- left programs running: ffmpeg.exe (1484)

Evidence:

```text
daemon pid 9440 answered after 319 ms
started rec_01a1aad8c9d9_0001
mic track 4.18 s
log hygiene: 1 WARN line(s) in the case's logs
```

### `rec.system_audio`: system audio contains a tone played by another process

- no volumedetect output: [aist#0:0/pcm_f32le @ 000002ba2ebb58c0] Guessed Channel Layout: stereo Input #0, wav, from 'C:\Users\trung\AppData\Local\Temp\cmr4320\3\library\regress recording.cmrec\system.wav': Duration: 00:00:06.85, bitrate: 3072 kb/s Stream #0:0: Audio: pcm_f32le ([3][0][0][0] / 0x0003), 48000 Hz, stere...

Evidence:

```text
daemon pid 7796 answered after 322 ms
started rec_01a1aad8e53b_0001
```

## Not run

- Every selected case ran.

## Evidence of passing cases

- `shot.second_monitor` (3474 ms): daemon pid 14848 answered after 319 ms; display 4327037 'Display 1' at (0,0) listed 1920x1080, saved 1920x1080; display 7931565 'S27B80P' at (-3840,-1861) listed 3840x2160, saved 3840x2160; log hygiene: 1 WARN line(s) in the case's logs
- `rec.camera` (7581 ms): daemon pid 7068 answered after 320 ms; started rec_01a1aad907e8_0001; camera 1280x720 h264 measured 29.4 fps, 5.47 s; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
