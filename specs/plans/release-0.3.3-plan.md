# Release 0.3.3: annotation pack (analysis of `0.3.3.md` and detail plan)

Status: **all open questions answered Nov 12, 2026 (section 5); waiting for the maintainer's review of this plan before Phase 0; nothing built.** Source: the maintainer's `0.3.3.md` at the repository root (callout, step badge upgrades, spotlight, magnify, stamps). The earlier scope list, the Velopack spike notes and what 0.3.2 taught us are in section 6 below.

## 1. What the proposal is, and how it fits the repository

| Item in `0.3.3.md` | Spec today | Proposal here |
| --- | --- | --- |
| 1 Callout | ANN-4 is a P2 *Draft* ("callout, stamp, image overlay"; no ACs) | Finish ANN-4 for `callout` and `stamp`; leave `image` (pasted overlay) in the P2 bucket |
| 2 Step badge upgrades | ANN-1 (step badge, AC2 renumbering) | New **ANN-6** "Step badge: label, numbering style, reorder" extending ANN-1; ANN-1 text and AC2 stay true for old ops |
| 3 Spotlight | none | New **ANN-7** Spotlight |
| 4 Magnify | none | New **ANN-8** Magnify |
| 5 Stamps | ANN-4 | In ANN-4 (optional, last) |
| 6 **Import existing images** (added by the maintainer Nov 12, 2026; not in `0.3.3.md`) | IMP-1 to IMP-5 cover video and audio only | New **IMP-6** "Import images" in `specs/features/18-import.md`, same tool, job and sheet; see section 3b |

IDs are never renumbered; ANN-5 is Copy text (0.3.2). FR table rows to add in `00-product-spec.md`: ANN-6, ANN-7, ANN-8 (P1); ANN-4 moves from P2 to P1 for callout and stamp only.

**Where the code is** (all three systems share it; no platform code needed):
- Op model, validation and the shared schema: `crates/cm-core/src/annotation.rs` (`OpKind`, 730 lines; each op has serde defaults).
- Renderer: `crates/cm-render/src/render.rs` (356 lines; one `match` over `OpKind`, ops drawn in list order, blur and pixelate read what is below them), `text.rs` (text layout), golden images in `crates/cm-render/tests/golden`.
- Agent surface: the `annotate` schema in `crates/cm-mcp/src/tools.rs` (op variants near line 117), the CLI `--annotate ops.json`, one daemon action. No new action or tool is needed (the MCP tool count stays 81); the op list grows.
- Editor: `apps/desktop/src/editor/` (`tools.rs` `Tool` enum with 12 tools, `keys.rs`, `state.rs` 1,825 lines, `geom.rs` hit-testing and handles) and `ui/editor_view/` (toolbar `chrome.rs`, `inspector.rs`, `canvas.rs`).
- Keys in use: V A L R E T H N C B X, G (Select text, 0.3.2), `[` `]`; K, S, M, U are free (Cmd+Shift+S is Save as, unaffected).

## 2. Problems in the proposal that must be settled before building

