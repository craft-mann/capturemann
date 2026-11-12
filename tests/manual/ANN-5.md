# ANN-5 manual script: Copy text from a screenshot

Covers what the automated tests cannot: the real recognizer reading a real screenshot, the Copy
text button, the Select text tool, the library menu entry and the capture toast seen by eye, and
what lands on the real clipboard. Everything else (reading order, columns, paragraph gaps, the
region crop, the rendered-image rule, limits, `not_implemented`, MCP and CLI parity) is covered by
`cargo test -p cm-render -p cm-daemon -p cm-mcp -p cm-cli` with a scripted recognizer, and the
real recognizer on synthetic PNGs by `CM_OCR_REAL=1 cargo test -p cm-render --test ocr_text_real
-- --ignored --nocapture`.

Text recognition runs on this computer and sends nothing anywhere. These steps read your screen
once for the screenshot: use a window you made up for this, never real secrets, and delete the
captures afterwards. Use the bundle (`make run-bundle-dev`) when permissions matter.

Setup. Make a note or text file with made-up content in two parts, for example:

```
Release notes for the demo app

First paragraph. It has two lines of
ordinary English text.

Second paragraph after a blank line.
Contact: jane.doe@example.com
```

and, if you write Vietnamese, one more line such as `Xin chào, đây là ảnh chụp màn hình`.
Show it at 14 pt or larger and take a screenshot of that window.

1. **Button.** Open the screenshot in the editor. Expected: a "Copy text" button in the view bar at
   the bottom of the stage (0.3.3; before it was a row under the canvas), after Undo and Redo, with a text-frame icon. On Linux without `tesseract`, and on
   Windows without an installed OCR language, the button is dimmed and its tooltip says what to
   install; on those systems install it as the tooltip says and restart the app, then continue.
2. **Whole image.** Click Copy text. Expected: the button shows "Reading text..." for a moment,
   then a toast "Copied N words" (N matches your text roughly). Paste into a text editor.
   Expected: the lines in reading order, the blank line between the two paragraphs kept, no
   trailing spaces, line breaks where the lines break. (ANN-5.AC1, AC4)
3. **Region.** Press G (or choose Select text, in the Select group of the tool palette: click the corner mark of the first button) and drag a rectangle
   over only the "Contact" line. Expected: a translucent blue box while dragging, then a toast
   "Copied 2 words" (or the number of words in the line) and only that line on the clipboard. A
   click without dragging, or a drag smaller than a few pixels, does nothing. (ANN-5.AC3, AC7)
4. **Nothing there.** Drag over an empty part of the image. Expected: a toast "No text found"
   and the clipboard keeps what it had. (ANN-5.AC4)
5. **Blurred text is not copied.** Blur the Contact line with the Blur tool (strength 12 or
   more). Press Copy text again. Expected: the pasted text no longer contains the address.
   Press undo, copy again: the address is back. (ANN-5.AC2)
6. **No edit, no undo step.** After steps 2 to 4 the status bar still says "All changes saved",
   Undo is only enabled for edits you made, and the library shows no new revision. (ANN-5.AC4)
7. **Library menu.** In the library, open the item menu of the screenshot (right click or the
   menu button). Expected: a "Copy text" entry between Reveal and Rename; choosing it gives the
   same toast. A recording or audio item has no such entry.
8. **Capture toast.** Take a new screenshot. Expected: the toast has a "Copy text" button next to
   Copy. Press it. Same toast and clipboard result.
9. **Vietnamese (macOS).** Take a screenshot of the Vietnamese line and press Copy text. Record
   what comes back (tone marks right, wrong or missing) in the report; this is the check of what
   the recognizer reads, not a pass-or-fail step.
10. **Other languages by system.** On Windows only the OCR languages installed in Windows are read;
    on Linux `tesseract` reads English unless other language data is installed. Text in an
    uninstalled language comes back wrong or empty, and that is expected: the tooltip and the
    docs say so.
11. **Large screenshot.** Take a screenshot of a whole 5K display. Press Copy text. Expected: the
    editor stays responsive (you can pan and zoom) while "Reading text..." shows, then the toast.
12. **CLI and agents.** `capturemann text <id> --copy` prints the same text and copies it;
    `capturemann text <id> --region x,y,w,h --lines` prints boxes in original-image pixels.

Pass criteria: steps 1 to 8 as written; step 9 and 10 recorded; step 11 without a frozen window.
Report in `verification/` (template in its README); say what was not run.
