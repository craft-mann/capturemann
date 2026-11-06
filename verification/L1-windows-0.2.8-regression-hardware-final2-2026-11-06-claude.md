# L1 windows 0.2.8 regression report (hardware)

- **Date:** 2026-11-06T05:32:49Z to 2026-11-06T05:35:08Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit eb3cb47c, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261106T053244Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera,second_monitor --only rec.external_4k,rec.each_monitor,ai.speakers --commit eb3cb47c`, tier `hardware`
- **Result:** 2 passed, 0 failed, 1 skipped of 3 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `9fd2c3b855167ea45af84bf8692500d167afcb704b848970e900faec2a0c8bd7`
  - `cm-mcp.exe` `8b3edef2119bd02f8901e2d126a809ce6cc53c6c363b2b20727aa6a49a8145ba`
  - `cm-daemon.exe` `fe5a069f7dd1ff59a7d55e2ca289351372bd5f4c521cd619d0eba0eaa95563fe`
  - `cm-desktop.exe` `06566742e157e32486ef9e8f4677a7c608e3be1c0830460dabe72d59e4bc3925`
  - `cm-overlay.exe` `1742f13a1db6f7f06b05b675399ff96a901bc75436b8080280b132249db52d5a`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `rec.external_4k` | PASS | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps |
| `rec.each_monitor` | PASS | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AI-8 | not run | `ai.speakers` |
| DPI-WIN steps 7 to 7c | PASS | `rec.each_monitor` |
| NFR-2 | PASS | `rec.external_4k` |
| REC-3 | PASS | `rec.external_4k` |
| REC-7 | PASS | `rec.each_monitor` |
| bug: 4K recording of the external lost frames on the UHD 620 | PASS | `rec.external_4k` |

## Problems found

- None in the cases that ran.
## Not run

- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (1): ai.speakers

## Evidence of passing cases

- `rec.external_4k` (81416 ms): daemon pid 14064 answered after 605 ms; started rec_01a1af5877ae_0001; export job job_13ff3d9c92020428: done; display 11142891: video 3840x2160, display listed 3840x2160 at scale 1.5, screenshot/physical 3840x2160; display 11142891: frame at 2 s vs its screenshot 0.035 (limit 0.15); vs others (frame, screenshots) [(0.158, 0.170)]; told apart from 1, not telling for 0 (screenshots too alike, under 0.05); display 11142891 3840x2160: track 48000..4107351 ticks, file_end NaN, effective_fps NaN, late...
- `rec.each_monitor` (54161 ms): daemon pid 14004 answered after 321 ms; display 27526735 'Display 1' at (0,0) 1920x1080 scale 1.5 primary; display 11142891 'S27B80P' at (-3840,-1861) 3840x2160 scale 1.5; started rec_01a1af59b6f4_0001; export job job_18ddea08211ad0c9: done; display 27526735: video 1920x1080, display listed 1920x1080 at scale 1.5, screenshot/physical 1920x1080; display 27526735: frame at 2 s vs its screenshot 0.017 (limit 0.15); vs others (frame, screenshots) [(0.175, 0.170)]; told apart from 1, not telling for ...

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
