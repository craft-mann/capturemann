# Release 0.3.4: screenshot catch-up (plan, draft, Nov 14, 2026)

Status: **merged Nov 14, 2026 (the maintainer's screenshot catch-up plan plus the plan of Nov 13); Phase 0 spec text written (section 10); nothing built.** It replaces the 0.3.4 plan of Nov 13, 2026. The maintainer asked on Nov 14, 2026 for 0.3.4 to close the screenshot gaps found in the competitor review. The decided items of the old plan that are about images stay in this release: Magnify (ANN-8), canvas text selection, CMYK conversion and the hygiene list. Video features move to a later release (section 7). Nothing is built yet. The defaults in section 8 wait for the maintainer. After the specs are final, the maintainer hands the UI and UX design to Claude Design (section 11).

Input: the maintainer's gap list of Nov 14, 2026, which compares CaptureMann with Snagit and names 6 gaps:
1. scrolling capture;
2. a step recorder;
3. editing pixels in place;
4. a simplified UI view;
5. multi-image layouts and batch work;
6. a loupe and drawing live while recording.

**Naming rule.** CaptureMann features get their own names. No name in this plan copies a Snagit feature name. Op and tool names in JSON stay plain English verbs or nouns. Where a name was already frozen (the `magnify` op of ANN-8), the frozen JSON name stays and only the name in the UI changes.

## 1. The gap list, and what 0.3.4 does with each gap

| # | Gap | Spec today | In 0.3.4 | CaptureMann name |
| --- | --- | --- | --- | --- |
| 1 | Long pages, sheets and chats as one image | CAP-6 *Draft* (P2, no ACs) | **Yes**: full spec, automatic and hand scrolling, vertical and horizontal | **Long Shot** |
| 2 | One screenshot per click, numbered, as a guide | none (the persona "step guides" in `00-product-spec.md`) | **Yes**: record, edit, export Markdown and HTML; PDF if the spike passes | **Click Guide** |
| 3 | Change the text or move parts of a flat screenshot | none | **Lite**: on-device text recognition plus a background fill, for flat UI backgrounds. Object detection and inpainting come later | **Touch-up**: tools **Retype** and **Move area** |
| 4 | Replace UI text with neutral shapes so guides do not age | none | **Lite**: recognized text lines become placeholder bars. Icons come later | **Blueprint** |
| 5 | Several screenshots in one layout; bulk resize, convert and watermark | none | **Yes**: layouts as a new capture, bulk export from a library selection | **Boards** and **Bulk export** |
| 6a | A sharp zoomed copy of a detail | ANN-8 *Ready* (frozen op `magnify`) | **Yes** (unchanged from the old 0.3.4 plan) | **Zoom Lens** (op name stays `magnify`) |
| 6b | Draw on the screen while recording | REC-9 (button shown disabled, "Coming soon") | **No**: video, moved to a later release | (REC-9 keeps its name) |

New IDs (IDs are never renumbered):
- CAP-6 is detailed and moves from P2 to P1.
- **ANN-9** Retype, **ANN-10** Move area and **ANN-11** Blueprint go in `features/02-annotation.md`.
- **GDE-1 to GDE-4** go in the new file `features/19-click-guide.md`.
- **BRD-1** Boards and **BLK-1** Bulk export go in the new file `features/20-boards-and-bulk.md`.
- Add rows to the FR table in `00-product-spec.md`, and add the new tools to `02-agent-interface.md`.

## 2. Problems to settle before building (Phase 0)

1. **Long images and the renderer's limits.** CAP-6 says "max height 30,000 px", but IMP-6 and the annotation limits cap one side at 16,384 px, and so may the GPU textures used by `cm-render`. **Proposal:** in 0.3.4 a Long Shot stops at 16,384 px on its long side and at 50 megapixels, the same limits as an imported picture. The capture stops cleanly with "Reached the longest image CaptureMann can edit". Tiled rendering, to reach 30,000 px or more, is a later item. This changes the CAP-6 text.
2. **Synthetic scrolling needs permission and differs on each system.**
   - macOS posts scroll events with `CGEventPost`, which needs **Accessibility**. Today Accessibility is "not needed" (REC-7), so the permission page gets a new optional row.
   - Windows uses `SendInput` with no extra permission.
   - Linux X11 uses XTest.
   - **Wayland has no way to inject scrolling.** So there are two modes:
     - **Auto scroll** on macOS, Windows and X11.
     - **Hand scroll** on every system, including Wayland: the user scrolls, and CaptureMann takes a frame each time the content settles.
3. **Stitching must not invent or lose rows.** Frames are matched by their overlapping rows (an exact or near-exact row hash, then a check on the pixels). Fixed headers and footers are found as bands that stay the same in every frame, and they are kept once. If no overlap is found (a jump, an animation, a virtualized list that redraws), the capture **stops there and keeps what it has**, with a note. It never guesses a join. Horizontal scrolling uses the same method on columns.
4. **Touch-up changes pixels people believe are real.** Retype and Move area paint over the original. Three rules:
   - They are **base edits**. In the list they come before every other op, and the editor inserts them there. `annotate` refuses a base edit that comes after a non-base op (`invalid_input` naming `ops[i]`). So the Zoom Lens, blur and Copy text all see the edited picture the same way, and no list order can show the old text next to the new.
   - `original.png` is never changed, as for every op (ANN-3), so Revert to original still works.
   - **They are not redaction.** The fill hides the old pixels in the export, but the spec says to use blur or pixelate for secrets. The Auto-redact detector still runs on the rendered image.
5. **Background fill: what "lite" means.** The fill is computed from the ring of pixels around the box: a solid colour when the ring is flat, a linear gradient when it changes smoothly. When the ring varies too much (a photo or a pattern), the tool says "The background is too busy to fill cleanly" and offers to go ahead anyway. The fill colours are **stored in the op** when it is created, so the editor, the CLI and agents render the same (ANN-1.AC6). No model and no network are involved.
6. **Matching the font is approximate.** Retype keeps the box, the text colour (the darkest or lightest cluster inside the box) and the cap height, measured from the box. It draws in the system UI font (San Francisco, Segoe UI, Cantarell or Ubuntu, through the existing `cosmic-text` fallback). The spec says plainly that a custom font is not matched.
7. **Blueprint is computed once.** `suggest_blueprint` runs text recognition and returns boxes. The `blueprint` op stores those boxes, so a render never runs OCR again. That makes the render deterministic and lets the user drop single boxes (for example, keep the heading the guide talks about). Blueprint is also a base edit, under rule 4.
8. **Click Guide and privacy.** It listens to global clicks, which needs Input Monitoring on macOS. CaptureMann already asks for this for cursor telemetry (REC-7). Rules:
   - It **never records keystrokes**.
   - It shows a HUD with the step count, Pause and Stop, and the HUD is excluded from the shots (REC-6).
   - Auto-redact runs on each step and proposes blurs, the same as in the editor; nothing is applied until the user accepts.
   - A click inside the CaptureMann HUD is not a step.
9. **When the frame is taken.** Many UIs change on mouse-up. CaptureMann takes the frame on **mouse-down** and keeps the click point, so the step shows what the user clicked, not the result. A spike measures the latency on each system (S3).
10. **Boards and storage.** A board is a **new capture** (kind `board`) that stores its layout and **copies** its source images flattened, with their annotations applied, as they were when added. A board does not change when a source changes, and deleting a source does not break it. A "Refresh from sources" command is out of scope.
11. **Bulk export never changes the originals.** It writes new files to a chosen folder, and only from the rendered images, so annotations and redactions are always applied. A watermark is drawn on the exported file, not stored as an op.

12. **Base edits and older apps (found while merging).** Rule 4 says base edits come first, but 0.3.3 keeps unknown ops in `unknown_ops` and writes them back after the known ops, so a file saved by 0.3.3 could hold a base edit after an arrow. **Rule:** the renderer and the editor **sort base edits to the front when they load a file** (stable, so their own order is kept) and the next save writes them in that order; only `annotate` input is strict (`invalid_input`). Test: a manifest with `retype` after `arrow`, as 0.3.3 would write it, renders the same as the sorted list. A 0.3.3 app shows the original pixels where a base edit applies and says "Kept an annotation this version cannot show"; it never drops the op.
13. **New capture kinds and older apps.** A **guide** is a new capture kind (`guide`, folder extension `cmguide`). A 0.3.3 app does not index a folder with an unknown extension (`CaptureKind::from_extension` returns none; the watcher skips it), so a 0.3.4 library opened by 0.3.3 simply does not list guides and does not damage them. A **board** is an ordinary screenshot capture (`kind: "screenshot"`, manifest `source: "board"` and `board` layout data), so 0.3.3 shows it as a picture. `CaptureKind` gains `Guide`; every `match` on it is checked in the build track.
14. **WebP.** The `image` crate is built with `png` and `jpeg` only; `image-webp` is in the tree for decoding. Writing WebP with that crate is **lossless only**. BLK-1 offers WebP (lossless) if the feature adds no new dependency and the size check passes; lossy WebP is not in 0.3.4.
15. **Windows window drag (B3).** Not a catch-up feature, but the Click Guide and Long Shot HUDs and the Zoom Lens editor panels use floating windows too. The Windows backend has no window-move call (section 9, B3); every new floating window declares a drag region where it needs one.

## 3. Feature specs

### CAP-6 Long Shot (scrolling capture), detailed
**Priority** P1 (0.3.4) · **Crates** cm-capture, cm-render, cm-daemon, apps/desktop · **Depends on** CAP-1, CAP-4, PRM

**User story.** As someone who documents or reports bugs, I want a whole web page, spreadsheet or chat thread as one image, so that I don't paste five screenshots in a row.

**Description**
- **Start:** a **Long Shot** mode in the capture overlay, a launcher command, and a configurable hotkey with no default. The user picks a window, then the scrollable area inside it (CaptureMann highlights the area under the pointer). A region drag works too.
- **Auto scroll** (macOS with Accessibility, Windows, X11):
  - CaptureMann scrolls by about 80 % of the area height each time, and waits until two frames 50 ms apart are the same (to a 150 ms limit).
  - It stops at the end (no change after a scroll), at the limit (problem 1), or when the user presses Esc or Stop. Stopping early keeps what has been captured.
- **Hand scroll** (every system; the only mode on Wayland): the user scrolls. CaptureMann takes a frame whenever the content settles and shows the image growing in a side preview.
- **Direction:** vertical by default. Horizontal is a switch in the HUD (and the `direction` parameter); auto scroll uses horizontal wheel events.
- **Stitching:** as in problem 3. The result is a normal screenshot capture that can be annotated, redacted, exported and shared like any other. Its manifest records `source: "long_shot"`, the frame count and the stops found.
- **Retina and HiDPI:** frames stay in physical pixels. Mixed-DPI displays are not a case, because the area belongs to one window.

**Acceptance criteria**
- **AC1** A test page 5 screen heights long with a numbered line on every row. After an auto-scroll Long Shot, every number appears exactly once and in order: no duplicated or missing rows (an OCR check plus a row-hash check).
- **AC2** The same page with a fixed header and footer. The stitched image shows each of them once, at the top and the bottom.
- **AC3** A page that jumps (no overlap between two frames). The capture stops at the last good join, keeps the frames before it, and says "Stopped: the next part did not line up". It never guesses a join.
- **AC4** A page longer than the limit. The capture stops at 16,384 px with the limit message, and the image is saved.
- **AC5** Wayland. Hand scroll is the only mode offered, and AC1 passes when the test scrolls by hand (a scripted wheel through the test page's own JavaScript).
- **AC6** macOS without Accessibility. Auto scroll is shown disabled with "Allow Accessibility to scroll for you", a button to grant it, and Hand scroll still works.
- **AC7** (agent) `capture_screenshot { mode: "long", target, direction?, max_px? }` returns the stitched image, like a normal capture. Auto scroll from an agent is **Confirm** tier because it moves the user's screen, and Hand scroll is refused for agents (`invalid_input`, reason `needs_user`).

**Out of scope:** tiled images over 16,384 px; scrolling inside nested frames in one shot; video of the scroll.

**Test notes:** golden stitching tests on recorded frame sequences (no screen needed); a test page in `tests/fixtures/long-shot/`; `tests/manual/CAP-6.md` for Chrome, Excel or LibreOffice Calc, Slack or Discord, and a PDF viewer on each system.

---

### ANN-8 Magnify, shown as **Zoom Lens**
Unchanged from the Phase 0 text of Nov 13, 2026: the op JSON is frozen, and AC1 to AC9 stay as written. The only change is the name in the UI: the tool, the menus and the docs say **Zoom Lens**, while the op stays `magnify` and the key stays M. Add one AC:
- **AC10** Given a Retype or Blueprint base edit inside the lens source, then the lens shows the edited pixels, because base edits come first in the list (problem 4).

---

### ANN-9 Retype (change text in a screenshot)
**Priority** P1 (0.3.4, lite) · **Crates** cm-core, cm-render, cm-mcp, apps/desktop · **Depends on** ANN-5 (OCR), ANN-1, ANN-3

**User story.** As someone writing documentation, I want to fix a typo, a name or a number in a screenshot without taking it again, so that the guide matches what I describe.

**Description**
- **Tool:** Retype (key Y, in a new **Touch-up** group with Move area). Hovering shows the recognized text lines as dashed boxes; a click on one opens it for editing with the recognized text filled in. The user can also drag a box over text the recognizer missed and type.
- **Op:** `retype { rect, fill: { kind: "solid" | "linear", colors: [..], angle? }, text, size, color, weight, align }`. `fill` and the text style are measured when the op is created (problems 5 and 6) and stored. The new text is drawn in the system UI font, vertically centred in `rect`, and clipped to `rect` (with a warning in the panel when the text does not fit).
- **A base edit** (problem 4): the editor inserts it before the first non-base op.
- **Busy background:** the warning from problem 5, with Cancel or Use anyway.

**Acceptance criteria**
- **AC1** A screenshot of a dialog on a flat background where "Save" is retyped to "Save all". The exported PNG shows "Save all", OCR reads "Save all" and not "Save", and the pixels outside `rect` are unchanged.
- **AC2** A button with a vertical gradient. The fill is `linear`, and the largest difference between the fill and the original background ring is at most 6 levels per channel.
- **AC3** A photo background. The tool warns before it creates the op, and Cancel stores nothing.
- **AC4** `annotate` with a `retype` op after an arrow answers `invalid_input` naming `ops[i]` (base edits come first).
- **AC5** An agent sends the same op that the editor drew, and the renders are identical (ANN-1.AC6).
- **AC6** Revert to original removes every Retype, and `original.png` is byte-identical before and after.
- **AC7** An older app (0.3.3) keeps the op in `unknown_ops`, unchanged ("Kept an annotation this version cannot show").

**Out of scope:** matching custom fonts; text on photos; editing text inside a Zoom Lens.

---

### ANN-10 Move area
**Priority** P1 (0.3.4, lite) · **Crates** cm-core, cm-render, cm-mcp, apps/desktop · **Depends on** ANN-9 (fill), ANN-3

**User story.** As a designer or writer, I want to move a button, an icon or a block in a screenshot, or remove it, so that I can show a layout change or tidy the shot without a design tool.

**Description**
- **Tool:** Move area (key J, Touch-up group). Drag a rectangle, then drag its contents to a new place, or press Delete to remove them. The hole left behind is filled the same way as in Retype.
- **Op:** `move_area { source: [x, y, w, h], to: [x, y] | null, fill }`. `to: null` removes the contents. This is a base edit (problem 4).
- **Snapping:** while dragging, the area snaps to the source row or column (Shift locks the axis).

**Acceptance criteria**
- **AC1** A toolbar icon on a flat bar, moved 120 px to the right. The icon appears at the new place pixel for pixel, and the hole matches the bar colour within 6 levels per channel.
- **AC2** `to: null` removes the contents, and the area is filled.
- **AC3** The source overlaps the destination. The result is the same as cutting, filling, then pasting (one defined order, with a golden test).
- **AC4** to **AC6**: parity, base-edit order and older-app rules, the same as ANN-9 AC4, AC5 and AC7.

**Out of scope:** detecting objects automatically (a click to select an icon); filling busy backgrounds well. Both wait for a later on-device model (section 7).

---

### ANN-11 Blueprint (placeholder text)
**Priority** P1 (0.3.4, lite) · **Crates** cm-core, cm-render, cm-mcp, apps/desktop · **Depends on** ANN-5

**User story.** As a technical writer, I want the UI text in a screenshot replaced by neutral bars, so that my guide still fits when a label changes, and the reader looks at the layout and not at the words.

**Description**
- **Command:** **Blueprint** in the editor's view bar. CaptureMann runs text recognition, shows every text line as a bar preview, and lets the user click a line to keep its real text (for example, the button the step is about). Apply makes one op, in one undo step.
- **Op:** `blueprint { boxes: [[x, y, w, h], ...], style: "bar" | "box", color: "auto" | "#RRGGBB", radius }`. With `color: "auto"`, each bar takes the text colour of its line, lightened toward the background, and the computed colours are stored per box. The area behind each bar is filled as in ANN-9, so the old letters do not show around the bar. This is a base edit (problem 4).
- **Also for whole libraries:** Bulk export (BLK-1) can apply Blueprint to each image it exports.

**Acceptance criteria**
- **AC1** A settings window with 20 text lines, with Blueprint applied and 2 lines kept. OCR on the export finds only the 2 kept lines.
- **AC2** Each bar lies inside its line's box. The bars of one paragraph have the same height and line gap.
- **AC3** A keep-list change is one op update and one undo step.
- **AC4** The CLI and agents run `suggest_blueprint { capture_id }` → `{ boxes: [{ rect, text_preview, confidence }] }`, then `annotate` with a `blueprint` op. The render is identical to the editor's.
- **AC5** Copy text after Blueprint returns only the kept lines, because it reads the rendered image (ANN-5).

**Out of scope:** replacing icons and images (it needs detection); turning a whole UI into a wireframe.

---

### GDE-1 Click Guide: record
**Priority** P1 (0.3.4) · **Crates** cm-capture, cm-daemon, cm-library, apps/desktop · **Depends on** CAP-1, REC-6, REC-7 (click listening), ANN-5, ANN-6, PRM · **File** `features/19-click-guide.md` (new)

**User story.** As a support engineer or trainer, I want to click through a task once and get a numbered guide with a screenshot of each click, so that I don't capture and number 15 screenshots by hand.

**Description**
- **Start:** **New Click Guide** from the capture overlay, the tray, the launcher and the library. The user picks a display, a window or a region to follow, then presses Start (a 3-second countdown, as in REC-4).
- **Each mouse-down** inside the target becomes a step:
  - CaptureMann takes a frame of the target (problem 9) and records the click point and the button.
  - It adds a step badge at the click point and a highlight ring around it, as normal ops, so the user can edit them.
  - It writes a draft step title from the text recognized nearest the click, for example `Click "Save"`. With no text near the click, the title is `Click here`.
  - Double-clicks and right-clicks are noted in the title (`Double-click`, `Right-click`).
- **Step crop:** each step can show the whole target or a **focus crop** around the click (default 1200 × 800 physical px, centred, clamped to the target). Both are stored: the frame is the image, and the crop is a non-destructive `crop` op (ANN-1).
- **HUD:** step count, Pause, Add note step (a step with no image, for "Type your password"), Undo last step, and Stop. The HUD is excluded from the shots.
- **Privacy** (problem 8): no keystrokes; Auto-redact runs on each step in the background and marks the steps that have proposals.
- **Result:** a capture of the new kind `guide`. Its folder holds `guide.json` (the title, intro, and the ordered steps with title, note, image id and click point) and one screenshot per step. Each step image is a full screenshot capture inside the guide, so every editor tool works on it.

**Acceptance criteria**
- **AC1** A scripted run that clicks 5 buttons in a test app produces a guide with 5 steps in order. Each step image shows the button's state before the click, its badge is at the click point within 2 px, and its title names the button text.
- **AC2** Clicks on the HUD, or outside the target, add no step.
- **AC3** Pause, 3 clicks, Resume. No steps are added while paused.
- **AC4** One step shows an email address. That step is marked "Has redaction proposals", and nothing is blurred until the user accepts.
- **AC5** macOS without Input Monitoring. Starting shows the permission card and its button; nothing records.
- **AC6** Wayland. Global clicks may not be available. If the portal gives no input events, Click Guide is shown disabled with the reason, and the rest of the app is unaffected.

### GDE-2 Click Guide: edit
- **Guide editor:** a list of steps on the left (drag to reorder, delete, duplicate, insert a note step, retake a step's image) and the selected step on the right, with its title and note fields and its image. **Edit image** opens the normal annotation editor for that step and comes back to the guide.
- The guide title and an intro paragraph sit at the top. Step numbers follow the order, with the numbering style of ANN-6 (`1 2 3`, `A B C`).
- Every change is one undo step, through the same history as ANN-3.
- **AC1** Moving step 4 to position 2 renumbers the badges and titles in one undo step, and undo restores the old order.
- **AC2** Deleting a step leaves no orphan image in the guide folder after the next save.

### GDE-3 Click Guide: export
- **Markdown:** a folder with `guide.md` and an `images/` folder of PNGs (rendered, so annotations and redactions are applied). The links are relative, so it pastes into a GitHub wiki or a docs repo as is.
- **HTML:** one self-contained file with the images embedded, light and dark styles, a table of contents and print styles.
- **PDF** (*if spike S4 passes*): A4 or Letter, one or two steps per page. If the spike fails, PDF moves to 0.3.5 and HTML's print styles are the fallback.
- **Copy as rich text:** pastes into a ticket, Notion or Google Docs.
- DOCX and PPTX are out of scope (section 7).
- **AC1** The Markdown export of a 5-step guide renders on GitHub with 5 images, in order, with titles. No absolute paths.
- **AC2** In any export, a step image with a blur shows the blur. OCR finds no text inside the blurred rectangle.

### GDE-4 Click Guide: agent interface
- **New tools:**
  - `start_guide { target }` (**Confirm**, because it listens to clicks).
  - `stop_guide`.
  - `get_guide { guide_id }`.
  - `set_guide { guide_id, steps }` (reorder, titles, notes, delete; one revision).
  - `export_guide { guide_id, format: "markdown" | "html" | "pdf", dest }`.
- The CLI has the same commands as `capturemann guide ...`.
- A plugin skill, `write-a-how-to`, shows the flow: start, the user clicks, stop, the agent rewrites the titles from the step text, then exports.

---

### BRD-1 Boards (several screenshots in one image)
**Priority** P1 (0.3.4) · **Crates** cm-render, cm-library, cm-mcp, apps/desktop · **File** `features/20-boards-and-bulk.md` (new)

**User story.** As someone presenting a change, I want two or more screenshots side by side with labels, so that I can show before and after, or a short sequence, in one picture.

**Description**
- **Start:** select 2 to 12 screenshots in the library, then **Make a board**. Or start an empty board and drag screenshots in.
- **Layouts:**
  - **Pair** (2 side by side or stacked, with optional BEFORE and AFTER labels, reusing the stamp words of ANN-4).
  - **Grid** (2 × 2 or 3 × 3, filled in order).
  - **Strip** (a numbered row or column of frames, for a sequence).
  - **Free** (place and size each image yourself).
- **Settings:** gap, outer margin, background (the same choices as the screenshot background setting), corner radius, shadow, a caption under each image, and one title.
- **Storage** (problem 10): the board is a new capture holding flattened copies of the sources. After **Finish** it is a normal screenshot capture that can be annotated, redacted, copied and shared. Until then, the layout stays editable.
- **AC1** Two screenshots of different sizes in Pair, side by side. Both are scaled to the same height, the gap and margin are as set, and the export matches the golden image within 1 %.
- **AC2** A source with a blur. The board shows the blur, and its copy holds no unblurred pixels.
- **AC3** Deleting a source after Finish leaves the board unchanged.
- **AC4** (agent) `create_board { capture_ids, layout, options }` → `{ capture_id, image }` (Free; it creates a new capture and changes nothing else).

### BLK-1 Bulk export
**Priority** P1 (0.3.4) · **Crates** cm-render, cm-daemon, cm-library, apps/desktop

**User story.** As someone preparing images for a site or a manual, I want to resize, convert, watermark or rename many screenshots at once, so that I don't do it image by image.

**Description**
- **Start:** select screenshots in the library, then **Export selected…**, which opens a sheet with these steps, each optional:
  - **Size:** a width, a height, a longest side or a percentage; never larger than the original unless the user switches that on.
  - **Format:** PNG, JPEG (quality 60 to 100), or WebP if the encoder is already in the tree; otherwise WebP waits.
  - **Watermark:** text or a PNG, a corner or the centre, opacity, and a size relative to the image.
  - **Blueprint:** on or off (ANN-11), with the default keep-list (none).
  - **File names:** a pattern with `{name}`, `{date}`, `{n}` and `{w}x{h}`, and a duplicate rule (number them, never overwrite).
- **Runs as one background job** with progress, Cancel, and a summary (`23 exported, 1 skipped: no space`). The originals are never changed (problem 11).
- **AC1** 50 screenshots exported as JPEG at a longest side of 1600 px with a text watermark gives 50 files of the right size, each with the watermark. The library entries are unchanged.
- **AC2** Cancel at 50 % keeps the files already written and leaves no partial file.
- **AC3** A file with the same name already in the folder. The new file gets a number; nothing is overwritten.
- **AC4** (agent) `export_images { capture_ids, dest, size?, format?, watermark?, blueprint?, name_pattern? }` → a job, like `export_video` (Free, writing only to the chosen folder).

## 4. Carried over from the old 0.3.4 plan (still in)

| Item | Spec | Why it stays |
| --- | --- | --- |
| Magnify, as **Zoom Lens** | ANN-8 (frozen) | Gap 6a |
| Canvas text selection, copy and paste, caret blink | ANN-1 AC9 to AC13 | Editor quality for Retype and the text tool |
| CMYK and YCCK JPEG converted to sRGB | IMP-6 AC10 to AC12 | Image import |
| PNG and JPEG registered for "Open With" | IMP-6 AC13 | Image import |
| B1: delete or rename blocked by a paused upload | LIB-1 | Bug |
| B2 and B3: the recording HUD and the webcam bubble cannot be dragged on Windows (screen and audio) | REC-5.AC18, VOC-1.AC12 | Bugs, reproduced Nov 14 (section 9); a fix, not a video feature |
| Hygiene | old plan, section 3 A | Windows regression of 0.3.3, PLUGIN-1 check, remove the "new" dots, Windows CI GPU crashes, the flaky socket test, a full test run on a quiet machine, `scripts/check-release-assets.sh`, the Vietnamese review |

## 5. Spikes (first, about 1 day in all, in parallel; S6 added in the merge)

| # | Question | Pass if | If it fails |
| --- | --- | --- | --- |
| S1 | Auto scroll and settle detection on each system (Chrome, a native list, a spreadsheet) | AC1 of CAP-6 on 3 apps per system | Ship Hand scroll only on that system |
| S2 | Background fill quality on 30 real UI screenshots (light, dark, gradients, Windows, macOS, GNOME) | 27 of 30 boxes within 6 levels on the ring check, with no visible seam at 100 % | Retype and Move area only on solid fills, and warn on the rest |
| S3 | Frame latency from mouse-down to frame, per system | 95 % of frames show the state before the click on the test app | Keep a frame from the last 100 ms (a rolling buffer of the target) and use it |
| S4 | PDF writer with no new native dependency (a pure-Rust crate with a compatible licence) | A 10-step guide in under 2 s, with embedded fonts for Vietnamese | PDF to 0.3.5; HTML print styles in 0.3.4 |
| S6 | Frame source on Wayland (GNOME and KDE): can Long Shot (hand scroll) and Click Guide take frames repeatedly without a prompt each time (a PipeWire screen-cast session kept open, as recording does)? | 30 frames in 60 s with one permission prompt | Wayland: Long Shot and Click Guide shown disabled with the reason (REC-5 banner rule) |
| S5 | The longest side the renderer can handle on the CI GPUs and the oldest supported Mac | 16,384 px renders on all of them | Lower the Long Shot limit to what passes |

## 6. Tracks (disjoint files; one `target/` per agent; the lessons of 0.3.3 apply)

| Track | Scope | Main files | Size |
| --- | --- | --- | --- |
| A | Zoom Lens core and render (from the old plan), base-edit ordering and validation, ops `retype`, `move_area`, `blueprint`, fills, goldens, MCP schema, CLI | `cm-core/src/annotation.rs`, `cm-render/**`, `cm-mcp/src/tools.rs`, CLI | 4 days |
| B | Editor: Zoom Lens tool, Touch-up group (Retype, Move area), Blueprint command, canvas text selection and caret | `apps/desktop/src/editor/**`, `ui/editor_view/**` | 4 days |
| C | Long Shot: auto and hand scroll per system, settle detection, stitching, overlay mode, permission row (Accessibility) | `cm-capture/**`, `cm-daemon` capture action, `ui/capture_overlay/**`, `features/14-permissions` code | 4–5 days |
| D | Click Guide: click listener reuse, step capture, `guide.json`, guide editor, exports, tools, plugin skill | `cm-library` (new `guide` kind), `cm-daemon`, `ui/guide_view/**` (new), `cm-mcp` | 5 days |
| E | Boards and Bulk export: compose, the board capture, export job, watermark, naming | `cm-render/src/compose*`, `cm-library`, `cm-daemon/src/export*`, `ui/library_view.rs` | 3 days |
| F | Carry-over: CMYK, file types, B1, B2 and B3 (window drag on Windows), hygiene, `cm-regress` cases, manual scripts (`CAP-6.md`, `GDE.md`, `ANN-9-11.md`, `BRD.md`) | as in the old plan | 2–3 days |

**Order:**
1. Phase 0: the spec text for section 3, and the frozen op JSON for `retype`, `move_area` and `blueprint`, plus the new tools. Half a day.
2. Spikes S1 to S5. One day.
3. Tracks A to F in parallel; merge each track when it is green.
4. One full gate on `main`.
5. Release candidates.
6. Regression on all three systems: book the lab laptop for Windows first.
7. Release.

**Size:** about two to three times 0.3.3. If time runs short, cut in this order:
1. PDF export;
2. horizontal Long Shot;
3. Move area;
4. Boards' Free layout.

**Must ship** (in this order of importance):
1. Long Shot vertical;
2. Click Guide with Markdown and HTML export;
3. Zoom Lens;
4. Retype;
5. Blueprint;
6. Boards Pair, Grid and Strip;
7. Bulk export;
8. the carry-over.

**Agent surface:** `capture_screenshot` gains `mode: "long"`. New tools:
- `suggest_blueprint`;
- `start_guide`;
- `stop_guide`;
- `get_guide`;
- `set_guide`;
- `export_guide`;
- `create_board`;
- `export_images`.

The tool count goes from 81 to 89. Update `02-agent-interface.md`, the tier table, the plugin skills and the README count.

## 7. Moved to a later release

| Item | Why it waits | Earliest |
| --- | --- | --- |
| **REC-9 Draw on screen while recording** (gap 6b) | Video; needs a capture-excluded layer and timed vector events | 0.3.5 or the video release after it |
| A live Zoom Lens on recordings | Video | With REC-9 |
| Click Guide as a video, with zoom on each click | Video; builds on GDE and auto zoom | Later |
| **Touch-up, full version:** select an object with one click, fill busy backgrounds | Needs an on-device segmentation and inpainting model (licence, size and speed check, the same way as the text models) | After a spike |
| **Blueprint, full version:** icons and images replaced | Needs on-device UI element detection | With the model above |
| Click Guide export to DOCX and PPTX | File writers and layout; HTML and Markdown cover the main use | 0.3.5 |
| Long Shot over 16,384 px (tiled rendering) | Renderer change | Later |
| Boards that refresh from their sources | Linked captures and change tracking | Later |
| Clipboard paste of a picture as a capture; WebP and HEIC import | Unchanged from the old plan | 0.3.5 |
| In-app update (Velopack), Windows code signing, a headless daemon, windowed audio | Platform work, unchanged from the old plan | 0.3.5 or later |

## 8. Questions for the maintainer (with the default I assumed)

1. **Names:** Long Shot, Click Guide, Touch-up (Retype, Move area), Blueprint, Boards, Bulk export, Zoom Lens. Keep, or rename? *(Default: keep.)*
2. **Long Shot limit:** 16,384 px on the long side in 0.3.4, with 30,000 px later? *(Default: yes, after S5.)*
3. **Accessibility on macOS:** add it as an optional permission row for Auto scroll? *(Default: yes; Hand scroll works without it.)*
4. **Keys:** Y for Retype, J for Move area, and no default hotkeys for Long Shot or Click Guide (the user sets them)? *(Default: yes. Y and J are not used by any editor tool in 0.3.3.)*
5. **Click Guide focus crop:** on by default at 1200 × 800? *(Default: on.)*
6. **PDF:** in 0.3.4 only if S4 passes, otherwise 0.3.5? *(Default: yes.)*
7. **Scope cuts:** is the cut order in section 6 right if the release runs long?
8. **Windows code signing:** has the SignPath Foundation application been sent? (Carried over; it decides whether 0.3.4 ships signed files.)
9. **Guide folder:** `cmguide` for the guide's folder extension, and a board stays an ordinary screenshot capture (problem 13)? *(Default: yes.)*
10. **WebP in Bulk export:** lossless only, or leave WebP out of 0.3.4 (problem 14)? *(Default: lossless if it adds no dependency, else out.)*
11. **Scope risk:** this is about 22 to 25 agent-days across six tracks, two to three times 0.3.3, which already ran long. Ship what is green at the gate, in the must-ship order of section 6? *(Default: yes; the cut order in section 6 applies, and any Must-ship item that is not green moves to 0.3.5 rather than delaying the rest.)*

## 9. Carried over from the plan of Nov 13: open items, lessons, bugs

### 9.1 What 0.3.3 left open (Windows regression done Nov 14: hardware tier 63/0, 19 skipped)


| Item | Where it comes from | State |
| --- | --- | --- |
| **Windows regression of 0.3.3** | the lab laptop could not boot Windows on release day | **done Nov 14, 2026**: hardware tier 63 passed, 0 failed, 19 skipped (`verification/L1-windows-0.3.3-regression-hardware-hardware-2026-11-14-claude.md`); release text updated |
| **Plugin check against the installed app** (`tests/manual/PLUGIN-1.md`) | release-day step "Claude plugin [M]" | not run |
| **Blue "new" dots** on the new editor tools | decided: they go after one release | to remove in 0.3.4 |
| **Windows CI: a GPU test process died** (twice, a different test each time: `cm-daemon export_real`, `cm-render compose`) | CI runs on `57d8a515` and `85df0235` | rerun was green; cause unknown |
| **`cm-daemon` socket test flake** (`ipc::tests::r2_7_stale_socket...` under load) | local full run | passes alone |
| **Tests not re-run on the merged code** | 49 of 52 `cm-daemon` test binaries, slow GPU and export tests of `cm-render` and `cm-media` | run them once on a quiet machine |
| **Vietnamese strings** | about 227 fuzzy drafts in `locales/vi.po` | maintainer's review |
| **CMYK JPEG import** | refused (`jpeg_cmyk`); the spec says "convert or refuse" | decide |
| **File types** | `Info.plist` and the Windows installer do not register PNG and JPEG for "Open With" | missing |
| **winget** | pull requests 472327 (0.2.8) and 477195 (0.3.1) open | waiting |
| **Text on the canvas** | no real text selection, caret does not blink, a drag-select stops at the field edge | known gaps of the text-fields work |
| **Magnify (ANN-8)** | moved out of 0.3.3 | design in `release-0.3.3-plan.md` section 2 items 1 and 2 |
| **Clipboard paste of a picture as a capture** | stretch goal of 0.3.3, not built | |
| **WebP and HEIC import** | refused in 0.3.3 | needs a licence and size check (HEIC: patents, decoder) |
| **In-app update** | Velopack spikes: macOS "go with changes"; needs hosting and the maintainer's decisions | parked |
| **Windows code signing** | SignPath Foundation recommended; the application is the maintainer's | files unsigned |
| **Headless daemon / `capturemann mcp` starting the app** | tools answer `daemon_unavailable` without the app | design point: Confirm-tier dialogs need a screen |
| **Windowed audio** | flat memory for long files and speakers | no release assigned |
| **PhoWhisper** | "not considered for now" | stays out |

### 9.2 Lessons of 0.3.3 to build into the process


1. **Phase 0 is spec text and the frozen format only**, not code (it took about 2 hours and delayed the parallel tracks).
2. **One `target/` per agent** (with `CARGO_PROFILE_DEV_DEBUG=line-tables-only`): the shared target made five agents queue on one lock. The disk allowed four.
3. **Scoped tests while developing, the full gate once at merge, a time limit on every command.** The full `cargo test --workspace` took hours on this Mac (GPU compose tests 80 to 180 s each, 52 `cm-daemon` test binaries).
4. **Merge each track as it finishes**; merge bugs (the same icon registered twice) show up at the first compile of the merged tree.
5. **Run clippy with the toolchain CI uses** (`cargo +1.100.0 clippy`; this Mac's stable was one release behind and CI found four lints). Keep that toolchain installed.
6. **Check the release assets before uploading**: the macOS files must be `CaptureMann-v<version>-macos-<arch>.dmg` (the cask and README expect it; 0.3.3 first went up with the wrong names and was corrected). Add a script that compares the asset names with the cask URL pattern and `releasing.md`.
7. **A Windows regression needs the lab laptop booted into Windows with a console login**; plan it before release day, not on it.
8. **The tap repo has no SSH key configured**: set `core.sshCommand` there like the main repo, so the cask push needs no special step.

### 9.3 Bug list (maintainer reports; more may come)


| # | Report | First analysis (not confirmed by a run) | Planned fix |
| --- | --- | --- | --- |
| B1 | Deleting a recording fails with "couldn't delete the capture `reg...` is being uploaded (job `job...`)" (Nov 13, 2026) | Same cause as the caption-save bug fixed in 0.3.3: an upload job **paused** with `auth_expired` (Google sign-in expired) counts as active. 0.3.3 made edits ignore paused jobs but kept `ensure_idle_strict` for rename and delete, so delete and rename stay refused until the account is reconnected or the job is cancelled by hand. Two such paused uploads existed in the maintainer's library. | Delete cancels the capture's paused upload and publish jobs itself (after the existing delete confirmation) and goes on; a **running** or queued job still blocks with the same message. Rename needs no block for a paused job (jobs name the capture id, not the folder): check that and relax it. Test in `library_ops.rs` and a daemon integration test. Also show in the Library that an upload is "waiting for sign-in" with a Reconnect button, so the state is not a dead end. |
| B2 | Windows user: the recording HUD cannot be dragged while recording **audio** (Nov 13, 2026) | Dragging is `window.start_window_move()` on the HUD bar's background (`ui/recording/hud.rs`, 0.2.9; tested for screen recordings on Windows). The audio HUD may place its own children (meter, marker field, label row) over the bar or stop the press, or the native move may behave differently for that window kind on Windows. **Reproduced Nov 14, 2026 on the ThinkPad (Windows 11, installed 0.3.3):** a synthetic mouse drag on the audio HUD bar (the press lands on the HUD, `WindowFromPoint` confirmed) did not move the window, and the same probe on a **screen** recording's HUD did not move it either. **Cause found in the code, not only the report:** `hud.rs` starts the move with `window.start_window_move()`, which is empty in GPUI's Windows backend (`gpui-pre-0.3.9`: implemented for Linux and macOS only); Windows moves a window only through a `WindowControlArea::Drag` region (answered as `HTCAPTION`), and nothing in `apps/desktop` declares one. So the drag added in 0.2.9 never worked on Windows, for screen and audio alike (the 0.2.9 text said it did; no Windows check of it is stored). The probe was a throw-away script (its drag coordinates need fixing; not stored), so a human drag on Windows is still the confirming check. | Declare the bar background, timer and meter as `WindowControlArea::Drag` on Windows (buttons stay out of it, so clicks still work; check that the area does not swallow the press on a button and that the collapse and snap-to-edge code still sees the move) and keep `start_window_move` for macOS and Linux; also the bubble (`bubble.rs`) and the window chrome (`csd.rs`) use the same call: check them on Windows; add a `cm-regress` desktop probe case if it can be automated (`desktop.hud_click_through` is the nearest). |
| B3 | **The recording HUD cannot be dragged on Windows at all** (found Nov 14, 2026 while reproducing B2; screen and audio) | Same cause as B2: the Windows backend has no window-move call and the app declares no drag region. REC-5 and the 0.2.9 text promise a draggable HUD on Windows 11; the promise was never true there. The webcam bubble and the window chrome use the same call. | `WindowControlArea::Drag` for the HUD bar, timer and meter and for the bubble body on Windows; new criterion REC-5.AC18; correct the 0.2.9 and 0.3.x release texts and `docs/project/status.md` ("drag works" is macOS and Linux only until 0.3.4); manual script `HUD-1.md` run on Windows with a stored report. |

Order of the carry-over inside track F: B1, B3 (with B2), CMYK, file types, hygiene.

## 10. Phase 0 (spec text only)

Written Nov 13, 2026 (old plan): ANN-8 Magnify, ANN-1 text selection, IMP-6 CMYK and file types, LIB-1 paused uploads, VOC-1.AC12. Written Nov 14, 2026 (this merge): CAP-6 in `features/01-screenshot.md`; ANN-9, ANN-10, ANN-11, the base-edit rule and the Zoom Lens name in `features/02-annotation.md`; the new files `features/19-click-guide.md` (GDE-1 to GDE-4) and `features/20-boards-and-bulk.md` (BRD-1, BLK-1); REC-5.AC18 in `features/03-recording.md`; the optional Accessibility row in `features/14-permissions.md`; FR rows in `00-product-spec.md`; the 8 new tools in `02-agent-interface.md`; `specs/README.md` and `platform-acceptance.md`. **Frozen:** the op JSON for `retype`, `move_area` and `blueprint`, the `guide.json` shape and the 8 tool names (the tool count goes from 81 to 89). After the maintainer accepts the defaults in section 8, the build can start.

## 11. After the specs: UI and UX design (Claude Design)

The maintainer will give Claude Design the surfaces below, after the specs are accepted. Each needs the kit's rules (`design/CLAUDE.md`, `docs/development/design-surfaces.md`), the text-field spec and the 0.3.3 palette:
1. Capture overlay: Long Shot mode, area highlight, the side preview while hand scrolling, the limit message, the Accessibility card.
2. Long Shot HUD (Pause, Stop, direction switch, frame count) and Click Guide HUD (step count, Pause, Add note step, Undo last step, Stop).
3. Editor: the **Touch-up** palette group (Retype Y, Move area J), the Retype popover and busy-background warning, the Move area handles, the **Zoom Lens** tool and inspector (from the Nov 13 text), the **Blueprint** command, preview and keep-list.
4. Guide editor (step list, step detail, retake, insert note) and the guide export sheet.
5. Boards: the Make a board sheet, layout picker, settings and the editable layout canvas.
6. Library: selection bar, Export selected… sheet (Bulk export), guide and board thumbnails and badges, "Upload waiting for sign-in" with Reconnect (B1).
7. Settings: hotkeys for Long Shot and Click Guide (none by default), the permissions page rows.