1. **Magnify contradicts itself.** It says the loupe renders from `original.png` (so it is sharp) and also that ops listed before it appear inside the loupe. Resolve: the loupe renders the source rectangle **again at `zoom` times the scale from `original.png` plus the vector ops that precede it in the list**, so it is sharp and shows earlier arrows or text. The loupe's own op is excluded. (Check how the existing 2× export scales ops; the renderer has no scale parameter today.)
2. **Magnify and Spotlight must never reveal redacted pixels.** A loupe placed before a blur in the list would show the unblurred area next to a blurred one. Rule: **blur and pixelate always apply to every copy of those pixels, wherever they sit in the list** (the loupe renders its source with all redaction ops applied). Acceptance test: OCR on the exported PNG finds no characters inside a redacted rectangle, also inside a loupe (extends ANN-2 AC1 and AC3). Spotlight `mode: "blur"` is a look, not a redaction: say so in the spec and the UI help.
3. **Spotlight and list order.** The proposal says blur-mode follows the ANN-2 order rule, but a spotlight added last would dim the arrows and callouts already drawn. Keep the model strictly list-ordered and make the **editor insert a new spotlight before the first non-redaction op** (agents control order by writing the list). Callouts and steps added later sit on top of the dim, the documentation look.
4. **"Only one spotlight per image; a second shape adds a hole to the existing op."** The editor does that as one revision (one undo step). `annotate` with two spotlight ops answers `invalid_input` naming `ops[i]` (shared error code, no new one).
5. **Callout "grows to fit the text".** A stored `rect` that the renderer silently enlarges would draw differently in the GUI and from an agent (ANN-1 AC6 parity). Resolve: `rect` is the **minimum** size; the renderer draws `max(rect, text size + padding)`; the editor writes the grown rectangle back when the text is committed, so what is stored is what is drawn. The tail tip is stored in image coordinates; moving the box does not move it (AC).
6. **Step numbering model.** Today `n` is the creation ordinal and deletion renumbers (ANN-1 AC2). With `style` and `start`, the label is `format(style, start + n − 1)`; `style` and `start` live on each op, and the Steps panel sets them for all steps together (a mixed set stays possible for agents). Reorder rewrites `n` on the affected ops in **one revision**. Letters past Z continue AA, AB (spreadsheet style). The label `text` goes to the right of the badge and flips to the left near the image edge.
7. **Older apps and new manifests (smaller than first written).** `annotation.rs` makes an unknown `op` type an `invalid_input` error, but the **library already protects the capture**: `cm-library` keeps such ops in `manifest.unknown_ops`, writes them back untouched at their place on every save, shows the warning "Kept an annotation this version cannot show", and keeps the thumbnail conservative (`thumbs.rs`). So a 0.3.3 capture opens in 0.3.2 and keeps its callouts, spotlights and loupes; 0.3.2 just does not draw them (its own renders and exports leave them out). The one real loss is **unknown fields inside a known op**: the step's new `style`, `start` and `text` are dropped when 0.3.2 saves an edit (the badge keeps its number). 0.3.2 is already shipped and cannot change, so this stays; recommendation: accept it, say it in the release text, and give the 0.3.3 reader a pass-through for unknown fields in known ops so later versions never repeat it. Check by test before release: a 0.3.3 manifest with every new op goes through the 0.3.2 library code (keep a fixture) and comes back unchanged apart from the step fields.
8. **Stamps.** Draw ✓ ✗ ! ? as vector paths (emoji fonts differ per system and break golden tests). "Before" and "After" are words: the label is written into the op at creation in the UI language (so it is stored text, not re-translated), with the Vietnamese strings reviewed by the maintainer as in 0.3.2.
9. **Spotlight and Copy text.** Copy text reads the rendered image, so text in a spotlight's blur-mode area is not copied. That is consistent (ANN-5 AC2); say it in the spec.

## 3. Detail plan

### Phase 0: decisions and spec (maintainer + me, about half a day)
- Settle section 2 items 3, 4, 6, 7 (the rest are my recommendations; say if you disagree).
- Write ANN-4 (callout, stamp), ANN-6, ANN-7, ANN-8 in `specs/features/02-annotation.md` with ACs, extend the op schema block, update `00-product-spec.md`, `02-agent-interface.md` (annotate schema), `04-delivery-plan.md` if it lists ANN.
- Freeze the op JSON. After this the editor track can start.

### Phase 1: core and render (track A; files: `cm-core/src/annotation.rs`, `cm-render/src/render.rs`, `text.rs`, `cm-render/tests`)
1. `OpKind::Callout { rect, tail, text, size, color, bg, radius, style }`, `Spotlight { shapes, dim, mode }`, `Magnify { source, at, zoom, shape, border, connector }`, `Stamp { at, kind, size, color, text? }`; `Step` gains optional `style`, `start`, `text` (serde defaults, so old manifests render the same). Validation with limits (zoom 2 to 8, dim 0 to 1, shape count, text length 2000 as in ANN-1, sizes capped at 2000, layer caps as for text).
2. Renderer: callout (bubble with curved tail, box with straight tail, reuse `text.rs`), spotlight (one dim layer with holes, `blur` mode), magnify (re-render the source at `zoom×` with the preceding ops and all redactions, circle or rectangle mask, border, connector), stamps (paths), step label and numbering styles.
3. Golden tests for each op and each style; ACs cited from tests (spotlight 40 % brightness, callout tail fixed when the box moves, steps C D E and relabel after delete, magnify sharp at 2× export, redaction inside a loupe).
4. Agent surface: `annotate` schema variants in `cm-mcp/src/tools.rs`, the CLI file form, `docs/user` text, the plugin skill `screenshot-and-redact` (mention the new ops). Parity test: the same op list from the GUI and from `annotate` renders identically (ANN-1 AC6).

