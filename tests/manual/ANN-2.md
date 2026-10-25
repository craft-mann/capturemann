# ANN-2 manual script: Auto-redact (emails and keys) in the annotation editor

Covers what the automated tests cannot: Apple Vision reading a real screenshot, the Auto-redact
button, the dashed boxes and the inspector card seen by eye, and the exported pixels. Everything
else (the detector on fixtures, `suggest_redactions` through daemon, MCP and CLI with a scripted
recognizer, one undo step for accepting, `not_implemented` without a recognizer) is covered by
`cargo test -p cm-render -p cm-daemon -p cm-cli -p cm-mcp -p cm-desktop`, and the real recognizer
on a synthetic PNG by `CM_OCR_REAL=1 cargo test -p cm-render --test ocr_real -- --ignored
--nocapture`. These steps read your screen once for the screenshot: use a window you made up for
this, never real secrets, and delete the captures afterwards.

Use the bundle (`make run-bundle`) when permissions matter. Text recognition runs on this Mac and
sends nothing anywhere.

Setup. Make a text file or a note with made-up content, for example:

```
Owner: jane.doe@example.com
Billing: ap@vendor.example.org
API key: sk-proj-A1b2C3d4E5f6G7h8I9j0K1l2
Server: 192.168.10.24    Phone: +1 (415) 555-0132
Plain sentence with no secrets in it.
```

Show it at a normal size (14 pt or larger) and take a screenshot of that window or region.

1. **Button.** Open the screenshot in the editor. Expected: an "Auto-redact" button in the row under
   the canvas, next to Revert. (On a machine without a recognizer, for example Windows today, the
   button is not there at all: see step 9.)
2. **Propose.** Press Auto-redact. Expected: a moment of "Looking for emails and keys...", then
   dashed boxes with a light tint over the two email addresses, the API key and, at lower
   confidence, the IP address and the phone number. The inspector shows an "Auto-redact" card:
   "N suggested. Nothing is blurred until you accept", one row per box (kind and masked text such
   as `ja...om`, never the full text) and the buttons Accept all and Dismiss. **Nothing is blurred
   yet**: the text under the boxes is still readable and the status bar still says "All changes
   saved". (ANN-2.AC4)
3. **Boxes fit.** Each box covers its own address or key, with a small margin and nothing much
   beyond it. The email in the middle of a sentence (try "Contact jane.doe@example.com today") gets
   a box for just the address. A box that is clearly too wide or too narrow is a finding to report
   (the span inside a word is estimated in proportion to the characters).
4. **Accept one.** Click inside one dashed box (or press its Accept button). Expected: that box
   turns into a blur right away, the other boxes stay, the card shows one fewer row. The history
   shows one new step.
5. **Accept all.** Press Accept all. Expected: every remaining box is blurred, the card goes away
   (or says nothing is left), and the status bar says Saving then All changes saved.
6. **Undo.** Press Cmd/Ctrl+Z once. Expected: only the blurs from Accept all disappear (one step);
   a second undo removes the single blur from step 4. Redo brings them back the same way.
7. **Dismiss.** Press Auto-redact again, then Dismiss (or Esc). Expected: the boxes and the card
   disappear and no blur is added. Press Auto-redact on an image with no secrets (a plain
   screenshot of a text-only page): the card says "No emails or keys found in this image."
8. **Export check.** With every secret blurred, press Copy image (or Save as) and open the result
   in Preview. Expected: the blurred areas cannot be read, even zoomed in 400 percent, and the rest
   of the image is untouched. Run the CLI on the saved copy to be sure nothing is left:
   `capturemann annotate <id> --suggest-redactions` on the screenshot still lists the secrets
   (it reads `original.png`, which stays unblurred and private in the library), but the exported
   PNG, imported as a new screenshot or opened in any OCR tool, shows none of the characters.
9. **No recognizer.** On Windows (stub) or after hiding Vision, the Auto-redact button is not
   shown, and `capturemann annotate <id> --suggest-redactions` exits 1 with a `not_implemented`
   message saying text recognition is not available. Annotating by hand still works.
10. **Agents.** `capturemann annotate <id> --redact-emails --json` prints two rects and applies
    nothing (`capturemann capture get <id>` shows no ops). In an MCP client, `suggest_redactions`
    is listed as a Free, read-only tool and its result has masked `text_preview` values only.

Report: write the run to `verification/` (template in its README). Delete the test captures from
the library when done.
