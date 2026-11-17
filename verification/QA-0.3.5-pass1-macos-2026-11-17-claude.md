# QA 0.3.5 pass 1, macOS, on the shipped 0.3.4 DMG (PARTIAL)

- **Date:** 2026-11-17. **Who:** Claude (QA agent, pass 1). **Machine:** Apple Silicon Mac (built-in 3024x1964 at scale 2 plus a 4K second display), macOS 26.x, screen unlocked, `caffeinate` on.
- **Build:** release DMG `CaptureMann-v0.3.4-macos-arm64.dmg` from GitHub (`gh release download v0.3.4`), mounted read-only. `codesign`: Developer ID Application (team DJ2ALRFN66), `spctl`: accepted, source Notarized Developer ID. `stapler validate` on the app copy says no ticket is stapled to the app itself (the ticket is on the DMG; expected for a copy, not checked on the DMG). App copy run from a folder in the worktree with its own `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, socket `/tmp/cmqa1.sock`, `CAPTUREMANN_SECRETS=file`. The maintainer's daily app (pid in /Applications) and `target/bundle` were not touched; the daily app was still running at the end.
- **Method:** CLI (`capturemann`, `cm-mcp` over stdio), window captures of my own studio window, a small CGEvent tool for clicks and keys on my window (no accessibility-tree driving: System Events merges two instances with the same bundle id and showed the daily app's window, so I did not use it). Confirm cards of agent requests are not visible to screen capture; the window dims instead. Tab then Return confirmed, Return declined (as already known).
- **Not consented and not done:** microphone, camera, system audio, recording. No real recording was made.
- **Pictures:** `verification/QA-0.3.5-shots/macos-pass1-*.png`, all of CaptureMann's own windows or images I annotated from them, checked by eye.
- **Privacy slip, corrected:** my first helper script captured a wrong window (a Terminal window with a private chat, because a `--window -` substring matched). The capture is in my private library only; I looked at it once, did not store or describe it, deleted the file, and moved the capture to the Trash through the app, then removed it from `~/.Trash`. A leftover import file dialog (from an early Cmd+I of mine) also swallowed my first ~8 clicks and keystrokes until I found it with a window-list probe (Esc closed it); those inputs went to that dialog and to my own window, not to other apps (checked with a window-at-point probe that CaptureMann's two windows were topmost at those points).

## What I ran

| Area | Result | Evidence |
| --- | --- | --- |
| Launch, version, isolation | PASS | v0.3.4 in the sidebar, socket up in about 6 s, library `/tmp/cmqa1/lib` |
| CLI `--version`, `targets`, `library list`, `settings get` | PASS | |
| MCP: `cm-mcp` initialize and `tools/list` | PASS | server `capturemann` 0.3.4, **89 tools** |
| Screenshot of a window by id (`capture screenshot --window`) | PASS (seen) | 2360x1586, `--out` wrote the PNG |
| Tray, dock tiles, global shortcuts, region capture | NOT RUN | region needs the overlay (no accessibility tree); global chords not tested (see Shortcuts) |
| Library: counts, chips, sort row, search by typing, card open | PASS (seen) | chips Imported 2, Boards 1, Click Guides 1 after my imports; totals matched Screenshots 19 + Click Guides 1 |
| Library: delete with confirmation | PASS | `library rm` waited for an on-screen card; Return declined (exit 3, `user_declined`); Tab then Return moved the capture to the Trash |
| Library: rename, hover tick, selection bar, Esc | NOT RUN | pointer-state looks not driven this pass (earlier passes covered them) |
| Annotation ops (arrow, line, rect, ellipse, text with Vietnamese, step 1 and step B, blur, callout, stamp check, crop) in one call | PASS (seen) | `macos-pass1-annotate-ops.png`; all drawn; auto text colour returned |
| Undo and redo of that call | PASS | revisions 2 and 3, `can_undo` and `can_redo` correct |
| Base edits: Retype (Vietnamese), Move area, Zoom Lens (circle, x2.5, connector, badge), Spotlight (dim) | PASS (seen) | `macos-pass1-base-edits.png`; diacritics complete; the leftover word "eneral" is my box being narrower than the heading, not a defect |
| Ordering and limits: base edit after another op, 5 lenses | PASS | `invalid_input` naming `ops[1]` and "at most 4 zoom lenses" |
| Blueprint: `annotate --suggest-blueprint` | PASS (suggest only) | 49 lines, Apple Vision, fills and colours; not applied this pass |
| Copy text (`text`) from a screenshot | PASS | plain text in reading order, includes the callout text I drew |
| Image import (JPEG and PNG) | PASS | two captures, job `done` |
| Import of a text file named `.png` | FAIL (polish) | see F1 |
| Board of two images (`board --layout pair --before-after`) | PASS (job result, 4744x1868; not looked at) | |
| Bulk export (3 shots, longest 800, JPEG, watermark DRAFT) | PASS (files) | 3 JPEG, width 800 checked with `sips` (800x545); watermark not looked at |
| Long Shot (`capture screenshot --long`) on my own Settings page, confirmed with Tab+Return | PASS (seen) | 2 frames, `reached_end`, 2360x1733, fixed sidebar not repeated, full page including the last toggle; the page was left scrolled about 150 px afterwards (known: restore scroll, D13) |
| Click Guide on my own window: start, confirm, 3 clicks, stop | PASS | 3 steps, click points exact in the stored picture's coordinates, titles from on-screen text; HUD and toast windows opened and closed (not capturable) |
| Click Guide exports: html, markdown (folder), pdf, second html | PASS | second html became `... (2).html` with `renamed: true` |
| Click Guide on a local web page | NOT RUN | needs my pointer to drive a browser that is the maintainer's; the guide on my own window stands in for it |
| Long Shot on a local web page | NOT RUN | same; one Long Shot of the app's own scrolling page was run |
| Settings > About | PASS (seen) | `macos-pass1-about.png`: Version 0.3.4, "a CraftMann app", licence text, buttons **craftmann.app** and **Source on GitHub** (the brief calls them Website and Source; there is no Send feedback button yet: the 0.3.5 item), Diagnostics card with Show diagnostics and Reveal log folder. Opening the two links was not run (no browser launch) |
| Settings > Shortcuts page | PASS (seen) | `macos-pass1-shortcuts.png`: Long Shot shows "Not set" with a Set button (fresh config); Reset to defaults present |
| Shortcuts conflict (Capture window changed to the chord of Capture region) | NOT RUN | after Change, my synthetic Option+Shift+1 was not taken (row stayed "Press the new shortcut", twice); Esc cancelled. Probably my tool, not the app: no conclusion |
| Vietnamese UI via the Settings language switch | PASS (seen) | `macos-pass1-library-vi.png` and `macos-pass1-editor-vi.png`: sidebar, chips (Đã nhập, Đã tải lên, Đã đăng, Cần chú ý, Bảng ảnh, Hướng dẫn thao tác), sort row, date heading "Hôm nay", editor header "Lưu thành…", "Sao chép ảnh", status "5 chú thích, chưa chọn mục nào" all complete. `settings set appearance.language` correctly refuses (`app_only_setting`) |
| Editor window opens for an annotated capture; layers count | PASS (seen) | 5 annotations, "Đã lưu mọi thay đổi" |
| Editor tools by pointer (arrow, text, step, blur, crop, undo/redo buttons, Retype, Move area, Zoom Lens, Blueprint panels) | NOT RUN by pointer | done through the op API above instead; the editor canvas has no accessibility tree |
| `cm-regress` desktop probes | NOT RUN | the 0.3.4 DMG has no `cm-regress`; the maintainer already stored `L1-macos-0.3.4-*` reports |

## Findings

| Id | Class | Finding | Steps | Expected | Seen | Reproduced | Known? |
| --- | --- | --- | --- | --- | --- | --- | --- |
| F1 | polish | A corrupt image file with an image extension is reported as a video | `capturemann import bad.png` where `bad.png` holds the text `not-an-image` | A message such as "this is not a valid PNG" | `invalid_input`: "The video codec Png is not supported yet.", `kind: video`, size 0x0 (ffprobe fallback) | once | not in the known list |
| F2 | polish | An absurd rectangle is accepted without a warning | `annotate --ops '[{"op":"rect","rect":[10,10,5000000,50]}]'` | `invalid_input` or a clip warning | Accepted, drawn as a strip across the picture (revision stored) | once | not in the known list |
| F3 | polish | A Long Shot of the app's own page leaves the page scrolled about 150 px | Long Shot on my studio window while it shows Settings > General, then look at the window | Scroll position restored | Page stayed scrolled | once | yes (restore scroll, D13) |
| F4 | polish | The window-capture helper accepts a bare `-` as a title and picks an unrelated window | `capture screenshot --window -` | `not_found` or ambiguous | Captured the first window whose title contains "-" (a private Terminal), no ambiguity warning | once | not in the known list |
| F5 | polish | Vietnamese Click Guide tile label wraps to two lines and the Preview pill sits on the icon | Settings language Tiếng Việt, look at the sidebar tiles (`macos-pass1-library-vi.png`) | One line or room for the pill | "Hướng dẫn thao tác" on two lines, "Xem trước" touching the icon | seen once this pass (same as CG-11 style items) | related to D28 |

No P1 (safety or data loss) finding. The delete confirmation, the Click Guide and Long Shot confirmation and the app-only settings guard all behaved as specified.

## Notes for the plan

- The agent confirm card cannot be seen by screen capture and Return declines it (already proposed; see the earlier pass 7 notes).
- F4 touches the privacy line: an agent or script passing a short substring can capture the wrong private window; consider an `ambiguous_target` error when more than one window matches.

## Skipped and why

Tray and dock tiles, region capture, global shortcuts, the shortcut conflict check, rename and selection looks, every editor tool by pointer, Blueprint applied, Long Shot and Click Guide on a web page, `cm-regress`, a second monitor use, recording (no consent), Intel Mac. The pointer tool is slow and was blocked for most of the session by a file dialog I had opened myself, so the editor work went through the op API.

## Cleanup

My app instance stopped, my `caffeinate` stopped, DMG detached, `/tmp/cmqa1` library, config, exports and the downloaded files removed after the commit; the private capture removed from the Trash.

## Verdict

PARTIAL. 0 P1, 5 polish findings (F1 to F5), none blocking.
