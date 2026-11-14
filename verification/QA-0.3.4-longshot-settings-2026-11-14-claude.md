# QA 0.3.4 Long Shot, permissions, settings, launcher, cross-cutting report (PARTIAL)

- **Date:** 2026-11-14
- **Who:** Claude (QA agent; the maintainer was away)
- **Machine:** Apple Silicon Mac (M4 Pro), macOS 26, one built-in display (1512x982 pt, 2x). The machine was shared with other agents running their own CaptureMann-dev instances and moving the one pointer and the focus.
- **Build:** signed dev bundle `target/bundle/CaptureMann-dev.app` (main 182ceee7, reports version 0.3.3), run with an isolated library, config dir, socket and `CAPTUREMANN_SECRETS=file`. Nothing rebuilt, no privacy setting touched.
- **Test page:** `tests/fixtures/long-shot/long-page.html` (plus a copy with a scroll logger), served on 127.0.0.1, in a separate Chrome profile started for this test (a second run with `--disable-smooth-scrolling`).
- **Limits of what was seen:** the confirm dialog, toast, launcher, overlay, HUD and side preview cannot be photographed (`screencapture -l` answers "could not create image from window"; they are hidden from screen capture, apparently on purpose). So none of the S1/S2/S2b/launcher visual comparisons were possible. Those items are BLOCKED, not passed. The confirm dialog was answered blind with Tab and Return (default focus is Cancel, so Tab moves to Allow).

## Results

