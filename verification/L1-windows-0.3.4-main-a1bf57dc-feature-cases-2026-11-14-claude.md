# L1 Windows 0.3.4 feature cases on main a1bf57dc (PARTIAL)

- **Date:** 2026-11-14
- **Who:** Claude (agent), on the dedicated lab laptop; nobody watched
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, one display 1920x1080 at 150 %, Edge, logged-in desktop session (scheduled-task runner `win-run-in-session.ps1`)
- **Build:** commit `a1bf57dc` (main), release build on the laptop (`cargo build --release --no-default-features`, crt-static, no LTO) of `capturemann`, `cm-daemon`, `cm-mcp`, `cm-overlay`, `cm-desktop`, `cm-regress`; FFmpeg, llama-server and licence files taken from the installed 0.3.3 package (same pins). Package folder `C:\lab\q034\pkg`; not an installer build, no speaker engine (`--no-default-features`). `capturemann --version` prints 0.3.4. Hashes are in the three `L1-windows-0.3.4-main-a1bf57dc-regression-*` reports.
- **Data:** isolated library, config folder, pipe and secrets file (`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET=\\.\pipe\cm-q034-claude`, `CAPTUREMANN_SECRETS=file`); Edge with a throw-away `--inprivate` window on local test pages (`text.html`, `buttons.html`, `sticky.html`, `long-page.html`); a synthetic CMYK JPEG made with ImageIO on the Mac. Everything on the laptop was deleted at the end.
- **Method:** the shipped CLI against the daemon (headless or hosted by `cm-desktop`), SendInput clicks, wheel notches and the Down key from a PowerShell helper, the app window captured with `CopyFromScreen` (floating windows of the app are excluded from capture: the confirmation dialog, the Click Guide bar and the toasts are not in any picture; they were found with `EnumWindows` and answered by keyboard). Pictures were looked at by the agent; nothing is "verified" by a person.
- **Companion reports:** `L1-windows-0.3.4-main-a1bf57dc-regression-full-…`, `…-regression-hardware-…`, `…-regression-soak5min-…` (cm-regress on the same files).

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| cm-regress full tier | PASS | 65 passed, 0 failed, 15 skipped (the 11 `linux.*`/`installer.dmg`/`desktop.recording_banner` rows are not implemented for Windows; `ai.*` need models; `installer.silent` needs `--installer`; `desktop.recording_placement` needs a second monitor) |
| cm-regress hardware tier (`--allow screen,sound,mic,camera`) | PASS | 68 passed, 0 failed, 20 skipped. Against `L1-windows-0.3.4-rc1-regression-hardware` (69 passed, 19 skipped) the only difference is `installer.silent`: no installer was built here. `guide.real_clicks` stays SKIPPED (headless daemon has no confirmation UI; run by hand below). |
| `rec.soak`, 5 minutes (`--soak-minutes 5 --allow screen,mic,sound`) | PASS | 302.8 s, constant 30 fps, 2 s key frames. This case was skipped in every earlier hardware report; it can run (a 3-hour soak was not run) |
| Click Guide, window target, real clicks (GDE-1.AC1) | PASS | `guide start --window "QA buttons"` asks on screen; answered with Tab, Enter on the topmost app window; three SendInput clicks on three buttons give three steps titled `Click "Open settings"`, `Click "Save profile"`, `Click "Send invoice"`; `guide stop` keeps them |
| Click Guide ring and badge (GDE-1.AC1, design) | PASS (looked at) | in the exported images the ring is centred on the click, hollow with a soft glow (label still readable, slightly tinted inside), the numbered badge sits on the ring's upper-left; PDF page 1 (A4, two steps a page) shows the same |
| Click Guide export Markdown / HTML / PDF | PASS | `guide.md` with relative `images/step-0N.png` links, three PNGs; one self-contained HTML (294 KB); one PDF (173 KB) |
| Rich text reaches the Windows clipboard (commit a1bf57dc) | PASS | headless daemon: `copied_to_clipboard: false` with the warning "no rich-text clipboard is available in this daemon" (as designed); with `cm-desktop` hosting the daemon: `copied_to_clipboard: true`; a STA read gives formats `UnicodeText, Text, HTML Format, OEMText`, CF_HTML header consistent (`EndHTML` = length 292728), 3 `<img>` tags, plain text `QA guide / 1. Click "Open settings" / ...`. The data was still there after `cm-desktop` was killed |
| Click Guide: clicks on the bar are not steps at 150 % (GDE-1.AC2) | PARTIAL | started from the library button (the bar is at 597,13 727x107 physical px): clicks at (612,34) and (641,34) gave no step while a click on the desktop gave one; the second click hit a bar control and ended the guide, so the rest of the sweep proves nothing. Seven more clicks (619..1106, y 34) in an earlier run also gave no steps. The sweep over the whole bar was not completed |
| Click Guide: keystrokes (NFR-9) | PASS (small) | three key presses while recording added no step |
| Click Guide started by the CLI shows no bar | observation | a CLI-started guide records but the HUD bar is only made by the app's own "New Click Guide" entry (`hud::start`); a toast window appears. Not judged |
| Long Shot Auto, Edge, sticky header page, probe (`long_shot_probe`) | PASS (after a fix, below) | 4 of 4 runs on the fixed engine: 11 frames, 1447x8386, `reached_end`, header 120 px once; OCR of the stitched image reads `ROW 0001` to `ROW 0140` each once, in order, one `Sticky header band`. `long-page.html ?screens=5`: 7 frames, 1447x4946 twice |
| Long Shot Auto through the daemon (`capture screenshot --window --long`) | PASS | confirmation answered on screen; `source: long_shot`, 11 frames, 1447x8386, `end: reached_end` |
| Long Shot Hand scroll by the CLI | PASS | refused for agents: `invalid_input`, `data.reason: needs_user` |
| Long Shot Hand, 12 wheel notches 500 ms apart (the macOS 1bf29fe2 case: overlay scrollbar over the sticky band, header hides) | PASS | 13 frames, `stopped_by_user`, header 120 px; OCR rows 1 to 45 once, in order |
| Long Shot Hand, Down arrow by SendInput, 40 presses 300 ms apart | PASS | 21 frames; OCR rows 1 to 56 once, in order |
| Long Shot Hand, 30 small wheel steps (delta 40) 250 ms apart | FAIL (not fixed) | 3 of 3 runs: 1 frame, `no_overlap`. See "Problems found" 2 |
| Retype (ANN-9) on a window screenshot | PASS | `Total amount due is 1200 dollars` becomes `... 1500 dollars`; OCR of the render reads 1500, not 1200; the box is filled white and the glyphs look like the page |
| Move area (ANN-10) | PASS | the `Status` line moved 144 px down, hole filled; OCR order changes accordingly |
| Blueprint (ANN-11), op and `export-images --blueprint` | PASS | the contact line (an e-mail) became a grey bar and OCR no longer reads it; bulk export of the whole picture replaces every text line by a bar, the capture itself is unchanged |
| Zoom Lens (ANN-8, op `magnify`) | PASS (looked at) | rounded loupe at x3 with connector and `x3` badge; the edge of the loupe shows a thin colour fringe on its right side (not investigated) |
| `suggest_blueprint` with Windows OCR | PASS | 11 boxes with measured fill and bar colour, `recognizer: windows_ocr` |
| Undo through the ops | PASS | four undo calls walk back through the four revisions |
| Boards (BRD-1): pair with captions and title, grid with columns/gap/margin/radius/shadow/background, free with slots | PASS | three new screenshot captures (800x365, 880x480, 852x282); the pair shows `BEFORE`/`AFTER` badges, `Before`/`After` captions and the title; CMYK source left as imported. A free layout needs `--slot '0,0,400,250'` quoted in PowerShell (unquoted it arrives as four words; my script, not the product) |
| Bulk export (BLK-1) | PASS | `--longest 600 --format jpeg --quality 80 --name-pattern qa-{n}`: 400x200, 600x385, 600x385; a second run writes `qa-1 (2).jpg` ... and overwrites nothing |
| CMYK JPEG import (IMP-6.AC10) | PASS | a 4-component JPEG (ImageIO CMYK: cyan patch on the left, red on the right, black bar) imports, row says `converted`, and the PNG export shows cyan, red and black correctly (not inverted) |
| Open with / forwarding a file to the running app | PASS | `cm-desktop.exe <bundle>` forwards to the running instance and brings the library window up (log: "CaptureMann is already running; showing it"). The registered file association was not tried (nothing registered for this isolated run) |
| Library (window of the app): version | PASS | sidebar reads `v0.3.4` |
| Library: filter chips per page | PASS | Audio: Imported, Uploaded only; Recordings: Imported, Uploaded, Published; Screenshots: Imported, Uploaded, Boards; All captures: Imported, Uploaded, Published, Boards, Click Guides; Click Guides page: none (empty state "No Click Guides yet" with the button) |
| Sidebar: no MCP card; Agents & CLI icon blue and page pill `MCP ready` | PASS (looked at) | no card in the sidebar; the Agents & CLI icon is blue on every page; the page header shows `MCP ready`. The not-ready colour was not seen |
| Recording HUD drag (REC-5.AC18) | PASS | `desktop.hud_drag` passed in the full tier |
| Pictures kept | | `verification/QA-0.3.4-shots/windows-library-audio.png`, `-recordings.png`, `-click-guides-empty.png`, `windows-agents-cli.png` (app window, synthetic content only). Pictures with Edge's toolbar (it shows the signed-in profile picture) were not kept |

