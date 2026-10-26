# M4 gate checklist: editing, compositor and export

M4 is done when every script below passes on the Mac, a report for each is stored in
`verification/` (see `verification/README.md`), and the automated gate passes. Until then the
statuses in `specs/` stay "In progress".

Nothing in these scripts records your screen, camera or microphone. They use a synthetic recording
(a frame counter with a white flash and a 1 kHz beep at known times, a camera track, microphone
and system audio, one pause, a cursor path) made by `make_fixture`. Your real library is not
touched: the fixture goes into its own folder and the app is started with its own data folders.

## One-time setup

Use these three variables in **every** terminal of the session (the app, the CLI and the fixture
tool), so the test copy has its own library, settings and daemon socket and your daily app is not
involved:

```sh
brew install ffmpeg                       # test fixtures need libx264; the shipped build is checked with
                                          # packaging/ffmpeg/with-bundled-ffmpeg.sh
export CAPTUREMANN_LIBRARY=/tmp/cm-m4-library
export CAPTUREMANN_CONFIG_DIR=/tmp/cm-m4-config
export CAPTUREMANN_SOCKET=/tmp/cm-m4.sock
cargo build -p cm-cli -p cm-daemon        # target/debug/capturemann, target/debug/cm-daemon
cargo run -p cm-daemon --example make_fixture -- --library $CAPTUREMANN_LIBRARY   # prints the id
cargo run -p cm-daemon --example make_fixture -- --library $CAPTUREMANN_LIBRARY \
  --title "Fixture 60s" --secs 60 --no-pause
cargo run -p cm-desktop                   # the editor window, on that library
target/debug/capturemann library list    # in a second terminal with the same variables
```

`make_fixture` flags: `--secs N` (4 to 120, default 12), `--size WxH`, `--fps N`, `--no-camera`,
`--no-audio`, `--no-cursor`, `--no-pause`, `--pause-at S --pause-for S`, `--title T`. Run it again
for another copy (the title gets " (2)" when it is taken). The CLI starts a headless daemon on
that socket when the app is not running (it needs `target/debug/cm-daemon`; set
`CAPTUREMANN_DAEMON` to its path if the CLI cannot find it). When you are done:
`pkill -f cm-m4.sock; rm -rf /tmp/cm-m4-library /tmp/cm-m4-config`.

None of these scripts needs a permission, so `cargo run` is fine; use the signed bundle only if
you want the real signing identity, and never run `make bundle` over `target/bundle` while your
daily app runs from it.

**Fixture notes (see `verification/M4-GATE-2026-10-22.md`):** a fixture with a cursor log
(the default; not with `--no-cursor`) is stored like a "record the cursor separately" recording
(`tracks.screen.cursor` is `false`), so the cursor is drawn and editable with no manual step. Every
export, untouched or edited, starts at the recording's `live_at` (0.58 s for the fixture; since
Oct 22, 2026), so the 12 s fixture exports at 11.8 s whichever renderer runs; `--no-cursor` makes an
untouched fixture use the FFmpeg-graph (`quick`) renderer, the default one the compositor. Edited exports no longer leave `*.mix.part` files behind.

## What the fixture contains (times are the numbers the scripts use)

* The screen file is 12 s of FFmpeg `testsrc` (a colour pattern with a large changing digit). The
  first screen frame is **0.39 s** after the session start; the camera starts at 0.58 s, the
  microphone at 0.10 s. The default in-point is 0.58 s (the moment every track is live).
* A **white flash** on the screen, a flash on the camera and a **beep** on the microphone happen
  together at screen file times 1, 4, 7 and 10 s, each 0.2 s long. In session time (the time the
  CLI and the timeline use) that is **1.39 s, 4.39 s, 8.39 s and 11.39 s**, because of the pause.
* A **pause** of 1.0 s: session 5.89 s to 6.89 s. It is not in any media file.
* System audio is a steady quiet 300 Hz tone (a pitch reference).
* The cursor path is a smooth loop with left clicks at screen file times 2.0, 6.5 and 9.0 s.
* Session length 13.39 s. Every export starts at `live_at` (0.58 s; decided Oct 22, 2026), with the
  pause removed: the 12 s fixture exports at **11.81 s** (the file is 11.80 or 11.81 s: frames are
  1/30 s) on both renderers, and the 60 s no-pause fixture at **59.81 s**. The CLI's "the export is
  now N s" after an edit and the exported file agree to within a frame. `live_at` is measured on
  the recording machine and stored per recording; the fixture's 0.58 s is only its value.

## Run these, in this order

