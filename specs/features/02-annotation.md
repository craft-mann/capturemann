# Annotation (ANN)

**Design source for 0.3.3 (ANN-4, ANN-6, ANN-7, ANN-8):** the maintainer's design pack `specs/capturemann-0.3.3-annotation-pack/` (local only, git-ignored): `release-0.3.3-design-spec.md`, boards N0 to N8b, icons. Plan: `../plans/release-0.3.3-plan.md`.

Annotations are vector operations stored in `manifest.json` (`ops` list) and rendered on top of `original.png` by `cm-render`; the original is never modified. The same op schema is used by the editor UI, `annotate` (MCP) and `--annotate ops.json` (CLI).

**Op schema (shared by ANN-1, ANN-2)** — coordinates are physical px of the original image; colors are `#RRGGBB`.

```json
{ "id": "op_7", "op": "arrow", "from": [120, 300], "to": [380, 210], "color": "#FF3B3B", "width": 4 }
{ "id": "op_8", "op": "rect", "rect": [400, 180, 240, 60], "color": "#FF3B3B", "width": 4, "fill": null, "radius": 6 }
{ "id": "op_9", "op": "ellipse", "rect": [400, 180, 240, 60], "color": "#2E8BFF", "width": 4 }
{ "id": "op_10", "op": "line", "from": [10, 10], "to": [200, 10], "color": "#111111", "width": 2 }
{ "id": "op_11", "op": "text", "at": [130, 320], "text": "Click Save", "size": 24, "color": "#FFFFFF", "outline": "#000000", "bg": null }
{ "id": "op_12", "op": "highlight", "rect": [90, 400, 300, 28], "color": "#FFD60A", "opacity": 0.35 }
{ "id": "op_13", "op": "step", "at": [400, 200], "n": 1, "size": 28, "color": "#FF3B3B" }
{ "id": "op_14", "op": "blur", "rect": [420, 88, 260, 24], "strength": 12 }
{ "id": "op_15", "op": "pixelate", "rect": [420, 120, 260, 24], "block": 12 }
{ "id": "op_16", "op": "crop", "rect": [0, 0, 1200, 800] }
```

**0.3.3 additions (frozen Nov 12, 2026; ANN-4, ANN-6, ANN-7).** Same conventions. Omitted optional fields take the defaults shown.

```json
{ "id": "op_20", "op": "callout", "rect": [100, 80, 220, 70], "tail": [90, 200], "text": "Click here", "size": 24, "color": "#FFFFFF", "bg": "#FF3B3B", "radius": 12, "style": "bubble" }
{ "id": "op_21", "op": "spotlight", "shapes": [ { "kind": "rect", "rect": [10, 10, 200, 100], "radius": 0 }, { "kind": "ellipse", "rect": [300, 40, 120, 120], "radius": 0 } ], "dim": 0.6, "mode": "dim", "strength": 12 }
{ "id": "op_22", "op": "step", "at": [400, 200], "n": 2, "size": 28, "color": "#FF3B3B", "style": "upper", "start": 3, "text": "Open Settings" }
{ "id": "op_23", "op": "stamp", "at": [50, 50], "kind": "check", "size": 34, "color": "#2EB872" }
{ "id": "op_24", "op": "stamp", "at": [90, 50], "kind": "label", "size": 34, "color": "#FF3B3B", "text": "Before" }
```

- `callout`: `rect` is the **minimum** box, `tail` the tip of the pointer in image px, `text` at most 2000 characters (default empty), `size` 24, `color` is the text colour (`#FFFFFF`), `bg` the fill (`#FF3B3B`), `radius` 12 (0 to 2000), `style` `bubble` (rounded box, curved tail; default) or `box` (rectangle, straight triangular tail).
- `spotlight`: `shapes` 1 to 8 holes (`kind` `rect` default or `ellipse`, `rect` with area, `radius` for rounded rectangle holes, default 0), `dim` 0 to 1 (default 0.6: the outside keeps 40 % of its brightness), `mode` `dim` (default) or `blur`, `strength` 4 to 32 (default 12, used by `blur` mode only). **At most one `spotlight` per image**: a second one is `invalid_input` naming it (`ops[i]`).
- `step` gains `style` (`number` default, `upper`, `lower`), `start` (at least 1, default 1) and `text` (label, at most 200 characters, default none). The label drawn in the badge is `step_label(style, start + n - 1)`; letters continue Z, AA, AB. A step without these fields is exactly the 0.3.2 step; the three fields are not written while they hold their defaults.
- `stamp`: `kind` `check`, `cross`, `exclaim`, `question` (vector paths) or `label` (a word on a pill; `text` required, at most 200 characters, written in the UI language when the stamp is placed), `at` is the centre, `size` 34 (cap 2000), `color` `#FF3B3B`.
- **Pass-through (0.3.3).** A field this build does not know inside a known op is kept and written back unchanged (a typed field the build writes wins over a stored duplicate). An unknown `op` type is still kept whole in `manifest.unknown_ops` by `cm-library`.
- Magnify is not in 0.3.3: the op name `magnify` and ANN-8 are reserved for it.

