# Release 0.3.3: annotation pack (analysis of `0.3.3.md` and detail plan)

Status: **released Nov 13, 2026 (tag `v0.3.3` on `85df0235`, published as Latest).** Added after the plan: the shared text-field look (design pack `specs/capturemann-text-fields/`, local), Steps list rows, the spotlight live drag preview, the paused-upload caption-save fix. Magnify is backlog (ANN-8 reserved). Windows regression not run on release day. Design hand-off: `specs/capturemann-0.3.3-annotation-pack/` (git-ignored, local). Source: the maintainer's `0.3.3.md`.

## 1. What the proposal is, and how it fits the repository

| Item in `0.3.3.md` | Spec today | Proposal here |
| --- | --- | --- |
| 1 Callout | ANN-4 is a P2 *Draft* ("callout, stamp, image overlay"; no ACs) | Finish ANN-4 for `callout` and `stamp`; leave `image` (pasted overlay) in the P2 bucket |
| 2 Step badge upgrades | ANN-1 (step badge, AC2 renumbering) | New **ANN-6** "Step badge: label, numbering style, reorder" extending ANN-1; ANN-1 text and AC2 stay true for old ops |
| 3 Spotlight | none | New **ANN-7** Spotlight |
| 4 Magnify | none | **Backlog (moved out of 0.3.3 on Nov 12, 2026).** ANN-8 is reserved for it; the design in section 2 items 1 and 2 stays for when it is built |
| 5 Stamps | ANN-4 | In ANN-4 (optional, last) |
| 6 **Import existing images** (added by the maintainer Nov 12, 2026; not in `0.3.3.md`) | IMP-1 to IMP-5 cover video and audio only | New **IMP-6** "Import images" in `specs/features/18-import.md`, same tool, job and sheet; see section 3b |

IDs are never renumbered; ANN-5 is Copy text (0.3.2). FR table rows to add in `00-product-spec.md`: ANN-6, ANN-7 (P1); ANN-8 (Magnify) as P2; ANN-4 moves from P2 to P1 for callout and stamp only.

**Where the code is** (all three systems share it; no platform code needed):
- Op model, validation and the shared schema: `crates/cm-core/src/annotation.rs` (`OpKind`, 730 lines; each op has serde defaults).
- Renderer: `crates/cm-render/src/render.rs` (356 lines; one `match` over `OpKind`, ops drawn in list order, blur and pixelate read what is below them), `text.rs` (text layout), golden images in `crates/cm-render/tests/golden`.
- Agent surface: the `annotate` schema in `crates/cm-mcp/src/tools.rs` (op variants near line 117), the CLI `--annotate ops.json`, one daemon action. No new action or tool is needed (the MCP tool count stays 81); the op list grows.
- Editor: `apps/desktop/src/editor/` (`tools.rs` `Tool` enum with 12 tools, `keys.rs`, `state.rs` 1,825 lines, `geom.rs` hit-testing and handles) and `ui/editor_view/` (toolbar `chrome.rs`, `inspector.rs`, `canvas.rs`).
- Keys in use: V A L R E T H N C B X, G (Select text, 0.3.2), `[` `]`; K, S, M, U are free (Cmd+Shift+S is Save as, unaffected).

## 2. Problems in the proposal that must be settled before building

