# Release day, 0.2.8 (Windows): exact steps for the maintainer

Written Nov 5, 2026 from how 0.2.7 was done (commits `407acfa3` version bump, `b90e6dd5` README and status, `7e550b69`
"published" status, then `releasing.md` steps 6 to 9) and from the Windows files of this branch. It adds the details to
`windows-0.2.8-release-checklist.md` (read that for the reasons). Text to publish: `release-0.2.8-github-text.md`.
"M" = only the maintainer does this (merge, push, tag, sign, publish, accounts, decisions). Run commands from the repo root.

## Before the day: decisions that block the text

1. **[M]** Is a macOS DMG (or Linux file) part of the 0.2.8 tag? The text assumes **no**: Windows files only, a pre-release.
2. **[M]** Transcription track (invented outro text): if nothing was merged for it, keep the "still a known fault" line;
   otherwise replace it with the measured result and its report.
3. **[M]** Vietnamese: the 2268 draft entries (`locales/` review sheet, commit `bf7523ee`) are reviewed, or the "draft
   translations" line stays in the text.
4. **[M]** `llama-server` model rows in `crates/cm-ai/data/text_models.json` are still `pending_pin`: pin them or leave
   the local text model as "download not offered yet" in the text.
5. Open points found in the reports and not yet fixed on hardware (see "Stale or open" in the hand-off): decide whether
   each is in the text as a limitation (it is) or blocks the release.

## 1. Version bump (one commit, on the branch that becomes `main`)

The workspace version is the only source: `capturemann diagnostics`, About, the sidebar badge, the daemon/CLI hello,
`build-windows.ps1` (file names, installer version through `cargo metadata`), `installer.nsi` and the macOS bundle all read
`CARGO_PKG_VERSION`. A package built before the bump says 0.2.7 (the lab laptop reports "CaptureMann 0.2.7" for that reason).

| File | Change |
| --- | --- |
| `Cargo.toml` line 9, `[workspace.package] version` | `"0.2.7"` to `"0.2.8"` |
| `Cargo.lock` | `cargo update -w` (moves every workspace crate to 0.2.8; no other crate changes) |
| `CHANGELOG.md` | add `## [0.2.8] - YYYY-MM-DD` under `## [Unreleased]` (text at the end of `release-0.2.8-github-text.md`); `release.yml` reads it for any macOS run |
| `README.md` lines 21 and 31 to 40 | badge `version-0.2.7` and `Version 0.2.7` to 0.2.8; rewrite the "Status" paragraph: Windows now records (see the stale list) |
| `docs/project/status.md` | the "Windows 0.2.8 track" section becomes "Release 0.2.8 (Windows)" with the date, tag, commit, file sums; keep "published" out until step 8 |
| `specs/plans/release-0.2.8-plan.md` | header `Status: released <date>` and the outcome |
| `packaging/windows/installer.nsi` line 2 | comment example `/DVERSION=0.2.6 /DVI_VERSION=0.2.6.0` to 0.2.8 (text only) |

Not to edit: `THIRD_PARTY_LICENSES.md`. The checked-in file is the **macOS** crate set; the Windows build regenerates its
own list (`build-windows.ps1`, `CM_SKIP_LICENSES` skips that). Check: `make licenses` must leave `git diff
THIRD_PARTY_LICENSES.md` empty; on Nov 5, 2026 `bash packaging/macos/gen-third-party.sh <tmp>` at this branch's head gave a
file identical to the committed one (554 crates). A workspace version bump does not change it. Commit it only if it differs.

Then the checks (the CI jobs `ci.yml` and Security scan run on the merge too):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
python3 scripts/check_target_os.py
cargo run -p cm-i18n -- check
```

## 2. Merge and tag

1. **[M]** Merge `track/windows` (with the bump) into `main`; push; wait for `ci.yml` and Security scan to pass.
2. **[M]** `git tag -a v0.2.8 -m "CaptureMann 0.2.8"` and `git push public v0.2.8` (the remote is `public`; never reuse a tag).

## 3. Build the Windows files

1. **[M]** GitHub, Actions, **Windows package**, Run workflow, ref `v0.2.8` (or the merge commit). Pushing a tag does not
   start it. About 15 to 35 minutes; it builds the zip and installer, the LGPL FFmpeg and `llama-server`, and runs the
   smoke and silent-install steps. The run creates no release.
2. Download the artifact with the file list, the checksums and the winget manifest in one go:

   ```sh
   mkdir -p ~/release-0.2.8/art && gh run download <run id> -R craft-mann/capturemann -n capturemann-windows-x64 -D ~/release-0.2.8/art
   packaging/windows/release-files.sh ~/release-0.2.8/art ~/release-0.2.8/out
   ```

   `release-files.sh` finds the zip and the installer at any depth, recomputes both SHA-256 sums, stops if one differs from
   its `.sha256` file, writes `out/SHA256SUMS.txt`, and renders the winget manifest into
   `out/winget/manifests/c/CaptureMann/CaptureMann/0.2.8/` with the installer URL
   `https://github.com/craft-mann/capturemann/releases/download/v0.2.8/capturemann-0.2.8-windows-x64-setup.exe`. It changes
   nothing else. Dry run on the newest successful package run on Nov 5, 2026 (run 42010089424, files named 0.2.7): it printed
   `987cf943...bb31b *capturemann-0.2.7-windows-x64.zip` and `4763088b...012621 *capturemann-0.2.7-windows-x64-setup.exe`,
   `shasum -a 256 -c` said OK for both, and the manifest was written with the uppercase installer hash.
