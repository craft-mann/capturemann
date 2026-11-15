# QA 0.3.4 Click Guide (GDE), macOS pass 4 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, macOS pass 4)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `a4f28a07`), isolated library, config and socket under `/tmp/cmq4`; only my own app instance was started and killed; my own Chrome profile (`--user-data-dir=/tmp/cmq4/chrome`), one Terminal window and a small AppKit fixture window (`qafix`, 5 buttons, a fake e-mail label) were used and closed. The maintainer's library, Chrome and Terminals were not touched (the window picker listed their windows; I only searched for my fixture).
- **FFmpeg:** not used.
- **Method:** pictures taken with `screencapture` of the app window; windows excluded from capture (Long Shot overlay, HUD, preview, bar, toasts, dialogs) were read through the accessibility tree and `CGWindowListCopyWindowInfo` rectangles, or marked NOT SEEN. Synthetic clicks (CGEvent) only work on a non-key window after a pointer move first. The Long Shot overlay bar has no accessibility children, so Hand scroll could not be driven (NOT RUN).
- **Fixture:** AppKit window "QA Fixture Window" (5 buttons, "Contact: jane.doe@example.com"); display guides also captured my own windows. Step pictures contain my desktop and are not stored here.

## Steps

| Case | Result | Evidence |
| --- | --- | --- |
| Library header buttons only on the Click Guides page | PASS | All captures header: only Import; Click Guides page: Import, window-guide icon button, New Click Guide; Screenshots/Recordings: none (AX) |
| Start from sidebar tile / rail | PASS | bar 586 x 58 at 463,45 (display), 688 x 58 (followed window); tile reads "Click Guide, running" |
| Bar buttons (AX) | PASS | Stop 44, Pause 84 (Resume 96), Note step 106, Undo step 108 |
| 5 clicks (G05) | PASS | titles Click "Open", "Save", "Share", "Export", "Finish"; ring + badge on the picture; a step shows in `guide get` up to about 3 s after the click |
| Pause, 3 clicks, Resume, 1 click (G07) | PASS | 1 step added |
| Note step / Undo step from the bar (G13) | PASS | 5 to 6 to 5 |
| Double click, right click (G10) | PASS | Double-click "Open", Right-click "Save" |
| Spam: 20 clicks in about 3 s | PASS | 10 steps, all "Double-click" (pairs merged); app responsive; RSS 847 MB (after Long Shots too) |
| Click outside the followed window (G06) | PASS | ignored |
| Window guide: picker "Capture a window" (search, Apps list, Grid/List, Cancel, Start guide), search for fixture | PASS | AX; purpose text of S16 NOT SEEN |
| **Followed window moved 100 pt then click "Save" (G20)** | **FAIL (known G-GD-6)** | step titled Click "Export", ring on Export, click point stored relative to the OLD window origin (220,210 instead of 120,130) |
| Window closed (G18) | PASS | toast read: "QA Fixture Window closed. The Click Guide is paused. Your 2 steps are kept." [Pick another window, Stop and open guide, Dismiss]; bar swapped to Stop + Pick a window... |
| Toast "Stop and open guide" (C2) | PASS (second run) | guide stopped, bar gone, editor opened with the guide. My first try clicked about 7 s after the toast appeared (toast already gone), so the first result was my timing |
| Stop and open guide from the bar | PASS | editor opens with the steps |
| Editor at 1180 x 800: picture fit, caption "click at x, y - taken on mouse-down", Edit image/Retake/Duplicate/Delete, Focus crop | PASS | seen |
| **Editor at the minimum width 720** | FAIL (low) | right pane picture is cut at the left and right (no fit) |
| Full-size viewer | PARTIAL | **Esc closes it (PASS)**; label "full size 1,400 x 933" while the step crop is 2,400 x 1,600 px (label is the preview size, low); at 720 wide the picture is cut at the left |
| Click-through | UNCONFIRMED (low) | with synthetic clicks, pressing "Export Markdown" also opened the full-size viewer of step 1 under the sheet (the button is above the picture). Recheck with a real mouse |
| Export sheet (Markdown/HTML/PDF, headings, image size 1x/2x, Intro/Step notes/Note steps) | PASS (seen) | no redaction warning or "Review step" line |
| Markdown with no folder chosen | PASS | opens the native folder chooser (it shows the user's own folders: consider that) |
| Markdown export | PASS | `guide.md` + `images/step-01..18.png`, 800 x 533 at 1x |
| HTML export, hostile titles/notes | PASS | no script/img element, text escaped; PDF shows the text literally, emoji/Arabic/CJK as tofu (known) |
| **Markdown escaping (G45)** | FAIL (known G-GD-13) | title/step titles keep `[x](url)`, `![i](url)` and backticks live (link and image injection); notes are written as Markdown (`# h`, table, *b*); only `<` is escaped |
| **Folder reuse (G46)** | FAIL (known) | second guide exported to the same folder overwrote `guide.md` silently and left the first guide's `step-02.png`; a 0-step guide exports "0 step(s) written" as success |
| Copy as rich text, paste into TextEdit (rich) | PASS (pasted with images) | images arrive at full pixel size (very large in TextEdit). Notes not run |
| **Agent/CLI-started guide (G33)** | FAIL (known G-GD-1) | confirm dialog "Start a Click Guide: CaptureMann will take a screenshot of each mouse click (clicks only; keystrokes are never recorded)" with Cancel / Start guide; then **no bar**, only a toast "Started by agent via MCP" (said MCP, started by the CLI), no Stop on it, the sidebar tile stays "Click Guide" (not running); stopped with `guide stop` |
| Step titles from clicked text (G36) | FAIL (known G-GD-10) | clicking Chrome page text gave Click "ember anchor marble cobalt canyon orcha..." (40 chars of body text); a click on my app gave Click "茴" and Click here |
| kill -9 mid guide (G28) | PASS | relaunch: guide listed as stopped with the 3 steps kept, no orphan step folder, no lock; **no notice to the user** |
| Redaction proposals removed | PASS | no amber line, no sheet warning, no "Review" button; only the serialized field `has_proposals` remains in `guide.json` and the binary |
| Default title local time | PASS | Click Guide 2026-11-15 14.45 (UTC+7) |
| Countdown, collapse of the bar, Vietnamese, 0/1/100/500 steps, retake, reorder by drag, covered window, resize while followed, display disconnect, sleep/lock, accessibility of the bar by keyboard, Windows items | NOT RUN | |

## Problems found

- **CG-1 (medium, known)**: moving a followed window records a wrong click point, wrong title and a ring on the wrong control.
- **CG-2 (medium, known)**: agent/CLI-started guide has no bar, no Stop in its toast, tile not running, wrong source wording ("MCP").
- **CG-3 (medium-low, known)**: Markdown export does not escape inline Markdown; folder reuse overwrites and leaves stale pictures.
- **CG-4 (low-medium, known)**: titles carry the clicked text (page body text, personal data).
- **CG-5 (low)**: editor picture not fitted at 720 px; viewer label size; viewer cut at 720.
- **CG-6 (low, unconfirmed)**: click-through from "Export Markdown" to the picture viewer.
- **CG-7 (low)**: kill -9 recovery is silent.

## Verdict

PARTIAL.
