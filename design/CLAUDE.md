# CaptureMann — Design Handoff for Coding Agents

> **Redesign (Oct 23, 2026, OD-16 to OD-20 in `specs/design-implementation.md`):** the app UI is being rebuilt to `design/redesign/` (read its `CLAUDE.md` first) with the icons in `design/icons/`. Where this file and the redesign disagree about layout, navigation, the primary button colour or icons, the redesign wins; tokens, fonts and the brand below still apply. Plan: `specs/redesign-plan.md`.

This folder is the source of truth for CaptureMann's brand and UI (v0.1). Read this file first. If you are building the app, website or docs, follow these specs. Don't invent new colors, fonts or logo variants.

**Product:** CaptureMann v0.1 is a free, open-source (Apache-2.0) desktop app for Windows and macOS. It takes screenshots and records the screen with a webcam bubble, at up to the display's native resolution. It saves to local or cloud drives and publishes to YouTube, TikTok and Facebook. AI agents can drive every feature through MCP and a CLI.

**UI references:** Raycast (keyboard-first command launcher) and Runway (dark media editor with preview, inspector and timeline).

---

## 1. Folder map

```
capturemann-design/
├── CLAUDE.md                 ← you are here (specs + rules)
├── README.md                 ← human overview
├── tokens/
│   ├── tokens.json           ← canonical tokens (W3C DTCG format)
│   ├── tokens.css            ← CSS custom properties, dark default + [data-theme="light"]
│   ├── tailwind.preset.js    ← Tailwind preset mapped to the CSS variables
│   └── CaptureMannTheme.xaml ← WPF / WinUI / MAUI / Avalonia ResourceDictionary
├── fonts/                    ← Geist + Geist Mono 1.8.2 (SIL OFL 1.1, see OFL-LICENSE.txt; covers Vietnamese. The *-Variable.woff2 files used by the reference HTML screens are still 1.2)
├── brand/
│   ├── logo/svg/             ← mark, wordmark, lockups (text converted to outlines, no font needed)
│   ├── logo/png/             ← raster exports
│   ├── app-icon/
│   │   ├── svg/app-icon.svg        ← full-bleed 1024 (Windows, Linux, web, stores)
│   │   ├── svg/app-icon-macos.svg  ← Apple grid: 824 body on a 1024 canvas, with shadow
│   │   ├── svg/app-icon-small.svg  ← heavier strokes, use at ≤32 px
│   │   ├── AppIcon.icns            ← macOS bundle icon
│   │   ├── AppIcon.iconset/        ← source set for `iconutil -c icns`
│   │   ├── app.ico                 ← Windows (16, 24, 32, 48, 64, 128, 256)
│   │   └── png/{macos,windows}/    ← every size incl. MSIX/Store tile sizes (44, 71, 150, 310…)
│   ├── tray/
│   │   ├── png/TrayTemplate.png, TrayTemplate@2x.png, @3x  ← macOS menu bar (template image)
│   │   ├── png/tray-recording-on-*.png                     ← macOS recording state (NOT template)
│   │   ├── tray-{light,dark,recording-on-light,recording-on-dark}.ico ← Windows notification area
│   │   └── svg/
│   └── favicon/              ← favicon.svg, favicon.ico, apple-touch-icon.png, 192/512 PNG
└── screens/
    ├── screens.json          ← manifest: id, title, size, responsive flag, file paths
    ├── png/                  ← @2x renders (+ -mobile.png at 390 px for responsive screens)
    ├── html/                 ← static HTML for each screen, inline styles; open in a browser
    └── source/               ← original .dc.html design files (template syntax: {{holes}}, <sc-for>, <sc-if>)
```

**How to use the screens:** Use `screens/png/*.png` as the visual target. For exact values (padding, colors, font sizes, radii, gaps), use `screens/html/*.html`. Every style is inline, so reading it is like reading a spec. Don't ship this HTML as-is. Rebuild it as real components that use the tokens.

## 2. Screen index

| id | Screen | Size | Notes |
|---|---|---|---|
| `Main` | Logo & app icon sheet | 1440×900 | Brand reference only |
| `Brand` | Color, type & voice | 1440×900 | Brand reference only |
| `Launcher` | Command launcher (Raycast-style) | 1280×800 (window 760 wide) | Global hotkey palette |
| `Recording` | Recording HUD + webcam bubble | 1280×800 | Overlay on the screen being recorded |
| `Editor` | Library & editor (Runway-style), dark | fluid, 1440 ref | Main window |
| `EditorLight` | Same, light theme | fluid | Shows the light-theme rules |
| `Settings` | Settings → Destinations & Agents | fluid | Drives, social accounts, MCP permissions |
| `Landing` | Website hero | fluid, max-width 1200 | Marketing site |
| `Social` | GitHub social preview | 1280×640 | Upload `screens/png/Social.png` in GitHub repo settings |

