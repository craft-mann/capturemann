# Click Guide (GDE)

**0.3.4, maintainer plan of Nov 14, 2026** (`specs/plans/release-0.3.4-plan.md`). One task clicked through once becomes a numbered guide with a screenshot of each click. UI name: **Click Guide**. IDs GDE-1 to GDE-4; guides are a new capture kind (`guide`, folder extension `cmguide`), see below.

**Rules that apply to the whole file**
- **Privacy.** It listens to global clicks, which needs Input Monitoring on macOS (CaptureMann already asks for it for the cursor track, REC-7) and the equivalent hook on Windows and X11. It **never records keystrokes**. It shows a HUD with the step count, Pause and Stop, excluded from the shots (REC-6). Auto-redact runs on each step and proposes blurs as in the editor; nothing is applied until the user accepts. A click inside the CaptureMann HUD is not a step.
- **When the frame is taken.** Many UIs change on mouse-up, so the frame is taken on **mouse-down** and the click point is kept: the step shows what the user clicked, not the result. Spike S3 measures the latency on each system; if frames arrive late, the capture keeps a rolling buffer of the last 100 ms of the target.
- **Storage.** A guide is a capture of the new kind `guide` in a folder `<name>.cmguide`: `manifest.json` (kind `guide`), `guide.json` and one screenshot bundle per step. A 0.3.3 app does not index a folder with an unknown extension, so it neither lists nor damages guides. The library index, search, delete, move and rename treat a guide like a recording-sized bundle; tests cover each.
- **Wayland.** Global clicks may not be available; when the portal gives no input events, Click Guide is shown disabled with the reason (spike S6).

### GDE-1 Click Guide: record
**Priority** P1 · **Status** Ready (0.3.4) · **Crates** cm-capture, cm-daemon, cm-library, apps/desktop · **Depends on** CAP-1, REC-6, REC-7 (click listening), ANN-5, ANN-6, PRM-1

**User story.** As a support engineer or trainer, I want to click through a task once and get a numbered guide with a screenshot of each click, so that I don't capture and number 15 screenshots by hand.

**Description**
- **Start:** **New Click Guide** from the capture overlay, the tray, the launcher and the library. The user picks a display, a window or a region to follow, then presses Start (a 3-second countdown, as in REC-4).
- **Each mouse-down** inside the target becomes a step:
  - CaptureMann takes a frame of the target (the frame is taken on mouse-down) and records the click point and the button.
  - It adds a step badge at the click point and a highlight ring around it, as normal ops, so the user can edit them.
  - It writes a draft step title from the text recognized nearest the click, for example `Click "Save"`. With no text near the click, the title is `Click here`.
  - Double-clicks and right-clicks are noted in the title (`Double-click`, `Right-click`).
- **Step crop:** each step can show the whole target or a **focus crop** around the click (default 1200 × 800 physical px, centred, clamped to the target). Both are stored: the frame is the image, and the crop is a non-destructive `crop` op (ANN-1).
- **HUD:** step count, Pause, Add note step (a step with no image, for "Type your password"), Undo last step, and Stop. The HUD is excluded from the shots.
- **Privacy** (privacy rules below): no keystrokes; Auto-redact runs on each step in the background and marks the steps that have proposals.
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

