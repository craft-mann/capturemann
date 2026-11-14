# QA-0.3.4 Click Guide (GDE-1 to GDE-4) report, macOS (PARTIAL)

- **Date:** 2026-11-15 (overnight run)
- **Who:** Claude (QA agent, maintainer asleep)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** signed dev bundle `~/Applications/CaptureMann-dev.app` of main `26a05100` (bundle id `dev.capturemann.app.dev`), isolated library/config/socket under `/tmp/cmq`, `CAPTUREMANN_SECRETS=file`. Nothing rebuilt.
- **Fixture:** my own AppKit window "QA Fixture Window" with buttons Open, Save, Share, Export, Finish and the text `jane.doe@example.com`; real mouse events (CGEvent) on this screen. The Confirm dialog of `start_guide` was answered blind with Tab and Return (it cannot be photographed).
- **Caveat:** another agent was driving the same screen (Chrome, Terminal); focus was stolen twice, so every click script re-raises the target first.

## Results

| Item | Result | Evidence |
| --- | --- | --- |
| GDE-1.AC1 five real clicks, five steps in order | PASS | `guide start --window`, clicks Open, Save, Share (double), Export (right), Finish: 5 steps; titles `Click "Open"`, `Click "Save"`, `Double-click "Share"`, `Right-click "Export"`, `Click "Finish"` |
| ring centre within 2 px, hollow, soft glow, badge upper-left | PASS | `step_1` manifest: ellipse rect [182,202,116,116] = centre (240,260) = the click (240,260) exactly; `glow` 13; step badge at (199,219), upper-left of the ring. Exported picture of step 3 looked at: hollow ring, soft pink glow, label "Share" readable, red badge "3" upper-left |
| frame taken on mouse-down | PASS | step 3 picture shows the Share button pressed and the status line still reading the earlier button ("Last: Save") |
| click coordinates stored in physical px with scale 2 | PASS | `click {x:240,y:260,scale:2.0}` for a click at window offset (120,130) pt |
| GDE-1.AC2 click outside the target adds no step | PASS | click on another window: step_count stayed 5 |
| GDE-1.AC3 pause, 3 clicks, resume | PASS | 1 click, `guide set --pause`, 3 clicks: still 1 step; `--resume`, 1 click: 2 steps; `--remove-last`: 1 step |
| GDE-1.AC4 email step marked, nothing blurred | PASS | all 5 steps `has_proposals: true`; exported step 3 shows the email unblurred; the editor lists "Redaction proposals to review" |
| GDE-1.AC5 macOS without Input Monitoring | not run | the dev identity holds the grant; I did not touch TCC |
| GDE-1.AC6 Wayland | not applicable on macOS | |
| HUD bar, countdown ring | not run | floating windows are hidden from screen capture; effects only (start, pause, resume, stop worked) |
| titles from nearest text | PASS | names above; 80 pt rule not measured separately |
| GDE-2.AC1 move step 4 to position 2 | PASS | `guide set --steps`: order 1,4,2,3,5, badge ops renumbered to 1,3,4,2,5 in each step file; `undo` restored order and badge numbers in one step; redo and undo again worked |
| GDE-2.AC2 delete leaves no orphan | PASS | after deleting step_3, `steps/` held 4 folders and `.undo/step_3.cmshot`; undo brought the folder back and `.undo` emptied |
| title, intro, numbering `upper` | PASS | labels A to F, step ops carry `style: upper`; export headings `A. Click "Open"` |
| note step (no image) | PASS | step F "Type your password" with note "Do not share it"; in Markdown and rich text |
| focus crop on/off | PASS (flag only) | `focus_crop: true` stored; no crop op is written and the fixture window (1400x984 px) is smaller than the 2400x1600 default, so no visible effect: crop result not seen |
| retake | PASS with remark | `--retake step_1` made `step_1-retake.cmshot` with a fresh ring and badge; undo restored the old folder. Remark R1: the retaken step has `has_proposals: false` although the new frame still shows the email |
| editor page (GUI) | PASS with defects | list with badges A..F, thumbnails, "Redaction proposals to review", up/down, Note step, `1 2 3 / A B C`, undo/redo, Copy as rich text, Export; shots `QA-0.3.4-shots/guide-editor-preview-clipped.png` |
| GDE-3 Markdown | PASS | `guide.md` + `images/step-01..05.png`, relative links, no absolute path, 800 px wide at 1x, intro and note step included |
| GDE-3 HTML | PASS (structure) | one file 349 KB, 5 `<img>` embedded as data URIs, `<nav class="toc">`, `prefers-color-scheme: dark` and `@media print`; not opened in a browser |
| GDE-3 PDF | PASS | 3 pages (A4), two steps on pages 1 and 2, one on page 3, heading stays with its picture, rings and badges visible; every page rendered with PDFKit and looked at (`guide-pdf-p1.png`) |
| rich text clipboard (NSPasteboard) | PASS | types `public.html` (with embedded data-URI images), `Apple HTML pasteboard type`, `public.utf8-plain-text`, `NSStringPboardType`; no RTF type. Remark R2: data-URI images may not paste into every target |
| GDE-3.AC2 blur visible, OCR finds none | PASS | blur op on the email of step 2, Markdown export: the image is blurred; `capturemann text` on it lists no email text (blur strength 16 is still faintly readable by eye) |
| export sheet (GUI) | PASS with defects | formats, folder, headings, 1x/2x, switches, warning with Review button; defects G2 and G3 below |
| GDE-4 CLI/MCP tools | PASS | `start_guide`, `stop_guide`, `get_guide`, `set_guide`, `export_guide` all work through the CLI; `tools/list` of the bundle's `cm-mcp` lists them (89 tools in all) |
| library kind `guide`, Click Guides page, no chips | PASS | `library list` kind `guide`; page shows 2 cards with GUIDE badge, "New Click Guide" button, sort bar and no filter chips (`library-click-guides.png`) |

