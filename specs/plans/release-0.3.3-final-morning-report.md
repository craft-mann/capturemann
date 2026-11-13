# Release 0.3.3: morning report (Nov 13, 2026)

Written by the follow-up session (Claude Code) after the orchestrator handed back a partial report. **State: all features merged on `main` and pushed once (Nov 13, 08:2x); CI result and regression numbers are in the "Update" lines at the end; nothing is tagged, signed or published.**

## What is in `main`

Phase 0 specs and frozen op JSON, track A (renderer, `callout_box`, MCP schema, pass-through), B1 (grouped palette, callout K, spotlight S, steps panel, stamps U), B2 (view bar, image strip), C (cm-regress cases `annotate.pack` and `import.images`, manual scripts `ANN-6.md` and `IMP-6.md`, docs sweep, release text drafts), D (image import IMP-6: PNG and JPEG, 50 MP cap, CMYK JPEG refused as `jpeg_cmyk`). Cut: clipboard paste for images. Not in 0.3.3 by decision: Magnify (ANN-8 reserved), in-app update, windowed audio, PhoWhisper.

## Checks run on the merged code (macOS, this Mac)

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean after one formatting commit (`tools.rs`) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | pass |
| `cargo deny check` | pass |
| Trivy gate (CRITICAL, secrets) | pass |
| `check_target_os.py`, `cm-i18n check`, link checks | pass |
| `cm-core`, `cm-library`, `cm-mcp`, `cm-cli` tests | pass |
| `cm-render` unit, `golden_033`, `golden`, `properties`, redaction, `fallback_fonts`, compose, caption, cursor, perf | pass |
| `cm-media` caption_export, decode, edit_view_export, import, quick_export, vp9_export | pass |
| `cm-daemon` `import_images` (6), `import` (17), `suggest_redactions` | pass |
| `cm-desktop` lib (skipping `runs_a_real_`), `layout_guard`, `text_input`, `import_request` | 1691 tests: one real failure (duplicate icon entries from merging B1 and B2), fixed and re-run: icon tests 21 passed |
| Compatibility with the shipped 0.3.2 library code | done: a 0.3.3 manifest with every new op keeps all 8 ops through 0.3.2's open, edit and save; as planned, unknown fields in known ops are dropped (`future_arrow_field`, and a step's `style`, `start`, `text`, `future_step_field`); callouts, spotlight, stamps and the unknown op are kept untouched with the warning "Kept an annotation this version cannot show" |

**Not re-run on the merged code (they take hours on this Mac and the code they cover is not part of 0.3.3):** the remaining `cm-daemon` integration binaries (49 of 52), the remaining `cm-media` and `cm-cli` slow cases beyond what ran, `cm-storage`, `cm-publish`, `cm-platform`, `cm-ipc`, `cm-overlay`. One `cm-daemon` test (`ipc::tests::r2_7_stale_socket_is_replaced_live_one_is_not`) failed once under heavy load and passed 3 of 3 alone; `ipc.rs` has not changed since 0.0.1, so treat it as a load flake to fix separately.

## Why this release was slower than usual

Phase 0 did code as well as specs (about 470 lines in `cm-core`); the agents shared one `target/` folder so cargo runs queued on one lock; a full `cargo test --workspace` crawled for over 3 hours (GPU compose tests take 80 to 180 s each, `cm-daemon` has 52 test binaries); merge bugs (duplicate icons, six tests and guards) showed up only at the end, and each `cm-desktop` rebuild takes 50 to 60 minutes here. Next time: spec text only in Phase 0, a `target/` per agent, per-crate gates with time limits, merge each track as it finishes.

## Not done (and why)

- **Windows:** the ThinkPad was not reachable (`lab.sh status` showed "up: none" at about 02:11 and later): no Windows candidate build and no `regress.sh windows --tier full --allow screen` run. **Needs the ThinkPad switched on.**
- **Linux x86_64:** needs the ThinkPad booted into Ubuntu (same address as Windows). The aarch64 VM was not used.
- **Editor by eye:** no editor window was looked at (palette, callout, spotlight, steps, stamp picker, view bar, image strip, Import sheet); scripts `tests/manual/ANN-6.md` and `IMP-6.md`.
- **macOS signing and notarization, tag, publish, Homebrew tap, winget:** the maintainer's, steps in `docs/development/release-day-0.3.3.md`. The `<FILL>` items of the release text need the final files and the reports.

## For the maintainer to review

- Vietnamese strings are fuzzy drafts in `locales/vi.po` (about 62 from B1, more from B2 and D).
- Image import refuses CMYK JPEG (the spec says "convert or refuse").
- B1 used segment buttons instead of sliders and click-to-type buttons in the inspector; B2 left out the thumbnail edge fade and kept Revert without a confirm step.
- The pack's icons carried C2PA metadata, which was stripped.
- The `cm-regress` skip detection for annotate and import on a 0.3.2 package guesses the old error wording.
- The root `README.md` still says import is video only; Info.plist and the Windows installer do not register image extensions for Open With.

## Updates
