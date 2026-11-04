# Release 0.2.8 plan: Windows catch-up and bugs

Status: **draft** (Nov 3, 2026), nothing built; 0.2.7 was released Nov 4, 2026 (macOS only), and its transcription follow-ups for this release are in `transcription-followups-0.2.8.md`. Scope set by the maintainer on Nov 3, 2026: 0.2.7 is macOS only
(`specs/plans/release-0.2.7-plan.md`), **0.2.8 is Windows**, 0.2.9 is Linux (`release-0.2.9-plan.md`). A separate
session and agent own this plan and run alongside the 0.2.7 work (see "Working next to 0.2.7").

Rules that apply (from `CLAUDE.md` and the maintainer): general techniques only, no phrase lists or one-recording
fixes; **nothing is called "verified" without a stored report in `verification/`** (a Windows report names the
machine, the Windows build and the app build); a box is checked only on the platform it belongs to; no GPL/AGPL in
shipped binaries; `target_os` only in platform code (`python3 scripts/check_target_os.py`); every user-facing action
has an MCP tool and a CLI command; UI text through `cm_i18n`; one change per commit with the two trailer lines; no
failing tests merge to `main`; the maintainer pushes `main`, signs, publishes and notarizes (signs Windows packages
once O4 is done).

## Where 0.2.6 left Windows (inputs)

From `verification/L1-windows-thinkpad-2026-11-02-claude.md` and `specs/plans/release-0.3-0.4-platforms-plan.md`
(stages W0 to W6; this release is a slice of W1, W4, W5 and the start of W2/W3, not the whole 0.3.0):

- Early build, zip, unsigned. No recording, no text recognizer, no speaker engine, no bundled FFmpeg.
- Picker: app names of packaged apps are cut or wrong ("C:\Program Files\Wi...", "Windows Terminal H...",
  "Application Frame H..."); the listed and the saved window size differ by 1 px; a display shows as
  "Unknown Monitor 65537".
- Diagnostics say releases include FFmpeg and `llama-server`, which the Windows zip does not.
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
   recording with screen and microphone plays back in sync (measured, report stored), plus pause and `kill -9`
   recovery. The 30-minute all-tracks gate of the platforms plan stays for 0.3. No hardware encoder gives
   `hardware_encoder_unavailable` (check in a VM without a GPU). Hardware tests are `#[ignore]` and run one at a time on
   the lab laptop (it is a dedicated test machine, so the "ask first" rule for the Mac does not apply there).
6. **Trust and install:** SmartScreen on a real download in a browser (the "downloaded from the internet" mark) with a
   report; SignPath application (O4); winget manifest once signed. Installer work (NSIS, stage W6) only if the
   maintainer pulls it into 0.2.8 (O7); otherwise the release stays a zip.
7. **Windows 10 22H2 and a second monitor with mixed scaling:** each with a stored step or an explicit "not run" in
   the release text.

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

## Working next to 0.2.7

The macOS tracks do not need the laptop, so the two plans do not compete for hardware. Shared code is the conflict
risk: the 0.2.7 window-lifetime audit (M1) changes `apps/desktop`, which Windows also builds. Rules: merge M1's
shared-code commits first and rebase `track/windows` on them; Windows work that touches `apps/desktop` is limited to
Windows-specific files until then. Release order is 0.2.7, then 0.2.8; do not tag 0.2.8 before 0.2.7 is published. If
0.2.8 work lands on `main` early, the maintainer decides whether it ships in 0.2.7's Windows zip (default: no, the zip
is rebuilt only with 0.2.8).

## Decisions needed from the maintainer

- **O4** Windows signing: apply to SignPath (open source programme) now, because approval takes calendar time; until
  then keep the unsigned warning text.
- **O5** Is recording on Windows a **must** for 0.2.8? Default proposal: no. It is attempted, and ships only if the gate
  passes; otherwise it moves to the next Windows release.
- **O7** Installer (NSIS) in 0.2.8 or still a zip?
- **O8** FFmpeg and sherpa-onnx Windows pins: approve the source versions and SHA-256 before the CI jobs are written.

## Risks

- One test laptop; if it fails, the VM and CI cover only the headless parts (FFmpeg, speaker engine, recogniser unit
  tests with fakes).
- `Windows.Media.Ocr` language packs vary by machine; the plan must not claim a language works until a report says so.
- Media Foundation hardware encoders vary by GPU; one machine is not a matrix. The release text says "tested on one
  laptop" and lists the GPU.
- A signed build is not available until O4 is approved; SmartScreen warnings stay in the install text.
