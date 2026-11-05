# Release 0.2.8 plan: Windows catch-up and bugs

Status: **draft** (updated Nov 4, 2026), nothing built. Scope set by the maintainer on Nov 3, 2026: 0.2.7 is macOS only
(`specs/plans/release-0.2.7-plan.md`, **released Nov 4, 2026**), **0.2.8 is Windows**, 0.2.9 is Linux
(`release-0.2.9-plan.md`). A separate session and agent own this plan. The transcription follow-ups from 0.2.7 are a
parallel track, not part of the Windows scope (see "Transcription track").

Rules that apply (from `CLAUDE.md` and the maintainer): general techniques only, no phrase lists or one-recording
fixes; **nothing is called "verified" without a stored report in `verification/`** (a Windows report names the
machine, the Windows build and the app build); a box is checked only on the platform it belongs to; no GPL/AGPL in
shipped binaries; `target_os` only in platform code (`python3 scripts/check_target_os.py`); every user-facing action
has an MCP tool and a CLI command; UI text through `cm_i18n`; one change per commit with the two trailer lines; no
failing tests merge to `main`; the maintainer pushes `main`, signs, publishes and notarizes (signs Windows packages
never while O4 stays unsigned).

## Where 0.2.6 left Windows (inputs)

From `verification/L1-windows-thinkpad-2026-11-02-claude.md` and `specs/plans/release-0.3-0.4-platforms-plan.md`
(stages W0 to W6; this release is a slice of W1, W4, W5 and the start of W2/W3, not the whole 0.3.0):

- Early build, zip, unsigned. No recording, no text recognizer, no speaker engine, no bundled FFmpeg.
- Picker: app names of packaged apps are cut or wrong ("C:\Program Files\Wi...", "Windows Terminal H...",
  "Application Frame H..."); the listed and the saved window size differ by 1 px; a display shows as
  "Unknown Monitor 65537".
- ~~Diagnostics say releases include FFmpeg and `llama-server`, which the Windows zip does not.~~ Closed Nov 5, 2026: the diagnostics line is per build, and the zip and installer now include `llama-server.exe` (llama.cpp v0.7.0, CI run 41963086549; no model was run with it yet).
- Seen once with the 0.2.5 build: no toast, and a hotkey press that left a `cm-overlay` running; not reproduced.
- Auto-redact on Windows uses `tesseract` (never run on Windows).
- SmartScreen on a real download and Windows 10 22H2 have never been run.

## Tasks

Lab: the ThinkPad booted into Windows 11 (`scripts/testlab/lab.sh switch windows`; network profile Private; the user's
session for GUI steps; helpers in `C:\lab`). 0.2.7 does not use the laptop, so the Windows agent holds it until it
hands it to the Linux agent (0.2.9) in the state that plan expects (Ubuntu Wi-Fi joined).

1. **Bugs from 0.2.6** (reproduce first with a stored step, then fix, then re-run the step):
   packaged-app names in the picker (read the display name from the package or window title, not the executable path);
   the 1 px size difference (decide which number is right and make both agree; DPI rounding is the first suspect);
   "Unknown Monitor 65537" (device friendly name through the display configuration API, with a fallback that names
   the monitor by number); the diagnostics FFmpeg and `llama-server` lines (say what this build has); the missing
   toast and stray `cm-overlay` (debug logging on, try to reproduce; if it does not happen in 10 tries, list the
   logging added and leave it open); Auto-redact with `tesseract` (run it once; if task 2 lands, this becomes the
   fallback).
2. **Text recognizer:** `Windows.Media.Ocr` behind the existing recognizer trait, so Auto-redact needs no extra
   program (stage W5). Languages: the ones installed on the machine; say which in the settings text.
3. **Bundled FFmpeg:** a Windows LGPL FFmpeg build with the same self-check as macOS (`packaging/ffmpeg`: licence, no
   libx264, listed components, linked libraries), so export works in the zip. Output files carry `SOURCE-OFFER.txt` and
   licences like macOS. Build in CI on `windows-latest`; the maintainer approves the pinned source versions.
