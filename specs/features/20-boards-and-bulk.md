# Boards and Bulk export (BRD, BLK)

**0.3.4, maintainer plan of Nov 14, 2026** (`specs/plans/release-0.3.4-plan.md`). UI names: **Boards** and **Bulk export**.

**Rules that apply to the whole file**
- **Storage.** A board is a **new capture** (an ordinary screenshot capture: `kind: "screenshot"`, manifest `source: "board"` plus the layout data, so a 0.3.3 app shows it as a picture) that **copies** its source images flattened, with their annotations applied, as they were when added. A board does not change when a source changes, and deleting a source does not break it. "Refresh from sources" is out of scope.
- **Bulk export never changes the originals.** It writes new files to a chosen folder, only from the rendered images, so annotations and redactions are always applied. A watermark is drawn on the exported file, not stored as an op.
- **WebP** is offered only as lossless and only if it adds no dependency (the `image` crate in the tree writes lossless WebP only); otherwise it waits.

### BRD-1 Boards (several screenshots in one image)
**Priority** P1 · **Status** In progress (0.3.4; Track E, Nov 14, 2026: Pair, Grid and Strip, `create_board`, `capturemann board` and the Make a board sheet are built and tested with fakes, the sheet is not seen by eye; Free is built in the renderer and the tool but has no drag canvas in the sheet) · **Crates** cm-render, cm-library, cm-mcp, apps/desktop

**User story.** As someone presenting a change, I want two or more screenshots side by side with labels, so that I can show before and after, or a short sequence, in one picture.

**Description**
- **Start:** select 2 to 12 screenshots in the library, then **Make a board**. Or start an empty board and drag screenshots in.
- **Layouts:**
  - **Pair** (2 side by side or stacked, with optional BEFORE and AFTER labels, reusing the stamp words of ANN-4).
  - **Grid** (2 × 2 or 3 × 3, filled in order).
  - **Strip** (a numbered row or column of frames, for a sequence).
  - **Free** (place and size each image yourself).
- **Settings:** gap, outer margin, background (**transparent, white, light grey, dark or a tint colour**; no screenshot background setting exists, so this list is the definition; transparent is for PNG, and a JPEG or lossy export puts it on white), corner radius, shadow, a caption under each image, and one title.
- **Storage** (storage rules below): the board is a new capture holding flattened copies of the sources. After **Finish** it is a normal screenshot capture that can be annotated, redacted, copied and shared. Until then, the layout stays editable.
- **AC1** Two screenshots of different sizes in Pair, side by side. Both are scaled to the same height, the gap and margin are as set, and the export matches the golden image within 1 %.
- **AC2** A source with a blur. The board shows the blur, and its copy holds no unblurred pixels.
- **AC3** Deleting a source after Finish leaves the board unchanged.
- **AC4** (agent) `create_board { capture_ids, layout, options }` → `{ capture_id, image }` (Free; it creates a new capture and changes nothing else).

### BLK-1 Bulk export
**Priority** P1 · **Status** In progress (0.3.4; Track E, Nov 14, 2026: `export_images`, `capturemann export-images` and the Export selected sheet are built and tested with fakes, the sheet is not seen by eye; WebP is lossless and adds no crate to `Cargo.lock`; the Blueprint step waits for the `blueprint` op) · **Crates** cm-render, cm-daemon, cm-library, apps/desktop

**User story.** As someone preparing images for a site or a manual, I want to resize, convert, watermark or rename many screenshots at once, so that I don't do it image by image.

**Description**
- **Start:** select screenshots in the library, then **Export selected…**, which opens a sheet with these steps, each optional:
  - **Size:** a width, a height, a longest side or a percentage; never larger than the original unless the user switches that on.
  - **Format:** PNG, JPEG (quality 60 to 100), or WebP if the encoder is already in the tree; otherwise WebP waits.
  - **Watermark:** text or a PNG, a corner or the centre, opacity, and a size relative to the image.
  - **Blueprint:** on or off (ANN-11), with the default keep-list (none).
  - **File names:** a pattern with `{name}`, `{date}`, `{n}` and `{w}x{h}`, and a duplicate rule (number them, never overwrite).
- **Summary:** `23 exported, 1 skipped` lists each skipped file with the reason and offers **Show in folder** and **Retry skipped** (a new job for the skipped files only).
- **Runs as one background job** with progress, Cancel, and a summary (`23 exported, 1 skipped: no space`). The originals are never changed (the originals are never changed).
- **AC1** 50 screenshots exported as JPEG at a longest side of 1600 px with a text watermark gives 50 files of the right size, each with the watermark. The library entries are unchanged.
- **AC2** Cancel at 50 % keeps the files already written and leaves no partial file.
- **AC3** A file with the same name already in the folder. The new file gets a number; nothing is overwritten.
- **AC4** (agent) `export_images { capture_ids, dest, size?, format?, watermark?, blueprint?, name_pattern? }` → a job, like `export_video` (Free, writing only to the chosen folder).

