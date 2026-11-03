# Release 0.2.7 plan: macOS stabilisation and faster, more accurate AI; Linux and Windows catch up

Status: **draft** (Nov 3, 2026), nothing built. Scope set by the maintainer on Nov 3, 2026, right after 0.2.6:

- **macOS 0.2.7:** stabilisation, plus faster and more accurate **transcription**, **speaker detection** and
  **report writing**.
- **Windows and Linux 0.2.7:** catch up with the macOS features and fix the bugs found in the 0.2.6 early builds.
- The work is split into four tracks that run **at the same time** in separate git worktrees, each driven by its own
  agent (see "How the agents work").

Rules that apply to every track (from `CLAUDE.md` and the maintainer): general techniques only, no phrase lists or
one-recording fixes (open source); **nothing is called "verified", "accurate" or "faster" without a stored report in
`verification/`** (benchmark reports in `verification/benchmarks/`); no GPL/AGPL in shipped binaries; `target_os` only
in platform code (`python3 scripts/check_target_os.py`); every user-facing action has an MCP tool and a CLI command;
UI text through `cm_i18n`; one change per commit with the two trailer lines; no failing tests merge to `main`; the
maintainer pushes `main`, signs, publishes and notarizes.

## Where 0.2.6 left things (inputs to this plan)

| Area | State at 0.2.6 | Source |
| --- | --- | --- |
| macOS Intel | Issue 21: Record panel, webcam bubble and (before 0.2.6) the export toast stay on top on an Intel Mac; only the toast wording is fixed, the stuck windows are unexplained, the reporter was asked to retest 0.2.6 and send the log | issue 21 |
| Transcription | Whisper large-v3-turbo on CPU/GPU; long recordings are cut into pieces, looping pieces are split and re-run (29 Whisper calls for 17 pieces on 88 min, cost unmeasured); Vietnamese is not measured | `docs/project/performance.md` |
| Speaker detection | 17 % diarization error on held-back English clips; Vietnamese not measured; an unknown speaker count can run the engine twice (88 min: 265 s plus 263 s); memory grows with length (0.6 GB at 10 min, 3.5 GB at 150) and Cancel is ignored until the run ends; `window_shift_ratio` 0.1 is the main cost | `docs/project/performance.md`, `verification/benchmarks/speaker-detection-2026-10-29.md` |
| Reports and translation | Built-in text model (Gemma 4 E2B); an 88 min Vietnamese-to-English translation took 691 s and 6.5 GB; repeated-text stand-ins failed with `output_too_long` and lost their partial result; report accuracy never measured | `docs/project/performance.md` |
| Linux | `.deb`/tarball early build, no recording (code exists behind `CM_LINUX_RECORDING=1`, never seen on a real desktop); Wayland window capture and global shortcuts have no path on GNOME 46; aarch64 only in a VM | `docs/development/linux-early-build.md`, `verification/LINUX-1-*` |
| Windows | early build, unsigned, no recording, no text recognizer, no speaker engine, no bundled FFmpeg; picker app names, 1 px size difference, "Unknown Monitor 65537"; SmartScreen and Windows 10 never run | `verification/L1-windows-thinkpad-2026-11-02-claude.md`, `specs/plans/release-0.3-0.4-platforms-plan.md` |

## Track M1: macOS stabilisation (agent `mac-stability`)

Goal: nothing a user can see gets stuck, and failures leave a report.

1. **Issue 21 (Intel).** Reproduce in the pieces we can: the Record panel is closed by the code on Start, the setup
   bubble on Start, the toast on dismiss. Add the missing evidence first: log lines at each close and each window
   creation/destruction (`cm_log`), a "Copy diagnostics" check that lists the live app windows, and a watchdog test
   with a scripted window host that fails to close. Then fix what the logs show. Do not guess a fix without a log.
   Wait for the reporter's 0.2.6 log; ask on the issue if none arrives.
2. **Window lifetime audit.** Every floating surface (panel, bubble, toast, overlay, launcher) gets one owner, one close
   path and a test that a failed close is retried or reported. Applies to Windows and Linux hosts too (shared code in
   `apps/desktop`).