| # | Item | Result | What was seen |
| --- | --- | --- | --- |
| 1a | Long Shot, Auto scroll, Chrome, AC1 (no missing or duplicate rows) | **FAIL** | See defects D1 and D2. With a 5-screen page: first runs scrolled nothing (`frames: 1`, `end: reached_end`, no scroll event reached the page). After the fix of the timing by `--delay 4`, the page does scroll (one step of about 367 to 420 px) but every run ends with `end: no_overlap`, note "Stopped: the next part did not line up", `frames: 1`. Tried: window target, region target, region that does not contain the pointer, smooth scrolling on and off. Never more than 1 frame, so the page was never stitched. |
| 1b | AC2 header and footer once (`?bands=1`) | BLOCKED | Not run: base case (1a) does not stitch. |
| 1c | AC3 jump stops with the note (`?jump=1`) | BLOCKED | Not run for the same reason. The note text itself ("Stopped: the next part did not line up") and the `no_overlap` ending do appear, saved image kept, in the library. |
| 1d | AC4 limit | PARTIAL | `--max-px 20000` is refused: `invalid_input`, "max_px: must be an integer from 1 to 16384". The 2000 px / `limit_reached` run was not possible (1a). |
| 1e | AC6 Accessibility card / Hand scroll without Accessibility | BLOCKED | Accessibility is granted to the dev bundle; I may not change it. Not seen. |
| 1f | AC7 agent rules | PASS | `capture screenshot --long --hand --window <id>` returns `invalid_input`, message "Hand scroll needs a person to scroll...", `data.reason: "needs_user"`, exit 2. `--hand` without `--long` is rejected by the CLI. |
| 1g | Manifest `source: "long_shot"` | PASS | The result JSON has `"source": "long_shot"` and `long_shot {frames, end, note, header_px, footer_px, direction}`; the library item exists for every ending (also `no_overlap`). The `manifest.json` `frames` field was not checked by eye. |
| 1h | Confirm tier appearance for `--long` Auto | PARTIAL | A floating panel (about 520x365 to 387 pt, layer 101, centre of the screen) opens in my app and the CLI waits (65 s in the first run, so the 5 min timeout works). After approval a toast window (440x96 bottom right, "Agent") shows for 7 s. Look of both not photographable. Tool policy shows `capture_screenshot` as Allow (default) but the Long Shot call still asked: right in effect, but the Settings row cannot express "Long Shot always asks" (see D6). |
| 1i | Overlay Long Shot mode (`cm-overlay long --auto-scroll available`) | PARTIAL | Starts, covers the display (layer 1000, whole display). Hover plus click on my Chrome window printed `{"picked":{"mode":"long","display_id":1,"window_id":<id>,"long":{"scroll":"auto","direction":"vertical","area":[200,160,2000,1400],"scale":2.0}}}`. So pick and output work. Bar, hint chips, Esc, HUD, side preview, Accessibility card, success toast: not seen (BLOCKED, cloaked from capture). Hand scroll run from the app: not run (needs clicking the unseen bar). |
| 1j | HUD absent from stitched frames | BLOCKED | The HUD never opened (no Hand run). The confirm dialog and toast are absent from window-target frames; but the macOS system prompt (D3) landed in the frames of region captures. |
| 1k | Esc / Stop | BLOCKED | Not reachable without the HUD. |
| 2a | Permissions page: Accessibility row | PASS | New fifth row "Accessibility", Optional, "Granted", text "Lets Long Shot scroll for you. Without it, scroll by hand and Long Shot still works.", "Needed for: Long Shot: Auto scroll". Summary reads "5 of 5 granted. Everything CaptureMann can use is allowed." with five segments. Seen only in the granted state; the not-granted look, the first-run setup screen with Accessibility, and "N of M" with a missing one: not seen (BLOCKED). Minor: the Accessibility icon is the same keyboard icon as Input Monitoring. |
| 2b | Long Shot hotkey row | PASS (partly) | Settings > Shortcuts has "Long Shot" under Capture with "Not set" and a **Set** button (other rows show keys and Change). Clicking Set and typing Option+Shift+1 (the Capture region keys) stored the row as "⌥ 1": the Shift key was dropped in the display (D5, maybe my synthetic key event). No conflict message appeared for this combination (it was not identical to Capture region), so conflicts were not tested. |
| 2c | Agents & CLI tool rows | PASS (static + 5 rows seen) | Rows seen: "List displays and windows", "Take screenshots", "Annotate captures", "Read a capture", "Suggest redactions", "Read the text in a screenshot", "Suggest placeholder bars for text" (new tool). A scripted comparison of the 89 tool names from `tools/list` against the titles table in `apps/desktop/src/ui/settings/model.rs` found none missing (so no "Other action"). The inner list scrolls; rows 8 to 89 were not looked at. |
| 2d | Tool policy `capture_screenshot` | PARTIAL | Allow / Ask first toggle shown for it (and for `annotate`), read-only tools locked. Setting is app-only (`settings set` cannot reach `agents.*`): not changed. |
| 3 | Launcher: Long Shot, New Click Guide, tray menu | BLOCKED (code read only) | Launcher window opens (`--demo-launcher`, 760x786) but is cloaked and closes itself after about 3 s. From `launcher/commands.rs`: "Long Shot" (Capture group, hint "Scrolling capture", keywords scroll/page/long, shortcut from the Long Shot action, disabled only when capture is unavailable) and "New Click Guide" (disabled with `guide_off` reason). Menu bar item not looked at. The Library page has a "New Click Guide" button and the "Click Guides 0" and "Boards 0" filter chips (seen). The sidebar quick-capture box still has only Region, Window, Display (no Long Shot button). `cm-desktop --capture` accepts only region, window, display. |
| 4a | CMYK JPEG import | PASS | Made with `sips` (4 components, JFIF, 256x128). `capturemann import cmyk.jpg src.png --wait` gave two image captures; the CMYK one was "prepared: converted". Pixels compared to a ColorSync conversion of the same file to sRGB: mean absolute difference 0.63, max 3 (of 255), so the conversion is ICC-aware and not a naive inversion. The saturated patches look desaturated against the RGB original (for example red 255,0,0 becomes 221,76,49), which is what a CMYK round trip does. |
| 4b | Open With in Info.plist | PASS | `CFBundleDocumentTypes` has "Picture" with extensions png, jpg, jpeg, role Viewer, rank Alternate (besides video and audio). Registration by Launch Services not tested. |
| 5 | MCP | PASS | `cm-mcp` over stdio: `initialize` ok (server 0.3.3), `tools/list` returns 89 tools in one page, every `inputSchema` is `type: object`, every tool has a description. `capture_screenshot` has `mode: "long"`, `direction`, `max_px` (1 to 16384), `scroll: auto|hand`; `list_library` accepts `kind: "guide"`; `start_guide` has a schema. |
| 6 | Pages in dark and light | PASS with notes | Photographed (own window only): screenshots, recordings, audio, guides, library (All captures), published, destinations, agents, ai, settings, permissions (opens the library when all is granted), settings-permissions. Looked at by eye: screenshots dark and light, agents dark, shortcuts dark, permissions dark, library dark. No layout break, clipped text or contrast problem seen in these. The others were photographed and not looked at (not enough time): audio, guides, published, destinations, ai, settings, recordings (6 pages x 2 themes). Notes: the window title bar stays dark in the light theme (screenshots light); the "SHOT" chip sits over the first line of text on a thumbnail (cosmetic). |

