# HUD-1 report: recording HUD drag on Windows, before the fix (B2/B3 confirmation)

- **Date:** 2026-11-14
- **Who:** Claude (Track F), run on the maintainer's lab laptop with the maintainer's approval
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, one 1920x1080 display, logged-in session
- **Build:** installed CaptureMann 0.3.3 (`%LOCALAPPDATA%\Programs\CaptureMann`), release
- **FFmpeg:** bundled with the installer (not used by this check)
- **Fixture:** none; real screen recording (720p) and real microphone recording, a private library, config folder and pipe (`C:\lab\trackF-run-*`, deleted afterwards); `start_recording` allowed in that private config only
- **Probe:** `scripts/trackF/trackF_hud_drag.ps1`, run in the desktop session through `scripts/testlab/win-run-in-session.ps1`: injected absolute pointer moves, press, 15 moves of (-90,-50), release; a drag counts only when the window moves sideways by 40 px or more (the HUD also changes height by itself when a hover hint opens, which looked like a move of (0,35) in the first run)

## An earlier probe was wrong

The first runs of this check (the earlier probe of Nov 14 and my first two runs) dragged the **wrong window**: the HUD is the wide, short window (1183 x 152 px for a screen recording, 1293 x 152 for an audio recording, bottom centre); the 682 x 218 window at the bottom right of the screen is a toast or card. Those results are discarded. Everything below drags the 1183 x 152 / 1293 x 152 window, the same one `cm-regress` now selects.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Control: drag the title bar of a WinForms window with the same probe | PASS | window moved by (99,60) for a drag of (100,60): the injected drag works |
| Screen recording: drag the HUD from 24 points (3 rows x 8 columns across the bar, every point confirmed by `WindowFromPoint` to be the HUD window) | FAIL | moved sideways in 0 of 24 drags (run `before4`) |
| Audio recording (`record start --audio`): same 24 points | FAIL | 0 of 24 (run `before7`) |
| A press on the Stop button | PASS | the recording ended: buttons answer clicks on 0.3.3 |
| Double-click and right-click on the bar | not run | nothing to grab before the fix |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| REC-5.AC18 (HUD drags on Windows) | FAIL before the fix | steps 2 and 3 |

## Problems found

- B3 confirmed on the installed 0.3.3: the HUD does not move when dragged on Windows 11, for screen and for audio recordings. Cause as in the plan: GPUI's Windows backend has an empty `start_window_move`, and no `WindowControlArea::Drag` was declared. The 0.2.9 text that said the HUD could be dragged was true on macOS only.
- `cm-regress` case `desktop.hud_drag` (new) FAILS on this install ("no pointer drag on the HUD moved it (24 points tried)"), so it discriminates.

## Verdict

PASS as a reproduction (the bug exists and the probe sees the difference). The fix is checked in `HUD-1-windows-after-fix-2026-11-14-claude.md`.
