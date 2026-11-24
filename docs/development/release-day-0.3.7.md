# Release day, 0.3.7 (Windows, macOS, Linux): steps

Written on Nov 24, 2026 from `release-day-0.3.6.md` (the pattern; its "Tag and publish" steps apply unchanged with 0.3.7). "[M]" = only the maintainer does this (tag, sign, notarize, publish, accounts, decisions). "[A]" = an agent can do it from this file. Text: `release-0.3.7-github-text.md` (its `<FILL: ...>` items). Plan: `specs/plans/release-0.3.7-plan.md`. Version 0.3.7 in `Cargo.toml`, `Cargo.lock` and the plugin; MCP tools 90.

## State on the night of Nov 24, 2026 (nothing is pushed)

- Everything is on local `main` (about 50 commits above `v0.3.6`). **Push is the maintainer's** [M]: `git push public main`, then wait for CI and the Security scan.
- **Full gate on the tree of the night:** see the line "Gate" in `docs/project/status.md` (section 0.3.7) for the result; fmt, workspace clippy and workspace tests were run once on `<FILL: commit>`; `cargo deny check` and the Trivy scan: <FILL: result or "not run">.
- **Regression (cm-regress), PARTIAL reports:** Windows lab build and macOS dev bundle as listed in the text; Linux needs the ThinkPad booted into Ubuntu (`scripts/testlab/lab.sh switch ubuntu`, it timed out once and needed a power cycle). The final files of each system get a second run before the tag (below).
- **Not done in 0.3.7:** the D4 measurements (`silence_probe`, `quiet_silence`, `outro_guard`; the limit stays and the text says so) and the last-word check on a long real recording (needs the maintainer's consent for a real recording).
- **Maintainer's own checks still open:** the bar's strip, return and default place on macOS (reported "look good" on the Nov 24 bundle), the macOS collision of ⌥⇧4 and ⌥⇧5 with system shortcuts, the toast fix on the ThinkPad (Windows, rename `ffmpeg.exe` to see the long message), "same layer" (the bar returns at the same place and display; whether it should also return behind or in front of other windows was asked and is not answered).

## Steps

1. **[M] Review** the plan, `CHANGELOG.md` (`## [0.3.7] - Unreleased`), the release text and this file.
2. **[M] Push** `main` to `public`; wait for CI green and the Security scan. Fix a red check before anything else.
3. **[A/M] Windows files:** run the GitHub "Windows package" workflow on the pushed code commit; download the artifact; install on the ThinkPad; `scripts/testlab/regress.sh windows --tier full --allow screen --package-run <run id> --commit <sha>` (standard and hardware as in 0.3.6); reports into `verification/` (`L1-windows-0.3.7-ci-<sha>-*`).
4. **[M] macOS:** `make release` for arm64 and x86_64 (hardened, DMG, notarize, `verify-release.sh`); `scripts/testlab/regress.sh macos --installer <dmg> --tier standard` and hardware; reports `L1-macos-0.3.7-dmg-<sha>-*`.
5. **[A/M] Linux:** `packaging/linux/release-build.sh` on the ThinkPad (x86_64) and the aarch64 VM (`reference-lab-access` memory has the recipe); `check-package.sh`; `regress.sh linux` on X11 (and Wayland if time); **[M]** minisign the eight files with `packaging/linux/minisign.pub`'s key.
6. **[A]** Stage the 25 files in `/Users/trungtran/release-0.3.7/upload/`; `scripts/check-release-assets.sh 0.3.7 <folder>`.
7. **[M]** `CHANGELOG.md` date, `CLAUDE.md` ("0.3.7 is released"), `docs/project/status.md`, plan header; commit and push; `git tag -a v0.3.7 -m "CaptureMann 0.3.7"` on the docs commit and `git push public v0.3.7`.
8. **[A]** `packaging/windows/release-files.sh 0.3.7` (checksums, winget manifest) and the asset check again.
9. **[M]** `gh release create v0.3.7 -R craft-mann/capturemann --draft --title "CaptureMann 0.3.7: bug fixes and a tidier sidebar" --notes-file <body>`; upload; check the list; download one file per system; publish. The release `models-sherpa-2026-11-21` stays a pre-release.
10. **After:** Homebrew cask (`scripts/update-homebrew-cask.sh 0.3.7 /Users/trungtran/source/repos/homebrew-tap`), winget, APT (`release-day-0.3.2.md` section 7 and the 0.3.6 APT notes in `status.md`), plugin, website, post-release hash checks, the big disk cleanup.

## CI notes (from 0.3.6)

The speaker model is fetched from the mirror `models-sherpa-2026-11-21`; Windows CI drops the cached whisper.cpp CMake build; Chocolatey ffmpeg is pinned to 9.0.2. Never run `packaging/sherpa/fetch-models.sh` in the repo checkout.

## Known open items after 0.3.7

D4 (invented outro over room-only noise), the last-word check on a long real recording, Wayland placement of the bar, Long Shot's first-step loss on Windows, the Edge second-tab case (P-3, not reproduced), the winget pull requests, daemon `list_library` kind `guide` with status, about 16 untranslated Vietnamese strings, the sidebar text in `specs/features/09-library.md`.
