# GitHub release text for 0.3.4 (draft, not published)

Written from `specs/plans/release-0.3.4-plan.md`, the feature specs (`01-screenshot.md` CAP-6, `02-annotation.md` ANN-8 to ANN-11,
`19-click-guide.md`, `20-boards-and-bulk.md`, IMP-6.AC10 to AC13, REC-5.AC18, LIB-1.AC8 to AC11) and the 0.3.3 text. **Nothing here is
final until the regression on the final files exists, and a line about a feature that was cut must be deleted.** Every `<FILL: ...>` is a
placeholder for a fact that only exists on the release day (commit, checksums, report names, notarization, winget, Homebrew). Never write
"passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule). Steps: `release-day-0.3.4.md`. State of Nov 16, 2026 night: the feature list is final; Fixed and Known limits are filled; regression numbers are filled per system (the table says which commit each covers); the lines marked MAINTAINER at the end are the maintainer's.

- **Tag:** `v0.3.4` (never reuse a tag). **Title:** `CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards`.
- **Type:** draft, then **pre-release** (0.x). Windows 11 (x64), macOS 13+ (Apple Silicon and Intel), Linux (Ubuntu 24.04+, x86_64; maintainer decision Nov 16: Linux ships in 0.3.4).
- **Files to attach:** as 0.3.3 with the version changed (`capturemann-0.3.4-windows-x64-setup.exe` and `.zip`, `CaptureMann-v0.3.4-macos-<arch>.dmg`, `capturemann-0.3.4-linux-x86_64.tar.gz`, `capturemann_0.3.4_amd64.deb`, `.sha256`, `.minisig`, `SHA256SUMS.txt`, and the APT repository with the 0.3.0 to 0.3.4 `.deb` files). Check names with `scripts/check-release-assets.sh 0.3.4 <folder>`.
- **Headline numbers:** 89 MCP tools (`crates/cm-mcp/tests/stdio.rs` asserts it); Windows files are unsigned; macOS DMGs are signed and notarized by the maintainer; Linux files are signed with minisign by the maintainer.

---

## Release body

# CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards

0.3.4 has files for **Windows 11, macOS and Linux** (Ubuntu 24.04+, x86_64; the aarch64 package was not run this time). It adds scrolling capture, a click-by-click guide maker, text editing inside a screenshot and
boards, and fixes the recording bar drag on Windows.

## New

**Long Shot and Click Guide are marked Preview** (a label in the app with a Send feedback button; they are always available and there is no switch). They are new, work on the pages and apps we tried, and will keep changing; see Known limits.

- **Long Shot (scrolling capture, Preview).** Capture a whole page or window as one picture. **Auto** scrolls for you (it needs the Accessibility
  permission on macOS); **Hand** lets you scroll at your own pace. Vertical or horizontal, with a bar in the picker, a small HUD, a live side
  preview, a size limit and **Esc** to stop and keep what was captured. Fixed headers, footers and toolbars are kept once.
  Start it from the launcher, the menu or a shortcut you set in Settings (no default).
- **Click Guide (Preview).** Click through a task and CaptureMann writes the steps: each click is a step with a picture, a ring and a numbered badge.
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

## Known limits

- **Long Shot.** A page that redraws while it scrolls, or has fixed areas the picker cannot tell from content, can end early with "the next part did not
  line up"; the picture so far is kept. A covered window, a window that moved and a page that stopped moving can still show the same wording. **Hand scroll**
  works only from the on-screen bar: agents and the CLI cannot start it. On Windows, Windows Terminal passed in a test; a classic console window (conhost) ended with "nothing scrolled" before a fix that was not run again, and a program that redraws the screen (a full-screen editor) is not expected to work. **Esc** is a global stop key and cannot be changed (a terminal program that uses Esc never sees it). Quitting CaptureMann during a Long Shot or a guide discards it.
  A failed run with one frame is still saved to the library.
