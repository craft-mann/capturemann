# 0.3.4 morning report (Nov 16, 2026 night, for Nov 17)

One page. Facts come from the reports in `verification/` and `git log` at the time of writing; where a fact is unknown it says so. Nothing is called
"verified"; every report is PARTIAL. Steps for the release: `release-day-0.3.4.md`. Text: `release-0.3.4-github-text.md`.

## What happened tonight

- Merged: the boards agent (`fea1b8a4`: the board sheet preview is drawn by the export renderer, test `board_preview_034`; Retype shows the "Not a redaction" note again; boards limits written into the text; no Settings toggle for guide titles) and the lab agent's reports on `596c7956`. **The final source commit is `19f64bb3`**; commits after it are verification and documents only.
- Decisions applied: Linux ships in 0.3.4; Long Shot and Click Guide are Preview (label only); no "New in" labels; Windows files unsigned; Long Shot trusted joins on weak evidence accepted.
- Targeted checks on the merge: `cargo fmt`, clippy 1.100.0 (cm-render, cm-desktop), cm-render tests, `layout_guard`, `cm-i18n check`: all clean.
- **macOS regression on the final commit** (dev bundle of `19f64bb3`, screen unlocked, `--allow screen`): standard 52/0 (12 skipped), full 58/0 (22 skipped), hardware 58/0 (30 skipped). Reports `L1-macos-0.3.4-regression-{standard,full,hardware}-dev-bundle-19f64bb3-screen-2026-11-16-claude.md`.
- **Windows and Linux on the final commit did not run.** The ThinkPad was rebooted at 21:45 with nobody logged in (Windows session tests need a console login; auto-logon was not enabled) and it did not come back after `lab.sh switch ubuntu` (no answer on 192.168.0.102 or 192.168.100.132). The Windows files of `19f64bb3` were built (`L1-windows-0.3.4-19f64bb3-candidate-build-only-*`; setup `aff18661…0112b`, zip `93782bed…03fb`, in `/Users/trungtran/cm-release-0.3.4/windows/`) but not installed or tested. The newest Windows and Linux regression reports name `596c7956`; the source difference is 4 files (board preview, Retype note UI).

## Regression table

| System | Commit | Result | Reports |
|---|---|---|---|
| macOS arm64 dev bundle | `19f64bb3` (final) | standard 52/0, full 58/0, hardware 58/0 | `L1-macos-0.3.4-regression-*-dev-bundle-19f64bb3-screen-*` |
| Windows 11 candidate files (no speaker engine) | `596c7956` | full 66/0 (14 skipped), hardware 68/0 (20 skipped) | `L1-windows-0.3.4-596c7956-final-regression-{full,hardware}-*` |
| Linux x86_64 (Ubuntu 24.04.5, X11) | `596c7956` | standard 57/0, full 65/0, deb install 1/0 | `L1-linux-0.3.4-596c7956-regression-{standard,full,deb-install}-*` |

## What you must do

1. Look at the ThinkPad (it needs power or a look at its screen; log in to Windows once). Then rerun on `19f64bb3` (or the later release commit): Windows install of the files above, `regress.sh windows --tier full` and `--tier hardware --allow screen,sound,camera --installer ... --commit 19f64bb3`; Linux `lab.sh switch ubuntu`, `release-build.sh`, `check-package.sh`, standard and full. The draft release does not carry Windows or Linux files whose regression is missing (see its notes).
2. macOS DMGs: sign, notarize, staple, rename (`release-day-0.3.4.md`, section 3). Linux `.minisig`, APT, Homebrew, winget, plugin line, tag, publish.
3. Look at the boards sheet in the live app: pair, strip, grid against an export, and **dragging in Free layout** (the preview re-renders up to 1800 px per move; if it is janky, cap the preview edge while dragging). Nobody looked at it. Also look at the Retype and Move area note and the Preview tags.
4. Decisions waiting: D1 to D31 (`ux-proposals-0.3.4-for-design.md`); the Windows licence list (accepted as the macOS list for now, stated in the text); the audit P1 items not built (listed in Known limits); Linux aarch64 (not run).

## Defects still open

As in the Known limits of the release text: Long Shot on Windows (YouTube 2 frames, carchargeos stop, Edge sync dialog), repeating fixed panels per slice on very long pictures, a guide started by an agent has no bar, Markdown export overwrites without asking, a click on a window over the guided one becomes a step, quitting discards a Long Shot or guide, boards plainer than the design. The Click Guide burst and early-stop fixes were measured with fakes and on macOS only.

## Bundles

- macOS dev bundle of `19f64bb3`: `target/bundle/CaptureMann-dev.app` and `~/Applications/CaptureMann-dev.app` (signed with the local dev identity, `codesign --verify` ok). No signed or notarized DMG exists.
- Windows (unsigned, untested): see above. Linux: none for the final commit.
