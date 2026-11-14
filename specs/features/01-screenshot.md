# Screenshot (CAP)

### CAP-1 Capture full display, window or region
**Priority** P0 · **Status** In progress (built on macOS: display, window and region capture, the frozen-frame region flow and the overlay picker; S1 and S2 passed on hardware, `tests/manual/CAP-1*.md` have no recorded run; "Include shadow" is not built; Windows capture and picker not built) · **Crates** cm-capture, apps/overlay, cm-library · **Depends on** NFR-1, DESIGN §5.1 · **Spike** S1, S2 (passed on macOS)

**User story** — As a developer, I want to capture my whole screen, one window, or a region I drag, so that I get exactly the pixels I need without cropping later.

**Description**
- Three modes: **Display** (one monitor), **Window** (one top-level window, without its shadow by default), **Region** (rectangle dragged on a frozen screenshot of one display).
- Region mode freezes the display first (the daemon's internal `freeze_display` returns the pixels without creating a capture; the overlay then shows the still image full-screen), and the saved region is cropped from that same frozen frame via `capture_screenshot { target: { frame_id, rect } }`, so the crop matches what the user saw. The overlay covers the whole display including the macOS menu bar and notch strip (borderless window at screen-saver level, not native fullscreen — finding from S2).
- Window pixels map 1:1 to capture pixels at the window's offset on the display; nothing is scaled. Window and region rects are relative to the window's own `display_id` (not to the global desktop origin), so mixed-DPI setups map correctly; a client matches a window to a display by `display_id`, never by rectangle intersection.
- In Region mode, hovering a window highlights it; a single click (no drag) captures that window.
- Output is stored at physical resolution (e.g. 3024×1964 on a 2× display) as PNG in a `.cmshot` bundle (`original.png`, `manifest.json`), then CAP-4 runs.
- The app's own windows (toasts, the confirmation dialog, the launcher, the overlay) are excluded from screenshots on macOS 14+ by a ScreenCaptureKit single-frame capture with the app's windows excluded; on macOS 13 the capture falls back to `xcap` and own windows can appear. The library window (Studio, with the editor and the setup screens) is the exception: it does not set `sharingType = none` and the host passes its window id as a kept window (REC-6.AC8), so a screenshot may include it while it is visible; the toast and the other floating windows never appear. Because the frozen picker frame shows the library window, `list_targets` lists it as a window target while it is visible (Oct 27, 2026; before, hovering it highlighted the window behind it and a click captured that window instead); the other own windows stay out of the list.

**Acceptance criteria**
- **AC1** Given a 2× display of 3024×1964 physical px, when the user captures the display, then `original.png` is 3024×1964.
- **AC2** Given Region mode, when the user drags a 420×649 px rectangle at (2207, 669), then the saved image is 420×649 and pixel-identical to that area of the frozen frame (golden test with a known pattern).
- **AC3** Given a notched MacBook, when Region mode opens, then the overlay covers the full display including the menu bar (window size equals display size, offset (0, 0)).
- **AC4** Given Window mode, when the user clicks a window, then the image contains only that window's content at physical resolution; a "Include shadow" setting (default off) adds the system shadow on macOS.
- **AC5** Given Region mode, when the pointer hovers a window and the user clicks without dragging (movement < 4 px), then that window is captured.
- **AC6** Given the overlay is open, when the user presses Esc, then the overlay closes and nothing is saved.
- **AC7** Given multiple displays, when Region mode opens, then each display gets its own frozen overlay and a selection can't cross displays.

**Edge cases and errors**
- Screen Recording permission missing (macOS) → overlay doesn't open; a dialog explains the permission with an "Open System Settings" button; MCP returns `permission_missing` (`screen`).
- Protected content (DRM video) captures as black; no error, by OS design.
- Selection smaller than 2×2 px → treated as a click (AC5) or cancelled.
- Display unplugged while the overlay is open → close that display's overlay, keep the others.
- Minimized or off-screen window in Window mode → `invalid_input` "Window is minimized".

**Agent interface**
- MCP `capture_screenshot` `{ target: { display_id } | { window_id } | { display_id, rect: [x,y,w,h] }, delay_s? }` → `{ capture_id, path, width, height, image }`. Tier: Free. Region by agent uses the given rect; no overlay is shown.
- MCP `list_targets` → displays and windows with ids, titles, app, bounds (physical px), scale.
- CLI `capturemann capture screenshot --display 1 | --window "<title substring>" | --region x,y,w,h [--out file.png]`.
- Events: `capture.created`.

**Out of scope** — (Scrolling capture is CAP-6 below, 0.3.4), capturing across displays in one image, capturing menus that close on focus change (P2 note).

**Test notes** — Golden pattern display (checkerboard with coordinates) for AC2; manual script for AC3 on a notched Mac and AC7 with two monitors; Windows at 100% and 150% scaling.

---

### CAP-2 Global hotkeys and tray icon
**Priority** P0 · **Status** In progress (built on macOS: hotkeys, the rebinding UI in Settings → Shortcuts, tray with recent captures and the recording variant, Launch at login as a LaunchAgent; the manual registration-failure and login scripts have not been run; "Replace system screenshot shortcuts" is not built; Windows compile-only) · **Crates** apps/desktop, apps/overlay (`global-hotkey`, `tray-icon`) · **Depends on** CAP-1, REC-1

**User story** — As any user, I want keyboard shortcuts and a menu-bar/tray icon, so that capture is one keystroke away without opening the app.

**Description**
- Default shortcuts (all rebindable in Settings → Shortcuts):

| Action | macOS | Windows |
| --- | --- | --- |
| Capture region | ⌥⇧1 | Ctrl+Alt+Shift+1 |
| Capture window | ⌥⇧2 | Ctrl+Alt+Shift+2 |
| Capture display | ⌥⇧3 | Ctrl+Alt+Shift+3 |
| Start/stop recording | ⌥⇧R | Ctrl+Alt+Shift+R |
| Pause/resume recording | ⌥⇧P | Ctrl+Alt+Shift+P |
| Open library | ⌥⇧L | Ctrl+Alt+Shift+L |
| Open launcher (LNC-1) | ⌥⇧Space | Ctrl+Alt+Shift+Space |
| Discard recording (HUD, REC-5) | ⌥⇧D | Ctrl+Alt+Shift+D |
| Restart recording (HUD, REC-5) | ⌥⇧T | Ctrl+Alt+Shift+T |
| Annotate last capture | ⌥⇧A | Ctrl+Alt+Shift+A |
| Start/stop audio recording (VOC-1; built Oct 26, 2026, tested with fakes; the maintainer used audio recording on Oct 27, 2026, as reported, no stored report) | ⌥⇧V | Ctrl+Alt+Shift+V |
| Add a marker while recording (VOC-1; built Oct 26, 2026, tested with fakes; the maintainer used audio recording on Oct 27, 2026, as reported, no stored report) | ⌥⇧M | Ctrl+Alt+Shift+M |

- Defaults avoid the OS screenshot keys (⌘⇧3/4/5 on macOS, Win+Shift+S on Windows), common editor shortcuts (⌘⇧P, Ctrl+Shift+P), and plain Ctrl+Alt on Windows (it equals AltGr on many European layouts). Settings offers "Replace system screenshot shortcuts" on macOS as an opt-in that explains how to turn off the system ones first.

- Tray/menu-bar menu (native menu, not custom-drawn): Capture region, Capture window, Capture display, Record…, Record audio (VOC-1), Open library, Recent captures (last 5), Settings, Quit. While recording, the icon switches to the kit's recording variant (see AC7), with elapsed time as the title text on macOS (tooltip on Windows) and Stop recording becomes the first item.
- The app keeps running in the tray when its windows are closed. **Launch at login** is a LaunchAgent (`~/Library/LaunchAgents/dev.capturemann.app.plist`) installed only for an installed `.app` bundle, never for a dev binary. **Default off**: a login item is persistence on the user's machine, so it needs an explicit opt-in. This is a deliberate deviation from the original "default on" (decided with the AC5 implementation; the product maintainer accepted it on Oct 21, 2026 (OD-13): it ships default off and stays off, to be revisited with an installer). The choice is a `launch_at_login` file beside `settings.json`, not a setting, so an agent cannot change it through `set_settings`.

**Acceptance criteria**
- **AC1** Given CaptureMann is running with no window open, when the user presses the region shortcut, then the region overlay appears within NFR-1 (≤ 150 ms).
- **AC2** Given the user rebinds a shortcut to a combination already used by CaptureMann, then the editor shows "Already used by `<action>`" and refuses to save. While the page waits for the new combination, every global shortcut is released (the OS gives a registered combination to its hot key, never to the window, so without that the press would start the other action and no message could show) and registered again when recording ends (a key is taken, Esc, Cancel, focus leaves, Settings closes). Shift plus a digit is recorded as the digit with Shift (the window systems report it as `!` with Shift cleared). Tested with fakes (`shortcuts.rs`, `platform.rs`, `ui/settings/reset.rs`); manual script `tests/manual/SHORTCUTS-1.md` (not yet run).
- **AC3** Given the OS or another app already owns a combination, when registration fails, then Settings shows "Couldn't register — used by another app" next to that shortcut and the other shortcuts still work.
- **AC4** Given recording is active, then the tray icon shows the recording state and its first menu item stops the recording.
- **AC5** (deliberate deviation: default off, see Description) Given "Launch at login" is on, when the user logs in, then CaptureMann starts in the tray with no window.
- **AC6** Given a release build, then the macOS bundle uses `design/brand/app-icon/AppIcon.icns`, the macOS menu bar item uses `design/brand/tray/png/TrayTemplate.png` (+ `@2x`, `@3x`) as a template image, and the Windows executable and installer use `design/brand/app-icon/app.ico` (DESIGN §8).
- **AC7** Given recording is active, then macOS shows the non-template `tray-recording-on-*.png` (the red stays visible in light and dark menu bars) and Windows shows `tray-recording-on-dark.ico` or `-light.ico` matching the taskbar theme; the idle icon is `tray-dark.ico` or `tray-light.ico`, switching with the OS theme.

**Edge cases and errors** — Shortcut pressed while the overlay is already open → ignored. Secure input fields (password prompts) block global shortcuts on macOS → no error, documented.

**Agent interface** — Not applicable (human input). `get_settings` can read `shortcuts.*`; `set_settings` refuses them from socket clients (app-only keys, see the settings-key registry in the Technical design tab).

**Out of scope** — Per-app shortcut profiles; mouse-button shortcuts.

**Test notes** — Unit tests for the conflict checker; manual script for registration failures and login launch on both OSes.

---

### CAP-3 Magnifier and coordinates in the region picker
**Priority** P1 · **Status** In progress (loupe, coordinates, size pill and nudging built in `apps/overlay` and unit tested; `tests/manual/CAP-1-overlay.md` has no recorded run; Windows not built) · **Crates** apps/overlay · **Depends on** CAP-1, DESIGN §5.1

**User story** — As a developer, I want a magnifier with pixel coordinates while selecting, so that I can align the selection exactly to UI edges.

**Description**
- 128 px circular loupe follows the pointer at 8× zoom with a 1 px pixel grid; the center pixel is outlined.
- Under the loupe: physical-pixel coordinates `x, y` and the center pixel's hex color.
- During a drag, a size pill shows `W × H` (physical px) above the selection; arrow keys nudge the pointer or active edge by 1 px, Shift+arrow by 10 px.
- Press C to copy the hex color of the center pixel to the clipboard (stays in the picker).
- The loupe flips to the other side of the pointer near display edges.

**Acceptance criteria**
- **AC1** Given the picker is open, when the pointer is at physical (100, 200), then the readout shows `100, 200` and the loupe's center pixel equals that pixel of the frozen frame.
- **AC2** Given a drag is in progress, when the user presses → five times, then the selection's right edge moves exactly 5 physical px.
- **AC3** Given the pointer is within 160 px of the right edge, then the loupe renders on the left side of the pointer and stays fully on screen.
- **AC4** Given the user presses C, then the clipboard contains the hex color in `#RRGGBB` form.

**Edge cases and errors** — Loupe rendering must not slow the drag below 60 fps on a 4K display; drop the pixel grid first if needed.

**Agent interface** — None (interactive aid).

**Out of scope** — Color picker history, measuring between two points.

**Test notes** — Unit tests for coordinate mapping and edge flipping; manual check on 1× and 2× displays.

---

### CAP-4 Clipboard and default destination after capture
**Priority** P0 · **Status** In progress (built on macOS and tested with fakes, not seen by eye: copy to clipboard (PNG, and a TIFF flavor on macOS) after capture and after each annotation save (a burst of edits copies once), a Copy button in the editor, save to library, the toast with Annotate, Copy, Save as… and Delete; "Also save a PNG to a folder" in Settings → General (AC2 to AC4: `apps/desktop/src/save_folder.rs`, the warning toast, never-overwrite, atomic write); the Windows clipboard retry (3 tries over 300 ms) is in the shared writer and tested with a fake clipboard, nothing ran on Windows; `tests/manual/CAP-4.md` has no recorded run; the library card's Rename and Delete use the daemon's `rename_capture` and `delete_capture`, which moves to the Trash; hands-on checks in `tests/manual/LIB-1.md`) · **Crates** cm-library, cm-storage, apps/desktop (`arboard`) · **Depends on** CAP-1, STO-1

**User story** — As a developer, I want the screenshot copied and saved automatically, so that I can paste it into a ticket immediately and still find it later.

**Description**
- After every capture (and after each annotation save), CaptureMann runs the "After capture" actions set in Settings → General:
  - Copy image to clipboard (default on).
  - Save to library (always).
  - Also save a PNG to a folder (default off; folder chosen by the user, file name pattern default `CaptureMann {yyyy-MM-dd} at {HH.mm.ss}.png`).
  - Open the editor (default off) or show a toast with Annotate / Copy / Save as / Delete (default on).
- *As built (maintainer decided Oct 22, 2026: follow the spec):* the copy runs after each annotation save and from the Copy button. Saves queued back to back share one copy.
- *As built:* the folder copy follows the same cadence as the clipboard copy: once after each capture, and once per burst of annotation saves (each burst writes a new file named by the time; earlier files are never replaced). The toast's Copy button does not write a file. A folder that is missing (an unplugged drive) is never created: the capture stays in the library and a warning toast says "Couldn't save to `<folder>`: drive not available" (or "permission denied", "disk full"). The three `capture.save_*` settings are app-only.
- Clipboard content is the rendered image (with annotations) as PNG; on macOS also as TIFF for older apps.

**Acceptance criteria**
- **AC1** Given defaults, when a capture finishes, then within 300 ms the clipboard holds the image and a toast offers Annotate, Copy, Save as, Delete.
- **AC2** Given "Also save to folder" points to `~/Pictures/Shots`, when a capture finishes, then a PNG with the pattern name exists there.
- **AC3** Given a file with the same name exists, then the new file gets ` (2)`, ` (3)`… appended; nothing is overwritten.
- **AC4** Given the folder is on an unplugged drive, then the capture is still saved to the library and a warning toast says "Couldn't save to `<folder>`: drive not available".

**Edge cases and errors** — Clipboard locked by another app (Windows) → retry 3× over 300 ms, then toast "Copied to library only".

**Agent interface** — `capture_screenshot` returns the image directly to the agent; clipboard copy is skipped unless `copy_to_clipboard: true` is passed. `annotate` also takes `copy_to_clipboard` and copies the **rendered** image (annotations and redactions applied), never the original; the CLI sends `--copy` with `annotate` when `--annotate` is present, so a redacted screenshot is never copied unredacted. CLI `--copy`, `--out`.

**Out of scope** — Uploading to cloud destinations for screenshots automatically (STO-3 covers recordings; screenshots can be uploaded manually or by agent `upload`).

**Test notes** — Integration test with a temp folder; manual clipboard paste into a browser, Slack and Word on both OSes.

---

### CAP-5 Delayed capture
**Priority** P1 · **Status** In progress (daemon `delay_s` and CLI `--delay` built and tested with fakes; the tray "Capture delay" submenu, the delay choice in Settings → Capture & recording ("Before a screenshot"; it left the library header with the redesign), the corner pill and its Esc path are built in `apps/desktop` and unit tested but not seen by eye; `tests/manual/CAP-5.md` has no recorded run; Windows not built) · **Crates** apps/desktop, cm-daemon · **Depends on** CAP-1

**User story** — As a support engineer, I want a 3, 5 or 10 second delay before capture, so that I can open a menu or hover state before the screen freezes.

**Description** — A delay option in the tray menu ("Capture delay") and in Settings → Capture & recording ("Before a screenshot"). Both show one choice, No delay / 3 / 5 / 10 seconds, remembered across restarts (`capture.delay_s`); it applies to every capture start (hotkey, tray, toolbar, launcher). During the delay a small countdown pill (DESIGN §5.3 styling, 48 px) shows in a screen corner and is excluded from the capture. When it ends, the chosen mode starts (Region freezes the screen at that moment).

**Acceptance criteria**
- **AC1** Given a 5 s delay in Display mode, when the user starts it, then the image is taken 5.0 s ± 100 ms later and the countdown pill is not in the image.
- **AC2** Given a delay is running, when the user presses Esc, then nothing is captured. (Built: the delayed capture is cancelled when the requesting connection drops, and `Daemon::cancel_delayed_capture(request_id)` exists. The pill calls it for Display delays, and stops its own timer for Region and Window delays. The pill never takes focus (edge case below), so Esc is registered as a global hotkey only while a delay runs (maintainer decided Oct 22, 2026) and released when it ends or is cancelled. If another app owns Esc, the registration fails and Esc works after a click on the pill.)
- **AC3** Given Region mode with delay, then the freeze happens when the countdown ends, and the menus open at that moment are in the frozen image.

**Edge cases and errors** — A menu that closes when the countdown pill appears: the pill must not take focus.

**Agent interface** — `capture_screenshot { delay_s: 0..30 }`; CLI `--delay 5`.

**Out of scope** — Repeating timed captures (interval capture).

**Test notes** — Timing test with a monotonic clock; manual test with an open context menu on both OSes.

---

### CAP-6 Long Shot (scrolling capture)
**Priority** P1 · **Status** In progress (0.3.4; was Draft, P2). Built and tested with fakes, not seen on a screen or run against a real window: the stitching engine and both scroll loops (AC1 to AC5 on synthetic pages), the daemon action with its tiers (AC6 daemon side, AC7), Windows `SendInput` and X11 XTest scrollers, the Accessibility row, the overlay's Long Shot mode (mode bar with Auto and Hand scroll, direction, Start, the Accessibility card of AC6, hover highlight of the window, region drag), the HUD and the side preview (placement rules S2b in `cm-core::long_shot_hud`), the launcher command, the menu item, the Settings hotkey row ("Not set", no default) and `source: "long_shot"` with the frame count in the stored manifest. Not built: Esc as a global key during the run (the HUD never takes the focus, so Stop is the HUD button), the direction switch in the HUD (the direction is chosen before Start and shown), the dashed header and footer lines in the preview (they are tags), and a scrollable-area highlight finer than the window (the area is the window; the stitcher keeps fixed bands once). Lab: `verification/L1-windows-0.3.4-long-shot-scroll-2026-11-14-claude.md` (engine and scroller on Windows); the overlay, HUD and preview have no report yet · **Crates** cm-capture, cm-render, cm-daemon, apps/desktop · **Depends on** CAP-1, CAP-4, PRM

**User story.** As someone who documents or reports bugs, I want a whole web page, spreadsheet or chat thread as one image, so that I don't paste five screenshots in a row.

**Description**
- **Start:** a **Long Shot** mode in the capture overlay, a launcher command, and a configurable hotkey with no default. The user picks a window, then the scrollable area inside it (CaptureMann highlights the area under the pointer). A region drag works too.
- **Auto scroll** (macOS with Accessibility, Windows, X11):
  - CaptureMann scrolls by about 80 % of the area height each time, and waits until two frames 50 ms apart are the same (to a 150 ms limit).
  - It stops at the end (no change after a scroll), at the limit (the limits above), or when the user presses Esc or Stop. Stopping early keeps what has been captured.
- **Hand scroll** (every system; the only mode on Wayland): the user scrolls. CaptureMann takes a frame whenever the content settles and shows the image growing in a side preview.
  - A frame that does not line up with the last one taken does not end Hand scroll: it is left out and the next frame that joins is taken (the user may scroll back, or less). The capture ends when the user stops it, at the end or at the limit; when it is stopped while the last frame was left out, it says "Stopped: the next part did not line up" and keeps the image so far.
- **Limits (0.3.4):** the stitched image stops at **16,384 px on its long side and 50 megapixels**, the same limits as an imported picture and the renderer; the old 30,000 px goal waits for tiled rendering. The capture ends cleanly with "Reached the longest image CaptureMann can edit" and the image is saved.
- **Direction:** vertical by default. Horizontal is a switch in the HUD (and the `direction` parameter); auto scroll uses horizontal wheel events.
- **Stitching:** the rules below. The result is a normal screenshot capture that can be annotated, redacted, exported and shared like any other. Its manifest records `source: "long_shot"`, the frame count and the stops found.
- **Retina and HiDPI:** frames stay in physical pixels. Mixed-DPI displays are not a case, because the area belongs to one window.

**Stitching rules (decisions of Nov 14, 2026).** Frames are matched by their overlapping rows (an exact or near-exact row hash, then a check on the pixels). Fixed headers and footers are found as bands that stay the same in every frame and are kept once. A band that goes away (a sticky header that hides while the page scrolls down) shrinks the band and the rows it held become content; moving edges (a scrollbar, including an overlay scrollbar that fades in over the bands) are cut from every frame. When no overlap is found (a jump, an animation, a virtualized list that redraws), Auto scroll **stops there and keeps what it has**, with a note; Hand scroll leaves that frame out and waits for one that joins (see above); neither ever guesses a join. Horizontal scrolling uses the same method on columns.

**Systems.** macOS posts scroll events with `CGEventPost`, which needs **Accessibility** (a new optional row on the permissions page, PRM-1; today no feature needs it). Windows uses `SendInput` (no permission), Linux X11 uses XTest. **Wayland cannot inject scrolling**, so Hand scroll is its only mode; whether frames can be taken repeatedly without a prompt each time is spike S6 (when it fails, Long Shot is shown disabled with the reason). A mouse-wheel step differs per app, so the 80 % step and the settle wait are measured, not assumed (spike S1).

**HUD and preview placement (design board S2b, Nov 14, 2026).** The Long Shot HUD (56 px, excluded from the frames) avoids the content being scrolled. Its place is chosen once when the capture starts and does not jump while it runs: above the area, centred, 12 px gap, when at least 64 px of room is above; else below the area; else, when the area fills the display height, inside the area at the bottom centre, 16 px up. The side preview goes on the side of the area with more room (right first); when neither side has 200 px it becomes a **Preview** button on the HUD that opens it as a floating panel. The HUD stays on the display that holds the area and never goes off-screen; it can be dragged at any time (on Windows through a declared drag region, B3) and a dragged place is kept for that Long Shot only. Click Guide uses the same rule for its HUD and step toast, measured against the followed target.

**Keys and names.** The UI name is **Long Shot**; no default hotkey (the user sets one in Settings, CAP-2).

**Acceptance criteria**
- **AC1** A test page 5 screen heights long with a numbered line on every row. After an auto-scroll Long Shot, every number appears exactly once and in order: no duplicated or missing rows (an OCR check plus a row-hash check).
- **AC2** The same page with a fixed header and footer. The stitched image shows each of them once, at the top and the bottom.
- **AC3** A page that jumps (no overlap between two frames). The capture stops at the last good join, keeps the frames before it, and says "Stopped: the next part did not line up". It never guesses a join.
- **AC4** A page longer than the limit. The capture stops at 16,384 px with the limit message, and the image is saved.
- **AC5** Wayland. Hand scroll is the only mode offered, and AC1 passes when the test scrolls by hand (a scripted wheel through the test page's own JavaScript).
- **AC6** macOS without Accessibility. Auto scroll is shown disabled with "Allow Accessibility to scroll for you", a button to grant it, and Hand scroll still works.
- **AC7** (agent) `capture_screenshot { mode: "long", target, direction?, max_px? }` returns the stitched image, like a normal capture. Auto scroll from an agent is **Confirm** tier because it moves the user's screen, and Hand scroll is refused for agents (`invalid_input`, reason `needs_user`).

**Out of scope:** tiled images over 16,384 px; scrolling inside nested frames in one shot; video of the scroll.

**Test notes:** golden stitching tests on recorded frame sequences (no screen needed); a test page in `tests/fixtures/long-shot/`; `tests/manual/CAP-6.md` for Chrome, Excel or LibreOffice Calc, Slack or Discord, and a PDF viewer on each system.
