# Annotation (ANN)

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

### ANN-1 Drawing tools: arrow, line, rectangle, ellipse, text, highlighter, step badge, crop
**Priority** P0 · **Status** In progress (built on macOS: the op schema and validation in `cm-core`, the renderer with golden tests in `cm-render`, the editor in `apps/desktop` and the `annotate` action and tool; ACs 1 to 6 are cited by automated tests; no hands-on editor verification report is stored; the editor has not run on Windows) · **Crates** cm-core, cm-render, apps/desktop · **Depends on** CAP-1, DESIGN §5.5

**User story** — As a developer, I want to draw arrows, boxes, text and numbered steps on a screenshot, so that others understand it without extra explanation.

**Description**
- Tools and shortcuts: Select (V), Arrow (A), Line (L), Rectangle (R), Ellipse (E), Text (T), Highlighter (H), Step badge (N), Crop (C), Blur (B, ANN-2), Pixelate (X, ANN-2).
- Shift constrains: arrows/lines to 0/45/90°, rectangles to squares, ellipses to circles.
- Every op is selectable, movable, resizable via handles, recolorable, and deletable (Delete/Backspace). Copy/paste duplicates ops with +10 px offset.
- Step badges auto-number in creation order (1, 2, 3…); deleting one renumbers the rest; the next number is shown on the cursor.
- Text: click to place, type, Enter for a new line, Esc to finish; default 24 px white with 2 px black outline (DESIGN annotation palette).
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

**Edge cases and errors** — Text with emoji or CJK renders with font fallback (`cosmic-text`); ops with coordinates outside the image are clipped, not rejected; invalid op type → `invalid_input` naming the op index (`ops[3]: ...`) when it comes from a client. **Forward compatibility:** an op type this build does not know that is already stored in a manifest (written by a newer version) is preserved and written back unchanged, not treated as corruption; it is not rendered.

**Agent interface** — MCP `annotate { capture_id, ops: Op[], replace?: false, copy_to_clipboard?, return_image?: true }` → `{ revision, image }` (Free); `return_image: false` (CLI `--no-image`) leaves the image out of the result, which the editor uses to stay cheap. `get_capture` returns current ops. CLI `capturemann annotate <id> --ops ops.json`.

**Out of scope** — Freehand pen (P2), shapes library, layers panel.

**Test notes** — Golden PNGs per op type at 1× and 2× in `tests/golden/annotation/`; property test: serialize → deserialize → render is stable.

---

### ANN-2 Blur and pixelate
**Priority** P0 · **Status** In progress (blur and pixelate built and tested in `cm-render` (AC1 by a structural stand-in for OCR, AC2, AC3); Auto-redact and `suggest_redactions` (AC4) are built on macOS (Oct 23, 2026): the `cm_render::ocr` recognizer trait with Apple Vision, the pure detector, the action through daemon, MCP and CLI, and the editor's Auto-redact button with dashed boxes and Accept all / one / Dismiss, all tested with a scripted recognizer; the Vision recognizer ran once on a synthetic PNG (ignored test), the editor UI is untested by eye, `tests/manual/ANN-2.md` has not been run, AC1 with real OCR has not been run, Windows has only an unavailable stub (Windows.Media.Ocr is a later milestone)) · **Crates** cm-render · **Depends on** ANN-1

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

### ANN-4 Callouts, stamps and image overlays
**Priority** P2 · **Status** Draft (not in 0.2.0; detail before work starts) · **Crates** cm-render

**User story** — As a creator, I want speech-bubble callouts, stamps (✓, ✗, !) and pasted images, so that my screenshots look polished.

**Summary** — New op types `callout` (text box with a pointer tail), `stamp` (built-in vector icons), `image` (pasted PNG with scale and rotation).

**Key criteria (to refine)** — New ops follow the same schema rules, undo, and golden-render tests as ANN-1.
