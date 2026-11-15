# Design requests for 0.3.4 (to forward to Claude Design)

Context: CaptureMann (desktop screenshot and screen-recording app, dark UI, tokens and recipes in the
existing design pack `specs/capturemann-0.3.4-design/`, boards S0 to S13). The items below were built
without a board, or the board and the build now disagree. For each: what exists, what to design, the
states needed. Please follow the pack's tokens (40 px controls, 44 px HUD targets, `rec` red, Geist).

## D1. Sidebar entry for Long Shot and Click Guide (DESIGNED in S14, option b, and BUILT Nov 15, 2026)
- Today: the sidebar has no tile for either. They start from the launcher, the tray, a hotkey (Long Shot,
  no default chord) and the library header ("New Click Guide", "Click Guide of a window…").
- Decide and draw: (a) Long Shot only, (b) Long Shot + Click Guide, or (c) neither. Show the sidebar at
  248 px with the tile(s) in rest, hover, active, and disabled (Click Guide on Wayland has no input
  events; a recording is running) states, plus the tooltip text for the disabled reason.

## D2. Library selection (replaces the Select / Done buttons) — S11 update
**Designed (board S15, Nov 15, 2026) and built:** card states, the 24 px tick in a 28 px hit area, Esc clears, the selection bar (S20: Delete in the text colour, collapse order Delete, Export selected, Make a board).

- Built: no Select or Done button. Hovering a card shows a round tick at its top-left (the kind pill
  fades out); ticking starts selecting; while any card is ticked every card shows its tick, a click
  on a card toggles instead of opening, and the bar offers Make a board, Export selected, Delete, and
  an X (clear). Unticking the last card ends selecting.
- Draw: card states (rest, hover, hover over the tick, ticked, selecting-but-unticked) for a
  screenshot, a recording, an audio and a guide card; the tick size and hit area (28 px target);
  the selection bar; keyboard focus ring on the tick; how a card shows "selected" besides the tick.

## D3. "Click Guide of a window…" (start a guide on a chosen window)
**Designed (board S16) and built:** header order and icon fallback, empty state, the picker's purpose text and countdown line, the bar's "Following" line, the closed-window state (daemon `window_closed`) and its toast.

- Built: a secondary button beside "New Click Guide" (library header and the Click Guides empty state),
  an app-menu item, and the existing window picker reused with the primary button "Start guide".
- Draw: the two header buttons together (widths, icon, Vietnamese labels are about 30 % longer),
  the window picker in its "Click Guide" purpose (title, button, one-line explanation that only clicks
  inside that window become steps), and what the recording bar says while following a window
  (target name under "Click Guide · clicks only"? or a chip), and what happens when the window closes
  (toast and bar state).

## D4. Click Guide bar (S3) — confirm width rule and the in-between states
**Designed (board S17) and built:** countdown, paused, following, long languages, narrow displays; width measured from the text.

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

## D7. Agents & CLI status in the sidebar (MCP card removed) (DESIGNED in S14 and BUILT Nov 15, 2026)
- Built: no card; the Agents & CLI icon is blue when MCP is ready and grey when offline, and its
  accessible label says the status ("MCP ready", "MCP offline", "Another app owns the socket").
- Draw: the icon in rest, hover, active, ready, offline (three reasons), and a hover tooltip carrying the
  status line so the colour is not the only cue (colour-blind safe).

## D8. Click Guide editor (S8) details found in QA
**Designed (board S19) and built:** contain fit, 240 px minimum, tall picture chip and the full-size viewer.

- The step picture must keep its own aspect ratio inside the right column (fixed in code); confirm the
  maximum picture height and what happens with very tall pictures (scroll inside the column or fit).
- G3 to G5 and R1 (see `verification/QA-0.3.4-*` reports) are small layout defects being fixed in code;
  design check afterwards only.

## D9. Translations and length
**Designed (board S20) and built for the Click Guide bar and the selection bar** (layout tests `s17_*`, `s20_*`); the Long Shot bar and the Make-a-board sheet are built by their own tracks.

- Vietnamese strings run about 20 to 40 % longer than English. Please state, for the Long Shot bar,
  Click Guide bar, library selection bar and the Make-a-board sheet, the maximum label length and the
  truncation or wrapping rule, so the layout tests can assert it.

