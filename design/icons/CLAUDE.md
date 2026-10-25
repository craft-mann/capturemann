# CaptureMann icon set: guide for coding agents

This kit adds icons to the CaptureMann desktop app. It is based on `specs/icon-inventory.md` (commit `8618ef8`), which found that 316 of the 329 Studio rows, and almost every button, menu item, tab and segment elsewhere, are text only.

**Read first:** `icon-map.md` lists every element, the icon it gets, its priority and its size (also as `icon-map.csv`). This file explains the rules behind it.

## 1. What's in the kit

```
capturemann-icons/
├── CLAUDE.md                  ← you are here
├── icon-map.md / icon-map.csv ← element → icon, priority, size, note (278 rows)
├── icons.json                 ← manifest: name, source, new vs already in app, where used
├── icons/                     ← 143 SVGs, 24×24, drop-in for apps/desktop/assets/icons/
│   ├── cm-*.svg               ← 15 CaptureMann custom glyphs (same grid and stroke as Lucide)
│   ├── *.svg                  ← Lucide v1.60.0 (the version the app already pins), ISC
│   └── LICENSE-lucide.txt
├── tray/                      ← two NEW tray states: Paused and Attention
│   ├── svg/  png/ (TrayPausedTemplate[@2x,@3x].png, TrayAttentionTemplate…, tray-*-on-dark/light…)
│   └── tray-{paused,attention}-on-{dark,light}.ico   (16, 20, 24, 32 px frames)
└── preview/icon-set.png       ← contact sheet of the whole set
```

## 2. Drawing rules (all icons)

- 24×24 viewBox, `fill="none"`, `stroke="currentColor"`, `stroke-width="2"`, round caps and joins. These are identical to the Lucide files already in `assets/icons/`, so `ui::components::icon(name, size, color)` tints them with no code change.
- The only fills are deliberate solid marks in custom glyphs (the record dot, the stop square, the bubble-corner dots). They also use `currentColor`.
- Never recolour an icon independently of its label. The icon takes the label's colour: `text`, `text_2`, `mist`, `warning`, `agent`, or ink on the rec-red Primary button.
- **Rec red stays reserved** for record and stop. Agent blue is for agent/MCP rows only.

## 3. Sizes

| Use | Size | Gap to label |
|---|---|---|
| Sidebar nav, Settings inner nav, Record sheet rows, editor transport and HUD (existing) | 18 px | 10 px |
| Leading icon on a 40 px button (`button`) and in-app menu items | 16 px | 8 px |
| Leading icon on a 32 px `small_button`, chips, segments with label | 14 px | 6 px |
| Inside pills and status labels (replacing the 6–8 px dot) | 12 px | 6 px |
| Toast kind icon | 18 px | 10 px |
| Card / banner leading icon | 18–20 px | 12 px |
| Empty-state tile | 32 px (tile 56 px) | — |
| Icon-only buttons | 18 px glyph in a 44 px hit target (32 px visual for toasts and the remove button, as today) | — |

## 4. Priorities and rollout

1. **P1 (132 rows):** icon-only controls, navigation, segment glyphs, status kinds, banners, empty states. These add meaning, so ship them first.
2. **P2 (99 rows):** a leading icon on text buttons (Export, Publish, Reveal, Copy link, Retry, Delete…). Add them screen by screen, and keep every label.
3. **keep (30 rows):** already correct. Don't touch.
4. **text only (9 rows):** deliberately no icon (see §6).
5. **planned (8 rows):** icons are ready for controls that aren't built yet (Auto-redact, audio mix, loop, frame step, timeline Fit, export preset/Cancel/Retry/Copy path, library rescan).

## 5. Fixes the inventory revealed (do these in P1)

