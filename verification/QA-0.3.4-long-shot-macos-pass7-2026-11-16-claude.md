# QA 0.3.4 Long Shot (CAP-6), macOS pass 7 (PARTIAL)

- **Date:** 2026-11-16 (about 10:40 to 11:15). **Who:** Claude (QA agent, macOS pass 7). Apple M4 Pro, one display used (1512x982 pt at 2x), the maintainer had quit CaptureMann and said the Mac was free.
- **Build tested:** the dev bundle `target/bundle/CaptureMann-dev.app` (binaries dated Nov 16 10:42, from `main` b2169576, **without** the later "jump-page protection" stitching fix), copied to `/tmp/cmq7/App.app` and run as a private instance (own library, config, socket; `CM_LONG_SHOT_DEBUG_DIR` set). Hand scroll used the **release** example `long_shot_hand` built from the same tree (b2169576).
- **Method:** my own Chrome profile (`--user-data-dir`, `--mute-audio`, no logins, no forms, nothing played), window-targeted Long Shot from the CLI (`capture screenshot --long --window <id>`). The on-screen confirm card is not visible to screen capture or the accessibility tree; I confirmed it with the keyboard (Tab, Return: Return alone **declines**). The screen was only captured as window captures of my Chrome; one display screenshot taken to find the dialog was deleted at once. Pictures: `QA-0.3.4-shots/macos-pass7-long-shot-*.png` (my fixtures and the three public sites, window captures only).
- **Not driven:** the sidebar tile and the overlay bar (no accessibility tree), so the Auto and Hand choice in the real bar, toasts, and Hand through the real HUD are NOT RUN. Esc and the stop paths were driven through the CLI run.

## A. Fixture pages (140 numbered paragraphs, my files in `/tmp/cmq7/fx`)

