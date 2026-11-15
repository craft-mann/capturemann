# Design requests for 0.3.4 (to forward to Claude Design)

Context: CaptureMann (desktop screenshot and screen-recording app, dark UI, tokens and recipes in the
existing design pack `specs/capturemann-0.3.4-design/`, boards S0 to S13). The items below were built
without a board, or the board and the build now disagree. For each: what exists, what to design, the
states needed. Please follow the pack's tokens (40 px controls, 44 px HUD targets, `rec` red, Geist).

## D1. Sidebar entry for Long Shot and Click Guide (open question)
- Today: the sidebar has no tile for either. They start from the launcher, the tray, a hotkey (Long Shot,
  no default chord) and the library header ("New Click Guide", "Click Guide of a window…").
- Decide and draw: (a) Long Shot only, (b) Long Shot + Click Guide, or (c) neither. Show the sidebar at
  248 px with the tile(s) in rest, hover, active, and disabled (Click Guide on Wayland has no input
  events; a recording is running) states, plus the tooltip text for the disabled reason.

## D2. Library selection (replaces the Select / Done buttons) — S11 update
- Built: no Select or Done button. Hovering a card shows a round tick at its top-left (the kind pill
  fades out); ticking starts selecting; while any card is ticked every card shows its tick, a click
  on a card toggles instead of opening, and the bar offers Make a board, Export selected, Delete, and
  an X (clear). Unticking the last card ends selecting.
- Draw: card states (rest, hover, hover over the tick, ticked, selecting-but-unticked) for a
  screenshot, a recording, an audio and a guide card; the tick size and hit area (28 px target);
  the selection bar; keyboard focus ring on the tick; how a card shows "selected" besides the tick.

## D3. "Click Guide of a window…" (start a guide on a chosen window)
- Built: a secondary button beside "New Click Guide" (library header and the Click Guides empty state),
  an app-menu item, and the existing window picker reused with the primary button "Start guide".
- Draw: the two header buttons together (widths, icon, Vietnamese labels are about 30 % longer),
  the window picker in its "Click Guide" purpose (title, button, one-line explanation that only clicks
  inside that window become steps), and what the recording bar says while following a window
  (target name under "Click Guide · clicks only"? or a chip), and what happens when the window closes
  (toast and bar state).

## D4. Click Guide bar (S3) — confirm width rule and the in-between states
- Built: 580 px wide, 58 px high; Stop is a 44 px red square icon button; Step n with a `rec` dot,
  "Click Guide · clicks only", a divider, then Pause, Note step, Undo step with labels.
- Draw/confirm: the bar for (1) countdown "Starting…" with a Cancel (Stop during the countdown
  cancels), (2) paused, (3) following a window, (4) a long language (Vietnamese: "Ghi chú bước",
  "Hoàn tác bước") so labels never clip; say the maximum width and what collapses first on a narrow
  display (labels to icons, in which order).

## D5. Long Shot: Hand scroll with continuous scrolling, and how a run ends
- **Designed and built (Nov 15, 2026), board S18** (`R034LongHand`, with S2 `R034LongRun`, S2b
  `R034HudPlace`, S20 `R034Lengths`), **tested with fakes and on the display-free model, not seen on a
  screen:** the frame counter fades each new number in over 120 ms (static with reduced motion), an 8 px
  dot after it (success while frames are taken, warning when the last part did not line up), "Keep
  scrolling" with a chevron over a fade at the bottom of the side preview and "End of page · press Stop"
  once no new rows came for a second, the "Esc to stop" hint after the mode (first thing to give way on a
  narrow display, then the mode label with its icon staying, then the second line; the HUD is at most
  640 px and at most the display minus 48), "Scroll back up a little" with an amber dot as the second
  line until a frame lines up again, the four end toasts with the board's words (Esc, size limit, a page
  that jumped in Auto scroll, the live "did not line up" toast with Stop and keep), and the passive HUD
  (Stop, the count, an Agent chip) for a Long Shot an agent or the CLI started. Differences from the
  board are in `specs/features/01-screenshot.md` (CAP-6, "HUD while a Hand scroll runs").
- Decision made in code: a frame is taken when the scrolled distance since the last frame reaches about
  a third of the area, or when the content has been still for about 150 ms.
- Draw: the HUD and the side preview while a continuous scroll runs (frame counter ticking, a visible
  "keep going" cue), the "Esc to stop" hint on the HUD (Esc now ends the run and keeps the frames so far)
  and the toast when it ends because of Esc, because of the size limit, or because the next part did
  not line up (today's wording is "Stopped: the next part did not line up"; propose wording that says
  what the user can do: scroll back a little and continue, or Stop and keep what is captured).
- Also: a Long Shot started from the CLI or an agent has no HUD. Should a small passive HUD (Stop only)
  appear for it, as the recording HUD does for agent recordings?

## D6. Retype: no overflow warning, no "Not a redaction" note
- Built: a longer text widens the box (the fill with it) instead of warning; the panel no longer shows
  "The new text is wider than the box…" nor "Not a redaction…" (Move area still shows the second).
- Draw (S4 update): the panel without those notes; how the widened box looks while typing (the dashed
  edit outline grows live?); whether Move area and Blueprint keep the "Not a redaction" note.

## D7. Agents & CLI status in the sidebar (MCP card removed)
- Built: no card; the Agents & CLI icon is blue when MCP is ready and grey when offline, and its
  accessible label says the status ("MCP ready", "MCP offline", "Another app owns the socket").
- Draw: the icon in rest, hover, active, ready, offline (three reasons), and a hover tooltip carrying the
  status line so the colour is not the only cue (colour-blind safe).

## D8. Click Guide editor (S8) details found in QA
- The step picture must keep its own aspect ratio inside the right column (fixed in code); confirm the
  maximum picture height and what happens with very tall pictures (scroll inside the column or fit).
- G3 to G5 and R1 (see `verification/QA-0.3.4-*` reports) are small layout defects being fixed in code;
  design check afterwards only.

## D9. Translations and length
- Vietnamese strings run about 20 to 40 % longer than English. Please state, for the Long Shot bar,
  Click Guide bar, library selection bar and the Make-a-board sheet, the maximum label length and the
  truncation or wrapping rule, so the layout tests can assert it.
