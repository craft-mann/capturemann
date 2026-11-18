# Quick Capture (QC)

**0.3.6, maintainer decision of Nov 17, 2026** (`specs/plans/release-0.3.6-plan.md`; brought into 0.3.5 and moved back to 0.3.6 the same day). UI name: **Quick Capture**. Design pack: `specs/capturemann-0.3.6-quick-capture/` (local, git-ignored; boards Q036Brief, Q036Panel, Q036After, Q036Presets, Q036Scope). This file is the source of truth for the build; where the pack and this file differ, this file wins. **Preview** (label and Send feedback button, no Settings switch).

QC-2 (one smart crosshair) and QC-6 (pinned presets in the dock) are **not** in 0.3.6; the IDs stay reserved.

**Rules that apply to the whole file**
- **Nothing existing changes.** The five dock tiles, the tray items, the Launcher (its hotkey stays ⌥⇧Space on macOS and Ctrl+Alt+Shift+Space on Windows and Linux) and every shortcut (⌥⇧1 to 4, R, M, L) work exactly as before. Quick Capture adds a way in; it does not replace one.
- **Panel options are separate from Settings.** The option tiles are remembered per kind and apply only to captures started from the panel or from a preset. Settings (CAP-4 "After capture", CAP-5 delay, the recording sheet) keep governing captures started any other way, and the two never overwrite each other.
- **Same actions.** The panel, a preset, the CLI and MCP all call the existing actions (`capture_screenshot`, `start_recording`, long shot, audio recording); there is no second capture path. Confirm rules are unchanged (AGT-2).
- **No telemetry, no network.** Presets are a local file. The Send feedback button opens the browser and sends nothing before the click.
- **Language.** All text through `cm_i18n` with Vietnamese entries; brand marks use the theme's text colour (OD-21).

### QC-1 Quick Capture panel
**Priority** P1 · **Status** In progress (built and tested with fakes: the state of the panel, its keys and accessible names, the run each kind makes, the shortcut and its conflicts, the Launcher row, the tray item, the label widths in English and Vietnamese, AC1 to AC8 as far as they do not need a screen; the panel, the dock button and the pickers have not been looked at on a screen or run on real hardware, see `tests/manual/QC-1.md`) · **Crates** apps/desktop, cm-core, cm-daemon · **Depends on** CAP-1, CAP-2, CAP-4, CAP-5, CAP-6, REC-1, VOC-1, LNC-1

**User story.** As someone who captures often, I want one small panel with the kind, the target, the options and one button, so that I do not choose between five tiles and hidden settings every time.

**Description**
- **Layout.** A vertical card 340 to 360 px wide, radius 18, the HUD surface, top to bottom:
  1. Header: the CaptureMann mark, "Quick Capture", the preset picker pill (QC-4) and a settings button (opens Settings > Capture > Presets).
  2. Three kind tabs: Screenshot, Video, Audio, with icons; only the active tab shows its name. The panel opens on the last-used kind.
  3. Target. Screenshot: Region, Window, Display, Long Shot. Video: Region, Window, Display. Audio: the one-line note "Mic and system sound, no screen". Region and Window use the existing pickers (CAP-1); Long Shot uses CAP-6.
  4. A 2×2 grid of 52 px option tiles. Screenshot: **Editor** (open in editor after capture), **Copy** (copy to the clipboard), **Auto-redact**, **Delay**. Video: **Mic**, **System audio**, **Webcam**, **Countdown**. Audio: **Mic**, **System audio**, **Transcribe**, **Markers**. A click toggles a tile; value tiles (Delay, Mic, Countdown) open a small menu; an on tile has a filled icon circle. The choices are remembered per kind.
  5. A 44 px full-width action bar, radius 12, with the shortcut inside it: a neutral **Capture** for Screenshot, rec red **Record** for Video and Audio (red still means recording only). With a preset picked it reads "Capture · {preset}" or "Record · {preset}".
