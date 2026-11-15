# QA 0.3.4 Published page removal, macOS pass 4 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, macOS pass 4)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `a4f28a07`), isolated library, config and socket under `/tmp/cmq4`; only my own app instance was started and killed; my own Chrome profile (`--user-data-dir=/tmp/cmq4/chrome`), one Terminal window and a small AppKit fixture window (`qafix`, 5 buttons, a fake e-mail label) were used and closed. The maintainer's library, Chrome and Terminals were not touched (the window picker listed their windows; I only searched for my fixture).
- **FFmpeg:** not used.
- **Method:** pictures taken with `screencapture` of the app window; windows excluded from capture (Long Shot overlay, HUD, preview, bar, toasts, dialogs) were read through the accessibility tree and `CGWindowListCopyWindowInfo` rectangles, or marked NOT SEEN. Synthetic clicks (CGEvent) only work on a non-key window after a pointer move first. The Long Shot overlay bar has no accessibility children, so Hand scroll could not be driven (NOT RUN).

| Case | Result | Evidence |
| --- | --- | --- |
| No Published item in the sidebar | PASS | seen and AX: All captures, Recordings, Audio, Screenshots, Click Guides, AI models, Agents & CLI, Settings |
| Recordings page chips | PASS | AX tabs: Imported, Uploaded, Published, Needs attention (Imported also shows here) |
| Screenshots page chips | PASS | Imported, Uploaded, Boards (no Published) |
| All captures chips | PASS | Imported, Uploaded, Published, Needs attention, Boards, Click Guides |
| Old Published deep link | NOT RUN | no known link form |
| Recording details Published card and header pill | NOT RUN | no fixture with a publish record built |
| Chip row at the 720 pt minimum width | FAIL (low) | chips run past the window edge ("Needs a..." cut, Boards and Click Guides off screen), no wrap or scroll |

Verdict: PARTIAL.
