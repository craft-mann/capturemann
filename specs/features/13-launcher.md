# Launcher (LNC)

A keyboard-first command launcher, opened by a global hotkey, in the style of Raycast. It is a human front end to actions that already exist; it adds no new capability and no new agent surface. Visual reference: design kit screen `Launcher` (`design/screens/png/Launcher.png`, `design/screens/html/Launcher.html`); styling in DESIGN §5.9.

### LNC-1 Command launcher
**Priority** P1 · **Status** In progress (built on macOS and unit tested: the launcher window, matcher, ranking, command list, keycaps, disabled states, the recording commands, the Actions list (⌘K) for the record and screenshot commands, MCP hint and config copy; not verified on hardware: AC1 timing, AC6, AC9, AC10 with VoiceOver; Publish and Save to cloud drive are disabled until M6; Windows not run) · **Crates** apps/desktop (launcher window), cm-core (command list) · **Depends on** CAP-1, CAP-2, REC-1, DESIGN §5.9

**User story** — As a keyboard-driven user, I want one hotkey that opens a searchable list of everything CaptureMann can do, so that I can start a capture, a recording or a publish without remembering shortcuts or opening the app.

**Description**
- **Open and close.** A global hotkey (default: ⌥⇧Space on macOS, Ctrl+Alt+Shift+Space on Windows; rebindable in Settings → Shortcuts; decided as OD-8, Oct 21, 2026) shows a 760 px wide launcher centered horizontally, with its top at 14% of the height of the display that has the pointer (the primary display where the pointer position is unknown), with the search field focused. Esc, a second press of the hotkey, or clicking outside (the launcher losing focus) closes it. It is also reachable from the tray menu ("Open launcher"). Opening it on macOS activates CaptureMann (see Window behavior).
- **Search.** The field filters commands as the user types: case-insensitive subsequence match on the title, word-start matches ranked first, then on keywords (for example "rec" finds the record commands). Matching runs on the in-memory command list and updates on every keystroke. An empty query shows all commands in group order.
- **Groups.** Results are grouped under the headers **Capture**, **Publish** and **Agents**; empty groups are hidden. Within a group, order is by rank, then by recent use (most recent first), then default order (see Ordering for disabled commands).
- **Rows** (DESIGN §5.9): a 30×30 icon tile, a title, a right-aligned hint (plain text, chips or a status), and a keycap when the command has a bound shortcut (the keycap shows the user's current binding, not a hardcoded one). The selected row has the selected fill and a `rec` icon tile with an `ink` glyph. Hints are live values: "Drag to select", "PNG · copy + save" ("PNG · save" when copy-after-capture is off), "Main window", "Preferences", or the MCP status ("MCP ready" in agent blue, "MCP offline" as plain text; the desktop app serves the daemon socket, it does not launch the MCP client, so ready/offline is all it can know). Hints from the kit that need recording or drives (recording size and bubble state, the default drive name, connected-platform chips) arrive with those features.
- **Footer.** Mark and "CaptureMann" on the left; on the right the selected command's primary verb with a ↵ keycap ("Start recording", "Take screenshot", "Open library", "Open settings", "Publish", "Save to drive", "Copy config"), a divider, then "Actions" with a ⌘K keycap (Ctrl+K on Windows).
- **Actions (⌘K).** Opens a secondary list for the selected command (v0.1: the options that exist for it, such as "Copy to clipboard instead" or "Choose destination"; commands with a single behavior show only "Run"). Esc returns to the main list. The kit shows the footer only; the contents are defined here. *As built (Oct 22, 2026, unit tested, the window untested by eye):* each row has a label, a short hint and, when it cannot run now, a dimmed look with the reason as its hint (↵ on it runs nothing and flashes the reason, AC5). The first row is the command's default and does exactly what ↵ in the main list does. Only the record and screenshot commands have more rows; the rest show "Run". The rows (code: `launcher/commands.rs` `options_for`, `launcher/plan.rs`):
  - *Record screen + webcam:* Start recording (webcam on, saved audio choices) · Without webcam · Screen + microphone only (no webcam, no system audio) · Screen only (no audio) · Record at 60 fps (disabled with "Run the 60 fps test in Settings first" until a current, passing self-test of this app version is stored; the engine still decides for the real display, size and codec when the recording starts) · Start in 5 s (a 5 second countdown for this recording).
  - *Record region:* Start recording (drag to select, then the sheet) · Include webcam · Without audio.
  - *Screenshot region, window and display:* Take screenshot · Save only (don't copy) when copy-after-capture is on, or Copy and save when it is off · Copy only (don't save), disabled with "Not available yet: every screenshot is saved to the library" · With 5 s delay (the CAP-5 pill, Esc cancels).
  - The record options open the start sheet (REC-1) pre-filled; pressing Start is the confirmation, so no option starts a recording by itself. A one-off audio choice is remembered like any other choice made in the sheet. `fps` and `countdown_s` go to `start_recording` as request fields. The launcher does not look for devices: a missing camera or microphone is handled by the sheet, not by disabling a row.
- **Commands (v0.1 set, 11).** Each command maps to an existing action in the Agent interface tab; the launcher never calls anything else.

| Group | Command | Action it runs | Needs | Default shortcut (from CAP-2) |
| --- | --- | --- | --- | --- |
| Capture | Record screen + webcam | `start_recording` (display, camera on) | REC-1, CAM-1 | ⌥⇧R |
| Capture | Record region | region picker, then `start_recording` | REC-1, CAP-1 | none (the kit's ⌥⇧5 is illustrative) |
| Capture | Screenshot region | `capture_screenshot` (region) | CAP-1 | ⌥⇧1 |
| Capture | Screenshot window | `capture_screenshot` (window) | CAP-1 | ⌥⇧2 |
| Capture | Screenshot display | `capture_screenshot` (display) | CAP-1 | ⌥⇧3 |
| Capture | Open library | opens the main window | LIB-1 | ⌥⇧L |
| Capture | Open settings | opens Settings; hint "Preferences" | none | none |
| Capture | Permissions… | opens Settings → Permissions (PRM-1); hint "What CaptureMann can use" | none | none |
| Publish | Publish last recording… | `publish_video` via the publish sheet; hint chips list the connected platforms | PUB-5 | none |
| Publish | Save to cloud drive | `upload` to the default drive; hint is the drive name (disabled until M6) | STO-2 | none |
| Agents | Copy MCP server config | copies the client config snippet (the same JSON as Settings → Agents & CLI, with the real `capturemann` path, `mcp` and `--socket`); hint is the MCP status | AGT-1 | none |

  The kit's illustrative shortcuts (⌥⇧5, and ⌥⇧3 for "Screenshot window") are not used; the table follows CAP-2. Commands come from a single list shared with the tray menu and the shortcut settings so labels and bindings cannot drift.
- **Disabled commands.** A command whose action is not built in this build, or whose prerequisite is missing (no connected drive, no recording yet), is shown disabled: title and tile in `mist`, hint explains why (built: "Not available yet" for Publish and Save to cloud drive, "A recording is running" for both record commands while a recording runs, "Library unavailable" for the three screenshot commands when no library is open; planned with their features: "Connect a drive in Settings", "No recording yet"), Enter does nothing except flash the row for 0.7 s, and the keycap is hidden. Disabled commands stay listed so the set is discoverable and sort after enabled ones within a group.
- **Record commands (built).** "Record screen + webcam" and "Record region" open the **start sheet** (REC-1) after the launcher closes: the first with the camera on, the second after the region picker (no screenshot is taken). They never start a recording directly, so the user sees the target, size and devices first. While a recording runs both are disabled with "A recording is running".
- **Confirmation.** The launcher is user input, so it does not trigger the agent confirm dialog (AGT-2). A recording started from the launcher behaves exactly like one started from the tray (REC-5 HUD, countdown, indicator).
- **Window behavior.**
  - It closes before the action runs. Screenshot commands then wait 150 ms (`CAPTURE_DELAY_MS`) before starting the capture so the closing window is gone from the screen before the overlay freezes it.
  - The launcher window is excluded from screen capture and recording like the HUD (`sharingType = none` and the pid exclusion), in addition to the close-then-delay sequence.
  - Keyboard focus (macOS): a non-activating panel receives no keyboard input from the window server (measured: keys went to the previous app), so opening the launcher activates CaptureMann and remembers the frontmost app. On close that app is activated again, except when the command itself brings CaptureMann forward (Open library, Open settings). Click-away closes it without restoring focus (the user already chose another app). Windows: the OS hands focus back.
  - Native caveat: no backdrop blur (DESIGN §4, accepted deviation in `specs/plans/design-implementation.md`). GPUI 0.3.9 cannot blur, so the launcher uses the opaque surface fill (`#111113`) instead of the kit's 96% fill, in a borderless pop-up window that is transparent only outside the rounded card (16 px corners, no shadow). The window is 760 px wide, tall enough for the full list (all groups and rows), capped at 80% of the display height.
- **Ordering.** Within a group, enabled commands come before disabled ones, then by rank, then by recent use, then default order. Recent use is kept per run only (see Known gaps).

**Acceptance criteria**
- **AC1** (unverified: the 150 ms visible time was not measured) Given CaptureMann is running with no window open, when the user presses the launcher hotkey, then the launcher is visible with the search field focused within 150 ms (NFR-1).
- **AC2** (met by unit test: matcher, ranking and model tests in `launcher/search.rs` and `launcher/model.rs`) Given the launcher is open, when the user types "rec", then only matching commands remain, grouped, and the first match is selected; ↑/↓ move the selection, ↵ runs it, Esc closes.
- **AC3** (met by unit test: `keycaps_follow_the_binding`, including a rebind) Given a command with a bound shortcut, then its row shows a keycap that matches the binding in Settings → Shortcuts, including after the user rebinds it.
- **AC4** (partly met: the ⌘K toggle, Esc order, the option lists and their outcomes are unit-tested (`lnc1_*` tests in `launcher/model.rs`, `commands.rs` and `plan.rs`); the footer verb comes from the command table and its rendering has no automated test) Given the selected row is "Record screen + webcam", then the footer reads "Start recording" with a ↵ keycap, and ⌘K opens Actions for that command.
- **AC5** (met by unit test: disabled reasons and Enter flashing) Given an action that is not implemented or lacks a prerequisite, then its row is disabled with a reason and ↵ runs nothing.
- **AC6** (unverified: the close-then-150 ms-delay mechanism is built, but no test or run checked that the launcher is absent from the image) Given the user runs "Screenshot window", then the launcher is gone from the screen before the capture and does not appear in the image.
- **AC7** (not met yet: Publish is disabled with "Not available yet" until PUB-5 exists, so neither the "No recording yet" state nor the publish sheet can be exercised) Given "Publish last recording…" with no recordings, then it is disabled with "No recording yet"; with a recording, ↵ opens the publish sheet (PUB-5) and never publishes directly.
- **AC8** (hint met by unit test `mcp_hint_is_the_real_status`; the wording is "MCP ready", not "MCP running". Copy and toast are implemented but have no automated test) Given the MCP status is ready, then the "Copy MCP server config" row shows the agent-blue "MCP ready" hint ("MCP offline" otherwise), and ↵ copies the JSON from Settings → Agents & CLI and shows the toast "MCP config copied".
- **AC9** (unverified: second-press toggle and click-away are implemented, neither was exercised) Given the launcher is open, when the user presses the hotkey again or clicks another app, then it closes and no command runs.
- **AC10** (met by unit test for accessible names; keyboard-only operation is covered by the model tests; VoiceOver and Narrator unverified) Given only keyboard input, then every command can be found and run without the mouse (NFR-13); each row has an accessible name that includes its hint and shortcut.

**Known gaps** (as of Oct 21, 2026)
- AC6 is unverified on a real display, although the window is now excluded from capture (see Window behavior).
- Windows: the pointer position lookup is missing, so the launcher always opens on the primary display; focus restore is a no-op (the OS handles it). Windows is unverified for the whole feature.
- Recent-use ordering is kept for the current run only, not across restarts.
- Not built: a "Stop recording" command (the record rows show "A recording is running" and the HUD, tray and ⌥⇧R stop it); the Actions panel (⌘K) lists options only for the record and screenshot commands (see Actions above); the other commands show only "Run", and "Copy only (don't save)" for screenshots needs a way to capture without storing.
- No backdrop blur (accepted deviation).

**Edge cases and errors**
- Hotkey registration fails (used by another app) → Settings shows "Couldn't register — used by another app" (as CAP-2.AC3); the tray menu entry still works.
- A recording is already running → record commands are disabled ("A recording is running"). Not built: a "Stop recording" command appearing first in Capture.
- Screen Recording permission missing → capture commands stay enabled; running one shows the existing `permission_missing` flow (CAP-1).
- Multiple displays → opens on the display with the pointer (macOS built; Windows falls back to the primary display).
- Full-screen apps → the launcher appears above them using the same window level as the HUD (REC-5) (pop-up window kind; not verified over full-screen apps).
- Very long hint text truncates with an ellipsis; the title never truncates before the hint.

**Agent interface** — None of its own. The launcher is human input like CAP-2; every command runs an action that already has an MCP tool and a CLI command (`capture_screenshot`, `start_recording`, `upload`, `publish_video`). "Copy MCP server config" is a GUI convenience that exposes no secret (the HTTP bearer token is never part of the snippet; AGT-1) and has no tool. The parity test (AGT-1.AC4) is unaffected because no action is added.

**Out of scope** — User-defined commands or scripts, extensions, clipboard history, searching library items or transcripts from the launcher (LIB-1 search stays in the library), calculator or other Raycast-style utilities, Windows Start-menu integration, per-command aliases.

**Test notes** — Unit tests for the matcher, the ranking (including recent use) and the command list generation (disabled reasons). UI snapshot test at the kit's 1280×800 reference against `design/screens/png/Launcher.png` (tolerance for the missing blur). Manual script: over full-screen apps, two displays, hotkey conflicts, VoiceOver and Narrator labels, both platforms.
