# EXP-2 / EXP-3 manual script: platform presets, GIF and WebM, played by real players

Covers what ffprobe checks cannot: the exports playing in QuickTime, Safari, Chrome and Firefox,
the colours, the loudness by ear, and the GIF looping in a browser. Frame sizes, bitrates, the
upscale refusal, the safe area, the preset file replacement, GIF frame counts, size limits and
WebM codecs are covered by `cargo test -p cm-media -p cm-daemon -p cm-cli` with FFmpeg and a GPU
(EXP-2.AC1, AC3, AC4; EXP-3.AC1, AC2). Nothing is recorded: use the synthetic fixture from
[`M4-GATE.md`](M4-GATE.md), the 60 s copy without a pause (`--secs 60 --no-pause`; a 1920x1080
copy is closer to life: `--size 1920x1080`).

Measurements for an exported file `F`:

```sh
ffprobe -v error -show_entries stream=codec_name,width,height,r_frame_rate,bit_rate,color_space \
  -show_entries format=duration,size -of default=nw=1 "F"
ffmpeg -i "F" -af ebur128=peak=true -f null - 2>&1 | grep -E "I:|Peak:" | tail -2     # loudness
```

1. **Presets (AC1).** For each of `youtube-4k`, `youtube-1440p`, `youtube-1080p`,
   `vertical-1080`, `square-1080`, `source`: `capturemann export ID --preset <id> --wait`
   (on a 1080p fixture `youtube-4k` is refused: see step 2). Expected sizes: 3840x2160,
   2560x1440, 1920x1080, 1080x1920, 1080x1080 and the recording's size; the frame rate stays the
   recording's; the bit rate is near the table (`youtube-1080p` about 8 Mbit/s at 30 fps;
   a synthetic pattern may come out lower).
2. **Refusal (AC3).** `capturemann export ID --preset youtube-4k` on a 1080p recording fails
   at once with `invalid_input` and a hint naming `youtube-1440p` or `youtube-1080p`; no job
   starts. The editor's export list shows the preset disabled with the same hint.
3. **Loudness.** On the exports of step 1 (not `source`): integrated loudness **-14 LUFS within
   1 LU** and true peak at most -1 dBTP (-0.5 is the test's allowance after AAC). `source` keeps
   the recorded level. `--loudness -16` overrides: -16 +/- 1.
4. **Colours.** Play the `youtube-1080p` file in **QuickTime Player**, **Safari** and **Chrome**
   next to the editor preview. Expected: the same colours in all (the pattern's red, green and
   blue bars are not shifted toward orange or teal). `ffprobe` shows `color_space=bt709`.
5. **Vertical safe area.** Open `vertical-1080` with the camera on: the bubble is fully inside and
   out of the bottom 20 % and right 12 % of the frame.
6. **GIF (EXP-3.AC1).** `capturemann export ID --format gif --width 800 --fps 15 --wait` on a
   10 s fixture (`--secs 10 --no-pause`). Expected: a GIF of **147 frames, plus or minus 1** (the 10 s fixture exports as 9.8 s from
   `live_at`; 9.8 s x 15 fps)
   (`ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of
   csv=p=0 F`), that **loops forever**: drag it into a Chrome tab and watch it restart; the
   colours are recognisable (a palette of 256, so the gradient bands slightly).
7. **GIF limits.** A clip over 30 s is refused at once with "Use MP4 or WebM for clips longer than
   30 s" (use the 60 s fixture). `--width 500` is refused (480, 640, 800, 1024 only).
8. **Fit under 10 MB (EXP-3.AC2).** `--format gif --max-mb 10` on a 20 s fixture at 1920x1080:
   the file is 10 MB or less (`ls -l`), at a smaller width or frame rate than asked; when it
   cannot fit, the message points to WebM. Same for `--format webm --max-mb 2`.
9. **WebM (EXP-3.AC3).** `capturemann export ID --format webm --wait`. Open it in **Chrome** and
   **Firefox**. Expected: it plays with picture and sound, the beeps and flashes together. If the
   command fails with `platform_rejected` and a hint about libvpx or libopus, your FFmpeg lacks
   them (the bundled one must have both): note it.
10. **Rules file (AC4).** Copy `crates/cm-media/data/presets.toml` to `/tmp/rules.toml`, change
    `youtube-1080p` `mbps_30` to 0.4 (the synthetic pattern needs only about 1.1 Mbit/s, so 4 changes
    nothing visible), run the next export with
    `CAPTUREMANN_PLATFORM_RULES=/tmp/rules.toml` set for the daemon (restart the headless daemon
    or the app with it): the file is about half the size (4.5 MB against 10 MB for the 60 s 1080p fixture), with no rebuild. A broken file is
    ignored with a warning on stderr; exports still work.
11. **Edits apply.** Trim, cut, speed and a background set earlier appear in every preset's file.
    The platform exports are named `<title> - <preset>.mp4` under `exports/` and each export
    replaces its own earlier file.
12. **Cancel.** Start `youtube-4k` on a 4K fixture and cancel it (`capturemann jobs cancel <id>`):
    no partial file stays in `exports/`.

Pass criteria: sizes, loudness, GIF frame count and loop, size limits, WebM in two browsers,
identical colours in the three players. Record the numbers and which players you used.

## Privacy

No camera, microphone or screen is used.

## Status of the author's runs

(none yet)