1. **(Backlog, not in 0.3.3.) Magnify contradicts itself.** It says the loupe renders from `original.png` (so it is sharp) and also that ops listed before it appear inside the loupe. Resolve: the loupe renders the source rectangle **again at `zoom` times the scale from `original.png` plus the vector ops that precede it in the list**, so it is sharp and shows earlier arrows or text. The loupe's own op is excluded. (Check how the existing 2× export scales ops; the renderer has no scale parameter today.)
2. **(Magnify part is backlog.) Magnify and Spotlight must never reveal redacted pixels.** A loupe placed before a blur in the list would show the unblurred area next to a blurred one. Rule: **blur and pixelate always apply to every copy of those pixels, wherever they sit in the list** (the loupe renders its source with all redaction ops applied). Acceptance test: OCR on the exported PNG finds no characters inside a redacted rectangle, also inside a loupe (extends ANN-2 AC1 and AC3). Spotlight `mode: "blur"` is a look, not a redaction: say so in the spec and the UI help.
3. **Spotlight and list order.** The proposal says blur-mode follows the ANN-2 order rule, but a spotlight added last would dim the arrows and callouts already drawn. Keep the model strictly list-ordered and make the **editor insert a new spotlight before the first non-redaction op** (agents control order by writing the list). Callouts and steps added later sit on top of the dim, the documentation look.
4. **"Only one spotlight per image; a second shape adds a hole to the existing op."** The editor does that as one revision (one undo step). `annotate` with two spotlight ops answers `invalid_input` naming `ops[i]` (shared error code, no new one).
5. **Callout "grows to fit the text".** A stored `rect` that the renderer silently enlarges would draw differently in the GUI and from an agent (ANN-1 AC6 parity). Resolve: `rect` is the **minimum** size; the renderer draws `max(rect, text size + padding)`; the editor writes the grown rectangle back when the text is committed, so what is stored is what is drawn. The tail tip is stored in image coordinates; moving the box does not move it (AC).
6. **Step numbering model.** Today `n` is the creation ordinal and deletion renumbers (ANN-1 AC2). With `style` and `start`, the label is `format(style, start + n − 1)`; `style` and `start` live on each op, and the Steps panel sets them for all steps together (a mixed set stays possible for agents). Reorder rewrites `n` on the affected ops in **one revision**. Letters past Z continue AA, AB (spreadsheet style). The label `text` goes to the right of the badge and flips to the left near the image edge.
7. **Older apps and new manifests (smaller than first written).** `annotation.rs` makes an unknown `op` type an `invalid_input` error, but the **library already protects the capture**: `cm-library` keeps such ops in `manifest.unknown_ops`, writes them back untouched at their place on every save, shows the warning "Kept an annotation this version cannot show", and keeps the thumbnail conservative (`thumbs.rs`). So a 0.3.3 capture opens in 0.3.2 and keeps its callouts, spotlights and loupes; 0.3.2 just does not draw them (its own renders and exports leave them out). The one real loss is **unknown fields inside a known op**: the step's new `style`, `start` and `text` are dropped when 0.3.2 saves an edit (the badge keeps its number). 0.3.2 is already shipped and cannot change, so this stays; recommendation: accept it, say it in the release text, and give the 0.3.3 reader a pass-through for unknown fields in known ops so later versions never repeat it. Check by test before release: a 0.3.3 manifest with every new op goes through the 0.3.2 library code (keep a fixture) and comes back unchanged apart from the step fields.
8. **Stamps.** Draw ✓ ✗ ! ? as vector paths (emoji fonts differ per system and break golden tests). "Before" and "After" are words: the label is written into the op at creation in the UI language (so it is stored text, not re-translated), with the Vietnamese strings reviewed by the maintainer as in 0.3.2.
9. **Spotlight and Copy text.** Copy text reads the rendered image, so text in a spotlight's blur-mode area is not copied. That is consistent (ANN-5 AC2); say it in the spec.

## 3. Detail plan

### Phase 0: decisions and spec (maintainer + me, about half a day)
- Section 2 items 3, 4, 6, 7 are settled (Nov 12, 2026, section 5 questions 7 to 9); the rest are accepted recommendations.
- Write ANN-4 (callout, stamp), ANN-6, ANN-7, ANN-8 in `specs/features/02-annotation.md` with ACs, extend the op schema block, update `00-product-spec.md`, `02-agent-interface.md` (annotate schema), `04-delivery-plan.md` if it lists ANN.
- Freeze the op JSON. After this the editor track can start.

### Phase 1: core and render (track A; files: `cm-core/src/annotation.rs`, `cm-render/src/render.rs`, `text.rs`, `cm-render/tests`)
1. `OpKind::Callout { rect, tail, text, size, color, bg, radius, style }`, `Spotlight { shapes, dim, mode }`, `Stamp { at, kind, size, color, text? }`; `Step` gains optional `style`, `start`, `text` (serde defaults, so old manifests render the same). Validation with limits (dim 0 to 1, shape count, text length 2000 as in ANN-1, sizes capped at 2000, layer caps as for text).
2. Renderer: callout (bubble with curved tail, box with straight tail, reuse `text.rs`), spotlight (one dim layer with holes, `blur` mode), stamps (paths), step label and numbering styles.
3. Golden tests for each op and each style; ACs cited from tests (spotlight 40 % brightness, callout tail fixed when the box moves, steps C D E and relabel after delete).
4. Agent surface: `annotate` schema variants in `cm-mcp/src/tools.rs`, the CLI file form, `docs/user` text, the plugin skill `screenshot-and-redact` (mention the new ops). Parity test: the same op list from the GUI and from `annotate` renders identically (ANN-1 AC6).