- **Opening.** Four ways: (a) the tray or menu-bar menu gets a first item **Quick Capture…** (the native menu stays, CAP-2); (b) a **Capture…** button in the sidebar dock under Record… (not in the narrow rail), a neutral primary button with its keycap; (c) a **Quick Capture** row in the Launcher, Capture group; (d) the global shortcut **⌥⇧C** on macOS and **Ctrl+Alt+Shift+C** on Windows and Linux, shortcut action `open_quick_capture`, rebindable in Settings > Shortcuts and conflict-checked like the others. From the tray the panel opens under the icon (its top at the top of the display when the pointer is in the upper half, near the pointer otherwise); from the shortcut, the dock or the Launcher it opens near the pointer on the display that has it. The panel is 352 px wide and 400 px high.
- **Keys.** Enter or Space captures or records; 1, 2, 3 switch the kind; Tab and Shift+Tab move through the four tiles (a ring shows the focused one); Up and Down toggle the focused tile or step a value tile through its values; Left and Right change the target; P opens the preset picker; Esc closes (Esc in a value menu, the picker or the name screen goes back one step first). The name field of "Save current as preset…" has the keys while it is open.
- **Value tiles.** Delay offers Off, 3 s, 5 s and 10 s; Countdown offers Off, 3 s, 5 s and 10 s. A click opens the choices in place of the grid; picking one or Esc goes back. Defaults on first use: Screenshot Copy on and the rest off; Video Mic on, Countdown 3 s; Audio Mic on. The choices are written to `quick_capture.json` in the settings folder, never to `settings.json`.
- **What each target does.** Screenshot Region: the capture overlay (a frozen frame, the same picker as Option+Shift+1). Window: the window list (R19). Display: the display that has the pointer. Long Shot: the Long Shot picker (CAP-6). Video Region and Window: the pickers in `--pick` mode, then the recording starts after the Countdown; Video Display: the primary display. The panel's Delay tile is the screenshot delay for that capture only. A recording from the panel starts at once (the Record button is the confirmation) and writes none of the Record sheet's remembered choices.
- **Audio tiles.** Mic and System audio choose the sources. Transcribe starts the transcription of the take when it ends. Markers shows, after the start, the shortcut that adds a marker (Option+Shift+M).
- **Screenshot tiles.** Editor opens the capture in the editor when it is stored. Copy copies the picture. Auto-redact runs the on-device text reader and blurs what it finds in one undo step before the picture is copied, so the clipboard never holds the unblurred picture. If the reader is not available the capture is kept and a notice says so.
- **Window behaviour.** The panel closes as soon as the capture starts, waits 150 ms (`CAPTURE_DELAY_MS`) like the Launcher, and is excluded from screen capture and recording like the HUD. Every control has an accessible name.
- **Preview.** A Preview label in the header (a tooltip, no popover) and a Send feedback button (a `PreviewFeature` entry in `cm-core/src/features.rs`; the feedback form is the one of the About button with the feature name Quick Capture). No Settings switch.
- **Hotkey fails to register.** Settings shows "Couldn't register: used by another app" (as CAP-2.AC3) and the tray item still works.

**Acceptance criteria**
- **AC1** Given the panel is opened, then it shows, in this order, the header, the kind tabs, the target, four option tiles and the action bar, and opens on the kind used last.
- **AC2** Given the Screenshot kind with Region and Copy on, when the user presses Enter, then the region picker opens, the panel is closed, the panel is not in the picture, and the capture is copied.
- **AC3** Given the Video kind, then the action bar is rec red and reads "Record"; given Screenshot, it is neutral and reads "Capture"; given Audio, it records audio only and no screen picker is shown.
- **AC4** Given ⌥⇧Space (macOS), then the Launcher opens as before; given ⌥⇧C, then the panel opens; both can be rebound; binding one to the other's chord is refused as a conflict.
- **AC5** Given the five dock tiles and the shortcuts ⌥⇧1 to 4, R, M and L, then each behaves exactly as in 0.3.4.
- **AC6** Given the Delay tile set to 5 s in the panel, then Settings > Capture & recording still shows the delay it had; given Copy turned off in the panel, then Settings > General "After capture" is unchanged, and a capture from the dock tile still follows Settings.
- **AC7** Given keys 1, 2, 3, Tab, Enter, Space and Esc, then the panel works without the mouse, and every control has an accessible name.
- **AC8** Given the panel, then it shows the Preview label and a Send feedback button that opens the browser with the app version only.

### QC-3 After-capture bar
**Priority** P1 · **Status** In progress (built and tested with fakes: the decision between the bar and the toast, the place under or above the area and on the display, Undo, the keys; not looked at on a screen or run on real hardware, see `tests/manual/QC-1.md`) · **Crates** apps/desktop · **Depends on** QC-1, CAP-4, ANN-5, CAP-6, REC-1

**User story.** As someone who captures first and decides later, I want a small bar after a screenshot, so that Edit, Copy, Save, Text, Long Shot or Record is one key away.

