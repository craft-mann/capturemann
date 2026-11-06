# Release 0.2.9 plan: bug fixes

Status: **draft** (Nov 6, 2026), nothing built. Scope set by the maintainer on Nov 6, 2026: 0.2.9 is a **bug-fix release** whose main targets are **stabilization, security, regression testing and some UI/UX enhancements** (no new features).
**0.2.9 ships on all three platforms** (Windows, macOS, Linux) with the bug fixes, the security work, the regression suite and the UI/UX fixes.
New Linux features and a Linux-only release are 0.3.0 (`release-0.3.0-plan.md`). More bugs will be added as the maintainer
sends them; every item below keeps its number.

Rules that apply: general techniques only, no one-recording fixes; **nothing is called "verified" without a stored
report in `verification/`**; no GPL/AGPL in shipped binaries; `target_os` only in platform code
(`python3 scripts/check_target_os.py`); UI text through `cm_i18n`; one change per commit with the two trailer lines; a
regression test or a stored real-device step for every bug; no failing tests merge to `main`; the maintainer pushes
`main`, tags and publishes. **0.2.9 ships on all three platforms** (decided Nov 6, 2026): each platform's files go out only after that platform
passes the same release regression as Windows 0.2.8 (items 6 and 7).

## Bugs

1. **Windows: the Reveal button in the export toast opens the wrong folder** (window export of a recording).
   Leads: `ToastAction::RevealExport(id)` in `apps/desktop/src/toast.rs`, handled by `crate::ui::export_ui::reveal`
   (`apps/desktop/src/ui/app/toasts.rs`). Check which path it opens (the recording's exports folder versus the exported
   file, path separators, `explorer /select,`). Acceptance: after an export, Reveal opens the folder holding that file
   with the file selected, on Windows and on macOS; on Linux it opens the folder (a file manager cannot be told to select a file in general). Add the acceptance as a new AC in `specs/features/08-export.md` (next free number, EXP-4 or the feature that owns Reveal) and cite it in the test and commit. Add a regress case (`cm-regress`, desktop area) and a unit test of
   the path it builds.
2. **Screenshot annotation editor: Vietnamese (Unicode) text cannot be typed, and the text defaults are wrong.**
   - Typing: Telex and VNI input (composed characters, IME composition) does not reach the Text tool. Leads:
     `apps/desktop/src/editor/tools.rs` and `keys.rs` (the Text tool takes key events; IME needs the platform's
     text-input path, as the caption editor and the manual script `tests/manual/I18N-IME.md` use). Acceptance: type
     "Tiếng Việt có dấu" with Telex on Windows and macOS; it appears in the editor **and in the exported PNG** (font has the glyphs, no tofu; the export path in `cm-render` has its own font handling, check it separately). Linux (IBus, Fcitx) is not tested in 0.2.9: the release text says so. Add the acceptance as a new AC (next free number after ANN-1.AC6) in `specs/features/02-annotation.md` and cite it in the test and commit.
   - Defaults (new text only; existing annotations are untouched and the user can change colour and size in the panel as before): the text annotation starts at **size 24** and **white**. Check the saved/exported render. **No automatic outline or shadow** (maintainer decision, Nov 6, 2026: the user can change colour if the screenshot is light).
