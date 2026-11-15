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
| 3 | Page with `?jump=1`. | Auto scroll stops with the warning "Stopped where the page jumped" ([Open, Try Hand scroll]); the part before the jump is kept (AC3). In Hand scroll the capture does not stop on its own: the frame after the jump is left out, the HUD's second line says "Scroll back up a little" with an amber dot and the toast "The last part didn’t line up" offers Stop and keep; scrolling back to a place that joins goes on and the second line returns by itself; Stop (or Esc) while a frame is left out keeps the image. |
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
| 14 | Pause, scroll the target by about two screens, Resume. | While paused the count does not move; after Resume the next frame may not line up (a jump while paused): the HUD says "Scroll back up a little"; stopping keeps what it had. Pause only holds the capture; it is not a way to skip content. |
| 15 | Drag the HUD aside (S2b rule 4; Windows: by its background, not a button). | It moves and stays there for this Long Shot only; the next Long Shot places it by the rules again. |
| 16 | Stop (the red square) in the middle of a run (Auto and Hand). | The image so far is saved; the toast says "Long Shot saved" with the size and frame count, with **Open** and **Copy**; Open opens the editor. |
| 17 | The toasts of the other endings: a page that jumps (`?jump=1`), a page longer than 16,384 px, a page that ends (success). | Warning "Stopped where the page jumped"; warning "Long Shot saved at the size limit"; success "Long Shot saved". The image is saved and in the library each time; its `manifest.json` has `"source": "long_shot"` and `"frames": <n>`. |
| 18 | macOS without Accessibility: open Long Shot. | Auto scroll is shown dimmed; clicking it (or Start with it chosen) opens the card "Auto scroll needs Accessibility" with **Grant access...** and **Use Hand scroll**; Hand scroll starts a Long Shot; Grant access shows the system prompt and the toast "Allow CaptureMann in Accessibility" (AC6). |
| 19 | Wayland: open Long Shot. | The bar shows one "Hand scroll" label and no Auto scroll (S1). |
| 20 | Click another mode on the bar (Region, Window, Display, Click Guide). | The overlay closes and that mode starts. |
| 21 | Horizontal on the bar, Start, scroll sideways. | The preview fits the whole image into the panel; the HUD shows Horizontal. |
| 22 | Hand scroll slowly and continuously (small wheel steps, never stopping) down a long page (design S18). | The counter ticks up about every third of the area scrolled; each new number fades in over 120 ms (no bounce; with Reduce motion on, it just changes); an 8 px green dot follows the count while frames are taken and goes out when you stop; the bottom of the side preview says "Keep scrolling" with a chevron over a fade; the HUD reads `Hand` then `Esc to stop`. |
| 23 | Keep going to the very end of the page and wait a second. | The preview's cue changes to "End of page · press Stop"; scrolling up a little brings "Keep scrolling" back. |
| 24 | Press Esc in the middle of a run (the target has the focus). | The run ends, the image so far is kept, and the toast reads "Long Shot saved" / "Stopped with Esc. Everything up to here is kept: {n} frames, {w} × {h}." with **Open** and **Copy**. |
| 25 | A page longer than 16,384 px. | "Long Shot saved at the size limit" / "It reached 16,384 px, the longest image CaptureMann can edit. Everything up to there is kept." with **Open** and **Copy**. |
| 26 | Hand scroll, then flick past the overlap (a jump). | The dot turns amber, the HUD's second line becomes "Scroll back up a little" with an amber dot, and a toast "The last part didn’t line up" offers **Stop and keep**; scrolling back makes the second line return to the size by itself; **Stop and keep** ends the run and keeps the frames. |
| 27 | Narrow the display (or use a small external one) and start Hand scroll. | The HUD is at most 640 px and at most the display minus 48; the Esc hint goes first, then the mode label (the icon stays), then the second line; the count never truncates. Same in Vietnamese. |
| 28 | Start a Long Shot from the CLI (`capturemann capture screenshot --mode long ...`) or an agent with `scroll: "auto"`. | A small HUD with Stop, the frame count and an **Agent** chip (no direction, Pause or mode) appears by the rule of S2b and is not in the image; Stop and Esc both end the run and the caller gets the image. |

Record per app: the scroll step in px that one event gave, how many frames, the total time.

Not seen on a screen when these steps were written (Nov 14, 2026; steps 22 to 28 Nov 15, 2026,
design S18): steps 10 to 28 are the first run of the overlay, HUD and preview. Note what differs from design boards S1, S2 and S2b (the bar has
text labels where the board has icons; the preview marks the header and footer with tags, not
dashed lines; Esc stops the run).
