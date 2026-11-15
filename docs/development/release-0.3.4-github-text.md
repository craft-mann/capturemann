# GitHub release text for 0.3.4 (draft, not published)

Written from `specs/plans/release-0.3.4-plan.md`, the feature specs (`01-screenshot.md` CAP-6, `02-annotation.md` ANN-8 to ANN-11,
`19-click-guide.md`, `20-boards-and-bulk.md`, IMP-6.AC10 to AC13, REC-5.AC18, LIB-1.AC8 to AC11) and the 0.3.3 text. **Nothing here is
final until the regression on the final files exists, and a line about a feature that was cut must be deleted.** Every `<FILL: ...>` is a
placeholder for a fact that only exists on the release day (commit, checksums, report names, notarization, winget, Homebrew). Never write
"passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule). Steps: `release-day-0.3.4.md`. State of Nov 15, 2026 night: the feature list is final; the Fixed and Known limits lists are filled from the CHANGELOG and the QA reports; what is left are the numbers of the regression on the final build, the checksums, the notarization line and the winget, Homebrew and plugin lines.

- **Tag:** `v0.3.4` (never reuse a tag). **Title:** `CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards`.
- **Type:** draft, then **pre-release** (0.x). Windows 11 (x64), macOS 13+ (Apple Silicon and Intel). **Linux is not part of this draft:** no Linux 0.3.4 build or regression was run (maintainer decision pending, `release-day-0.3.4.md`, decision 1). If Linux ships, restore the Linux lines (Ubuntu 24.04+, x86_64 and aarch64, tarballs, `.deb`, minisign, APT with 0.3.0 to 0.3.4); if not, the text says Linux stays at 0.3.3.
- **Files to attach:** as 0.3.3 with the version changed (`capturemann-0.3.4-windows-x64-setup.exe` and `.zip`, `CaptureMann-v0.3.4-macos-<arch>.dmg`,
  `.sha256`, `SHA256SUMS.txt`; Linux files only if Linux ships: `capturemann-0.3.4-linux-*.tar.gz`, `capturemann_0.3.4_*.deb`, `.minisig`, and APT with the 0.3.0 to 0.3.4 `.deb` files). Check names with `scripts/check-release-assets.sh 0.3.4 <folder>`.
- **Headline numbers:** 89 MCP tools (`crates/cm-mcp/tests/stdio.rs` asserts it); Windows files are unsigned; macOS DMGs are signed and notarized by the maintainer.

---

## Release body

# CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards

0.3.4 has files for **Windows 11 and macOS** (<FILL: Linux sentence: "Linux stays at 0.3.3" or the Linux line if it ships>). It adds scrolling capture, a click-by-click guide maker, text editing inside a screenshot and
boards, and fixes the recording bar drag on Windows.

## New

- **Long Shot (scrolling capture).** Capture a whole page or window as one picture. **Auto** scrolls for you (it needs the Accessibility
  permission on macOS); **Hand** lets you scroll at your own pace. Vertical or horizontal, with a bar in the picker, a small HUD, a live side
  preview, a size limit and **Esc** to stop and keep what was captured. Fixed headers, footers and toolbars are kept once.
  Start it from the launcher, the menu or a shortcut you set in Settings (no default).
- **Click Guide.** Click through a task and CaptureMann writes the steps: each click is a step with a picture, a ring and a numbered badge.
  **It records clicks only, never keystrokes.** Start on a display or on **a chosen window**. Edit the guide (reorder by dragging, retake a
  step, edit the picture, add note steps) and export **Markdown, HTML or PDF**, or copy it as rich text. Guides are a new kind in the Library,
  with a filter chip. On macOS it needs the Input Monitoring permission; agents get five new tools. **Click Guide is new in 0.3.4 and
  will keep improving: it does not mark or blur text (step titles come from the text near the click), so check a guide before you share it.**
- **Retype.** Fix a typo, a name or a number inside a screenshot: click a recognized line and type. The box is filled with the measured
  background and the text is drawn in a matching size and colour; a longer text widens the box. **It is not a redaction**: use Blur or
  Pixelate to hide information.