- **Click Guide.** **Step titles come from the text near the click and the app does not mark or blur anything, so a guide can contain private text: look at every
  step before you share or export it.** A guide started by an agent or the CLI shows only a small toast, no bar and no Stop button (stop it with `capturemann guide stop`);
  it needs the app running and its confirmation. A fast burst of clicks at one point becomes one "Double-click" step. Markdown export writes into the folder you choose
  and **overwrites** a file or folder of the same name without asking. A click on a window lying over the guided window can become a step.
- **Boards:** the preview in the sheet is drawn by the same code as the exported board (same margins, rounding, shadow, stamps, captions, title). Not built yet: the inspector does not fold so Title and Captions can sit below the fold, there is no warning for extreme picture ratios or for a board scaled down to fit 16,384 px, no drop tile to add a forgotten screenshot (use the arrows to reorder; to add one, start again), numeric values are set with presets, not sliders, and Bulk export shows no size estimate and does not remember its folder.
- **Retype, Move area and Blueprint are not redactions**: use Blur or Pixelate to hide information.
- Windows files are not code-signed (SmartScreen will warn). The speaker engine ("Find speakers") is not in the Windows files built on the lab laptop (built with `CM_NO_SPEAKER_MODELS=1`); the licence list inside the Windows package is the macOS one (the lab laptop has no bash and Python for the generator).
- **Linux:** tested on one ThinkPad T490 (Ubuntu 24.04.5, X11, Intel graphics) only; Wayland, AMD/NVIDIA, KDE, other distributions and the aarch64 package were not run for 0.3.4. Long Shot on Wayland cannot inject a scroll (use Hand scroll). Intel Mac hardware has not been tried.
- **Long Shot on real pages (Preview).** Chrome on macOS and Firefox on Linux joined the five test pages (carchargeos.com, YouTube channel videos, Wikipedia Rust, docs.rs/serde, a long essay) continuously, but a fixed panel that is not at the page edge (the Wikipedia contents list, the YouTube left rail, the Appearance panel) repeats once per slice of a very long picture. On Windows (Edge) YouTube ended after 2 frames at its sticky tab bar and carchargeos.com stopped where a frosted header sits over a dark image; Edge with a signed-in profile shows a "we are syncing" dialog that blocks the wheel (use a profile without it).
- **Boards.** Only the sheet preview was brought to the exported look in this release; the other differences listed above (labels, margins, shadow, inspector fold, warnings, drop tile, sliders) remain.

## What was checked

Reports are in `verification/` and all are **PARTIAL**; nothing was looked at on a screen by a person unless a report says so.

