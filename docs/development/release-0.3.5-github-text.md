# GitHub release text for 0.3.5 (draft, not published)

Written on Nov 17, 2026 from `specs/plans/release-0.3.5-plan.md`, `specs/features/02-annotation.md` (ANN-12), `specs/features/19-click-guide.md`, the QA reports `verification/QA-0.3.5-*` and `L1-*-0.3.5-*`, and the 0.3.4 text. **Nothing here is final until the regression on the final files exists, and a line about a fix that was cut must be deleted.** Every `<FILL: ...>` is a placeholder for a fact that only exists on the release day (commit, checksums, report names and counts, notarization, winget, Homebrew, plugin). Never write "passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule); things the maintainer saw are written "reported by the maintainer". Steps: `release-day-0.3.5.md`.

- **Tag:** `v0.3.5` (never reuse a tag). **Title:** `CaptureMann 0.3.5: fixes, dashed lines and a feedback button`.
- **Type:** draft first; full release or pre-release is the maintainer's call (0.3.4 was a full release). Windows 11 (x64) and macOS 13+ (Apple Silicon and Intel). **Linux ships too (decided Nov 18, 2026): x86_64 and aarch64 tarballs and `.deb` files, signed with minisign by the maintainer; the APT repository gets the 0.3.5 `.deb` files after publishing.**
- **Files to attach:** `capturemann-0.3.5-windows-x64-setup.exe` and `.zip`, `CaptureMann-v0.3.5-macos-<arch>.dmg` (arm64 and x86_64), the `.sha256` files and `SHA256SUMS.txt` and the four Linux files with their `.minisig` files. Check names with `scripts/check-release-assets.sh 0.3.5 <folder>`.
- **Headline numbers:** 89 MCP tools (`crates/cm-mcp/tests/stdio.rs` asserts it; no new tool); Windows files are unsigned; macOS DMGs are signed and notarized by the maintainer.

---

## Release body

# CaptureMann 0.3.5: fixes, dashed lines and a feedback button

0.3.5 is a small release with files for **Windows 11, macOS and Linux** (Ubuntu 24.04 and newer, x86_64 and aarch64). It fixes the bugs found in 0.3.4's Click Guide, Long Shot, transcripts and meeting minutes, and adds dashed and dotted lines for shapes, a Send feedback button in Settings > About, and a Preview label on Boards. Quick Capture is not in this release.

## New

- **Line styles.** Arrow, Line, Rectangle and Ellipse can be **solid, dashed or dotted** (a three-button control in the shape panel; it is remembered per tool and one undo step reverts it). An arrow's head stays solid. Agents and the CLI pass `style` in `annotate` ops (`"solid"`, `"dashed"`, `"dotted"`; a wrong value is refused naming the op). A screenshot without styles renders exactly as before, and an older app keeps the style when it saves the file (it draws it solid). Seen by the maintainer in the editor (reported by the maintainer, no stored report).
- **Send feedback in Settings > About.** A third button next to Source on GitHub opens a pre-filled GitHub issue form with the app version and the title `CaptureMann 0.3.5 general feedback`. Nothing is sent before you click, and no logs, paths, screenshots or library data are included.
- **Boards are marked Preview**, like Long Shot and Click Guide: a label, a short intro card the first time, and a Send feedback button. Boards work as before; the MCP tool `create_board` and the CLI command `board` are marked preview in their descriptions.

## Fixed

**Click Guide**
- A guide started by an agent or the CLI now shows a bar with the recording dot, the step count, "Started by agent <client>" and a **Stop** button (the toast has Stop too, and the sidebar tile shows it running). If the followed window moves, the bar shows **Paused** and a **Resume** button.
- Markdown export no longer overwrites: files are named `guide-YYYY-MM-DD-HHMMSS.md` with an `images-YYYY-MM-DD-HHMMSS/` folder, a guide with no steps is refused, and the success toast has an **Open folder** button.
- `export_guide` (agents) refuses to write over a file that exists, and a path outside the library and your export folders (your Documents, Desktop and Downloads folders and any folders you list under `export.folders` in the `settings.json` file (there is no Settings page for it)) asks you first and shows the exact path.
- When a clicked button has an accessibility name, the step title uses it (`Click "Save"`); otherwise it uses the nearest text as before. Special Markdown characters in titles are escaped on export.
- When the guided window moves or resizes, the guide pauses with a notice instead of recording a wrong step, and a click on a different window is ignored.