- **Move area** and **Blueprint.** Move or remove part of a screenshot (the hole is filled with the background), or turn text into
  placeholder bars. Both are edits that sit under your other annotations.
- **Zoom Lens.** A magnifier loupe for a detail, round or rectangular, with a connector.
- **Boards.** Select 2 to 12 screenshots and make a before/after pair, a strip, a grid or a free layout.
- **Export selected as pictures.** Choose size, JPEG or WebP, a file name pattern, a watermark and Blueprint, for many screenshots at once.
- **The Published page is gone.** Recordings has the Published and Needs attention filters and a Published card on the details page.
- **Text colour from the picture.** The Text tool starts dark on a light screenshot and light on a dark one (an outline on a busy picture); `annotate` does the same when no colour is given.
- **Library selection.** Hover a card and tick it to select; the bar offers Make a board, Export selected and Delete. There is no
  Select button. Deleting a capture whose upload waits for a Google sign-in is no longer blocked.
- **Canvas text selection.** Select and copy recognized text straight on the picture.
- **CMYK JPEG import,** and **Open with** for the import types.
- **Agents & CLI** icon in the sidebar shows whether MCP is ready (the MCP card is gone).
- **Vietnamese** for every new string.

## Fixed

- The recording bar and the webcam bubble can be **dragged on Windows** (they could not be moved before).
- Copy as rich text reached the clipboard only in some builds; filter chips now follow the page (Audio no longer lists Boards).
- Default titles use your local time instead of UTC.
- Retype of Vietnamese text no longer cuts stacked marks and tails with a tight box.
- Zoom Lens edge colour fringe; a long library path now fails fast with a clear message instead of hanging; the speaker-model script works on
  Windows PowerShell 5.1; `library list` prints `-` for a guide with no size.
- Deleting a capture whose Google Drive upload waits for a sign-in is no longer blocked.
- Click Guide stops at 500 steps with a toast and no longer queues pictures without a limit; Long Shot keeps fixed side bars once and ends with a
  clearer reason when the window moves or the size limit is reached.
- Vietnamese: the agent confirmation for a guide, and one name for Click Guide everywhere.
  <FILL: check this list against `git log v0.3.3..<release commit>` once the commit is chosen>

## Known limits

- **Long Shot.** A page that redraws while it scrolls, or has fixed areas the picker cannot tell from content, can end early with "the next part did not
  line up"; the picture so far is kept. A covered window, a window that moved and a page that stopped moving can still show the same wording. **Hand scroll**
  works only from the on-screen bar: agents and the CLI cannot start it. On Windows, Windows Terminal passed in a test; a classic console window (conhost) ended with "nothing scrolled" before a fix that was not run again, and a program that redraws the screen (a full-screen editor) is not expected to work. **Esc** is a global stop key and cannot be changed (a terminal program that uses Esc never sees it). Quitting CaptureMann during a Long Shot or a guide discards it.
  A failed run with one frame is still saved to the library.
- **Click Guide.** **Step titles come from the text near the click and the app does not mark or blur anything, so a guide can contain private text: look at every
  step before you share or export it.** A guide started by an agent or the CLI shows only a small toast, no bar and no Stop button (stop it with `capturemann guide stop`);
  it needs the app running and its confirmation. A fast burst of clicks at one point becomes one "Double-click" step. Markdown export writes into the folder you choose
  and **overwrites** a file or folder of the same name without asking. A click on a window lying over the guided window can become a step.
- **Boards** look simpler than the design: BEFORE and AFTER labels are flat bands, not stamps, and the default margins, rounding and shadow are plainer than the design pack.
- **Retype, Move area and Blueprint are not redactions**: use Blur or Pixelate to hide information.
- Windows files are not code-signed (SmartScreen will warn). The speaker engine is not in the Windows files built on the lab laptop (`<FILL: say if the shipped files differ>`). <FILL: Windows licence list: "the list in the package is the macOS one" or "fixed", `release-day-0.3.4.md` decision 3>
- Linux is not part of this release (`<FILL: or its line>`). Intel Mac hardware has not been tried.

## What was checked

