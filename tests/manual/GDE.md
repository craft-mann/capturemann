# GDE manual script: Click Guide (GDE-1 to GDE-4)

Covers what the tests with fakes cannot: real clicks, the always-on-top bar, the Input Monitoring
permission, the drag in the step list and the round trip to the image editor. The logic is in
`cargo test -p cm-daemon --test guide` (scripted clicks, fake screen), `cargo test -p cm-desktop
--lib guide_view nav launcher` (reorder model, exclusion rectangle, permission gate, entries) and
`cm-regress` (`guide.edit_export`, `guide.refusals`, and on Windows `guide.real_clicks` in the
hardware tier). The OCR check of GDE-3.AC2 for Markdown, HTML and PDF is
`CM_OCR_REAL=1 cargo test -p cm-daemon --test guide_ocr_real -- --ignored --nocapture`.

Use the bundle (`make run-bundle-dev`) or the installed app, never `target/release/cm-desktop`, on
macOS: the permissions belong to the app that runs. Use throw-away data for a first look:

```
export CAPTUREMANN_LIBRARY=$(mktemp -d) CAPTUREMANN_CONFIG_DIR=$(mktemp -d) CAPTUREMANN_SOCKET=/tmp/gde/d.sock
mkdir -p /tmp/gde
```

Privacy: a Click Guide records the position and the button of every mouse click and a screenshot
of the followed target at that moment. It never records keystrokes. Do the clicks on a page with
nothing private on it (a settings page of a test account, the Long Shot test page).

## 1. Entry points (GDE-1)

1. Tray menu: **New Click Guide** is listed under Open launcher. Click it: the bar appears.
   Stop it at once (Stop, nothing recorded: the toast says "No steps were taken").
