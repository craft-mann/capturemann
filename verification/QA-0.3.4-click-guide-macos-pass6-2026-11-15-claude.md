# QA 0.3.4 Click Guide, macOS pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 6). Final dev bundle of main `0bcac6a3`, isolated under `/tmp/cmq6`, Vietnamese UI for the first run.
- **Fixture:** my own AppKit window "QA6 Fixture" (420 x 260 pt, 5 buttons) at 20,388, frontmost. The guide followed that window only (`--window "QA6 Fixture"`), so clicks elsewhere are ignored; all click points were inside the window (checked by its accessibility frame). The first burst of five clicks missed because the fixture was not yet active (status label stayed "Last: none"); a few clicks therefore landed on whatever was at those points, the fixture window being first in the window list.
- Guide started from the CLI (agent path), confirm clicked by script.

| Case | Result | Evidence |
| --- | --- | --- |
| Agent/CLI-started guide | PARTIAL | the confirm dialog (Vietnamese UI) has the title "Cho phép hành động này?" but the body "Start a Click Guide: CaptureMann will take a screenshot of each mouse click (clicks only; keystrokes are never recorded)" and the button "Start guide" are **English** (seen by accessibility text). After Start only a 440 x 96 toast window at 1056,838 (7.1 s, no accessibility text); no bar (known CG-2) |
| First click recorded | PASS | `Click "Open"` after 4 s |
| Fast clicks on the same point, 150 ms apart (99 clicks) | as designed | 50 steps, all `Double-click "Save"`: the daemon merges same-point clicks inside the double-click time (`DOUBLE_CLICK_NS`) |
| 50 clicks alternating two far points, 120 ms apart | PASS | 50 steps, `recorded_steps` equals steps (101 in total) |
| 100-step guide | PASS (seen, counted) | 101 steps |
| 500-step cap | PARTIAL | 400 more alternating clicks: exactly **500** steps, state still `recording`. Six more clicks: still 500, no toast window, no change of state, `guide.error` is published by the daemon (`a guide holds at most 500 steps`) but **nothing in apps/desktop listens to it** (grep): the user gets no message (CG-9). Stop from the CLI then worked (`stopped`, 500 steps) |
| Memory | defect | RSS of the app 0.85 GB at start of the guide, 1.07 GB at 51 steps, 3.76 GB at 500 steps, 2.0 GB after Stop and 2.1 GB with the guide open in the editor. The guide folder is 16 MB on disk. About 6 MB per step held while recording (CG-10) |
| Guide editor with 500 steps (Vietnamese) | PASS (seen) | opens in about 6 s, list, picture with ring and badge, caption "nhấp tại 150, 224 · chụp ngay khi nhấn nút chuột", buttons Sửa ảnh, Chụp lại, Nhân đôi, Xoá, Toàn bộ cửa sổ; picture `QA-0.3.4-shots/macos-pass6-guide-editor-vi.png` |
| Drag-reorder with a real mouse drag | PASS (seen via `guide get`) | step_3 dragged over step_1: order step_3, step_1, step_2 |
| Retake | NOT RUN | |
| Note step in the rich-text paste | NOT RUN | |
| Bar after Stop from the CLI | n/a | there was no bar for the agent-started guide |
| Click Guide from the sidebar tile, bar, countdown | NOT RUN this pass | pass 4 covered the bar |

## Verdict

PARTIAL. 100 steps work, reorder by drag works, the 500 cap holds but is silent, memory grows by several MB per step.
