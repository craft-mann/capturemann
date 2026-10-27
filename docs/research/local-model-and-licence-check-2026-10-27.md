# CaptureMann — Local model and licence check (diarization and text models)

Oct 27, 2026 · @Trung Tran

## Recommendations

Ship speaker labels with **sherpa-onnx + pyannote segmentation-3.0 (MIT) + 3D-Speaker CAM++ VoxCeleb (Apache-2.0)**, **and text reports with llama.cpp and Gemma 4 E2B-it on every Mac (Qwen3.5-4B as the optional translation model; see B2)**; every file below was checked at its source on Oct 27, 2026.

| Job | Pick | Licences | Verdict |
| --- | --- | --- | --- |
| Diarization runtime | `sherpa-onnx` Rust crate 2.0.1, static (bundles ONNX Runtime) | Apache-2.0; ONNX Runtime MIT | Safe to ship |
| Diarization models | `sherpa-onnx-pyannote-segmentation-3-0` (7.0 MB) + `3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx` (29.6 MB) | MIT; Apache-2.0 | Safe to ship (keep notices; see the VoxCeleb note) |
| Diarization challenger for spike S7 | NVIDIA Nemotron-3-Diarization GGUF (107 MB, up to 8 speakers) on NeMo-Speech.cpp | OpenMDW-1.1; Apache-2.0 | Safe to ship, but macOS support for diarization is unverified |
| Text runtime | llama.cpp `llama-server` (or the `llama-cpp-2` crate), static, Metal | MIT (llama.cpp and ggml) | Safe to ship |
| Report model, every Mac (default) | **Gemma 4 E2B-it Q4\_0** (2.84 GB file, about 3.7 GB in use); best score in our test (B2) | Apache-2.0 | Safe to ship |
| Optional, 16 GB+ (best translation) | Qwen3.5-4B Q4\_K\_M (2.71 GB file, about 4.1 GB in use) | Apache-2.0 | Safe to ship |
| Lowest-memory fallback | Qwen3.5-2B Q4\_K\_M (1.27 GB file, about 2.1 GB in use) | Apache-2.0 | Safe to ship; weaker extraction |

*Revised Oct 27 after the small-model test (B2): reports don't need reasoning, so the 9B, 12B and 26B models in section B are no longer recommended; they stay listed for reference.*