## 3. Brand rules (non-negotiable)

1. **The mark** is a capture frame. Three crop corners plus a solid circle in the bottom-right corner. The circle is the webcam bubble, and it doubles as the record light. Never rotate the mark, move the bubble to another corner, add gradients or outline the bubble.
2. **Red (`#FF4F3A`, "Rec") is reserved.** Use it only for the bubble, record/stop controls, the recording state and the destructive confirmations (OD-17: the primary action is neutral, not red). Never use it for errors. Errors use `warning`/text. Text on Rec is always ink `#0A0A0B`, never white (white fails contrast).
3. **Blue (`#5B8CFF`, "Agent")** means "an AI agent / MCP / CLI did this". Use it for MCP status, agent-triggered toasts and agent sections. Don't use it as a generic link color.
4. **Dark-first.** Dark is the default theme. In the light theme, the primary button is ink (`#0A0A0B`) with white text (OD-17: no red dot, red stays for record, stop and destructive confirmations).
5. **Type:** Geist for UI and display, Geist Mono for timecodes, shortcuts, paths, tool names and tiny uppercase labels. Display text uses weight 600 with letter-spacing −0.045em. Headings use −0.03em.
6. **Wordmark:** "CaptureMann", camel case, one word. Use the SVG lockups. Never retype it in another font.
7. **Clear space** = the bubble diameter on every side. Minimum size is a 16 px mark or a 96 px wide lockup.
8. No emoji and no gradient washes. Icons are 2 px stroke, round caps (Lucide-style; `lucide` is a good library match).

## 4. Tokens (summary — see tokens/tokens.json)

| Token | Dark | Light | Use |
|---|---|---|---|
| ground | `#0A0A0B` | `#F6F6F4` | Window background |
| sidebar | `#0E0E10` | `#EFEFEC` | Sidebar, footers |
| surface | `#111113` | `#FFFFFF` | Cards, panels |
| raised | `#1E1E22` | `#F6F6F4` | Selected nav, hover, keycaps |
| border | `#222226` | `#E4E4E0` | Card borders |
| line | `#2E2E34` | `#D6D6D1` | Strong borders, dividers |
| text / 2 / 3 | `#EDEDEF` / `#C9C9CF` / `#A1A1A9` | `#0A0A0B` / `#2A2A30` / `#44444B` | Text hierarchy |
| mist | `#8B8B93` | `#55555C` | Captions (lowest contrast allowed for small text) |
| rec | `#FF4F3A` | `#FF4F3A` | Record / stop / destructive (OD-17) |
| agent | `#5B8CFF` | `#3B6FE8` | MCP / agent |
| success / warning | `#3DD68C` / `#F5B84A` | — | Status chips |

Radii: 6 (keycaps), 8 (nav), 10 (buttons/rows), 14 (cards), 16 (launcher/HUD), 999 (pills). Spacing is on a 4 px grid. Controls are 40 px tall, and hit targets are at least 44 px (list rows 48).

## 5. Components