2. Launcher (the tray's Open launcher, or its hotkey): type "guide". The row **New Click Guide**
   shows "Click through a task, get the steps". Enter starts the bar.
3. Library: the header of **All captures** and of **Click Guides** has a **New Click Guide**
   button. On a computer with no guides, the Click Guides page says "No Click Guides yet" and
   offers the same button.
4. Sidebar: the LIBRARY group has **Click Guides** with a count; the page lists only guides. The
   **Click Guides** chip on All captures shows the same set. A guide's card badge reads GUIDE.
5. Menu bar (macOS): the app menu item **New Click Guide** (as before).

## 2. The Input Monitoring card (macOS, GDE-1.AC5)

Only on a Mac where Input Monitoring is off for the app: System Settings, Privacy and Security,
Input Monitoring, switch CaptureMann off (or use a bundle that never had it).

1. Start a guide from any entry. **No bar appears.** A card opens: "Click Guide needs Input
   Monitoring", the pill "Not granted", the line "It never records what you type", the buttons
   **Grant access...** and **Cancel**.
2. **Cancel** closes the card. Nothing was recorded; the library has no new guide.
3. Start again, press **Grant access...**: the system dialog or the Settings pane opens, and the
   card adds the line about turning CaptureMann on. Switch it on. Within a second the card
   closes by itself and the guide starts (the bar, the countdown).
4. If the grant was made by an earlier ad-hoc build, macOS may keep the old code requirement and
   the pill stays "Not granted": remove the entry with the minus button and add the app again
   (see CLAUDE.md, the signing-identity note).

Pass: no click is listened to before the permission is on; the card never mentions keystrokes
except to say they are not recorded.

## 3. A session with no click events (Linux Wayland, GDE-1.AC6)

1. In the launcher the row **New Click Guide** is dimmed and its text is the reason ("...gives no
   click events..."). Enter does nothing.
2. The Library header button is dimmed; hovering shows the same reason.
3. The tray item is present; clicking it shows a toast "Click Guide is not available here" with
   the reason, and no bar.
4. `capturemann guide start` answers `not_implemented` with the same reason.

X11 sessions are not affected: the bar starts and clicks are recorded.

## 4. Recording a guide with real clicks (GDE-1.AC1 to AC3)

1. Start a guide (tray). A 3 second countdown shows "Starting..." in the bar. Do not click yet.
2. The bar says **Step 1**, "Click Guide - clicks only". Click five different controls on the test
   page. After each click the number goes up. Every step should have its own screenshot.
3. Click **on the bar itself** (its Pause, then Resume): no step is added (GDE-1.AC2).
4. **Pause**, click twice on the page: no step; the bar says "Paused - clicks are not recorded".
   **Resume**, click once: one step.
5. **Note step** adds a step with no image; **Undo step** removes the newest step.
6. **On a Windows laptop with the display at 125 % or 150 %**: click the bar's centre, its left
   end and its right end: none is a step. (The bar's rectangle is given in physical px;
   REC-5.AC18 and this check are the same class of bug.) On a second monitor with another scale,
   repeat with the bar on that monitor.
7. Press **Stop**: the guide editor opens with the steps in order, each with its badge and ring.

Pass: five clicks make five steps in order; clicks on the bar and while paused make none; the
step pictures do not contain the bar.

Not drawn on screen in this build: the 52 px click ring with glow and the step toast under the
bar (design S3). Mark them "not built" when running this script.

## 5. The guide editor (GDE-2)

1. **Drag to reorder:** grab the grip at the left of a step row and drop it on another row. The
   list shows the lifted row following the pointer and a line on the row it lands on. After the
   drop the order and the numbers change, and **Undo** (the page's undo, or Cmd/Ctrl+Z) puts it
   back in one step.
2. **Keyboard fallback:** select a row, use **Move up** and **Move down**: the same result.
3. **Edit image:** select an image step, press **Edit image**. The annotation editor opens on that
   step's picture and its breadcrumb reads "Click Guide / Guide". Draw a callout or a blur, press
   the breadcrumb (or the back shortcut): the guide editor returns on the same step, and the
   picture and the row thumbnail show the change.
4. Add a note step, retitle, delete a step, duplicate a step; each is one undo step, and a deleted
   step's picture comes back with undo.
5. **Retake** replaces one step's frame with a fresh one of the followed target. **Focus crop**
   toggles the guide crop of that step. The crop is ONE rectangle for the whole guide (the box of
   all click points of that frame size, padded by half the default focus size): record three
   clicks in the same part of a window and one far away in the same window; every step shows the
   same part of the window, so the framing does not jump from step to step, and the ring and
   badge sit where each click was. Delete the far step: the other steps' crop tightens again.
   Turn the crop off on one step: only that step shows the whole frame.

## 6. Export (GDE-3)

1. **Markdown:** export, open `guide.md` in a Markdown viewer: the images show, in order, with
   the step titles; open the file in a text editor: every link starts with `images/`.
2. **HTML:** open the file offline: images embedded, a table of contents, a dark style in a dark
   system theme, a clean print preview.
3. **PDF:** A4 and Letter; one or two steps a page; the fonts are Geist; the badge and ring show.
4. **Rich text:** paste into a note app or a ticket form: the steps with their pictures.
5. Put a blur (Edit image, then Blur or Auto-redact) over text in one step and export in each format:
   the blur shows, and a text search over the picture (the OS's Live Text on the Markdown image)
   finds nothing in that region. The automated version is `guide_ocr_real` (see the top).
6. A step that shows an email is not marked and the sheet shows no warning (GDE-1.AC4 is withdrawn
   for 0.3.4): the export runs directly and nothing is blurred unless you did it in step 5.

## 7. Agent tools (GDE-4)

Run `capturemann guide get`, `guide set`, `guide export` on the guide you made, and from an MCP
client call `start_guide`: the on-screen confirmation names the app and says "Clicks only.
Keystrokes are never recorded.", Cancel has the default focus, and declining creates no guide.
The plugin skill `write-a-how-to` follows the same flow.

## Report

Store the run in `verification/` (template in its README) as PARTIAL, naming the system and the
sections run. Do not write "verified" without the report.