## Problems found

- **G1 (medium, not fixed).** In the guide editor the selected step's picture is drawn at the bottom of its box, with about 250 px blank above it and its lower part clipped (the box is ~425 px tall, the picture starts ~250 px down). Repro: open a guide with 1400x984 step images in a 1180x792 window. Code: `apps/desktop/src/ui/guide_view/mod.rs` around line 1032, `img(..).w_full().object_fit(Contain)` inside a div in a scrolling flex column with no height rule; likely the same GPUI auto-height image problem as D5 of `QA-0.3.4-editor-library-2026-11-14-claude.md`. I could not rebuild `cm-desktop` (disk 32 GB free, no target in the worktree), so no fix. Picture: `QA-0.3.4-shots/guide-editor-preview-clipped.png`.
- **G2 (low, fixed in commit f7cba774, not rebuilt or seen).** The export sheet's warning "Step 1 still has redaction proposals..." did not wrap and pushed the "Review step 1" button off the sheet edge (clipped). Added `min_w(0)` to the text. Only a one-line change; **unverified** until a rebuild.
- **G3 (low).** The export sheet's warning names only the first step with proposals ("Step 1") though all five have them; the spec says it names any such step.
- **G4 (low).** The switches Intro / Step notes / Note steps are drawn as plain white squares when on and empty outlines when off (no tick), easy to misread.
- **G5 (low).** In the editor three text fields (guide title, intro, step title) all showed a text selection highlight at once.
- **R1, R2** above. Also: ring and badge revisions are credited to `agent:capturemann-cli` although the app drew them (the revision author of a guide step made by the session).

## Not run

GDE-1.AC5 (Input Monitoring card), the HUD itself, the countdown, Add note step from the HUD, drag-to-reorder by mouse, Edit image round trip and Retake by mouse in the GUI, `rich text` paste into a real target, HTML in a browser, the `write-a-how-to` plugin skill, a guide of a display or region target, 2x image size, Letter paper.

## Verdict

PARTIAL. GDE-1 AC1 to AC4, GDE-2 AC1/AC2 and GDE-3 AC1/AC2 pass with real clicks on this Mac; GDE-1.AC5 and the HUD are not run; one real GUI layout defect (G1) found and left open.