3. **Long-session health.** A 3-hour soak of recording plus the library open (`tests/manual/VOC-1.md` soak,
   `REC-*`), memory and file-handle growth recorded in a report.
4. **Intel confirmation.** Ask for one more Intel report or borrow a machine; until then the Intel DMG stays
   "built on Apple Silicon, checked under Rosetta" in every public text.
5. Items from the 0.2.6 review that touch macOS only: none known. Add any found in M1.

Done when: a stored soak report, window-lifetime tests in CI, issue 21 either fixed with a log-backed cause or still
open with the evidence listed.

## Track M2: macOS AI speed and accuracy (agent `mac-ai`)

Everything starts with a **benchmark that exists before the change**, stored under `verification/benchmarks/`, so
every claim is a before/after on the same data. Public wording: "measured on ...", never "accurate".

### M2a Transcription
- Build a word-error-rate (WER) harness over public clips with reference text (English first; Vietnamese from an open
  corpus the maintainer approves; no private recordings in the repository). Report WER and real-time factor per model.
- Measure the cost of the repeat re-runs (performance.md item 3) and cut it: detect looping with a general rule,
  re-run only the looping piece with a changed temperature or without the previous text as prompt, not a phrase list.
- Long-recording speed: overlap of pieces, GPU vs CPU split, model choice by length. Target set after the baseline.
- Vietnamese: measure first (open decision O1), then decide if a different model or language setting is warranted.

### M2b Speaker detection
- **Baseline for Vietnamese and for 3+ speakers** (today only English is measured, 14 clips).
- Remove the second engine run for unknown speaker counts (performance.md item 1): options are a grouping pass over
  the voice prints, or a first-pass cap that does not need a rerun. Needs a change in the vendored sherpa-onnx
  wrapper (`vendor/sherpa-onnx*`, trimmed copies: keep the no-download build rule).
- Windowed runs to cap memory and make Cancel work (item 2).
- Retune `window_shift_ratio` and the clustering threshold on a larger set than 14 clips (item 4), reporting DER
  next to time for each setting.

### M2c Reports and translation
- Measure report **speed** (tokens per second on M-series and Intel) and **quality**: define a checkable rubric
  (every item links to a timestamp that exists; all speakers named in the transcript appear; no invented names or
  numbers, checked by string match against the transcript) and score a fixed set of transcripts. Report the
  scores; no "accuracy" claim without the report.
- Fix `output_too_long` for translation (performance.md item 5): keep partial work, split the piece and retry
  (the reports path already splits).
- Candidate speed-ups to measure, not assume: prompt/KV cache reuse across pieces, smaller context for short
  pieces, a larger quant or model choice for Apple Silicon. Any new model needs a pinned URL and SHA-256 in
  `crates/cm-ai/data/text_models.json` (no `pending_pin` entries ship) and the maintainer's approval (O2).

Done when: before/after reports for each of M2a, M2b, M2c are stored, the tests (with fakes) cover the new
splitting/cancel behaviour, and `tests/manual/AI-5/7/8/9.md` have been re-run on the new build.

## Track L: Linux catch-up and bugs (agent `linux`)

Lab: the ThinkPad T490 (`scripts/testlab/lab.sh`, Ubuntu 24.04, GNOME 46, Xorg and Wayland). The VM at
`~/VMs/cm-linux` covers aarch64 and headless work.

1. **Bugs from 0.2.6:** Wayland Shortcuts page shows Recording rows and Change buttons although recording is off;
   the X11 tray "delay" check mark (stale or missed click) and "Recent captures" listing only the app's capture;
   the `user_declined` code for a portal failure (code 2 is not a user decline); the odd "secrets in the system
   credential store" line with no Secret Service; unlabelled steps in the X11 report (editor, tarball default prefix,
   tray activation, language) run one by one.
2. **Wayland window capture on GNOME 46:** the portal's `InteractiveScreenshot` returns no file. Try the ScreenCast
   portal with the window source type for a single frame, or read the file the shell saved. Pick the approach by an
   experiment on the laptop, record it as a report.
