# QA 0.3.4 Long Shot on Windows, pass 4 (PARTIAL, stopped early)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %.
- **Build:** main `a4f28a07` (`git archive`, `packaging\windows\build-windows.ps1`, LTO off, `CM_NO_SPEAKER_MODELS=1`), setup SHA-256 `31E117DB76A664F89E15E630EDE4FFC08906C8E07077A6721F2E85395BAF5034`, zip SHA-256 `F0801D07C9BC3F4FD5131247B5E3DF93BABBF2EC03175DB69B9FC53A719413D6`. Installed silently over the previous copy.
- **Method:** shipped CLI (`capture screenshot --long`, agent confirmation answered through UI Automation) against the running app with a private library, Edge InPrivate on the fixture pages, OCR through `capturemann text`. Stopped when the maintainer asked for the laptop; the run was paused, not finished.

## Results

| Case | Result | Evidence |
| --- | --- | --- |
| Auto, 5 screens, Edge | PASS | `reached_end`, 8 frames, 1182x4549, header 120, footer 7, OCR rows 1..67 once each in order (OCR stops reading near row 67) |
| Page with a fixed LEFT sidebar (240 px), a red left edge on rows and a fixed right bar, whole window | PASS (full width kept) | 1182x16384 (300 rows at 66 px = 19,800 px, so `limit_reached` is right), dark sidebar column present at x=20 in 94 % of sampled rows (rest is text), red row edge and right bar visible; looked at the image |
| Same page, region that includes the sidebar | PASS | 1000x3997, `reached_end`, sidebar column at x=20 in 100 % of sampled rows |
| Fixed header and footer (`bands=1`) | PASS | header 192, footer 79 px, 9 frames, 1182x4825 (OCR order check noisy: 2 dups; image not inspected for duplicates) |
| Page that jumps | PASS | `no_overlap`, 4 frames, 1182x2500, note "Stopped: the next part did not line up", rows 1..36 once |
| Limit `--max-px 2000` and the real 16,384 limit | PASS | `limit_reached` 2000 px / 16384 px, rows once in order |
| Horizontal, whole window | PASS | `reached_end`, 14 frames, 12410x739 |
| Horizontal, region, on a page already scrolled to the right end | `nothing_scrolled` (expected; see proposal 1) | 1 frame, note "the area may be covered by another window or the page is not scrollable" |
| Horizontal, region, fresh page | PASS | `reached_end`, 16 frames, 12228x500 |
| Windows Terminal 200 lines, conhost, covered target, Esc, window moved or closed, HUD Stop, passive HUD measure, second start, Hand wheel / Down key / small steps | NOT RUN | paused by the maintainer; also my harness failed after the terminal launch (wt.exe launches from the scheduled-task session hung twice and left hung `wt.exe` processes; afterwards `capturemann.exe` started inside the session exited 0xC0000142) |

## Observations

- The whole-window image keeps the browser's own scrollbar: its arrows and thumb repeat at several heights inside the stitched image (seen on the sidebar page). Not a left-content problem.
- The run only scrolls forward from where the page is: a page already at its end gives `nothing_scrolled` whose wording blames a covering window.

## Verdict

PARTIAL
