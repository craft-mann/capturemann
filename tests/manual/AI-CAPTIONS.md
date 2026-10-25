# AI-CAPTIONS: burned-in captions in the exported video (by eye and ear)

Covers AI-1 burned-in captions (`specs/features/07-ai.md`: Inter 600, white on a 60% black box, the
platform safe area, AC5) and the two switches of `set_caption_options`. The automated tests check
where the caption is drawn and when; they cannot tell whether it is readable, comfortable or in time
with the voice. That is what this script is for.

**Privacy:** it uses a recording you already have (with your own voice on it) and the speech model
you installed on the AI page. Nothing is recorded, uploaded or downloaded. The exports are written
into that recording's own `exports/` folder.

## Setup

```sh
# A recording with speech and a transcript. If it has no transcript yet:
capturemann ai transcribe <id> --wait
capturemann ai transcript <id>          # check the words are about right
```

The recordings in your library are listed by `capturemann library list`.

## Steps

| # | Do | Expect |
| --- | --- | --- |
| 1 | `capturemann ai captions options <id> --burn` then `capturemann export <id> --wait`. | The export finishes. The job result says `captions: burned` (add `--json` to see it). |
| 2 | Open the exported MP4 in QuickTime Player and watch about 30 s with sound. | A caption box (white text, rounded dark box, about 60% black) appears at the bottom while words are spoken and goes away in the pauses. The text matches what you hear. |
| 3 | Look at the timing: does the line appear when the first word is spoken and leave soon after the last one? | Within about 0.3 s of the voice. A line is on screen for at least 1 s. |
| 4 | Look at the type. | Easy to read on both light and dark content. At most two rows. No row is cut off or runs off the frame. Letters look like a medium-heavy sans (Inter, thickened: SemiBold itself is not bundled). |
| 5 | Set a background first: `capturemann edit background <id> --kind color --value "#e8e9ec" --padding 8`, export again. | The caption sits on the screen picture, with a margin above the picture's bottom edge. It never hangs onto the grey background. |
| 6 | If the recording has the camera bubble, look at a caption that is wide. | The caption never covers the bubble: it moves beside it or above it. |
| 7 | `capturemann export <id> --preset vertical-1080 --wait`, open the file. | A 9:16 video. The caption is in the picture area, clear of the bottom 20% (where the TikTok controls go) and of the right 12%. |
| 8 | `capturemann export <id> --preset square-1080 --wait` and `--preset webm --wait`, `--preset gif --wait`. | Each has the caption in the lower part, readable at its size. |
| 9 | `capturemann export <id> --no-captions --wait`. | No caption in the video, although the recording is set to burn them. |
| 10 | Cut a part out: `capturemann edit cut <id> 10-15`, export with `--burn-captions`. | No caption refers to the cut part; the lines after it come earlier by the length of the cut. |
| 11 | A recording with no transcript: `capturemann export <other-id> --burn-captions`. | It fails at once with "has no captions yet; run `transcribe` first". Without `--burn-captions` (and the recording's switch on) the export goes ahead without captions and says `captions: skipped_no_transcript`. |
| 12 | `capturemann ai captions options <id> --hide --no-burn`, then `capturemann undo <id>`. | The first command turns both off; undo turns them back on. Running it twice says "nothing changed". |
| 13 | Edit a caption line in the app's Captions card (or `ai captions set`), export with `--burn-captions`. | The burned-in text is your edited text. |
| 14 | In the app, transcribe a recording that has never had a transcript, then open it with Edit. | The Captions card lists the lines and "Show captions in the preview" is already on. "Include in exported video" is off. The preview shows the caption box while the lines play and nothing in the pauses. |
| 15 | Turn "Show captions in the preview" off, then on. Then press Cmd+Z once. | The preview loses the captions and gets them back at once. Cmd+Z undoes the last switch only and the switch follows it. |
| 16 | Edit a line (Enter saves), play over it. Then cut a part out of the timeline over some lines. | The preview shows your text. Lines inside the cut are no longer shown, and later lines come earlier. |
| 17 | Re-generate the transcript (details page) after turning the preview captions off. | They stay off: the app never changes a choice you made. |
| 18 | On the details page, tick "Burn in captions" next to Export and press "Export again". | The export finishes; its toast says "Captions are burned into the picture." Play it: captions are in the picture. |
| 19 | Untick it and export again. | No captions in the picture and the toast says nothing about captions. |
| 20 | Change "Include in exported video" in the Transcript section, then look at the tick beside Export. | The tick follows the recording's setting again (a tick you set by hand is forgotten). |
| 21 | In the editor's Output card, use "Export MP4" with the "Burn in captions" tick. | Same result as steps 18 and 19. |

## Pass criteria

* Steps 1, 5, 7, 9, 10, 11 and 12 match exactly (they are checked by the automated tests too).
* Steps 2 to 4: captions follow the voice, are readable and never leave the frame.
* Step 6 and 8: nothing important is covered.

## Status of the author's runs

(none yet: fill in from `verification/README.md`'s template)