3. **Recording on a real desktop:** turn on the existing recorder (`CM_LINUX_RECORDING=1`) on the T490 (VA-API H.264
   hardware encode was seen working; software VP9 is the labelled fallback, decisions D-R1 to D-R5 in
   `specs/plans/linux-recording-plan.md`); fix what a real desktop shows (banner controls, Start sheet keycaps, the
   settings that mean nothing for VP9). Gate: a 10-minute recording with screen and microphone plays back in sync on X11
   and Wayland, report stored.
4. **Global shortcuts:** GlobalShortcuts portal on desktops that have it (KDE, newer GNOME); test one KDE machine or VM.
5. **Hardware gaps:** a real ARM device (or keep "VM only" in the notes), NVIDIA and AMD, two monitors and mixed scale.
6. **Packaging:** decision on signing keys stays minisign; consider an APT repository or Flatpak (open decision O3).

Done when: bugs closed with tests or a stored real-device step, the Wayland window path chosen and reported, and
recording seen on the laptop with a report (or explicitly deferred to 0.2.8 in the release text).

## Track W: Windows catch-up and bugs (agent `windows`)

Lab: the same ThinkPad booted into Windows 11 (`lab.sh switch windows`; private network profile; the user's session
for GUI steps, helpers in `C:\lab`).

1. **Bugs from 0.2.6:** picker app names for packaged apps ("C:\Program Files\Wi...", "Windows Terminal H...",
   "Application Frame H..."); the 1 px difference between the listed and the saved window size; the "Unknown Monitor
   65537" display name; the diagnostics line about FFmpeg and `llama-server` that says releases include them; the
   missing toast seen once with the 0.2.5 build and the hotkey press that left a `cm-overlay` (try to reproduce with
   debug logging); Windows Auto-redact with `tesseract` (never run).
2. **Text recognizer:** `Windows.Media.Ocr` so Auto-redact needs no extra program (plan stage W5).
3. **Speaker engine and the bundled FFmpeg:** a Windows build of the pinned sherpa-onnx libraries and an LGPL FFmpeg
   with a self-check like macOS (`packaging/ffmpeg`), so speaker labels and export work in the zip.
4. **Recording:** Windows.Graphics.Capture + Media Foundation H.264 into the shared fragmented-MP4 writer, WASAPI
   microphone and loopback (stages W2, W3). Gate: a 10-minute recording with screen and microphone, plays back in
   sync, report stored; the 30-minute gate of the platforms plan stays for 0.3.
5. **Trust:** SmartScreen on a real download (browser, "downloaded from the internet" mark) with a report; decide on
   signing (SignPath application, open decision O4); winget manifest once signed.
6. **Windows 10 22H2** and a second monitor with mixed scaling, each with a stored step or an explicit "not run".

Done when: the bugs are closed or listed with reasons, a recognizer works on a clean Windows 11, and either recording
passes its gate with a report or is explicitly left for 0.2.8.

## How the agents work (parallel worktrees)

Following the parallel-agent rules we already use (disjoint files, unique script names, I merge and retest).

```sh
# once per track, from the main checkout, after `git checkout main && git pull --ff-only public main`
git worktree add ../capturemann-wt/mac-ai       -b track/mac-ai
git worktree add ../capturemann-wt/mac-stability -b track/mac-stability
git worktree add ../capturemann-wt/linux       -b track/linux
git worktree add ../capturemann-wt/windows     -b track/windows
```

The `track/*` branches are **temporary work areas**; the maintainer's rule "work on `main`, no feature branches" is
about the shared line of work. Each agent's finished, tested commits are merged to `main` by me (fast-forward or
cherry-pick, keeping the commit messages), never pushed by the agent. Delete each worktree and branch after merging.

