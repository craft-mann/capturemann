# Release day, 0.3.4 (Windows, macOS and Linux): steps for the maintainer

Written on Nov 15, 2026 from `release-day-0.3.3.md` (the pattern) with the version changed and the 0.3.4 items added.
"[M]" = only the maintainer does this (push, tag, sign, notarize, publish, accounts, decisions). "[A]" = an agent can do it
from this file. Run commands from the repo root. Text to publish: `release-0.3.4-github-text.md` (its `<FILL: ...>` items need the
final files and their reports). Plan: `specs/plans/release-0.3.4-plan.md`; state of the night: `release-0.3.4-morning-report.md`.
**Run on Nov 17, 2026: the release was published (tag `v0.3.4` on `1a1586d0`, 03:34 UTC); see "Done on Nov 17" at the end for what was done, what is still open and the lessons.** **Blanks to fill on the day are written `<FILL: ...>`, `<date>`, `<sha>`, `<run id>`.**

**The gate** (the same as 0.3.3, with what exists today written next to it):

- CI green on all three systems and the Security scan on the release commit.
- A stored `cm-regress` report per shipped system **on the final build**. State on Nov 15, 2026 night: Windows full 66/0 and hardware
  68/0 on the installed build of `84bc00cd` (`L1-windows-0.3.4-84bc00cd-final-regression-*`); macOS full 57/0, standard 49/0, hardware 57/0 on
  a **dev bundle of `26a05100`** (Nov 14; `L1-macos-0.3.4-dev-bundle-26a05100-regression-*`). `main` has moved on since both (Long Shot,
  Click Guide, Retype and Blueprint fixes, Auto text colour, the Published page removal; see `git log --oneline 84bc00cd..HEAD`). **So no
  regression report names the commit you will tag; run the standard and full tiers again on the final candidates (section 3).** A report on
  candidate files says "candidate files of commit <sha>", never "final files".
- The release text with no `<FILL: ...>` left that the reports can fill.

## Before the day: decisions

1. **Linux ships in 0.3.4 (decided Nov 16, 2026).** Linux x86_64 candidate files of `596c7956` ran standard 57/0, full 65/0 and the `.deb` install 1/0 (`L1-linux-0.3.4-596c7956-*`); a run on the final commit and the aarch64 VM are not done. Steps: `packaging/linux/release-build.sh --src <checkout> --commit <sha> --jobs 3` on the ThinkPad (`scripts/testlab/lab.sh switch ubuntu`) and in the VM, `packaging/linux/check-package.sh`, `scripts/testlab/regress.sh linux --tier standard` and `--tier full`, `packaging/linux/sign-linux.sh` [M], the APT update with the 0.3.0 to 0.3.4 `.deb` files [M].
2. **[M] Which audit P1 items are built before the tag** (see `release-0.3.4-morning-report.md`, "Decisions waiting"). Whatever is not built goes into
   "Known limits" of the text, which already lists the main ones.
3. **[M] Windows licence list.** The Windows package builder printed "the Windows third-party licence list could not be generated (needs bash
   and python3)" and shipped the checked-in macOS list (`QA-0.3.4-windows-final-build-2026-11-15-claude.md`). Either fix it (run
   `make licenses` on the Mac, commit `THIRD_PARTY_LICENSES.md`, so the checked-in list is current; or put Git Bash and Python on the lab
   laptop's PATH so `build-windows.ps1` can generate its own, lines 267 to 290) or accept it and say so in the text ("the licence list is the
   macOS one"). The cargo tree is the same on every system except the platform crates, so the difference is the Windows-only crates; check with
   `cargo deny list` for the Windows target if you want to be sure. [A] can do the fix on the Mac side.
4. **[M] winget.** Pull requests 472327 (0.2.8) and 477195 (0.3.1) were open on Nov 11 and the status after that is not known to this file:
   `gh pr view 472327 -R microsoft/winget-pkgs`, same for 477195. If neither is merged, 0.3.4 ships without winget (the three cases are in
   `release-day-0.3.2.md`, section 7).
5. **[M] macOS notarization.** Only you sign and notarize: the Developer ID identity and the App Store Connect API key are yours. The DMGs of 0.3.2
   and 0.3.3 were built on your Mac with `make release` and `make release CM_ARCH=x86_64`. Without notarization the text says so.