**Description**
- **When it shows.** After a **screenshot started from the panel or a preset** when both **Editor** and **Copy** are off for that capture. The screenshot is saved to the library first, so no choice loses it.
- **Actions.** **Edit (E)** opens the editor; **Copy (C)** copies the image; **Save (S)** saves to the default folder; **Text (T)** copies recognised text (ANN-5); **Long Shot (L)** extends this area by scrolling (CAP-6, the captured area as the start); **Record (R)** records exactly this area (the countdown, then the normal HUD).
- **Place.** Under the captured area, or above it near the bottom edge of the display, always fully on the display. The area is the rectangle the overlay reports for a Region, the window for a Window, the display for a Display; for a delayed Display capture the display is the primary one.
- **Dismiss.** Esc or a click elsewhere closes the bar; the capture stays in the library. It closes by itself after 20 seconds without the pointer over it, and never while the pointer is over it.
- **Save (S).** Writes the picture into the folder of Settings > General "Save a copy to a folder" when that is on; otherwise it opens Save as. Text (T) is dimmed when no text reader is available. Record (R) records the area with the panel's Video tiles (Mic, System audio, Webcam, Countdown). Long Shot (L) starts CAP-6 on the area with Auto scroll when it is available, else Hand scroll.
- **When Editor or Copy is on.** No bar: the capture goes where the panel says and a toast offers **Edit** and **Undo** for 6 seconds (the toast stays while the pointer is over it); Undo removes that capture from the library without asking, because it was just made.

**Acceptance criteria**
- **AC1** Given a panel screenshot with Editor off and Copy off, then the bar shows the six actions with their keys under the captured area.
- **AC2** Given Editor on or Copy on, then no bar appears, the capture goes there and a toast shows Edit and Undo for 6 s.
- **AC3** Given Esc on the bar, then the bar closes and the capture is in the library.
- **AC4** Given Record (R), then a recording of exactly that area starts after the countdown with the normal HUD.
- **AC5** Given Undo on the toast, then the capture is removed from the library.
- **AC6** Given the pointer over the bar, then it does not close by itself.
- **AC7** Given a capture near the bottom edge, then the bar is above the area and fully on the display.

