# Release day, 0.3.5 (Windows and macOS): steps for the maintainer

Written on Nov 17, 2026 (night) from `release-day-0.3.4.md` (the pattern), for the release planned on **Nov 18, 2026**.
"[M]" = only the maintainer does this (push, tag, sign, notarize, publish, accounts, decisions). "[A]" = an agent can do it
from this file. Run commands from the repo root. Text to publish: `release-0.3.5-github-text.md` (its `<FILL: ...>` items need the
final files and their reports). Plan: `specs/plans/release-0.3.5-plan.md` (BUG-1 to BUG-18, ANN-12, the About Send feedback button, Boards marked Preview).
This is a light release: no new tool (89 MCP tools), no new Settings switch, Quick Capture is in 0.3.6.

**The gate** (the same as 0.3.4):

- CI green on all three systems and the Security scan on the release commit.
- A stored `cm-regress` report per shipped system **on the final build** (the files of the tag). A report on candidate files says "candidate
  files of commit <sha>", never "final files".
- The release text with no `<FILL: ...>` left that the reports can fill.
- The full gate (section 1, step 2) is run **once**, on the final tree, as decided; during the night only scoped tests ran.

## What is already done (night of Nov 17, 2026)

- **Built and merged on `main`:** BUG-1 to BUG-18, ANN-12 (line styles), the About Send feedback button (with the title `CaptureMann {version} general feedback`), Boards marked Preview. Version is 0.3.5 in `Cargo.toml`, `Cargo.lock` and the plugin. MCP tool count 89.
  Tested with fakes (unit and integration tests, goldens for ANN-12); BUG-10 and BUG-11 and the line styles, Boards Preview label and intro card were also seen by the maintainer on the dev bundle (reported by the maintainer, no stored report).
- **QA pass 1 (macOS, on the shipped 0.3.4 DMG):** `QA-0.3.5-pass1-macos-2026-11-17-claude.md` (PARTIAL; findings F1 to F5; F4 became BUG-9).
- **BUG-6 (Long Shot on Windows):** `QA-0.3.5-bug6-long-shot-windows-2026-11-17-claude.md` (cause found and fixed; YouTube and carchargeos.com no longer end after 2 frames).
- **QA pass 2:** macOS `QA-0.3.5-pass2-macos-2026-11-17-claude.md` and Windows `QA-0.3.5-pass2-windows-2026-11-17-claude.md` (both PARTIAL, on a dev bundle and a lab build of `7cd4e31d`; no P1; the Windows pass found Q-1 to Q-8, which became BUG-12 to BUG-18 and were fixed after it).
- **`cm-regress` on `7cd4e31d`:** macOS dev bundle standard 51 passed / 1 failed / 12 skipped and full 57 / 1 / 22 (the one failure is the test case `guide.edit_export`, fixed in `5678dc90`; `L1-macos-0.3.5-dev-7cd4e31d-regression-{standard,full}-*`);
  Windows lab build standard 57 / 0 / 7, full 66 / 0 / 14, hardware 68 / 1 / 19 (the one is `rec.camera` 10.1 fps in a dark room; the installed 0.3.4 gave the same; `L1-windows-0.3.5-lab-7cd4e31d-regression-{standard,full,hardware}-*`).
  **These reports predate BUG-12 to BUG-18 and the PhoWhisper clean-up: no regression report names the commit you will tag. Run the standard and full tiers again on the final files (section 3).**
- **Merged after those reports (tested with fakes only, not seen on a screen):** BUG-12 (large replies), BUG-13, 14, 16, 17, 18 (apps/desktop), BUG-15 (cm-capture, Windows code).

## What is still open before the tag