### Phase 2: editor (track B, starts after the schema freeze; files: `apps/desktop/src/editor/*`, `ui/editor_view/*`, icons, `locales`)
1. Tools K, S, M, U in `Tool` (12 to 16: check the vertical toolbar at small window heights against DESIGN 5.5 and the layout guard `shrink_text`), keys in `keys.rs`, palette order, tooltips with the key.
2. Callout: place box, drag the tail tip (own handle), text through the Text tool's input method (Vietnamese Telex and VNI must work, ANN-1 AC7 pattern), box grows and writes back.
3. Spotlight: draw shapes, add a hole to the existing op, dim slider, dim or blur toggle, delete shape or op.
4. Magnify: draw the source, place the loupe, zoom, shape, connector.
5. Steps panel: label field, numbering style, start number, drag-to-reorder list.
6. Stamp picker (last, optional).
7. Icons: the kit has `zoom-in` and `search` already; new ones needed for callout, spotlight, stamp and the step label, following `design/icons/CLAUDE.md` (Lucide set; proposals: `message-square`, `focus`, `stamp`). Per-surface rules in `design/CLAUDE.md` apply.
8. i18n: wrap every string, `cm-i18n extract`, Vietnamese drafts marked fuzzy for the maintainer's review, `cargo test -p cm-desktop --lib layout_width_tests` and the pseudo-locale check.

### Phase 3: verify and release
- Unit and golden tests per AC; `cm-regress` gets one case that applies every new op through the CLI and checks the render (pixel statistics, redaction inside a loupe), run on the three systems; PARTIAL reports in `verification/`; `tests/manual/ANN-6.md` for the editor by eye (the editor has no hands-on report for ANN-1 yet, so do both).
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
- **Limits.** Maximum 16 384 px per side and 100 megapixels (checked from the header before decoding, so a decompression bomb never allocates), reported as `unsupported_format` with `data.reason: "too_large"`. Corrupt or empty: `unreadable_file`. A screenshot-like scale factor of 1 (physical = logical) unless the file says otherwise (PNG pHYs 144 dpi marks a 2x image: keep as a hint only).
- **Privacy.** Imported images are local only, as everything else. Auto-redact and Copy text work on them with no change.
- **Tests.** Pure tests on headers (orientation, size limits, bombs, CMYK JPEG, 16-bit, palette PNG), golden `annotate` on an imported image, an `import_media dry_run` row test, a `cm-regress` case (import a PNG and a JPEG, annotate, export), and `tests/manual/IMP-6.md` for drag and drop.
- **Size.** About 1.5 days, one agent, files disjoint from tracks A and B: `cm-media/src/import.rs` (probe/classify), `cm-library/src/import.rs` (bundle), `cm-mcp` schema text, `apps/desktop` Import sheet and drop target, `specs/features/18-import.md`, `locales`. The sheet change touches `apps/desktop`, so run it after track B's toolbar work lands, or in a separate files set (the Import sheet is not in `editor/`).

## 4. Size and what else fits in 0.3.3

Rough size: A about 2 days, B about 3 days, C about 1 day, D (image import) about 1.5 days, verification and release 1 day: still a normal release. **Decision (maintainer, Nov 12, 2026): in-app update (Velopack) is not brought in at this phase** (parked, spikes kept: section 6). 0.3.3 is the annotation pack plus the small carried-over fixes. Windowed audio and PhoWhisper have no release assigned; PhoWhisper stays out unless the maintainer asks.