### QC-4 Presets
**Priority** P1 · **Status** In progress (built and tested with fakes: the model, `presets.json`, the starters, the daemon actions, the picker's choices, the shortcut conflict check against every global shortcut and every other preset, the Save to folder with the local date, Auto-redact and the editor tool after a capture, the Launcher and tray entries, Settings > Capture > Presets and the edit sheet, AC1 to AC7 as far as they do not need a screen; not looked at on a screen or run on real hardware, see `tests/manual/QC-1.md`) · **Crates** cm-core, cm-daemon, apps/desktop · **Depends on** QC-1

**User story.** As someone who repeats the same setup, I want to save it with a name, so that one pick (or one shortcut) does it.

**Description**
- **A preset holds:** name; kind; target (Region, Window, Display or Long Shot; never the area itself); the kind's option tiles; where to save (a folder, with `{date}`); what to do after (open the editor with a chosen tool, copy, or nothing); an optional shortcut.
- **Storage.** `presets.json` in the app's settings folder, so a team can share one file. Unknown fields are kept when saving. An invalid preset in a hand-edited file is skipped and one notice names it; the others still load.
- **Where they show.** The preset picker in the panel header (No preset, the presets with a one-line summary, **Save current as preset…**, **Edit presets…**); the Launcher ("Preset: {name}"); the tray menu; Settings > Capture > Presets (reorder, edit, delete).
- **Edit sheet.** Name; kind and target; toggles Copy to clipboard, Auto-redact, Open in editor; Then (a tool); Save to; Shortcut with Change; Save preset, Cancel, Delete.
- **Shortcut.** A preset's shortcut runs it directly; it is conflict-checked with every other shortcut.
- **What the app does after a capture.** A call to `capture_screenshot` or `start_recording` carries what a call can carry (QC-5); the app does the rest through the existing actions: Auto-redact (`suggest_redactions`, then one `annotate` with the blur), the editor and its tool, the folder (the rendered picture is written there in the preset's format; scale 2 keeps the capture's pixels, scale 1 writes half the width and height), and transcribing after an audio take (`transcribe`). A screenshot preset's `cursor` is kept in the file and shown nowhere: no capture action takes it today. A preset's `markers` (audio) is the panel's Markers tile.
- **Where they show, in the app.** The picker in the panel; the Launcher rows "Preset: {name}" (the shortcut of the preset on the row); the tray's **Presets** submenu (after Recent captures); Settings > Capture & recording > Presets at the top of the page, where **Edit presets…** and the panel's settings button lead. A preset's Delete there asks for a second click. A preset picked in the panel keeps its tool, folder, format and scale while the tiles are changed; changing the tab to another kind drops it.
- **Shortcut.** A preset's shortcut is registered as a global hotkey where the desktop lets an app have global keys (not on a Wayland session without the portal). The sheet records it with Change: a chord that a global action or another preset already has is refused with that owner named ("Already used by Capture region", "Already used by the preset Bug report"); and a global action cannot take a chord a preset has. A hand-edited file that gives two presets one chord, or a preset a global action's chord, keeps the first and registers no second one.
- **Starter presets** (editable and deletable; deleting one does not bring it back): **Bug report** (Region, copy, Auto-redact, then the step badge tool), **Docs shot** (Window, editor, cursor shown, PNG at 2×), **Demo video** (Display, mic, webcam, 3 s countdown), **Meeting** (Audio, mic and system, transcribe after).

**Acceptance criteria**
- **AC1** Given the picker, when "Bug report" is chosen, then kind, target and option tiles are filled and the action bar reads "Capture · Bug report".
- **AC2** Given Save current as preset, then a preset with the panel's current setup appears in `presets.json` and in the picker, and reading the file back gives the same setup.
- **AC3** Given a `presets.json` with an unknown field, when a preset is saved, then the unknown field is still in the file.
- **AC4** Given a hand-edited `presets.json` with one invalid preset, then only that preset is skipped and one notice names it.
- **AC5** Given a preset with a shortcut, then the shortcut runs it directly; a shortcut already used by another action is refused with the conflict named.
- **AC6** Given the four starters deleted, then they do not come back on the next start.
- **AC7** Given a preset with Save to `~/Bugs/{date}`, then the capture is saved there with the local date and also appears in the library.

### QC-5 Presets for agents and the CLI
**Priority** P2 · **Status** In progress (built and tested with fakes, AC1 to AC6; no run on real hardware yet) · **Crates** cm-daemon, cm-mcp, cm-cli · **Depends on** QC-4, AGT-2

**User story.** As someone who uses an AI agent, I want the agent to run my preset by name, so that it captures the way I do.

**Description**
- `capture_screenshot` and `start_recording` take an optional `preset` (the name or the id). A new read-only tool **`list_presets`** (Free) returns the presets. CLI: `capturemann capture screenshot --preset "Bug report"`, the matching argument on the record command, and `capturemann presets list`.
- An explicit argument in the same call wins over the preset's value.
- **Confirm rules are unchanged:** a preset that records still needs the on-screen confirmation, and the dialog shows the plan the preset resolves to. A preset cannot loosen a locked tool or change `agents.tool_policy`.
- A preset holds no area, so a call that uses one still gives `target`. The CLI is the exception: `--preset` alone captures the primary display, and a Window preset asks for `--window`. A screenshot preset does not fit `start_recording` and a video or audio preset does not fit `capture_screenshot`; the call fails with `invalid_input`.
- The preset is written into the call's parameters before the call is checked, so the tier, the user's per-tool setting and the confirmation see an ordinary call. A preset sets only what the call itself can carry (delay, copy, Long Shot mode; microphone, system sound, webcam, cursor, frame rate and countdown); Auto-redact, the editor, the folder and the tool are for the app.
- `start_recording` already had `preset` for the five audio use cases (`room`, `online`, `lecture`, `product`, `note`). With `mode: "audio"` those five words keep that meaning; any other value is a Quick Capture preset.
- Agents read presets (`list_presets`) and run them; saving, deleting and reordering presets are internal actions for the app and are refused for agents.
- An unknown preset gives `invalid_input` naming it. `docs/reference/` is regenerated (`docs/reference/gen_agent_reference.py`) and the tool-count assertion in `crates/cm-mcp/tests/stdio.rs` becomes 90.

**Acceptance criteria**
- **AC1** Given `capture_screenshot { preset: "Bug report" }`, then the result equals the same call with the preset's options written out.
- **AC2** Given `list_presets`, then it returns every preset with its name, kind and target, and writes nothing.
- **AC3** Given `capture_screenshot { preset: "Bug report", delay_s: 3 }`, then the delay is 3 s and the other options come from the preset.
- **AC4** Given a recording preset started by an agent, then the confirmation dialog appears and shows the resolved plan.
- **AC5** Given `preset: "nope"`, then `invalid_input` names it.
- **AC6** Given the CLI `--preset "Bug report"`, then it behaves as AC1.

**Agent interface** — as above; both tools keep their tiers (Free for screenshots, Confirm for recording).

**Out of scope** — QC-2 smart kind, QC-6 pinned presets in the dock, a preset for Click Guide, shared preset folders.