### ANN-1 Drawing tools: arrow, line, rectangle, ellipse, text, highlighter, step badge, crop
**Priority** P0 · **Status** In progress (built on macOS: the op schema and validation in `cm-core`, the renderer with golden tests in `cm-render`, the editor in `apps/desktop` and the `annotate` action and tool; ACs 1 to 6 are cited by automated tests; no hands-on editor verification report is stored; the editor has not run on Windows) · **Crates** cm-core, cm-render, apps/desktop · **Depends on** CAP-1, DESIGN §5.5

**User story** — As a developer, I want to draw arrows, boxes, text and numbered steps on a screenshot, so that others understand it without extra explanation.

**Description**
- Tools and shortcuts: Select (V), Arrow (A), Line (L), Rectangle (R), Ellipse (E), Text (T), Highlighter (H), Step badge (N), Crop (C), Blur (B, ANN-2), Pixelate (X, ANN-2).
- Shift constrains: arrows/lines to 0/45/90°, rectangles to squares, ellipses to circles.
- Every op is selectable, movable, resizable via handles, recolorable, and deletable (Delete/Backspace). Copy/paste duplicates ops with +10 px offset.
- Step badges auto-number in creation order (1, 2, 3…); deleting one renumbers the rest; the next number is shown on the cursor. Label, numbering style, start number and reordering: ANN-6.
- Text: click to place, type, Enter for a new line, Esc to finish; default 24 px white with no outline or shadow (changed in 0.2.9, AC8; the user changes colour and size in the panel); typed text goes through the platform input method so Telex, VNI and Pinyin compose (AC7).
- Defaults: color `#FF3B3B`, width 4 px; the last used color and width persist per tool.
- Crop is non-destructive: it sets the visible rect; ops outside it remain stored.
- Rendering is anti-aliased and resolution-independent; widths are physical px of the original.
- Limits: text is at most 2000 characters (`invalid_input` above that); a text layer is capped at 32 Mpx (and 16,384 px per side) so a huge font size cannot exhaust memory; sizes are capped at 2000.

**Acceptance criteria**
- **AC1** Given a screenshot, when the user draws an arrow from (120, 300) to (380, 210), then the manifest holds one `arrow` op with those coordinates and the render matches the golden image within 1% pixel difference.
- **AC2** Given three step badges, when the user deletes badge 2, then the remaining badges read 1 and 2.
- **AC3** Given Shift is held while drawing a rectangle, then width equals height.
- **AC4** Given a crop to (0, 0, 1200, 800), when exported, then the PNG is 1200×800 and contains the ops inside that rect.
- **AC5** Given any op is selected, when the user changes its color, then only that op changes and the change is one undo step.
- **AC6** Given an agent sends the op list from the schema above to `annotate`, then the result renders identically to the same ops drawn in the GUI.
- **AC7** Given the Text tool is open and a Vietnamese input source (Telex or VNI) is active, when the user types `Tieengs Vieetj cos daaus` (Telex), then the text reads `Tiếng Việt có dấu` in the editor, with no raw Latin letter left in front of a composed one, and the exported PNG shows the same glyphs (no missing-glyph boxes). Tested with fakes: the key handler leaves typed text to the input method and the draft/commit path builds the phrase (`editor::state` tests); glyph coverage in `cm-render` (`tests/fallback_fonts.rs`); by hand on Windows and macOS: `tests/manual/ANN-1-IME.md`. Linux (IBus, Fcitx) is not tested in 0.2.9.
- **AC8** Given a new text annotation is placed, then it is 24 px, white, with no outline and no shadow; an existing text annotation keeps its stored colour, size and outline, and the user can still change colour and size in the panel.

