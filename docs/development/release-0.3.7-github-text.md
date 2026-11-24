# GitHub release text for 0.3.7 (draft, not published)

Written on Nov 24, 2026 (night before the planned release) from `specs/plans/release-0.3.7-plan.md`, `CHANGELOG.md` and the reports `verification/*0.3.7*`. **Nothing here is final until the regression on the final files exists.** Every `<FILL: ...>` is a fact that only exists on the release day. Never write "passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule); what the maintainer saw is written "reported by the maintainer". Steps: `release-day-0.3.7.md`.

- **Tag:** `v0.3.7` on the commit that holds the release docs, directly above the code commit `<FILL: code commit>` (never reuse a tag). **Title:** `CaptureMann 0.3.7: bug fixes and a tidier sidebar`.
- **Type:** draft first; full release or pre-release is the maintainer's call.
- **Files to attach:** as 0.3.6: `capturemann-0.3.7-windows-x64-setup.exe` and `.zip`, `CaptureMann-v0.3.7-macos-<arch>.dmg` (arm64 and x86_64), the `.sha256` files and `SHA256SUMS.txt`, the four Linux files with their `.minisig`. Check with `scripts/check-release-assets.sh 0.3.7 <folder>`.
- **Headline numbers:** 90 MCP tools (unchanged); Windows files unsigned; macOS DMGs signed, notarized and stapled by the maintainer.

---

## Release body

# CaptureMann 0.3.7: bug fixes and a tidier sidebar

0.3.7 fixes the problems found in 0.3.6 (mostly in the Quick Capture bar) and tidies the sidebar. It adds no new capability. Files for **Windows 11, macOS 13+ (Apple Silicon and Intel) and Linux** (Ubuntu 24.04 and newer, x86_64 and aarch64).

## Changed

- **A tidier sidebar.** Two buttons, **Quick Capture…** (⌥⇧C) and **Record…** (⌥⇧R; red only while it records), then one row of six modes with the key printed under each icon: Region ⌥⇧1, Window ⌥⇧2, Display ⌥⇧3, Long Shot ⌥⇧4, Click Guide ⌥⇧5, Audio ⌥⇧V (Ctrl+Alt+Shift on Windows and Linux). **Long Shot and Click Guide get default shortcuts** (⌥⇧4 and ⌥⇧5); a field you cleared stays cleared. The library lists All captures with its count and then Screenshots, Recordings, Audio and Click Guides in a fixed order; its filter bar holds statuses, **Needs attention** first and amber, then Boards as a view; Agents & CLI says Ready, Off or Busy; under 960 pt the sidebar is an icon rail with tooltips.
- **The Quick Capture bar stays until you close it.** After a screenshot, a recording or a Long Shot started from it, the bar steps aside and comes back at the same place on the same display (0.2 s after a screenshot, 1.5 s after a recording or Long Shot). It first opens at the middle of the top of the display. **Quick Capture…** is also the first item of the macOS app menu. On Video and Audio the Record button fills the room the kind leaves over.
- The message for a missing FFmpeg names the right fix for each system (Homebrew on macOS, the package manager on Linux, nothing to install on Windows).

## Fixed

- Quick Capture: the empty strip under the bar lets clicks through (macOS, Windows, Linux X11); the bar can be dragged on X11; Tab moves between the fields of the preset sheet; the bar no longer shifts by 5 px on Windows or jumps when the picker opens; no error lines in the log when it closes; the Boards intro card closes with its sheet.
- A long error message in a toast is no longer cut off at the end.
- Long Shot on Windows no longer drops rows where the page has repeating bars (seen in Edge).
- The ambiguous-window error no longer lists the titles of other applications' windows.
- The Windows package's licence list is the Windows one.

## Known limits

- **Transcripts.** Over audio that holds only room noise, a made-up sentence can still pass the check; the measured rule planned for this is not in 0.3.7. Look at transcripts of very quiet recordings.
- **Quick Capture on Linux.** On a Wayland session the bar opens at the top-left because a program cannot place itself there; Linux has no way to keep the bar out of a capture another program starts.
- **Shortcuts.** ⌥⇧4 and ⌥⇧5 may already be taken by another program or a system shortcut on your Mac; change them in Settings if so. <FILL: result of the macOS collision check, or "not checked">
- **Long Shot on Windows** can lose its first scroll step and then recovers.
- **Preview features:** Quick Capture, Long Shot, Click Guide and Boards.
- Windows files are not code-signed (SmartScreen will warn). They are the GitHub "Windows package" workflow's files and include the speaker engine.

## What was checked

Reports are in `verification/` and all are **PARTIAL**. The overlay windows (bars, HUD, picker, toasts, dialogs) cannot be captured by the test tools: they were read through window rectangles, input and the accessibility tree.

| System | Result | Report |
|---|---|---|
| Windows 11 (lab build of `f25ab382`, no speaker engine, screen only) | standard 57 passed, 0 failed, 7 skipped of 64; full 66 / 0 / 14 of 80; hardware 66 / 0 / 22 of 88 | `L1-windows-0.3.7-lab-f25ab382-regression-*` |
| macOS arm64 (dev bundle, screen locked overnight so desktop cases skipped; to be repeated on the DMG) | standard 43 / 0 / 21 of 64; full 44 / 0 / 36 of 80 | `L1-macos-0.3.7-regression-*` |
| Linux x86_64 (ThinkPad, Ubuntu 24.04) | not run on the night of Nov 24 (the laptop did not come back from the switch to Ubuntu) | `L1-linux-0.3.7-*` |
| <FILL: the final files per system: Windows CI package, macOS DMGs, Linux tarballs and .deb> | <FILL> | <FILL> |
| Bar checks (Linux X11, Windows) | strip click-through, drag, return after a capture, default place | `QC-0.3.7-bugfix-checks-2026-11-22-claude.md` |

**Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed-DPI monitors, NVIDIA and AMD encoders, KDE, Wayland checks of the new bar behaviour, the sidebar's narrow rail on a real narrow window, bar returns after Region, Video, Audio and Long Shot on every system.
