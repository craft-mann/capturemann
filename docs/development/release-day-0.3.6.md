# Release day, 0.3.6 (Windows, macOS, Linux): steps

Written on Nov 21, 2026 from `release-day-0.3.5.md` (the pattern; read its sections 4 and 5 and "Rollback" for the generic steps and lessons). "[M]" = only the maintainer does this (tag, sign, notarize, publish, accounts, decisions). "[A]" = an agent can do it from this file. Text to publish: `release-0.3.6-github-text.md` (its `<FILL: ...>` items). Plan: `specs/plans/release-0.3.6-plan.md`. Version is 0.3.6 in `Cargo.toml`, `Cargo.lock` and the plugin; MCP tools 90.

## What is already done (Nov 21, 2026)

- **Pushed:** `public/main` at `96b85253` (and docs commits above it). CI green on `96b85253`, the Security scan green, the Windows package workflow green (run 44898477660, files are workflow artifacts).
- **Full gate:** run once on `77d50772` (fmt, `cargo deny`, workspace clippy, tests 5555 plus the i18n fix, Trivy vulnerability and secret scan). Later commits: Quick Capture fixes (Ctrl+1, name-screen Esc, drag on Windows, value menu arrows) and CI/test fixes; clippy and the quick tests ran on each, the whole workspace was run by CI on `96b85253`.
- **macOS DMGs [M, done]:** `target/bundle/CaptureMann-0.3.6-arm64.dmg` (`7cdc5fde...`) and `target/bundle-x86_64/CaptureMann-0.3.6-x86_64.dmg` (`c52e6256...`), notarized, stapled, `verify-release.sh` passed for both; `cm-regress` on the arm64 app: `L1-macos-0.3.6-dmg-96b85253-regression-*`.
- **QA passes:** macOS `77d50772`, Windows `8a746f6b`, Linux `57f50732` and the recheck `103290cd` (reports named in the text).
- **CI lessons of Nov 21:** the speaker model is fetched from the mirror release `models-sherpa-2026-11-21` (k2-fsa replaced the upstream file); Windows CI drops the cached whisper.cpp CMake build; Chocolatey ffmpeg is pinned to 9.0.2. Never run `packaging/sherpa/fetch-models.sh` in the repo checkout.

## Done by the morning of Nov 22, 2026 (everything before the tag)

- **Windows files:** built by the Windows package workflow run 44914326579 on `1778431b` (it makes the bar take the foreground window on Windows, finding F-1 of the `96b85253` pass); installed and checked on the ThinkPad: standard 57 / 0 and full 68 / 0, F-1 not reproduced in 28 tries (`QA-0.3.6-windows-ci-package-1778431b-*`). The `96b85253` package had hardware 68 / 0 too.
- **Linux files:** x86_64 and aarch64 tarballs and `.deb` files of `96b85253`, checked, regressed and **signed by the maintainer** (all eight `.minisig` verify with `packaging/linux/minisign.pub`).
- **macOS DMGs:** renamed and re-hashed copies of the notarized files (byte-identical, stapled).
- **Staging folder:** `/Users/trungtran/release-0.3.6/upload/`, 25 files (4 Linux packages with 16 sidecars, 2 DMGs and 2 Windows files with `.sha256` each, `SHA256SUMS.txt`); `scripts/check-release-assets.sh 0.3.6 /Users/trungtran/release-0.3.6/upload` says OK. The Windows `.sha256` files were rewritten from `hash *name` to `hash  name` (the checker wants two spaces; the hashes are the CI's).
- **Text:** `release-0.3.6-github-text.md` has no `<FILL>` left. **Tag on `1778431b`** (the latest code commit; docs commits above it); the macOS and Linux files are of `96b85253`, which differs only by that Windows-only change, and the text says so.
- **CI:** green on `1778431b` (CI run 44914294415 after a rerun of a cancelled macOS job; Security scan; Windows package).

## Still open before the tag

1. **[M] Review** the release text (`release-0.3.6-github-text.md`), the staging folder and this file; decide full release or pre-release.
2. **[M]** `CHANGELOG.md` date, `CLAUDE.md` wording ("released"), `docs/project/status.md`, plan header; push; CI green again (docs only).

## Tag and publish

1. **[M]** `CHANGELOG.md`: `## [0.3.6] - Unreleased` becomes `## [0.3.6] - <date>` and a new empty `## [Unreleased]` goes above; `CLAUDE.md` says "0.3.6 is released" with the final facts; `docs/project/status.md`; header of `specs/plans/release-0.3.6-plan.md` ("released <date>"). Commit and push; wait for CI and the Security scan. These docs commits sit above the tag if the tag stays on the commit the files were built from: say so in the text.
2. **[M]** `git tag -a v0.3.6 -m "CaptureMann 0.3.6"` on the docs commit above `1778431b` (done Nov 22) and `git push public v0.3.6` (never reuse a tag).
3. **[A]** `packaging/windows/release-files.sh` (checksums, winget manifest), then `scripts/check-release-assets.sh 0.3.6 <folder>`.
4. **[M]** `gh release create v0.3.6 -R craft-mann/capturemann --draft --title "CaptureMann 0.3.6: Quick Capture" --notes-file release-body.md` and upload the files; check the list; download one file per system; publish. The release `models-sherpa-2026-11-21` is a pre-release mirror and must stay a pre-release (never Latest).
5. **After:** Homebrew cask (`scripts/update-homebrew-cask.sh 0.3.6 /Users/trungtran/source/repos/homebrew-tap`), winget (`release-day-0.3.2.md`, section 7), APT, the plugin (`tests/manual/PLUGIN-1.md`), website download links and the feature page (`docs/website-features.md`), post-release hash checks, then the big disk cleanup (`target/` is about 300 GB; keep the two DMGs until the release is public).

## Known open items for 0.3.7

The plan is `specs/plans/release-0.3.7-plan.md`; this list is its source.

The FFmpeg-missing message names Homebrew on Linux (`crates/cm-core/src/message.rs` lines 138 and 164, `crates/cm-media/src/tools.rs` line 38; case `regress.ffmpeg_missing_wording` fails in a clean Linux root without a system FFmpeg; low impact, the packages bundle FFmpeg); D4 (invented outro over room-only noise), X11 drag and Wayland placement of the bar, Tab in the preset sheet on X11, the Windows `ai.transcribe` case limit (600 s, `--timeout-scale` does not change it), `desktop.hotkey_overlay` guard (match the hotkey holder, not the process name), Long Shot's first-step loss on Windows, P-3 not reproduced, F-4 (the bar opens 5 px high on Windows), headless Wayland checks of Long Shot and Click Guide.