6. **[M] Plugin.** The plugin version is 0.3.4 already (`python3 scripts/check_plugin_version.py` prints "plugin version 0.3.4 matches the
   workspace"). The skill text may need the new tools (Long Shot, the five Click Guide tools, `create_board`, `export_images`); check
   `plugins/capturemann/skills/*/SKILL.md`. The directory submission is still held until the organization is Verified
   (`plugin-submission-0.3.2.md`).
7. **[M] Windows signing.** 0.3.4 ships **unsigned** (not in the plan; `windows-signing-options.md`). SmartScreen will warn.
8. No Intel Mac is available: the Intel DMG is checked on Apple Silicon only and the text says so.

## 1. Check the tree

1. [A] `git log --oneline v0.3.3..HEAD | head -100`. Decide which commit is the release commit (the one the candidate files are built from).
   Every report in `verification/` named `L1-*-0.3.4-*` that you want to quote must name it, or the text must name the commit it covers.
2. [A] Checks on the merged tree (CI runs them too): `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`;
   `cargo test --workspace --locked`; `cargo deny check`; the Trivy gate (run it locally before the
   merge; spike lockfiles broke it on Nov 11); `python3 scripts/check_target_os.py`; `python3 scripts/check_no_bare_command.py`;
   `python3 scripts/check_md_links.py`; `cargo run -p cm-i18n -- check`; `python3 scripts/check_plugin_version.py`; `make licenses` leaves
   `THIRD_PARTY_LICENSES.md` unchanged (or the regenerated file is committed before the tag).
3. [A] The MCP tool count is 89 (`crates/cm-mcp/tests/stdio.rs` asserts it); the release text says 89.
4. **[M]** Push local commits to `public/main` (`git push public main`) and wait for `ci.yml` and the Security scan. The Windows check had a GPU test
   process die twice on 0.3.3; a rerun is not a failure of the release, but note it. Tag the release commit itself and let docs commits sit above
   it (or tag the commit the files were built from and say both in the text, as 0.3.0 did).

## 2. Version and documents

Already done on `main` (checked Nov 15, 2026): `Cargo.toml` and `Cargo.lock` say 0.3.4, `plugins/capturemann/.claude-plugin/plugin.json` says
0.3.4, the sidebar shows `v0.3.4`. **Still to do:** the README badge (`img.shields.io/badge/version-0.3.3-blue` in `README.md` line 21) and the README
status paragraphs; the marketplace entry in `.claude-plugin/marketplace.json` has no version field (nothing to change; confirm).

When the date is known: `CHANGELOG.md` (`## [Unreleased]` becomes `## [0.3.4] - <date>`; the section must be in the tagged commit if the `Release`
workflow is used), `docs/project/status.md` (Release 0.3.4 entry with `<date>`, tag commit, run ids), the "Current state" of `CLAUDE.md` (0.3.4
released, 0.3.5 planned), the header of `specs/plans/release-0.3.4-plan.md` (`Status: released <date>`), the Status lines of CAP-6, ANN-8 to ANN-11,
GDE-1 to GDE-4, BRD-1, BLK-1 (`In progress (...)`, or `Ready` only as the wording rule allows; "verified" only with a report).

## 3. Candidates, tag and the final files

1. **Candidates [A, M for macOS signing].** Build from the release commit:
   - **Windows [A]:** the manual **Windows package** workflow (`windows-package.yml`) on the commit, or the lab build on the ThinkPad as on Nov 15
     (`git archive HEAD` to `C:\wt\main`, `packaging\windows\build-windows.ps1`, output in `C:\wt\out`; the scheduled-task wrapper `rc2_build.ps1` and the
     `HKCU\Console\%%Startup` delegation keys are described in `QA-0.3.4-final-build-windows-2026-11-15-claude.md`; the build takes about 20 minutes with LTO off).
     **The lab build uses `CM_NO_SPEAKER_MODELS=1`: the speaker engine is not in those files.** A CI build may differ; say which one ships.
   - **macOS [M for signing]:** `make release` (arm64) and `make release CM_ARCH=x86_64` (Intel), with `CM_SIGN_IDENTITY`, `APPLE_API_KEY_ID`,
     `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH` set (`releasing.md`, "Local release"). The chain is `bundle-release`, `dmg`, `notarize`, `verify-release`;
     `make ffmpeg`, `make llama`, `make sherpa` must have run once. `make release` writes `CaptureMann-<v>-<arch>.dmg`: **rename it to
     `CaptureMann-v0.3.4-macos-<arch>.dmg` and write the `.sha256` again** (`releasing.md`, "Asset names"). Staple is part of `notarize.sh`; confirm
     with `xcrun stapler validate <dmg>` and `spctl -a -t open --context context:primary-signature -v <dmg>`. **Do not run any of this over
     `target/bundle` while an app runs from it.** For regression on the candidate without signing: `make bundle-dev` (the dev bundle is
     `CaptureMann-dev.app`, bundle id `dev.capturemann.app.dev`).
   - **Linux (only if decision 1 says so) [M for signing]:** `packaging/linux/release-build.sh --src <checkout> --commit <sha> --jobs 3` on the ThinkPad
     (x86_64) and in the VM (aarch64), then `packaging/linux/check-package.sh`.
2. **Regression per shipped system on the candidate files [A]:** `scripts/testlab/regress.sh windows --tier standard --allow screen` and `--tier full`
   (add `--installed ... --commit <sha>` as the Nov 15 run did); macOS: `scripts/testlab/regress.sh macos --tier standard` (and `full`, `hardware` with
   your consent) against the candidate DMG or the dev bundle; Mac check of the CLI/MCP cases from a build tree:
   `cargo build -p cm-daemon -p cm-cli -p cm-mcp -p cm-regress && target/debug/cm-regress --dev --package-dir target/debug --tier standard`.
   Store as `L1-<system>-0.3.4-<sha>-regression-<tier>-*`. The 5-minute soak (`rec.soak`) needs `--allow mic` and has not been run on 0.3.4;
   run it if you consent to the microphone.
3. **Manual scripts [M]:** the macOS desktop session you have not had yet on 0.3.4 (the QA agents could not look at floating windows: Long Shot
   bar and HUD, Click Guide bar, countdown, picker, toasts; `ux-proposals-0.3.4-for-design.md`, section 1, "Important limit of the evidence").
   Use `make run-bundle-dev`, never `/Applications` while a release app runs. Report as `verification/<ID>-<date>-maintainer.md`.
4. **[M] Tag:** `git tag -a v0.3.4 -m "CaptureMann 0.3.4"` on the release commit and `git push public v0.3.4` (never reuse a tag; pushing a tag
   starts no workflow). Build the final files from the tag exactly as in step 1.
5. **Candidates against final files:** compare SHA-256 sums; the same sum means the candidate report covers the file; different sums need a smoke run
   on the final file, stored as `L1-<system>-0.3.4-regression-smoke-final-*` (commands in `release-day-0.3.2.md`, section 4). A failed smoke run stops the release.
6. **Windows files, checksums [A]:** `packaging/windows/release-files.sh` checks the artifact against its `.sha256` files, writes `SHA256SUMS.txt` and
   renders the winget manifest. Names: `capturemann-0.3.4-windows-x64-setup.exe`, `capturemann-0.3.4-windows-x64.zip`. **Check the asset names
   and sums with `scripts/check-release-assets.sh 0.3.4 <staging folder>` before uploading** (it fails on a wrong name, a mismatching `.sha256`,
   a missing `.minisig` or a `SHA256SUMS.txt` that disagrees).
7. Not run and listed in the text: Intel Mac hardware, Windows 10, SmartScreen after "Keep", mixed DPI, NVIDIA and AMD encoders, a second monitor,
   Linux (unless decided), a Drive account for the paused-upload delete (B1), the 5-minute soak without a microphone, hand scroll through the real HUD.

## 4. Fill the release text and publish (draft first)

1. [A] In `release-0.3.4-github-text.md` replace every `<FILL: ...>` (commit sha, report names and counts per tier, the SHA-256 table, notarization,
   the winget sentence, the Homebrew line, the plugin sentence, the licences sentence from a diff of `Cargo.lock` and `THIRD_PARTY_LICENSES.md`
   against `v0.3.3`, the CHANGELOG date); delete the Linux lines if Linux is not shipped. Check that no line says "passed" or "verified" without a stored
   report. Save the "Release body" section as `release-body.md` (not committed).
2. **[M] Create the draft:** `gh release create v0.3.4 -R craft-mann/capturemann --draft --prerelease --title "CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards" --notes-file release-body.md`
   (or use the macOS draft the `Release` workflow made, if the secrets exist). Upload the files: 2 DMGs, 2 Windows files, (4 Linux packages), their `.sha256`
   (and `.minisig`), `SHA256SUMS.txt`. Run `scripts/check-release-assets.sh 0.3.4 <folder>` on the folder and against the draft's list. Download one file
   per system in a browser and check it. Then **[M] publish.**

## 5. After the release is public

- **Homebrew cask [M]:** `scripts/update-homebrew-cask.sh 0.3.4 /Users/trungtran/source/repos/homebrew-tap` (never a temporary clone; the script runs the asset check first),
  commit, push, verify the push through the API. Before any install test run `brew list` and look in `/Applications` (the Nov 4 test removed the maintainer's app).
- **winget [M]:** the three cases of `release-day-0.3.2.md`, section 7; `winget validate --manifest <win-out>\winget\manifests\c\CaptureMann\CaptureMann\0.3.4` on the ThinkPad first.
- **APT [M], only if Linux shipped:** `make-apt-repo.sh` with the `.deb` files of **0.3.0, 0.3.1, 0.3.2, 0.3.3 and 0.3.4**, upload to release `apt` with `--clobber`,
  then on a clean Ubuntu 24.04 `apt install capturemann` and `capturemann --version` says 0.3.4. If Linux is not shipped, **the APT repository stays as it is
  (0.3.0 to 0.3.3)** and nothing is uploaded.
- **Claude plugin [M]:** `claude plugin marketplace add craft-mann/capturemann` and `claude plugin install capturemann@capturemann-plugins` against the installed 0.3.4 app
  (`tests/manual/PLUGIN-1.md`); report in `verification/`. The plugin sentence in the text and README stays "held by the directory until the organization is Verified" unless that changed.
- **Website and docs [M]:** download links at <https://www.craftmann.app/capturemann/>; README badge and status paragraphs; `docs/user/user-guide.md` for Long Shot, Click Guide, boards.
- **Post-release checks [A]:** download each asset from the public release and compare with `SHA256SUMS.txt`; `capturemann --version` on the installed Windows and macOS files says 0.3.4; the
  Homebrew cask installs into a throwaway `--appdir`.
- **Housekeeping [A]:** `CHANGELOG.md` date, `docs/project/status.md`, `CLAUDE.md` "Current state", the README, the plan header, move the open items (the audit P1 and P2 list,
  `ux-proposals-0.3.4-for-design.md` decisions not taken, the Windows licence list, in-app update, windowed audio, headless daemon, WebP and HEIC import) to a
  `release-0.3.5-plan.md`, remove the blue "new" dots from the editor as decided, delete `~/release-0.3.4` and the worktrees of merged agents with their `target/` folders
  (the disk filled up on Nov 5), `git worktree prune`. On the lab laptop remove `C:\wt` and `C:\lab` leftovers when no longer needed.

## Rollback

`releasing.md`, "Rollback and yank". In short: draft not published, delete the draft and the tag and tag again; published and broken, mark it a pre-release or back to draft, add a note on top, do
not delete assets others have sums for; dangerous (data loss, leaked secret), delete the assets, say so, publish 0.3.5 (never reuse a tag). A bad file is replaced by a new tag, never an edited asset.
Homebrew: revert the cask commit in `homebrew-tap` and push. winget: close or update the pull request. APT: re-run `make-apt-repo.sh` without the bad `.deb`
and upload with `--clobber`. Update the website download link to 0.3.3 meanwhile. A Developer ID key that leaked is revoked at Apple.

## Done on Nov 17, 2026 (release day record)

**Done:** Linux ships (decision 1); candidates and final files built from `1a1586d0`; regression on the final files of all four targets (macOS DMG app copy, Windows installed CI setup, Linux x86_64, Linux aarch64; reports `L1-*-0.3.4-1a1586d0-*` in `verification/`); CI and the Security scan green on `1a1586d0`; macOS DMGs signed, notarized and stapled (section 3, step 1); Linux `.minisig` files; draft release, `scripts/check-release-assets.sh`, publish (full release, 25 files); Homebrew cask at 0.3.4 (tap commit `7789f42`, verified through the API); the release text filled from the reports.

**Still open [M]:** winget (PRs 472327 and 477195), the APT repository update with the 0.3.0 to 0.3.4 `.deb` files, the plugin line, the website, the README badge and status paragraphs, `CHANGELOG.md` date, `release-0.3.5-plan.md`, the Windows licence list (decision 3), Windows signing.

**Lessons:**

- The GitHub "Windows package" workflow gives the Windows files **with** the speaker engine; the lab build uses `CM_NO_SPEAKER_MODELS=1`. The CI files are the shipping source.
- A draft release creates no tag; publishing it creates a lightweight tag on the target commit. Make sure the target is the release commit before publishing.
- `scripts/check-release-assets.sh` expects both Linux architectures; run it on the folder before the draft.
- The first CI run found real defects: `cm-mcp` overflowed the 1 MB main-thread stack on Windows in a debug build (now 8 MB worker stacks), plus Linux clippy and test thresholds (see the morning report).
- The first launch of each new macOS binary is slow (`syspolicyd`, XProtect scanning): run `cm-regress` with `--timeout-scale 3`.
- A locked Mac screen skips every desktop-session case and can fail `desktop.hotkey_overlay` ("no displays") mid-run: unlock and run under `caffeinate -d -i`; the locked-screen reports are kept but labelled.
- macOS notarization needs the Apple developer agreement accepted in the account first.
- The ThinkPad has no auto-logon: after each reboot the maintainer logs in to Windows once, or the session cases cannot run.
