# QA 0.3.4 Click Guide bar, window start, editor picture, title, macOS pass 2 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, second pass)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** installed dev bundle `~/Applications/CaptureMann-dev.app` of main `84bc00cd`, isolated library/config/socket under `/tmp/cmq2`; the maintainer's app was not running.
- **Important limit:** the Click Guide bar (HUD), the countdown, the toast and the window picker are floating windows excluded from screen capture (share state 0, `CGWindowListCopyWindowInfo`; `screencapture -l` says "could not create image from window"; no accessibility tree). **I could not see the bar, so the check against `S3-click-guide-recording.png` was NOT done.** Only the window geometry was read.

| Item | Result | Evidence |
| --- | --- | --- |
| Bar shows Stop (red square), Step n, label, Pause, Note step, Undo step all visible | NOT SEEN | the bar window is 580 x 58 pt at the top centre (466,45). The design bar is about 525 pt wide (882 of 2419 px at 1.19 scale and 2x), so there is 55 pt of room; contents unverified. In Vietnamese the window was the same 580 x 58 |
| Start from the app ("New Click Guide") | PASS | the bar window opened; two clicks became steps (titles "Click ..." from the nearest text); a click on the bar's left edge where Stop is expected (496,74) stopped the guide, closed the bar, saved the guide (guide count 2 to 3) and opened the guide editor |
| Start on a window ("Click Guide of a window...") | NOT RUN through the GUI | the picker is hidden from capture and was not driven blind. A window guide through the CLI (`guide start --window "QA Fixture"`) started (Confirm dialog answered with Tab, Return) and I clicked in my fixture; no bar opened for the CLI path (only the 440 x 96 toast window) |
| Default title uses local time | PASS | "Click Guide 2026-11-15 09.13" and "...09.23" while the screen clock read 9:12 to 9:23 |
| G1: editor picture not clipped | PARTIAL | the picture now starts at the top of its box (no 250 px blank). Its lower edge is still cut: a 1512 x 982 display step fills the box width (686 pt) and about 25 pt of height are outside the box (the last sidebar row is cut). Not checked with the 1400 x 984 window fixture of pass 1. NOT saved as a picture (it shows the maintainer's other windows) |
| Stale bar after CLI stop | FAIL (seen once) | after `capturemann guide stop` of a guide started from the library button, the 580 x 58 bar window stayed open; clicking its Stop gave an error toast ("not recording"), and the library button did nothing while it was there. Not reproduced in my second try (the click may have missed the button); a restart cleared it. C1 (medium, unconfirmed) |
| Step label of a library-button guide | PASS | step 1 title was taken from page text near the click ("Click \"Hom nay 09:13 - 4,6 MB\"", a card caption); fine |
| Privacy | note | a display guide is a full-screen picture of whatever is on the screen, including other windows; the editor shows them at once. That is by design for a display target |

## Verdict

PARTIAL. The most important item (the bar contents against S3) could not be checked by capture. Suggest a person looks, or a debug switch that lets the HUD be captured.
