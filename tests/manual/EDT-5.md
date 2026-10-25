# EDT-5 manual script: reframe to 9:16 and 1:1

Covers the reframe view in the window and the exported vertical and square files in a real player.
The compositor's fit-and-center and the sizes are covered by `cargo test -p cm-render -p
cm-media` today; the crop window and Follow cursor are meant to be covered by golden frames and a
cursor-inside-frame test once the feature lands. Nothing is recorded: use the synthetic fixture
from [`M4-GATE.md`](M4-GATE.md), the 60 s copy, which has a moving cursor path:

```sh
cargo run -p cm-daemon --example make_fixture -- --library $CAPTUREMANN_LIBRARY \
  --title "Lesson 60" --secs 60 --no-pause
```

Status of the feature: built (Oct 22, 2026): `capturemann edit reframe ID --aspect 9:16|1:1 --mode
fit|fixed|follow [--focus x,y]` and the editor's reframe card. Without a reframe the presets **fit**
the 16:9 screen by width and centre it. The cursor is drawn on a fresh fixture, as in EDT-4
(`tracks.screen.cursor` is `false`).

1. **Fit today.** `capturemann export ID --preset vertical-1080 --wait`, then
   `--preset square-1080`. Play both. Expected: 1080x1920 and 1080x1080 files
   (`ffprobe -v error -show_entries stream=width,height -of csv=p=0 F`), the whole 16:9 screen
   visible, centred, with the background above and below (or left and right); nothing stretched,
   the digit is not squashed, the camera bubble is inside the frame and out of the bottom 20 % and
   right 12 % of the vertical frame.
2. **Reframe view [needs EDT-5].** Choose the 9:16 preset in the editor. Expected: a reframe view
   with a crop window at 9:16 over the 16:9 preview, three modes (Fixed, Follow cursor, Fit) and a
   window you can drag.
3. **Fixed (AC1) [needs EDT-5].** On a 3840x2160 fixture (`make_fixture --size 3840x2160 --secs
   10`) position the window over the centre and export `vertical-1080`. Expected: a 1080x1920 file
   showing exactly the chosen window at full available resolution (the window is 1215x2160
   source pixels, so it is scaled down, not up); compare an exported frame with the window in the
   preview.
4. **Follow cursor (AC2) [needs EDT-5].** Mode Follow cursor on the 60 s fixture, export. Expected:
   the cursor (drawn with EDT-4, or read from the telemetry) is inside the frame in at least 99 %
   of frames; the window glides with a dead zone, no jitter. Count with the checker the feature
   provides, or watch the file at 0.25 speed.
5. **Per aspect ratio (AC3) [needs EDT-5].** Set reframe for 9:16, then export the 16:9
   `youtube-1080p` preset and compare with an export made before the reframe (`ffmpeg ... -f
   framemd5`, or look): unchanged. Switching to 1:1 keeps its own settings.
6. **Camera bubble in the vertical frame.** With reframe active, the bubble is in the bottom third
   centred by default and inside the safe area (CAM-3 rules).
7. **Source narrower than the window [needs EDT-5].** A 640x360 fixture exported to 1080x1920:
   upscaled, with a quality warning in the export sheet.
8. **Undo and agents [needs EDT-5].** Each reframe change is one Cmd+Z; `set_reframe` via MCP or
   CLI gives one revision.

Pass criteria: step 1 passes today; steps 2 to 8 once built, with AC1 to AC3 as written.

## Privacy

No camera, microphone or screen is used.

## Status of the author's runs

(none yet)
