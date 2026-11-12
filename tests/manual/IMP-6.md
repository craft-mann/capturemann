# IMP-6 manual script: importing images

Covers IMP-6 (`specs/features/18-import.md`) where the window and the file manager matter: dragging
files onto the library, the Import sheet rows, a real phone photo, and what happens next in the
editor. The header checks, limits, normalisation and the `import_media` rows are covered by
`cargo test -p cm-media -p cm-library -p cm-daemon`, and the CLI path (a PNG and a JPEG imported,
annotated and exported) by the `cm-regress` case `import.images`. Nothing here records the screen,
the camera or the microphone.

Use made-up pictures only (no real person, no real screen). Delete the imported captures
afterwards. For step 6 you need a copy of a photo taken with a phone held upright (portrait), which
has an EXIF orientation of 6 or 8; use a photo of a plain object, never of a person or a place you
do not want stored.

Setup. Make these files in one folder:

1. `plain.png` any screenshot or picture saved as PNG.
2. `shot.jpg` a JPEG (export any picture as JPEG).
3. `phone.jpg` the phone photo above (portrait).
4. `huge.png` an image over 50 megapixels or over 16 384 px on a side, for example 9 000 x 6 000
   px (any tool that can make a blank image of that size; the file can be small).
5. `photo.webp` a WebP picture, and `photo.heic` a HEIC picture (an iPhone photo exported as is).
6. `fake.png` a GIF or JPEG file renamed to `.png`.
7. `text.png` a screenshot of a made-up window with an email address and a sentence in it.

## Steps

1. **Import sheet.** Library, header Import... (or Ctrl+I / Cmd+I). Choose `plain.png` and
   `shot.jpg`. Expected: the sheet lists both with their size in pixels and a status like other
   files; nothing about preparing a copy. Press Import. Expected: two screenshots appear in the
   library with the Imported mark; the Imported filter shows them; each opens in the screenshot
   editor, not the video editor.
2. **Drag and drop.** Drag `plain.png` and `shot.jpg` from the file manager onto the library
   window. Expected: the Import sheet opens with both listed (IMP-6.AC9), then the same result as
   step 1. Drag a mix of `shot.jpg`, a video file and a folder. Expected: the folder row says
   folders are not supported, the other rows have their own status, and nothing is imported until
   you press Import.
3. **Original and metadata.** Reveal the imported bundle (library item menu, Reveal). Expected: a
   `.cmshot` folder with `original.png`, a thumbnail and `manifest.json` whose `source` has the
   original file name; no copy of the source JPEG. The image size in the editor equals the size of
   the file in pixels.
4. **Annotate, redact, Copy text, export.** Open `text.png` imported. Draw an arrow and a callout,
   run Auto-redact and accept the email, press Copy text on the sentence. Expected: everything works
   as on a screenshot taken here; the pasted text has no blurred email; export with Copy or Save as
   PNG gives the annotated picture, and Revert to original (three-dot menu) brings the plain image
   back.
5. **Transparent PNG.** Import a PNG with a transparent background. Expected: the transparency is
   kept (the editor shows its checkerboard or the app's backdrop, an exported PNG is still
   transparent) and the sheet row does not say "converted" unless the file was 16-bit or palette.
6. **Phone photo orientation.** Import `phone.jpg`. Expected: it opens upright, the same way your
   photo viewer shows it, not on its side. (IMP-6.AC2)
7. **Too large.** Import `huge.png`. Expected: the row says Can't import and that the image is too
   large (over 16 384 px a side or 50 megapixels); the app stays responsive and does not grow in
   memory; nothing is added to the library. (IMP-6.AC3)
8. **WebP and HEIC are refused.** Import `photo.webp` and `photo.heic`. Expected: each row says Can't
   import and names the format ("WebP", "HEIC"), nothing is added. (IMP-6.AC4)
9. **Content decides.** Import `fake.png`. Expected: if it is a GIF, the row names GIF and refuses
   it; if it is a JPEG it imports as a JPEG. (IMP-6.AC4)
10. **Broken file.** Make an empty file named `empty.png` and cut a copy of `shot.jpg` in half.
    Import both. Expected: both rows say the file could not be read; nothing is created. (IMP-6.AC6)
11. **Remove originals.** In the Import sheet tick "Remove the originals" for a copy of `plain.png`
    and press Import. Expected: a confirmation dialog first (Confirm tier); after Allow the copy
    goes to the Trash/Recycle Bin only once the capture is in the library; after Deny nothing is
    imported and the file stays. (IMP-6.AC8)
12. **Agents and CLI.** `capturemann import plain.png shot.jpg --wait` prints two finished
    captures. `capturemann import --dry-run huge.png photo.webp text.png` prints a row per file and
    imports nothing. `capturemann annotate <id> --suggest-redactions` and `capturemann text <id>`
    work on the imported `text.png`. (IMP-6.AC7)
13. **Many images.** Drop 20 mixed PNG and JPEG files. Expected: one sheet, 20 rows, the import
    runs in the background with progress, the library stays usable, Cancel stops the rest and
    keeps what was done.

Pass criteria: every step as written; no crash; no image added from a refused row; no sideways
photo; the originals of a refused or cancelled import are untouched. Report in `verification/`
(template in its README); say which systems were not run.

## Report

Copy this block into `verification/IMP-6-<date>-<who>.md` and fill it in.

```markdown
# IMP-6 report

- **Date:**
- **Who:**
- **Machine:** model, chip or CPU, system version
- **Build:** git commit, release or debug, bundle or `cargo run`
- **Fixture:** the files used (sizes in pixels, how the phone photo was made)

## Steps

| Step | Result (PASS / FAIL / skipped) | Notes |
| --- | --- | --- |
| 1 | | |
| 2 | | |
| 3 | | |
| 4 | | |
| 5 | | |
| 6 | | |
| 7 | | |
| 8 | | |
| 9 | | |
| 10 | | |
| 11 | | |
| 12 | | |
| 13 | | |

## Acceptance criteria

| AC | Result | Evidence (step) |
| --- | --- | --- |
| IMP-6.AC2 | | 6 |
| IMP-6.AC3 | | 7 |
| IMP-6.AC4 | | 8, 9 |
| IMP-6.AC6 | | 10 |
| IMP-6.AC8 | | 11 |
| IMP-6.AC9 | | 2 |

## Problems found

-

## Verdict

PASS / FAIL / PARTIAL (which steps were not run and why)
```