| System | Result | Report |
|---|---|---|
| macOS (Apple Silicon, dev bundle of commit `19f64bb3`, the final source commit) | standard 52 passed, 0 failed, 12 skipped of 64; full 58/0, 22 skipped of 80; hardware (screen only) 58/0, 30 skipped of 88 | `L1-macos-0.3.4-regression-{standard,full,hardware}-dev-bundle-19f64bb3-screen-2026-11-16-claude.md` (earlier: `L1-macos-0.3.4-dev-bundle-596c7956-*`, `...-26a05100-*`) |
| Windows 11 (installed candidate files of commit `596c7956`, unsigned, setup SHA-256 `27ec67d9…7e14`) | full 66 passed, 0 failed, 14 skipped of 80; hardware 68/0, 20 skipped of 88. **The final source commit `19f64bb3` differs from `596c7956` only in the board sheet preview and the Retype note (4 files); Windows files of `19f64bb3` were built but no regression ran on them** (see the build record) | `L1-windows-0.3.4-596c7956-final-regression-{full,hardware}-2026-11-16-claude.md`, `L1-windows-0.3.4-19f64bb3-candidate-build-only-2026-11-16-claude.md` |
| Linux x86_64 (Ubuntu 24.04.5, X11, candidate files of commit `596c7956`) | standard 57 passed, 0 failed, 7 skipped of 64; full 65/0, 15 skipped of 80; `.deb` install 1/0. Not run on `19f64bb3`; aarch64, Wayland and the hardware tier not run | `L1-linux-0.3.4-596c7956-regression-{standard,full,deb-install}-2026-11-16-claude.md` |
| Feature checks, macOS (six passes, Nov 14 to 15) | per-feature PARTIAL reports; Long Shot, Click Guide, boards, bulk export, editor tools, library, Published recordings, Vietnamese | `QA-0.3.4-summary-macos-pass{2..6}-*`, `QA-0.3.4-click-guide-macos-*`, `QA-0.3.4-long-shot-macos-*`, `QA-0.3.4-boards-bulk-*` |
| Feature checks, Windows (passes 1 to 6) | per-feature PARTIAL reports on a ThinkPad T490 at 150 % scaling | `QA-0.3.4-summary-windows-pass{3..6}-*`, `QA-0.3.4-cap-6-long-shot-windows-*`, `QA-0.3.4-gde-click-guide-windows-*`, `QA-0.3.4-brd-1-boards-windows-*`, `QA-0.3.4-blk-1-bulk-export-windows-*`, `QA-0.3.4-ann-{8,9,10,11}-*-windows-*`, `QA-0.3.4-imp-6-cmyk-jpeg-windows-*`, `QA-0.3.4-b1-*`, `QA-0.3.4-b2-b3-*`, `QA-0.3.4-final-build-windows-*`, `QA-0.3.4-windows-final-build-*` |
| Earlier single checks | Long Shot, Click Guide, fill quality, recording bar drag | `L1-macos-0.3.4-long-shot-*`, `L1-windows-0.3.4-long-shot-scroll-*`, `GDE-macos-0.3.4-quality-fixes-*`, `GDE-windows-0.3.3-guide-cases-*`, `S2-fill-quality-0.3.4-*`, `HUD-1-windows-{before-fix,checks,after-fix}-*` |

The Windows install of the **latest** build (commit `7fb80d70`) was checked only for installing, `capturemann --version` 0.3.4 and one start of the app (`QA-0.3.4-windows-final-build-2026-11-15-claude.md`);
no regression ran on it. Fixes made after the reports above (Long Shot fixed bars and early stops, the Click Guide step limit and queue, the Retype and Blueprint line boxes) were tested with fakes and, for the Long Shot sidebar case on macOS, measured in one run; most of them were not rerun on a screen.
The floating windows (Long Shot bar and HUD, Click Guide bar, picker, toasts) cannot be captured by the test tools, so their sizes, text and behaviour were read from the accessibility tree and nobody looked at them in the reports.

**Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed-DPI monitors, NVIDIA/AMD encoders, Linux aarch64, Wayland and the Linux hardware tier, the Windows 5-minute soak
(it needs a microphone), a Drive account for the paused-upload delete (B1: checked with a fixture only), Hand scroll through the real on-screen bar, the webcam bubble drag on Windows (no camera), the
countdown, retake and Tall-picture chip of Click Guide.

**Files and checksums (maintainer items are marked):**

- Windows (unsigned): `capturemann-0.3.4-windows-x64-setup.exe` and `capturemann-0.3.4-windows-x64.zip`; SHA-256 in `SHA256SUMS.txt` of this release. MAINTAINER: <FILL: confirm the attached files are the ones a regression covered>
- Linux x86_64: `capturemann-0.3.4-linux-x86_64.tar.gz` and `capturemann_0.3.4_amd64.deb`. MAINTAINER: <FILL: minisign signatures (`.minisig`) and the APT repository update with the 0.3.0 to 0.3.4 `.deb` files>
- macOS: MAINTAINER: <FILL: `CaptureMann-v0.3.4-macos-arm64.dmg` and `-x86_64.dmg`, built, signed, notarized and stapled by the maintainer, with their SHA-256>
- MAINTAINER: <FILL: Homebrew cask line, winget line, plugin line ("held by the directory until the organization is Verified" unless that changed)>