3. Put the two sums into the release text (`<sha256 zip>`, `<sha256 installer>`).

## 4. Check the package (before anything is public)

1. **[M or the lab agent]** Regression on the laptop against this exact package:
   `scripts/testlab/regress.sh windows --tier standard --allow screen --package-run <run id>`, then
   `--tier hardware --allow screen,sound,mic,camera,second_monitor --installer <setup.exe> --soak-minutes 3`
   (the installer case, the tray case and the 4K case need the matching flag or the second monitor). Read the failures.
   Store the reports in `verification/` and change the "How it was tested" paragraph of the text to this run's numbers
   (the stored runs are on commit `668eef82`, package run 42010089424; later commits are not covered by any stored run).
2. Install once by hand on a machine that is not the build machine, start the app, take a screenshot, make a 10 s
   recording, uninstall. Store a report if the text says anything about it.
3. Optional, not run so far: SmartScreen on a real browser download, Windows 10 22H2, a second monitor with mixed scaling.
   The text lists them as not run; remove a line only with a stored report.

## 5. Publish the GitHub release

1. **[M]** Draft first, as a pre-release (the Windows files are the only assets):

   ```sh
   gh release create v0.2.8 --draft --prerelease -R craft-mann/capturemann \
     --title "CaptureMann 0.2.8: recording comes to Windows" --notes-file release-body.md \
     ~/release-0.2.8/art/.../capturemann-0.2.8-windows-x64-setup.exe  <its .sha256> \
     ~/release-0.2.8/art/.../capturemann-0.2.8-windows-x64.zip         <its .sha256> \
     ~/release-0.2.8/out/SHA256SUMS.txt
   ```

   (`release-body.md` = the "Release body" section of `release-0.2.8-github-text.md`.) Review the draft, download the
   installer from it in a browser on a Windows PC (this is also the first SmartScreen look), then **publish**.
2. Do not use `release.yml` for Windows: it builds macOS only.

## 6. winget (after the release is public)

The installer URL must answer before the pull request.

1. On a Windows PC with winget: `winget validate --manifest <out>\winget\manifests\c\CaptureMann\CaptureMann\0.2.8` and
   `winget install --manifest <that folder>` (this needs "Enable local manifest files": `winget settings --enable LocalManifestFiles`).
2. **[M]** Fork `microsoft/winget-pkgs`, copy the folder `manifests/c/CaptureMann/CaptureMann/0.2.8/` to the same path in
   the fork, open the pull request (title `New version: CaptureMann.CaptureMann version 0.2.8`; for the first submission
   `New package: ...`). Expect questions about the unsigned installer; the validation pipeline runs the installer silently
   with `/S`. Note: the template says `MinimumOSVersion: 10.0.19045.0` (Windows 10 22H2) while Windows 10 is untested; keep
   or change it before submitting.
3. After the merge, add the `winget install CaptureMann.CaptureMann` line to the release body (edit the release).

## 7. Homebrew cask

None for 0.2.8 if no DMG is attached (the cask is macOS only and still points at 0.2.7, which stays correct). If a DMG is
shipped: `scripts/update-homebrew-cask.sh 0.2.8 ~/source/repos/homebrew-tap` after the release is public, commit and push the
tap, test with `brew install --cask` from a throwaway tap (`releasing.md` step 9). **Before any install test, check
`brew list` and `/Applications` first** (the Nov 4 test removed the maintainer's app).

## 8. After publishing

1. **[M]** Commit to `main`: `docs/project/status.md` ("published", tag, commit, file sums, winget link or "submitted"),
   `specs/plans/release-0.2.8-plan.md`, the README status line. Push.
2. Update the website download link (manual; <https://www.craftmann.app/capturemann/>) and announce. **[M]**
3. Clean up: remove `~/release-0.2.8`, the `capturemann-wt/*` worktrees and their `target/` folders (disk filled up on Nov 5).
4. Rollback: `releasing.md` "Rollback and yank"; a bad Windows file is replaced by a new tag (0.2.9 is Linux, so use 0.2.8.1 or
   agree the number first), never an edited asset.
