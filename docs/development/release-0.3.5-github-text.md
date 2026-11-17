# GitHub release text for 0.3.5 (draft, not published)

Written on Nov 17, 2026 from `specs/plans/release-0.3.5-plan.md`, `specs/features/02-annotation.md` (ANN-12), `specs/features/19-click-guide.md`, the QA reports `verification/QA-0.3.5-*` and `L1-*-0.3.5-*`, and the 0.3.4 text. **Nothing here is final until the regression on the final files exists, and a line about a fix that was cut must be deleted.** Every `<FILL: ...>` is a placeholder for a fact that only exists on the release day (commit, checksums, report names and counts, notarization, winget, Homebrew, plugin). Never write "passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule); things the maintainer saw are written "reported by the maintainer". Steps: `release-day-0.3.5.md`.

- **Tag:** `v0.3.5` (never reuse a tag). **Title:** `CaptureMann 0.3.5: fixes, dashed lines and a feedback button`.
- **Type:** draft first; full release or pre-release is the maintainer's call (0.3.4 was a full release). Windows 11 (x64) and macOS 13+ (Apple Silicon and Intel). **Linux: <FILL: the Linux decision of `release-day-0.3.5.md`; the recommended wording is "Linux stays at 0.3.4; the APT repository is unchanged">.**
- **Files to attach:** `capturemann-0.3.5-windows-x64-setup.exe` and `.zip`, `CaptureMann-v0.3.5-macos-<arch>.dmg` (arm64 and x86_64), the `.sha256` files and `SHA256SUMS.txt` (and Linux files only if shipped). Check names with `scripts/check-release-assets.sh 0.3.5 <folder>`.
- **Headline numbers:** 89 MCP tools (`crates/cm-mcp/tests/stdio.rs` asserts it; no new tool); Windows files are unsigned; macOS DMGs are signed and notarized by the maintainer.

---

## Release body

# CaptureMann 0.3.5: fixes, dashed lines and a feedback button

0.3.5 is a small release with files for **Windows 11 and macOS** (<FILL: Linux sentence>). It fixes the bugs found in 0.3.4's Click Guide, Long Shot, transcripts and meeting minutes, and adds dashed and dotted lines for shapes, a Send feedback button in Settings > About, and a Preview label on Boards. Quick Capture is not in this release.

## New

- **Line styles.** Arrow, Line, Rectangle and Ellipse can be **solid, dashed or dotted** (a three-button control in the shape panel; it is remembered per tool and one undo step reverts it). An arrow's head stays solid. Agents and the CLI pass `style` in `annotate` ops (`"solid"`, `"dashed"`, `"dotted"`; a wrong value is refused naming the op). A screenshot without styles renders exactly as before, and an older app keeps the style when it saves the file (it draws it solid). Seen by the maintainer in the editor (reported by the maintainer, no stored report).
- **Send feedback in Settings > About.** A third button next to Source on GitHub opens a pre-filled GitHub issue form with the app version and the title `CaptureMann 0.3.5 general feedback`. Nothing is sent before you click, and no logs, paths, screenshots or library data are included.
- **Boards are marked Preview**, like Long Shot and Click Guide: a label, a short intro card the first time, and a Send feedback button. Boards work as before; the MCP tool `create_board` and the CLI command `board` are marked preview in their descriptions.

## Fixed

