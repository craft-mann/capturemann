# L1 windows 0.2.8 regression report (hardware)

- **Date:** 2026-11-06T05:52:44Z to 2026-11-06T05:54:11Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit eb3cb47c, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261106T055238Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera,second_monitor --only ai.speakers,ai.transcribe,ai.no_model --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --commit eb3cb47c`, tier `hardware`
- **Result:** 3 passed, 0 failed, 0 skipped of 3 cases
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
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | PASS | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models) |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model` |
| AI-8 | PASS | `ai.speakers` |
| bug: Transcribe illegal instruction (AVX-512) | PASS | `ai.transcribe` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `ai.no_model` (2262 ms): daemon pid 14420 answered after 324 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.transcribe` (32562 ms): daemon pid 8900 answered after 339 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-06T05:52:50.719Z", "depends_on": [], "elapsed_ms": 29811, "error": null, "eta_ms": null, "finished_at": "2026-11-06T05:53:20.532Z", "held": false, "inputs": { "...; library entry after transcription: {"broken":false,"created":"2026-11-06T05:52:50.4453916Z","duration":6015000,"height":null,"id":"rec_06GTYTNVYVSDPV0M","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"imported...
- `ai.speakers` (48614 ms): daemon pid 12808 answered after 321 ms; bundled speaker models: 2 .onnx files; speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-06T05:53:53.821Z", "depends_on": [], "elapsed_ms": 8082, "error": null, "eta_ms": null, "finished_at": "2026-11-06T05:54:09.875Z", "held": false, "inputs": { "b...; speakers listed: 2; speakers on the words: {"1", "2"}; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
