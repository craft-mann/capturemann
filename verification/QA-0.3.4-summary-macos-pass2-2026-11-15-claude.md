# QA-0.3.4 summary, macOS, second pass (PARTIAL)

- **Date:** 2026-11-15, Claude (QA agent). Dev bundle of main `84bc00cd`, isolated instance, Apple M4 Pro, macOS 26.6.2. Real mouse/key events. No source file was changed.
- **Limit that shaped the pass:** every floating window (Click Guide bar, countdown, toasts, launcher, window picker, Long Shot bar, confirm dialogs) is excluded from screen capture, so they could be measured (size, position) but not looked at.

| # | Item | Report | Result |
| --- | --- | --- | --- |
| 1 | Filter chips per page | `QA-0.3.4-library-chips-select-board-macos-pass2-2026-11-15-claude.md` | PASS |
| 2 | Hover-select, bar, X, untick | same | PASS (remarks L1 to L4) |
| 8 | Make-a-board sheet | same | PASS |
| 3 | Settings Long Shot row | `QA-0.3.4-settings-launcher-agents-macos-pass2-2026-11-15-claude.md` | PASS; launcher entries NOT SEEN (menu bar entries seen) |
| 4 | Accessibility card | same | PARTIAL (granted state only) |
| 5 | Agents icon ready/offline, MCP card gone | same | PASS |
| 6 | Canvas text selection | `QA-0.3.4-editor-flows-macos-pass2-2026-11-15-claude.md` | PARTIAL: word, Shift+arrows, Cmd+A pass; triple-click selects a word only (E1) |
| 7 | Retype, Move area, Blueprint, Zoom Lens, Keep-by-click | same | PASS with defects E3, E4 |
| 9 | Click Guide bar vs S3, window start, G1, local-time title | `QA-0.3.4-click-guide-macos-pass2-2026-11-15-claude.md` | bar contents NOT SEEN; title PASS; G1 PARTIAL; stale bar after CLI stop (C1) |
| 10 | Long Shot Chrome, Terminal, limit, Esc | `QA-0.3.4-long-shot-macos-pass2-2026-11-15-claude.md` | PARTIAL: Chrome 1 of 2 runs complete, Terminal ended early, limit PASS, Hand NOT RUN, Esc inconclusive |
| 11 | Open With | `QA-0.3.4-open-with-vietnamese-macos-pass2-2026-11-15-claude.md` | PASS (running app) |
| 12 | Vietnamese | same | PASS except one clipped sidebar label |

## Defects (new in this pass)

- **E3 (medium):** clicking "Use anyway" or "Cancel" on the Retype / Move area busy-background sheet also hits the canvas under it (it retyped the "Save" line, created a Spotlight). Not fixed.
- **C1 (medium, unconfirmed):** a Click Guide bar stayed open after `guide stop` from the CLI; its Stop then showed an error toast.
- **LS-T (medium):** Long Shot Auto in Terminal reported `reached_end` with about 75 rows left; Chrome run 1 stopped with `no_overlap` after 3 of about 9 frames.
- **E4 (low-medium):** the Retype panel's Text field takes no typed characters (only the on-canvas box does).
- **E1 (low-medium):** triple-click in a canvas text box does not select all text.
- **L2 (low-medium):** the export-selected sheet is taller than the window (footer cut).
- **G1 (low-medium, partly fixed):** guide editor picture still cut about 25 pt at the bottom.
- Low: L1 (selection stays after Make a board), L3 (no ring on selected card, amber Delete), L4 (Duration sort on Screenshots), E2 (white default text colour), A1 (misleading "Another app owns the socket"), Vietnamese sidebar label "Click Guides" clipped, Accessibility card uses the Input Monitoring icon.

## Not run (whole)

Click Guide bar contents, picker, launcher entries, Hand scroll, Long Shot bar, the not-granted Accessibility and Input Monitoring cards, cold-launch Open With, bulk export to a folder, Esc during a Long Shot from the overlay.

Pictures: `verification/QA-0.3.4-shots/macos-pass2-*.png` (fixtures only; guide editor pictures were not kept because they show other windows).