### Phase 2: editor (track B, starts after the schema freeze; files: `apps/desktop/src/editor/*`, `ui/editor_view/*`, icons, `locales`)
1. Tools K, S, U in `Tool` (12 to 15; M is reserved for Magnify), keys in `keys.rs`. **The design groups the 16 tools into 8 palette buttons** (design spec section 2, board N6: Select V/G, Shapes A/L/R/E, Text T/K, Highlighter H, Markers N/U, Emphasis S (M joins with Magnify), Redact B/X, Crop C; the face shows the last tool used per group, remembered per user; group menu on corner click, press-and-hold or right-click; every key still picks its tool directly; blue "new" dot; tooltips like `Callout [K] in Text`; accessible names and radio-item menu). This replaces the earlier "check 16 buttons at small heights" item: the palette is about 380 px tall. Follow `design/CLAUDE.md` and DESIGN 5.5.
2. Callout: place box, drag the tail tip (own handle), text through the Text tool's input method (Vietnamese Telex and VNI must work, ANN-1 AC7 pattern), box grows and writes back.
3. Spotlight: draw shapes, add a hole to the existing op, dim slider, dim or blur toggle, delete shape or op.
4. (Magnify: backlog.)
5. Steps panel: label field, numbering style, start number, drag-to-reorder list.
6. Stamp picker (last, optional).
7. Icons: the design pack ships them in `specs/capturemann-0.3.3-annotation-pack/icons/` (Lucide 1.60.0, ISC, licence file included): `message-square` (Callout), `focus` (Spotlight), `stamp`, `text-select`, `list-ordered`, `grip-vertical`, `layers`, `gallery-horizontal-end`, `ellipsis`, `chevron-down`, `chevron-right`, `scan-text`, plus the custom `cm-group-corner.svg`; (`search` for Magnify is not needed now.) The callout draws its handles from the renderer's measured box size, not the stored `rect` (agent-made callouts can be drawn larger than stored). Copy them into `design/icons/` through the usual icon process (`design/icons/CLAUDE.md`, licence list, `specs/icon-inventory`).
8. i18n: wrap every string, `cm-i18n extract`, Vietnamese drafts marked fuzzy for the maintainer's review, `cargo test -p cm-desktop --lib layout_width_tests` and the pseudo-locale check.

### Phase 3: verify and release
- Unit and golden tests per AC; `cm-regress` gets one case that applies every new op through the CLI and checks the render (pixel statistics), run on the three systems; PARTIAL reports in `verification/`; `tests/manual/ANN-6.md` for the editor by eye (the editor has no hands-on report for ANN-1 yet, so do both).
- Docs sweep for the new grouped toolbar: `docs/user`, the plugin skill and `tests/manual` scripts that name toolbar buttons.
- Compatibility note in the release text (section 2, item 7) and a fixture test that a 0.3.3 manifest survives a round trip through the 0.3.2 library code.
- Release as 0.3.2 and 0.3.1: Security scan before merging, CI green, candidate files overnight, the maintainer tags and publishes. Windows files stay unsigned. The Homebrew cask goes through `/Users/trungtran/source/repos/homebrew-tap`.

### Agents and files (disjoint)
| Track | Files | Starts |
| --- | --- | --- |
| A core and render | `cm-core/src/annotation.rs`, `cm-render/**`, `cm-mcp` schema, CLI | after Phase 0 |
| B editor and i18n | `apps/desktop/src/editor/**`, `ui/editor_view/**`, `locales/**`, `design` icons | after the schema freeze |
| C specs, docs, regress case | `specs/**`, `docs/**`, `crates/cm-regress`, `tests/manual` | in parallel from Phase 0 |

### 3b. Import existing images (IMP-6, added Nov 12, 2026; track D)

