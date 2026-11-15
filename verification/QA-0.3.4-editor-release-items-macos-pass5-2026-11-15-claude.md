# QA 0.3.4 editor release items, macOS pass 5 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, macOS pass 5)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app`, dev bundle of main `a4f28a07` (bundle id `dev.capturemann.app.dev`); my own instance started with `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET` under `/tmp/cmq5`; no cargo build; no source edits.
- **FFmpeg:** not used for these cases.
- **Method:** synthetic pointer and key events (a small CGEvent tool written for this pass, `/tmp/cmq5/ev`), app-window pictures with `screencapture -R`, the saved `render.png` of the bundle zoomed with `sips` for pixel questions, the bundle `manifest.json` for stored values, and the `capturemann` CLI of the same bundle. Fixtures only: imported PNGs made for this pass (light dialog picture, dark picture, full-picture RGB noise, a 20-line and a 6-line text picture). Keys typed into popovers and system panels needed `osascript` keystrokes (see "Driver notes").
- **Pictures:** `QA-0.3.4-shots/macos-pass5-*.png` (app window only).

## Results

| Item | Result | Evidence |
| --- | --- | --- |
| Text default colour AUTO, light picture | PASS (seen, measured) | Text tool on the light fixture: the panel shows Auto / Light / Dark with Auto selected, an AUTO tag on the dark swatch and "Auto: picked from the picture (light)". Placed text stored `color: #111111`, `outline: null` in `manifest.json`. `macos-pass5-text-auto-light.png` |
| Auto, dark picture | PASS (seen, measured) | AUTO tag on the white swatch, "Auto: picked from the picture (dark)", stored `#FFFFFF`. `macos-pass5-text-auto-dark.png` |
| Auto, busy picture | PASS (seen, measured) | Full-picture RGB noise: "Auto: busy picture, outline added" and "Outline on: 2 px · #FFFFFF"; stored `color #111111`, `outline #FFFFFF`. `macos-pass5-text-auto-busy.png` |
| Typing does not re-sample | PASS (measured) | typed "On dark" and "Hello Auto" after placing; colour unchanged in the manifest |
| Hand-picked colour | PASS (seen, measured) | clicking the red swatch: none of Auto / Light / Dark selected, line "Your last colour (Text tool)"; the next text stored `#FF3B3B`. `macos-pass5-text-manual-colour.png` |
| Clicking Auto restores | PASS with a visual defect (D-ED1) | Auto is selected again and the next text is sampled (dark picture gave `#FFFFFF`), but the swatch row still shows the old red ring next to the Auto highlight |
| CLI `annotate` text op without colour | PASS (measured) | `capturemann annotate <id> --ops '[{"op":"text",...}]'` returns `auto_text: [{index, color, outline, outline_width}]`: light picture region with a gradient: `#111111` with `#FFFFFF` outline 2.0; dark picture: `#FFFFFF`, no outline. The result carries the Auto sample; with Auto selected it did not return a stored hand colour (not tested with a hand colour set, NOT RUN) |
| Retype: click a recognized line | PASS (seen) | dotted boxes on every recognized line; click selects, Text field in the panel filled with the line text |
| Retype: panel Text field accepts typing after the click | PASS (seen) | typed text replaced the selection and showed in the field (and wrapped on two lines in the panel) |
| Retype: longer text widens the box | PASS (seen, measured) | "Email: bob.smith@example.com (verified primary address)" grew the box from the line's width to 665 px (screen), applied with Enter; the op is `retype` with the new text |
| Retype: no "wider than the box" warning, no "Not a redaction" note | PASS (seen) | only the blue base-edit note; Move area does show "Not a redaction" (correct there) |
| Retype: Vietnamese with stacked marks and tails in a tight box | PASS (seen at 3x) | "Ứng Ặẫ Ệợ Ữỹ gỵ ự Lệ Ỗ ặ" typed into a 12 px line and "Kế hoạch: Chuyên nghiệp (hằng năm) ỵ"; the saved `render.png` cropped and enlarged 3x shows every mark above and below the letters, nothing clipped. `macos-pass5-retype-vietnamese-render.png` |
| Retype: busy-background sheet (Use anyway / Cancel do not click through) | NOT RUN | I did not build a retype on a busy picture; fixture and test still to do |
| Move area: drag a source, drag contents | PASS (seen) | drew a 145x145 source around the orange circle, dragged the contents 70 px down-left; the circle moved and the source hole was filled with the picture colour (`filled · #EDEDED`); the op sits under later text |
| Move area: Shift axis lock | PASS (seen, measured) | with Shift the destination stayed on the source row: panel "Source 685, 105 · Moved to 343, 105" |
| Move area: Delete on a source only | PASS (seen) | Del removed the contents (circle gone, 7 annotations) |
| Move area: Delete after a move | PARTIAL, confusing (P-ED2) | with a move in place, "Remove contents (Del)" deleted the whole move: the circle came back at the source instead of being removed. The hint says "press Delete to remove them" |
| Blueprint: 20-line case | PASS (seen) | "20 text lines found · 20 become bars · 0 kept"; 20 bars drawn; Apply Blueprint reachable (by scrolling the panel once lines are kept) |
| Blueprint: keep-by-click, Apply | PASS (seen) | clicked lines 3 and 7: "18 become bars · 2 kept", a `kept` badge and a "Kept lines" list with remove buttons; Apply gave 1 annotation, 18 bars, the 2 kept lines in real text. `macos-pass5-blueprint-keep.png` |
| Blueprint in bulk export (20 lines) | PASS (seen) | exported file shows exactly 20 neutral bars. `macos-pass5-bulk-blueprint-20.png` |
| Zoom Lens: circle and rounded rect | PASS (seen) | circle, rounded and rect shapes, 3x badge, connector; a second lens on the orange circle |
| Zoom Lens: no colour fringe on the edge | PASS (seen at 30x, one place) | enlarged crop of the saved render where the orange circle meets the white lens border: only a lighter-orange anti-alias column, no dark or off-colour halo. The white border on a grey picture was also clean. Only two edge locations looked at. `macos-pass5-zoomlens-edge-zoom.png` |
| Canvas text selection: double-click a word | PASS (seen) | "world" selected |
| Canvas text selection: triple-click a line | PASS (seen) | the whole "hello brave world" selected (the tool sets the click count to 3; pass 2 saw the word only, so its synthetic clicks did not carry the count) |
| Canvas text selection: Shift+arrows, Cmd+A | PASS (seen) | Shift+Right x3 from the start selected "hel"; Cmd+A selected the whole text |
| Select text (G) | NOT CONFIRMED | tool selects and the key G works, the panel is empty ("Settings for new annotations." and nothing else). A drag, a double-click and Cmd+A over recognized lines showed no highlight and put nothing on the clipboard in my runs (pass 2 reported a clipboard result). Cause not found: could be my event driver or a defect. Needs a person |