## 5. Questions for the maintainer
1. (Answered Nov 12: the annotation pack is 0.3.3 and in-app update is not in this phase.) Windowed audio: **later** (Nov 12; no release assigned).
2. (Answered Nov 12: **accepted**.) Section 2 item 7: 0.3.2 will not draw the new ops and drops the new step fields when it saves (the ops themselves are kept); 0.3.3 adds field pass-through, a round-trip fixture test and a note in the release text.
3. (Answered Nov 12: **both accepted**.) Spotlight order: the editor inserts a new spotlight before the first non-redaction op (agents control order by writing the list). Magnify: blur and pixelate always apply to every copy of the pixels, also inside a loupe.
4. (Answered Nov 12: **stretch goal, built last**; cut first if time runs short.) Stamps.
5. (Answered Nov 12: **leave as is**.) `capturemann mcp` starts no daemon in 0.3.3: the app must be open and tools answer `daemon_unavailable`. A headless daemon (or MCP launching the app) goes on the 0.3.4+ list; open design point: Confirm-tier tools need an on-screen dialog, which a headless daemon lacks.
6. (Answered Nov 12: **accepted**.) Image import (IMP-6, section 3b): (a) formats **JPEG (`.jpg`, `.jpeg`) and PNG (`.png`) only** in 0.3.3, the rest later; (b) paste from the clipboard is a stretch goal after stamps; (c) metadata is dropped and the source file is not kept; (d) limits 16 384 px per side and 100 megapixels.

## 6. Earlier scope, spike notes and carry-over (moved here from the former plan file, Nov 12, 2026)

This section is the content of the former `release-0.3.3-plan.md` (draft of Nov 11, 2026, updated Nov 12), merged when that file was removed so the Velopack spike results and the 0.3.2 lessons are not lost.
The earlier scope list (Nov 11, 2026), kept as the record; 0.3.3 is now the annotation pack plus image import (sections 1 to 5). Windowed audio is later and PhoWhisper is out unless asked.

1. **In-app update: NOT in 0.3.3 (maintainer, Nov 12, 2026: "we do not bring in-app update at this phase"); parked, no release assigned.** The Velopack spikes and notes below stay as the record. Earlier text: moved from 0.3.2 by the maintainer, Nov 11, 2026. Everything: Velopack Apply on Windows and macOS, and the opt-in notify-only check (also the Linux path, with the `apt` command). Start from the section "In-app update with Velopack" in `release-0.3.2-plan.md` (analysis, conflicts with the Oct 25, 2026 "no network call" decision and NFR-9, the six decisions still open) and the spike reports `docs/development/velopack-spike-0.3.2.md` (macOS, GO WITH CHANGES) and `docs/development/velopack-spike-win-0.3.2.md` (Windows, when it lands). Decisions still needed: opt-in default off; Linux notify-only; feed host (a release tagged `updates`); Windows signing before Apply (depends on the SignPath outcome of 0.3.2); first-install formats; the three `deny.toml` questions (`webpki-roots`, `derivative`, `zip`); our own `UpdateSource` that sends no identifiers and verifies minisign. The maintainer must run the permission-survival check on a Developer ID build. Order: the opt-in check first (small, no packaging change), Apply after.
2. **Windowed audio** (flat memory for long files and speakers), moved from 0.3.2.
3. **PhoWhisper**, only if the maintainer wants it again: dropped from 0.3.2 because the models are fine-tuned without timestamp tokens, so adopting them means rebuilding word timing and decoding (captions, seek, speakers and reports depend on word times).

#### Spike results for item 1 (Nov 11, 2026)

