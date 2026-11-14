# CAP-6 Long Shot: manual script

Run per system on the installed build. It scrolls real windows: do it at the machine, with
nothing private on screen. Report in `verification/` (template in its README); say "verified"
only with that report.

Setup: a page 5 screens long with a numbered line on every row (`tests/fixtures/long-shot/`),
open in Chrome. Also: Excel or LibreOffice Calc with 500 numbered rows, Slack or Discord with a
long thread, a PDF viewer with a 10-page PDF.

| # | Steps | Pass |
| --- | --- | --- |
| 1 | CLI: `capturemann capture screenshot --long --window <Chrome id>` and approve the dialog (agent) or run as the user. | Every number once and in order (AC1; `capturemann text <id>` to check), result `long_shot.end` = `reached_end`. |
| 2 | Same page with `?bands=1` (fixed header and footer). | Header once at the top, footer once at the bottom (AC2). |
| 3 | Page with `?jump=1`. | Stops with "Stopped: the next part did not line up"; the part before the jump is kept (AC3). |
| 4 | `--max-px 2000` on a longer page. | Image is 2000 px, `end` = `limit_reached`, the limit note is shown (AC4). |
| 5 | Wayland: Hand scroll only (`--hand`), page scrolled by its own script. | AC1 passes (AC5). Note whether the desktop asked once or every frame (spike S6). |
| 6 | macOS without Accessibility (remove it in System Settings): Auto scroll. | `permission_missing` / "Allow Accessibility to scroll for you"; Hand scroll still works (AC6). Settings > Permissions shows the Accessibility card. |
| 7 | `--horizontal` on a wide spreadsheet. | Columns once and in order. |
| 8 | An agent calls with `scroll: "hand"`. | `invalid_input`, `data.reason` = `needs_user` (AC7). |
| 9 | Excel, Slack/Discord, PDF viewer, each with Auto scroll. | Note per app: end found, no duplicated rows, settle time. |
| 10 | Start Long Shot from the launcher (command "Long Shot"), the menu bar item "Long Shot" and, after setting one in Settings > Shortcuts (it reads "Not set" until then), the hotkey. | The overlay opens with the bottom bar: Region, Window, Display, **Long Shot** (selected), Click Guide, Auto scroll / Hand scroll, Vertical / Horizontal, Start with Enter (S1). |
| 11 | Move the pointer over a window, then click it (or press Enter). Repeat by dragging a region and pressing Enter or Start. | The window under the pointer is highlighted with the chips "Scrollable area", its size and the scale; the hint reads "Click to use this area, or drag a region"; Esc cancels and nothing is saved. |
| 12 | Click Start with Hand scroll chosen. Scroll the target by hand. | The HUD opens above the area (below when the area touches the top edge; inside at the bottom when it fills the display height; S2b), the side preview opens on the side with more room (right first) and grows as you scroll; the counter moves; the HUD shows `Hand`; the HUD is **not in the stitched image**; the target window keeps the focus (its title bar stays active; Windows: it must not deactivate after the first frame). |
| 13 | Full-display area (Display via the bar is not Long Shot: drag a region over the whole display). | No room beside the area: the HUD has a **Preview** button that opens the preview as a floating panel over the far side. |
| 14 | Pause, scroll the target by about two screens, Resume. | While paused the count does not move; after Resume the next frame may not line up (a jump while paused): the capture then ends with "Stopped: the next part did not line up" and keeps what it had. Pause only holds the capture; it is not a way to skip content. |
| 15 | Drag the HUD aside (S2b rule 4; Windows: by its background, not a button). | It moves and stays there for this Long Shot only; the next Long Shot places it by the rules again. |
| 16 | Stop (the red square) in the middle of a run (Auto and Hand). | The image so far is saved; the toast says "Long Shot saved" with the size and frame count, with **Open** and **Copy**; Open opens the editor. |
| 17 | The toasts of the other endings: a page that jumps (`?jump=1`), a page longer than 16,384 px, a page that ends (success). | Warning "Stopped: the next part did not line up"; warning "Reached the longest image CaptureMann can edit"; success "Long Shot saved". The image is saved and in the library each time; its `manifest.json` has `"source": "long_shot"` and `"frames": <n>`. |
| 18 | macOS without Accessibility: open Long Shot. | Auto scroll is shown dimmed; clicking it (or Start with it chosen) opens the card "Auto scroll needs Accessibility" with **Grant access...** and **Use Hand scroll**; Hand scroll starts a Long Shot; Grant access shows the system prompt and the toast "Allow CaptureMann in Accessibility" (AC6). |
| 19 | Wayland: open Long Shot. | The bar shows one "Hand scroll" label and no Auto scroll (S1). |
| 20 | Click another mode on the bar (Region, Window, Display, Click Guide). | The overlay closes and that mode starts. |
| 21 | Horizontal on the bar, Start, scroll sideways. | The preview fits the whole image into the panel; the HUD shows Horizontal. |

Record per app: the scroll step in px that one event gave, how many frames, the total time.

Not seen on a screen when these steps were written (Nov 14, 2026): steps 10 to 21 are the first run
of the overlay, HUD and preview. Note what differs from design boards S1, S2 and S2b (the bar has
text labels where the board has icons; the preview marks the header and footer with tags, not
dashed lines; there is no Esc during the run, use Stop).
