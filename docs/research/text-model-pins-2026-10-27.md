# Text model pins (read Oct 27, 2026; revised after the small-model test, B2 of the research note)

Read from the Hugging Face API and the LFS pointer files at the commit shown (public files, nothing
downloaded). Candidates and test results: `local-model-and-licence-check-2026-10-27.md` (sections B and B2).
All three repos tag `license:apache-2.0`. A pin is only final after the maintainer has downloaded the file once
and the SHA-256 matches.

| Role | Repo @ commit | File | SHA-256 | Bytes | ram_mb (research estimate at 16K) |
| --- | --- | --- | --- | --- | --- |
| Default, every Mac: Gemma 4 E2B-it | `ggml-org/gemma-4-E2B-it-GGUF` @ `6938c58e698502037401a5f19f6905f2fb2f7dee` | `gemma-4-E2B-it-Q4_0.gguf` | `8e30dff3ac4c8434c49a7036fa15564bdbb6044e42bf04550bf1a096ad7e6a52` | 2841481184 | 3700 |
| Optional, best translation: Qwen3.5-4B | `lmstudio-community/Qwen3.5-4B-GGUF` @ `f9f88ac3e234be915e23811a6d28ea287bdb927e` | `Qwen3.5-4B-Q4_K_M.gguf` | `25082a7dd3776cc3c741c6347d3bd04523f05796607b3fbc32fa3a25dfa1418c` | 2707513696 | 4100 |
| Lowest-memory fallback: Qwen3.5-2B | `lmstudio-community/Qwen3.5-2B-GGUF` @ `bb84e11355a036e28f080c7793fa6d22b7c4e344` | `Qwen3.5-2B-Q4_K_M.gguf` | `0bfe35afc9f05b7fac3fa04925e051ac7939a42a8a17ea11afc99701bea826cc` | 1270808032 | 2100 |

URL shape: `https://huggingface.co/<repo>/resolve/<commit>/<file>`. The earlier 9B (16 GB) and Gemma 4 26B-A4B (32 GB)
pins were dropped on Oct 27, 2026: reports only extract and rephrase what the transcript says, so they do not need them.

Added Oct 27, 2026 (maintainer): `ggml-org/gemma-4-26B-A4B-it-GGUF` @ `9cf8c8db0f83fb4582f15381f8fb1a7c1b0b9ca2`, `gemma-4-26B-A4B-it-Q4_0.gguf`, SHA-256 `d208665ab1cd3a69f7a9a4bc59430e8448c8093d9b06334f566ac59d6d504a03`, 14618145824 bytes, ram_mb 16800 (estimate), shown only on Macs with 32 GB or more.
