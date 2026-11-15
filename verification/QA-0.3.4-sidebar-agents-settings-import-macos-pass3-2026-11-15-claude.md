# QA 0.3.4 sidebar (S14), Agents & CLI status, settings, launcher, CMYK and Open With, long library path, Vietnamese, macOS pass 3 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, pass 3)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `5e832011`), isolated libraries and configs under `/tmp/cmq3` (one per case); the maintainer's app was not running; every instance I started was ended by its own process id.
- **Fixture:** the same 5 pictures, `cmyk.jpg` (4 components, 900 x 600).

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Sidebar rest: Long Shot and Click Guide tiles, 2 per row under Region / Window / Display | PASS with defect | 54 pt tiles (accessibility 99 x 54). **The Long Shot tile has no icon** (S1); Click Guide has its icon |
| Hover and tooltip | NOT SEEN | tooltips not looked at |
| Active (border and rec dot) | PASS | while a Click Guide ran, the Click Guide tile had a red border and a red dot (accessibility name "Click Guide, running") |
| Disabled (40 % opacity, reasons) | NOT RUN | no recording started; Long Shot stayed enabled while a Click Guide ran |
| Rail mode (editor) | PASS with defect | the rail lists Record, Region, Mic, Long Shot, Click Guide...; the Long Shot button is accessible ("Long Shot", 44 x 44) and starts the overlay, but is **invisible** (no icon) |
| Agents & CLI: ready dot | PASS | blue dot on the icon; accessibility "Agents & CLI, MCP ready"; page header "MCP ready" |
| Offline (hollow ring) | PASS | socket folder not creatable: ring; header "MCP offline", reason "Could not create the socket folder"; accessibility "Agents & CLI, MCP offline" (`macos-pass3-agents-offline-ring.png`) |
| Socket owned by another app ("!" badge) | PASS | a python process held the socket: amber "!" badge, accessibility "...another app owns the socket", header "MCP offline" and "Another app owns the socket" (`macos-pass3-agents-socket-owned.png`) |
| MCP card gone | PASS | the page shows Client config, Tool permissions and Clients; no separate MCP card |
| Settings, Shortcuts: Long Shot row | PASS | "Long Shot, Not set, [Set]" (the row uses the capture-region icon) |
| Settings, Permissions | PARTIAL | 5 of 5 granted. The Accessibility card **not-granted** state and the Input Monitoring not-granted card: NOT RUN (the isolated config cannot reset the TCC grants of the bundle) |
| Launcher | PARTIAL | `Option+Shift+Space` opens the launcher (760 x 786); rows are not in the accessibility tree (NOT SEEN); typing "long shot" + Return started Long Shot, "guide" + Return started a Click Guide; no entry matched "click guide of a window" |
| App menu | PASS | Library, Long Shot, New Click Guide, Click Guide of a window..., Import file..., Settings..., Quit |
| CMYK JPEG import (CLI) | PASS | imported as a screenshot, 900 x 600, colours a little duller than the RGB file (expected for the conversion) |
| Open With, cold launch (`open -n --env ... -a CaptureMann-dev.app cmyk.jpg`) | PASS | the app opened with an empty isolated library and the Import sheet: "cmyk.jpg · 900x600 · JPEG · 460.6 KB", chip "Converted", note "cmyk.jpg is saved as a PNG (colours converted to 8-bit RGB). Camera and location data are not copied."; the preview square shows a generic image icon (low) (`macos-pass3-open-with-cold-launch-cmyk.png`). The Import button was pressed; the result was not looked at |
| Open With into a running app | NOT RERUN | pass 2 PASS |
| Library path too long (1115 characters, five 200 character folders) | PASS with remarks | the app opened at once (no hang); All captures shows an error banner "Library unavailable - connect <path>" and "(The library folder's path is too long (1115 characters; at most 944 work, and no folder name may be longer than 255): choose a shorter path)" (`macos-pass3-library-path-too-long.png`). Remarks (low): the path is printed twice and fills the page, the helpful sentence is at the end; the wording "connect <path>" is odd for this cause |
| Vietnamese: sidebar | PASS | "Hướng dẫn nhấp" now fits (pass 2 clipped it); dock tiles "Chụp dài", "Hướng dẫn" fit |
| Vietnamese: Click Guides page | PASS with a wording remark | the header button reads "Hướng dẫn theo thao tác mới"; the design's draft was "Click Guide mới" (strings are drafts) |
| Vietnamese: selection bar, bulk export sheet, picker, Click Guide bar | PASS | no clipping seen; footer visible; the bar is 672 x 58 with the Undo label collapsed to an icon (see Click Guide report) |
| Vietnamese: Long Shot HUD, S18 strings, Board editor | NOT SEEN / NOT RUN | |
| Default titles use local time | PASS | "Click Guide 2026-11-15 11.04", "Screenshot 2026-11-15 11.10" match the clock (UTC+7); the manifests store UTC |

## Problems found

- **S1** (medium): Long Shot tile and rail button without an icon (source: `long-shot.svg` is not registered in `apps/desktop/src/ui/paint.rs`).
- Low: library path banner prints the path twice; the CMYK sheet preview is a generic icon.