**Three facts that change the plan table from Oct 16.** Gemma 4 is licensed under plain Apache-2.0 (Gemma 3 and earlier used Google's own terms). Qwen3.8-27B (Apache-2.0, Aug 2026) is newer than Qwen3.6. NVIDIA's original Sortformer v1 is non-commercial (CC-BY-NC-4.0) and the 4-speaker models can't cover 6 speakers.

**What is still unverified:** final linked binary sizes, peak memory on a Mac with Metal (figures below are computed), Vietnamese quality beyond one smoke test, and NeMo-Speech.cpp diarization on macOS. Spikes S7 and S8 settle these.

## A. Diarization models

Six of the ten candidates can ship (some with attribution); the original Sortformer v1 cannot, and the 4-speaker Sortformers don't meet the 6-speaker need. Sizes come from the Hugging Face API or the download's `Content-Length`.

| Model and exact file | Size | Weights licence (where read) | Gate / account | Training data note | Verdict |
| --- | --- | --- | --- | --- | --- |
| pyannote segmentation-3.0, ONNX: `onnx-community/pyannote-segmentation-3.0` → `onnx/model.onnx` (int8: `onnx/model_int8.onnx`); or sherpa-onnx release `sherpa-onnx-pyannote-segmentation-3-0.tar.bz2` | 6.0 MB (int8 1.5 MB); archive 7.0 MB | MIT ([pyannote card](https://huggingface.co/pyannote/segmentation-3.0), [ONNX card](https://huggingface.co/onnx-community/pyannote-segmentation-3.0)) | Original repo gated (accept, give company and website; agrees to emails). ONNX copies are not gated | Trained on AISHELL, AliMeeting, AMI, AVA-AVD, DIHARD, Ego4D, MSDWild, REPERE, VoxConverse; some have their own terms, but the authors release the weights under MIT | **Safe to ship** (keep MIT notice) |
| pyannote `speaker-diarization-community-1`: `segmentation/pytorch_model.bin` + `embedding/pytorch_model.bin` + PLDA files | 5.9 + 26.6 MB | CC-BY-4.0 ([card](https://huggingface.co/pyannote/speaker-diarization-community-1)) | Gated: Hugging Face account, company, use case, consent to emails. No redistribution limit in the gate text | Embedding is WeSpeaker ResNet34 (VoxCeleb) | **Safe to ship with attribution**; mirroring allowed by CC-BY (name the source, link the licence, say you converted it). PyTorch only: needs conversion for Rust |
| WeSpeaker ResNet34: `Wespeaker/wespeaker-voxceleb-resnet34` → `voxceleb_resnet34.onnx`; LM variant `voxceleb_resnet34_LM.onnx` | 26.5 MB each | Repo tag says Apache-2.0, but WeSpeaker's docs say VoxCeleb models follow the dataset's **CC-BY-4.0** ([pretrained.md](https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md)); LM variant tagged CC-BY-4.0 | None | VoxCeleb is CC-BY-4.0, but its page says "for research purposes" and that video copyright stays with the owners ([licence.txt](https://mm.kaist.ac.kr/datasets/voxceleb/files/license.txt)) | **Needs a lawyer (low risk)**: treat as CC-BY-4.0 with attribution; conflicting labels |
| 3D-Speaker CAM++ English: `3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx` (sherpa-onnx release); source `iic/speech_campplus_sv_en_voxceleb_16k` | 29.6 MB | Apache-2.0 (ModelScope model record; code [LICENSE](https://github.com/modelscope/3D-Speaker/blob/main/LICENSE)) | None | VoxCeleb (same note). Avoid the `zh-cn ... common` models: Chinese data | **Safe to ship** (VoxCeleb note applies) |
| 3D-Speaker ERes2Net English: `3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx` | 26.5 MB | Apache-2.0 (ModelScope `iic/speech_eres2net_sv_en_voxceleb_16k`) | None | VoxCeleb | **Safe to ship** |
| NVIDIA TitaNet-large: `speakerverification_en_titanet_large.nemo`; ONNX `nemo_en_titanet_large.onnx` | 101.6 MB; ONNX 101.4 MB | CC-BY-4.0 ([card](https://huggingface.co/nvidia/speakerverification_en_titanet_large)) | None | VoxCeleb 1/2, Fisher and other corpora | **Safe to ship with attribution**; 3× larger than CAM++ |
| NVIDIA Sortformer v1: `diar_sortformer_4spk-v1.nemo` | 493 MB | **CC-BY-NC-4.0** ([card](https://huggingface.co/nvidia/diar_sortformer_4spk-v1)) | None | — | **Not allowed** (non-commercial) |
| NVIDIA Streaming Sortformer v2: `diar_streaming_sortformer_4spk-v2.q8_0.gguf` | 147 MB | CC-BY-4.0 ([card](https://huggingface.co/nvidia/diar_streaming_sortformer_4spk-v2)) | None | Max **4 speakers** | Licence safe; **does not meet 2–6 speakers** |
| NVIDIA Streaming Sortformer v2.1 | 471 MB (.nemo) | NVIDIA Open Model License ([card](https://huggingface.co/nvidia/diar_streaming_sortformer_4spk-v2.1)) | None | Max 4 speakers | **Needs a lawyer** (custom licence) and too few speakers |
| **NVIDIA Nemotron-3-Diarization** (released Sep 23, 2026): `Nemotron-3-Diarization.q8_0.gguf` | 107 MB | **OpenMDW-1.1** ([card](https://huggingface.co/nvidia/nemotron-3-diarization), [licence](https://openmdw.ai/license/1-1/)): use, modify, redistribute, commercial; keep a copy of the licence and notices; patent grant ends if you sue | None | Up to **8 speakers**, offline and streaming; trained on Fisher, AMI, licensed David AI data and others; card says "ready for commercial or non-commercial use" | **Safe to ship** (keep licence copy). Card lists Linux and NVIDIA GPUs only |

**Mirrors to avoid:** `avencera/speakrs-models` (the files `speakrs` downloads) and `csukuangfj/speaker-embedding-models` carry no licence of their own; convert from the original sources, or keep the original licence files next to anything you host.

## A. Diarization runtimes

sherpa-onnx is the safest fit: an official Apache-2.0 Rust crate with an offline diarization API, static linking by default and a static macOS arm64 build; `ort` means writing the whole pipeline ourselves.

| Runtime | Code licence (where read) | Diarization support | Static / signed macOS app | Size | Rust bindings | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| **sherpa-onnx** (k2-fsa) | Apache-2.0 ([LICENSE](https://github.com/k2-fsa/sherpa-onnx/blob/master/LICENSE)); bundles ONNX Runtime, MIT ([LICENSE](https://github.com/microsoft/onnxruntime/blob/main/LICENSE)) | Built-in offline API: pyannote segmentation + any embedding model + fast clustering (`num_clusters` or a `threshold`); no PLDA/VBx | Crate default feature `static`; build script downloads `sherpa-onnx-v2.0.1-osx-arm64-static-lib.tar.bz2` from GitHub releases, or links your own build via `SHERPA_ONNX_LIB_DIR`. Static libs mean nothing extra to sign | Archive 20.8 MB; static libraries total 95.9 MB, of which ONNX Runtime 75.1 MB (measured). Final linked size **unverified** (the linker drops unused code) | Official crate `sherpa-onnx` 2.0.1, Apache-2.0, updated Oct 27, 2026, about 686k downloads ([crates.io](https://crates.io/crates/sherpa-onnx)). Community `sherpa-rs` 0.6.8 (MIT) last updated Oct 2025 | **Safe to ship.** Pin the archive's SHA-256 or build it yourself |
| **ort** (pykeio) | MIT OR Apache-2.0 ([crates.io](https://crates.io/crates/ort)); ONNX Runtime MIT | None: we would write VAD, powerset decoding, overlap-add, embedding windows and clustering | Static linking supported; downloads prebuilt ONNX Runtime unless you supply one; CoreML execution provider available | Same ONNX Runtime cost as above | `ort` 2.0.0-rc.15 (still a release candidate), about 23.7M downloads | Safe to ship; **most work** |
| speakrs (alternative) | Apache-2.0 ([crates.io](https://crates.io/crates/speakrs)) | Full community-1 pipeline in Rust, including PLDA and VBx; CoreML backend with no ONNX Runtime. Author reports 7.1% DER on VoxConverse dev at 529× real time on an M4 Pro (not checked) | CoreML, native | **Unverified** | 0.7.0, about 9k downloads, one maintainer (young) | Code safe; its model mirror has no licence (host converted community-1 files yourself, CC-BY) |
| NeMo-Speech.cpp (for Nemotron-3-Diarization) | Apache-2.0 ([LICENSE](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/LICENSE)); ggml MIT; SentencePiece needed; KenLM (LGPL) only with the optional Flashlight decoder, off by default ([notices](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/THIRD_PARTY_NOTICES.md)) | `nemo_speech/diar.h` C ABI, offline and streaming | Metal builds documented; release archives ship **shared** libraries (static **unverified**) | **Unverified** | None: write FFI to the C ABI | Safe to ship if KenLM stays off; **macOS diarization unverified** |

**Recommendation for spike S7:** build with `sherpa-onnx` (static) + segmentation-3.0 + CAM++ English VoxCeleb. In the same spike, run Nemotron-3-Diarization through NeMo-Speech.cpp on the same recordings; switch only if it is clearly better on 5–6-speaker meetings and its macOS build works.

## B. Text models per Mac

Qwen3.5-4B, Qwen3.5-9B and Gemma 4 26B-A4B fit 8, 16 and 32 GB Macs with room for CaptureMann itself, and all are Apache-2.0. Peak memory is **computed, not measured on a Mac**: file + KV cache at 16,384 tokens (f16) + about 0.8–1 GB of working buffers, with one slot (`llama-server -np 1`; the default is 4 slots).

| Tier | Model (released) | Exact GGUF file | Size | KV at 16K | Est. peak | Weights licence (where read) | Vietnamese | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **8 GB pick** | Qwen3.5-4B (Feb 2026) | `lmstudio-community/Qwen3.5-4B-GGUF` → `Qwen3.5-4B-Q4_K_M.gguf` | 2.71 GB | 0.54 GB (only 8 of 32 layers keep a KV cache) | \~4.1 GB | Apache-2.0 ([LICENSE](https://huggingface.co/Qwen/Qwen3.5-4B/blob/main/LICENSE)) | Card claims 201 languages; good in our smoke test | **Safe to ship** |
| 8 GB alt | Phi-4-mini-instruct (Feb 2025) | `unsloth/Phi-4-mini-instruct-GGUF` → `Phi-4-mini-instruct-Q4_K_M.gguf` | 2.49 GB | 2.15 GB | \~5.2 GB | MIT ([LICENSE](https://huggingface.co/microsoft/Phi-4-mini-instruct/resolve/main/LICENSE)) | **Not in its supported-language list** | Safe to ship; weak fit |
| 8 GB alt | Gemma 4 E4B-it (Mar 2026) | `ggml-org/gemma-4-E4B-it-GGUF` → `gemma-4-E4B-it-Q4_0.gguf` | 4.59 GB | ≤ 0.5 GB | \~5.9 GB | Apache-2.0 ([Gemma 4 licence](https://ai.google.dev/gemma/docs/gemma_4_license)) | 35+ supported, 140+ pre-trained; Vietnamese not named | Safe to ship; too big for 8 GB alongside the app |
| **16 GB pick** | Qwen3.5-9B (Feb 2026) | `lmstudio-community/Qwen3.5-9B-GGUF` → `Qwen3.5-9B-Q4_K_M.gguf` | 5.63 GB | 0.54 GB | \~7.1 GB | Apache-2.0 ([LICENSE](https://huggingface.co/Qwen/Qwen3.5-9B/blob/main/LICENSE)) | As 4B; stronger | **Safe to ship** |
| 16 GB alt | Gemma 4 12B-it (May 2026) | `google/gemma-4-12B-it-qat-q4_0-gguf` → `gemma-4-12b-it-qat-q4_0.gguf` (official) | 6.98 GB | \~0.6 GB (sliding-window layers) | \~8.6 GB | Apache-2.0 ([Gemma 4 licence](https://ai.google.dev/gemma/docs/gemma_4_license)) | As E4B | **Safe to ship**; compare in S8 |
| 16 GB alt | Ministral 3 8B Instruct 2512 (Dec 2025) | `mistralai/Ministral-3-8B-Instruct-2512-GGUF` → `Ministral-3-8B-Instruct-2512-Q4_K_M.gguf` (official) | 5.20 GB | 2.28 GB | \~8.2 GB | Apache-2.0 ([card](https://huggingface.co/mistralai/Ministral-3-8B-Instruct-2512)) | Vietnamese not in its listed languages | Safe to ship; weak fit |
| **32 GB pick** | Gemma 4 26B-A4B-it (Mar 2026; MoE, 4B active) | `ggml-org/gemma-4-26B-A4B-it-GGUF` → `gemma-4-26B-A4B-it-Q4_0.gguf` | 14.6 GB | \~0.55 GB | \~16.2 GB | Apache-2.0 ([Gemma 4 licence](https://ai.google.dev/gemma/docs/gemma_4_license)) | As E4B | **Safe to ship**; fast for its size |
| 32 GB alt | Qwen3.8-27B (Aug 2026; dense) | `ggml-org/Qwen3.8-27B-GGUF` → `Qwen3.8-27B-Q4_K_M.gguf` | 18.97 GB | 1.07 GB | \~21.2 GB | Apache-2.0 ([card](https://huggingface.co/Qwen/Qwen3.8-27B)) | Qwen family; untested | Safe to ship; tight on 32 GB and slower (dense) |
| 32 GB alt | Qwen3.6-35B-A3B (Apr 2026; MoE, 3B active) | `ggml-org/Qwen3.6-35B-A3B-GGUF` → `Qwen3.6-35B-A3B-Q4_K_M.gguf` | 20.42 GB | 0.34 GB | \~21.8 GB | Apache-2.0 ([LICENSE](https://huggingface.co/Qwen/Qwen3.6-35B-A3B/blob/main/LICENSE)) | Qwen family; untested | Safe to ship; tight on 32 GB |
| Excluded | Llama 3.1/3.2/4 | — | — | — | — | Llama Community License, gated ([card](https://huggingface.co/meta-llama/Llama-3.1-8B-Instruct)) | — | **Not Apache/MIT: needs a lawyer** |
| Excluded | Qwen3.8-Flash-Next | — | — | — | — | "qwen-community-1.0" ([card](https://huggingface.co/Qwen/Qwen3.8-Flash-Next)); 125B total | — | **Needs a lawyer**; too big anyway |

**Converted GGUF files keep the original licence** (the repos above tag Apache-2.0 or MIT). Mirroring them is allowed: keep the licence file and a NOTICE naming the source model. On 32 GB Macs, macOS limits how much memory the GPU may use by default; whether \~21 GB fits next to CaptureMann is **unverified**, which is why the 32 GB pick is the 14.6 GB Gemma model.

**One measurement (not a Mac):** Qwen3.5-4B Q4\_K\_M in a static CPU build of `llama-server` on Linux with 16K context and 4 slots peaked at 5.3 GB resident. That build also keeps a CPU-optimised copy of the weights, so a Metal build should use less.

## B2. Smaller models without reasoning (tested Oct 27)

**Gemma 4 E2B-it is the best fit for transcript tasks:** in our test it got every decision, action item and owner right in both English and Vietnamese, from a 2.84 GB file. That is about half the memory of the earlier 16 GB pick. Reports, minutes and follow-up actions only extract and rephrase what the transcript says, so models of this size are enough; reasoning ("thinking") was turned off for every model.

**Test.** Each model ran three tasks: English meeting minutes, Vietnamese meeting minutes (both as schema-constrained JSON), and translating a 6-line English excerpt into Vietnamese. Setup: same prompts, temperature 0.2, `llama-server` CPU build on Linux, 2 cores, 16K context, one slot. The answer key has 4 items per meeting: decisions, actions with owners, and open questions. One short sample per task, so treat this as a screen, not a verdict.

| Model (GGUF file, licence) | File | Est. peak on Mac (16K) | Peak RSS in test (CPU) | EN minutes (of 4) | VI minutes (of 4) | EN→VI translation | Mistakes seen | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **Gemma 4 E2B-it** (`ggml-org/gemma-4-E2B-it-GGUF` → `gemma-4-E2B-it-Q4_0.gguf`, Apache-2.0) | 2.84 GB | \~3.7 GB | 4.0 GB | **4** | **4** | Good | Two redundant open questions; one timestamp on the line before the commitment | **Pick** for every Mac |
| **Qwen3.5-4B** (`lmstudio-community/Qwen3.5-4B-GGUF` → `Qwen3.5-4B-Q4_K_M.gguf`, Apache-2.0) | 2.71 GB | \~4.1 GB | 5.3 GB | 4 | 3 | **Best** | Missed Hung's "ask HR" action; one junk open question (`"at"`) | Second choice; best for translation |
| Qwen3.5-2B (`lmstudio-community/Qwen3.5-2B-GGUF` → `Qwen3.5-2B-Q4_K_M.gguf`, Apache-2.0) | 1.27 GB | \~2.1 GB | 2.3 GB | 3 | 3 | Good | Turned An's release-notes task into an open question; wrong timestamp on the launch decision | Fallback for very low memory |
| Ministral 3 3B (`mistralai/Ministral-3-3B-Instruct-2512-GGUF` → `…Q4_K_M.gguf`, Apache-2.0) | 2.15 GB | \~4.5 GB (full-attention KV 1.7 GB) | 5.5 GB | 4 | 2 | Fair ("tính chất" for "feature") | Missed the Vietnamese decision, invented an open question | Not recommended |
| Qwen3.5-0.8B (`lmstudio-community/Qwen3.5-0.8B-GGUF` → `Qwen3.5-0.8B-Q4_K_M.gguf`, Apache-2.0) | 0.53 GB | \~1.0 GB | 1.3 GB | 3 | 1 | Poor (dropped timestamps, "release" became "develop") | Wrong timestamps, missed most Vietnamese items | Not good enough |

Relative speed on the same CPU (seconds for the three tasks together): 0.8B 40, 2B 89, Gemma 4 E2B 134, Ministral 3B 153, Qwen3.5-4B 194. On a Mac with Metal all are much faster; Gemma 4 E2B runs about 1.5× faster than Qwen3.5-4B here.

**Checked and left out:** Qwen3-4B-Instruct-2507 (Apache-2.0, no thinking mode, but every layer keeps a KV cache: 2.4 GB at 16K); Phi-4-mini (MIT; Vietnamese not supported); IBM Granite 4.2 3B (Apache-2.0; built for reasoning, Vietnamese not among its tested languages); Liquid LFM2.5 ("lfm1.0" licence, not Apache/MIT).

**Still to check in S8:** long transcripts (30–90 minutes, about 6,000–18,000 tokens), where the chunk-and-merge step and the citation checks matter more than in a 6-line sample. Gemma 4 E2B keeps a very small KV cache (sliding-window and shared layers), so 32K context stays cheap if a single pass is better than chunking.

## B. Vietnamese, structured JSON and current names

Grammar-constrained output gives valid JSON every time but not correct content; Vietnamese evidence is thin and comes from vendor claims plus one smoke test, so S8 must score it.

| Question | Finding | Evidence | Confidence |
| --- | --- | --- | --- |
| Vietnamese support | Qwen3.5 claims 201 languages and reports multilingual scores (MMLU-ProX, WMT24++, INCLUDE); Gemma 4 claims 35+ supported and 140+ pre-trained languages. Neither reports Vietnamese on its own. Phi-4-mini and Ministral 3 don't list Vietnamese | Model cards | Vendor claims only |
| EN → VI translation, smoke test | Qwen3.5-4B Q4\_K\_M translated a 6-line meeting excerpt into fluent, correct Vietnamese, keeping names and timestamps (`[00:00:21] Minh: Tôi nghĩ ngày 20 tháng 11 là khả thi nếu tính năng ghi âm được đóng băng vào ngày 10 tháng 11.`) | Run in this session (llama.cpp CPU build, thinking off) | One sample |
| Summarisation, smoke test | Same model, meeting minutes with a JSON schema: valid JSON on the first try, but it **filed the open question as a decision** and **turned "An needs the feature list" into an action for An** | Same run | One sample |
| Structured JSON | `llama-server` accepts `response_format` with a JSON schema (OpenAI style) or a `json_schema` field, and turns it into a GBNF grammar, so output always parses. Known limits: `additionalProperties` defaults to false; no `if/then/else`, `not`, `uniqueItems`, nested `$ref`; `minimum/maximum` for integers only. The schema is not shown to the model, so describe it in the prompt too | [server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), [grammars README](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md) | Documented + tested |
| Current names (Oct 27, 2026) | Qwen3.5 (Feb), Qwen3.6 (Apr), **Qwen3.8-27B (Aug)**; **Gemma 4** E2B/E4B/12B/26B-A4B/31B (Mar–May); Phi-4-mini-instruct (Feb 2025, still the newest mini); Ministral 3 3B/8B/14B "2512"; Llama 3.x/4 (custom licence). Your Oct 16 table's names were right but missed Qwen3.8 and Gemma 4's switch to Apache-2.0 | Hugging Face API listings | Checked |

**Implication for AI-9:** keep the spec's guards (citations must match transcript lines, owners must be named speakers) and add a check that an item listed under decisions isn't phrased as a question.

## B. llama.cpp build for macOS arm64 + Metal

llama.cpp and ggml are MIT, and a static `llama-server` builds with CMake alone; we confirmed the build without Python on Linux, while the macOS Metal build follows the documented defaults and is not yet run.

| Item | Finding | Where read | Verdict |
| --- | --- | --- | --- |
| llama.cpp licence | MIT, "Copyright (c) 2023-2026 The ggml authors" | [LICENSE](https://github.com/ggml-org/llama.cpp/blob/master/LICENSE) | Safe to ship |
| GGML licence | MIT (same text; ggml is vendored in llama.cpp) | [ggml LICENSE](https://github.com/ggml-org/ggml/blob/master/LICENSE) | Safe to ship |
| Bundled third-party code in `llama-server` | cpp-httplib (MIT), nlohmann/json (MIT), miniaudio (public domain or MIT-0), stb\_image, sheredom `subprocess.h` (Unlicense, like `whisper-rs` already in your `deny.toml`), hash helpers | `vendor/` in the repo at commit `7aca0d2` (Oct 26, 2026) | Safe to ship; list them in THIRD\_PARTY\_LICENSES (stb and the hash helpers' exact licences **unverified**) |
| Static build | `cmake -B build -DBUILD_SHARED_LIBS=OFF -DLLAMA_OPENSSL=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF` then `cmake --build build --target llama-server` | [docs/build.md](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md) | — |
| Metal | On by default on macOS (`GGML_METAL_DEFAULT ON`); `GGML_METAL_EMBED_LIBRARY` defaults to on with Metal, so the shaders are compiled into the binary and there is no separate `.metallib` to ship or sign | `ggml/CMakeLists.txt` | — |
| No Python | Configure and build used only CMake and a C++ compiler; this session built a 19.9 MB static `llama-server` (Linux x86-64, CPU) in 5 min 52 s on 2 cores. **macOS arm64 Metal build not run here** | Build log in this session | Verified on Linux only |
| OpenSSL | `LLAMA_OPENSSL=OFF` removes it; CaptureMann only talks to loopback, so HTTPS is not needed | docs/build.md | Turn it off |
| In-process option | The `llama-cpp-2` crate (MIT OR Apache-2.0, 0.1.160, Oct 15, 2026) links llama.cpp into the app instead of running a server process | [crates.io](https://crates.io/crates/llama-cpp-2) | Safe to ship; decide in S8 |

**Signing note:** a separate `llama-server` binary inside the `.app` must be signed with the same Developer ID and the hardened runtime, like the FFmpeg sidecar; a statically linked crate avoids that step.

## Sources

Read Oct 27, 2026. Model licences come from each model card's metadata and LICENSE file through the Hugging Face API; code licences from each project's LICENSE file. This is a technical licence check, not legal advice.

**Diarization models**

- [pyannote/segmentation-3.0](https://huggingface.co/pyannote/segmentation-3.0) (MIT, gate text, training data)
- [onnx-community/pyannote-segmentation-3.0](https://huggingface.co/onnx-community/pyannote-segmentation-3.0) (MIT, ONNX files)
- [pyannote/speaker-diarization-community-1](https://huggingface.co/pyannote/speaker-diarization-community-1) (CC-BY-4.0, gate)
- [Wespeaker/wespeaker-voxceleb-resnet34](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34) and [WeSpeaker pretrained.md](https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md) (licence follows the dataset)
- [VoxCeleb licence.txt](https://mm.kaist.ac.kr/datasets/voxceleb/files/license.txt) and [VoxCeleb page](https://mm.kaist.ac.kr/datasets/voxceleb/)
- [3D-Speaker LICENSE](https://github.com/modelscope/3D-Speaker/blob/main/LICENSE); ModelScope records `iic/speech_campplus_sv_en_voxceleb_16k`, `iic/speech_eres2net_sv_en_voxceleb_16k` (Apache License 2.0)
- [NVIDIA TitaNet-large](https://huggingface.co/nvidia/speakerverification_en_titanet_large), [Sortformer v1](https://huggingface.co/nvidia/diar_sortformer_4spk-v1), [Streaming Sortformer v2](https://huggingface.co/nvidia/diar_streaming_sortformer_4spk-v2), [v2.1](https://huggingface.co/nvidia/diar_streaming_sortformer_4spk-v2.1)
- [NVIDIA Nemotron-3-Diarization](https://huggingface.co/nvidia/nemotron-3-diarization) and [OpenMDW-1.1](https://openmdw.ai/license/1-1/)

**Runtimes**

- [sherpa-onnx LICENSE](https://github.com/k2-fsa/sherpa-onnx/blob/master/LICENSE), [sherpa-onnx crate](https://crates.io/crates/sherpa-onnx) (build script read from the 2.0.1 package)
- [ONNX Runtime LICENSE](https://github.com/microsoft/onnxruntime/blob/main/LICENSE), [ort crate](https://crates.io/crates/ort), [speakrs crate](https://crates.io/crates/speakrs), [pyannote.audio LICENSE](https://github.com/pyannote/pyannote-audio/blob/main/LICENSE)
- [NeMo-Speech.cpp LICENSE](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/LICENSE), [third-party notices](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/THIRD_PARTY_NOTICES.md), [build docs](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/build.md)

**Text models and llama.cpp**

- [Qwen3.5-4B LICENSE](https://huggingface.co/Qwen/Qwen3.5-4B/blob/main/LICENSE), [Qwen3.5-9B LICENSE](https://huggingface.co/Qwen/Qwen3.5-9B/blob/main/LICENSE), [Qwen3.6-35B-A3B LICENSE](https://huggingface.co/Qwen/Qwen3.6-35B-A3B/blob/main/LICENSE), [Qwen3.8-27B](https://huggingface.co/Qwen/Qwen3.8-27B), [Qwen3.8-Flash-Next](https://huggingface.co/Qwen/Qwen3.8-Flash-Next)
- [Gemma 4 licence (Apache 2.0 text)](https://ai.google.dev/gemma/docs/gemma_4_license), [gemma-4-12B-it](https://huggingface.co/google/gemma-4-12B-it)
- [Phi-4-mini-instruct LICENSE](https://huggingface.co/microsoft/Phi-4-mini-instruct/resolve/main/LICENSE), [Ministral-3-8B-Instruct-2512](https://huggingface.co/mistralai/Ministral-3-8B-Instruct-2512), [Llama-3.1-8B-Instruct](https://huggingface.co/meta-llama/Llama-3.1-8B-Instruct)
- GGUF repos: [lmstudio-community/Qwen3.5-4B-GGUF](https://huggingface.co/lmstudio-community/Qwen3.5-4B-GGUF), [lmstudio-community/Qwen3.5-9B-GGUF](https://huggingface.co/lmstudio-community/Qwen3.5-9B-GGUF), [ggml-org/gemma-4-26B-A4B-it-GGUF](https://huggingface.co/ggml-org/gemma-4-26B-A4B-it-GGUF), [google/gemma-4-12B-it-qat-q4\_0-gguf](https://huggingface.co/google/gemma-4-12B-it-qat-q4_0-gguf), [ggml-org/Qwen3.8-27B-GGUF](https://huggingface.co/ggml-org/Qwen3.8-27B-GGUF)
- [llama.cpp LICENSE](https://github.com/ggml-org/llama.cpp/blob/master/LICENSE), [ggml LICENSE](https://github.com/ggml-org/ggml/blob/master/LICENSE), [build docs](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md), [grammars README](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md), [server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
