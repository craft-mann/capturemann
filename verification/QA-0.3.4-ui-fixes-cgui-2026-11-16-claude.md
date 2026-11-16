# QA 0.3.4: UI fixes and the Preview label (agent cg-ui), Nov 16, 2026

Result: PARTIAL. Code and tests only; nothing was looked at on a screen (the maintainer's Mac was
not to be used for the app), so no layout, colour, dashed border, focus ring or popover position was
seen by eye. Tests ran with fakes on macOS (`--no-default-features`).

| Item | What was done | Test | Failed first? |
| --- | --- | --- | --- |
| Preview label (design S24) | one component `ui/preview_label.rs` (XS 14, S 18, M 22, dashed pill, tokens, hover, focus ring, tooltip, popover, intro card, feedback URL, `previews.<feature>` read with default on); overlay mode bar tag in `apps/overlay/src/long_paint.rs` | `preview_label::tests` (6), overlay tag rect test | new code |
| CG-2 passive bar | `guide.started` from an agent or the CLI opens a passive bar (Stop, "Step n", "Started by agent {client}", Agent chip); Long Shot already had its passive HUD (`long_shot/mod.rs open_passive`), unchanged | `cg_2_*` (2) in `guide_view/hud.rs` | new code |
| Quit during Long Shot or guide | `quit_plan` (pure): a running guide gets `stop_guide` (saved), a running Long Shot is cancelled (frames kept) and awaited up to 10 s, then the app closes; toast "Saving your Long Shot or guide" | `quit_stops_and_keeps_a_running_long_shot_or_guide` | new code; the real quit was not run |
| CG-11 Vietnamese tile cut | label box is two 13 px lines (explicit line height), height from `nav::tile_metrics` | `dock_tile_height_holds_two_label_lines_and_the_keycaps`, layout width test (vi lines <= 2) | n/a (layout) |
| CG-12 "Long image" pill | tall only when height >= 1.8 x width (`fit::TALL_ASPECT`) | `s19_a_tall_picture_is_fitted_by_height_and_flagged` | yes (did not compile before the constant; the 770 x 860 case now not tall) |
| CG-13 thumbnail | the daemon already draws the guide crop (new test `cg_13_the_card_picture_of_a_display_guide_is_the_guide_crop` PASSES without a change, so the uncropped picture seen in pass 7 is not reproduced here); "No preview" at first: `ThumbCache` now keeps the placeholder and asks again up to 4 times, 3 s apart, before "No preview" | `a_missing_poster_is_asked_for_again_a_few_times` | no (could not reproduce the uncropped frame) |
| LIB-1 tall thumbnails | `cm-library::thumbs::write_poster`: a picture more than 2x as high as wide gets its top part cut to the card shape at 512 px width with a 25 % fade; other pictures unchanged | `lib_1_*` (2) | new code |
| Move area note | unchanged: `editor_view/touch_panels.rs:409` "Not a redaction. To hide secrets, use Blur (B) or Pixelate (X)." Retype shows no note. Left as is, not removed. | | |

Commands: `cargo test -p cm-desktop --no-default-features --locked` (1913 lib tests, layout_guard 2/2
after fixing two guard hits), `-p cm-overlay` (128), `-p cm-library`, `-p cm-daemon --test guide --test
library_ops`; clippy 1.100 `-D warnings` on cm-desktop, cm-overlay, cm-library, cm-daemon clean;
`cm-i18n check` ok.

## Not checked

- Anything by eye: the dashed look, the XS tag straddling the tile corner, the overlay tag (drawn
  with solid ring plus 2 px gaps), the popover and intro card positions (popover: top left beside the
  sidebar; intro: top right), the Vietnamese pill text, the passive guide bar on screen.
- The real Quit with a running Long Shot or guide; the passive guide bar from a real CLI start.
- Existing tall posters are not redrawn until the capture changes; new ones use the new path.
- The Windows and Linux builds.

## Preview label placements (for the designer)

1. Sidebar dock tiles Long Shot and Click Guide: XS tag, top right, straddling the border (right -4,
   top -8, filled with the tile surface). The red running dot moved to the top left so they never meet.
   Rail (icon only): no tag; the tooltip adds "Preview: new and still changing." and the accessible
   name is "{name}, preview".
2. Capture overlay mode bar (Long Shot picker): XS tag above the top-right corner of the Long Shot
   and Click Guide buttons (right edge 2 px past the button, bottom 4 px inside it). Always dark.
3. Launcher rows "Long Shot" and "New Click Guide": S label after the title, not clickable there.
4. Click Guides library page title: M label (flask icon) after the title, opens the popover. Room is
   taken from the header buttons' icon-only rule.
5. Click Guide editor toolbar: M label after the title (a new `page_toolbar_tagged`).
6. Native menus (macOS app menu, tray "New Click Guide"): text suffix " (Preview)" on Long Shot,
   New Click Guide, Click Guide of a window...
7. Popover (click on a label): "{name} is in preview", three lines, Send feedback, Turn off in
   Settings > Previews (goes to Settings General; the Previews section is built by agent `previews`).
8. Intro card (once per feature, flag `ui.preview_intro.<feature>`): top right, Got it / Send feedback;
   shown on the first Click Guide page or Long Shot start.
9. Not done: Settings rows (S), library header Click Guide buttons, HUDs (stay clean), cards, toasts.

Questions for the design: popover and intro positions are fixed offsets (no anchor to the label);
the overlay tag cannot use a true dashed stroke or semi-bold in the software canvas; the tile's
accessible name now says "preview" before the chord.
