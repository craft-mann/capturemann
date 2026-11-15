# QA 0.3.4 Windows summary, pass 4 (PARTIAL, paused by the maintainer)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %.
- **Build:** main `a4f28a07`, built on the laptop, installed silently at `C:\Users\trung\AppData\Local\Programs\CaptureMann` (`capturemann 0.3.4`, `cm-desktop.exe` SHA-256 `207616C3CA76E4A3B288BCF2890B3C36A476550F2F3523D438FCAC576BE17CDD`). Zip `capturemann-0.3.4-windows-x64.zip` SHA-256 `F0801D07C9BC3F4FD5131247B5E3DF93BABBF2EC03175DB69B9FC53A719413D6` (73,193,647 bytes); setup `capturemann-0.3.4-windows-x64-setup.exe` SHA-256 `31E117DB76A664F89E15E630EDE4FFC08906C8E07077A6721F2E85395BAF5034` (51,326,490 bytes). Files in `C:\wt\out` (kept, with `C:\wt\main`, `bundles`, `target`). Unsigned. App not running. No cm-regress run.

| Area | Result |
| --- | --- |
| Install | PASS: silent install exit 0, version 0.3.4, uninstall entry present |
| Long Shot | partly run, see `QA-0.3.4-long-shot-windows-pass4-2026-11-15-claude.md`: Auto, left sidebar (full width kept, whole window and region), bands, jump, limits, horizontal window and region: PASS |
| Click Guide, library, text colour, Retype, Move area, Blueprint, Zoom Lens, boards, bulk export, CMYK, Open With, B2/B3 drag, long path, Chrome | NOT RUN (paused by the maintainer's request for the laptop; harness problems before that) |

## Defects

| # | Severity | Defect | Steps and evidence |
| --- | --- | --- | --- |
| 1 | none new | no product defect confirmed in this pass | the only failing cases were my harness (hung `wt.exe`, `capturemann.exe` 0xC0000142 inside the task session after hung terminal launches, probably desktop resource exhaustion) |

## UX proposals

| # | Proposal | Evidence |
| --- | --- | --- |
| 1 | Auto Long Shot on a window already at the end of the page ends `nothing_scrolled` with a note that blames a covering window; scroll to the start first, or say "already at the end" | horizontal region run on an already scrolled page |
| 2 | Whole-window Long Shot keeps the browser scrollbar, which repeats (arrows, thumb) down the image; exclude it or say so | sidebar page image |

## Not run

Everything listed as not run above, plus Windows Terminal and conhost output, covered window, Hand scroll, HUD Stop, Esc, second start, Click Guide, editor ops and boards.

## Lab state

Temporary libraries, scripts and `C:\lab\q4` removed; scheduled tasks removed; apps and browsers stopped. A few stale `wt.exe` clients may remain until a reboot. `C:\wt` kept.

## Verdict

PARTIAL
