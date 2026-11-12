# ANN-6 manual script: the 0.3.3 screenshot editor by eye

Covers ANN-4 (callout, stamps), ANN-6 (step label, numbering style, start, reorder), ANN-7
(spotlight) and the three editor changes of the 0.3.3 design pack: the grouped tool palette, the
view bar and the image strip. The pure parts (op validation, `step_label`, `callout_box`, golden
images, field pass-through) are covered by `cargo test -p cm-core -p cm-render -p cm-library` and
`cargo test -p cm-desktop --lib editor::`; the `annotate` ops through the CLI are covered by the
`cm-regress` case `annotate.pack`. A window can only be judged by eye, and the editor has no
hands-on report for ANN-1 yet, so this script also touches the old tools.

Run it on macOS first (use the bundle, `make run-bundle-dev`), then on Windows 11 and Linux when
you can. No camera, microphone or screen recording is needed beyond the screenshots in the setup.
Use made-up content only and delete the captures afterwards.

Setup. Take three screenshots of a made-up window (a note with a few lines, a form, a table) so the
library holds at least three images; the image strip steps (27 to 33) need three or more images in
one library view, and step 31 needs a window you can make shorter than 640 px. A fourth image with a
line of Vietnamese text is useful for step 9. If you have a Vietnamese Telex keyboard
(`tests/manual/ANN-1-IME.md` lists the setup), enable it for steps 9 and 21.

## A. The grouped palette

1. **Eight buttons.** Open the first screenshot in the editor. Expected: a vertical palette on the
   left of the stage with exactly 8 buttons in this order: Select, Shapes, Text, Highlighter,
   Markers, Emphasis, Redact, Crop. Thin dividers sit before Shapes, Emphasis, Redact and Crop. The
   palette is about 380 px tall. A small corner mark sits on each button that holds more than one
   tool (Select, Shapes, Text, Markers, Redact; Emphasis only if more than one tool is built, and
   Magnify is not part of 0.3.3).
2. **Faces and keys.** Hover each button. Expected: the tooltip names the tool on the face and its
   key, for example `Callout [K] in Text`. Press the keys and check the button that lights up:
   V Select, G Select text, A Arrow, L Line, R Rectangle, E Ellipse, T Text, K Callout, H Highlighter,
   N Step badge, U Stamp, S Spotlight, B Blur, X Pixelate, C Crop. Every key picks its tool at once
   and the button face changes to it (pressing K makes the Callout the face of the Text button).
3. **Group menu.** Click the corner mark of Shapes, or press and hold the button, or right-click it.
   Expected: a menu with the group title and four rows (Arrow, Line, Rectangle, Ellipse), each with
   an icon, a name and a key; the current one is marked. Choose Ellipse. Expected: the menu closes
   and Shapes now shows the ellipse. Esc closes the menu without a change; arrow keys move through it.
4. **Face is remembered.** Quit the app and open it again. Expected: Shapes still shows Ellipse and
   Text still shows the tool you used last.
5. **New-tool mark.** The buttons whose group has a new tool (Text, Markers, Emphasis) may show a
   small blue dot. Record whether it shows and whether it goes away (the plan says: it goes after one
   release, nothing is stored per user). Not a pass-or-fail step.

## B. Callout (K)

6. **Place.** Press K. Drag a box on the image. Expected: a rounded box in the fill colour, the
   Callout panel in the inspector (text field, Bubble / Box, fill swatches, size, corners) and a
   tail. Click once more on the image, or drag the blue dot labelled "Tail tip", to aim the tail.
   Expected: the tail base is on the box edge nearest to the tip.
7. **Move keeps the tip.** Drag the box to another place. Expected: the tip stays where it was.
   Switch Bubble to Box. Expected: a rectangle with a straight triangular tail; the colour and
   text stay.
8. **Grows to fit.** Type a long sentence into the text field or on the canvas. Expected: the box
   grows to fit the text; the handles sit on the drawn box; after you finish (Esc) and select it
   again the handles still sit on the drawn box. (ANN-4.AC5)