**Long Shot**
- On Windows, Long Shot of YouTube and carchargeos.com no longer ends after 2 frames (the scroller sent whole wheel notches and static page areas were judged too strictly). A large Long Shot no longer reports an error after the picture was saved (`line too long`): the reply names the saved capture.

**Window capture**
- `capturemann capture screenshot --window <text>` refuses an ambiguous title (`invalid_input`, reason `ambiguous_target`) and lists the matching windows (id, app, title) so you can choose by id; an exact title still wins. This is for the CLI; over MCP a window is chosen by id.
- On Windows, a capture of a program that is not DPI-aware filled only the top-left of the picture; the frame is now sized from the real content. Checked on the ThinkPad at 150 % scaling with a test window that is not DPI-aware: the picture fills the frame and a DPI-aware window (Notepad) is unchanged (`QA-0.3.5-windows-ci-package-9dcfb5c3-2026-11-18-claude.md`).

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
- **Window capture on Windows.** The DPI-unaware fix was checked on one machine (150 % scaling, one test window); other programs and scale factors were not tried.
- **Transcripts.** The fix for text after muted stretches was checked on the maintainer's Vietnamese recording (reported) and on synthetic English speech; other languages and the live transcription path are not covered. A very short recording can still contain text nobody said and no `[unclear]` mark (work is planned). The last word of a transcript piece can have the same start and end time.
- **Retype, Move area and Blueprint are not redactions**: use Blur or Pixelate to hide information.
- The BUG-3 confirmation shows the destination. The maintainer ran the export checks by hand on macOS (a folder under Documents without a dialog, a folder outside with the dialog, an added `export.folders` entry without a dialog; reported by the maintainer, no stored report). On Windows the dialog was read through the accessibility tree only.
- Windows files are not code-signed (SmartScreen will warn). They are the GitHub "Windows package" workflow's files and include the speaker engine ("Find speakers" ran in the regression on the installed files).
- **Linux:** the files were tested on a ThinkPad (x86_64, Ubuntu 24.04) and in a headless aarch64 VM, where every desktop-session case is skipped; Long Shot and Click Guide were not run on Linux. Intel Mac hardware has not been tried.

## What was checked

Reports are in `verification/` and all are **PARTIAL**. The overlay windows (bars, HUD, picker, toasts, dialogs) cannot be captured by the test tools: they were read from the accessibility tree and nobody looked at them in these reports.

