# QA 0.3.4 summary, macOS pass 4 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, macOS pass 4)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `a4f28a07`), isolated library, config and socket under `/tmp/cmq4`; only my own app instance was started and killed; my own Chrome profile (`--user-data-dir=/tmp/cmq4/chrome`), one Terminal window and a small AppKit fixture window (`qafix`, 5 buttons, a fake e-mail label) were used and closed. The maintainer's library, Chrome and Terminals were not touched (the window picker listed their windows; I only searched for my fixture).
- **FFmpeg:** not used.
- **Method:** pictures taken with `screencapture` of the app window; windows excluded from capture (Long Shot overlay, HUD, preview, bar, toasts, dialogs) were read through the accessibility tree and `CGWindowListCopyWindowInfo` rectangles, or marked NOT SEEN. Synthetic clicks (CGEvent) only work on a non-key window after a pointer move first. The Long Shot overlay bar has no accessibility children, so Hand scroll could not be driven (NOT RUN).

Reports: long-shot, click-guide, boards-bulk, published-recordings, editor-release-items (`QA-0.3.4-<topic>-macos-pass4-2026-11-15-claude.md`). Pictures: `QA-0.3.4-shots/macos-pass4-*.png` (fixtures only).

## Per topic

| Topic | Result |
| --- | --- |
| Long Shot | Auto, Esc, region, horizontal, covered window, during a recording, CLI passive HUD, kill -9 pass; **left-sidebar page FAILS (LS-1)**; Hand scroll and HUD text not run |
| Click Guide | core flows pass (5 steps, pause, note, undo, close toast, Stop and open, export, rich paste, kill -9 recovery, no redaction UI); known gaps reproduced (move, agent bar, escaping, titles) |
| Boards and Bulk | CLI renders good, bulk rules pass; sheet UI not run |
| Published removal | pass; recording details card not run |
| Editor items | not run |

## Defects

| Id | Severity | Steps | Evidence |
| --- | --- | --- | --- |
| LS-1 | high | Chrome 770x860 pt page with fixed left sidebar + edge strip, Long Shot Auto on the window | later frames show window shadow/grey on the left (~58 px) and right (~110 px), edge strip lost; macos-pass4-long-shot-left-sidebar-seam.png |
| CG-1 | medium (known) | window guide, move the window 100 pt, click Save | step titled Export, ring on Export, click stored from the old origin |
| CG-2 | medium (known) | guide start from the CLI, Allow | no bar, toast without Stop and saying MCP, tile not running |
| LS-2 | medium | move the window during an Auto run | toast "Stopped where the page jumped" |
| LS-3 | medium (known) | Cmd+Q during a run | image lost without warning |
| CG-3 | medium-low (known) | export Markdown with link/image text in titles; two guides to one folder | injection; overwrite + stale pictures; 0-step "success" |
| CG-4 | low-medium (known) | click page text | title holds 40 chars of the text |
| CG-5, CG-6, CG-7 | low | editor at 720 wide; viewer label 1,400 x 933 vs 2,400 x 1,600; click-through on Export Markdown; silent kill -9 recovery | see click-guide report |
| BB-1, BB-2 | low | library cards, pair board labels | no S11 badges or Long Shots chip; flat pills instead of S10 stamps |
| LS-4..6, chips at 720 | low | user-limit wording, start position, scrollbar thumbs, clipped chip row | see reports |

## UX proposals

| Proposal | Why | Suggested fix |
| --- | --- | --- |
| Name the real stop reason in Long Shot toasts | "page jumped" for a moved window, "longest image" for a user limit | add reasons window_moved / user_limit |
| Warn before quitting during a Long Shot or a guide | image and frames lost silently | confirm on Cmd+Q / Quit |
| Show a bar or tray Stop for agent-started guides and Long Shots | user cannot stop an agent guide | reuse the bar with an Agent chip |
| Long Shot: offer "start from top" and restore scroll | starts mid-page, leaves page moved, one frame at a bottom | toggle in the bar |
| Re-resolve the followed window rect on every click, or pause with a notice when it moves | wrong steps | read the rect at mouse-down |
| Escape Markdown in titles and never overwrite a folder silently | injection and stale pictures | escape, or ask / use a subfolder |
| Tell the user after crash recovery of a guide | silent | toast "Click Guide kept with n steps" |
| Make the chip row and editor fit at 720 pt | clipped | wrap or scroll |
| Add S11 badges and decide stamp vs pill | design drift | follow S10/S11 or update them |