**Edge cases and errors** — Text with emoji or CJK renders with font fallback (`cosmic-text`); ops with coordinates outside the image are clipped, not rejected; invalid op type → `invalid_input` naming the op index (`ops[3]: ...`) when it comes from a client. **Forward compatibility:** an op type this build does not know that is already stored in a manifest (written by a newer version) is preserved and written back unchanged, not treated as corruption; it is not rendered.

**Agent interface** — MCP `annotate { capture_id, ops: Op[], replace?: false, copy_to_clipboard?, return_image?: true }` → `{ revision, image }` (Free); `return_image: false` (CLI `--no-image`) leaves the image out of the result, which the editor uses to stay cheap. `get_capture` returns current ops. CLI `capturemann annotate <id> --ops ops.json`.

**Out of scope** — Freehand pen (P2), shapes library, layers panel.

**Test notes** — Golden PNGs per op type at 1× and 2× in `tests/golden/annotation/`; property test: serialize → deserialize → render is stable.

---

### ANN-2 Blur and pixelate
**Priority** P0 · **Status** In progress (what Auto-redact finds, its benchmark and its generated documentation are now specified in RED-1 to RED-3, `features/17-redaction.md`; the text below is the state when AC4 was built, with emails, tokens, phone numbers and IPv4 addresses; blur and pixelate built and tested in `cm-render` (AC1 by a structural stand-in for OCR, AC2, AC3); Auto-redact and `suggest_redactions` (AC4) are built on macOS (Oct 23, 2026): the `cm_render::ocr` recognizer trait with Apple Vision, the pure detector, the action through daemon, MCP and CLI, and the editor's Auto-redact button with dashed boxes and Accept all / one / Dismiss, all tested with a scripted recognizer; the Vision recognizer ran once on a synthetic PNG (ignored test), the editor UI is untested by eye, `tests/manual/ANN-2.md` has not been run, AC1 with real OCR has not been run, Windows has only an unavailable stub (Windows.Media.Ocr is a later milestone)) · **Crates** cm-render · **Depends on** ANN-1

**User story** — As a support engineer, I want to blur or pixelate emails, tokens and faces, so that I can share screenshots safely.

**Description**
- Blur (Gaussian, strength 4–32, default 12) and Pixelate (block 6–32 px, default 12) apply to a rectangle of the canvas **at the op's position in the op list**: ops earlier in the list are included in what is blurred, ops later are drawn on top. A blur placed before an arrow therefore blurs the pixels without the arrow. A zero-area blur or pixelate rect is `invalid_input` (it would redact nothing).
- **Export safety**: rendered output never contains the unblurred pixels in any form (no layers, no metadata). The clipboard copy and all exports are flattened.
- The redaction is irreversible in exports but editable in the bundle (the original stays local and private).
- "Auto-redact" button (P1, uses AI-free detection): finds email addresses and long tokens via on-device OCR (Apple Vision on macOS, Windows.Media.Ocr on Windows) and proposes blur rects for the user to accept.

**Acceptance criteria**
- **AC1** Given a pixelate op over text, when exported, then OCR on the exported PNG returns no characters from that rect.
- **AC2** Given a blur at strength 12, when exported at 2×, then the visual result matches the golden image (≤ 1% difference).
- **AC3** Given an exported PNG, then it has no extra chunks or metadata containing original pixels (checked by parsing chunks).
- **AC4** Given Auto-redact on a screenshot with two email addresses, then two blur rects are proposed and nothing is applied until accepted.

**Edge cases and errors** — Very small rects (< 6 px) still blur with a minimum kernel; OCR unavailable → Auto-redact button hidden.

**Agent interface** — `annotate` with `blur` / `pixelate` ops; MCP `suggest_redactions { capture_id, kinds? }` → `{ rects: [{ rect, kind, confidence, text_preview }] }` (Free, read-only, never applied; the preview is masked to the first and last two characters; `not_implemented` when there is no recognizer). CLI `capturemann annotate <id> --suggest-redactions` (all kinds) or `--redact-emails` (emails only): both only print the proposals; accepting is `annotate --ops` with `blur` ops. As built the detector finds emails, API keys (`sk-`, `ghp_`, `AKIA`, `xox`, JWTs) and other long mixed letter-and-digit strings of 24+ characters, plus phone numbers and IPv4 addresses as low-confidence extras. In the editor, accepted proposals become blur ops (strength at least 16) in one undo step.

**Out of scope** — Face detection (P2), video redaction (a separate future feature).

**Test notes** — OCR-based test for AC1; chunk parser test for AC3.

---

### ANN-3 Undo, redo and non-destructive editing
**Priority** P0 · **Status** In progress (built on macOS in `cm-core` and `cm-library`: the command stack, persisted history and revisions, and the id-based replace diff; AC1, AC3 and AC4 are cited by automated tests, AC2 by the library and agent tests; Windows not run) · **Crates** cm-core, cm-library · **Depends on** ANN-1, AGT-3

**User story** — As any user, I want unlimited undo and to keep my original, so that I can experiment without fear.

**Description**
- Every action (add/change/delete op, crop, agent `annotate` call) is one undo step; an agent call with 10 ops is one step.
- Undo history persists in `manifest.history` (the reversible changes, each holding its forward and inverse data), so undo works after reopening the capture. `manifest.revisions` is only the log (number, action, `by`, time) and is not what undo reads. Both keep the last 200 entries (the undo depth is 200 steps). History this build cannot read (new change kinds) is kept as is and written back until the next edit; the ops are authoritative. `annotate { replace: true }` over existing ops is stored as a **diff by op id** (R1-3, built Oct 21, 2026): untouched ops cost nothing, a changed op is a `ReplaceOp`, a vanished one a `RemoveOp`, a new one an `AddOp`, a new order one `ReorderOps`; the whole call is still one undo step, and an identical list records nothing. A probe with 30 shapes and 200 edits keeps `manifest.history` under 200 KB, and the history is written as one compact line. Histories written earlier with `ClearOps` still undo.
- "Revert to original" clears all ops (one undoable step).
- `original.png` is read-only after capture; `render.png` is regenerated on save.

**Acceptance criteria**
- **AC1** Given 5 edits, when the user presses undo 5 times, then the render equals the original pixel for pixel.
- **AC2** Given an agent call that added 10 ops, when the user presses undo once, then all 10 ops are removed.
- **AC3** Given the app is restarted, when the user reopens the capture and presses undo, then the last edit is undone.
- **AC4** Given any edit, then `original.png` is byte-identical to its state at capture (hash check).

**Edge cases and errors** — Corrupted manifest → open the original with no ops and keep the broken manifest as `manifest.broken.json`; show a warning.

**Agent interface** — MCP `undo` / `redo { capture_id }` → `{ revision }` (Free). Revisions list in `get_capture`.

**Out of scope** — Branching history.

**Test notes** — Unit tests on the command stack; restart test in integration suite.

---

### ANN-4 Callouts and stamps
**Priority** P1 for `callout` and `stamp` (the pasted `image` overlay stays P2 and is not planned) · **Status** Ready (0.3.3; the op JSON is frozen above; `stamp` is the stretch goal: built last, cut first if time runs short) · **Crates** cm-core, cm-render, cm-mcp, cm-cli, apps/desktop · **Depends on** ANN-1, ANN-3

**User story** — As a creator, I want speech-bubble callouts and stamps (check, cross, exclamation, question, "Before", "After"), so that my screenshots look polished and explain themselves.

**Description**
- **Callout** (key K, in the Text group of the tool palette): a text box with a pointer. Two looks: `bubble` (rounded box, curved tail) and `box` (rectangle, straight tail). Drag to place the box, click again or drag the blue handle to set the tail tip. The text uses the Text tool's input method (Telex, VNI, Pinyin compose, as ANN-1.AC7). Colours: text, fill; size and corner radius in the panel.
- **The box grows to fit the text, and what is stored is what is drawn.** `rect` is the minimum size; the renderer draws `max(rect, measured text size + padding)` (the same in the editor and from `annotate`, so ANN-1.AC6 holds). When the user commits text the editor writes the grown rectangle back. The editor draws its selection handles from the renderer's measured box (`cm_render::callout_box`), not from the stored `rect`, because an agent-made callout can be drawn larger than stored.
- The tail tip is stored in image coordinates: moving the box does not move the tip. The tail base is on the box edge nearest to the tip.
- **Stamp** (key U, in the Markers group): `check`, `cross`, `exclaim`, `question` drawn as vector paths (emoji fonts differ per system and would break golden tests), and `label` stamps "Before" and "After" whose word is written into the op in the UI language when placed (stored text, not re-translated). Click to place; size and colour in the panel.
- Limits as ANN-1: text 2000 characters, sizes capped at 2000, layer area capped as for text.

**Acceptance criteria**
- **AC1** Given a callout with `rect` smaller than its text, when rendered, then the drawn box is `max(rect, text + padding)`, the render matches the golden image within 1 %, and `callout_box` returns that box.
- **AC2** Given a callout, when its `rect` is moved by (dx, dy) and `tail` is unchanged, then the tail tip is drawn at the same pixel (the tip pixel is inside the tail colour in both renders).
- **AC3** Given the `box` and the `bubble` style, then each matches its golden image at 1× and 2×.
- **AC4** Given each stamp kind, then it matches its golden image; a `label` stamp without `text` is `invalid_input` naming `ops[i]`.
- **AC5** (editor) Given the Callout tool, when the user types text longer than the box, then the stored `rect` is the grown rectangle after the commit and the handles sit on the drawn box; typing `Tieengs Vieetj` in a Telex source gives `Tiếng Việt`.
- **AC6** Given an agent sends the same callout and stamp ops to `annotate`, then the result renders identically to the same ops drawn in the GUI (ANN-1.AC6).
- **AC7** Given a callout or stamp op in a manifest, when it is opened by a version that does not know it, then it is kept unchanged (`unknown_ops`) and not drawn.

**Edge cases and errors** — Empty callout text draws an empty box; a tail tip inside the box draws no tail; a tip or box outside the image is clipped, not rejected.

**Agent interface** — `annotate` with `callout` and `stamp` ops; no new tool.

**Out of scope** — Pasted image overlays, custom stamp artwork, rotated callouts.

**Test notes** — Golden PNGs per style and kind; `callout_box` tests; the editor by hand in `tests/manual/ANN-6.md`.

---

### ANN-6 Step badge: label, numbering style, start, reorder
**Priority** P1 · **Status** Ready (0.3.3) · **Crates** cm-core, cm-render, apps/desktop · **Depends on** ANN-1

**User story** — As someone writing a how-to, I want step badges with a short label, letters instead of numbers, a start number and a drag-to-reorder list, so that a guide reads "A. Open Settings, B. Choose Network".

**Description**
- A step keeps `n`, its creation ordinal. The drawn label is `step_label(style, start + n - 1)`: numbers, upper-case letters or lower-case letters; after Z come AA, AB (spreadsheet columns). `style` and `start` are stored on each step; the **Steps panel sets them for all steps together**, a mixed set stays possible for agents.
- `text` is drawn in a dark pill to the right of the badge; near the right image edge it flips to the left.
- **Reorder** (drag in the Steps panel) rewrites `n` on the affected steps in **one revision**. Deleting a step renumbers the rest, as in ANN-1.AC2, for every style.
- Text typed in the panel goes through the platform input method (ANN-1.AC7).

**Acceptance criteria**
- **AC1** Given style `upper` and `start` 3, then the first three steps read C, D, E; after the second is deleted they read C, D.
- **AC2** Given 27 steps in `upper`, then the 26th reads Z and the 27th AA.
- **AC3** Given a step with `text` and its badge within the label width of the right edge, then the label is drawn left of the badge (golden image).
- **AC4** Given a reorder of steps 1, 2, 3 to 3, 1, 2, then the `n` values change in one undo step and one undo restores them.
- **AC5** Given a step written by 0.3.2 (no new fields), then it renders exactly as before (the 0.3.2 golden images are unchanged).
- **AC6** Given a 0.3.3 manifest with `style`, `start` and `text` read and saved by a build that does not know them, then every other field and op is intact (the fixture round-trip test in `cm-library`); the build that does know them keeps them (pass-through for later unknown fields).

**Agent interface** — `annotate` step ops with `style`, `start`, `text`.

**Test notes** — `step_label` unit tests; golden images per style; `tests/manual/ANN-6.md` for the editor.

---

### ANN-7 Spotlight
**Priority** P1 · **Status** Ready (0.3.3) · **Crates** cm-core, cm-render, apps/desktop · **Depends on** ANN-1, ANN-2

**User story** — As someone documenting a screen, I want to dim everything except the part that matters, so that the reader looks at the right place.

**Description**
- **Spotlight** (key S, in the Emphasis group): one dim layer over the image with holes (rectangles, rounded rectangles, ellipses). `dim` 0 to 1 (default 0.6, so the outside keeps 40 % brightness).
- **One per image.** The editor adds a second shape as a hole in the existing op, in one revision (one undo step), up to 8 holes. `annotate` with two `spotlight` ops answers `invalid_input` naming the second (`ops[i]`).
- **Order.** The model is strictly list-ordered like every op. The **editor inserts a new spotlight before the first non-redaction op**, so arrows, steps and callouts added later stay bright; agents control the order by writing the list. Blur and pixelate stay below it as in ANN-2 (a spotlight never un-redacts anything: it only dims or blurs pixels already composited).
- **Blur mode is a look, not a redaction.** `mode: "blur"` blurs the outside area for emphasis; the panel says so and points to Blur (B) and Pixelate (X). Copy text reads the rendered image (ANN-5.AC2), so text in a blurred area is not copied, and text in a dimmed area still is.

**Acceptance criteria**
- **AC1** Given a spotlight with dim 0.6 and one hole, then a pixel outside the hole has 40 % of the brightness it had and a pixel inside is unchanged; the render matches the golden image within 1 %.
- **AC2** Given two `spotlight` ops, then `annotate` answers `invalid_input` naming `ops[1]`, and nothing is stored.
- **AC3** (editor) Given a spotlight and a drawn second shape, then the op has two shapes after one undo step; undo leaves one.
- **AC4** (editor) Given an arrow and a spotlight added afterwards, then the spotlight sits before the arrow in the list and the arrow stays bright; a blur stays before the spotlight.
- **AC5** Given mode `blur`, then the outside is blurred with `strength` and the hole is sharp; the spec text, the panel and the docs say it is not a redaction.
- **AC6** Given 9 shapes, no shape, a shape with no area or a dim outside 0..1, then `invalid_input` naming `ops[i]`.

**Agent interface** — `annotate` with a `spotlight` op; no new tool.

**Test notes** — Golden images for dim, blur, ellipse and rounded holes; `tests/manual/ANN-6.md` for the editor.

---

### ANN-8 Magnify (reserved)
**Priority** P2 · **Status** Backlog (moved out of 0.3.3 on Nov 12, 2026). The ID and the op name `magnify` are reserved. Design notes to keep: the loupe renders its source rectangle again at `zoom` times from `original.png` plus the vector ops that precede it, so it is sharp; blur and pixelate always apply to every copy of those pixels, also inside a loupe (OCR on the export finds no characters in a redacted rectangle, also inside a loupe). Detail in `../plans/release-0.3.3-plan.md` section 2 items 1 and 2.

---

### ANN-5 Copy text from a screenshot
**Priority** P1 · **Status** In progress (the headless part is built and tested with a scripted recognizer: `ocr::layout_text` and the region crop in `cm-render`, the action `recognize_text` through daemon, MCP and CLI; the editor part is built too: a Copy text button, the Select text tool (key G), a library menu entry and a capture-toast button, with the pure parts (region mapping, outcomes, enabled state) tested and the window not seen by eye; `tests/manual/ANN-5.md` has not been run; the real recognizer ran only on synthetic PNGs, recorded in `verification/benchmarks/ocr-copy-text-synthetic-macos-2026-11-11.md`) · **Crates** cm-render, cm-daemon, cm-mcp, cm-cli, apps/desktop · **Depends on** ANN-1, ANN-3, ANN-2 (the recognizer)

ID note: the plan for 0.3.2 called this ANN-4, but ANN-4 is the draft "Callouts, stamps and image overlays" (and FR row ANN-4), so this feature is ANN-5.

**User story** — As anyone who screenshots an error message, a table or a chat, I want to select the text in the image and copy it, so that I do not retype it.

**Description**
- Action `recognize_text { capture_id, region?: [x, y, w, h], copy_to_clipboard?, format?: "text" | "lines" }` → `{ capture_id, recognizer, region, text, word_count, truncated, copied_to_clipboard, warnings, lines? }`. `region` is in pixels of the original image (like annotation ops); with none, the whole image. `format: "lines"` adds `lines: [{ text, rect, confidence }]`.
- **It reads the rendered image** (annotations and redactions applied; rendered on the fly when `render.png` is missing or stale), never `original.png`. This is the opposite of Auto-redact on purpose: text under a blur or pixelate must not come back (RED-2.AC1 applies to copied text too). With no ops the render equals the original. With a crop op, the numbers stay original pixels (the handler shifts them).
- **Layout** (`cm_render::ocr::layout_text`, general, no language or phrase lists): lines in reading order from their boxes; pieces on one row joined left to right; a blank line where the vertical gap is more than 0.9 of the median line height; columns (no line covers a stretch of at least 2 median heights between them) read one after the other with a blank line between; a line that spans the gutter (a wide heading) makes the page one column; trailing spaces trimmed; hyphenated line ends are not joined.
- **Region**: cropped with an 8 px margin so edge glyphs are not cut, recognized, boxes mapped back, lines whose centre is outside the region dropped.
- **Limits**: a region of more than 64 million pixels is `invalid_input`; text is cut at 100,000 characters with `truncated: true`; a region outside the image or empty is `invalid_input`.
- Free tier, read-only, on this computer only: no network, nothing stored (no sidecar, no manifest change, no revision, not an undo step). `copy_to_clipboard` copies the same text, only when some was found; an empty result leaves the clipboard alone.
- **Editor**: a **Copy text** button (the whole image) and a **Select text** tool (drag a rectangle); the text goes to the clipboard through the action; a toast says "Copied N words" or "No text found". **Library**: a "Copy text" entry in the item menu and in the capture toast. Recognition runs off the UI thread with a spinner. Where there is no recognizer the entries are dimmed with the install hint, as Auto-redact is.
- **What each system reads** (be exact in the UI hint and the docs): macOS (Apple Vision) reads the languages Vision supports; Windows (`Windows.Media.Ocr`) reads only the OCR languages installed in Windows (Settings, Time and language, Language and region), and falls back to `tesseract` when none is usable; Linux uses the system `tesseract`, which reads English with the `eng.traineddata` it comes with, and other languages only when their traineddata is installed (`CM_TESSDATA`). Handwriting and heavily stylised text are not promised on any system. Accuracy is not claimed beyond what `verification/` records.

**Acceptance criteria**
- **AC1** Given a screenshot whose recognizer returns lines out of order, in two columns and with a large vertical gap, then `text` has the lines in reading order, the columns one after the other and a blank line at the gap.
- **AC2** Given a pixelate op over text, when `recognize_text` runs, then the recognizer is given the rendered image (pixel for pixel the image `annotate` returns), which differs from the original inside the rectangle; and after undo it is given the original again.
- **AC3** Given `region`, then only that region (plus the margin) is given to the recognizer and the returned boxes are in original pixels, also after a crop op.
- **AC4** Given `copy_to_clipboard: true` and text found, then the clipboard holds the same text, the image slot is untouched, and the capture's revision and ops are unchanged; with no text found nothing is copied.
- **AC5** Given no recognizer, then the action answers `not_implemented` with the install hint, before any work, and the editor shows the entries dimmed with that hint.
- **AC6** Given a bad `region`, `format` or other parameter, then `invalid_input` before any recognition.
- **AC7** (editor) Given the Select text tool and a dragged rectangle, then the daemon action runs with that rectangle in image pixels and the toast reports the number of words or that none was found.

**Edge cases and errors** — Very large screenshot: runs off the UI thread; rotated or vertical text is read as the recognizer reads it. No text: `text: ""`, not an error. Another app holding the clipboard: `copied_to_clipboard: false` and a warning.

**Agent interface** — MCP `recognize_text` (Free, read-only), CLI `capturemann text <id> [--region x,y,w,h] [--copy] [--lines]`; same parameters and result.

**Out of scope** — Recognizing text in recordings or video frames, a searchable text index of the library, translating the recognized text, a PDF with a text layer.

**Test notes** — `layout_text` and the region crop with scripted `TextLine`s (`crates/cm-render/src/ocr/text.rs`); the action with the daemon rig and a recording recognizer (`crates/cm-daemon/tests/recognize_text.rs`); MCP and CLI tests; the real-engine check on synthetic PNGs is ignored by default (`crates/cm-render/tests/ocr_real.rs`); `tests/manual/ANN-5.md` for the editor.