1. **[A] Windows lab check of BUG-12, BUG-15 and BUG-18** on the CI files (or the lab build): a Long Shot of YouTube through `capture screenshot --long --json` returns the capture and no `line too long`; a window capture of a DPI-unaware window (the WinForms test window of the QA agent) fills the frame and a DPI-aware window (Notepad) is unchanged; the export confirmation dialog (`export_guide` outside the library) exposes its body and the exact destination to UI Automation.
2. **[M] A look at BUG-13, BUG-14 and BUG-17 on the dev bundle** (`make run-bundle-dev`; the agents cannot see overlay windows): the New board sheet covers the sidebar Preview pills and the Strip and Free labels show (BUG-13); pause an agent-started guide by moving the target window: the bar says Paused and Resume works (BUG-14); at a window about 660 px tall the rail scrolls and Settings can be reached, in the guide editor and elsewhere (BUG-17). Also look at the About page (button, title in the form), the Markdown export names and Open folder button, and the guide bar of an agent-started guide on a real screen (the macOS agent saw a 440x58 layer-101 window only).
3. **[A] Final regression** on the tagged files (section 3, step 2).
4. **[M] Push** (section 1, step 4): until `feedback.yml` is on `public/main`, the About button lands on the issue chooser (QA Q-6).

## Before the day: decisions

1. **Linux: recommended not to ship Linux files in 0.3.5; Linux stays at 0.3.4.** 0.3.5 changed shared code (`cm-core`, `cm-render`, `cm-daemon`, `cm-cli`, `cm-ai`, the guide engine) and no Linux QA pass or regression has run on it; the ThinkPad is a dual boot, so a Linux run on the final files and the Windows final regression cannot both happen on the same day without a reboot and login in between. Three options:
   - **(Recommended) Skip Linux:** the text says "Linux stays at 0.3.4 (the APT repository is unchanged)"; Linux gets 0.3.5's fixes in 0.3.6. Nothing to build, sign or upload for Linux; skip the APT item.
   - **Ship Linux x86_64 after a run:** boot the ThinkPad into Ubuntu, `packaging/linux/release-build.sh --src <checkout> --commit <sha> --jobs 3`, `packaging/linux/check-package.sh`, then `cm-regress` standard and full and the `.deb` install; sign with minisign; APT update with the 0.3.0 to 0.3.5 `.deb` files. A QA pass 2 on Linux is not part of the plan's exit list, but the text must then say what was not run.
   - **Ship Linux files with a smoke run only** (CI build, `check-package.sh`, `cm-regress` smoke): possible, but the text must say that only a smoke run covers the Linux files.
2. **[M] Windows files.** They come from the GitHub **Windows package** workflow on the tagged commit after you push (it has the speaker engine; the lab build does not: `CM_NO_SPEAKER_MODELS=1`). Download the artifact, install it on the ThinkPad, run the regression on the installed files. Unsigned; SmartScreen will warn (`windows-signing-options.md`).
3. **[M] macOS.** Only you sign and notarize: `make release` (arm64) and `make release CM_ARCH=x86_64` with `CM_SIGN_IDENTITY`, `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH` set; rename the DMGs to `CaptureMann-v0.3.5-macos-<arch>.dmg` and write the `.sha256` again; `xcrun stapler validate` and `spctl` as in `releasing.md`. Never over `target/bundle` while an app runs from it. Intel Mac hardware is not available: the Intel DMG is checked on Apple Silicon only and the text says so.
4. **[M] Trusted folders for `export_guide` (BUG-3).** The agent built "outside the library and the user's export folders" as a new app-only setting `export.folders` plus Documents, Desktop and Downloads. This was an agent assumption; confirm it or name other folders before the tag (the release text names the folders).
5. **[M] winget.** Pull requests 472327 (0.2.8) and 477195 (0.3.1) were open on Nov 11; check `gh pr view <n> -R microsoft/winget-pkgs`. If neither is merged, 0.3.5 ships without winget (`release-day-0.3.2.md`, section 7).
6. **[M] Plugin.** Version 0.3.5 already (`python3 scripts/check_plugin_version.py`); no new tool, so no skill text change is expected (check `plugins/capturemann/skills/*/SKILL.md` for `ambiguous_target` and `style` if you want them mentioned). The directory submission is still held until the organization is Verified.
7. **[M] Windows licence list.** Decision 3 of 0.3.4 is still open: the CI package builder can generate the list; the lab build cannot. Say in the text which one ships.
8. **[M] Release type.** 0.3.4 was published as a full release. Decide again for 0.3.5 (draft first, as before).