| Script | What it proves | Needs a built piece of |
| --- | --- | --- |
| [`PLAY-1.md`](PLAY-1.md) | the preview plays, scrubs and stays in sync, with sound | M4 as built |
| [`EDT-1.md`](EDT-1.md) | trim, cut, restore, split, undo; the export length | M4 as built |
| [`EDT-2.md`](EDT-2.md) | per-segment speed; pitch by ear; muted above 2x | M4 as built |
| [`EDT-3.md`](EDT-3.md) | background, padding, radius, shadow; presets | M4 as built |
| [`CAM-3.md`](CAM-3.md) | bubble move, resize, shape, layouts; the preview and export agree | CAM-3 (in progress) |
| [`EDT-4.md`](EDT-4.md) | cursor style and zoom regions | EDT-4 (in progress) |
| [`EDT-5.md`](EDT-5.md) | reframe to 9:16 and 1:1 | EDT-5 (in progress) |
| [`EXP-2.md`](EXP-2.md) | the platform presets, GIF and WebM, played by a real player | M4 as built |

Steps that depend on a piece that is not built are marked **[needs CAM-3]**, **[needs EDT-4]** or
**[needs EDT-5]**: skip them and note "not built" in the report until that piece lands.

## The automated gate

```sh
cargo test -p cm-desktop --test parity -- --nocapture                 # must pass
CM_PARITY_KEEP=/tmp/cm-parity cargo test -p cm-desktop --test parity -- --nocapture  # keep PNGs
```

* `m4_gate_export_matches_the_preview_frame_for_frame`: for six edit lists (control, trim,
  cut + speed across the pause, gradient background with padding and radius, bubble, everything),
  the export and the editor's preview are compared on sampled frames (mean error, PSNR, share of
  clearly different pixels, and a frame shift of at most one). It prints SKIPPED without FFmpeg or
  a GPU, so run it on this Mac.
* `m4_gate_bubble_layout_matches`: the same, with the camera bubble included and nothing masked
  (no longer ignored; it passes since the preview follows the stored camera layout).

### Parity numbers

Fill in from a run on the Mac and keep the table in the report (the bundled run below was on an
Apple M4 Pro, VideoToolbox, a 1280x720 fixture, not a stored report):

| Case | Frames | Mean error (of 255) | PSNR dB | Pixels clearly different | Shift |
| --- | --- | --- | --- | --- | --- |
| control (no edit) | 3 | 0.0 to 0.6 | 36 to 99 | 0.5 % | 0 |
| trim | 3 | 0.5 to 0.6 | 36 | 0.4 to 0.6 % | 0 |
| cut + speed across the pause | 5 | 0.5 to 1.0 | 35 to 37 | 0.5 to 0.7 % | 0, one frame at 2x |
| gradient background, 8 % padding, 12 px radius | 3 | 0.3 to 1.0 | 32 to 48 | 0.1 to 1.0 % | 0 |
| bubble (preview and export bubble boxes masked) | 3 | 0.5 to 0.8 | 36 | 0.6 % | 0 |
| everything, bubble masked | 5 | 0.9 to 1.0 | 32 to 33 | 0.8 to 0.9 % | 0 |
| bubble, nothing masked (the ignored test) | 8 | 9.7 to 11.8 | 15 | 8 % | 0 |

The last row is the known gap: the export draws the stored layout (position, ring, mirror) and the
preview draws a fixed bottom-left bubble.

## Also check on a real recording (not the fixture)

The fixture's masters are tagged BT.709. Make one short real recording (the usual
`record_e2e` or the app, 5 s, screen only) and run:

```sh
ffprobe -v error -show_entries stream=color_space,color_range,color_primaries -of default=nw=1 \
  <bundle>/screen.mp4
```

Expected: `color_space=bt709` (or a bt709 tag on the CVPixelBuffer path). If it prints `unknown`,
the preview (FFmpeg's RGBA decode, which assumes BT.601 for untagged video) and the export
(NV12 into the compositor, BT.709) will disagree about colours on real recordings by roughly
7 of 255. Write down what it prints; that decides whether the recorder must write the colour tags.

**Result so far (Oct 23, 2026, metadata only, nothing recorded or modified):** on the maintainer's real
recording "Recording 2026-10-22 09.12" `screen.mp4` is HEVC 3024x1964 with `color_space=bt709`,
`color_range=tv`, `camera.mp4` is H.264 1920x1080 `bt709` `tv`, and the microphone files are
`pcm_f32le` WAV. So the recorder does write the colour tags and preview and export agree on colour for
real recordings; repeat the check after any change to the recorder or the encoder settings.

## Done means

1. Every script above has a stored report with PASS or "not built" per step.
2. `cargo test --workspace --locked` passes, including both parity tests.
3. Statuses in `specs/features/06-editing.md`, `04-webcam.md` (CAM-3), `08-export.md`
   (EXP-2, EXP-3) change from "In progress" to Done only where every acceptance criterion passed.
