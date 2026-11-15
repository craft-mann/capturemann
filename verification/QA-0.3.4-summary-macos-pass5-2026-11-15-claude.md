# QA 0.3.4 summary, macOS pass 5 (PARTIAL, paused by the maintainer)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 5). Apple M4 Pro, macOS 26.6.2. Dev bundle of main a4f28a07, isolated library under /tmp/cmq5. Nothing is called "verified".

Reports (`QA-0.3.4-<topic>-macos-pass5-2026-11-15-claude.md`): editor-release-items, library-boards-bulk-ui, published-recordings, long-shot-hand-terminal, click-guide-scale, vietnamese. Pictures: QA-0.3.4-shots/macos-pass5-*.png.

| Topic | Result |
| --- | --- |
| Editor: text colour AUTO (light, dark, busy, hand colour, CLI), Retype, Move area, Blueprint, Zoom Lens, canvas text selection | PASS with low defects; Select text G not confirmed; Retype busy sheet not run |
| Library selection, Make a board, Export selected | PASS with notes; B1, progress, Agents offline not run |
| Published recording details | fixture built, NOT RUN |
| Long Shot Hand and HUD | NOT RUN (overlay not drivable); Terminal PASS on a correct fixture |
| Click Guide 100 / 500 | partial; 500 run invalid |
| Vietnamese | NOT RUN |

## Defects

| Id | Severity | Steps | Evidence |
| --- | --- | --- | --- |
| D-ED1 | low | Text tool Auto after a commit or a hand colour | Auto highlighted and a swatch ringed together; the result line loses "(light)" |
| D-ED2 | low | Retype, longer text | box widens, text inside stays old until Enter |
| D-ED3 | low | Text tool click outside the picture | text placed and cut off |
| D-ED4 | low | Blueprint, keep lines | badge hides the line start; Apply below the fold |
| D-LIB1 | low | Esc after closing the Export sheet | selection stays |
| EX-1 | low | Choose folder in the Export sheet | panel opened on a folder from another session and set it |
| LS-7 | low-medium | Long Shot via CLI on a window the app window covers | "Nothing scrolled" |
| LS-8 | low | Terminal after `clear` | one-frame image |
| CG-8 | medium | guide clicks over other apps | step titles carry text from other apps' elements (known CG-4 family) |
| Select text G | unknown | drag, double-click, Cmd+A | no highlight or clipboard in my runs |

## UX proposals

| Proposal | Why | Suggested fix |
| --- | --- | --- |
| Move area: Delete after a move should remove the contents, or be labelled "Undo move" | "Remove contents" restores the circle | two buttons |
| Select text: explain in the panel | panel is empty | one help line and a visible selection |
| Pin Apply Blueprint | below the fold | sticky footer |
| Strip board with a tall image | sliver | limit the height ratio |
| Board sheet: S10 drop tile and drag reorder | missing | add them or update the design |
| Long Shot from the CLI: send the app window behind the target | covered target | back the window |

## Cleanup
Click loop, guide and my app instance stopped; my Terminal window closed; my Chrome profile killed. Temp files remain under /tmp/cmq5 (a bulk removal was blocked); the clipboard holds fixture text. The 500-click guide bundle there holds private text and was not committed.