Reports are in `verification/` and all are **PARTIAL**; nothing was looked at on a screen by a person unless a report says so.

| System | Result | Report |
|---|---|---|
| macOS (Apple Silicon, dev bundle of commit `26a05100`, Nov 14) | full 57 passed, 0 failed, 23 skipped of 80; standard 49/0, 15 skipped of 64; hardware (screen only) 57/0, 31 skipped of 88. **Older than the final code**; <FILL: numbers of the run on the final candidate, or say these are the newest> | `L1-macos-0.3.4-dev-bundle-26a05100-regression-{standard,full,hardware}-2026-11-15-claude.md` |
| Windows 11 (installed build of commit `84bc00cd`, unsigned) | full 66 passed, 0 failed, 14 skipped of 80; hardware 68/0, 20 skipped of 88. **Older than the final code**; <FILL: numbers of the run on the final candidate> | `L1-windows-0.3.4-84bc00cd-final-regression-{full,hardware}-2026-11-15-claude.md` (earlier: `L1-windows-0.3.4-ac730b17-regression-*`, `L1-windows-0.3.4-main-a1bf57dc-regression-*`, `...-rc1-regression-hardware-*`) |
| Linux | not run on 0.3.4 (`<FILL: or the numbers if Linux ships>`) | the last Linux reports are `L1-linux-0.3.3-*` |
| Feature checks, macOS (six passes, Nov 14 to 15) | per-feature PARTIAL reports; Long Shot, Click Guide, boards, bulk export, editor tools, library, Published recordings, Vietnamese | `QA-0.3.4-summary-macos-pass{2..6}-*`, `QA-0.3.4-click-guide-macos-*`, `QA-0.3.4-long-shot-macos-*`, `QA-0.3.4-boards-bulk-*` |
| Feature checks, Windows (passes 1 to 6) | per-feature PARTIAL reports on a ThinkPad T490 at 150 % scaling | `QA-0.3.4-summary-windows-pass{3..6}-*`, `QA-0.3.4-cap-6-long-shot-windows-*`, `QA-0.3.4-gde-click-guide-windows-*`, `QA-0.3.4-brd-1-boards-windows-*`, `QA-0.3.4-blk-1-bulk-export-windows-*`, `QA-0.3.4-ann-{8,9,10,11}-*-windows-*`, `QA-0.3.4-imp-6-cmyk-jpeg-windows-*`, `QA-0.3.4-b1-*`, `QA-0.3.4-b2-b3-*`, `QA-0.3.4-final-build-windows-*`, `QA-0.3.4-windows-final-build-*` |
| Earlier single checks | Long Shot, Click Guide, fill quality, recording bar drag | `L1-macos-0.3.4-long-shot-*`, `L1-windows-0.3.4-long-shot-scroll-*`, `GDE-macos-0.3.4-quality-fixes-*`, `GDE-windows-0.3.3-guide-cases-*`, `S2-fill-quality-0.3.4-*`, `HUD-1-windows-{before-fix,checks,after-fix}-*` |

The Windows install of the **latest** build (commit `7fb80d70`) was checked only for installing, `capturemann --version` 0.3.4 and one start of the app (`QA-0.3.4-windows-final-build-2026-11-15-claude.md`);
no regression ran on it. Fixes made after the reports above (Long Shot fixed bars and early stops, the Click Guide step limit and queue, the Retype and Blueprint line boxes) were tested with fakes and, for the Long Shot sidebar case on macOS, measured in one run; most of them were not rerun on a screen.
The floating windows (Long Shot bar and HUD, Click Guide bar, picker, toasts) cannot be captured by the test tools, so their sizes, text and behaviour were read from the accessibility tree and nobody looked at them in the reports.

**Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed-DPI monitors, NVIDIA/AMD encoders, Linux (unless <FILL>), the Windows 5-minute soak
(it needs a microphone), a Drive account for the paused-upload delete (B1: checked with a fixture only), Hand scroll through the real on-screen bar, the webcam bubble drag on Windows (no camera), the
countdown, retake and Tall-picture chip of Click Guide.

<FILL: checksums, notarization, winget and Homebrew lines, as in the 0.3.3 text>
