# QA 0.3.4 B2 and B3 recording HUD and webcam bubble drag on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine:** ThinkPad T490, Intel UHD 620 and its built-in camera, Windows 11 Pro 10.0.26200, 1920x1080 at 150 %
- **Build:** main `26a05100` release package built on the laptop (hashes in `QA-0.3.4-cap-6-long-shot-windows-2026-11-15-claude.md`)
- **Method:** real recordings started with `capturemann record start` (the confirmation answered on screen), windows found with `EnumWindows`, `SetCursorPos` plus `mouse_event` presses and moves (a real pointer drag, 10 to 15 steps), the window rectangle read with `GetWindowRect` before and after. The HUD and the bubble are excluded from capture: nothing here was seen, only measured. Recordings were stopped and deleted.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Webcam bubble drag (REC-5.AC18, CAM-3), screen + camera recording, three drags of (+140,-60) from the bubble's centre | PASS | bubble 382x440: `25,615` -> `165,555` -> `305,495` -> `445,435`; each drag moved it by exactly the pointer distance, and the bubble stayed one window of the same size |
| Recording HUD drag, audio recording, press on the bar | PASS (moves) | HUD 1293x152: `314,813` -> `314,843` after a drag (-90,-70) from the middle (it moved, then settled at the snap position, AC4), `314,843` -> `305,843` after a second drag |
| HUD drag at other points | PARTIAL | the HUD window is taller than the bar it draws; a press in the transparent part or on a button does not drag. In my sweep of 3 points per mode some presses moved nothing (screen recording: the middle press did not move, the right-hand press moved by -9) and one press at the left end ended the window (a button on the bar; the window rectangle read `0,0 0x0` afterwards). A press on a button must not start a drag (AC18), and none did, but my sweep did not pin each point to bar background. A second attempt that pressed 30 px above the window's lower edge with a 500 px drag did not move the HUD in three tries (probably the transparent strip or a hover hint). |
| `cm-regress` case `desktop.hud_drag` (24 points, hit-tested with `WindowFromPoint`) | see the full-tier report of the final build; PASS in `L1-windows-0.3.4-main-a1bf57dc-regression-full-...` | the case finds a point that belongs to the HUD and requires a sideways move of at least 40 px |
| Drag through the HUD of a screen recording, then continuing the recording | not checked | the recordings were stopped right after (the stop answered `Stopped rec_...`) |

## Acceptance criteria

REC-5.AC18: bubble PASS, HUD PASS on the regression case and on three of my presses, snapping behaviour (AC4) seen (the HUD settles on the bottom edge); presses on buttons did not drag.

## Not run

A second monitor, mixed DPI, the "remembered place" after restarting the app (AC4 memory), Windows 10.

## Verdict

PARTIAL
