# Design system: CaptureMann

This file is the product-side contract for the UI: which surface follows which screen of the design kit, what the kit leaves open, and what native implementations may do differently. It does not repeat the kit.

**Note for contributors.** The HTML/PNG reference boards mentioned below (`design/screens/`, `design/redesign/`) are not part of the repository; the tokens, icons, brand marks and fonts in `design/` are. Where this file cites a board, the description here stands.

**Source of truth.** The brand and UI kit in [`design/`](../design/CLAUDE.md) is canonical for brand, tokens, fonts, icons and the reference screens. Read `design/CLAUDE.md` first. Exact values live in `design/tokens/tokens.json` (and its `tokens.css`, `CaptureMannTheme.xaml` mirrors) and in `design/screens/html/*.html`. If this file and the kit disagree on a visual value, the kit wins and this file is wrong; fix it. Where the kit is silent (see "Where the kit is silent" in each surface), the choices here and in the code stand until the designer fills the gap.

Specs cite sections of this file as "DESIGN §n". The numbering is stable: §1–§9, surfaces §5.1–§5.9. Traceability from kit screens to spec IDs and build status is in [`specs/design-implementation.md`](../specs/design-implementation.md).

## 1. Visual theme and atmosphere

CaptureMann feels like a precise camera body: dark, quiet, instant and out of the way until needed. Two references set the feel, never to be copied: Raycast (keyboard-first launcher, keycaps) and Runway (dark media editor with preview, inspector and timeline).

- **Dark first.** Dark is the default theme. The light theme exists (kit screen `EditorLight`): the primary button becomes ink with a small red dot, so red stays rare.
- **Floating surfaces** (launcher, recording HUD, bubble, region picker, toasts, dialogs) sit on top of the user's own content and must read over any wallpaper, video or document.
- **The workspace** (library, editor, settings) is flat: tone and 1 px borders separate areas; the user's video and thumbnails are the most colorful things on screen.
- **Mark.** The capture frame (three crop corners plus a solid circle bottom-right) is the logo. Its crop-bracket shape is reused as the selected-area marker (§5.1) and the circle is the webcam bubble (§5.4). Follow the kit's brand rules in `design/CLAUDE.md` §3 (no rotating the mark, no gradients, no emoji, clear space = bubble diameter).

The older "Overlay mode" and "Studio mode" vocabulary and the Lens Violet brand color are retired. Say "floating surface" and "workspace".

## 2. Color

Canonical: `design/tokens/tokens.json`. Implementers use the token names below, never raw hex in code.