## Defects

| Id | Severity | Steps | Evidence |
| --- | --- | --- | --- |
| D-ED1 | low | Text tool: place text (Auto), commit with Esc, or click Auto after a hand colour | after the first sample the panel shows Auto selected AND a ring on a swatch (white after a dark placement, red after a hand pick); the line reads "Auto: picked from the picture when text is placed" without the "(light)/(dark)" result or the AUTO tag; both states sit in one panel. `macos-pass5-text-auto-after-commit-ring.png` |
| D-ED2 | low | Retype, type a longer text in the on-canvas box | the box widens at once, but the text inside the box on the canvas still shows the old, shorter text clipped ("Email: bob.smith@example.cor") until Enter; the panel field has the new text |
| D-ED3 | low | Text tool, click outside the picture | a text annotation can be placed outside the picture and is cut off at the picture edge ("ed hand") |
| D-ED4 | low | Blueprint, keep two lines | the `kept` badge is drawn over the first letters of the kept line, so "Row 3:" cannot be read under it; once lines are kept, Apply Blueprint is below the fold of the right panel (scroll needed) |

## Driver notes (so the next pass does not lose time)

- A synthetic Unicode type event is not accepted by the Rename popover or a system open panel; `osascript` `keystroke` is. The editor canvas and panel fields accept both.
- A Select text drag with my tool did not show a selection; pass 2 used a tool that posts 12 moves with 30 ms gaps. Not the cause as far as I could tell.
- The clicks that open a card menu are on the hover "..." button; a right-click on a card in the Screenshots page opens the editor (single click opens, so the right-click path was not tested).

Verdict: PARTIAL. Not run: Retype busy-background sheet, CLI annotate with a hand colour set, Select text confirmation, undo of Retype and Move area, Zoom Lens on a very large image.