4. **Speaker engine:** a Windows x64 build of the pinned sherpa-onnx v2.0.1 static libraries (same version, SHA-256
   pinned, fetched by a script like `fetch-macos.sh`, never by the Cargo build) and the `--no-default-features`
   fallback message when absent. Check the speaker model files work unchanged.
5. **Recording (gated, see O5):** Windows.Graphics.Capture source + Media Foundation H.264 hardware encoder into the
   shared fragmented-MP4 writer; WASAPI microphone and loopback (stages W2, W3). Gate for 0.2.8: a 10-minute 1080p30
   recording with screen and microphone plays back in sync (measured, report stored), plus pause and crash recovery (end the
   process with `taskkill /F` mid-recording; the next start recovers the bundle through `recording.lock`, as the macOS
   `--kill9-test` does). The 30-minute all-tracks gate of the platforms plan stays for 0.3. No hardware encoder gives
   `hardware_encoder_unavailable` (check in a VM without a GPU). Hardware tests are `#[ignore]` and run one at a time on
   the lab laptop (it is a dedicated test machine, so the "ask first" rule for the Mac does not apply there).
6. **Trust and install:** SmartScreen on a real download in a browser (the "downloaded from the internet" mark) with a
   report; no signing (O4 decided Nov 5, 2026); winget manifest (unsigned installers are accepted). Installer work (NSIS, stage W6) only if the
   maintainer pulls it into 0.2.8 (O7); otherwise the release stays a zip.
7. **Windows 10 22H2 and a second monitor with mixed scaling:** each with a stored step or an explicit "not run" in
   the release text.

## Windows track: state on Nov 5, 2026 (written from the stored reports)

Branch `track/windows` (pushed to `public`). Evidence: `verification/L1-windows-0.2.8-hardware-2026-11-05-claude.md`
(one ThinkPad T490, Intel UHD 620, Windows 11 build 26200.9711; PARTIAL) and the night log next to it.

- Item 1 (bugs): display name "Display 1" (was "Unknown Monitor 65537"), app names Notepad/Settings readable, listed and saved
  window size agree (1426x720 both; fixed with per-monitor DPI awareness), diagnostics wording for FFmpeg and
  `llama-server` is per build. Windows Terminal is still listed as "Windows Terminal Host". The missing toast and the stray
  `cm-overlay` were not tried again.
- Item 2 (recognizer): `Windows.Media.Ocr` ran on the laptop (synthetic PNG and a real window shot through the daemon; en-US
  only is installed there). Other languages: not run.
- Item 3 (FFmpeg): the bundled LGPL FFmpeg 9.0.2 runs on the laptop; export quick/youtube-1080p (h264_mf), webm, gif worked.
  Import editing copies worked after a fix (Quick Sync needs NV12).
- Item 4 (speaker engine): linked in the package; run with the real models on a synthetic two-voice clip (2 speakers found, RTF 0.16) and `medium` transcription measured at 0.63x real time (round 2 report).
- Item 5 (recording): screen (WGC) + Media Foundation hardware H.264 + mic + cursor record on the laptop through the daemon and
  the app (on by default since O5: ships in 0.2.8, decided Nov 5, 2026); pause/resume, kill -9 recovery and window/region targets pass. The
  10-minute gate ran 621 s with all tracks (round 2 report); the camera makes 30 fps in a lit room; the loopback excludes a child's sound; the A/V offset is about 87 ms with the stimulus's own latency included, so the 40 ms criterion is not shown. Decision O5 stays with the maintainer.
- Items 6 and 7: SmartScreen and Windows 10 not run. A second monitor (3840x2160, 150 %, right of the panel, equal scales) was run on Nov 5: shots and recordings match the listing; the countdown, HUD and brackets are placed as if the monitor began at 0,0 (open); mixed DPI, negative origins and the cursor sprite premise need another layout.

## Done when

Bugs closed with a test or a stored step, or listed with reasons; a recognizer works on a clean Windows 11 (VM or
laptop, report stored); export works in the zip with the bundled FFmpeg; speaker labels run on the zip; and either the
recording gate passes with a report or recording is left for the next Windows release and the release text says so
(`docs/project/status.md` entry written from the reports, not from this plan).

