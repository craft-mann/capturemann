# QA-0.3.4 Boards, Bulk export, CMYK import, Open With, tools count report, macOS (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent)
- **Machine:** Apple M4 Pro, macOS 26.6.2
- **Build:** dev bundle of main `26a05100`, isolated instance (`/tmp/cmq`)

| Item | Result | Evidence |
| --- | --- | --- |
| BRD-1 Pair, gap, margin, radius, shadow, captions, title, BEFORE/AFTER | PASS | 1888x794 board looked at (`QA-0.3.4-shots/board-pair-before-after.png`): BEFORE and AFTER pills sit above each picture, outside it; captions below; title above; the earlier D6 (labels over the picture) is not seen |
| Grid (3 columns, 5 images), Strip stacked, Free (overlapping slots) | PASS | sizes 2812x1288, 964x1884, 832x565; grid looked at: even cells, last row left-aligned, photo-like noise cell intact |
| Board sources copied as they are now | PASS | the pair shows the Zoom Lens that the source had at that moment |
| Make a board sheet (GUI, D1 of the earlier report), hint text under the canvas, Free drag and snap lines | not run | the GUI was not reached in this run |
| BLK-1 `export-images` 3 shots, longest 400, JPEG, watermark | PASS | three JPEGs written; a second run wrote `flat (2).jpg`, nothing overwritten |
| Bulk sheet (GUI), cancel, blueprint option | not run here | see 2026-11-14 report |
| IMP-6 CMYK JPEG | PASS | CMYK JPEG made with `sips -m` (space CMYK) imported with `prepared: ["converted"]`; the stored `original.png` looked at: layout intact, colours plausible (blue button duller, as CMYK gamut implies) |
| Open With: Info.plist document types | PASS (plist only) | `CFBundleDocumentTypes` lists video, audio and image (png, jpg) types, rank Alternate. `open -a ... file` was not run (it would hand a file to whichever copy owns the bundle id) |
| 89 MCP tools | PASS | `tools/list` through the bundle's `cm-mcp`: 89; includes `create_board`, `export_images`, `suggest_blueprint`, the five guide tools |
| Version label | PASS | sidebar reads `v0.3.4`; `capturemann --version` 0.3.4; MCP serverInfo 0.3.4 |
| Sidebar: no MCP card; Click Guides item | PASS | `QA-0.3.4-shots/library-click-guides.png` (Agents & CLI icon blue while the daemon runs) |
| Agents & CLI icon colour when MCP is not ready | not run | needs a second instance owning the socket |
| Library chips per page | PARTIAL | Click Guides page: no chips (PASS); All captures shows Imported, Uploaded, Published, Boards, Click Guides (seen); Audio, Recordings, Screenshots pages not looked at |
| Settings rows (Long Shot, New Click Guide), Accessibility card, launcher entries | not run | a click on Settings > Shortcuts lost focus to another agent's window; the launcher is hidden from capture. See the 2026-11-14 report |
| B1 delete a capture with a paused upload | not run | needs a destination and credentials |
| Canvas text selection (Select text tool) | not run | |

## Verdict

PARTIAL.