9. **Vietnamese typing.** With a Telex keyboard type `Tieengs Vieetj` in the callout. Expected:
   `Tiếng Việt`, no doubled letters. (ANN-4.AC5, ANN-1.AC7)
10. **One undo step.** Undo after placing the callout. Expected: it disappears whole (box, text,
    tail); Redo brings it back.

## C. Spotlight (S)

11. **Dim.** Press S and drag a rectangle over the part that matters. Expected: everything outside
    dims to about 40 % of its brightness, the inside stays as it was, and the Spotlight panel shows
    "Holes 1 of 8", Dim 60 %, Outside the holes: Dim / Blur, and the note about one spotlight per
    image.
12. **One per image, more holes.** With the Spotlight tool still on, drag a second shape (use the
    Ellipse button in the panel for an ellipse). Expected: it becomes a second hole in the same
    spotlight ("Holes 2 of 8"), not a second dim layer. Undo once. Expected: only the second hole
    goes. Add holes up to 8; a ninth is refused or the add button is disabled. (ANN-7.AC3)
13. **Order.** Draw an arrow, then add a new spotlight on an image that has none. Expected: the
    arrow stays bright (the spotlight sits below it). Then draw a blur over some text after the
    spotlight exists: the blur still hides the text. (ANN-7.AC4)
14. **Blur mode is only a look.** Set Outside the holes to Blur. Expected: the outside is blurred
    with a Strength slider; the panel warns that blur here is only a look and points to Blur (B) and
    Pixelate (X). Press Copy text with a line of text outside the holes. Expected: that text is not
    copied (it is read from the rendered picture); in Dim mode it is copied. (ANN-7.AC5)

## D. Steps (N) and the Steps panel

15. **Place four.** Press N and click four times. Expected: badges 1 to 4 and the Steps panel with
    Numbering (1 2 3, A B C, a b c), Start at, and an order list with a grip, a badge and a delete
    button per row.
16. **Style and start.** Choose A B C and set Start at 3. Expected: the badges read C, D, E, F. Choose
    a b c: lower case. Set Start back to 1 and the style to numbers.
17. **Label.** Select a step and type `Open Settings` into Label. Expected: a dark pill with the text
    to the right of the badge. Move that step to within one label width of the right edge.
    Expected: the pill flips to the left of the badge. (ANN-6.AC3)
18. **Reorder.** Drag the fourth row in the list to the top. Expected: the row being dragged is
    lifted with a blue border; the numbers on the image follow; one Undo puts all four back
    (one revision). (ANN-6.AC4)
19. **Delete renumbers.** Delete step 2. Expected: the remaining badges read 1, 2, 3. (ANN-1.AC2)
20. **Long run.** Use style A B C and make 27 steps (press N and click 27 times). Expected: the 26th
    reads Z, the 27th AA. (ANN-6.AC2)
21. **Vietnamese label.** Type `Mở cài đặt` through the Telex or VNI keyboard into the Label field.
    Expected: the accents compose correctly.

## E. Stamps (U)

Skip this section if stamps were cut from 0.3.3 and write "not built" in the report.

22. **Place.** Press U (or choose Stamp in the Markers menu). Expected: a picker with the four
    symbols (check, cross, exclamation, question) and BEFORE / AFTER labels. Click the image after
    choosing each. Expected: the symbols are drawn as shapes in the chosen colour and size, the same
    on every system; BEFORE and AFTER appear in the app language when placed.

## F. The view bar

23. **Order.** Expected at the bottom centre of the stage: Undo, Redo | Copy text, Auto-redact |
    zoom out, the zoom value with a chevron, zoom in | Images, Keyboard, the three-dot button.
    There is no separate Fit or 100 % button any more. Undo and Redo are dimmed when there is
    nothing to undo or redo.
24. **Zoom menu.** Click the zoom value. Expected: Fit to window, Actual size (100%), 200% and, when
    the zoom is something else, a checked "Custom: n%" row. Choose each and check the image.
    Zoom in and out with the buttons and see the value change.
25. **Revert to original.** Open the three-dot menu. Expected: Revert to original... in the warning
    colour with the helper line "Removes every annotation, crop and redaction." Choose it on an
    image with annotations. Expected: the same confirmation as before; after Confirm the image is
    the original (Cancel leaves everything).
