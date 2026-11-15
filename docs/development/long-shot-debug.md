# Long Shot: sending a failing pair (opt-in debug dump)

When Long Shot says "Stopped where the page jumped" (or its Hand scroll HUD says "Scroll back up a little") and you think it should not have, the engine can
write the two frames it could not join. It is **off by default**, writes only to a folder you name, and nothing
is ever uploaded.

```text
CM_LONG_SHOT_DEBUG_DIR=/tmp/ls-debug open -a CaptureMann      # or export it before cm-desktop / cm-daemon
```

For every frame the stitcher refuses (Auto scroll: the one that ended the run; Hand scroll: each settled frame
that did not join the last one taken), at most 12 per process, the folder gets:

| File | Content |
| --- | --- |
| `refused-NN-last.png` | the last frame that was accepted |
| `refused-NN-refused.png` | the frame that did not join it |
| `refused-NN.txt` | frame sizes, the run kind, and why `find_join` refused (fixed band changed, no shift matches with the closest shift and how many lines differ, several shifts match equally, frame of another size), first with exact lines and then with a few differing pixels allowed |

The pictures hold what was on your screen: look at them before sending them to anyone.

Real run without the app, through the daemon action as the app runs it (Hand scroll, a window or its area):

```text
CM_LONG_SHOT_DEBUG_DIR=/tmp/ls-debug cargo run --release -p cm-daemon --example long_shot_hand -- --window "<title part>" --secs 20 --out /tmp/ls.png [--area]
```

You scroll (or another program does); the example stops after `--secs` the way the HUD's Stop button does. Offline
check of saved frames: `cargo run -p cm-capture --example long_shot_stitch -- <dir of frames> out.png`.

## Offline reproduction of the 0.3.4 QA findings (Nov 15, 2026)

The cases below are reproduced with synthetic frames in `crates/cm-capture/src/long_shot/tests.rs` (no screen, no
input); a dump from the real window is still the better proof, so keep sending them.

| Finding (Windows QA, Nov 15) | Cause found offline | Test |
| --- | --- | --- |
| Horizontal Long Shot of a whole browser window: one frame, `no_overlap` | the tab strip and toolbar are rows that run across the scroll axis and do not move; no column of a frame matches the shifted column of the next. Now cut as static strips (`static_strips`) before the join is looked for | `cap_6_horizontal_whole_window_with_a_static_toolbar_joins` |
| Slow continuous Hand scroll: one frame | not reproduced as such (the engine joins small shifts). The frame is now taken at a third of the area or when the content holds still; a first frame that was not what stayed on screen is rebased | `cap_6_hand_scroll_with_continuous_small_steps_...`, `cap_6_hand_scroll_takes_a_frame_whenever_the_content_holds_still`, `cap_6_hand_scroll_rebases_on_a_first_frame_that_joins_nothing` |
| Windows Terminal, 200 numbered lines, stopped at line 86 | not reproduced with the real frames. Reproduced with text lines that differ in a few pixels each plus a cell that changes every frame: the tolerant look called every shift equal (ambiguous, or "no change" and a false end). Now the shift with the fewest differing pixels per line is chosen when clearly the best, and "no change" needs that no shift explains the difference better | `cap_6_terminal_lines_that_differ_in_a_few_pixels_do_not_stop_the_join` |
| First Auto run ended after one frame | not reproduced; a first frame taken while the screen still changes refuses every join. First frame taken again after 250 ms, first pair retried twice | `cap_6_auto_first_frame_taken_while_the_screen_still_changes`, `cap_6_auto_first_pair_is_retried_before_no_overlap`, `cap_6_auto_scroller_that_ignores_the_first_step` |
| Esc does not stop a run (a CLI run has no HUD) | Esc was not a global key during a run | `cap_6_esc_stops_a_run_that_has_no_request_id_and_keeps_the_frames`, `cap_6_long_shot_state_stop_ends_the_run_and_keeps_the_image` (daemon); `esc_follows_the_daemons_long_shot_events` (desktop) |

None of these has a report on a real screen after the fix, so none is called verified.
