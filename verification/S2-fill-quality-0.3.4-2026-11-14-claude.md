# S2 report: background-fill quality for Retype / Move area on real macOS and Windows UI (0.3.4)

- **Date:** 2026-11-14
- **Who:** Claude (agent, Track S2 redo)
- **Machines:** a MacBook (Apple Silicon, macOS 26, Retina, dark and light); the ThinkPad T490 (Windows 11 25H2 build 26200, 1920x1080, light and dark)
- **Build:** worktree branch `worktree-agent-acdbd7e91d6655148`, `cm-render` debug; measurer `s2_real_set` in `crates/cm-render/tests/touchup_034.rs` (ignored test, `CM_S2_DIR=<folder>`); no change to `touchup.rs`
- **Recognizer:** macOS Vision for all three sets (the Windows screenshots were copied to the Mac and measured there; the boxes are therefore Vision's, not the Windows OCR engine's)
- **Pass line (plan section 5):** 90 % of boxes within 6 levels (p98 of the largest channel difference between the stored fill and the ring) and no visible seam at 100 %
- **Privacy:** all screenshots were kept outside the repository (scratchpad), of system apps only (macOS: System Settings panes, Finder showing /Applications, TextEdit with sample text; Windows: Settings, File Explorer on C:\Windows and C:\Program Files, Edge on example.com in a private profile, Notepad, Calculator, Folder Options, System Properties, winver, a WinForms installer-style dialog). They were deleted with all copies on the laptop. Only counts are stored here.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1. macOS set: 20 window captures (`screencapture -l`), 10 dark, 10 light | PASS | Screen Recording permission was already there; theme set to light for the second half and back to dark afterwards |
| 2. Windows set: 27 window captures, light and dark | PARTIAL | Theme registry values restored (original light). Notepad shots are overlapped by two other Notepad windows (tabs of other lab runs) and are a messy but real "tabs + Mica title bar" case. The classic dialogs (Folder Options, System Properties, winver) and the WinForms dialog do not follow the dark theme, so their "dark" files duplicate the light ones (14 + 25 boxes counted twice); a locked-theme-free count is given below |
| 3. Measure | PASS | numbers below |
| 4. Look at 18 Retype renders at 100 % (fill only, empty text) | PASS with findings | below |
| 5. Cheap general improvement | tried, no gain, not kept | below |

## Results (boxes are the text lines Vision finds; rect floor/ceil +1 as the editor does)

| Set | Pictures | Boxes | Within 6 levels | Busy flagged | Failing and not flagged | Linear fills |
| --- | --- | --- | --- | --- | --- | --- |
| macOS dark | 10 | 634 | 622 (98.1 %) | 12 | 0 | 17 |
| macOS light | 10 | 631 | 579 (91.8 %) | 52 | 0 | 55 |
| **macOS all** | 20 | 1265 | **1201 (94.9 %)** | 64 | 0 | 72 |
| Windows dark | 13 | 517 | 501 (96.9 %) | 16 | 0 | 19 |
| Windows light | 14 | 487 | 467 (95.9 %) | 20 | 0 | 30 |
| **Windows all** | 27 | 1004 | **968 (96.4 %)** | 36 | 0 | 49 |
| Old set (36 repo pictures, no real OS UI), same measurer | 36 | 1416 | 1233 (87.1 %) | 183 | 0 | 206 |

Both real-UI sets clear the 90 % line, including per picture class: flat panels, list rows (Finder zebra rows), sidebars, tabs, toolbars, dialogs and the dark theme at 97 to 100 %. The weakest pictures are light-mode Displays (79 %), light Control Center (79 %), the Notepad shots (70 to 79 %, overlapped windows) and the WinForms dialog (71 %, 4 of 14 boxes). **Every box that fails the 6-level ring check was also flagged busy (zero silent failures, in all three sets)**, so the "busy" warning already catches them.

Kinds: 1193 solid and 72 linear (macOS), 955 and 49 (Windows).

Failing boxes by size of the error (macOS and Windows together, 100 boxes): 7 to 12 levels 10, 13 to 40 levels 20, 41 to 100 levels 22, over 100 levels 48.

### Worst boxes and why (largest first; coordinates are in the picture)