26. **Copy text and Auto-redact stay.** Press Copy text (its tooltip explains that it copies all the
    text, while Select text (G) copies only a selection) and Auto-redact. Expected: both work as in
    0.3.2 (`tests/manual/ANN-2.md` and `ANN-5.md`).

## G. The image strip

27. **Shown.** With three or more images in the library view you opened this one from, expected: a
    strip of thumbnails under the stage with previous and next buttons at the ends. The current
    image has a red ring and is scrolled into view; images with annotations show a small count pill;
    there are no names under the thumbnails.
28. **Same list as the Library.** Go back to the Library, filter or search so only two images show,
    and open one. Expected: the strip lists those two, in the Library's order. Clear the filter,
    sort by another order, open an image: the strip follows that order.
29. **Hover.** Hover a thumbnail. Expected: a tooltip with the full name, and a second line with
    the date, the number of annotations and the position (for example `5 of 24`).
30. **Switch.** Click another thumbnail; use the previous and next buttons; use the app's existing
    previous and next shortcuts. Expected: the image changes at once, no "Save changes?" prompt
    ever appears, and each image keeps its own undo history (annotate A, switch to B, annotate B,
    switch back to A: Undo undoes A's edit, not B's).
31. **Hide.** Press the Images button. Expected: the strip hides and the button turns off; quit and
    reopen the app: it stays hidden. Show it again. Make the window shorter than 640 px high.
    Expected: the strip hides by itself and the shortcuts still work.
32. **One image only.** Open an image from a view that lists only that image. Expected: no strip.
33. **Keyboard and screen reader.** Tab into the strip. Expected: arrow keys move through the
    thumbnails, Enter opens one and focus returns to the canvas. With VoiceOver or Narrator the
    options read like "name, 3 annotations, 3 of 24".

## H. Everything together

34. **Save and reopen.** Make one image with a spotlight, a callout, labelled steps in A B C and
    (if built) a stamp. Close the editor and reopen the image from the library. Expected: all of it
    is back exactly as drawn; `capturemann capture get <id> --out x.png` writes the same picture
    as the editor shows.
35. **CLI parity.** Copy the `ops` array from the image's `manifest.json` into a file, run
    `capturemann annotate <id> --ops ops.json --replace --out y.png`. Expected: the same picture as
    the editor shows. (ANN-4.AC6)
36. **Pseudo-locale.** Start with `CAPTUREMANN_PSEUDO_LOCALE=1` and look at the Steps, Spotlight and
    Callout panels (300 px wide) and the palette tooltips. Expected: no cut-off or overlapping text.

Pass criteria: steps 1 to 4 and 6 to 36 as written (step 5 is recorded only; section E is "not
built" if stamps were cut); no crash; no prompt to save when switching images; nothing in the
editor changes an image you did not edit. Report in `verification/` (template in its README); say
which systems and which steps were not run.

## Report

Copy this block into `verification/ANN-6-<date>-<who>.md` and fill it in.

```markdown
# ANN-6 report

- **Date:**
- **Who:**
- **Machine:** model, chip or CPU, system version, display(s)
- **Build:** git commit, release or debug, bundle or `cargo run`
- **Fixture:** which screenshots, library size

## Steps

| Step | Result (PASS / FAIL / not built / skipped) | Notes |
| --- | --- | --- |
| 1 to 5 (palette) | | |
| 6 to 10 (callout) | | |
| 11 to 14 (spotlight) | | |
| 15 to 21 (steps) | | |
| 22 (stamps) | | |
| 23 to 26 (view bar) | | |
| 27 to 33 (image strip) | | |
| 34 to 36 (together) | | |

## Acceptance criteria

| AC | Result | Evidence (step, screenshot) |
| --- | --- | --- |
| ANN-4.AC5 | | 8, 9 |
| ANN-4.AC6 | | 35 |
| ANN-6.AC2, AC3, AC4 | | 17, 18, 20 |
| ANN-7.AC3, AC4, AC5 | | 12, 13, 14 |

## Problems found

-

## Verdict

PASS / FAIL / PARTIAL (which steps were not run and why)
```