## Problems found

1. **Long Shot Auto ended after one frame on a sticky-header page (fixed on this branch, c84a36dc).** Two first runs in a row (`reached_end`-less: `no_overlap`, 1 frame, 0.6 s). The dumped frames showed frame 2 equal to frame 1, then a frame 34 px into a 330 px step, then a frame ~870 px down. Cause: Edge's smooth scrolling paused at a tenth of the asked distance for two grabs, which `wait_settled` took as settled; the learned distance clamped to 0.25, the next step was about 3 content heights and the next frame did not overlap. `run_auto_ex` now looks again when a join moved less than half of what was asked and joins the late part on top. Test `cap_6_smooth_scroll_that_pauses_early_is_not_believed` fails on the old engine (NoOverlap) and passes now; `cargo test -p cm-capture --no-default-features --lib long_shot` 28 passed, clippy `-D warnings` clean. On the laptop the failure did not come back in 4 runs after the fix (and not in 5 runs on the old engine after the first two), so the hardware evidence is "not reproduced again", not "proven fixed".
2. **Long Shot Hand scroll with continuous small wheel steps captures nothing (open).** 30 wheel deltas of 40 sent 250 ms apart: Edge animates continuously, the screen is never equal in two polls, and the only settled frames were the first scrolled frame (18 px) and the last one (1200 px away): `no_overlap`, 1 frame, 3 of 3 runs. 500 ms notches and the Down key work. Hand scroll takes a frame only when the content stops; a slow continuous scroll never stops. Possible directions: take a frame when the content moved enough since the last one even if it is still moving, or tell the user to pause. Not changed (a design decision).
3. **A small first scroll is refused (open, minor).** The first pair with an 18 px shift did not join: the overlay scrollbar thumb moves 2 px, so its columns differ on 4 rows, below `VOLATILE_SHARE` (1 % of the rows, at least 3), the edge strips came out 2 and 21 px wide and the cropped join still failed on the thumb's edge rows. Hand scroll skips the refused frame and a later one (49 px) joined offline, so this only matters with problem 2. Diagnosed offline with `long_shot_stitch` on the dumped frames.
4. `capturemann library list` prints guides with size `nullxnull`.
5. The Edge `Restore pages` and sync pop-ups appear in fresh profiles; `--inprivate` avoids them (test setup only).

## Not run

Windows 10, SmartScreen, a second monitor, the NSIS installer (`installer.silent`), speech models and the speaker engine (`ai.*`), a 3-hour soak, Chrome, other scales than 150 %, the editor and board/bulk sheets in the app window (the CLI actions were run), the guide editor's drag reorder, Retake and focus crop, the Click Guide bar's full sweep (problem above), the not-ready colour of the Agents & CLI icon, the registered file association.

## Verdict

PARTIAL