## Defects, by severity

**D1 (high). Auto scroll can report success with one frame and nothing scrolled.** Repro: Chrome window with the test page at the top, pointer over it, `capturemann capture screenshot --long --window <id> --json`, approve the dialog with the pointer where the dialog was (centre of the screen). Result: `frames: 1`, `end: reached_end`, `note: null`, image saved as if the page were one screen long. Four runs did this; the page logger saw no scroll event. When the same run is made with `--delay 4` the scroll arrives (events at 367 to 420). Likely cause (not proven): on macOS the scroller posts wheel events at the pointer's place and does not move the pointer (`MacScroller`, anchor ignored); right after Allow the pointer is over the confirm panel (and the panel may still be closing), so the events go to the panel, the second frame equals the first and the engine ends with `reached_end`. A real user clicks Allow with the pointer over the dialog, so this is the normal path. Suggest: move the pointer to the area centre (as the Windows and X11 scrollers do) and wait for the dialog to close; and do not call a run with one frame "reached_end".

**D2 (high). Auto scroll on a plain 5-screen page never stitches.** Repro: as above with `--delay 4`; Chrome with smooth scrolling on or off, window or region target: `end: no_overlap`, "Stopped: the next part did not line up", `frames: 1`. AC1 fails in this setup, so AC2 to AC4 could not be tried. Unknown whether the cause is the settle time, the step (first step asks 50 % of the area; Chrome moved about 370 to 420 pt for 613 to 700 asked), the pixel-unit scale at 2x, or the frames. The run only keeps the first frame; I could not look at the second one.

**D3 (medium, environment or product, unclear). macOS "requesting to bypass the system private window picker" prompt** for CaptureMann-dev appeared (UserNotificationCenter window, 260x383) during these runs and stayed open; I did not click it (it would change a privacy grant). It was captured into the frames of region-mode runs (`qa2R2`, `qa2R3`) because it sits over the area, and it can break a stitch. Whether the app's capture path triggered it, or another agent's instance, is not known. The maintainer should look at the screen.

**D4 (low). Visual comparison impossible by tooling:** overlay, HUD, preview, confirm dialog, toast and launcher are hidden from screen capture, so even a maintainer-approved QA cannot photograph them. Consider a debug switch (for example an environment variable that turns the cloak off) so boards S1, S2, S2b and the launcher can be checked.

**D5 (low, maybe my input). Shortcut recorder dropped Shift:** Long Shot row after Set and Option+Shift+1 (typed by System Events keystroke) shows "⌥ 1". Repro by hand to see if a real Option+Shift+1 keeps Shift.

**D6 (low). Policy display:** Settings > Agents & CLI shows `capture_screenshot` as Allow, yet the call with `--long` (Auto scroll) always asks. The behavior is right (AC7 and the Confirm tier), but the row gives no hint.

**D7 (cosmetic).** Accessibility row and Input Monitoring row use the same keyboard icon; the Long Shot hotkey row uses the Capture region icon.

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| CAP-6.AC1 | FAIL | D1, D2 |
| CAP-6.AC2, AC3 | not run | blocked by AC1 |
| CAP-6.AC4 | PARTIAL | `max_px` range validated only |
| CAP-6.AC5 | not run | macOS only here |
| CAP-6.AC6 | not run | Accessibility is granted, may not be changed |
| CAP-6.AC7 | PASS | step 1f |

## Not tested and why

- Hand scroll end to end, overlay bar/HUD/side preview/toasts, Esc and Stop, the HUD placement rules, Pause: need clicking and looking at surfaces that cannot be photographed.
- Accessibility not granted case (card, "Use Hand scroll", first-run setup count): would need revoking a grant.
- Launcher and menu-bar item look and enable state; the Long Shot hotkey firing; hotkey conflict message.
- Horizontal mode, `?wide`, `?self`, `?bands`, `?jump`, `?rows`, Excel, Slack, a PDF viewer.
- OCR (`capturemann text`) of a stitched image: only run on the single-frame result (rows 1 to 14 read correctly).
- The remaining visual pages (audio, guides, published, destinations, ai, settings, recordings) were photographed but not looked at.

## Cleanup

My app instances, the Chrome profile, the web server and the overlay were stopped; the isolated library, config, screenshots and helper binaries were deleted. The maintainer's running app (pid 8171) was not touched. The macOS prompt (D3) is still on screen. Helper scripts `qa2_*` are left uncommitted in the worktree.