## 1. Check the tree

1. [A] `git log --oneline v0.3.4..HEAD | head -100`. Decide the release commit (the one the files are built from). Every report you quote must name it, or the text must name the commit it covers.
2. [A] **The full gate, once, on the merged tree:** `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` (CI toolchain: `cargo +1.100.0 clippy ...`; the default stable has newer lints that fail on old code); `cargo test --workspace --locked` (build with `export SHERPA_ONNX_LIB_DIR=$PWD/packaging/sherpa/out/lib`; the `cm-daemon` integration test `ai_reports` hung once in an agent's run: check whether it hangs on `main` before blaming a change);
   `cargo deny check`; the Trivy gate (run it locally before the push; spike lockfiles broke it on Nov 11); `python3 scripts/check_target_os.py`; `python3 scripts/check_no_bare_command.py`; `python3 scripts/check_md_links.py`; `cargo run -p cm-i18n -- check`; `python3 scripts/check_plugin_version.py`; `python3 scripts/gen_agent_reference.py --check` (89 tools and 105 commands); `make licenses` leaves `THIRD_PARTY_LICENSES.md` unchanged (or commit the regenerated file before the tag).
3. [A] The MCP tool count is 89 (`crates/cm-mcp/tests/stdio.rs` asserts it); the release text says 89.
4. **[M]** Push local commits to `public/main` (`git push public main`) and wait for `ci.yml` and the Security scan. The Windows check had a GPU test process die on 0.3.3 and a rerun is not a failure of the release (note it). **Check that `.github/ISSUE_TEMPLATE/feedback.yml` is on `public/main`** (open the About link as a user would: it must show the form with the version and the title `CaptureMann 0.3.5 general feedback`). Tag the release commit itself and let docs commits sit above it, or tag the commit the files were built from and say both in the text.

## 2. Version and documents

Already done on `main`: `Cargo.toml` and `Cargo.lock` say 0.3.5, the plugin says 0.3.5, the README badge says 0.3.5, `CHANGELOG.md` has an `[Unreleased]` section for 0.3.5 (the 0.3.4 section is dated Nov 17, 2026), `docs/project/status.md` has a "Release 0.3.5 (prepared)" entry, `CLAUDE.md` says "0.3.5 planned for Nov 18".

On the day: `CHANGELOG.md` (`## [Unreleased]` becomes `## [0.3.5] - <date>`; it must be in the tagged commit if the `Release` workflow is used, and a new empty `## [Unreleased]` goes above), `docs/project/status.md` (state, `<date>`, tag commit, run ids), `CLAUDE.md` "Current state" (0.3.5 released, 0.3.6 planned), the header of `specs/plans/release-0.3.5-plan.md` (`Status: released <date>`), the Status lines of ANN-12 in `specs/features/02-annotation.md` (`In progress (...)`, or the wording rule's term; "verified" only with a report).

## 3. Candidates, tag and the final files

1. **Candidates [A, M for macOS signing].** Build from the release commit:
   - **Windows [A]:** the manual **Windows package** workflow (`windows-package.yml`) on the commit after the push (the CI files are the shipping source). The lab build on the ThinkPad (`git archive HEAD`, `packaging\windows\build-windows.ps1`, output `C:\wt\out`, as on Nov 17) has no speaker engine and is only for a quick check.
   - **macOS [M for signing]:** `make release` and `make release CM_ARCH=x86_64` as in decision 3. `make ffmpeg`, `make llama`, `make sherpa` must have run once. For regression on an unsigned candidate: `make bundle-dev`.
   - **Linux:** only if decision 1 says so (commands there).
2. **Regression per shipped system on the candidate files [A]:** `scripts/testlab/regress.sh windows --tier standard --allow screen` and `--tier full` (add `--installed ... --commit <sha>` as the Nov 15 run did; hardware tier with `--allow` as before if you consent); macOS: `scripts/testlab/regress.sh macos --tier standard` and `full` against the DMG's app copy or the dev bundle (screen unlocked, `caffeinate -d -i`, `--timeout-scale 3` for a new binary); Mac check of the CLI/MCP cases from a build tree:
   `cargo build -p cm-daemon -p cm-cli -p cm-mcp -p cm-regress && target/debug/cm-regress --dev --package-dir target/debug --tier standard`. Store as `L1-<system>-0.3.5-<sha>-regression-<tier>-*`. The 5-minute soak (`rec.soak`) needs `--allow mic` and was not run on 0.3.5. The Windows hardware case `rec.camera` is expected to read 10.1 fps in a dark room; light the room.
3. **Manual look [M]:** the items under "still open" 2 above, reported as `verification/<ID>-<date>-maintainer.md`. Use `make run-bundle-dev`, never `/Applications` while a release app runs.
4. **[M] Tag:** `git tag -a v0.3.5 -m "CaptureMann 0.3.5"` on the release commit and `git push public v0.3.5` (never reuse a tag; pushing a tag starts no workflow). Build the final files from the tag.
5. **Candidates against final files:** compare SHA-256 sums; the same sum means the candidate report covers the file; different sums need a smoke run on the final file, stored as `L1-<system>-0.3.5-regression-smoke-final-*` (commands in `release-day-0.3.2.md`, section 4). A failed smoke run stops the release.
6. **Windows files, checksums [A]:** `packaging/windows/release-files.sh` checks the artifact against its `.sha256` files, writes `SHA256SUMS.txt` and renders the winget manifest. Names: `capturemann-0.3.5-windows-x64-setup.exe`, `capturemann-0.3.5-windows-x64.zip`. **Run `scripts/check-release-assets.sh 0.3.5 <staging folder>` before uploading** (note: it expected both Linux architectures in 0.3.4; if Linux is skipped, check how it handles a folder without Linux files and run it on exactly what you upload).
7. Not run and listed in the text: Intel Mac hardware, Windows 10, SmartScreen after "Keep", mixed DPI, a second monitor, NVIDIA and AMD encoders, Linux (unless decided), the 5-minute soak, Retype on Windows, BUG-10 on non-English speech other than the maintainer's Vietnamese recording, the floating windows by eye (unless the maintainer's look is stored).

## 4. Fill the release text and publish (draft first)

1. [A] In `release-0.3.5-github-text.md` replace every `<FILL: ...>` (commit sha, report names and counts per tier, the SHA-256 table, notarization, the winget sentence, the Homebrew line, the plugin sentence, the licences sentence from a diff of `Cargo.lock` and `THIRD_PARTY_LICENSES.md` against `v0.3.4`, the CHANGELOG date); fix the Linux line to the decision. Check that no line says "passed" or "verified" without a stored report. Save the "Release body" section as `release-body.md` (not committed).
2. **[M] Create the draft:** `gh release create v0.3.5 -R craft-mann/capturemann --draft --title "CaptureMann 0.3.5: fixes, dashed lines and a feedback button" --notes-file release-body.md` (add `--prerelease` if you decide so). Upload the files: 2 DMGs, 2 Windows files (Linux files only if shipped), their `.sha256` (and `.minisig`), `SHA256SUMS.txt`. Run `scripts/check-release-assets.sh 0.3.5 <folder>` on the folder and against the draft's list. Download one file per system in a browser and check it. Then **[M] publish.** A draft creates no tag and publishing creates a lightweight tag on the target: make sure the target is the release commit.

## 5. After the release is public

- **Homebrew cask [M]:** `scripts/update-homebrew-cask.sh 0.3.5 /Users/trungtran/source/repos/homebrew-tap` (never a temporary clone; the script runs the asset check first), commit, push, verify the push through the API. Before any install test run `brew list` and look in `/Applications` (the Nov 4 test removed the maintainer's app).
- **winget [M]:** the three cases of `release-day-0.3.2.md`, section 7; `winget validate --manifest <win-out>\winget\manifests\c\CaptureMann\CaptureMann\0.3.5` on the ThinkPad first.
- **APT [M]:** only if Linux shipped (decision 1); otherwise **the APT repository stays as it is (0.3.0 to 0.3.4)** and nothing is uploaded.
- **Claude plugin [M]:** `claude plugin marketplace add craft-mann/capturemann` and `claude plugin install capturemann@capturemann-plugins` against the installed 0.3.5 app (`tests/manual/PLUGIN-1.md`); report in `verification/`. The plugin sentence stays "held by the directory until the organization is Verified" unless that changed.
- **Website and docs [M]:** download links at <https://www.craftmann.app/capturemann/>; README badge is already 0.3.5; `docs/user/user-guide.md` for line styles, Boards (Preview) and the About button.
- **Post-release checks [A]:** download each asset from the public release and compare with `SHA256SUMS.txt`; `capturemann --version` on the installed Windows and macOS files says 0.3.5; the Homebrew cask installs into a throwaway `--appdir`.
- **Housekeeping [A]:** `CHANGELOG.md` date, `docs/project/status.md`, `CLAUDE.md` "Current state", the plan header, move the open items (`release-0.3.6-plan.md`: Quick Capture, short-audio `[unclear]` work, the Windows licence list, in-app update, windowed audio, headless daemon, WebP and HEIC import, and the QA findings that stay open: F1, F2, F3 and F5 are in `release-0.3.6-plan.md` section 2b; **G3 (the last word of a transcript piece has start equal to end; may predate 0.3.5) and G4 (the ambiguous-window error lists other apps' window titles) are not yet in it: add them**), delete the worktrees of merged agents with their `target/` folders (the disk filled up on Nov 5), `git worktree prune`. On the lab laptop remove `C:\wt` and `C:\lab` leftovers when no longer needed (the QA agent left the built zip and setup in `C:\wt\out`).

## Rollback

`releasing.md`, "Rollback and yank". In short: draft not published, delete the draft and the tag and tag again; published and broken, mark it a pre-release or back to draft, add a note on top, do not delete assets others have sums for; dangerous (data loss, leaked secret), delete the assets, say so, publish 0.3.6 (never reuse a tag). A bad file is replaced by a new tag, never an edited asset. Homebrew: revert the cask commit in `homebrew-tap` and push. winget: close or update the pull request. APT: re-run `make-apt-repo.sh` without the bad `.deb` and upload with `--clobber`. Update the website download link to 0.3.4 meanwhile. A Developer ID key that leaked is revoked at Apple.

## Lessons carried over from 0.3.4

- The GitHub "Windows package" workflow gives the Windows files **with** the speaker engine; the lab build uses `CM_NO_SPEAKER_MODELS=1`. The CI files are the shipping source.
- A draft release creates no tag; publishing it creates a lightweight tag on the target commit. Make sure the target is the release commit before publishing.
- `scripts/check-release-assets.sh` expects both Linux architectures; run it on the folder before the draft.
- The first CI run can find real defects (0.3.4: `cm-mcp` overflowed the 1 MB main-thread stack on Windows in a debug build; Linux clippy and test thresholds).
- The first launch of each new macOS binary is slow (`syspolicyd`, XProtect scanning): run `cm-regress` with `--timeout-scale 3`.
- A locked Mac screen skips every desktop-session case and can fail `desktop.hotkey_overlay` ("no displays") mid-run: unlock and run under `caffeinate -d -i`.
- macOS notarization needs the Apple developer agreement accepted in the account first.
- The ThinkPad has no auto-logon: after each reboot log in to Windows once, or the session cases cannot run. The lab IP changes between home and office (Nov 17: 192.168.0.101; `scripts/testlab/lab.sh status` and `~/.config/capturemann-lab`).
- The overlay windows (bars, HUD, picker, toasts, confirm dialogs) are excluded from screen capture: QA agents read them through the accessibility tree and nobody has looked at them unless a report says so.
- `clippy` is run with the CI toolchain (`cargo +1.100.0 clippy`); the default stable fails on lints in untouched old code.
- A QA agent's `--window <text>` capture once took a private window: use window ids; BUG-9 now refuses an ambiguous title (CLI).
