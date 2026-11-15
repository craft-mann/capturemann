# QA 0.3.4 CAP-6 Long Shot on Windows, pass 3 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, one display 1920x1080 at 150 %, logged-in desktop session (`win-run-in-session.ps1`).
- **Build:** main `5e832011` (`git archive HEAD`, `packaging\windows\build-windows.ps1`, LTO off, `CM_NO_SPEAKER_MODELS=1`). Zip SHA-256 `38abeef6be9b82faf3ca29c92426f9952dc81467e0e4e865423e91f16dfa5151`, setup SHA-256 `9febf0322b639f3be59b2fee44bdd20d14f33dc17bcb65c5507780cf9f699dc8`, installed silently into `%LOCALAPPDATA%\Programs\CaptureMann`. Probe examples (`long_shot_probe`) built from the same tree.
- **Browsers:** Edge (InPrivate) and Chrome for Testing (portable, stable channel), page `tests/fixtures/long-shot/long-page.html` (`screens=5`, `bands=1`, `jump=1`, `screens=20/30`) and a 25-column page (`COL 01` to `COL 25`, 200 CSS px each). Windows Terminal running `cmd` with 200 lines `LINE n the quick brown fox ...`.
- **Method:** the engine through `long_shot_probe` (real capture and `SendInput` scroller), wheel and key events sent with `mouse_event` and `keybd_event`, OCR of the result through `capturemann text` (Windows OCR) with a check that the labels are unique and in order; the daemon path through the shipped CLI (`capture screenshot --long`) with the agent confirmation answered by invoking the "Allow" button through UI Automation. Floating windows were measured, not looked at.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Auto, Edge and Chrome, 5 screens | PASS | `reached_end`, 7 frames, 1482x4681 / 1482x4770; OCR rows 1..59 / 1..69 once each, in order |
| Fixed header and footer (`bands=1`) | PASS | Edge 8 frames, header 192 px, footer 79 px; Chrome 9 frames, header 287, footer 73; rows 1..69 once, in order |
| Page that jumps (`jump=1`) | PASS | `no_overlap`, note "Stopped: the next part did not line up"; frames before the jump kept (Edge 3 frames rows 1..32, Chrome 4 frames rows 1..39) |
| Limit 2,000 px and the real 16,384 px | PASS | `limit_reached`, 3 frames 2000 px; 23 frames 16384 px, rows 1..246 once each in order, note "Reached the longest image CaptureMann can edit" |
| Horizontal, whole browser window (used to stop after one frame) | PASS (fixed) | Edge `reached_end`, 7 frames, 7515x909; Chrome 7 frames 7503x819; OCR `COL` labels 24 of 25 once each in order (the 25th label was not read by OCR; the width matches 25 columns) |
| Horizontal with a region, Edge | PASS | `reached_end`, 7 frames, 7503x500, `COL 01` to `COL 25` once each, in order |
| Horizontal with a region, Chrome | FAIL (unresolved) | `no_overlap`, 1 frame, 1470x500, one run. My region (window y + 160, 500 px high) may include a toolbar part; not repeated with another offset |
| Hand scroll, 12 wheel notches 500 ms apart, Edge and Chrome | PASS | `stopped_by_user`, 13 / 12 frames, 1482x2841; rows 1..41 / 1..40 once each in order |
| Hand scroll, Down key, 40 presses 300 ms apart | PASS | 9 frames 3148 / 3141 px; rows 1..46 / 1..44, once, in order |
| Hand scroll, continuous small wheel steps (30 x delta 40 every 250 ms; used to give 1 frame) | PASS (fixed) | 7 frames, 1482x2541; rows 1..36 / 1..35 once, in order, both browsers |
| Hand scroll, fast small steps (80 x delta 30 every 60 ms), Edge | PASS | 10 frames 4041 px, rows 1..59 once, in order |
| Windows Terminal, 200 near-identical lines, Auto | FAIL (still open, improved) | `no_overlap` after 3 frames, 1721x1711; OCR `LINE 54` to `LINE 111` once each in order, then "Stopped: the next part did not line up" (before: stopped at line 86 of 200). The terminal was not scrolled to the top before the run (first visible line 54), so the run started in the middle, but it still ended 90 lines before the end |
| Classic console host (`conhost.exe cmd`) | NOT RUN | the launch step hung my script; not repeated |
| First-run Auto (first run on a freshly opened window) | PASS | every case above opened a fresh window and was its first run: 0 of 14 Auto runs ended after one frame |
| Auto when another window covers the target's centre | FAIL (new, low) | a console window over the Edge window: `nothing_scrolled`, 1 frame, note "Nothing scrolled: check that the window can scroll". The wheel goes to the window under the pointer; Auto does not raise the target first. The note does not name the cause |
| Esc during a run started from the CLI (global key) | PASS | `stopped_by_user`, 12 frames, 8809 px kept (before: Esc did nothing) |
| CLI-started run: passive HUD | PASS (measured) | a toolbar window 531x95 px (354x63 logical) with `Stop` (66x66 px) and the status text "Started by agent capturemann-cli"; a second window 682x155 px with "Started by agent via MCP" and "Dismiss notification" |
| HUD Stop invoked through UI Automation, 3 runs | PARTIAL | the run ends and the image is kept (8 frames, 5889 px) each time, but the stored end is `stopped_by_user` in 1 run and `no_overlap` with the note "the next part did not line up" in 2 runs (spec: the same end as Esc) |
| Limit from the CLI (`--max-px 2000`) | PASS | `limit_reached`, 3 frames, 2000 px |
| Confirmation for Auto from the CLI | PASS | an "Allow" dialog appeared on screen every run |
| Hand scroll refused for agents | not repeated | carried from earlier reports |

## Problems

1. Terminal long output still stops at line 111 of 200 (medium, open).
2. Chrome horizontal with a region ended after one frame in one run (unresolved, probably my region).
3. Auto Long Shot does not bring the target window to the front, so a covering window receives the wheel (low).
4. HUD Stop reports `no_overlap` in 2 of 3 runs (low).

## Not run

Overlay flow with a person (launcher, mode bar, Start, side preview), conhost, a second monitor, other DPI scales, Windows 10.

## Verdict

PARTIAL
