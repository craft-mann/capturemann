# QA 0.3.4 annotation ops, local-time titles, Vietnamese, sidebar on Linux X11 (PARTIAL)

- **Date:** 2026-11-16, Claude (agent). Same machine, session and build (main `54a5bc68`).

| Case | Result | Evidence |
| --- | --- | --- |
| Retype, Move area, Blueprint, Zoom Lens in one `annotate` call, then undo and redo | PASS (looked at) | a 700x330 region shot; the call made revision 1, undo gave revision 2 (41,817 bytes, the original), redo gave revision 3 (34,935 bytes, same as the edited picture). The result shows the retyped text ("Savar all"), the moved strip, two grey bars over labels and a x2 lens with badge and connector. Coordinates were arbitrary so the picture overlaps itself. Observation: `weight: 700` on Retype looks regular, not bold (not compared with other systems). |
| Blueprint with one colour for two boxes | refused clearly | `ops[2]: colors must be empty or have one colour per box` |
| `annotate --suggest-blueprint` | PASS | 9 text lines found with tesseract, boxes with fill and bar colour |
| Boards, Bulk export | PASS (cm-regress) | `board.create`, `bulk.export_images` in `L1-linux-0.3.4-54a5bc68-regression-full-x11-2026-11-16-claude.md` |
| Local-time titles | PASS | library titles read "Screenshot 2026-11-16 10.26" to "10.32" at the laptop's local time (UTC+7); the stored creation time is UTC (03:26Z) |
| Sidebar tiles, Agents & CLI status icon | PASS (looked at) | LIBRARY group with counts (All captures, Recordings, Audio, Screenshots, Click Guides); Agents & CLI row with the blue status dot; launcher tiles Region, Window, Display, Long Shot, Click Guide |
| Vietnamese (`appearance.language: vi` written into settings.json, app restarted) | PARTIAL, one clip | the Click Guide tile reads "Hướng dẫn theo thao tác" on two lines and the second line is cut off at the bottom of the tile; "Quay..." and the sidebar "Hướng dẫn theo thao t..." end in an ellipsis. Other labels fit. Titles stay "Screenshot ..." in Vietnamese. Settings and editor not looked at. |
| Window picker | not run | needs UI driving |
| Wayland | not run | |

Verdict: PARTIAL.