**Why it fits.** The annotation pack only works on screenshots CaptureMann took. With image import, a phone screenshot, a design mock-up or a picture from another tool can be annotated, redacted, Copy-text'd and exported in the same way. The pieces exist: `import_media` (Free tier, `dry_run`, `remove_originals` as Confirm, the `import` job, cancel, resume, disk check), the Import sheet (IMP-5), and PNG/JPEG decoding (`image` crate with `png` and `jpeg` is already built into `cm-library`, `cm-media`, `cm-daemon`).

**Decided Nov 12, 2026 (to be written as IMP-6 with ACs):**
- **Same entry points, no new tool.** `import_media` accepts images; the tool count stays 81. CLI `capturemann import <files>` and the app's Import sheet accept them; drag and drop onto the library as for video. The sheet row shows size in pixels and status like other files.
- **Result.** One image becomes one `.cmshot` bundle (`kind: screenshot`) with `original.png` (annotation ops are in physical px of `original.png`, so the pipeline needs PNG), a manifest with `source: { path, imported_at, original_name }`, a thumbnail, and an empty ops list. The editor opens it like a screenshot; it shows in the library with the "imported" mark.
- **Formats (decided Nov 12).** In: PNG (`.png`) and JPEG (`.jpg`, `.jpeg`) only (decoders already shipped). Recognised and refused with `unsupported_format` and the format named: GIF (animated), WebP, HEIC/HEIF, TIFF, BMP, SVG, RAW. A later candidate: WebP and HEIC (iPhone), once a licence and size check is done (HEIC has patent and decoder questions). Content decides, not the extension (as IMP-1 AC8).
- **Normalisation.** Apply the EXIF orientation (a phone photo must not arrive sideways); convert to 8-bit sRGB RGBA (16-bit PNG and wide-gamut profiles are converted; the row says so); keep transparency. **Metadata is not copied into `original.png`** (EXIF with GPS and camera data stays out), and the source file is not kept unless the user keeps it by not removing the original.
- **Limits.** Maximum 16 384 px per side and 50 megapixels (maintainer, Nov 12: 100 MP RGBA is 400 MB per decode, too much for an 8 GB Mac) (checked from the header before decoding, so a decompression bomb never allocates), reported as `unsupported_format` with `data.reason: "too_large"`. Corrupt or empty: `unreadable_file`. A screenshot-like scale factor of 1 (physical = logical) unless the file says otherwise (PNG pHYs 144 dpi marks a 2x image: keep as a hint only).
- **Privacy.** Imported images are local only, as everything else. Auto-redact and Copy text work on them with no change.
- **Tests.** Pure tests on headers (orientation, size limits, bombs, CMYK JPEG, 16-bit, palette PNG), golden `annotate` on an imported image, an `import_media dry_run` row test, a `cm-regress` case (import a PNG and a JPEG, annotate, export), and `tests/manual/IMP-6.md` for drag and drop.
- **Size.** About 1.5 days, one agent, files disjoint from tracks A and B: `cm-media/src/import.rs` (probe/classify), `cm-library/src/import.rs` (bundle), `cm-mcp` schema text, `apps/desktop` Import sheet and drop target, `specs/features/18-import.md`, `locales`. The sheet change touches `apps/desktop`, so run it after track B's toolbar work lands, or in a separate files set (the Import sheet is not in `editor/`).

### 3c. Design pack: what it adds beyond the five items (Nov 12, 2026, track B)

