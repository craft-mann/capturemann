# Release 0.2.9 plan: bug fixes

Status: **draft** (Nov 6, 2026), nothing built. Scope set by the maintainer on Nov 6, 2026: 0.2.9 is a **bug-fix release**.
The macOS and Linux catch-up moved to 0.3.0 (`release-0.3.0-plan.md`). More bugs will be added as the maintainer
sends them; every item below keeps its number.

Rules that apply: general techniques only, no one-recording fixes; **nothing is called "verified" without a stored
report in `verification/`**; no GPL/AGPL in shipped binaries; `target_os` only in platform code
(`python3 scripts/check_target_os.py`); UI text through `cm_i18n`; one change per commit with the two trailer lines; a
regression test or a stored real-device step for every bug; no failing tests merge to `main`; the maintainer pushes
`main`, tags and publishes. **0.2.9 ships on all three platforms** (decided Nov 6, 2026): Windows, macOS and Linux get the fixes, and each platform
passes the same release regression as Windows 0.2.8 before its files go out (item 5).

## Bugs

1. **Windows: the Reveal button in the export toast opens the wrong folder** (window export of a recording).
   Leads: `ToastAction::RevealExport(id)` in `apps/desktop/src/toast.rs`, handled by `crate::ui::export_ui::reveal`
   (`apps/desktop/src/ui/app/toasts.rs`). Check which path it opens (the recording's exports folder versus the exported
   file, path separators, `explorer /select,`). Acceptance: after an export, Reveal opens the folder holding that file
   with the file selected, on Windows and on macOS. Add a regress case (`cm-regress`, desktop area) and a unit test of
   the path it builds.
2. **Screenshot annotation editor: Vietnamese (Unicode) text cannot be typed, and the text defaults are wrong.**
   - Typing: Telex and VNI input (composed characters, IME composition) does not reach the Text tool. Leads:
     `apps/desktop/src/editor/tools.rs` and `keys.rs` (the Text tool takes key events; IME needs the platform's
     text-input path, as the caption editor and the manual script `tests/manual/I18N-IME.md` use). Acceptance: type
     "Tiếng Việt có dấu" with Telex on Windows and macOS; it appears and exports correctly (font has the glyphs, no tofu).
   - Defaults: the text annotation starts at **size 24** and **white**. Check the saved/exported render and a light
     background (white on white needs the existing outline or shadow, or say so).
3. **AI models page on Windows shows macOS wording.** The page repeats "Mac" platform messages for every model
   (for example "every Mac", options "for Macs"). Show only what applies to the running platform and use the
   platform noun from `vocab` ("computer" where none fits). Leads: `apps/desktop/src/platform_text.rs`,
   `apps/desktop/src/ai/builtin.rs`, the model descriptions in `crates/cm-ai/data/*.json`, `crates/cm-ai/src/text_model/store.rs`
   and `speaker_engine.rs`. Acceptance: a pass over the Models page on Windows with the pseudo-locale and English finds no "Mac", "Apple Silicon" or "Metal" that does not apply; a test pins the platform
   filter. (Maintainer's note was short; confirm the exact strings from a screenshot before changing, O2.)
4. **Code-scanning findings, all High, all in `packaging/phowhisper/requirements.txt`** (Trivy and Dependabot):
   `torch==2.9.1` (arbitrary code execution through a malicious checkpoint file, Trivy #7, Dependabot #9) and
   `transformers==4.57.3` (remote code execution, `trust_remote_code` override, `save_pretrained` path traversal,
   custom generation code downloaded before consent, LightGlue model loading: Trivy #3, #4, #5; Dependabot #4, #5, #7, #8).
   The file is used only by `packaging/phowhisper/convert.sh`, which the maintainer runs by hand to convert PhoWhisper
   checkpoints; nothing from it ships in the app, the zip or the installer. Work: (a) read each advisory and write down
   the patched versions and whether the converter's use (loading the VinAI checkpoints it names, saving nothing through
   `save_pretrained`) reaches the vulnerable path; (b) bump `torch`, `transformers`, `huggingface_hub`, `safetensors` and
   `numpy` to patched versions and **re-run `convert.sh small`** to check that the output hash is unchanged (pins change
   only together with a successful run, see the file header; the previous result is in `packaging/phowhisper/README.md`);
   (c) if no patched version works with the converter, say so in the README, keep the pins with a short written
   justification, and let the maintainer dismiss the alerts as "not used in shipped code" (only the maintainer can
   dismiss them). Acceptance: the alert list for the file is empty, or each remaining alert is dismissed with a reason.
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
   - Acceptance: with 3, 6 and 10 speakers at the smallest supported window height (the laptop in the screenshot, and
     the pseudo-locale, which is 30 % longer), at least three rows show at once, the rest scroll with the wheel and the
     keyboard, nothing is cut mid-row, and the controls are reachable. Add a layout test (the `layout_width_tests`
     pattern) and a stored screenshot of the result with sample names only.
6. **Release regression on macOS and Linux, the same as Windows 0.2.8** (`cm-regress`, tiers smoke, standard,
   hardware; a stored PARTIAL report per platform and tier in `verification/`, template and rules as for
   `L1-windows-0.2.8-regression-*`). Today only the CLI, MCP, library, import and export cases run on macOS (`--dev`
   or `--package-dir`); the desktop-app, window, recording, audio and installer cases are stubs there and there is no
   Linux driver (`docs/development/regression.md`, "Run it on macOS"; `crates/cm-regress/src/sys/` has only `windows.rs`
   and `other.rs`). Work:
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
   - **Gate:** smoke, standard and hardware tiers run on the final 0.2.9 files of each platform, failures read and
     fixed in the product or the suite (not the evidence), and the release text says what was and was not run per
     platform (as the 0.2.8 text does). Build the drivers **first**, so the bug fixes in items 1 to 5 get regress cases on
     all platforms as they land.

## Decisions needed from the maintainer

- **O1** (answered) All three platforms ship 0.2.9, each after its own regression run. Open: a second Mac for the
  Intel build, or is the Intel macOS DMG built and checked on Apple Silicon only, as in 0.2.1 to 0.2.7 (the release text
  must then say it)?
- **O4** Lab time: the ThinkPad serves both Windows and Ubuntu; agree a window for Ubuntu hardware runs (Wayland and
  X11 each) and for the Samsung monitor cases on Windows.
- **O2** Bug 3: send a screenshot of the Models page, or confirm the lead strings.
- **O3** Alerts in bug 4: bump and re-convert (needs the Python 3.12 setup and about 500 MB of download), or dismiss as
  not used in shipped code.

## Done when

Each bug has a test or a stored real-device step; the packages for 0.2.9 are built for Windows, macOS and Linux, and
`cm-regress` smoke, standard and hardware are run on each (reports stored, PARTIAL), CI is green on `main`, and the code-scanning list for
`packaging/phowhisper/requirements.txt` is empty or dismissed with reasons.

## How the agent works

One agent per bug where the files are disjoint, each in its own worktree off `main`
(`git worktree add ../capturemann-wt/<name> -b <name>`), no pushing, no publishing; the maintainer merges. Commands before every
handoff: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, `python3 scripts/check_target_os.py`, `cargo run -p cm-i18n -- check`. Windows-only
code cannot be compiled on the Mac: CI on `track/windows` (workflow_dispatch) is the check, and the Windows job takes
about an hour until the CI speed-up in `release-0.3.0-plan.md` lands (consider doing that first).