| Group | Tokens | Rule |
| --- | --- | --- |
| Grounds | `ink` (window ground), `sidebar`, `surface` (cards, panels), `raised` (selected nav, hover, inputs, keycaps) | Dark by default; the light theme remaps them under `light.*`. |
| Borders | `border` (cards), `line` (strong borders, dividers, keycap edges) | 1 px. |
| Text | `text`, `text-2`, `text-3`, `mist` | `mist` is the lowest contrast allowed for small text. |
| `rec` `#FF4F3A` | Reserved: bubble ring, record/stop controls, recording state, the primary action on dark | **Text and glyphs on `rec` are `ink` (#0A0A0B), never white.** Never use `rec` for errors; errors use `warning` plus text. A destructive confirmation's primary button follows the normal primary rule (rec on dark), and its wording names the action. |
| `agent` `#5B8CFF` (`agent-text` for text) | MCP, CLI or agent activity only | Agent chip, agent toast, MCP status pill, agent sections. Not a generic link color. |
| `success`, `warning` | Status pills and chips | `warning` also carries error states. |

- **Light theme.** Primary button is `ink` with white text and an 8 px `rec` dot before the label; agent blue shifts to `light.agent`; `rec` is unchanged.
- **Annotation palette** (content colors drawn inside screenshots and videos, not UI colors): `#FF3B3B` red (default), `#FF8A00`, `#FFD60A`, `#2FD158`, `#2E8BFF`, `#B15CFF`, `#111111`, `#FFFFFF`. Step badges default to red with white numerals; text defaults to white with a 2 px black outline. The kit is silent here; this list stands. These are content, so the "red is reserved" rule does not apply to them.
- Contrast: body text at least 4.5:1. Use the token, not a lighter gray.

## 3. Typography

- **Families.** Geist for UI and display; Geist Mono for timecodes, timers, shortcuts, keycaps, paths, tool names, coordinates and small labels. Both are SIL OFL and bundled from `design/fonts/` (see `OFL-LICENSE.txt`; ship the license text in the app's About/Licenses).
- **Weights.** The kit ships Geist Regular, Medium, SemiBold and Bold plus Geist Mono Regular and Medium (static TTF and variable files). Native frameworks that cannot select a weight from a variable font must register each static file as its own face. Use 400 body, 500 labels and nav, 600 buttons and display.
- **Scale.** Token `fontSize`: caption 12, ui 14, body 15, title 17, h2 22, h1 26, display 64 (marketing). Letter spacing: display -0.045em, headings -0.03em, mono labels +0.04em.
- Use tabular figures (or Geist Mono) for every live number: timers, sizes, coordinates, timecodes, progress.
- Sentence case everywhere except the wordmark "CaptureMann". Verb-first labels: "Record region", "Publish last recording…". Buttons name the action and target ("Publish to YouTube"), never "OK".
- Native menus, the macOS menu bar item and the Windows tray menu use the OS font and are never custom-drawn.

**Native caveat.** GPUI cannot apply letter spacing. Accepted deviation: render display and heading text at the nominal size with zero tracking; do not fake it with per-glyph layout. Revisit if the framework gains support.

## 4. Components

Dimensions come from the kit (`design/CLAUDE.md` §4–§5 and the screen HTML). Summary of what implementers must not miss:

- **Controls.** 40 px tall, radius 10, padding 0 18, 14 px/600. Hit targets are at least 44 px (list rows 48). Spacing is a 4 px grid.
- **Primary button.** Dark: `rec` fill, `ink` text. Light: `ink` fill, white text, rec dot. One per view (Record, Publish, Export).
- **Primary button (amended Oct 23, 2026, OD-17, supersedes OD-12 and the sentences about `rec` primaries and the light-theme dot in §1, §2, §4 and §5.6).** The primary is neutral: dark = `text` fill with `ink` text (hover a little dimmer), light = `ink` fill with white text, no rec dot. `rec` red is only for `ButtonKind::Record` (Record, Stop, Start recording) and `ButtonKind::Danger` (destructive confirmations: Delete, Discard, Disconnect, used with a trash icon), both with `ink` text. Where this file still says "primary is rec", read it as Record/Danger.
- **Secondary button.** `surface` fill, 1 px `line` border, `text`. There is no separate destructive-red style; the kit defines none. **Decided (OD-12, Oct 21, 2026):** a destructive confirmation (Discard, Delete) uses the normal primary style (`rec` fill with `ink` text on dark; `ink` fill with the `rec` dot on light), as §2 and §5.6 say. There is no separate danger fill. In code `ButtonKind::Danger` in `apps/desktop/src/ui/components.rs` now renders as Primary, so the confirmation dialog and the annotation editor's "Delete" follow this rule; the wording carries the risk, not the color.
- **Nav item.** 40 px, radius 8, 14 px; active = `raised` (dark) or white with border (light); optional count in `mist` 12 px.
- **Card.** `surface`, 1 px `border`, radius 14, padding 16; section label 12 px/500 `mist`.
- **Keycap.** Geist Mono 12, `raised`, radius 6, padding 3 7. macOS shows ⌘ ⌥ ⇧ ⌃; Windows shows Ctrl, Alt, Shift as words.
- **Status pill.** Radius 999, 1 px `line`, 12 px, dot plus label in `success`, `mist` or `warning`.
- **Chips.** Size/resolution chips (mono 12, `ink` fill, 1 px `line`, radius 6, padding 4 8) label the selected area (§5.1).
- **Inputs.** `raised` fill, 1 px `line`, radius 10, 40 px tall. The kit does not show a text field outside the launcher search (20 px); use this. Secrets show masked with a reveal toggle and "Stored in Keychain" or "Stored in Credential Manager".
- **Segmented control** (Circle / Rounded / Off, kit `Editor`): 40 px options, radius 10, 1 px `line`; selected = `rec` border (dark) or `ink` border (light).
- **Slider.** Track `line`, filled part `rec`, round thumb; value right-aligned in `mist` (kit `Editor`, Size 18%).
- **Progress** (kit silent): 4 px bar, radius 2, track `line`, fill `text`; indeterminate = 30% segment sliding at 1.2 s. Rows show bytes, speed and ETA in mono.
- **Toasts.**
  - *Toast* (redesign R17, OD-20): `surface_2`, 1 px `border`, radius 14, 440 px wide, a 3 px tone stripe on the left, a 30 px tinted icon tile whose glyph differs by kind (`info`, `circle-check`, `triangle-alert`, `octagon-alert`, `bot`), title 14/600, body 13, small actions with icons, a dismiss `x`. One stack, bottom-right of the active screen, at most 3. Toasts without action buttons auto-dismiss after 6 s and pause while the pointer is over them; toasts with action buttons ("Copy link", "Open", "Retry") and errors stay until dismissed. Tones: Info neutral, Success green, Warning brass, Error the soft rec of the kit's error status, Agent blue.
  - *Agent toast* (same shape, `bot` icon, agent tone, joins the bottom-right stack; it was top-right before R17): title "Started by agent via MCP" followed by the real tool name in mono `agent-text` (for example `start_recording`) and "from {client}". Used for every agent-triggered action surfaced to the user. No Stop action (the board draws one: not built, `TODO(design)`).
- **Agent chip** (confirm dialog): `agent` dot, "Requested by {client name}".
- **Timeline.** 34 px track bars, radius 8, 80 px label column. Screen neutral, Webcam `rec` at 22% fill with a 60% border, Mic waveform stripes, playhead a 2 px `text` line, ruler in mono 12 `mist`.

**Native caveat (accepted deviations).**

- GPUI cannot do backdrop blur. Everything the kit blurs (launcher, HUD) uses the same fill at 96% opacity, which the kit already specifies (`rgba(17,17,19,0.96)`), without the blur.
- Toasts and dialogs are separate opaque windows without drop shadows. They keep the 1 px border and radius and drop `shadow.float` and `shadow.hud`. On the light theme this makes the edge of a dialog over a white page faint; the 1 px `line` border is therefore mandatory.
- Floating windows over user content may be transparent only where the platform framework supports it (the overlay already does for the picker); the bubble ring and HUD must not depend on it.

## 5. Product surfaces

Each subsection names the kit screen it follows, what it requires, and where the kit is silent.

### 5.1 Region picker (floating, full-display)

Follows kit `Recording` for the selected-area treatment; the kit has no picker screen.

- Frozen display fills the screen; outside the selection a 55% black scrim (as in the kit's dim layer).
- **Crop-bracket motif:** the selection corners are the logo's crop brackets: four L-shaped corners, 4 px white, radius 6, 28 px arms, drawn just outside the selection. Edges between the brackets are not drawn. Adopt this instead of an 8 px handle square outline. While recording, a thin `rec` outline (2 px, 70% opacity, dashed while paused) also runs along the selection edge (REC-5).
- **Size readout** above the top-left corner: mono 12 chips in the kit's chip style, for example `1920 × 1080` and a second chip `2× · native` (scale factor and "native" when capture is at physical resolution).
- Crosshair, magnifier (128 px circle at 8x with pixel grid, hex and coordinates) and the toolbar after selection (Copy, Save, Annotate, Record this area, Cancel, each with a keycap) are specified in CAP-1 and CAP-3.
- **Kit is silent on:** the crosshair, the magnifier, the post-selection toolbar chrome, and drag handles. Keep the existing implementation; restyle only with tokens (radius 16 floating toolbar, 40 px/44 px targets, `raised` hover, keycaps per §4).

### 5.2 Recording HUD (floating, excluded from capture)

Follows kit `Recording`, the control bar.

- Floating bar, radius 16, padding 6, gap 6, 1 px `line`, fill `rgba(17,17,19,0.96)`; 44×44 icon buttons with radius 11; draggable, snaps to screen edges, remembers position per display (REC-5).
- Order: **Stop** (`rec` fill, 14 px `ink` rounded square), **timer** (Geist Mono 15, 8 px `rec` dot before it), divider, **Pause**, **Mic** (with a five-bar level meter), **Webcam**, **Draw** (disabled, hint "Coming soon"; P2, REC-9), divider, **Restart**, **Discard**. Restart sits immediately left of Discard; the kit has no Restart, so this is a deliberate deviation from the kit's order (decision OD-4, Oct 21, 2026). Active toggles (Mic, Webcam) use `raised` fill. Restart discards the take and restarts with the same settings after a new countdown (REC-5).
- Pause becomes Resume while paused; the timer dot stops pulsing and turns `mist`, and the `rec` outline of the selected area turns dashed. With the OS "Reduce motion" setting on, the dot is static (no blink).
- The HUD is a non-activating panel (it never takes focus from the recorded app); a click makes it key and then Tab reaches the buttons, so while recording the keyboard route to Stop, Pause, Discard and Restart is the global hotkeys (CAP-2). During the 60 fps self-test the HUD is a Stop-only bar.
- Selected area: crop brackets as §5.1 with the dim scrim outside, plus a thin `rec` outline (2 px, 70% opacity) along the selection edge for visibility on busy backgrounds (decision OD-3, Oct 21, 2026), and the size and scale chips above the top-left corner.
- **Differences from the kit, decided Oct 21, 2026** (see `specs/design-implementation.md`, Decided): Restart is added next to Discard (OD-4); the `rec` outline is added to the brackets (OD-3); the mic level meter is kept (OD-10) and the collapsed pill is an optional P2 item; Draw is disabled until REC-9 (OD-5).
- Setup panel before recording (REC-1: source, size, fps, camera, mic, system audio): the kit has no screen for it. Built as a launcher-style start sheet (§5.9 styling); the launcher's record commands open it. The HUD, countdown, bubble, toast, start-sheet, confirmation and launcher windows set `sharingType = none`; the engine also excludes the whole process by pid. Studio and the permissions screen deliberately do not set `sharingType`, and the library window is captured by recordings and screenshots (REC-6.AC8, decided after R4-6: a recording shows everything on screen except the recorder UI); its window id is passed to the engine as a kept window.
- **Kit is silent on:** collapsed pill state (optional P2), behavior over full-screen apps. Keep REC-5 behavior.

### 5.3 Countdown

The kit has no countdown. Keep the specified behavior (3-2-1, Esc cancels, Space skips): a 160 px circle centered on the recording area with the digit in Geist Mono 96/600 `text` and a 4 px ring that drains each second. Fill `surface`, border `line`. The ring color is `rec` (recording is about to start, and the kit reserves rec for that state). The corner delay pill of CAP-5 (48 px disc in the top right corner of the display, digit Geist Mono 20/600, 2 px `rec` ring that drains over the whole delay; built, `apps/desktop/src/ui/delay_pill.rs`) uses the same styling. Like the countdown it never takes focus; a click makes it key and then Esc cancels. Reduced motion (honored when macOS "Reduce motion" is on): the ring stays full and nothing repaints between digits.

Built behavior: the countdown window never activates CaptureMann and takes no keyboard focus when it opens, so the recorded app keeps the keyboard; a click on the disc makes it key and then Space skips and Esc cancels, and the global shortcuts cancel without a click. Esc during a **start** countdown discards the attempt (nothing is recorded); Esc during a **resume** countdown cancels the resume and the recording stays paused. The window is excluded from capture.

### 5.4 Webcam bubble (floating, excluded from capture, drawn again at export)

Follows kit `Recording` (live) and `Editor` (inspector).

- Position: bottom-left of the recorded area by default (24 px margin) at every start, setup preview included; free to drag anywhere (snap to the 8 anchors only within 32 px). The last dragged position is restored only when Settings → Webcam & audio → "Remember where I put the bubble" is on.
- Circle, 4 px `rec` ring (ring is part of the bubble's outer size), **no shadow and no backdrop** (accepted deviation from the kit's `0 12px 30px rgba(0,0,0,.5)`, decided Oct 21, 2026 after a product-owner report: on a transparent GPUI window a soft shadow or a glass fill shows as a square behind the circle; the ring is enough). The window is exactly the bubble's box and its transparent corners ignore the mouse.
- Shapes: Circle, Rounded (rectangle 4:3, 24 px radius in the specs), Off. Off hides the bubble from that time (CAM-1 hide), it does not stop the camera track.
- Size: three presets, S 160, M 240 (default), L 320 px (decision OD-1, Oct 21, 2026). The kit's inspector shows an 8–40% slider with an 18% default; it is not built in v0.1. Size is stored as a fraction of the output frame so a slider can be added later (CAM-1).
- The bubble toolbar (board R18, built Oct 24, 2026: a 235x40 pill above the bubble, below it when there is no room; size S/M/L, circle/rounded, mirror, camera off) and the no-camera state (initials or camera-off glyph on `raised`) are not in the kit; keep CAM-1 behavior and style them with the HUD recipe (§5.2).
- The export renderer uses the same geometry stored as fractions of the recorded area, so the exported bubble matches what the user saw, including the ring.

### 5.5 Editor

Follows kit `Editor` (dark) and `EditorLight`.

- Window layout: sidebar 220 px plus content; content is the preview (16:9, `surface` frame, radius 14) and a 300 px inspector, with the timeline full-width below. Header: breadcrumb ("Recordings /"), the capture title (17 px/600), secondary button "Save to drive" and primary button "Publish".
- Transport row below the preview: play button, mono timecode `current / total`, right-aligned mono stats (fps, codec, size).
- Inspector cards: **Webcam bubble** (segmented Circle / Rounded / Off, size as the S / M / L presets; the kit's slider is deferred, OD-1), **Output** (Resolution, Format, Save to), **Publish to** (checkbox per platform with its aspect, for example `16:9`, `9:16 crop`).
- Timeline per §4. Cuts stay visible as diagonal hatching so they can be restored (EDT-1); the kit has no hatching, so use `line` 1 px stripes at 6 px.
- Sidebar footer card: agent-blue MCP status (dot, "MCP server running", last call and client, in mono). Data comes from daemon events (AGT-3).
- **Annotation editor (screenshots)**: the kit has no screen for it. Keep the existing implementation (left tool rail 44 px wide, color swatches from the annotation palette, stroke widths 2, 4, 6, 8) and restyle it with tokens in phase 2 of the adoption plan.
- **Recording editor as built (Oct 22, 2026, untested by eye):** the preview frame (`ink` letterbox, radius 14), a transport row (play, `current / total` in mono, right-aligned stats), a 300 px inspector with Speed, Background and Output cards, and the timeline below: an 80 px label column, a 24 px ruler in mono 12 `mist`, 34 px track bars with radius 8 (Screen `raised`, Webcam `rec` at 22% with a 60% border, Mic neutral), hatched cuts (`mist` at 70%, 1 px, 6 px apart), splits as 2 px `text_3` lines, trim handles in `text` with a `ground` grip, a 2 px `text` playhead, a range in `agent` at 18%. It uses only theme tokens, so the dark and light editors follow the window theme (contrast checked in `timeline.rs` tests). Not built from the kit: the Webcam bubble, Output and Publish-to cards, and the header\'s Save to drive and Publish buttons (the header has "Recordings /", the title and "Recording details").
- **Kit is silent on:** zoom regions, captions track, undo/redo chrome, the screenshot editor, error states. Keep the specs.

### 5.6 Publish sheet and confirmations

The kit's publish surface is the editor's "Publish to" card and the agent toast; it has no sheet or dialog screen.

- **Publish sheet** (PUB-5): 720 px wide, radius 16, `surface`, 1 px `line`. Content first: the export preview at the platform aspect (16:9 or 9:16) with the generated title under it, then per-platform tabs and fields per the Integrations spec. Platform names are plain text unless official assets are used per each platform's guidelines. The TikTok tab follows TikTok's required UX (nickname, privacy with no default, interaction toggles off, commercial disclosure, music declaration).
- **Agent confirmation** (AGT-2): modal on the active display, 480 px, radius 16. Agent chip at the top (§4), a plain sentence of what will happen, the file and account details, then two buttons: Cancel (secondary, default focus) and a primary naming the exact action. Enter is ignored for 1 s after the dialog appears. The primary is `rec` on dark even for publish or delete; the wording carries the risk, not the color.
- Native caveat: the dialog is an opaque window without shadow (§4).

### 5.7 Library

Follows kit `Editor` for the shell only; the kit has no library grid screen.

- Sidebar (OD-16, Oct 23, 2026, built as Phase 1: `ui/sidebar.rs`, model in `nav.rs`): one component with three modes. **App** (248 px, Library, Intelligence, Settings pages): brand row with the version pill; the **capture dock** (a red Record… button that opens the Record sheet, then Region, Window and Display tiles; every hint is the live chord from the shortcut map, so a rebound key shows at once and Windows shows its own; a tile shows the whole chord when it is three keys or fewer, else its last key, with the full chord in the tooltip and the accessible name); group LIBRARY (All captures, Recordings, Screenshots, Published; the real counts, hidden while unknown); group INTELLIGENCE (AI models, Agents & CLI, two items, no tabs); Settings pinned at the bottom; the MCP status card at the very bottom (real status text, a button that opens Agents & CLI). **Rail** (64 px, icons with tooltips) on the full-window pages (recording details, recording editor, screenshot editor, publish): brand mark, Record, Region, a divider, All captures, Recordings, Screenshots, Published, AI models, Agents & CLI, then Settings and the MCP button at the bottom; the active item has the raised background. **Settings** (248 px) on Settings and Destinations: "Back to library" (returns to the last library page), the title, the 8 sections with icons and the compact MCP card; there is no inner settings nav. The setup screen keeps the app sidebar, dimmed to 50%. Highlight: details and the recording editor under Recordings, the screenshot editor under Screenshots, the publish page under Published, the setup screen under Settings. "All captures" (`Route::Library`, the mixed grid) is the default landing page; Screenshots has its own route. The old page header is gone: library pages and Published get a minimal title row with the real count (search, chips and sort stay in the library toolbar until the Phase 2 restyle); the capture-delay choice moved to Settings → Capture & recording ("Before a screenshot"); the hero row is off (OD-20).
- Grid and home layout (latest capture large, two recents beside it, then a grid; list view rows 48 px) are kept from the product spec, restyled with tokens: thumbnail radius 14 or 10, duration badge in mono 12 on 70% black, status icons top-right, title 15 px and date 12 px `mist` below.
- **Kit is silent on:** the home layout, empty states, search field, "All" and "Uploaded" filters, list view.

### 5.8 Settings

Follows kit `Settings` (Destinations and Agents & CLI on one scrolling page).

- Sidebar (settings mode of the one sidebar, OD-16; the 200 px inner nav is removed): General, Capture & recording, Webcam & audio, Destinations, Permissions, Privacy, Shortcuts, About · Apache-2.0 (AI models and Agents & CLI are sidebar items of their own). `TODO(design)`: the guide says About scrolls to an About card on General; About stays its own page until the Phase 5 page change. Spec text that says "Settings → Recording" or "Camera & audio" means these pages. Publishing accounts live under Destinations; the agent client policy lives under Agents & CLI. Privacy has no kit page; keep it as an extra section before Shortcuts until the designer adds it. AI is its own sidebar section (§5.11); its Settings entry is only a pointer to it.
- **Destinations.** Section "Save to": one row per destination (30 px icon tile, name, mono subtitle with path or "Not connected", status pill "Default", buttons Manage or Connect): Local folder, Google Drive, OneDrive, Dropbox. Section "Publish to": three cards, YouTube, TikTok, Facebook, each with a status pill (Connected or Not connected), one line of detail, and a Manage or Connect account button. Connect opens the guided bring-your-own setup (PUB-1, STO-2).
- **Agents & CLI.** Header with the agent-blue status pill (built wording: "MCP ready" or "MCP offline", the same text as the launcher hint; the transports are listed after it). Left card "Client config": the JSON snippet in mono with a Copy button; right card: one row per capability (title, mono tool names, control showing Allow or Ask first). The control labels and their relation to the permission tiers are defined in AGT-2.
- Recording settings show the disk estimate per minute live as values change (REC-2).
- General holds "Launch CaptureMann at login" (default off; a preference file, not a setting, see CAP-2) and Shortcuts has a rebinding UI for the ten global shortcuts. A corrupt `settings.json` is moved aside to a timestamped backup (the newest five are kept), the user gets one warning, and the per-client agent policies are restored from the `client_policies.json` mirror so a restrictive policy is not lost with the file.

### 5.9 Launcher

Follows kit `Launcher`. Behavior and acceptance criteria are in LNC-1.

- Window 760 px wide, radius 16, fill `rgba(17,17,19,0.96)`, 1 px `line`, centered in the upper third of the active display. Search row with a magnifier glyph, a 20 px input and an `esc` keycap. Groups (Capture, Publish, Agents) with 12 px/500 `mist` headers.
- Row: min height 48, radius 10, padding 10 12, gap 14: a 30×30 icon tile (radius 8, 1 px `line`), title 15 px, right-aligned hint 13 px `mist` or chips, optional keycap. The selected row has fill `#232328` and its tile uses `rec` with an `ink` glyph. The Agents group tile uses an `agent`-tinted tile; its hint uses the agent status color.
- Footer: logo mark, "CaptureMann", then the selected row's primary action with a ↵ keycap, a divider, "Actions" with a ⌘K keycap (Ctrl+K on Windows).
- The kit shows a faint dot grid behind the launcher on a dark backdrop; that is presentation of the screenshot, not an app surface. The launcher floats over the user's desktop with no dimming.
- Native caveat: no backdrop blur (§4).

### 5.10 Permissions (setup screen and Settings page)

**Kit is silent**: there is no permissions screen. Revised Oct 21, 2026: the first-run experience is a screen of the main window, not a popup window. It reuses the Settings kit pieces. Behavior and acceptance criteria are in PRM-1.

- Setup screen: the content area of the main (Studio) window (route `Permissions`), no page header; the sidebar stays but is dimmed to 50%, and clicking an item leaves the screen. Content on `ground`, max width 720. H1 "CaptureMann needs a few permissions", one `text_2` sentence, the cards, and a footer with a `line` top border: a `text_2` caption on the left, "Not now" (ghost) and "Continue" or "Restart CaptureMann" (primary) on the right. Continue is dimmed to 45% until the required permission works.
- Card: the kit `card` (radius 12, 1 px `border`, `surface`, padding 16), name 15 px/600, a "Required" or "Optional" outline pill, and a state pill (dot plus label in the state color: `success` for Granted, `warning` for Not granted, Denied, Restricted and restart required, `text_3` for Not asked yet). Under it the reason (`text_2`), "Needed for:" (caption, `mist`), an optional extra sentence, and buttons: the first is primary, the rest secondary.
- Settings → Permissions shows the same cards under a "Check again" small button; it sits before Privacy in the Settings nav.
- Library banner: the existing status banner with a `warning` left edge and the buttons under the text.
- The setup screen is part of the main window, so it needs no separate capture handling (Studio and the setup screen do not set `sharingType = none` and are captured by recordings and screenshots while visible, REC-6.AC8; only the recorder UI is excluded by pid).

### 5.11 AI page (flat workspace, own sidebar section)

Not in the kit; built from existing kit components only (OD-14, owner decision Oct 22, 2026: AI has its own menu, not a Settings page). The sidebar item "AI models" (sparkles) opens it, followed by "Agents & CLI" under INTELLIGENCE; there are no tabs (OD-16). The page is a scrolling column like Settings (max width 900, 40 px side padding), with the page title row (H1 "AI models", subtitle "Transcripts and captions, made on this Mac.").

- **Notice (no model installed).** A `card` with a 20 px `bot` icon, the title "No speech model yet" (15 px / 500), one `mist` caption line and a small secondary "Dismiss" button. Quiet: no accent colour, no modal, no toast. The same words appear as a one-line hint under the Recordings header (small buttons "Open AI" and "Dismiss") and, beside a disabled Transcribe button, as `mist` text with an "Open AI" button (that one is not dismissible because it explains the disabled button). Dismissal is remembered per viewer.
- **Speech models.** A `list-card` with one row per model (tiny, base, small, medium): title, mono subtitle "size · speed-vs-accuracy hint", a dot status (`success` dot when in use, `warning` dot for a failed download, `mist` otherwise: Installed, Not installed, Downloading 26%, Paused at 26%, Remove this model?) and buttons on the right: small secondary buttons Download, Import from file, Use this model, Remove, Cancel; the committing step (Download 466 MB, Resume, Remove model) uses the kit's primary button (OD-12: destructive confirmations are the normal primary). Pressing Download only opens the question on the row; the second press, on "Download 466 MB", starts it. A running download shows a 4 px bar (`line` track, `text` fill) and "120 MB of 466 MB" under the row, and Cancel keeps the partial file so Resume continues.
- **Copy.** A `mist` caption over the list says transcription runs on this Mac and nothing is downloaded unless the user presses Download.
- **Switch.** One list-card row "Transcribe after each recording" with the standard switch (default on at 16 GB of RAM or more); its subtitle names the model in use or says "Needs a model above".
- **Recording editor.** The inspector gets a Captions card (toolbar of small buttons: Add at playhead, Save SRT, Save VTT, Save TXT, Save changes, and a Timestamps-in-TXT switch; a scrolling list of lines with a mono timestamp, the text and a warning "!" when a rule is broken; the selected line expands into the text field, its rule messages and Split, Merge with next, Remove) and a Suggestions card (Suggest zooms, Suggest cuts; each answer is a list of mono rows with Accept and a remove "✕", plus "Accept all"). Rules in `warning` text: 42 characters a row, 2 rows, 1 to 7 s.

Open visual questions for the designer (the build uses the nearest kit component): a dedicated AI icon in the nav (the nav icon is now `sparkles`); a determinate progress style (the 4 px bar above is a stand-in); the caption list's row density.

## 6. Layout, spacing and depth

- **Spacing:** 4 px grid; tokens `space.1` to `space.10` (4 to 40 px). Controls 40 px, targets 44 px, sidebar 248 px (rail 64 px), inspector 300 px, launcher 760 px.
- **Radii** (`radius` tokens): 6 keycaps and chips, 8 nav items and icon tiles, 10 buttons and rows, 14 cards and the preview, 16 launcher, HUD, sheets and dialogs, 999 pills (status pills only; the bubble circle is a shape, not a pill). Do not use the retired 2/4/12 values.
- **Layout.** Main window and Settings: sidebar 248 px (64 px rail on full-window pages) plus content (stacks above the content when narrow). Editor: preview and inspector side by side, timeline below in a horizontally scrolling box.
- **Depth.** Workspace surfaces are flat: no shadows. Floating surfaces use `shadow.float` (launcher) and `shadow.hud` (HUD) where the framework supports shadows; otherwise the 1 px `line` border carries the edge. No gradients, no inner highlights.

## 7. Motion, sound and accessibility

- Durations 80 ms press, 120 ms hover, 180 ms panels, 240 ms sheets; easing `cubic-bezier(0.2, 0, 0, 1)`.
- Floating surfaces appear with no animation; speed is the feature (hotkey to visible 150 ms or less, NFR-1).
- Reduced motion: no pulsing dot (solid), no draining ring (numbers only), crossfades instead of slides.
- Optional sounds, off by default: shutter, countdown tick, finish chime.
- Focus is always visible (2 px `text` ring, or `ink` on light); every flow works by keyboard; icon buttons have accessible names and tooltips with the shortcut keycap; minimum target 44 px for HUD and launcher, 40 px controls elsewhere.

## 8. Iconography, assets and imagery

- Icons: 2 px stroke, round caps (Lucide-style). 18 px in the HUD and launcher tiles, 16 px in rows. Default color `text-3`, `text` on hover.
- Platform logos (YouTube, TikTok, Facebook, Google Drive, OneDrive, Dropbox): plain text names until official assets are added per each platform's brand guidelines; never recolored.
- **App and tray icons** come from the kit, not from code-drawn shapes:

| Use | Asset (under `design/brand/`) |
| --- | --- |
| macOS app bundle | `app-icon/AppIcon.icns` (regenerate from `AppIcon.iconset` with `iconutil`) |
| macOS menu bar, idle | `tray/png/TrayTemplate.png`, `@2x`, `@3x`, set as a template image |
| macOS menu bar, recording | `tray/png/tray-recording-on-*.png`, not a template image so the red shows |
| Windows exe, installer | `app-icon/app.ico`; MSIX tiles from `app-icon/png/windows/` |
| Windows notification area | `tray/tray-dark.ico` or `tray-light.ico` by taskbar theme; `tray-recording-on-*.ico` while recording |
| In-app logo | `logo/svg/mark-currentcolor.svg` |

  Acceptance criteria are in CAP-2 and REC-5.

## 9. Do's, don'ts and review checklist

### Do
- Use token names; never raw hex.
- Put `ink` text and glyphs on `rec`.
- Use `agent` blue for anything an agent or MCP client did.
- Show keycaps for shortcuts; name buttons by action and target.
- Put default focus on Cancel in any confirmation an agent triggered.
- Use Geist Mono for timers, timecodes, coordinates, paths and tool names.
- Keep workspace surfaces flat; let thumbnails and video carry the color.
- State every native-framework deviation in code comments and in `specs/design-implementation.md`.

### Don't
- Don't use red for errors.
- Don't put white text on `rec`.
- Don't use blue as a general link color.
- Don't recolor, rotate or outline the mark, or move the bubble to another corner.
- Don't add gradients, emoji or new brand colors or logo variants.
- Don't retype "CaptureMann" in another font; use the SVG lockups.
- Don't animate floating surfaces in.
- Don't custom-draw native menus or the tray menu.
- Don't use the Lens Violet palette, Inter, or the old radius scale.

### Example prompts
- "Build the recording HUD per DESIGN §5.2 and design/screens/html/Recording.html: radius 16, 44 px buttons radius 11, Stop in rec with an ink square, mono 15 timer with an 8 px rec dot."
- "Build the Destinations page per DESIGN §5.8 and design/screens/html/Settings.html with the real default folder and live connection state, not the bracketed placeholders."
- "Build the launcher per DESIGN §5.9 and LNC-1; commands whose action is not built yet are shown disabled."

### Review checklist
1. Colors come from tokens; text on `rec` is `ink`.
2. `rec` is used only for recording, the bubble and the primary action; errors use `warning`.
3. Agent activity is blue.
4. Controls are 40 px, hit targets 44 px, radii from the scale.
5. Workspace is flat; floating surfaces have a 1 px `line` border.
6. Geist and Geist Mono are bundled; live numbers use mono or tabular figures.
7. Agent-triggered confirmations default to Cancel.
8. Any deviation from the kit is listed in `specs/design-implementation.md`.

**Recording details: Transcript section (Oct 22, 2026).** Under Exports, the Transcript section shows what is stored, not just a Transcribe button: a one-line summary (language, model, words, when), then small buttons Open captions, Download TXT, SRT, VTT, a Timestamps switch (TXT) and Re-generate. Re-generate opens a bordered panel under it: the sentence saying what it replaces (in `warning` when caption edits would be discarded), a segmented control for the model (installed models only), a wrapping segmented control for the language (Auto first) and a primary Re-generate with Cancel. With no transcript the section shows the Transcribe button and the gentle no-model hint. No new component: it is built from small buttons, segmented controls and the switch of the kit.

## 10. Redesign components (Phase 0, Oct 23, 2026)

Building blocks for the redesign (`design/redesign/CLAUDE.md`, plan `specs/redesign-plan.md`). Use these in Phases 1 to 5; do not hand-build the same markup. All take `t: &Theme`.

Tokens (`theme.rs`, mirrored in `design/tokens`): `surface_2`, `field`, `faint`, `rec_soft`, `rec_hover`; code `border` = kit `--line`, code `line` = kit `--line-strong`. Theme helpers `segment_selected()` (#2A2A30), `icon_tile()` (#1A1A1E), `row_divider()` (#1E1E22). Scale: `SIDEBAR_W` 248, `RAIL_W` 64, `TOAST_W` 440, `RADIUS_DIALOG` 18, `RADIUS_DIALOG_LG` 20.

| Component | API and sizes |
|---|---|
| Button | `components::button(t, id, label, kind)`; with an icon: `button_with_icon(t, id, label, kind, Some("name"))` (40 px, icon 16 px, gap 8, icon colour = label colour, decorative). Kinds: `Primary` (neutral), `Secondary` (`surface_2`, `line` border), `Ghost`, `Record` and `Danger` (rec red, ink text). `ButtonKind::colors(t)` is the pure colour table. |
| Small button, segment | `widgets::small_button_with_icon(t, id, label, icon)` (32 px, icon 14, gap 6), `widgets::segment_with_icon(t, id, label, active, icon)`. Plain `small_button` and `segment` keep their signatures. |
| Segmented control | `widgets::segmented(t, id, aria)`: `field` track, 3 px padding, `line` border, `role=radiogroup`; selected segment `segment_selected()` with `text`, `role=radio`. |
| Switch | `components::switch(t, id, label, on, enabled)`: 36x20, on = `text` track with ink knob, off = `line` track with `mist` knob, `role=switch`, 44 px target. |
| Icon button | `components::icon_button(t, id, icon, aria, pressed, tooltip_keys)`: 18 px glyph in a 44 px target, aria label required (also the tooltip text when `tooltip_keys` is `Some`, with keycaps), `pressed` draws a `raised` background and `aria-toggled`. |
| Tooltip | `tooltip::tip(label, keys)` for `.tooltip(...)` on any element (the editor's former `KeyTip`). |
| Keycap | `components::keycap`/`keycaps` stay the compact form (tooltips, hints). `kit::keycap_lg(t, "R")` is the kit keycap (min 26x28, `raised`, 2 px bottom border, mono 13); `kit::shortcut_caps(t, "⌥⇧R")` splits with `kit::split_shortcut` (also `Ctrl+Shift+R`). |
| Pill, status | `kit::pill(t, Tone, Some(icon), label)`: 24 px, 1 px border, 12 px icon, gap 6. `Tone` = `Neutral`, `Brass`, `Success`, `Agent`, `Danger`; tinted tones use 10 % fill and 38 % border. `kit::status(t, StatusKind, label)` with `Ok` (circle-check), `Warning` (triangle-alert), `Error` (octagon-alert), `Pending` (loader-circle), `Neutral`. |
| Callout | `kit::callout(t, Tone, icon, title, body, action)`: tinted card, 18 px icon, title 14/500, body 13, optional right action. |
| Settings rows | `widgets::settings_group(t, title, note, rows, footer)` builds the title row and one card; rows from `widgets::settings_row(t, tile_icon, label, hint, control, last)` (34 px tile r10, label 14/500, hint 12.5 `mist`, dividers 1 px); `widgets::footer_row(t, action)` for "Add local folder...". |
| Page toolbar | `page_toolbar(.., Option<AnyElement>)` as before; `page_toolbar_actions(.., Vec<AnyElement>)` for several actions. |
| Nav row, stat tile | `kit::nav_row(t, id, icon, label, count, active)` (40 px, r8, icon 18, gap 10, count `mist` 12 tabular, `raised` when active); `kit::stat_tile(t, label, value)`. |

Icons: `components::icon(name, size, color)`; sizes in `components::icon_size`. All icons the plan uses are in `apps/desktop/assets/icons` and `paint::ASSETS`; a test checks them against `design/icons/icon-map.csv`. Tray: `TrayState::Paused` and `Attention` (macOS template PNGs, Windows `.ico` files).

**Pop-up scrim (Oct 25, 2026).** The Record sheet and confirmation dialogs stay windows of their own (planning decision C4: they open from a hotkey with the app closed, and a full-screen dim would cover the window being picked and would have to be kept out of the recording). When the main CaptureMann window is visible behind one, it is greyed out with a `modal_scrim` tint (`rgba(5,5,6,0.66)` as the board draws it, no backdrop blur: GPUI has none) and ignores clicks until the pop-up closes. The window picker and the launcher get no scrim (their boards show the real desktop). Tested with fakes (the logic is `modal_dim_wanted`), not seen by eye.
