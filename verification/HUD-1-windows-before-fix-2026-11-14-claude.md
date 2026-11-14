# HUD-1 report: recording HUD drag on Windows, before the fix (B2/B3 confirmation)

- **Date:** 2026-11-14
- **Who:** Claude (Track F), run on the maintainer's lab laptop with the maintainer's approval
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 (10.0.26200), one 1920x1080 display, logged-in session
- **Build:** installed CaptureMann 0.3.3 (`%LOCALAPPDATA%\Programs\CaptureMann`), release
- **FFmpeg:** bundled with the installer (not used by this check)
- **Fixture:** none; real screen recording (720p) and real microphone recording, a private library, config and pipe (`C:\lab\trackF-run-*`, deleted afterwards); `start_recording` allowed in that private config only
- **Probe:** `scripts/trackF/trackF_hud_drag.ps1`, run in the desktop session with `scripts/testlab/win-run-in-session.ps1` (mouse_event absolute moves: press, 15 moves, release)

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Control: drag the title bar of a WinForms window with the same probe | PASS | window moved by (99,60) for a drag of (100,60): the injected drag works |
| Screen recording: drag the HUD (682x218 window at 1225,589) from 15 points (3 rows x 5 columns, every point confirmed by `WindowFromPoint` to be the HUD window), by (-90,-50) | FAIL | moved by (0,0) in 15 of 15 drags |
| Audio recording (`record start --audio`): same 15 drags | FAIL | moved by (0,0) in 15 of 15 drags |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| REC-5.AC18 (HUD drags on Windows) | FAIL before the fix | steps 2 and 3 |

## Problems found

- B3 confirmed: on Windows 11 the HUD does not move when dragged, for screen and for audio recordings. The earlier probe of Nov 14 (coordinates in the window shadow) was inconclusive; this one drags on the bar itself with a control that proves the injected drag moves a normal window. Cause as in the plan: GPUI's Windows backend has an empty `start_window_move`, and no `WindowControlArea::Drag` is declared.

## Verdict

PASS for the check as a reproduction (the bug exists). The 0.2.9 and 0.3.x claim that the HUD can be dragged on Windows was not true.
