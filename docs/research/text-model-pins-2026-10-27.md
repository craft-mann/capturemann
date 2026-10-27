# Text model pins (read Oct 27, 2026)

Read from the Hugging Face API and the LFS pointer files at the commit shown (public files, nothing
downloaded). Candidates come from `local-model-and-licence-check-2026-10-27.md`. All three repos
tag `license:apache-2.0`. A pin is only final after the maintainer has downloaded the file once
and the SHA-256 matches.

| Tier | Repo @ commit | File | SHA-256 | Bytes |
| --- | --- | --- | --- | --- |
| 8 GB | `lmstudio-community/Qwen3.5-4B-GGUF` @ `f9f88ac3e234be915e23811a6d28ea287bdb927e` | `Qwen3.5-4B-Q4_K_M.gguf` | `25082a7dd3776cc3c741c6347d3bd04523f05796607b3fbc32fa3a25dfa1418c` | 2707513696 |
| 16 GB | `lmstudio-community/Qwen3.5-9B-GGUF` @ `1379f25c6b505a3fc737bd7818cb09389cf807c1` | `Qwen3.5-9B-Q4_K_M.gguf` | `cd76ec205963b3b33350093e6904d9de16c4e666fd104e1f632d25c7f15f2a13` | 5627044256 |
| 32 GB | `ggml-org/gemma-4-26B-A4B-it-GGUF` @ `9cf8c8db0f83fb4582f15381f8fb1a7c1b0b9ca2` | `gemma-4-26B-A4B-it-Q4_0.gguf` | `d208665ab1cd3a69f7a9a4bc59430e8448c8093d9b06334f566ac59d6d504a03` | 14618145824 |

URL shape: `https://huggingface.co/<repo>/resolve/<commit>/<file>`. `ram_mb` is not measured: the research
estimates about 4.1, 7.1 and 16.2 GB peak at 16K context (file + KV cache + buffers); use 4500, 7600
and 16800 until measured on a Mac.
