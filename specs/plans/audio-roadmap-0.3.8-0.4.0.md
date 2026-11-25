# Audio module: review and roadmap, 0.3.8 to 0.4.0 (proposal, Nov 25, 2026)

Status: **proposal for the maintainer**, not a plan of record. It is based on `main` at 0.3.7 (`03832140`): the
crates `cm-audio` (8.4k lines) and `cm-ai`, the specs `05-audio`, `15-voice` and `07-ai`, the 0.2.7 transcription
review brief, the release plans up to 0.3.7, the CHANGELOG and the reports in `verification/benchmarks/`. Nothing
here is built or measured. Any number given as a target still needs a stored report, as the project requires.

## 1. Where the audio module stands (0.3.7)

| Area | State | Evidence |
| --- | --- | --- |
| **Capture** | Mic through `cpal`, 48 kHz 32-bit float. RNNoise copy (`mic.denoised.wav`). System audio through ScreenCaptureKit on macOS, WASAPI *process loopback* on Windows (the app's own sound excluded), PipeWire monitor on Linux. Session-clock alignment, pause without holes, mute, device-change handling, `kill -9` recovery | VOC-1 hardware script PASS, including a 3 h soak (Oct 27) |
| **Audio-only mode** | Five presets, markers, consent line, "How many people?", 4 h default and 12 h maximum | VOC-1, VOC-2 |
| **Transcription** | whisper.cpp 1.8.3 (patched), pieces of 300 s, loop, broken-token and coverage re-reads, a repeated-sentence rule, glossary, Silero VAD with a `VadFirst` wrapper, a doubtful-line guard, `[unclear]`, partial results during long jobs | Review brief and 10+ benchmark reports |
| **Speakers** | sherpa-onnx 2.0.1: pyannote seg-3.0 and CAM++ zh-en; threshold 1.0; windows of about 10 min linked by voice prints over 20 min; `fold_to_n` | DER 17.1 % on 6 held-out English clips; **no Vietnamese DER**; real Vietnamese meeting over-counts (7–8 found for 6) |
| **Text model** | Gemma 4 E2B through `llama-server`; 6 report templates with citations; translation; normalisation (AI-10) | Benchmarks Oct 30 and Nov 3 |
| **Live mode** | Built (`pieces/live.rs`, `daemon/ai/live.rs`), **off by default, 20-minute hardware check never done** | CHANGELOG 0.2.x |
| **GPU** | Metal on macOS only. **Windows and Linux transcribe on CPU** (`whisper-rs` without `vulkan` or `cuda`) | `crates/cm-ai/Cargo.toml` |

### Main weaknesses

| # | Weakness | Why it matters |
| --- | --- | --- |
| W1 | **Memory is not flat.** The whole file is decoded into memory, and the speaker job more than doubles the peak: 11.7 GB for the 4 h 14 min Vietnamese meeting. "Windowed audio" has been carried over from 0.3.2 to 0.3.7 with no release | A 16 GB laptop, which is the common case on Windows, can't safely do long meetings with speakers |
| W2 | **Windows and Linux are CPU-only** for Whisper | Most Windows laptops have an iGPU or an NVIDIA GPU that sits idle; the wait after Stop is the most visible cost |
| W3 | **Hallucination guards are partly Vietnamese-only** (AC12 rule), and **D4 is still open**: text invented over audio that holds only room noise | A business transcript with "Subscribe to the channel" in it loses trust at once |
| W4 | **Speaker quality is unmeasured on real meetings and on Vietnamese.** Clustering is one threshold, and turns aren't aligned to words | "29 speakers" history; mid-sentence speaker flips make the minutes read badly |
| W5 | **Echo between tracks.** In an online meeting played on speakers, the mic also records the remote voices that are already on the system track | Double words in a mixed transcript, and remote people labelled as the local speaker |
| W6 | **Disk use.** Masters are 32-bit float WAV, plus a denoised copy: about 1.4 GB per hour of mic. Compact masters were deferred as P2 | A week of meetings fills a small SSD; it slows uploads and backups |
| W7 | **Live transcript is built but never shipped** (gate not run). VOC-3 is still a draft | The one thing Otter does that CaptureMann doesn't (competitor review) |
| W8 | **Specs lag the code.** AUD-1 says "Windows never run" and AUD-2 says "Windows loopback never run", yet 0.2.8 onwards ships and records on Windows | Wrong statuses hide real gaps |
| W9 | **No quality gate per release** for WER and DER; numbers are one-off reports | A change that helps English can silently hurt Vietnamese, as happened in the speaker retune |

## 2. Proposal at a glance

| Release | Theme | Headline items |
| --- | --- | --- |
| **0.3.8** | Make long audio safe and measured | Flat memory (A1), a quality gate (A2), the D4 rule for every language (A3), spec status hygiene (A4), the live-mode hardware gate (A5) |
| **0.3.9** | Faster and cleaner audio | GPU Whisper on Windows and Linux (B1), echo removal from the mic (B2), compact masters (B3), speaker clean-up on words and better clustering (B4), voice clean-up on export (B5) |
| **0.4.0** | Meetings, live | Live transcript as Preview (C1), one-app audio capture (C2), meeting start detection (C3), remembered voices (C4), ask your recordings (C5), a slot for the CraftMann Vietnamese model (C6) |

One theme per release keeps each regression run small. Everything in 0.3.8 is a precondition: without flat memory
and a quality gate, the 0.3.9 and 0.4.0 changes can't be judged safely.

## 3. 0.3.8: make long audio safe and measured

### A1. Flat memory for long audio ("windowed audio", carried since 0.3.2)
- **Decode by window, not the whole file.** FFmpeg reads at most a piece plus margin (300 s ± 20 s, so about 20 MB of 16 kHz mono `f32`), and the piece cutter asks for the quietest 100 ms through a small level index built in one streaming pass.
- **Speakers by window, always.** The windowed path already exists (`speaker_engine/windowed.rs`). Use it for all lengths, so memory depends on the window and not on the file. Linking by voice prints (already used over 20 min) keeps "Speaker 2" the same in hour 1 and hour 3; keep the whole-file run as a measured comparison only.
- **Never run the transcript and the speakers at full size together.** Run them in sequence, or share one decoded window between them.
- **Target, to be confirmed by a stored report:** a peak under 3 GB for a 4-hour recording with speakers on a 16 GB machine, at the same WER and DER (within ±0.5 points) as today on the existing long-audio reports.
- **AC sketch:** the 4 h 14 min benchmark re-run with speakers keeps the peak under 3 GB, and DER on the held-out English clips is unchanged within 0.5 points.

### A2. An audio quality gate in `cm-regress`
- **Public, reproducible sets** (licences checked before use):
  - **English:** the held-out clips already used (PriMock57, CC BY 4.0) plus AMI meeting excerpts (CC BY 4.0).
  - **Vietnamese:** FLEURS vi_vn (CC BY 4.0), made far-field with room impulse responses, room noise and overlapping speakers (a synthetic far-field mix, as the review brief asked).
  - **Hallucination probes:** room-only clips, muted stretches and music beds, with the expected result "no words".
- **Reported per release:** WER, DER, the speaker-count error, words invented per hour of non-speech, time against real time, and peak memory, per system.
- **The gate fails** when a number gets worse by more than a set margin against the last release. The reports go to `verification/benchmarks/` as today.
- **Private data:** the maintainer's real meetings stay private. The gate script can read a local folder of reference transcripts if one is present, and never stores their content.

### A3. One hallucination rule for every language (closes D4)
- **Combine signals already computed:** the Silero speech share (`silero.rs`), the level against the room floor, Whisper's no-speech probability, average log-prob, and words per second.
- **Room-only audio has no speech reference.** Use the Silero share as the reference instead: a line over a stretch where Silero finds under X % speech and the level is within Y dB of the floor is dropped, or written as `[noise]`. X and Y are measured on the A2 probes and stored with their numbers, as the 0.3.7 plan required.
- **Remove the Vietnamese-only limit** on the AC12 rule once A2 shows that English loses no real words (a target of no more than 0.2 % of real words removed).
- **Still no phrase lists** (a project rule).

### A4. Spec and status hygiene
- Update AUD-1, AUD-2, AUD-3 and AUD-6 with the Windows and Linux facts as shipped (process loopback, PipeWire, the 0.2.8 and 0.3.0 hardware runs), and move VOC-3 from Draft to a real spec (C1).
- Write down what "windowed audio" means (A1), so the item stops being carried over without a definition.

### A5. Run the live-mode hardware gate
- Run the 20-minute check on macOS (M-series and one Intel Mac), Windows (the ThinkPad) and Linux (X11 and Wayland). Measure CPU, memory, dropped live pieces, and the time from Stop to the final transcript.
- **No user-facing change** in 0.3.8. It unblocks C1.

**Size:** A1 about 4–5 days, A2 about 3 days, A3 about 2 days, A4 half a day, A5 1 day of hardware time.

## 4. 0.3.9: faster and cleaner audio

### B1. GPU transcription on Windows and Linux
- Build `whisper-rs` with **Vulkan** (MIT, in whisper.cpp; works on NVIDIA, AMD and Intel GPUs) on Windows and Linux, with a **CPU fallback** when no usable device exists, when the device has too little memory, or when the first piece fails.
- **Settings → AI:** show the device in use ("GPU: Intel Arc (Vulkan)" or "CPU"), and a switch to force CPU.
- **Measure before shipping:** speed and WER against the CPU build on 2–3 laptops (an iGPU, an NVIDIA dGPU, an AMD APU). Windows packaging must confirm that the Vulkan loader exists on Windows 11 (it is normally part of the GPU driver).
- CUDA is out of scope: it is a large download and NVIDIA-only.
- **Also, on all systems:** turn on flash attention where it measures faster with the same WER, and try quantised Turbo (q5_0 or q8_0) as a "Faster" model choice. Both are measured on the A2 set.

### B2. Remove the meeting's echo from the mic (a post-process)
- **The problem (W5).** On a call played through speakers, the mic records the remote voices again, so a mixed transcript has them twice and the speaker job labels them as the local person.
- **The approach.** After Stop, run acoustic echo cancellation on `mic.wav` with `system.wav` as the far-end reference. The tracks are already aligned on the session clock, and the drift is measured, so this is an offline job and needs no real-time path.
- **Candidate:** the WebRTC audio processing module (AEC3, BSD-3-Clause) through a Rust binding. Check the licence and the build on all three systems in a spike.
- **The result** is a third mic file, `mic.echo-free.wav`. Originals stay as they are. Transcription and speakers use it when the recording has both tracks and echo is detected (correlation between the tracks over a threshold).
- **AC sketch:** in a scripted call played on speakers, words from the system track no longer appear twice in a mixed transcript, and the speaker job finds the right count.

### B3. Compact masters (W6)
- **After Stop,** convert the masters to **FLAC** (lossless; 24-bit at 48 kHz is about 0.1–0.15 GB per hour for speech) and remove the WAVs when the FLAC decodes back identical to 24 bits. The denoised and echo-free copies are derived files, kept in FLAC as well.
- **While recording,** stay with WAV, for crash safety and the existing `kill -9` recovery.
- **Optionally,** write the WAV in 15-minute parts, so that a crash or a full disk loses at most one part, and recovery never repairs a 4-hour header.
- **A setting:** "Keep original WAV files" (off by default).
- Older apps can't read FLAC masters. Bump the bundle version, so an older app says "made by a newer CaptureMann" instead of failing.

### B4. Speaker clean-up, part 1
This part needs no model change.
- **Align turns to words.** Give each word the speaker it overlaps most, snap turn changes to pauses and sentence ends, and absorb turns under about 0.8 s into a neighbour. This gives one speaker at a time in the transcript, the "exclusive" view.
- **Clustering in Rust on sherpa's voice prints.** Spectral clustering with an eigengap for the speaker count when no hint is given, and VBx as a second candidate (its parameters are published under CC-BY with pyannote community-1). Keep the threshold method as the fallback.
- **Ship only what wins on the A2 gate** for both English and Vietnamese. The 0.2.7 retune improved English and hurt Vietnamese, so a per-language result is required.
- **Also:** try WeSpeaker ResNet34-LM (CC-BY-4.0) against CAM++ in the same slot. Do not use the VoxBlink2 multilingual models, whose licence is non-commercial.

### B5. Voice clean-up on export ("Clear voice")
- An option in Export audio and in the editor: a stronger, local speech enhancer than RNNoise, for example DeepFilterNet (MIT or Apache-2.0; licence and size to be checked), applied to an export copy only.
- **Transcription does not use it by default.** The brief warns that denoising may remove soft Vietnamese consonants. A2 measures whether it helps or hurts Whisper per language before it's ever used for transcripts.
- Loudness targets for the export: −16 LUFS for podcasts, −23 LUFS for broadcast. `mix/loudness.rs` already measures loudness.

**Size:** B1 3–4 days plus hardware, B2 a 1-day spike plus 3 days, B3 2–3 days, B4 4–5 days, B5 2–3 days. If time runs short, B5 moves to 0.4.0.

## 5. 0.4.0: meetings, live

### C1. Live transcript while recording (VOC-3, as Preview)
- **What exists:** the pieces are cut and read during the recording (`pieces/live.rs`).
- **What it adds:** an expandable panel on the audio HUD showing the last few lines; a click on a line adds a marker there; a "Live" pill.
- **Models:** live uses a small model (base or small, or Turbo on a GPU from B1). The final transcript after Stop uses the user's chosen model and replaces the live text.
- **Rules:**
  - The live panel never blocks or degrades the recording: it drops live pieces instead.
  - Off by default.
  - Marked Preview through the existing feature registry.
  - On a laptop on battery, it says so and offers to wait until Stop.
- **Measured targets** from VOC-3: text within 3 s of speech, and under 30 % of one core on an M-series Mac with base.
- **Agent:** `get_live_transcript { recording_id, since? }`.

### C2. Record one app's audio
- **Problem:** "Online meeting" records every sound the computer plays, including notifications, music and other tabs.
- **Windows:** process loopback is already in the code in *exclude* mode (for CaptureMann itself). Its *include* mode captures one process tree, such as Teams, Zoom or a browser.
- **macOS:** ScreenCaptureKit can filter by application.
- **Linux:** PipeWire can target an application's output node.
- **In the Record sheet,** under System audio: All sounds, or a chosen app (the list of apps playing sound now).
- **Agent:** `start_recording { system_audio: { app: "zoom.us" } }`. This app is out of scope in VOC-1 today, so the scope note changes.

### C3. Notice when a meeting starts (no bot)
- **What it notices:** the mic becomes active in Zoom, Teams, Meet in a browser, Slack huddles or Webex. Per system:
  - macOS: microphone-in-use APIs.
  - Windows: the CapabilityAccessManager consent store or the audio session list.
  - Linux: PipeWire stream events.
- **What it does:** one quiet notification, "Meeting detected in Zoom. Record audio?", with Record and Not now, plus a per-app "Never ask".
- **Never automatic:** it never records without a click. Off by default, and the consent line is shown as in VOC-1.
- **Pairs with C2:** the detected app is preselected as the audio source.

### C4. Remember voices (speaker profiles)
- After renaming "Speaker 2" to "Lan", offer "Remember Lan's voice". A voice print (the mean embedding of the speaker's longest turns) is stored locally. Later recordings label a matching speaker "Lan?" for the user to confirm.
- **Voice prints are biometric data:**
  - opt-in per person;
  - stored only on this computer, in the library database;
  - listed and deletable in Settings;
  - never exported, uploaded or given to agents (tools return names, not prints).
- **The match threshold is measured on A2,** and a wrong name is worse than "Speaker 2", so the rule is to keep "Speaker 2" when unsure.

### C5. Ask your recordings
- **Questions across the library,** answered by the built-in text model with links to the moments, for example "What did we decide about pricing last month?" or "Which meetings mentioned the Velopack spike?".
- **How:**
  - retrieval over transcript chunks, using the existing transcript search (LIB-1) plus a small local embedding model (licence check);
  - an answer with citations, checked against the transcript the same way AI-9 checks report citations;
  - "Not found in your recordings" when nothing matches.
- **Agent:** `ask_library { question, kinds?, since? }`. This turns the agent interface into a personal meeting memory.

### C6. A slot for the CraftMann Vietnamese meeting model
- **When the fine-tuned model is published on Hugging Face:**
  - add it to the model catalogue as a download, with checksum and licence;
  - add a per-recording choice of model;
  - make it the default for Vietnamese **only when** A2 shows a lower WER than Turbo on the far-field set and on the maintainer's private reference.
- **Code-switching:** read each piece with language detection limited to `vi` and `en`, and keep the glossary prompt for English technical terms. Measure on A2 against the fixed `vi` setting.
- **If the model isn't ready,** C6 moves to 0.4.x without affecting the rest.

### Also worth a line in 0.4.0, if time allows
- **Edit by transcript for audio (AI-6b):** delete words in the transcript and the audio is cut, as one undo step, reusing `cut_segments`.
- **Chapters in M4A exports** from markers and report sections.
- **Action items out:** export a report's tasks as Markdown checkboxes or an `.ics` file, or hand them to an agent (`get_report` already carries them).

## 6. Not proposed

| Idea | Why not now |
| --- | --- |
| A bot that joins calls | Against the local-first principle; needs accounts and servers |
| Cloud transcription as an option | Same; and the local pipeline is the product's point |
| Real-time echo cancellation while recording | The offline version (B2) is simpler, deterministic and good enough for transcripts |
| Voice cloning or text to speech | Off-mission; the competitor review put it under "don't chase" |
| End-to-end diarization (Nemotron 3 Diarization) inside 0.4.0 | sherpa-onnx support is a draft pull request; adopt when a release ships it, measured on A2. A Python comparison on the A2 set can happen any time |
| PhoWhisper conversion | Removed in 0.3.5; the CraftMann Vietnamese model (C6) replaces that path |

## 7. Questions for the maintainer

1. **Order:** is "safe and measured first" (0.3.8) acceptable, even though it ships little that users see?
2. **A2 data:** may the gate use AMI excerpts and synthetic far-field FLEURS mixes? Is there a 5-minute hand-corrected stretch of a real Vietnamese meeting for a private check, kept out of the repo?
3. **B3:** FLAC masters by default after Stop, with "Keep original WAV files" off? This changes the bundle version.
4. **C4:** are you comfortable shipping remembered voices, given that voice prints are biometric data? If yes, opt-in per person as described?
5. **C1 and C3:** keep both off by default and marked Preview in 0.4.0?
6. **C6:** what is the target date for the first fine-tuned Vietnamese model, so that 0.4.0 can plan its slot?