**Click Guide**
- A guide started by an agent or the CLI now shows a bar with the recording dot, the step count, "Started by agent <client>" and a **Stop** button (the toast has Stop too, and the sidebar tile shows it running). If the followed window moves, the bar shows **Paused** and a **Resume** button.
- Markdown export no longer overwrites: files are named `guide-YYYY-MM-DD-HHMMSS.md` with an `images-YYYY-MM-DD-HHMMSS/` folder, a guide with no steps is refused, and the success toast has an **Open folder** button.
- `export_guide` (agents) refuses to write over a file that exists, and a path outside the library and your export folders (<FILL: the folders the maintainer confirmed: the Documents, Desktop and Downloads folders and `export.folders` in the app's settings, if confirmed>) asks you first and shows the exact path.
- When a clicked button has an accessibility name, the step title uses it (`Click "Save"`); otherwise it uses the nearest text as before. Special Markdown characters in titles are escaped on export.
- When the guided window moves or resizes, the guide pauses with a notice instead of recording a wrong step, and a click on a different window is ignored.

**Long Shot**
- On Windows, Long Shot of YouTube and carchargeos.com no longer ends after 2 frames (the scroller sent whole wheel notches and static page areas were judged too strictly). A large Long Shot no longer reports an error after the picture was saved (`line too long`): the reply names the saved capture.

**Window capture**
- `capturemann capture screenshot --window <text>` refuses an ambiguous title (`invalid_input`, reason `ambiguous_target`) and lists the matching windows (id, app, title) so you can choose by id; an exact title still wins. This is for the CLI; over MCP a window is chosen by id.
- On Windows, a capture of a program that is not DPI-aware filled only the top-left of the picture; the frame is now sized from the real content. <FILL: only if the lab check on Windows is stored: "checked on the ThinkPad at 150 % scaling in `<report>`"; otherwise keep the sentence under Known limits>

**Transcripts and meeting minutes**
- Text nobody said after a long muted stretch (a sign-off line repeated four times in a 16-minute Vietnamese meeting) is no longer written: digital silence is handled when the room level is measured and when the audio is cut into pieces. Re-generate after updating; a transcript already made is unchanged. (Reported fixed by the maintainer on the recording that showed it; checked with synthetic audio on both systems in `QA-0.3.5-pass2-{macos,windows}-*`.)
- Meeting minutes asked for in Vietnamese are no longer a mix of English and Vietnamese: the output language is named in the prompt, a section in the wrong language is regenerated once, and the section titles and labels follow the report's language. A report already made is unchanged; Re-generate uses the fix. (Reported by the maintainer; no stored report.)

**Boards, Settings and other windows**
- The New board sheet says "Use the arrows to change the order" (it said drag) and its pill says "Preview"; the sheet now sits above the sidebar Preview pills, so the Strip and Free labels are readable.
- The disabled Make a board button names what is wrong when a Click Guide is selected.
- The rail with Settings, Agents and AI models can be scrolled in a short window (it sat below the window in the guide editor).
- The export confirmation dialog exposes its text, including the destination, to screen readers.
- Vietnamese strings for everything new.

## Known limits

- **Long Shot.** A page that redraws while it scrolls, or has fixed panels the picker cannot tell from content, can end early with "the next part did not line up"; the picture so far is kept. Fixed side panels away from the page edge repeat in every slice of a very long picture. **Hand scroll** works only from the on-screen bar. **Esc** is a global stop key and cannot be changed. On Windows (Edge) the local test page, carchargeos.com and YouTube joined after the fix (Windows lab build, `QA-0.3.5-bug6-long-shot-windows-2026-11-17-claude.md`; checked by frame counts and end reasons, the pictures were not looked at). Chrome and Firefox on Windows were not run.
- **Click Guide.** Titles that come from the nearest text (when a button has no accessibility name) can contain private text, and the app does not mark or blur anything: look at every step before you share a guide. A Vietnamese sidebar tile label wraps to two lines and its Preview pill touches the label. Failed clicks at the 500-step cap are silent. A fast burst of clicks at one point becomes one "Double-click" step.
- **Window capture on Windows.** <FILL: DPI-unaware window fix, and whether the lab check is stored>
- **Transcripts.** The fix for text after muted stretches was checked on the maintainer's Vietnamese recording (reported) and on synthetic English speech; other languages and the live transcription path are not covered. A very short recording can still contain text nobody said and no `[unclear]` mark (work is planned). The last word of a transcript piece can have the same start and end time.
- **Retype, Move area and Blueprint are not redactions**: use Blur or Pixelate to hide information.
- The BUG-3 confirmation shows the destination, but nobody has looked at the dialog on a screen (the test tools cannot capture it; it was read through the accessibility tree on Windows).
- Windows files are not code-signed (SmartScreen will warn). <FILL: whether the shipped Windows files have the speaker engine ("Find speakers"; the GitHub package has it) and which licence list is inside the package>
- **Linux:** <FILL: the Linux sentence; if Linux is skipped, "Linux stays at 0.3.4 and gets these fixes in the next release">. Intel Mac hardware has not been tried.

## What was checked

Reports are in `verification/` and all are **PARTIAL**. The overlay windows (bars, HUD, picker, toasts, dialogs) cannot be captured by the test tools: they were read from the accessibility tree and nobody looked at them in these reports.

| System | Result | Report |
|---|---|---|
| macOS (Apple Silicon, <FILL: final files or candidate of commit `<sha>`>) | <FILL: standard and full passed/failed/skipped on the final files> | <FILL: `L1-macos-0.3.5-<sha>-regression-{standard,full}-*`> |
| Windows 11 (<FILL: installed files of commit `<sha>`>) | <FILL: standard, full, hardware passed/failed/skipped on the final files> | <FILL: `L1-windows-0.3.5-<sha>-regression-*`> |
| Earlier runs on commit `7cd4e31d` (before BUG-12 to BUG-18) | macOS dev bundle: standard 51 passed, 1 failed, 12 skipped of 64; full 57 / 1 / 22 of 80 (the failure was a test case that exported outside the trusted folders, fixed in `5678dc90`; the case passes). Windows lab build (no speaker engine, unsigned): standard 57 / 0 / 7, full 66 / 0 / 14, hardware 68 / 1 / 19 (`rec.camera` 10.1 fps in a dark room; the installed 0.3.4 gave the same) | `L1-macos-0.3.5-dev-7cd4e31d-regression-{standard,full}-2026-11-17-claude.md`, `L1-windows-0.3.5-lab-7cd4e31d-regression-{standard,full,hardware}-2026-11-17-claude.md` |
| QA pass 1, macOS, on the shipped 0.3.4 DMG | no P1; five polish findings, one became a fix (the ambiguous window title) | `QA-0.3.5-pass1-macos-2026-11-17-claude.md` |
| QA pass 2, macOS (dev bundle of `7cd4e31d`) | no P1; the items that need a real click (the accessibility title, the moved window, Long Shot and Click Guide on screen) could not be driven | `QA-0.3.5-pass2-macos-2026-11-17-claude.md` |
| QA pass 2, Windows 11 (lab build of `7cd4e31d`) | no P1; eight findings, all fixed after it (BUG-12 to BUG-18); Retype not run | `QA-0.3.5-pass2-windows-2026-11-17-claude.md` |
| Long Shot on Windows | cause found and fixed; frame counts and end reasons on three pages | `QA-0.3.5-bug6-long-shot-windows-2026-11-17-claude.md` |

Fixes made after those reports (BUG-12 to BUG-18) were tested with fakes and unit tests; <FILL: which of them have a stored check on the final files: the Windows lab check of the large reply, the DPI-unaware window and the dialog text, and the maintainer's look at the board sheet, the paused guide bar and the rail>.

**Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed-DPI monitors, NVIDIA/AMD encoders, Linux (<FILL: unless decided>), the 5-minute soak (it needs a microphone), Retype on Windows, the muted-stretch fix on languages other than the maintainer's Vietnamese recording and an English test, and Long Shot in Chrome and Firefox on Windows.

**Files and checksums (maintainer items are marked):**

- Windows (unsigned): `capturemann-0.3.5-windows-x64-setup.exe` and `capturemann-0.3.5-windows-x64.zip`; SHA-256 in `SHA256SUMS.txt` of this release. MAINTAINER: <FILL: confirm the attached files are the ones a regression covered and say whether they are the GitHub workflow's files>
- macOS: MAINTAINER: <FILL: `CaptureMann-v0.3.5-macos-arm64.dmg` and `-x86_64.dmg`, built, signed, notarized and stapled by the maintainer, with their SHA-256>
- MAINTAINER: <FILL: Homebrew cask line, winget line, plugin line ("held by the directory until the organization is Verified" unless that changed)>