- **Primary button (OD-17):** 40 h, radius 10, padding 0 18, weight 600 / 14 px. Dark: bg text (`#EDEDEF`), text ink. Light: bg ink, text white. **Record / Danger button:** same size, bg rec, text ink, used only for Record, Stop and destructive confirmations.
- **Secondary button:** 40 h, radius 10, 1 px `line` border, bg `surface`, text `text`.
- **Nav item (sidebar):** 40 h, radius 8, padding 0 10, 14 px. Active: bg `raised` (dark) / white + border (light). Optional right-aligned count in `mist` 12 px.
- **Card:** bg `surface`, 1 px `border`, radius 14, padding 16. Section label 12 px/500 in `mist`.
- **Launcher row:** min-h 48, radius 10, padding 10 12, gap 14. Contains a 30×30 icon tile (radius 8; selected row's tile = rec bg with ink glyph), a title (15 px), a right-aligned hint (13 px `mist`) and a keycap (Geist Mono 12 px, bg `raised`, radius 6, padding 3 7). Selected row bg `#232328`. Group headers are 12 px/500 `mist`.
- **Launcher window:** 760 wide, radius 16, bg `rgba(17,17,19,0.96)` + blur, 1 px `line`, shadow float. Search input 20 px. Footer 13 px with ↵ and ⌘K keycaps.
- **Recording HUD:** floating bar, radius 16, padding 6, gap 6, 44×44 icon buttons (radius 11). Order: Stop (rec bg, 14 px ink square), timer (mono 15 px with an 8 px rec dot), divider, Pause, Mic, Webcam, Draw, divider, Discard. Selected-area corners use the logo's crop brackets (4 px white, radius 6). The size label is mono 12 px.
- **Webcam bubble:** circle, 4 px rec ring, shadow `0 12px 30px rgba(0,0,0,.5)`. Default 168 px at 1280 wide (≈18% of preview width), adjustable 8–40%, shape Circle / Rounded / Off.
- **Agent toast:** surface bg, 1 px `rgba(91,140,255,.45)` border, radius 12, 8 px agent dot, text "Started by agent via MCP" plus the tool name in mono `agent-text`.
- **Status pill:** radius 999, 1 px line, 12 px, dot + label (success / mist / warning).
- **Timeline:** 34 px track bars, radius 8, 80 px track label column. Screen = neutral, Webcam = rec at 22% fill with a 60% border, Mic = waveform stripes. The playhead is a 2 px `text` line.

## 6. Layout rules

- The main window and Settings are a flex-wrap row: sidebar 220 px + content `flex: 999 1 560px`. On narrow widths, the sidebar stacks above the content.
- The Editor content is preview (16:9, `flex: 999 1 520px`) + inspector (`flex: 1 1 300px`), with the timeline full-width below in an `overflow-x: auto` box.
- The website uses a max-width 1200 container with 24 px gutters. The H1 is `clamp(44px, 7vw, 88px)`.

## 7. Platform asset wiring

- **macOS app bundle:** `brand/app-icon/AppIcon.icns` (or regenerate it with `iconutil -c icns brand/app-icon/AppIcon.iconset`).
- **macOS menu bar:** `brand/tray/png/TrayTemplate.png` + `@2x` + `@3x`, set as a template image (Electron: a filename ending in `Template`; Swift: `isTemplate = true`). While recording, switch to `tray-recording-on-dark.png` (non-template, so the red shows).
- **Windows exe / installer:** `brand/app-icon/app.ico`. MSIX/Store tiles: `brand/app-icon/png/windows/app-icon-{44,71,150,310}.png`.
- **Windows notification area:** `brand/tray/tray-dark.ico` (dark taskbar) / `tray-light.ico`, and the matching `tray-recording-on-*.ico` while recording.
- **Web:** `brand/favicon/favicon.svg` + `favicon.ico` + `apple-touch-icon.png`. OG/social image: `screens/png/Social.png`.
- **In-app logo:** `brand/logo/svg/mark-currentcolor.svg` (inherits `color`; bubble uses `--cm-rec`).

## 8. Placeholders to replace (do not ship as-is)

These show up in the screens as `[BRACKETED]` text. Replace them with real values or wire them to data:

- `[CLOUD DRIVE]`, `[DEFAULT DRIVE]`, `[~/Movies/CaptureMann]`: the actual default save path / selected drive
- `[CHANNEL NAME]`, `[AGENT CLIENT]`: runtime data
- `[PATH TO CLI]`, `[github.com/OWNER/capturemann]`, `[YOUR APP / SCREEN CONTENT]`, `[RECORDING PREVIEW]`, `[PRODUCT SCREENSHOT]`
- Google Drive / OneDrive / Dropbox are **assumed** cloud destinations. Confirm the list with the product owner.
- The CLI commands (`capturemann record --webcam bubble --res native`), the MCP tool names (`capture.screenshot`, `record.start`, `record.stop`, `save.cloud`, `publish.*`), the shortcuts (⌥⇧R, ⌥⇧5, ⌥⇧3) and the stats (2560×1440, 60 fps, 1.2 GB) are **illustrative**. Use the real API names once they're defined.
- Platform names are plain text. If you add official YouTube/TikTok/Facebook logos, follow each platform's brand guidelines.

## 9. Voice

- Tagline: **"Capture anything. Ship it anywhere."** Alt: "Your screen, your agents, your channels."
- Principles: Native, not resized · Local-first · Agent-ready · Open by default.
- Tone: short, direct, verb-first labels ("Record Region", "Publish Last Recording…"). Sentence case everywhere except the wordmark.
