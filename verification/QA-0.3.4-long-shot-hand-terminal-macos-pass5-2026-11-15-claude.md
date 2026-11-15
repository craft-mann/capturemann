# QA 0.3.4 Long Shot Hand scroll and Terminal, macOS pass 5 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 5). Dev bundle of main a4f28a07, isolated library under /tmp/cmq5. Mac M4 Pro, macOS 26.6.2.

| Item | Result | Evidence |
| --- | --- | --- |
| long_shot_hand example | built binary exists (target/debug/examples/long_shot_hand, built 14:48) but hung silently for over 60 s with no output; killed. Cause unknown (perhaps a Screen Recording grant for an unsigned example binary) |
| CLI Hand scroll | refused for agents by design |
| Hand scroll in Chrome (wheel, Down key, small steps) | NOT RUN: the overlay bar has no accessibility tree; one blind click through the picker ended the run after 3.8 s with a warning toast I could not read |
| Terminal, first fixture (with `clear`) | one frame, "Stopped: the next part did not line up": the top of the scrollback is about 30 blank lines. A fixture problem, but a user with a cleared terminal gets a one-frame image (LS-8) |
| Terminal, correct fixture (`seq 1 400 | sed 's/^/line /'`, no clear, scrolled to top, 700x640 window, app window moved away) | PASS (measured, sampled): 14 frames, end reached_end, 1288x12260 px, about 30.1 px per line; a sample at 5600 px shows lines 181-233 in order. Full OCR check not done (cm text gave nothing within 120 s) |
| Terminal with the CaptureMann window over it | "Nothing scrolled: the area may be covered by another window" (accurate; the app window comes to the front after Allow) (LS-7) |
| HUD content states | NOT RUN |

Verdict: PARTIAL.
