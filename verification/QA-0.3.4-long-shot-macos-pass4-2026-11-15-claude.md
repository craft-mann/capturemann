# QA 0.3.4 Long Shot (CAP-6), macOS pass 4 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, macOS pass 4)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `a4f28a07`), isolated library, config and socket under `/tmp/cmq4`; only my own app instance was started and killed; my own Chrome profile (`--user-data-dir=/tmp/cmq4/chrome`), one Terminal window and a small AppKit fixture window (`qafix`, 5 buttons, a fake e-mail label) were used and closed. The maintainer's library, Chrome and Terminals were not touched (the window picker listed their windows; I only searched for my fixture).
- **FFmpeg:** not used.
- **Method:** pictures taken with `screencapture` of the app window; windows excluded from capture (Long Shot overlay, HUD, preview, bar, toasts, dialogs) were read through the accessibility tree and `CGWindowListCopyWindowInfo` rectangles, or marked NOT SEEN. Synthetic clicks (CGEvent) only work on a non-key window after a pointer move first. The Long Shot overlay bar has no accessibility children, so Hand scroll could not be driven (NOT RUN).
- **Fixtures:** (a) `side.html`: 120 unique paragraphs, a position:fixed red edge strip (14 css px) and a fixed green left sidebar (200 px) with 12 nav items, left margin 260 px; (b) `page2.html`: 120 paragraphs, sticky header, no left content; (c) a 12000 px wide page of 40 columns; (d) a Terminal with 300 numbered lines. Chrome window 770 x 860 pt (1540 x 1720 px).

## Steps

| Case | Result | Evidence |
| --- | --- | --- |
| Start from sidebar tile (now has an icon), overlay opens, click a window | PASS | overlay window `CaptureMann overlay` layer 1000 0,0 1512x982 |
| **Left-edge regression check (fixed left sidebar page), Auto, window target** | **FAIL** | output width 1540 = window width x 2 (full width kept, normal screenshot of the same window is also 1540). Frame 1 (top 1720 px) is identical to the normal screenshot at the left edge (red strip, green sidebar, "Nav item 1..4"). But every later frame has the **left about 58 px replaced by the window shadow and desktop grey** (the red 14 px edge strip and about 30 px of the sidebar are lost), the sidebar green is lighter, and the **right about 110 px are grey** too; the seam at y=1720 shows the window's rounded corners. Text columns stay aligned (Para text starts at x=522 in both). Pictures: `QA-0.3.4-shots/macos-pass4-long-shot-left-sidebar-seam.png`, `macos-pass4-long-shot-left-edge-zoom.png`. Same run on a page with no fixed left content: seam clean, left margin white (`macos-pass4-long-shot-plain-page-seam-ok.png`) |
| Auto, unique text, sticky header (page2) | PASS | 1540 x 16384, 14 frames, `limit_reached`; Para numbers continuous, no repeat; toast text read: "Long Shot saved at the size limit. It reached 16,384 px, the longest image CaptureMann can edit. Everything up to there is kept." [Open, Copy, Dismiss] |
| Page reaches its end | PASS | manifest `reached_end`, 7 frames, 1540 x 6645 |
| Esc | PASS | toast read: "Long Shot saved. Stopped with Esc. Everything up to here is kept: 2 frames, 1,540 x 2,236." [Open, Copy] (Esc 1 s after click) |
| Region drag 400 x 400 pt + Enter | PASS | image 800 px wide (= 400 x 2), reached 16384 |
| Horizontal (CLI, whole browser window) | PASS | 16384 x 1542 px (toolbar cut, 178 px), 14 frames, Col 011 to 015 in order. Page scrollbar thumb is stamped at the bottom of every frame (low) |
| Covered window (a second Chrome window over the page centre) | PASS | scrolled and stitched cleanly, reached_end, no cover pixels in the image |
| **Window moved by 100 pt during an Auto run** | PARTIAL / wording FAIL | run ended with 2 frames, 1540 x 2236, toast "Stopped where the page jumped. Auto scroll stops here and keeps 2 frames. Try Hand scroll for pages that redraw while scrolling." (text read). Frames before are kept; the wording names the wrong cause |
| Second start while a run is on (click the sidebar tile again) | PASS (no second overlay) / no feedback | window list: only HUD bar 570 x 56 at 840,828 and preview panel 360 x 860 (layer 101); no overlay, no toast (known G-LS-8) |
| Long Shot while a screen recording runs | PASS | recording of one window (CLI, camera off, mic off) + overlay Long Shot: image 1540 x 16384; recording saved 23 s, tracks screen, cursor; warning "48.4% of screen frames were late over the last 30 s" (load while the Long Shot ran) |
| CLI Auto + confirm dialog + passive HUD | PASS (size) | dialog "Allow this action? Capturemann Cli is asking to run Take screenshots..." 520 x 365/387, then bar 340 x 56 at the bottom of the area. `--max-px 4000` gave 1540 x 4000, `limit_reached`, warning "Reached the longest image CaptureMann can edit" (wrong for a user limit, low); JSON `kind: "region"` for a window target |
| Quit (Cmd+Q) during a run | FAIL as predicted (G-LS-2) | app quit at once, no warning, no image stored (library unchanged) |
| kill -9 during a run | PASS | no half capture, no leftover process (no cm-overlay), relaunch clean; log "previous run did not end normally" |
| HUD sizes | PASS (measured) | bar 570 x 56, preview 360 x 860, passive bar 340 x 56 |
| Terminal | INCONCLUSIVE | at the bottom of its scrollback: 1 frame (nothing to scroll); after Cmd+Home the first screen was blank: `no_overlap`, 1 frame. Fixture mistake, not run again |
| Hand scroll, HUD text/dot/"Keep scrolling", amber state, end toasts for Hand, sidebar tile states while running, display target, non-Chrome apps, 50 MP limit, Vietnamese, limit of other cases | NOT RUN / NOT SEEN | overlay bar and HUD expose no accessibility children |

## Problems found

- **LS-1 (high)**: fixed-left-sidebar page: frames after the first show the window shadow/desktop grey at both sides (left about 58 px, right about 110 px) and the left edge strip is lost. Full width is kept, so the earlier "lost left content" may be this same effect or a neighbour of it. Steps: Chrome window 770 x 860, page with position:fixed left sidebar, sidebar tile, click the window.
- **LS-2 (medium)**: moving the window mid-run ends the run with "Stopped where the page jumped".
- **LS-3 (medium, known)**: Cmd+Q during a run discards everything without a warning.
- **LS-4 (low)**: end wording for a user limit and for the 50 MP case says "longest image CaptureMann can edit".
- **LS-5 (low)**: a run starts from the current scroll position (no jump to top) and leaves the page where it ended; a Terminal at the bottom gives one frame without a hint (see proposals).
- **LS-6 (low)**: scrollbar thumbs and window corner notches are stamped into the stitched image.

## Verdict

PARTIAL. LS-1 fails the key regression check on a page with fixed left content; plain pages stitch cleanly.
