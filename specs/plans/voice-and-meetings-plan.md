# Voice recording, speaker labels and AI reports

Status (Oct 27, 2026): planned Oct 26, 2026 (maintainer request); **Stages 0 to 6 are built for release 0.2.0** (tested with fakes and real engines through the CLI on one meeting; hardware checks pending, see `../../docs/project/status.md`). Spike S7: technical Go, accuracy open. Spike S8: resolved by the research and tests (Gemma 4 E2B), with Vietnamese and long-recording quality to keep checking. Behaviour and acceptance criteria live in `../features/15-voice.md` (VOC-1 to VOC-3) and `../features/07-ai.md` (AI-5, AI-7, AI-8, AI-9); this plan records the approach, the model choices and the spikes.

## Why

Recording a meeting, an interview, a one-on-one or a product presentation needs no screen: a full audio recording, a transcript with who said what, and a written report is the job. CaptureMann already has every hard part except the speaker labels and the text model: the audio tracks on one clock with crash-safe WAV masters (AUD-1, AUD-2), Whisper transcripts (AI-1), caption editing and glossary (AI-6a), the library and agent tools.

## Maintainer decisions (Oct 26, 2026)

- Audio-only recording is a mode of the existing recorder (`start_recording { mode: "audio" }`), not a separate app; bundles stay `.cmrec` with `kind: "audio"`.
- Speaker labels come from the **voices** (diarization), not from which device heard them: one laptop on the table in an interview must work. The mic-vs-system shortcut is an opt-in for headset calls only.
- Reports are evidence with timestamps. Interview notes never score or recommend.

## Order of work (no release dates)

The maintainer decided on Oct 26, 2026 that there is no release pressure: features are built in this order and a release is cut when the maintainer is satisfied. "Next" in the specs means the next feature in this order, not a date.

| Stage | Ships | Needs |
| --- | --- | --- |
| 1 | VOC-1 audio recording, VOC-2 audio editor and M4A/WAV export, AI-5 translation **to English via Whisper**, `large-v3-turbo` and `large-v3` in the model catalogue | No new runtime |
| 2 | AI-7 text model provider (the built-in engine; Ollama / LM Studio and remote are later), AI-9 reports, AI-5 translation to any language | Spike S8 |
| 3 | AI-8 speaker labels | Spike S7 (model and licence), `ort` runtime |
| Later (P2) | VOC-3 live transcript, embedded llama.cpp, custom report templates, per-app audio capture | — |

## Models

### Speech to text (whisper.cpp, already in `cm-ai`)

| Model | Use | Notes |
| --- | --- | --- |
| `large-v3-turbo` (~1.6 GB) | Default recommendation for transcription on 16 GB+ Macs | Near large-v3 accuracy, much faster; **not trained for translation** (returns the source language) |
| `large-v3` (~3.1 GB) | Translation to English; best accuracy | Used automatically for AI-5 Whisper-translate when installed |
| `small` / `base` | 8 GB Macs; VOC-3 live transcript | Already in the catalogue |

Pin both new files (URL at a Hugging Face commit, SHA-256, size, `ram_mb` from the whisper.cpp README) in `crates/cm-ai/data/models.json`.

### Text model (AI-7: through a local server, not shipped)

The built-in model list is three models for every Mac (maintainer decision Oct 27, 2026: reports only extract and rephrase what the transcript says, so they need no reasoning and no big models). Only Apache-2.0 or MIT licences. Pins: `docs/research/text-model-pins-2026-10-27.md`; test results: `docs/research/local-model-and-licence-check-2026-10-27.md`.

| Model | Role | Licence | Notes |
| --- | --- | --- | --- |
| **Gemma 4 E2B** | **Default, recommended on every Mac** | Apache-2.0 | Best score in the small-model test, English and Vietnamese |
| Qwen3.5 4B | Optional: best for translation into other languages | Apache-2.0 | Needs about 4 GB of memory |
| Qwen3.5 2B | Smallest, for Macs with little free memory | Apache-2.0 | Notes can miss items |

The default is the one marked `recommended` in the catalogue; S8's remaining runs (Vietnamese quality on real recordings) can still change it.

### Speaker diarization (AI-8: decided by S7)

