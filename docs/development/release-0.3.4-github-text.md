# GitHub release text for 0.3.4 (draft, not published)

Written from `specs/plans/release-0.3.4-plan.md`, the feature specs (`01-screenshot.md` CAP-6, `02-annotation.md` ANN-8 to ANN-11,
`19-click-guide.md`, `20-boards-and-bulk.md`, IMP-6.AC10 to AC13, REC-5.AC18, LIB-1.AC8 to AC11) and the 0.3.3 text. **Nothing here is
final until the regression on the final files exists, and a line about a feature that was cut must be deleted.** Every `<FILL: ...>` is a
placeholder for a fact that only exists on the release day (commit, checksums, report names, notarization, winget, Homebrew). Never write
"passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule). Steps: copy `release-day-0.3.3.md`.

- **Tag:** `v0.3.4` (never reuse a tag). **Title:** `CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards`.
- **Type:** draft, then **pre-release** (0.x). Windows 11 (x64), macOS 13+ (Apple Silicon and Intel), Linux (Ubuntu 24.04+, x86_64 and aarch64).
- **Files to attach:** as 0.3.3 with the version changed (`capturemann-0.3.4-windows-x64-setup.exe` and `.zip`, `CaptureMann-v0.3.4-macos-<arch>.dmg`,
  `capturemann-0.3.4-linux-*.tar.gz`, `capturemann_0.3.4_*.deb`, `.sha256`, `.minisig`, `SHA256SUMS.txt`). APT: run `make-apt-repo.sh` with the 0.3.0 to 0.3.4 `.deb` files.
- **Headline numbers:** 89 MCP tools (`crates/cm-mcp/tests/stdio.rs` asserts it); Windows files are unsigned; macOS DMGs are signed and notarized by the maintainer.

---

## Release body

# CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards

0.3.4 has files for **Windows 11, macOS and Linux**. It adds scrolling capture, a click-by-click guide maker, text editing inside a screenshot and
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
- Zoom Lens edge, guide export sheet, guide editor picture, long library path and the speaker-model script on PowerShell 5.1.
  <FILL: final list from the merged fixes>

## Known limits

- Long Shot: <FILL: the open Long Shot items after the last run, for example a terminal that redraws, lazy-loading pages>.
- Click Guide cannot start from a CLI or agent without the app running (it needs the app's confirmation and click listener).
- Windows files are not code-signed (SmartScreen will warn). The speaker engine is not in the Windows files.

## What was checked

Reports are in `verification/` and all are **PARTIAL**; nothing was looked at on a screen by a person unless a report says so.

| System | Result | Report |
|---|---|---|
| macOS (dev bundle `84bc00cd`) | full 53+, standard and hardware: <FILL: numbers from the final run> | `L1-macos-0.3.4-*` |
| Windows 11 (installed `84bc00cd`) | full 66/0 (14 skipped), hardware 68/0 (20 skipped) | `L1-windows-0.3.4-84bc00cd-final-regression-*` |
| Linux | <FILL: not run, or the numbers> | |
| Feature QA, macOS and Windows | per-feature PARTIAL reports | `QA-0.3.4-*` |

**Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed-DPI monitors, NVIDIA/AMD encoders, the Windows 5-minute soak
(it needs a microphone), a Drive account for the paused-upload delete.

<FILL: checksums, notarization, winget and Homebrew lines, as in the 0.3.3 text>
