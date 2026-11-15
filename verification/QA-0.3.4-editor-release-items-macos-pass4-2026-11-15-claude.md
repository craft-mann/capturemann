# QA 0.3.4 editor and other release items, macOS pass 4 (PARTIAL, mostly NOT RUN)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, macOS pass 4)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `a4f28a07`), isolated library, config and socket under `/tmp/cmq4`; only my own app instance was started and killed; my own Chrome profile (`--user-data-dir=/tmp/cmq4/chrome`), one Terminal window and a small AppKit fixture window (`qafix`, 5 buttons, a fake e-mail label) were used and closed. The maintainer's library, Chrome and Terminals were not touched (the window picker listed their windows; I only searched for my fixture).
- **FFmpeg:** not used.
- **Method:** pictures taken with `screencapture` of the app window; windows excluded from capture (Long Shot overlay, HUD, preview, bar, toasts, dialogs) were read through the accessibility tree and `CGWindowListCopyWindowInfo` rectangles, or marked NOT SEEN. Synthetic clicks (CGEvent) only work on a non-key window after a pointer move first. The Long Shot overlay bar has no accessibility children, so Hand scroll could not be driven (NOT RUN).

| Item | Result | Evidence |
| --- | --- | --- |
| CMYK import | PASS | converted to RGB, see boards report |
| Default titles in local time | PASS | Screenshot 2026-11-15 14.33 and Click Guide titles in UTC+7 |
| Sidebar Long Shot tile icon (earlier S1 defect) | PASS | icon visible |
| Agents & CLI tile status | seen in the sidebar ("Agents & CLI, MCP ready") | other shapes NOT RUN |
| Retype (Vietnamese marks), Move area, Blueprint, Zoom Lens, canvas text selection, text default colour AUTO (CLI annotate has no UI default), library hover-select, Open With, Settings rows, launcher entries, long library path error, Vietnamese clipping, menu items | NOT RUN | time and the editor needs a driven pointer session; pass 2 and 3 reports cover part of them |

Verdict: PARTIAL (not run).