| Case | Result | Evidence |
| --- | --- | --- |
| LS-1 fixed left sidebar (14 repeated links), red 14 px strip left, blue strip right, overlay scrollbar | PASS (seen) | 13 frames, 1540 x 15654, `reached_end`, note null. Text S001 to S140 continuous in the crops I looked at (S059 to S081 seen), the fixed sidebar repeats slice by slice as designed, both edge strips run unbroken; the overlay scrollbar thumb is drawn at several heights (three places in the crop I viewed) because each slice shows it where it was |
| Terminal-like page (black, green monospace, 400 lines) | PASS (seen) | 12 frames, 1540 x 13418, `reached_end`; lines 0177 to 0194 at 6.4k px and 0377 to 0400 at the bottom are in the right order and place (33.4 px per line all the way, no duplicate or missing line seen). Not the real Terminal app (the maintainer's Terminal is running; I did not touch it) |
| Second Long Shot while one runs | PASS | `busy`, exit 6, "A Long Shot is already running. Stop it first.", no dialog |
| Esc during a CLI run | PASS (partly) | run ended `stopped_by_user`, 1 s after the key; it had 12 frames and 13418 px, the same as a complete run, so Esc probably came when the run was almost done: the frames-kept part is not proven |
| Window closed during the run | **FAIL (wording)** | Chrome killed 2.5 s after the confirm: the CLI answers `internal` "Failed to copy data" (exit 1), no frames kept, no window title. Windows pass 7 got a named message |
| Hand scroll, release example (`long_shot_hand`, `--secs 30`, Page Down x4, Page Up x2, Page Down x6, arrow keys) | PASS (seen) | `frames 16`, 1540 x 12456, `stopped_by_user`, `header_px 174` (the browser tab and toolbar kept once). Page text S059 to S076 continuous in the crop I viewed. The scroll-back (two Page Up) added no frame and no duplicated text that I saw. Pass 6 got 2 frames from the debug build; the release build works. I did not diff every seam |
| Run started on a page already at its end | NOTE | `nothing_scrolled` (2 of 2), the run does not go to the top first |
| Run while CaptureMann's own studio window covers the page's centre | **FAIL** | `nothing_scrolled` (side fixture, Wikipedia) although the left quarter of the Chrome window (x 0 to 332 pt) was free. After moving my window away, the same runs scrolled. Pass 3 said the quarter points were added (LS-flaky b); not effective here |
| Hand scroll through the real HUD, tile start, toasts, 50 MP, horizontal, display target | NOT RUN | |

## B. Real public pages (no logins, muted, nothing played, whole window, Auto, vertical)

| Page, window | End, frames, size | What the PNG shows |
| --- | --- | --- |
| https://www.carchargeos.com/ , 1100 pt wide (2200 x 1720 px) | `no_overlap`, **1 frame**, 2200 x 1720, 2 of 2 runs | Only the first screen. The stitcher refused the second frame: "no shift matches (header 174), closest shift 1469 with 61 of 61 compared lines differing". The refused frame (`macos-pass7-long-shot-carchargeos-refused-frame.png`) is scrolled by only about 520 px, far less than the asked 1469, and has a translucent sticky header over blurred content |
| same, 700 pt wide (1400 x 1720 px) | `no_overlap`, **1 frame**, 1 run | the same refusal, the same "closest shift 1469" |
| https://en.wikipedia.org/wiki/Rust_(programming_language) , 1100 pt | `no_overlap`, **1 frame** | The refused frame is scrolled about 700 px, again far less than 1469; a fixed contents button floats over the text and the infobox is sticky on the right. The same words "closest: shift 1469 with 61 of 61 compared lines differing" |
| https://www.youtube.com/@Google/videos , 1100 pt | `no_overlap`, **2 frames**, 2200 x 2236 | Masthead (112 px) kept once under the browser bar (header 286 px); video cards in order, no doubled card seen in the part viewed (`macos-pass7-long-shot-youtube-result.png`). Stopped after one join, "closest shift 1363, 55 of 55 lines differ" |

Observation for B: on all three real pages the first scroll step moved the page by much less than the asked step (about 520 to 700 px of the asked 1469), and the stitcher only looks near the asked step, so the real shift is never found and the run ends after 1 or 2 frames. My own plain pages scrolled the full step. I did not find why (smooth-scroll animation still running when the frame is taken is my guess; not proven). The refused-frame dumps were read in place (the dump folder was deleted with `/tmp/cmq7`); the refusal text is quoted above. This is the same family as LS-9 (pass 6) and the Windows pass 7 finding 1, and the real pages hit it every time, so it is **not fixed in b2169576**. The later jump-page fix on `main` was not in the bundle I tested.

## Defects

| Id | Severity | Defect |
| --- | --- | --- |
| LS-13 | high | Auto Long Shot of real public pages (carchargeos.com, Wikipedia, YouTube channel) ends after 1 to 2 frames with `no_overlap`: the first scroll moves the page much less than the asked step and the join search only looks near the asked step |
| LS-12 | medium | `nothing_scrolled` when CaptureMann's own window covers the target's centre, although other points of the target were free (2 of 2); the run does not try another point |
| LS-14 | low-medium | Target window closed during a run: `internal` "Failed to copy data" instead of a message naming the window and keeping the frames |
| LS-15 | low | A run that starts at the page end says `nothing_scrolled` (page not scrollable); say "already at the end" or scroll to the top first |
| LS-10 | low | (carried) the confirm dialog text blames "Ask first" setting; not re-read this pass (the card cannot be read) |

UX proposals: (1) when a run ends after fewer than 3 frames name the cause and offer Hand mode; (2) treat a first step that moved far less than asked as "scrolled by N" and search around the measured shift, not the asked one; (3) show the confirm card in the accessibility tree so keyboard and screen-reader users, and QA, can answer it; (4) make Return confirm and Esc decline, or label which button is default (today Return declines).

## Not run

Overlay/HUD controls, toasts, Cmd+Q during a run, display target, horizontal, 50 MP limit, the real Terminal app.
