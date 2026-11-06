# L1 windows 0.2.7 regression report (hardware)

- **Date:** 2026-11-06T01:07:12Z to 2026-11-06T01:08:41Z
- **Who:** verify7-agent
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 2e299d68b1be6c815d19bde47ed81fec8bdb09ad, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261106T010707Z --who verify7-agent --verdict PARTIAL --allow screen,sound,mic,camera --only ai.no_model,ai.speakers,ai.transcribe,rec.camera --whisper-model C:/lab/r6/ggml-small.bin --speech C:/lab/r6/speech2.wav --speaker-models C:/lab/r6/spk --commit 2e299d68b1be6c815d19bde47ed81fec8bdb09ad`, tier `hardware`
- **Result:** 3 passed, 1 failed, 0 skipped of 4 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `151778d1c113ac4275a98fc23f5228ee33e71df4499b3aa5e72796a0768acc3c`
  - `cm-mcp.exe` `ae6fbf68c31c2b1044dd1ca6a66c14ad7c4193ff88766560bf695ef486b1e514`
  - `cm-daemon.exe` `79cc26336e889696b4da1227e7a36e1b5aef378c043b5b285f4bc44d1bdf23af`
  - `cm-desktop.exe` `3e255bc92a491c0fe1743cac1305002027e9b544c789474c0ada21ecdad1ee5a`
  - `cm-overlay.exe` `39e0356725895600049b88306d88f4abfd2aa141c82e78da4a44b699af370fef`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `rec.camera` | PASS | a camera track keeps at least 24 fps in a lit room |
| `ai.no_model` | FAIL | without a model, transcribe and the text model say what to do (exit 2) instead of failing: expected exit 2 with reason no_model, got exit 0 (0x00000000) None None |
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | PASS | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models) |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AI-1 | FAIL | `ai.no_model`, `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
| AI-7 | FAIL | `ai.no_model` |
| AI-8 | PASS | `ai.speakers` |
| CAM-1 | PASS | `rec.camera` |
| CAM-2 | PASS | `rec.camera` |
| bug class: camera 10 fps in low light | PASS | `rec.camera` |
| bug: Transcribe illegal instruction (AVX-512) | PASS | `ai.transcribe` |

## Problems found

### `ai.no_model`: without a model, transcribe and the text model say what to do (exit 2) instead of failing

- expected exit 2 with reason no_model, got exit 0 (0x00000000) None None

Evidence:

```text
daemon pid 2300 answered after 321 ms
transcribe without a model: exit 0 (0x00000000) None 
```

## Not run

- Every selected case ran.

## Evidence of passing cases

- `rec.camera` (9519 ms): daemon pid 14224 answered after 321 ms; started rec_01a1ae6544d3_0001; export job job_9b15ea1d0e011548: done; camera 1280x720 h264 measured 26.1 fps, 5.59 s; log hygiene: 1 WARN line(s) in the case's logs
- `ai.transcribe` (29155 ms): daemon pid 6668 answered after 321 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-06T01:07:27.872Z", "depends_on": [], "elapsed_ms": 26785, "error": null, "eta_ms": null, "finished_at": "2026-11-06T01:07:54.66Z", "held": false, "inputs": { "a...; library entry after transcription: {"broken":false,"created":"2026-11-06T01:07:27.7936642Z","duration":6015000,"height":null,"id":"rec_06GTWSBPS4JS98QC","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"imported...
- `ai.speakers` (44897 ms): daemon pid 3236 answered after 321 ms; speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-06T01:08:24.726Z", "depends_on": [], "elapsed_ms": 7691, "error": null, "eta_ms": null, "finished_at": "2026-11-06T01:08:40.117Z", "held": false, "inputs": { "b...; speakers listed: 2; speakers on the words: {"1", "2"}; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
