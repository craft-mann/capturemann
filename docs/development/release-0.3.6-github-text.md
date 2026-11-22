# GitHub release text for 0.3.6 (draft, not published)

Written on Nov 21, 2026 from `specs/plans/release-0.3.6-plan.md`, `specs/features/21-quick-capture.md`, `CHANGELOG.md`, the QA and regression reports `verification/*0.3.6*`, and the 0.3.5 text. **Nothing here is final until the regression on the final Windows and Linux files exists.** Every `<FILL: ...>` is a fact that only exists on the release day. Never write "passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule); things the maintainer saw are written "reported by the maintainer". Steps: `release-day-0.3.6.md`.

- **Tag:** `v0.3.6` on the commit that holds the release docs, directly above the code commit `1778431b` (only documents, reports and the changelog differ from it; never reuse a tag). **Which commit built what:** the Windows files are of `1778431b` (it makes the Quick Capture bar take the foreground window on Windows: with the desktop in front the bar opened without the keyboard on `96b85253`); the macOS DMGs and the four Linux files are of `96b85253`, which differs from `1778431b` only by that Windows-only code path. Say so in the text. **Title:** `CaptureMann 0.3.6: Quick Capture`.
- **Type:** draft first; full release or pre-release is the maintainer's call.
- **Files to attach:** `capturemann-0.3.6-windows-x64-setup.exe` and `.zip`, `CaptureMann-v0.3.6-macos-<arch>.dmg` (arm64 and x86_64; the maintainer's build names are `CaptureMann-0.3.6-<arch>.dmg`, renamed), the `.sha256` files and `SHA256SUMS.txt`, and the four Linux files with their `.minisig`. Check with `scripts/check-release-assets.sh 0.3.6 <folder>`.
- **Headline numbers:** 90 MCP tools (`list_presets` is new); Windows files unsigned; macOS DMGs signed, notarized and stapled by the maintainer (arm64 `7cdc5fde...`, Intel `c52e6256...` as built on Nov 21; the renamed files must be hashed again).

---

## Release body

# CaptureMann 0.3.6: Quick Capture

0.3.6 adds **Quick Capture**: one slim bar for screenshots, video and audio, with presets. It also stops the transcript from inventing text over some quiet audio, and fixes the problems found in the 0.3.5 checks. Files for **Windows 11, macOS 13+ (Apple Silicon and Intel) and Linux** (Ubuntu 24.04 and newer, x86_64 and aarch64).

## New

- **Quick Capture (Preview).** Press **⌥⇧C** (Ctrl+Alt+Shift+C on Windows and Linux), choose Quick Capture from the tray or menu-bar icon, the sidebar's Capture… button or the Launcher. One row of icons: the kind (1 Screenshot, 2 Video, 3 Audio), the target (R Region, W Window, D Display, L Long Shot), the options that fit the kind (Editor, Copy, Auto-redact and Delay for a screenshot; Mic, System audio, Webcam and Countdown for video; Mic, System audio, Transcribe and Markers for audio) and one Capture or Record button. Every control has its key printed under its icon and a tooltip. The bar is opaque, you can drag it by its background, collapse it to a compact form (key **O**, remembered), and it stays open until you close it. Its options are kept per kind and apart from Settings, so the dock, the hotkeys and the tray work as before. It is marked Preview.
- **Presets.** Save a setup with a name and an optional shortcut: Quick Capture's picker (P, or Ctrl+1 to Ctrl+4) and Settings edit them. Four starters ship and can be edited or deleted: Bug report, Docs shot, Demo video and Meeting. Agents and scripts run a preset by name: the new `list_presets` tool (**90 tools**), a `preset` argument on `capture_screenshot` and `start_recording`, `capturemann presets list` and `--preset`.
- **After-capture bar.** After a screenshot from Quick Capture with Editor and Copy off, a small bar offers Edit, Copy, Save, Copy text, extend the area as a Long Shot, or record exactly that area, each one key away.

## Changed

- **Transcripts no longer invent text over some quiet audio.** A doubtful line over audio that is not speech-like is written as an `[unclear]` mark and its words are left out; audio under 30 s is judged when speech and room are clearly apart; every word has an end after its start. Re-generate after updating (a transcript already made is unchanged). This helps; it is not a guarantee (see Known limits).

## Fixed

- A text file named `.png` is refused as an unreadable image; `annotate` refuses a shape far outside the picture and no longer answers `internal` on a very large image; the ambiguous-window error lists only the matching windows; Long Shot says why it stopped and how far it moved the page and leaves the page where it was.
- Windows: the export dialog no longer shows the `\\?\` prefix, the left rail shows that it scrolls, and the Boards intro card closes with its sheet. Vietnamese Click Guide tiles and Preview pills fit.
- Linux: Quick Capture, the after-capture bar, the preset sheet and the Launcher take the keyboard on X11; toasts no longer stay held by a hover that ended with its window.

## Known limits

- **Transcripts.** Over audio that holds only room noise, a made-up sentence can still pass the check (a 9-second Vietnamese test clip with only room noise still gave an invented sign-off on Windows, see `QA-0.3.6-windows-final-8a746f6b-*`). Look at transcripts of very quiet recordings.
- **Quick Capture on Linux.** On X11 the bar cannot be dragged (the window manager does not move such windows); on a Wayland session the bar opens at the top-left because a program cannot place itself there. Tab in the preset sheet does not move to the next field on X11. Linux has no way to keep the bar out of a screenshot or recording another program starts; the app closes it before its own captures.
- **Long Shot on Windows** can lose its first scroll step and then recovers; a maximised Edge window with a second tab ended early once in the earlier checks and has not been reproduced.
- **Preview features:** Quick Capture, Long Shot, Click Guide and Boards.
- Windows files are not code-signed (SmartScreen will warn). They are the GitHub "Windows package" workflow's files and include the speaker engine.

## What was checked

**Which commit built what:** the Windows files are of `1778431b`, the macOS DMGs and the Linux files are of `96b85253`; the two differ only in a Windows-only change (the bar takes the foreground window, so it has the keyboard when the desktop is in front). The tag `v0.3.6` sits on the commit above `1778431b` that holds only the changelog, documents and reports.

Reports are in `verification/` and all are **PARTIAL**. The overlay windows (bars, HUD, picker, toasts, dialogs) cannot be captured by the test tools: they were driven and read through input and the accessibility tree, and the maintainer looked at the macOS bar by hand (reported by the maintainer, no stored report).

| System | Result | Report |
|---|---|---|
| macOS arm64, the notarized DMG's app of commit `96b85253` (screen only) | standard 52 passed, 0 failed, 12 skipped of 64; full 58 / 1 / 21 of 80 (the failure was the checking agent's own mount of the same DMG, "Resource busy"; the case passed alone); hardware 59 / 0 / 29 of 88 | `L1-macos-0.3.6-dmg-96b85253-regression-*`, `QA-0.3.6-macos-dmg-96b85253-2026-11-21-claude.md` |
| macOS Intel DMG | signature, notarization, version, architecture and contents checked on Apple Silicon only; not run | same QA report |
| macOS QA pass of the bar (dev bundle of `77d50772`) | standard 51 / 0 / 13, full 56 / 1 / 23 (the one was the maintainer's own overlay running), hardware 57 / 0 / 31 | `QA-0.3.6-macos-final-77d50772-*`, `L1-macos-0.3.6-dev-77d50772-regression-*` |
| Windows 11 (lab build of `8a746f6b`, no speaker engine) | standard 57 / 0 / 7; full 65 / 1 / 14 and hardware 65 / 1 / 22 (`ai.transcribe` ran past its fixed 600 s limit on a busy laptop; it passed earlier) | `QA-0.3.6-windows-final-8a746f6b-*`, `L1-windows-0.3.6-lab-8a746f6b-regression-*` |
| Windows 11 (the GitHub package of commit `96b85253`, installed; the files of the release are of `1778431b`, see below) | standard 57 passed, 0 failed, 7 skipped of 64; full 68 / 0 / 12 of 80; hardware 68 / 0 / 20 of 88 (screen only) | `L1-windows-0.3.6-ci-96b85253-installed-regression-{standard,full,hardware}-2026-11-21-claude.md`, `QA-0.3.6-windows-ci-package-96b85253-2026-11-21-claude.md` |
| Windows 11 (the final files of commit `1778431b`, installed) | standard 57 / 0 / 7 of 64; full 68 / 0 / 12 of 80; the bar opened with the desktop, Edge, the Studio window in front or reopened at once took the keyboard in 28 of 28 tries (it did not in about 3 of 10 desktop tries on `96b85253`) | `L1-windows-0.3.6-ci-1778431b-installed-regression-{standard,full}-2026-11-21-claude.md`, `QA-0.3.6-windows-ci-package-1778431b-2026-11-22-claude.md` |
| Linux x86_64 (ThinkPad, Ubuntu 24.04; `.deb` of `57f50732`, X11 and Wayland) | X11 standard 57 / 0 / 7, full 64 / 0 / 16, hardware 64 / 0 / 24; Wayland standard 55 / 0 / 9, full 61 / 0 / 19; `.deb` install 1 / 0 | `QA-0.3.6-linux-final-57f50732-*`, `L1-linux-0.3.6-regression-*-lab-57f50732-deb-*` |
| Linux, two fixes rechecked (`.deb` of `103290cd`) | Ctrl+1 and Esc on the name screen work on X11 and Wayland; standard 57 / 0 / 7 | `QA-0.3.6-linux-recheck-103290cd-*` |
| Linux x86_64 final files (commit `96b85253`, X11) | `check-package.sh` passed on the tarball and the `.deb`; standard 57 / 0 / 7 of 64; full 64 / 0 / 16 of 80; `.deb` install 1 / 0; the bar opened and took Ctrl+1, O and Esc | `L1-linux-0.3.6-regression-{standard,full,deb-install}-x11-final-96b85253-deb-2026-11-21-claude.md`, `QA-0.3.6-linux-final-files-96b85253-2026-11-21-claude.md` |
| Linux aarch64 final files (commit `96b85253`, headless VM) | `check-package.sh` passed; standard 42 / 0 / 22 of 64; full 44 / 0 / 36 of 80; `.deb` install 1 / 0; every desktop-session case skipped | `L1-linux-aarch64-0.3.6-96b85253-regression-{standard,full,deb-install}-2026-11-21-claude.md`, `QA-0.3.6-linux-aarch64-96b85253-2026-11-21-claude.md` |

CI is green on `1778431b` (CI run 44914294415 after one rerun of a cancelled macOS job, Security scan 44914294061) and so is the Windows package workflow (run 44914326579, the Windows files). The macOS and Linux files were built from `96b85253` (CI run 44898477740 green); `1778431b` differs from it only in the Windows-only change below.

**Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed-DPI monitors, NVIDIA and AMD encoders, KDE, the two-row layout on a narrow real display (Windows was checked at 800x600), Quick Capture drag and chevron clicks on macOS by an agent (clicks cannot be injected there), the 5-minute soak, Window, Display and Long Shot targets from the bar on Linux, Retype on Windows.