3. **AI models page on Windows says "Mac" in the model descriptions** (maintainer, Nov 6, 2026: look at the text model
   section on the Windows app). Cause found in the data, not in the UI: each model's one-line `hint` comes from the
   catalogues `crates/cm-ai/data/text_models.json` and `models.json` and is shown as is, on every platform:
   - Gemma 4 E2B: "Recommended for reports and notes on **every Mac**. ..." (recommended model)
   - Gemma 4 26B-A4B: "Higher quality for **Macs** with plenty of memory ..."
   - Qwen3.5 2B: "Smallest option for **Macs** with little free memory ..."
   - Whisper medium (`models.json`): "... A good choice on **8 GB Macs**."
   These hints are English-only (they are not in `locales/capturemann.pot`, so the Vietnamese page shows English).
   Fix: make the hints platform-neutral ("every computer", "computers with plenty of memory", "8 GB of memory") **and
   translatable**: either keep the JSON hint as a message id that goes through `tr!` (extracted into the `.pot`), or move
   the hint text into the app and look it up by model id; write the Vietnamese. Check the whole Models page (speech, text,
   speaker sections, sizes and memory lines, the "Metal" or "Apple Silicon" mentions in `speaker_engine.rs` and
   `text_model/store.rs`) on Windows for the same problem. Acceptance: on Windows, English and Vietnamese, the page
   shows no "Mac", "Apple Silicon" or "Metal" text; a test fails if any catalogue hint (`text_models.json`, `models.json`) contains a platform noun (`Mac`, `macOS`, `Apple`,
   `Metal`, `Windows`, `Linux`); no exemptions. A hint from a replacement catalogue (`CAPTUREMANN_TEXT_CATALOG`) that has no
   translation is shown in English as written. The hint wording is a product
   decision for the 0.2.9 notes only if a hint claims something (like "best score in our test"); keep those claims
   as written.
