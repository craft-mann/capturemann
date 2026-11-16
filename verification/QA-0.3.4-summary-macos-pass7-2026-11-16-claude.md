# QA 0.3.4 summary, macOS pass 7 (PARTIAL)

- **Date:** 2026-11-16. **Who:** Claude (QA agent, macOS pass 7). Apple M4 Pro, macOS 26.6.2. Build under test: the dev bundle from `main` b2169576 (binaries dated Nov 16 10:42), **without** the later jump-page stitching fix. Release daemon built fresh from b2169576 (`/tmp/cmq7-rel/cm-daemon`). Nothing is called "verified".
- Reports: `QA-0.3.4-long-shot-macos-pass7-...`, `QA-0.3.4-click-guide-macos-pass7-...`, `QA-0.3.4-editor-library-misc-macos-pass7-...` (all 2026-11-16-claude.md); pictures `QA-0.3.4-shots/macos-pass7-*.png`.

## Regression on the final bundle (daemon from b2169576, `--commit b2169576`)

| Tier | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard (run by the orchestrator) | 49 | 0 | 15 | `L1-macos-0.3.4-regression-standard-pass7-2026-11-16-claude.md` |
| full | 50 | 0 | 30 | `L1-macos-0.3.4-regression-full-pass7full-2026-11-16-claude.md` |
| hardware (`--allow screen`) | 58 | 0 | 30 | `L1-macos-0.3.4-regression-hardware-pass7hw-2026-11-16-claude.md` |

## Feature results

| Topic | Result |
| --- | --- |
| Long Shot, fixture with fixed sidebar and edge strips | PASS (seen): 13 frames, 15654 px, reached end |
| Long Shot, terminal-like page | PASS (seen) |
| Long Shot, busy, Hand scroll with scroll-back (release example) | PASS |
| Long Shot, Esc | PASS for the stop, frames kept not proven |
| Long Shot, window closed | FAIL (wording `internal: Failed to copy data`) |
| Long Shot, real public pages (carchargeos.com at 2 widths, Wikipedia, YouTube channel) | **FAIL**: 1 to 2 frames, `no_overlap`, first scroll moved far less than asked |
| Click Guide, window (whole window every step) and display (one shared crop), exports MD/HTML/PDF written | PASS |
| Click Guide, Vietnamese name | PASS (one cut tile label) |
| Retype Vietnamese, Blueprint suggest, Board, Bulk export | PASS (thin, results looked at only for Retype) |

## Defects (top first)

| Id | Severity | Defect |
| --- | --- | --- |
| LS-13 | high | Auto Long Shot of real pages ends after 1 to 2 frames (`no_overlap`); the real first shift (about 520 to 700 px) is far from the asked step (1469) the stitcher searches around |
| LS-12 | medium | `nothing_scrolled` when CaptureMann's own window covers the target's centre though other points were free |
| LS-14 | low-medium | target window closed mid-run: `internal` "Failed to copy data" |
| LS-15 | low | run on a page already at its end: `nothing_scrolled` without saying so |
| CG-11 | low | Vietnamese tile "Hướng dẫn theo thao tác" label cut at the bottom |
| CG-12 | low | "Long image" pill on a normal tall window frame in the guide editor |
| CG-13 | low | guide card thumbnail is the uncropped display frame (and "No preview" at first) |
| LIB-1 | low (carried) | tall Long Shot cards are a blurred crop |

## UX proposals (one line each)

1. After a Long Shot ends with fewer than 3 frames, say why and offer Hand mode.
2. Search for the join around the measured first shift, not only the asked step.
3. Expose the agent confirm card in the accessibility tree; make Return the named safe action (today Return declines, Tab then Return confirms).
4. Say "already at the end" for a run that starts at the page end, or scroll to the top first.
5. Show and let the user edit the shared crop of a display Click Guide; use it for the card thumbnail.
6. Name the target window in the "window closed" message and keep the frames.

## Not run

Overlay bar and sidebar tile start of Long Shot and Click Guide (no accessibility tree), Hand scroll through the real HUD, toasts, Click Guide countdown, retake, 500-step cap and its toast, the Click Guide step-title in Vietnamese bar and toasts, Move area, Zoom Lens, Text Auto colour, tile hover/active/disabled, library hover-select and selection bar, Retype edit-mode preview, Blueprint applied, Published and Needs attention chips with data, real Terminal app, a second monitor, Intel Mac. The first screen capture I took (to find the confirm card) showed unrelated windows; I deleted it and did not store or describe it.

## Cleanup

My app instance, Chrome profile process and `/tmp/cmq7` were removed; the maintainer's app was never touched. `/tmp/cmq7-rel` (fresh release `cm-daemon`, `long_shot_hand`, regress logs) is kept for the orchestrator.