| Where | Today | Change |
|---|---|---|
| Record sheet → System audio row | `rec-play` (a play triangle) | `volume-2` |
| Record sheet → Webcam bubble row | `rec-video` (camera) | `cm-webcam-bubble` (the row is about the bubble) |
| HUD → Discard | `x` (reads as "close the HUD") | `trash-2` |
| HUD → Stop | red square drawn with a div | `cm-stop` (ink on the red button) |
| Screenshot editor → Fit | `maximize` (four corner brackets, the same shape as region capture) | `shrink` (also used for Reframe "Fit") |
| Launcher → Permissions… | shares `launcher-settings` | `shield-check` |
| Toasts | 8 px dot only; Warning and Error look the same | `info`, `circle-check`, `triangle-alert`, `octagon-alert`, `bot`, so the kinds differ in shape, not only colour |
| Confirm dialog → "Allow for this session" | text "✓" | `rec-check` (registered but unused today) |
| Suggestions → dismiss | Unicode "✕" | `x` |
| Zoom chip → follow cursor | text " ⊙" | `locate` (12 px) |
| Bubble toolbar → Mirror / Shape | "⇆" glyph / drawn box | `flip-horizontal-2` / `circle` or `square` |
| Library sort chips | text " ↓" / " ↑" | `arrow-down` / `arrow-up` (trailing) |
| Blur, Pixelate and recording-editor steppers | text "-" / "+" | `minus` / `plus` |
| Timeline toolbar | text "In (I)", "Out (O)", "Split (S)", "Cut (⌫)", "Restore (R)", "Snap", "−", "+" | `cm-trim-in`, `cm-trim-out`, `cm-split`, `scissors`, `archive-restore`, `magnet`, `zoom-out`, `zoom-in`; keep the key in the tooltip |
| Destinations → local/library tile | mono "~" | `folder` / `hard-drive` |

## 6. Deliberately text only

- **Value segments:** resolution, fps, codec, quality, countdown seconds, languages, model names, S/M/L, percentages and sizes. A value reads better than a pictogram.
- **Dialog verbs** with no icon of their own: Cancel, Done, Keep it, Continue, Not now, Skip optional.
- **Native menus** (tray menu, macOS menu bar) stay text only. That's the platform convention, and those menus use native checkmarks.
- **Third-party logos** (Google Drive, OneDrive, Dropbox, YouTube, TikTok, Facebook) are not redrawn here. Either keep the letter tiles or use each company's official logo file under its brand guidelines.

## 7. Adding icons to the app

1. Copy the needed files from `icons/` into `apps/desktop/assets/icons/`. Copy only what you use. Lucide files keep their `@license` comment.
2. Register each one in the `ASSETS` table in `src/ui/paint.rs` (embedded with `include_str!`). The launcher has its own table in `launcher_window::asset`.
3. Render with `components::icon("name", size, color)`. For a leading icon, put it as the first child of the button with the gap from §3.
4. `button`, `small_button` and `segment` have no icon slot today. Add an optional `icon: Option<&'static str>` parameter (or a `.with_icon()` builder) rather than hand-building each button.
5. Accessibility:
   - When a label is visible, the icon is decorative: no extra aria text.
   - An icon-only control keeps its existing aria label, and gets a tooltip naming the action (with its keycap where one exists).
   - Toggled icon buttons (Snap, Mirror, Webcam) show their pressed state with a background, not only a different icon.
6. Loading spinners (`loader-circle`) rotate. With reduced motion, show them static.

## 8. Tray

The inventory notes the tray has only Idle and Recording, so Paused, countdown and errors all show the red recording icon. This kit adds:

- **Paused:** the frame with two pause bars in the bubble corner.
  - macOS: `TrayPausedTemplate.png` (template image).
  - Coloured versions: `tray-paused-on-{dark,light}` with red bars.
  - Windows: `.ico` files with 16, 20, 24 and 32 px frames.
- **Attention:** the frame with an amber "!" disc, for an upload that failed, a needed reconnect, the library going offline, or the tray not being able to build.
  - Use the template version on macOS, and the amber versions where colour is allowed.

Both new states need code support: `TrayState::Paused` and `TrayState::Attention` in `platform.rs`, with `recording/tray.rs` choosing Paused for Paused and Resuming. Starting, Countdown and Stopping can keep the Recording icon.

## 9. Custom glyph reference

| Name | Meaning |
|---|---|
| `cm-capture-region` | Capture a dragged region: brand crop corners plus a crosshair. Window capture uses `app-window` and display capture uses `monitor` |
| `cm-record` / `cm-stop` | Start / stop recording |
| `cm-webcam-bubble` | Webcam bubble over the screen |
| `cm-layout-screen` / `-bubble` / `-camera` / `-split` | Camera layout segments: Screen only, Bubble, Camera full, Side by side |
| `cm-corner-top-left` / `-top-right` / `-bottom-left` / `-bottom-right` | Bubble position segments (icon only, keep the aria labels) |
| `cm-trim-in` / `cm-trim-out` | Set the In / Out point |
| `cm-split` | Split at the playhead (timeline and caption lines) |
