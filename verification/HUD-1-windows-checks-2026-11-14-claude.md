# HUD-1 report: recording HUD and webcam bubble, snap, remembered place, focus and edge drags on Windows (REC-5.AC4, AC16, AC18)

- **Date:** 2026-11-14
- **Who:** Claude (Track F2), on the maintainer's lab laptop with the maintainer's approval
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, one 1920x1080 display at 150 % scale (so the 16 pt edge margin is 24 px), logged-in session
- **Build:** branch `worktree-agent-a6f3fd70218ed0ea4` (source of `a07667de` plus four commits of this track), `cm-desktop.exe` built on the laptop in release mode (LTO off, as in `HUD-1-windows-after-fix-2026-11-14-claude.md`), SHA-256 `9b3fa22924f254a9c59e8562a1f3c531d339958ac5708744cc0a283c4a29f388`, put into a copy of the installed 0.3.3 folder. Not a release build.
- **Probe:** `scripts/trackF2/trackF2_hud_checks.ps1` (modes `hud`, `bubble`), `cm-regress` cases `desktop.hud_click_through` and `desktop.hud_drag`; private library, config and pipe, deleted afterwards. Real screen and camera recordings of a few seconds.

## Defects found and fixed (all Windows-only)

1. Snap, edge pull-back and remembered place did nothing after a drag: the grip is a system title-bar region, so the view never got a mouse-down and the drag state never started. Fixed: a bounds change with the button held starts it (`DragState::observe_move`, unit test).
2. The webcam bubble never moved its own window (`NativeWindow::of` is `None` on Windows): no snap into the area, no first placement. Fixed with `for_frame`.
3. A snap before the button was released could be undone by the system move loop. Fixed: settling waits for the release.
4. The HUD opened about 5 px above the asked rectangle, so a remembered place came back 5 px off. Fixed: set to the rectangle after opening.

## Results on the final build

| Check | Result | Notes |
| --- | --- | --- |
| HUD opening does not take the keyboard from the recorded app | PASS | foreground unchanged |
| A press (drag) on the HUD keeps the recorded app in front | FAIL (by design) | the HUD becomes the foreground window on the first press; REC-5 says a click makes it key. Manual script wording corrected; whether a drag should give the focus back is a product question, not changed |
| Drop 25 px from the right edge snaps to the margin | PASS | right edge 1896 = 1920 - 24 |
| Drop half off the top-left and half off the bottom is pulled back | PASS | at (24,24); bottom 984 above the taskbar (work area bottom 1008) |
| Drag starting at the snapped screen edge | PASS | moved 400 px sideways |
| Collapse after a drag keeps the left edge; expand keeps the right edge | PASS | collapsed bar also snapped to the bottom-right corner |
| Remembered place after quit and restart | PASS | opened at (24,24) where it was left (before the fix 5 px off) |
| Webcam bubble drag by its picture | PASS | moves; stays inside the screen when dropped at the corner |
| Bubble toolbar mirror and size buttons click after a drag, and the bubble drags after the resize | PASS | settings changed; the window is re-opened on a size change |
| Bubble position restored on a CLI-started recording | FAIL / not investigated | `camera.position` is written but a recording started with `capturemann record start --camera` opens bottom-left (CAM-1.AC8 through the start sheet not run) |
| `cm-regress` `desktop.hud_click_through` | PASS | now finds the real HUD (wide, short, topmost) instead of the smallest window (a toast or card) |
| `cm-regress` `desktop.hud_drag` | PASS | |

## Not run

Windows 10, a second display and mixed DPI (the laptop has one display), the audio-recording HUD (the probe has an `-Audio` switch; not run), a screen reader, the display-unplug step 7 of `tests/manual/HUD-1.md`, the bubble's remembered position through the start sheet, macOS.

## Verdict

PARTIAL: the checks above passed on one machine after the four fixes; the foreground behaviour on a press and the CLI-started bubble position are open.
