# L1 windows 0.2.8 regression report (hardware)

- **Date:** 2026-11-06T05:37:15Z to 2026-11-06T05:37:19Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit eb3cb47c, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261106T053714Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic --only ai.speakers --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --commit eb3cb47c`, tier `hardware`
- **Result:** 0 passed, 1 failed, 0 skipped of 1 cases
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
| `ai.speakers` | FAIL | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): the case started a daemon twice |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AI-8 | FAIL | `ai.speakers` |

## Problems found

### `ai.speakers`: speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models)

- the case started a daemon twice

Evidence:

```text
daemon pid 2636 answered after 321 ms
bundled speaker models: 2 .onnx files
```

## Not run

- Every selected case ran.

## Evidence of passing cases


## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.

## Note added after the run

- `ai.speakers` FAIL ("the case started a daemon twice") is a **suite bug**: with the bundled models the case started its daemon for the diagnostics check and again before the transcription. The bundled models were found (`bundled speaker models: 2 .onnx files`, the diagnostics line "bundled with this build" matched). Fixed in `crates/cm-regress/src/cases/models.rs` (the second start is skipped when the first ran). The fixed case has not been run: it needs a new `Windows tests` build. The speaker result for this package rests on the by-hand check in the final hardware report (2 speakers with `--num 2`, models from `Resources\Models\speaker`).