| Candidate | Parts | Licence to check | Notes |
| --- | --- | --- | --- |
| A. Segmentation + embedding + clustering, ONNX | pyannote `segmentation-3.0` + a WeSpeaker or 3D-Speaker embedding model, run through `ort` (or `sherpa-onnx` / `pyannote-rs`) | Segmentation MIT; embedding models Apache-2.0 (verify each file at pinning) | Rust crates exist; small downloads; clustering in our code |
| B. pyannote `speaker-diarization-community-1` | Same pipeline family with VBx clustering | CC-BY-4.0 (attribution); gated on Hugging Face (accepting terms), so CaptureMann would mirror the files with attribution or ask the user to import them | Better reported accuracy than pyannote 3.1 on most benchmarks (for example AMI single-mic DER 19.9%) |

## Spikes

**S7 — Speaker diarization on a Mac (Rust, offline). Status Oct 27, 2026: technical Go (sherpa-onnx with pyannote segmentation-3.0 and CAM++, shipped inside the app); accuracy not measured, to be tested with users and the team over the coming weeks. `docs/project/spikes.md`.** Run candidates A and B on: a 10-minute two-person interview on the MacBook's built-in mic, a 30-minute four-person meeting around a laptop, and one public meeting set with a CC-BY licence. Measure word attribution, DER, time and peak memory on the M4 Pro and on a 16 GB machine. **Pass:** AI-8.AC1 and AC2 targets, ≤ 600 MB peak memory, a licence that allows mirroring. Record the result in `docs/project/spikes.md`.

**S8 — Text model for reports and translation. Status Oct 27, 2026: resolved by the Cowork research (`docs/research/`) and the tests: Gemma 4 E2B is the default; Qwen3.5 4B and 2B and Gemma 4 26B-A4B are the other choices; Vietnamese and long-recording quality are to keep checking.** With the built-in engine (`llama-server`), run the AI-9 templates and AI-5 EN↔VI translation on 3 meetings, 2 interviews and 1 lecture (English and Vietnamese) with the four models above at Q4. Score by hand: citation accuracy, invented items, Vietnamese quality, structured-output success, time. **Pass:** ≥ 95% schema success, no invented action items in the review set, report ≤ 5 minutes for 30 minutes of speech on 16 GB. The winner becomes the AI page's default recommendation.

## Decisions made after the plan

- **Masters stay WAV** (maintainer, Oct 26, 2026). About 1.4 GB per hour for the mic with its denoised copy; the disk estimate in the start sheet says so. FLAC masters or a "Compact this recording" action are P2, to be decided if disk use becomes a complaint.
- **WAV export needs two FFmpeg additions** found on Oct 26, 2026 while checking `packaging/ffmpeg/build-macos.sh`: the `wav` muxer and the `pcm_s24le` encoder are not in the lists (`aac`, the `mp4` muxer, `loudnorm`, `ebur128`, `amix`, `pan` are). Add them with the VOC-2 work and rebuild with `make ffmpeg`.
- **Code gaps found Oct 26, 2026:** `CaptureKind` in `cm-library` has only `Screenshot` and `Recording`; `manifest.markers` and `add_marker` do not exist; code that reads the `screen` track assumes it is present (`index.rs`, `rec_edit.rs`); `ort` is not a dependency.

## Open questions

- **Embedded llama.cpp** (no separate server) once AI-7 proves the features: shares ggml with whisper.cpp, but adds multi-GB downloads and model-licence handling in the app. **Decided Oct 27, 2026:** it ships as AI-7's `embedded` provider, as a helper program (`llama-server`, MIT) in the bundle rather than a library linked into the app, so the two ggml copies never meet; see AI-7 "Built-in engine".
- **Display name** for "Me" (AI-8.AC7): a new `profile.display_name` setting, or the macOS account name.

## Sources (checked Oct 26, 2026)

- [Best Local LLMs 2026 (updated Oct 16, 2026)](https://omidsaffari.com/blog/best-local-llms-2026): model sizes, context, licences, Q4 memory
- [Hugging Face: open-source LLMs to run locally in 2026](https://huggingface.co/blog/daya-shankar/open-source-llm-models-to-run-locally)
- [Whisper turbo vs large: translation](https://instagit.com/openai/whisper/how-turbo-whisper-model-differs-large-model): turbo not trained for translation (confirm against OpenAI's README when pinning)
- [pyannote speaker-diarization-community-1](https://huggingface.co/pyannote/speaker-diarization-community-1): CC-BY-4.0, gated, components and DER table
- [pyannote-rs](https://github.com/doc/pyannote-rs) and [sherpa-rs](https://github.com/BenLocal/sherpa-rs): Rust bindings for ONNX diarization