| Track | Owns (may change) | Must not touch without asking |
| --- | --- | --- |
| mac-ai | `crates/cm-ai`, `crates/cm-media` (transcription and report paths), `vendor/sherpa-onnx*`, `verification/benchmarks/`, `tests/manual/AI-*.md` | `apps/desktop` UI beyond progress and cancel wiring |
| mac-stability | `apps/desktop` (window lifetime, diagnostics), `apps/overlay`, `crates/cm-daemon` (recording lifecycle), soak scripts | `crates/cm-ai` |
| linux | `crates/cm-capture/src/sys/` (Linux files), `crates/cm-audio` Linux files, `crates/cm-platform` Linux code, `packaging/linux`, `docs/development/linux-early-build.md`, `scripts/testlab/` Linux steps | macOS and Windows `sys/` files |
| windows | the Windows `sys/` files and `packaging/windows`, `apps/desktop/app.rc`, `scripts/ci/windows-smoke.ps1`, `C:\lab` helpers | macOS and Linux `sys/` files |

**Shared files with one owner** (agents add entries on their branch, I resolve at merge): `Cargo.lock` and workspace
`Cargo.toml` (me; version stays 0.2.6 until the release commit bumps all to 0.2.7), `locales/capturemann.pot` and
`vi.po` (regenerate with `cargo run -p cm-i18n -- extract` after the merge, not on the branch), `CHANGELOG.md` and
`docs/project/status.md` (me, from the agents' final reports), `THIRD_PARTY_LICENSES.md` (`make licenses` after a
`Cargo.lock` change).

**One test laptop:** Linux and Windows share it (dual boot, reboot by `lab.sh switch`), so the `linux` and `windows`
agents take **turns**: one holds the lab at a time, says so in its report, and leaves the laptop in the other
system's expected state (Windows booted, network Private; Ubuntu Wi-Fi joined). The macOS tracks do not need it.
Hardware tests on the laptop are allowed (dedicated test machine); on the Mac, recording tests need the maintainer's
consent and stay short (`CLAUDE.md`).

**Each agent's brief** names the goal, the files it owns, the baseline report to create first, the commands to run
(`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, `python3 scripts/check_target_os.py`), what it must not do (run `make bundle` over
`target/bundle`, push, publish, say "verified" without a report) and the report it hands back: commits, tests run,
reports written, what was not done.

## Order and checkpoints

1. **Week 1:** baselines and reproductions only (M2 benchmarks, M1 logging, L/W bug lists with steps). No behaviour
   change merges before its baseline report exists.
2. **Week 2:** fixes and speed-ups with before/after reports; Linux Wayland window path and Windows recognizer chosen.
3. **Week 3:** recording gates on Linux and Windows (or the explicit deferral), Intel check, soak, full test run on
   all three CI OSes.
4. **Release:** release text from the reports; packages and DMGs by the pattern of 0.2.6 (draft, user signs, user
   publishes, I check one download per platform, cask script, status entry); one version for everything: **0.2.7**.

CI: `ci:windows` label runs the Windows jobs on a branch; Linux and Windows package workflows need the temporary
`push` trigger trick on a non-default branch (see `docs/development/releasing.md`); remove it before merging.

## Decisions needed from the maintainer

- **O1** Vietnamese quality: which open corpus may we use for WER and speaker tests, and is a different Whisper
  model for Vietnamese acceptable to ship?
- **O2** Text model for reports: keep Gemma 4 E2B or add a second (larger) model; model licence and the download size
  limit.
- **O3** Linux distribution beyond `.deb`/tarball: APT repository or Flatpak in 0.2.7, or later.
- **O4** Windows signing: apply to SignPath (open source programme) now; until then keep the unsigned warning text.
- **O5** Is recording on Linux and Windows a **must** for 0.2.7, or may it ship as 0.2.8 if the gate is not met?
- **O6** Issue 21: if the reporter does not answer, do we ship a diagnostics-only build to Intel testers?

## Risks

- The one lab laptop is a serial resource; Windows recording and Linux recording both want it. Mitigation: VM and CI
  for headless parts, a lab schedule in the plan above.
- Accuracy work without a good Vietnamese set can only be claimed for English; the release text must say so.
- A change inside the vendored sherpa-onnx wrapper must keep the no-download, no-GPL build rules.
- Three platforms edit `apps/desktop`; the window-lifetime audit (M1) touches shared code that Linux and Windows
  also use: merge M1's shared-code commits first, then rebase the platform tracks.