macOS: `docs/development/velopack-spike-0.3.2.md` (GO WITH CHANGES). Windows: `docs/development/velopack-spike-win-0.3.2.md` (GO WITH CHANGES; ThinkPad, public 0.3.1 zip as payload, velopack and vpk 1.2.161). Windows findings to carry:
- The updater silently kills every process whose exe is in the install folder (daemon, overlay, MCP server: about 1.4 s down); a terminal whose working directory is `current\` makes the update fail after 10 retries and the old version restarts.
- No rollback and no health check: a new version that will not start leaves a dead install; vpk packs any exe without checking it uses Velopack, so CI needs its own smoke test.
- Defaults to change: `run()` auto-applies a downloaded package at startup (set `set_auto_apply_on_startup(false)`, else the Confirm tier is bypassed); the stock HTTP source has no timeout and sends a persistent random `stagingId`.
- Integrity on unsigned Windows is only the feed SHA-256 over HTTPS; a minisign check works only inside a custom `UpdateSource` (delta files are deleted after patching and the rebuilt full package differs from the published one).
- Numbers: Velopack install 2.1 s (NSIS 17.2 s), no admin, 373 MB on disk (NSIS 246 MB), Setup.exe 119.9 MB (NSIS 86.6 MB); the 0.2.9 to 0.3.1 delta is 28.7 MB (25 % of full), the 33 MB speaker models never travel in a delta; rebuilding from a delta costs about 7 s CPU on the T490; skipped versions work through chained deltas; `run()` costs 2 to 3 ms and 2.8 MB.
- Coexistence: Velopack Setup does not detect an NSIS install (two "CaptureMann" entries); a hook that runs the NSIS uninstaller silently works (6 s, keeps settings); NSIS and flat-zip installs return `NotInstalled` and can never self-update; do not use pack id `CaptureMann` (collides with the NSIS Apps key); Setup silently downgrades over a newer install; feed JSON must have no UTF-8 BOM.
- Hosting correction: GitHub shows v0.3.1 as a full release and `releases/latest/download` resolves to it, so "all 0.3.x are pre-releases" is out of date; old deltas live in older releases (a client several versions behind falls back to the full package).
- The maintainer must check Edge, SmartScreen and the non-silent Setup on a real desktop, choose the Windows integrity policy (SignPath, minisign in a custom source, or notify-only), decide the NSIS migration (hook removes the old copy or only warns), pick the pack id, approve the winget manifest change (`InstallerType: exe`, `--silent`), and if SignPath goes ahead spike signing all 13 files with `--signTemplate` on a Windows runner.

### Carried over from 0.3.2

Known by name only, no reproduction (skipped for 0.3.2 by the maintainer, Nov 11, 2026; fix when it comes back): process-tree kill on cancel (a cancelled job may leave a child process such as ffmpeg running), the redaction box left-edge trim.


Whatever of the 0.3.2 carry-over list is not done by release day: the speech-detector second opinion for the outro guard, "Always ignore this line", `estimate_vs_real` on Windows, Intel Mac hardware, Windows 10, a 16 GB Mac for long recordings, the cm-overlay exit race, the optional keyboard check of the shortcut fixes on Windows 11 and Linux X11 (`tests/manual/SHORTCUTS-1.md`; the macOS report is closed).

### Found while releasing 0.3.2 (Nov 12, 2026)

1. **Claude plugin and the directory.** Submitted; the portal **refused** the version on Nov 12 ("Local MCP servers accepted only from Verified organizations"); the maintainer decided to pursue verification and distribute from the repository meanwhile (state and the question to ask in `docs/development/plugin-submission-0.3.2.md`). The directory puts a policy hold on any plugin that runs a local command or MCP server unless the publisher is a **Verified organization** (a launcher script did not help and was reverted); until then the plugin installs from the repository marketplace. To do: find out how CraftMann becomes Verified (portal), then answer the review. Raise `plugin.json` `version` with every plugin release (pinned to the workspace version by `scripts/check_plugin_version.py`; the push webhook is set up, the directory follows `main`).
2. **The MCP server starts no daemon.** `capturemann mcp` runs `cm-mcp`, which connects to a daemon that is already running; with the app closed every tool answers `daemon_unavailable`, and the macOS bundle has no `cm-daemon` (the app embeds it). The plugin docs said the opposite until Nov 12 (corrected on `main`). **Decided Nov 12, 2026: leave it in 0.3.3 (the app must be open); a headless daemon is 0.3.4+.** Options were: leave it or make `capturemann mcp` start a headless daemon (Free tools only, Confirm tools refused). The maintainer's remark "app closed -> opened" on PLUGIN-1 check 8 was not clarified: ask what he saw.
3. **PLUGIN-1 not complete:** check 5 (the client in Settings, Agents and CLI), 9 and 10 not run; nothing run on Windows or Linux (`sh`-free: the plugin starts `capturemann mcp` directly).
4. **Regression gaps from 0.3.2:** the Windows files were the overnight candidates, not rebuilt from the tag (same code); macOS reports were run on the overnight candidates plus a standard run on the released DMG (43/0); full and hardware tiers on the released DMGs were not run; Linux aarch64 is a candidate from CI (VM regression only).
5. **winget:** PRs 472327 (0.2.8) and 477195 (0.3.1) still wait for a moderator; submit 0.3.2 after the first is accepted.
6. **Release process:** `regress.sh` on a Mac uses the dev bundle by default (stale `target/release/cm-daemon` once caused a false failure): pass `--installer <dmg>` for a release run. `make-apt-repo.sh` needs `apt-ftparchive`, which the Mac lacks: run it on the aarch64 VM (staging recipe in the Nov 12 session; key copied in and shredded). The maintainer builds and notarizes the DMGs himself (no notary credentials in agent sessions).
