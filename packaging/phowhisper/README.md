# PhoWhisper for CaptureMann (M2e)

`vinai/PhoWhisper-{small,medium,large}` (VinAI, Whisper fine-tuned for Vietnamese, BSD-3-Clause per the model
card) are Hugging Face transformers checkpoints. whisper.cpp reads ggml files, and no whisper.cpp-compatible
conversion of these three exists, so the maintainer converts and uploads them to `craftmann/phowhisper-ggml`; the app pins the upload like every
other model (`crates/cm-ai/data/models.json`, three `pending_pin` rows `phowhisper-small|medium|large`).

Status (Nov 3, 2026): **the script ran end to end on `small`** (python 3.12 venv, the pinned packages install, the
converter writes `ggml-model.bin`, the file is moved and hashed). Result: `ggml-phowhisper-small.bin`, 487,601,984
bytes (float16), sha256 `db8de368e822fce5dcdce7c0c4c80c4b554c27655516dea5ec8c093e22f58785`, from checkpoint revision
`a86b604c346caf7148c37512eafe783a16420adb`. `ModelStore::import` accepts it (ignored test
`a_converted_phowhisper_file_is_accepted_by_import`, set `CM_PHOWHISPER_GGML`). whisper.cpp loads it and transcribes
Vietnamese, **but not well with the app's current settings**: see
`verification/benchmarks/vietnamese-fleurs-baseline-2026-11-03.md` (PhoWhisper was fine-tuned without timestamp
tokens, so it needs a no-timestamps decode, and then words glue together). `medium` and `large` were not converted.
Set `PYTHON=/opt/homebrew/bin/python3.12` (the default `python3` may be too new for the pinned torch).

## Convert

```sh
OUT=~/phowhisper-out CLIP=<short public Vietnamese clip> packaging/phowhisper/convert.sh small
```

Output (outside the repository): `ggml-phowhisper-<size>.bin`, `.SHA256`, `.INFO` (bytes, SHA-256, checkpoint
revision, tool commits). Tools are pinned by commit (whisper.cpp v1.9.5, openai/whisper v20250625); the Python
packages by `requirements.txt`.

## Check before uploading

1. The `CLIP` transcription check in the script (the app's `real_model` test, language `vi`) prints text, not
   nothing or a loop. Compare with the same clip on `large-v3-turbo`; that comparison is the M2e benchmark.
2. `ModelStore::import` accepts the file once the catalogue row has `sha256` and `bytes` (copy them from `.INFO`).
3. Word times: PhoWhisper conversions may lack the alignment heads the DTW word times use
   (`whisper.rs::dtw_preset` picks them by model id, so a new id gets none: the default token times apply). Check
   a transcript's word times by ear on one clip and write the result in the benchmark report.

## Upload and pin (maintainer)

Upload the three `.bin` files, the card text of each model (BSD-3-Clause, keep VinAI's copyright line) and a note
of the tool commits to the Hugging Face repo **`craftmann/phowhisper-ggml`** (org https://huggingface.co/craftmann; the
agent never uploads). File names: `ggml-phowhisper-small.bin`, `-medium.bin`, `-large.bin`; the repo's README should
name the source models, the BSD-3-Clause licence and the converter commits. Then, for each row in `models.json`, set `url` to the
commit-pinned `resolve/<commit>/<file>` URL, `sha256`, `bytes` and `ram_mb` (measured, not guessed: the
`perf` example prints peak memory), and remove `"pending_pin": true`. Add the NOTICE entry for the weights.
Until then the rows are neither offered nor downloadable.