## How the agent works

One agent, one worktree: `git worktree add ../capturemann-wt/windows -b track/windows` from `main`. Owns: the Windows
`sys/` files, `packaging/windows`, `packaging/ffmpeg` (Windows script), the Windows sherpa fetch script,
`apps/desktop/app.rc`, `scripts/ci/windows-smoke.ps1`, `C:\lab` helpers, Windows manual scripts and reports. May add
Windows-only code under `crates/cm-ai` (recognizer, engine glue) and `vendor/sherpa-onnx*` link settings for Windows
only, in separate commits. Must not touch macOS or Linux `sys/` files, run `make bundle` over `target/bundle`, push,
publish, or say "verified" without a report. Shared files with one owner (the maintainer resolves at merge):
`Cargo.lock`, workspace `Cargo.toml`, `locales/*.pot`/`vi.po`, `CHANGELOG.md`, `docs/project/status.md`,
`THIRD_PARTY_LICENSES.md`. Version stays 0.2.6 on the branch until the release commit.
Commands before every handoff: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D
warnings`, `cargo test --workspace --locked`, `python3 scripts/check_target_os.py`, and the Windows jobs (`ci:windows`
label on a branch). Final report: commits, tests run, reports written, what was not done.

## Base and merging

0.2.7 is published, so `track/windows` branches from the 0.2.7 release commit (version 0.2.7 on the branch until the
release commit) and rebases on `main` before each handoff. The 0.2.7 window-lifetime changes in `apps/desktop` are
already in `main`; Windows work there is still limited to Windows-specific files unless a shared fix is needed, in a
separate commit. The 0.2.8 tag needs nothing from other plans. If the transcription track (below) lands on `main` first,
the Windows zip picks it up when it is rebuilt for 0.2.8.

## Transcription track (from 0.2.7; macOS and shared `cm-ai`)

Written from the 0.2.7 study (`docs/research/transcription-quality-study-0.2.7.md`) and its stored reports; run by a macOS
agent in `cm-ai` in parallel, so Windows is not delayed. It is **not a gate** for this release: the Windows scope above
ships without it. Its outcome goes into the release text either way: if T1 or T2 lands and passes its gate with a stored
report, 0.2.8 ships that mitigation; if not, the release text says the invented-text fault (outro-style sentences over
quiet stretches) is still known and open, with the `ai.transcribe_raw_mic` default and the benchmark numbers. T3 to T6 are
stretch. Rules as in the 0.2.7 plan: general techniques only (no phrase lists, no channel names, no per-language
blocklists, no one-recording fixes), local only, nothing is "verified" without a stored report, private meeting audio
never enters the repository (counts only). One Windows-side item: before the release text recommends a transcription
model on Windows, measure speed on the lab laptop (no Metal; CPU or the chosen GPU backend).

### Invented text over quiet or noisy stretches

The maintainer saw it again on the 0.2.7 release candidate (Nov 4, 2026): Turbo writes an outro-style sentence (an invitation to subscribe to a channel, thanks for watching) over stretches with no speech or with faint speech, confidently (word confidence 0.5 to 0.9). It is promotional text nobody said, so it is the most damaging transcript fault we know.

What 0.2.7 established (reports in `verification/benchmarks/`):

- It reproduces on **public, generated audio**: 171 invented units per hour of reference-silent audio for Turbo in Vietnamese, 109 in English, 194 for Large v3 in Vietnamese (`invented-text-repro-2026-11-04.md`). It needs something faintly audible (room tone at -45 to -55 dBFS, hum, clicks, music-like tones) over a long enough stretch; exact digital silence and -60 dBFS room tone give 0.
- **No single signal separates it from faint real speech** (two confirmed invented units, a cough and a noise-only stretch, against 19 real quiet-speech units): energy, Silero, the speaker engine, RNNoise, confidence, words per second, spectral flatness, perturbation agreement, cross-model agreement and a runtime silence signature all overlap or fail leave-one-out (`invented-veto-signals-2026-11-04.md`).
- Beam search is the only decode change that removed it on the public set, but it is 2 times slower and hurts clean read speech; using it only for re-reads gave no gain (`parameter-search-2026-11-04.md`, careful re-reads in the release comparison report).
- Reading the raw microphone brings the sentence back where the noise-reduced copy writes nothing, which is why `ai.transcribe_raw_mic` ships off.

### Ordered work

| # | Work | Why | Gate |
|---|---|---|---|
| T1 | **PhoWhisper on the invented-text set.** Finish the ggml upload to `craftmann/phowhisper-ggml` (file ready, sha256 recorded in the 0.2.7 notes), pin the three `pending_pin` rows, and measure Vietnamese PhoWhisper small and medium on the public silence set and the labelled clips. It was trained on Vietnamese speech, not on web video with outro captions, so its prior may not contain the sentence. | The cheapest test of whether the fault is the model's prior. | Units per hour on the silence set (baseline 171) and WER-exact on the three references and FLEURS vi. |
| T2 | **An audio-tagging model as the independent signal** (an AudioSet-style classifier under an open licence, a few MB; the same model gives the planned `[laughter]` and `[whispering]` marks). A unit whose stretch has a low Speech probability and a high Cough, Noise or Music probability becomes `[noise]` (never deleted text, only a marker, like the 0.2.7 sounds). It is the one kind of evidence we have not tried: everything so far is a speech/non-speech detector or a Whisper opinion. | Cough and noise are exactly the two confirmed cases. | Removes both confirmed invented units, flags none of the 19 real quiet-speech units, then does not raise WER on FLEURS, the English sets, the 5-minute reference and the 84 s window, and cuts the public set's units per hour. |
| T3 | **More labels.** The third batch (clips 111 to 118 in `~/Movies/CaptureMann-review/silent-clips-3/`) is not labelled yet. Batches 4 and up only if a signal needs them (noise-like units with a high RNNoise or Silero value). | n = 2 invented units is too few to choose a rule. | At least 6 labelled noise-type invented units before any rule is fitted. |
| T4 | **A second consented recording with both microphone tracks and a corrected reference** (not seeded from a machine transcript). Decides the raw-microphone default and the Vietnamese default model. | One 1,000-word reference seeded from an older transcript carries every Large v3 conclusion. | Both models scored on two independent references. |
| T5 | **Mark, do not delete, when a model disagrees.** Turbo against Large v3 disagreement (0.35 or less) on a unit becomes `[unclear]`: it caught both invented units and flagged 6 of 19 real ones. Only if T1 and T2 fail; cost is a second model (3.1 GB) and extra decode time. | Cheapest mitigation that works on the labelled data. | The real units turned into `[unclear]` stay under 10 %, and the user can restore them. |
| T6 | **Show weak units in the editor** (a thin underline or a dimmed style for units with low speech evidence, with a one-click remove). | Even a good rule will miss some; the user needs the fastest way to delete a sentence nobody said. | Manual script with the maintainer; no new network or model. |

### Carried over from 0.2.7 (not invented-text)

- Normalisation: an AI-page switch, a "Tidy" button, and an automatic mode that shows the raw text first and tidies turn by turn in the background; shorter instructions and prompt caching to cut the time (15 minutes of Vietnamese took 116 to 154 s under contention); try Qwen 3.5 4B as the Vietnamese model (`normalization-2026-11-04.md`); the 50-turn human check `tests/manual/NORM-1.md`.
- Quantised models (Turbo Q8_0, Large v3 Q5_0): pin only if within 0.5 point and 1.3 times faster.
- Two-stream decode (microphone and system audio decoded separately, merged by time).
- Word-time stacks with no room: 32 to 111 words per meeting remain; a better timing fallback.
- A per-region quiet-input lift for the possible 3-point Turbo gain on the 5-minute reference (it lost on FLEURS in 0.2.7).
- Speaker counts and invented-text counts for the private meetings once silent-span marks exist.
- Windows has no Metal: before recommending Large v3 or Turbo there, measure speed on the CPU (or the GPU backend chosen) on the lab laptop; the 0.2.7 numbers do not transfer.
- The 20-minute microphone check `tests/manual/M2H-LIVE.md` for transcription while recording.

## Decisions needed from the maintainer

- **O4** Windows signing. **Decided by the maintainer on Nov 5, 2026:** no signing while CaptureMann is 0.x. Windows
  releases ship unsigned with SHA-256 checksums in the release text, the SmartScreen warning text and a winget manifest
  (`packaging/windows/winget/`). SignPath asks for too much in its application; Azure Artifact Signing or a
  cloud-signing certificate are the options to revisit for 1.0.
- **O5** Is recording on Windows a **must** for 0.2.8? Default proposal: no. It is attempted, and ships only if the gate
  passes; otherwise it moves to the next Windows release.
- **O7** Installer (NSIS) in 0.2.8 or still a zip? **Decided by the maintainer on Nov 5, 2026:** both. The zip and the unsigned NSIS installer are published; the winget manifest points at the installer.
- **O8** FFmpeg and sherpa-onnx Windows pins: approve the source versions and SHA-256 before the CI jobs are written.
  **Approved by the maintainer on Nov 4, 2026:** FFmpeg 9.0.2, libvpx 1.17.0, libopus 1.6.1 (as in `build-macos.sh`), and sherpa-onnx v2.0.1 `win-x64-static-MT-Release-lib` (SHA-256 `23e7b986b197eda0bbc47ff3d7d5c87afd6ef2b9077214ba26b0f1ec52ef0e66`) and `win-x64-static-MD-Release-lib` (`7c1f3ef15487f2a06625ef06685a9f96f03c0a1f0b3ad52de9e3bf6af6c1be39`).

## Decisions of Nov 5, 2026 (maintainer)

- O4: no signing in 0.x (unsigned zip and installer, SHA-256 sums, winget manifest). O5: Windows recording ships. O7: zip and installer.
- `llama-server` for Windows: a CI job builds it from the same pin as macOS (LLAMA_TAG v0.7.0); the model rows in `text_models.json` stay `pending_pin` until the maintainer pins them.
- Cursor sprite density: when a monitor's scale is not 100 %, the monitor's DPI is used (not the system DPI).
- The camera after an unplug: Windows reopens the same device and the daemon publishes `camera.reconnected`; macOS reconnect is on the backlog (documented difference).
- Merging `track/windows` into `main`: after 0.2.8 is developed and tested, by the maintainer.

## Known limitations of the Windows 0.2.8 release (for the release text)

- **Not run:** Windows 10 22H2; a second monitor with mixed scaling (review and steps in `tests/manual/DPI-WIN.md`, if the maintainer connects a monitor the steps are run and reported separately); SmartScreen on a real browser download (the installer and zip are unsigned, see O4).
- **Camera:** replugging a camera into a different USB port may give it another device id; the recorder reopens the same id only, so it may not resume (CAM-1 step W7 checks it). A camera in a dark room may drop its frame rate (the camera's own low-light behaviour).
- **Hardware:** tested on one ThinkPad T490 with an Intel UHD 620 only. NVIDIA and AMD hardware encoders and drivers are untried.
- **Multi-monitor:** the launcher and the start sheet always open on the primary display, and the HUD and the camera bubble cannot be dragged or snapped on Windows.
- **Text recognition:** only the languages installed in Windows are recognized; en-US was the one checked.
- **Process loopback** (system audio without CaptureMann's own sound) needs a recent Windows 10 or 11; older ones fall back to the plain loopback, which includes CaptureMann's sound.

## Risks

- One test laptop; if it fails, the VM and CI cover only the headless parts (FFmpeg, speaker engine, recogniser unit
  tests with fakes).
- `Windows.Media.Ocr` language packs vary by machine; the plan must not claim a language works until a report says so.
- Media Foundation hardware encoders vary by GPU; one machine is not a matrix. The release text says "tested on one
  laptop" and lists the GPU.
- A signed build is not available until O4 is approved; SmartScreen warnings stay in the install text.