| System | Result | Report |
|---|---|---|
| macOS (Apple Silicon, the notarized DMG's app of commit `9dcfb5c3`, screen only: no microphone, camera or system audio) | standard 52 passed, 0 failed, 12 skipped of 64; full 59 / 0 / 21 of 80; hardware 59 / 0 / 29 of 88 | `L1-macos-0.3.5-dmg-9dcfb5c3-screen-regression-{standard,full,hardware}-2026-11-17-claude.md`, `QA-0.3.5-macos-dmg-9dcfb5c3-2026-11-17-claude.md` |
| Windows 11 (the GitHub package of commit `9dcfb5c3`, installed on the ThinkPad) | standard 57 passed, 0 failed, 7 skipped of 64; full 68 / 0 / 12 of 80; hardware 70 / 0 / 18 of 88 (a first standard run used an old test build and failed one case that exports outside the trusted folders: 56 / 1; kept as a labelled report) | `L1-windows-0.3.5-ci-9dcfb5c3-installed-regression-{standard,full,hardware}-2026-11-18-claude.md`, `QA-0.3.5-windows-ci-package-9dcfb5c3-2026-11-18-claude.md` |
| Linux aarch64 (candidate files of commit `9dcfb5c3`, headless VM) | standard 42 / 0 / 22 of 64; full 44 / 0 / 36 of 80; `.deb` install 1 / 0 | `L1-linux-aarch64-0.3.5-9dcfb5c3-regression-{standard,full,deb-install}-2026-11-18-claude.md` |
| Linux x86_64 (candidate files of commit `9dcfb5c3`, ThinkPad, Ubuntu 24.04, X11) | standard 57 passed, 0 failed, 7 skipped of 64; full 64 / 0 / 16 of 80 | `L1-linux-0.3.5-regression-{standard,full}-x11-candidate-deb-2026-11-18-claude.md`, `QA-0.3.5-linux-x86_64-9dcfb5c3-2026-11-18-claude.md` |
| Earlier runs on commit `7cd4e31d` (before BUG-12 to BUG-18) | macOS dev bundle: standard 51 passed, 1 failed, 12 skipped of 64; full 57 / 1 / 22 of 80 (the failure was a test case that exported outside the trusted folders, fixed in `5678dc90`; the case passes). Windows lab build (no speaker engine, unsigned): standard 57 / 0 / 7, full 66 / 0 / 14, hardware 68 / 1 / 19 (`rec.camera` 10.1 fps in a dark room; the installed 0.3.4 gave the same) | `L1-macos-0.3.5-dev-7cd4e31d-regression-{standard,full}-2026-11-17-claude.md`, `L1-windows-0.3.5-lab-7cd4e31d-regression-{standard,full,hardware}-2026-11-17-claude.md` |
| QA pass 1, macOS, on the shipped 0.3.4 DMG | no P1; five polish findings, one became a fix (the ambiguous window title) | `QA-0.3.5-pass1-macos-2026-11-17-claude.md` |
| QA pass 2, macOS (dev bundle of `7cd4e31d`) | no P1; the items that need a real click (the accessibility title, the moved window, Long Shot and Click Guide on screen) could not be driven | `QA-0.3.5-pass2-macos-2026-11-17-claude.md` |
| QA pass 2, Windows 11 (lab build of `7cd4e31d`) | no P1; eight findings, all fixed after it (BUG-12 to BUG-18); Retype not run | `QA-0.3.5-pass2-windows-2026-11-17-claude.md` |
| Long Shot on Windows | cause found and fixed; frame counts and end reasons on three pages | `QA-0.3.5-bug6-long-shot-windows-2026-11-17-claude.md` |

Fixes made after those reports (BUG-12 to BUG-18) were tested with fakes and unit tests, and BUG-12 to BUG-18 were then checked on Windows on the installed files of commit `9dcfb5c3` through UI Automation and the CLI (the large Long Shot reply, the sheet above the pills, the paused guide bar with Resume, the DPI-unaware window, the board hint, the scrolling rail, the dialog text) in `QA-0.3.5-windows-ci-package-9dcfb5c3-2026-11-18-claude.md`. Nobody looked at BUG-13, 14 and 17 on a macOS screen. On macOS the large reply and the About title were tested with fakes only.

**Not run:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed-DPI monitors, NVIDIA/AMD encoders, Linux Wayland and the Linux hardware tier, Long Shot and Click Guide on Linux, the 5-minute soak (it needs a microphone), Retype on Windows, the muted-stretch fix on languages other than the maintainer's Vietnamese recording and an English test, and Long Shot in Chrome and Firefox on Windows.

**Files and checksums (maintainer items are marked):**

- Windows (unsigned): `capturemann-0.3.5-windows-x64-setup.exe` and `capturemann-0.3.5-windows-x64.zip`; SHA-256 in `SHA256SUMS.txt` of this release. They are the files of the GitHub "Windows package" run 44100622053 on `9dcfb5c3` (the files the Windows regression ran on). Zip SHA-256 `427f53b2d67e2020041b5ed7ba049e1df44a35314fbea0e0eeb50ce9bea183df`, setup `cae7b644a6676e12ffd433cd10be839b1855f08cdb266ee11e855fe26271f011`.
- macOS: `CaptureMann-v0.3.5-macos-arm64.dmg` (SHA-256 `53fe3400e7f3940ad916f3401da1fe3a579e7fc266cef9fb86a44e41251dd67c`) and `CaptureMann-v0.3.5-macos-x86_64.dmg` (`cc8ab66c39e48998b19184a55adea60058e523493e99ab29bba8800a186a903a`), built, signed, notarized and stapled by the maintainer from the same commit. The Intel DMG was checked for signature, notarization and version only.
- Linux (signed with minisign by the maintainer; key fingerprint as for earlier releases): `capturemann-0.3.5-linux-x86_64.tar.gz` (`7c869386d9738257576b50c17a0c4b639cd6417c0d1a970d0816b42d90913395`), `capturemann_0.3.5_amd64.deb` (`a6fba5aeba57d0966227ed26c24df3a05ef02e89e5892b52b05f3822928a6e7b`), `capturemann-0.3.5-linux-aarch64.tar.gz` (`07aed3c823330fd794dcf16732487ef91485fccd3fe6d409de93b51a5badc0d4`), `capturemann_0.3.5_arm64.deb` (`6a859036a70d8a85043b408b9488ffcc55ebc035851df1af633e662396373cb9`).
- After publishing: the Homebrew cask, the winget package and the APT repository (with the 0.3.5 `.deb` files) are updated. The Claude plugin is held by the directory until the organization is Verified; it installs from this repository meanwhile.