## Built from board S14 (Nov 15, 2026): D1 and D7
- D1: second dock row with Long Shot and Click Guide tiles and the same two as rail buttons (`ui/sidebar.rs`
  `extra_tile`, model in `nav.rs`: `ExtraTile`, `DockFacts`, `tile_state`, `tile_tip`). Differences from the board: the
  Wayland sentence shows for any session with no click events (the engine only knows "no click events", Wayland is the
  case that exists today); Long Shot is disabled only when there is no library (the launcher's reason), because no
  engine rule is known that blocks it during a recording or a guide; the focus ring is two spread shadows (2 px text, 2 px gap).
- D7: status shape at the icon's bottom-right (`StatusMark`: dot, ring, "!" badge), mist icon unless ready, a status
  tooltip, and the accessible names "Agents & CLI, MCP ready / MCP offline / another app owns the socket". Missing library
  and missing endpoint show "MCP offline" with their own reason, never "Another app owns the socket". The board's
  "2 min ago" is not shown: the activity log's last entry has no time in `LastCall`.
- Not seen on screen: tested with pure models and width tests only.

## Published page removed (maintainer decision Nov 15, 2026; boards S22 and S23; built the same day)

Not a request: the Published page and its sidebar item are gone. Recordings carries the Published and
Needs attention chips, a published card shows the platform pill, "Published <time> · <size>" and one status
line, and the recording details page has a Published card. Differences from the boards are listed in
`docs/development/design-surfaces.md` ("Published (S22, S23, 0.3.4)").

## D10. Default colour of new Text annotations (screenshot editor, Text tool, key T)
**Designed (board S21, Nov 15, 2026) and built:** the Auto default (`cm_render::auto_text_style`; #111111 or #FFFFFF by the mean luminance of the picture under the box, switch point 0.19; a 2 px outline in the other colour when fewer than 90 % of the sampled pixels reach 4.5:1), the Auto / Light / Dark control with the AUTO tag and the source line, `annotate` with no `color` (answers `auto_text`), ANN-1.AC8. Differences from the board: the Text panel had no Outline or Shadow controls to keep, so it shows a read-only "Outline 2 px · #111111" line when Auto turned one on; an explicit `color` sent to `annotate` without `outline` keeps the schema's old black outline default (Auto adds none).

- Today: a Text annotation placed from the Text tool starts white, 24 px, with no outline and no shadow
  (a deliberate 0.2.9 choice). On a light screenshot (a document, a web page, a dialog) the text is
  invisible until the user opens the panel and changes the colour. Black would fail the same way on a
  dark screenshot. QA of 0.3.4 flagged it twice. The colour the user last picked for the Text tool is
  remembered; an existing annotation keeps its stored colour; Callout, Step labels, Stamps and Retype have
  their own defaults and are not part of this request.
- Decide and draw (the editor at 1440 x 900, dark UI, tokens and recipes of the existing pack):
  (a) which default a new Text annotation should have, with the reason, for both a light and a dark
  screenshot; options to weigh: keep white; black (`#111111`); a **contrast-aware default** (the editor
  samples the picture around the click and starts with black or white, whichever reads better, and the
  swatch in the panel shows which one was chosen); or white with a thin dark **outline or shadow** by
  default so it reads on any picture (the panel's Outline and Shadow controls already exist; today both
  are off);
  (b) how the text looks while it is being typed (the box, caret and selection from the text-field spec)
  on a light picture and on a dark picture with the chosen default;
  (c) the Text panel: where the colour swatch sits, whether a one-click "Light / Dark" toggle is worth
  adding beside the palette, and how the panel shows that the colour came from the picture rather than
  from the user's last choice (a small "Auto" label on the swatch?);
  (d) states: first text on a light picture, first text on a dark picture, a picture with a busy
  photograph behind the text, and a second text after the user changed the colour by hand (it keeps
  their colour, not the automatic one).
- Constraints: text stays editable and stored as an op (colour is a stored value, not a render-time
  guess); an agent calling `annotate` without a colour must get a deterministic result (state what the
  default is for an agent, since it cannot see the picture); WCAG 4.5:1 for text on the picture is the
  target where the design can reach it; the choice must not change annotations that already exist.
- Output wanted: the recommended default and one fallback, the panel and on-canvas states for light,
  dark and photo pictures, exact colour values, and the one-line rule I can write into the spec (ANN-1.AC8).
