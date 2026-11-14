# Long Shot: sending a failing pair (opt-in debug dump)

When Long Shot says "Stopped: the next part did not line up" and you think it should not have, the engine can
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
