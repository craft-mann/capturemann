# 0.3.4 morning report (Nov 16, 2026 night, for Nov 17)

One page. Facts come from the reports in `verification/` and `git log` at the time of writing; where a fact is unknown it says so. Nothing is called
"verified"; every report is PARTIAL. Steps for the release: `release-day-0.3.4.md`. Text: `release-0.3.4-github-text.md`.

## What happened tonight

- Merged: the boards agent (`fea1b8a4`: the board sheet preview is drawn by the export renderer, test `board_preview_034`; Retype shows the "Not a redaction" note again; boards limits written into the text; no Settings toggle for guide titles) and the lab agent's reports on `596c7956`. **The final source commit is `19f64bb3`**; commits after it are verification and documents only.
- Decisions applied: Linux ships in 0.3.4; Long Shot and Click Guide are Preview (label only); no "New in" labels; Windows files unsigned; Long Shot trusted joins on weak evidence accepted.
- Targeted checks on the merge: `cargo fmt`, clippy 1.100.0 (cm-render, cm-desktop), cm-render tests, `layout_guard`, `cm-i18n check`: all clean.
- **macOS regression on the final commit** (dev bundle of `19f64bb3`, screen unlocked, `--allow screen`): standard 52/0 (12 skipped), full 58/0 (22 skipped), hardware 58/0 (30 skipped). Reports `L1-macos-0.3.4-regression-{standard,full,hardware}-dev-bundle-19f64bb3-screen-2026-11-16-claude.md`.
- **Windows and Linux on the final commit did not run.** The ThinkPad was rebooted at 21:45 with nobody logged in (Windows session tests need a console login; auto-logon was not enabled) and it did not come back after `lab.sh switch ubuntu` (no answer on 192.168.0.102 or 192.168.100.132). The Windows files of `19f64bb3` were built (`L1-windows-0.3.4-19f64bb3-candidate-build-only-*`; setup `aff18661…0112b`, zip `93782bed…03fb`, in `/Users/trungtran/cm-release-0.3.4/windows/`) but not installed or tested. The newest Windows and Linux regression reports name `596c7956`; the source difference is 4 files (board preview, Retype note UI).

## Gate and CI (what really happened)

- The local gate: fmt, `cargo deny` (advisories, bans, licences, sources ok), clippy 1.100.0 `--no-default-features` (clean), doc tests (ok), Trivy (CRITICAL and secrets, exit 0) ran clean. **The local workspace test stage did not finish on this Mac**: every freshly built test binary waited about 2.5 minutes at its first start (macOS security scanning, `syspolicyd` and XProtect at high CPU; a rustc also hung loading a proc-macro dylib until the old `target/debug/incremental` data, 26 GB, was bypassed). About 25 of 174 test binaries ran, all passing. CI is the test evidence.
- First CI run on `1c82debc` found real defects, fixed on `main`: Linux clippy (unused import `Direction` in `cm-capture/src/sys/long_scroll.rs`); `no_ui_literal_is_left_unwrapped` (the Preview feedback issue text had no `i18n-ignore`); **`cm-mcp.exe` overflowed the 1 MB main-thread stack on Windows in a debug build** (the server now runs on 8 MB worker stacks); Click Guide burst and queue tests that depend on runner speed (thresholds relaxed, the printed numbers are the measure); a Windows test of the busy-endpoint reason (a busy named pipe is "access denied", filed under the folder reason: the tooltip can say folder instead of "another daemon owns it"); a font-width test that measured wider on the Windows runner; a PDF speed test (2.3 s in debug on a shared runner, limit now 10 s in debug).

- **CI on `ed92ca93` (all code fixes in): all green**: CI run 43962022567 (check macOS, Ubuntu and Windows, bundle-macos, msrv, licenses, docs, plan) and the Security scan. Runs before it were red for the reasons above (43936839134, 43951149329, 43952533604). Commits after `ed92ca93` are documents and reports only.

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

## Release day (Nov 17, 2026)

Facts from the published release, the stored reports and the release text. All reports are PARTIAL.

- **Published by the maintainer** at 03:34 UTC on Nov 17, 2026: <https://github.com/craft-mann/capturemann/releases/tag/v0.3.4>, tag `v0.3.4` on `1a1586d0`, a full release (not a pre-release), 25 files. The tag was created by publishing (lightweight tag).
- **Files:** macOS arm64 and x86_64 DMGs, signed, notarized and stapled (arm64 SHA-256 `68571d0f…104c`, x86_64 `2234ac54…2db1`); Windows setup and zip, unsigned, from the GitHub "Windows package" workflow on `1a1586d0`, with the speaker engine (setup `88f0c432…9897df`); Linux x86_64 and aarch64 tarballs and `.deb` files, signed with minisign. Homebrew cask at 0.3.4 (tap commit `7789f42`, push checked through the API).
- **Final reports (all on `1a1586d0`, 0 failed):**

| System | Result | Reports |
|---|---|---|
| macOS arm64, app copied from the notarized DMG, screen unlocked | standard 52 (12 skipped), full 59 (21), hardware 58 (30) | `L1-macos-0.3.4-dmg-1a1586d0-screen-regression-*` |
| macOS, same, screen locked | standard 43, full 45, hardware 44 (desktop cases skipped) | `L1-macos-0.3.4-dmg-1a1586d0-regression-*` |
| Windows 11, installed CI setup | full 66 (14 skipped), hardware 68 (20) | `L1-windows-0.3.4-1a1586d0-final-regression-{full,hardware}-*` |
| Linux x86_64, ThinkPad, X11 | standard 57 (7), full 65 (15), `.deb` install 1 | `L1-linux-0.3.4-1a1586d0-regression-*` |
| Linux aarch64, headless VM | standard 42 (22), full 44 (36), `.deb` install 1 (run alone) | `L1-linux-aarch64-0.3.4-1a1586d0-regression-*` |

- **CI:** green on `1a1586d0`: CI run 43970813077 and Security scan 43970812991. The Intel DMG was checked for signature, notarization, version and architecture only.
- **What remains (maintainer):** winget (PRs 472327 and 477195), the APT repository update with the 0.3.0 to 0.3.4 `.deb` files, the plugin line (held by the directory until the organization is Verified), the audit P1 items and decisions D1 to D31, `release-0.3.5-plan.md`.
- **Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA/AMD encoders, Linux Wayland and the Linux hardware tier, aarch64 desktop-session cases, the Windows soak, a Drive account for B1, the Windows zip.
- **Known limit that shipped:** Long Shot on Windows (Edge, 1280x900) ended after 2 frames on YouTube and carchargeos.com; Wikipedia, docs.rs and the essay joined.