1. macOS Accessibility, 27x25 at (512,478), error 194: an app icon; Vision found a glyph inside the icon, the ring is the icon's own gradient plus the white rim.
2. macOS light Displays, four boxes 87x15 at x=1083 (errors 165 to 185): labels on the display-arrangement thumbnail, a translucent picture of the wallpaper; the ring crosses the thumbnail edge.
3. macOS dark Finder, 192x27 at (568,1571), error 175, and light Finder (568,1747), 156: the last visible rows, at the rounded bottom corner of the window (ring includes the window edge and the transparent corner).
4. macOS light Desktop and Dock, 258x27 at (1130,1751), 185x27 at (915,1751), errors 148 to 161: rows at the bottom edge of the window, same cause.
5. macOS dark Finder, 185x38 at (1323,35), error 155: the search field pill (text, magnifier icon and the pill's rim inside the ring).
6. Windows light Explorer, 76x40 at (1813,954), error 209: the view-toggle icons at the status bar's right end (icons, not text).
7. Windows Folder Options, 393x19 at (110,549), error 227: text beside a bitmap preview inside the dialog (the same in both themes).
8. Windows WinForms dialog, 59x17 at (424,173), error 209: white text on a small blue badge; the OCR box is wider than the badge, so the ring is half blue, half grey. 141x19 at (19,307), error 206: a progress bar label next to the bar.
9. Windows light Explorer Program Files, 73x10 at (306,485), error 240: a tiny box on a selection highlight edge.
10. Windows Settings (light display page), 120x35 at (1055,707), error 193: a control with a drop-down chevron in the ring.

Common causes in the 100 failing boxes: an icon or a control's own rim inside the box or the ring (most), the window's rounded bottom corners, a thumbnail picture (translucency over a wallpaper), a badge narrower than the OCR box. Mild failures (errors 7 to 40, 30 boxes) are wide boxes on gradients or Mica/vibrancy toolbars and slightly differently tinted neighbours (the Notepad tab strip and left list, the Finder toolbar search, Control Center rows). No failure was a plain translucent sidebar: the macOS sidebars, the Windows Mica title bars and the dark panels all passed.

## What I saw in the 100 % renders (18 crops, original left, Retype with empty text right)

- Flat dark and light panels, Finder zebra rows, Settings rows, Explorer rows and selected rows, About/Version text, a Calculator display, a Notepad cell and dark Settings headings: no visible seam; the right half is the plain background (a stray dot of one pixel in the dark "Widgets" label, invisible at normal size).
- A gradient behind a sidebar item and a purple gradient behind the "Videos" row label: the fill follows the gradient and fades correctly; the neighbouring icon is left intact.
- Where the OCR box contained an icon (Wi-Fi row, "Personalization" with its brush icon, Settings app icons, an HDR badge, the pop-up button's stepper) the icon is wiped to a flat or smeared patch, visibly wrong at 100 %. These are exactly the boxes flagged busy.
- A pop-up button text ("Automatic (MacBook Pro Microphone)" with its pill): the pill's rounded end is half removed, leaving a pale ghost; flagged busy, 15 levels.
- A button label ("Set Up Keyboard...") at error 0: the fill is clean but the tight Vision box left thin remnants of the top of the glyphs. This is a box-padding matter, not a fill matter (this test does not pad the rect beyond floor/ceil +1; I did not check what the editor does).
- A dark text block (error 22): the rounded dark card's corner is visible in the filled box (box overlapped the card edge).

## Cheap improvement tried

A robust refit of the linear gradient (fit, drop the 15 % worst-fitting ring pixels, fit again, keep the better): +2 boxes on the old set, 0 on macOS and Windows. Not kept; the failures are icons and controls in the box, not outliers pulling the fit. `touchup.rs` is unchanged and the golden tests pass (`cargo test -p cm-render --test touchup_034`: 18 passed, 4 ignored). Ideas not tried because they are specific or costly: shrinking the box to the glyph extent, rejecting boxes whose ring crosses a window edge, detecting icons.

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| S2 pass line (90 % within 6 levels, no visible seam) | PARTIAL pass | 94.9 % macOS, 96.4 % Windows on real UI; no seam in the passing crops; failing boxes are all flagged busy |
| ANN-9.AC2 (ring check) | consistent | zero failing-and-unflagged boxes in 3685 boxes over 83 pictures |

## Problems found

- OCR boxes that contain an icon or a control rim (about 4 % of text lines on real UI) produce a visibly wrong fill; the busy flag catches all of them in these sets.
- Vision's tight rect can leave glyph-top remnants when erased without padding (one crop); check the editor pads the box.
- Windows dark theme: the classic dialogs and WinForms ignore it, so the "dark" Windows count has 39 boxes duplicated from light.

## Not covered

The Windows OCR engine's own boxes (measured with Vision), Windows 10, mixed DPI, a Mac with a Retina scale other than 2x, Safari and Terminal (not captured), translucent windows over busy wallpapers (captures are of the window's own pixels).

## Verdict

PARTIAL. Recommendation: ship Retype and Move area on all fills with the busy warning (Cancel / Use anyway), no restriction by fill kind. The measure is good enough on real macOS and Windows UI (94.9 % and 96.4 %), and every failing box is flagged. The warning copy should mention icons and buttons ("this area contains an image or a control") since that is the real cause, and the editor should pad the erased box by a pixel or two.