The pack's scope line: "only the screenshot editor changes". Besides the tools it specifies three editor changes that were **not** in `0.3.3.md` or in this plan before; they are part of 0.3.3 because the maintainer supplied the design:
- **Grouped tool palette** (N6): see Phase 2 item 1.
- **View bar** (N7): `Undo · Redo | Copy text · Auto-redact | Zoom out · [68% ▾] · Zoom in | Images · Keyboard · ⋯`. The zoom value opens a menu (Fit to window, Actual size 100%, 200%, Custom n%) and replaces the separate Fit and 100% buttons; **Revert to original… moves into the ⋯ menu** (same name and confirm step as today, with the helper line "Removes every annotation, crop and redaction."); Copy text stays, its tooltip explains the difference from Select text (G).
- **Image strip** (N8, N8b): a thumbnail strip under the canvas for quick navigation to another image: same list and order as the Library view the image was opened from, library thumbnails loaded lazily, an annotation-count pill, name and date only in the hover tooltip, hidden below 640 px window height or by the Images button (remembered per user), listbox accessibility, each image keeps its own undo history, no save prompt, existing previous and next shortcuts unchanged, UI only (no agent change). Preload the previous and next full images. This needs the editor to know the Library list it was opened from (new state in `apps/desktop`; check how the editor is opened today).
- Image import (IMP-6) needs **no new screen**: it uses the existing Import form.
- Rules in the pack that the plan above already follows: spotlight (one per image, up to 8 holes, inserted before the first non-redaction op, Blur mode only a look), magnify (from `original.png`, redaction inside the loupe, connector and zoom badge), callout (tail-tip handle, box grows and writes back), steps (22 to 26 px badges, label pill flips near the edge, letters Z, AA, AB), text input through the Text tool's method. Stamps stay last and first to cut.
- New in the pack, to settle in Phase 0: the blue "new" dot is removed after first use or after one release (decide which).
- Size: the palette, view bar and strip add about 2 days to track B (B becomes about 5 days); the total rises to about 9 days.

## 4. Size and what else fits in 0.3.3

Rough size (day counts are a rough estimate; the maintainer expects 4 to 8 hours of work in total, Nov 12; Magnify is out, so A is smaller): A about 2 days, B about 5 days (with the palette, view bar and image strip, section 3c), C about 1 day, D (image import) about 1.5 days, verification and release 1 day: still a normal release. **Decision (maintainer, Nov 12, 2026): in-app update (Velopack) is not brought in at this phase** (parked; the spike reports stay in `docs/development/`). 0.3.3 is the annotation pack plus the small carried-over fixes. Windowed audio and PhoWhisper have no release assigned; PhoWhisper stays out unless the maintainer asks.

## 5. Questions for the maintainer
1. (Answered Nov 12: the annotation pack is 0.3.3 and in-app update is not in this phase.) Windowed audio: **later** (Nov 12; no release assigned).
2. (Answered Nov 12: **accepted**.) Section 2 item 7: 0.3.2 will not draw the new ops and drops the new step fields when it saves (the ops themselves are kept); 0.3.3 adds field pass-through, a round-trip fixture test and a note in the release text.
3. (Answered Nov 12: **both accepted**.) Spotlight order: the editor inserts a new spotlight before the first non-redaction op (agents control order by writing the list). Magnify: blur and pixelate always apply to every copy of the pixels, also inside a loupe.
4. (Answered Nov 12: **stretch goal, built last**; cut first if time runs short.) Stamps.
5. (Answered Nov 12: **leave as is**.) `capturemann mcp` starts no daemon in 0.3.3: the app must be open and tools answer `daemon_unavailable`. A headless daemon (or MCP launching the app) goes on the 0.3.4+ list; open design point: Confirm-tier tools need an on-screen dialog, which a headless daemon lacks.
6. (Answered Nov 12: **accepted**.) Image import (IMP-6, section 3b): (a) formats **JPEG (`.jpg`, `.jpeg`) and PNG (`.png`) only** in 0.3.3, the rest later; (b) paste from the clipboard is a stretch goal after stamps; (c) metadata is dropped and the source file is not kept; (d) limits 16 384 px per side and 50 megapixels (lowered from 100 on Nov 12 review).
7. (Answered Nov 12: **accepted**.) Spotlight, one per image: the editor adds a second shape as a hole in the existing op (one undo step); `annotate` with two spotlight ops answers `invalid_input` naming `ops[i]`; at most 8 holes.
8. (Answered Nov 12: **accepted**.) Step numbering: label = `format(style, start + n - 1)`; `style` and `start` stored on each step; the Steps panel sets them for all steps together; reorder rewrites `n` in one revision; letters continue Z, AA, AB.
9. (Answered Nov 12: **accepted**.) The blue "new" dot on the new tools disappears after one release (0.3.4); no per-user state.
10. (Nov 12.) Callout handles use the renderer's measured box size (agent-made callouts can be drawn larger than stored). Magnify is backlog. Image import cap is 50 megapixels. Scope otherwise kept (palette, view bar, image strip stay in 0.3.3).
