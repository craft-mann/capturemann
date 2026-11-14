# QA-0.3.4 summary, macOS (PARTIAL)

- **Date:** 2026-11-15, Claude (overnight QA, maintainer asleep). Dev bundle of main `26a05100`, isolated instance, Apple M4 Pro, macOS 26.6.2.

| Feature | Report | Result |
| --- | --- | --- |
| Click Guide GDE-1 to GDE-4 | `QA-0.3.4-click-guide-macos-2026-11-15-claude.md` | PARTIAL: AC1 to AC4 (GDE-1), AC1/AC2 (GDE-2), AC1/AC2 (GDE-3) pass with real clicks; HUD, AC5 not run; defects G1 (open), G2 (fixed, unbuilt), G3 to G5 |
| Retype, Move area, Blueprint, Zoom Lens | `QA-0.3.4-touch-up-ops-macos-2026-11-15-claude.md` | PARTIAL, render level passes |
| Boards, Bulk export, CMYK, Open With plist, 89 tools, version label, sidebar, chips | `QA-0.3.4-boards-bulk-import-macos-2026-11-15-claude.md` | PARTIAL |
| Long Shot | `QA-0.3.4-long-shot-macos-2026-11-15-claude.md` | PARTIAL: Auto scroll TextEdit passes 1 of 2 runs; Hand scroll not run |
| Regression (`cm-regress`, cm-regress built from the worktree, daemon `target/debug/cm-daemon` of Nov 14 22:37, the bundle's CLI and MCP) | `L1-macos-0.3.4-dev-bundle-26a05100-regression-{standard,full,hardware}-2026-11-15-claude.md` (+ `.json`) | standard 49/0, full 57/0 (screen allowed), hardware 57/0, 31 skipped |

Notes on the regression runs: the first standard runs used the stale `target/release/cm-daemon` (Nov 13, version 0.3.3, no guide tools) and failed `pkg.diagnostics` and `guide.*`; those are not product failures and are not stored. The daemon used for the stored runs is a debug build of unknown commit (main, built by another agent). The hardware tier ran with only `--allow screen`; microphone, camera and sound cases are skipped.

## Defects

- G1 (medium, open): guide editor picture drawn at the bottom of its box and clipped.
- G2 (low): export sheet "Review step" button clipped; fixed in worktree commit `f7cba774` (one line, not built, not seen).
- G3 to G5 (low), R1 (retake clears the redaction-proposal flag), Long Shot Auto "nothing scrolled" success on run 1: see the reports.

## Not run (whole)

Hand scroll and Long Shot in Chrome, Terminal and Preview; the HUD, countdown and launcher (hidden from capture); Settings rows; Make a board sheet and Free layout; canvas text selection; B1; MCP-not-ready icon colour; Input Monitoring card (AC5); `open -a`.