4. **Code-scanning findings, all High, all in `packaging/phowhisper/requirements.txt`** (Trivy and Dependabot):
   `torch==2.9.1` (arbitrary code execution through a malicious checkpoint file, Trivy #7, Dependabot #9) and
   `transformers==4.57.3` (remote code execution, `trust_remote_code` override, `save_pretrained` path traversal,
   custom generation code downloaded before consent, LightGlue model loading: Trivy #3, #4, #5; Dependabot #4, #5, #7, #8).
   The file is used only by `packaging/phowhisper/convert.sh`, which the maintainer runs by hand to convert PhoWhisper
   checkpoints; nothing from it ships in the app, the zip or the installer. Work: (a) read each advisory and write down
   the patched versions and whether the converter's use (loading the VinAI checkpoints it names, saving nothing through
   `save_pretrained`) reaches the vulnerable path; (b) bump `torch`, `transformers`, `huggingface_hub` (transformers 5 needs 1.0 or later), `safetensors` and
   `numpy` to patched versions and **re-run `convert.sh small`** (pins change only together with a successful run, see the file header; the previous result
   is in `packaging/phowhisper/README.md`). **Pass criterion:** the new ggml file loads in `cm-ai`
   (`real_model` test) and transcribes a fixed Vietnamese audio file with a word error rate no worse than the stored result
   for the pinned file. A different file hash alone is not a failure (a torch or transformers major version can change the
   bytes of an equivalent model); record both hashes in the README and say which file is the new pin;
   (c) if no patched version works with the converter, say so in the README, keep the pins with a short written
   justification, and let the maintainer dismiss the alerts as "not used in shipped code" (only the maintainer can
   dismiss them). Acceptance: the alert list for the file is empty, or each remaining alert is dismissed with a reason.
   **What the open alerts say (read Nov 6, 2026; 8 Dependabot and 7 code-scanning alerts, all in that one file, none in
   the Cargo, Actions or other dependencies):** `torch==2.9.1` is fixed in **2.10.0** (high: CVE-2026-24747, checkpoint
   loading; two low and two medium older ones are covered by the same bump); `transformers==4.57.3` is fixed only from
   **5.10.0** (the newest of its high alerts, CVE-2026-9856, `save_pretrained` path traversal; the others are fixed from
   5.3.0 and 5.5.0; Dependabot lists the vulnerable range as 4.49.0 to 5.8.1). That is a **major-version jump** (4.x to
   5.x) for the converter, which was written against 4.x: expect `huggingface_hub` and the whisper.cpp
   `convert-h5-to-ggml.py` loading code to need checks, so the re-conversion of `small` and its transcript comparison is the real
   test, not the pin edit.
   Treat the checkpoints as untrusted input in the meantime: the README says to convert only the three pinned VinAI
   revisions in a throwaway virtual environment.

5. **Speakers tab on a laptop screen: the list is cramped** (maintainer's screenshot, Nov 6, 2026, a 3-speaker recording
   on the Mac laptop; the screenshot shows names of real people, so it is not stored in the repository). The speaker list
   shows one and a half rows between the heading and the controls, because the rows have a fixed small height and the
   panel below takes the space. Work, in `apps/desktop/src/ui/recording_editor/speakers_tab.rs`:
   - Move the sentence "Voices are compared in memory only. No voiceprint is saved, and people are never recognised across
     recordings." (the `lock` note at the end of the tab, around line 614) to **Settings -> Privacy**, as a fifth fact
     (`privacy_facts()` in `apps/desktop/src/ui/settings/model.rs`; the test that pins four facts and the i18n catalogue
     change with it; translate to Vietnamese). The tab keeps no long note; if a short pointer is wanted, a link to the
     Privacy page, not the sentence.
   - Give the list the free height of the panel and let it **scroll** when it has more speakers than fit (a flexible
     height with a minimum of about three rows and a thin scrollbar, not a fixed one); the controls ("How many people?",
     "My microphone only hears me", "Find again") stay visible under it or in one compact row group.
   - Acceptance: with 3, 6 and 10 speakers at the smallest window the app allows (**720 x 480 logical px**, the minimum set in
     `ui/app/routing.rs`; the default is 1180 x 760; also the laptop in the screenshot, and
     the pseudo-locale, which is 30 % longer), the list scrolls with the wheel and the keyboard, nothing is cut mid-row, and
     the controls are reachable (maintainer decision, Nov 6, 2026: at the 720 x 480 minimum the panel may scroll as a whole,
     "scrolls, nothing is cut"; three rows without scrolling are required only from the default window size down to the
     laptop screen in the screenshot, and the layout test pins the 3-row minimum of the list itself). Add a layout test (the `layout_width_tests`
     pattern) and a stored screenshot of the result with sample names only.
6. **Regression drivers and reports for macOS and Linux, the same as Windows 0.2.8** (a gate for the macOS and Linux packages) (`cm-regress`, tiers smoke, standard,
   hardware; a stored PARTIAL report per platform and tier in `verification/`, template and rules as for
   `L1-windows-0.2.8-regression-*`). **Status Nov 6, 2026:** both drivers are written and merged (`sys/macos.rs`, `sys/linux.rs`, `cases/linux.rs`,
   `regress.sh macos|linux`), tested with fakes only. First dry runs: macOS `--dev --tier standard` 40 passed, 0 failed,
   16 skipped (no `--allow`); Linux smoke on the lab laptop against the installed 0.2.6 package 9 passed, 3 failed (old package:
   missing licences, `llama-server` line, MCP tools added in 0.2.8), report `verification/L1-linux-0.2.6-regression-smoke-x11-*`.
   Still SKIP on macOS: `hud_click_through`, `recording_placement`, `shot.window`, `redact.window_text`, `rec.each_monitor`.
   Linux needs a Linux x86_64 `cm-regress` (the Mac builds a static musl one with `rust-lld`, no Docker needed). Remaining work:
   - **macOS driver** (`sys/macos.rs`): display and window listing, the desktop-app cases (stays up, second launch,
     hotkey and overlay, settings applied, menu-bar icon), recording on the screen, microphone, system audio and camera
     with the same picture and frame-rate checks as Windows, crash reports, the app bundle and the DMG install and
     uninstall case. Run on the Mac with `--allow screen,sound,mic,camera` (permissions granted to the dev bundle,
     see `docs/development/building.md`). Screen Recording needs the grant of the bundle that runs the test.
   - **Linux driver** (`sys/linux.rs`): the same list for X11 and for GNOME Wayland on the lab laptop (Ubuntu 24.04),
     the `.deb` install, uninstall and tarball install cases, VA-API versus software encoder reporting. Recording
     cases run only where the build has recording (Linux recording stays behind `CM_LINUX_RECORDING=1`; otherwise they
     report SKIP with that reason).
   - **Runner scripts** (`scripts/testlab/regress.sh macos|linux`): the lock, volume restore, power settings, report
     copy-back and clean-up the Windows script has; Linux runs on the lab laptop, macOS on the maintainer's Mac or a
     second Mac. Never touch the user's real library, the display settings or the audio output device.
   - **Gate:** smoke, standard and hardware tiers run on the final 0.2.9 files of each platform (the Mac, and the Ubuntu
     24.04 lab laptop for Linux; the laptop is available, so no reduced Linux scope is planned). Failures are read and fixed in the product or the suite (not the evidence), and the
     release text says what was and was not run per platform (as the 0.2.8 text does). Build the drivers **early**, so the bug
     fixes in items 1 to 5 get regress cases on all platforms as they land.
   - Hardware-tier runs record the real screen, microphone and camera: run them only when the maintainer asks. On the Mac
     use the `-dev` bundle and never touch `/Applications`.
7. **Windows CI speed-up** (moved from `release-0.3.0-plan.md` item 2, Nov 6, 2026; the Windows `Test` step took about 55 min
   on a cold cache): `cache-on-failure: true` for rust-cache; skip `compose_perf` and the GPU `parity` test on the Windows
   runner (macOS covers them); split the job or use nextest; optional opt-level 2 for dependencies. Measure before and after
   on a branch; target 15 to 20 min warm. Acceptance: the Windows job on `track/windows` takes 20 min or less warm, the skipped
   tests still run on the macOS job, and nothing else in coverage drops.

## Decisions (maintainer, Nov 6, 2026)

- **Platforms and order:** one 0.2.9 tag for Windows, macOS and Linux, released when all three regressions have passed.
  0.3.0 is Linux-only (new Linux features).
- **Intel Mac:** no Intel Mac is available. The Intel DMG is built and checked on Apple Silicon only, and the release
  text says so (as in 0.2.1 to 0.2.7).
- **Linux recording:** stays behind `CM_LINUX_RECORDING=1`; Linux stays an early build. Linux files are signed with
  minisign as in 0.2.6.
- **Bug 2:** the 24 and white defaults apply to new text only.
- **Lab:** the ThinkPad has two systems (Windows 11 Pro and Ubuntu), so the runs take turns. The Samsung monitor cases are
  skipped; Windows 10 and a mixed-DPI monitor stay "not run" in the release text.
- **Release process:** as for 0.2.8 (the agent prepares; the maintainer pushes `main` and the tag, and disables the tag
  ruleset for a moment only if a tag has to move).

## Defaults pending confirmation

- **O3** Alerts in bug 4: default is to bump `torch` to 2.10.0 or later and `transformers` to 5.10.0 or later and re-convert
  `small` (about 500 MB download, Python 3.12, on the maintainer's Mac or the agent's, with the throwaway virtual
  environment). If the converter does not work with transformers 5, the pins stay and the maintainer dismisses the
  alerts as "not used in shipped code" (the file is not part of any shipped artifact).
- **O5** Order of work (default): CI speed-up first (item 7; the Windows job takes about an hour), then the regression drivers
  (item 6), then bugs 1 to 5 with their regress cases, then the PhoWhisper pins, then the three regression runs, then the
  packages and release.

## Done when

Each bug has a test or a stored real-device step; the 0.2.9 packages are built for Windows, macOS and Linux, and `cm-regress` smoke, standard and
hardware are run on each (reports stored, PARTIAL); the Windows CI job meets
item 7; CI is green on `main`; and the code-scanning list for `packaging/phowhisper/requirements.txt` is empty or dismissed
with reasons. Release mechanics, as for 0.2.8: version bump, `docs/project/status.md` and `CLAUDE.md` current-state update,
release text and steps from the 0.2.8 files as the template, winget package update, Homebrew cask, macOS notarization, Linux minisign signatures; the maintainer pushes `main`, the tag and
publishes.

## How the agent works

One agent per bug where the files are disjoint, each in its own worktree off `main`
(`git worktree add ../capturemann-wt/<name> -b <name>`), no pushing, no publishing; the maintainer merges. Commands before every
handoff: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, `python3 scripts/check_target_os.py`, `cargo run -p cm-i18n -- check`. Windows-only
code cannot be compiled on the Mac: CI on `track/windows` (workflow_dispatch) is the check, and the Windows job takes
about an hour until the CI speed-up (item 7) lands, so do that first.
