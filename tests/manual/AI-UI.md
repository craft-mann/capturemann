# AI-UI: the AI page, the notice and the caption editor (by eye)

Covers the desktop UI of AI-1, AI-3 and AI-4 (`specs/features/07-ai.md`, DESIGN §5.11). The UI is
unit tested and tested against a scripted daemon, but nobody has looked at it. Steps marked
**[needs engine]** need the speech engine and the daemon actions (other M5 streams); until they
land the page shows "The AI actions are not available in this build of the daemon." and that is
the expected result of step 1.

**Privacy:** nothing here records your screen, camera or microphone. Nothing downloads unless you
press **Download** and then confirm; if a download starts without that, that is a bug, stop and
report it.

## Setup

Use a copy of the app with its own data so your real settings are not touched:

```sh
export CAPTUREMANN_LIBRARY=/tmp/cm-ai-library CAPTUREMANN_CONFIG_DIR=/tmp/cm-ai-config \
  CAPTUREMANN_SOCKET=/tmp/cm-ai.sock
cargo run -p cm-daemon --example make_fixture -- --library $CAPTUREMANN_LIBRARY
cargo run -p cm-desktop        # or: cargo run -p cm-desktop -- --page ai
```

## Steps

| # | Do | Expect |
| --- | --- | --- |
| 1 | Look at the sidebar. | An **AI** item sits after Agents & CLI. Click it: the page title is "AI". With no engine: a card "Models are not available" with the reason and **Try again**. With the engine: a list of models (tiny, base, small, medium) with size and a hint. |
| 2 | **[needs engine]** With no model installed. | A quiet card "No speech model yet" with **Dismiss**. No dialog, no toast, nothing downloading. |
| 3 | **[needs engine]** Press **Download** on `base`. | The row changes to "Ready to download" with **Download 148 MB** and **Cancel**. Nothing downloads yet (check the model folder is empty). Press **Cancel**: the row is as before. |
| 4 | **[needs engine]** Press Download, then **Download 148 MB**. | A progress bar and "N MB of 148 MB" appear and move. **Cancel** stops it and the row says "Paused at N%" with **Resume**. Resume continues, not from zero. |
| 5 | **[needs engine]** When it finishes. | The row says "In use" (or "Installed" with **Use this model** if another is active); the notice is gone from the AI page, the Recordings header and the recording's Transcribe button. |
| 6 | **[needs engine]** **Import from file** on `small` with a real `ggml-small.bin`. | A file dialog opens; after choosing, the row becomes installed. A wrong file shows the error text under the list. |
| 7 | **[needs engine]** **Remove** an installed model. | The row asks "Remove this model?" with **Remove model** and **Cancel**; Cancel changes nothing. |
| 8 | Click **Dismiss** on the notice, quit and reopen the app. | The AI page card and the Recordings hint stay hidden (remembered). |
| 9 | Open **Recordings**. With no model and the notice not dismissed. | A one-line hint above the grid with **Open AI** and **Dismiss**. |
| 10 | Open the fixture's details card. | A "Transcript" row with **Transcribe**. With no model it is disabled, and the text "Transcribing needs a speech model. Add one on the AI page." with **Open AI** stays visible even after Dismiss. |
| 11 | **[needs engine]** With a model, press **Transcribe**. | The button shows "Transcribing…" (with a percentage when reported); a "Transcript ready" toast appears at the end. |
| 12 | **[needs engine]** **Edit** the recording and scroll the inspector to **Captions**. | A list of lines with a mono timestamp each. Click a line: the playhead jumps to its start and the line opens a text field. Type: the text wraps at 42 characters over at most 2 rows; a third row or a line outside 1 to 7 s shows a "!" and a message, and nothing is cut. |
| 13 | **[needs engine]** **Split**, **Merge with next**, **Remove**, **Add at playhead** (put the playhead in a gap). | Each does what it says; **Add at playhead** inside a caption says to move to a gap. |
| 14 | **[needs engine]** Edit a line, then **Save changes**, close and reopen the editor. | The edit is still there. **Save SRT** and **Save VTT** each end with a "Captions saved" toast naming the file; open the file and check the timings. |
| 15 | **[needs engine]** **Suggest zooms**, then **Accept** on one row and **Accept all**. | Rows like `0:12 · 2.0× · after a click`; **Accept** adds one zoom to the zoom lane, **Accept all** the rest, **✕** drops a row only. One Cmd+Z undoes one accept. |
| 16 | **[needs engine]** **Suggest cuts**, then **Accept all**. | Rows like `0:10 · 1.4 s · silence` and `0:03 · 0.4 s · “um”`; Accept all makes the cuts as **one** undo step (Cmd+Z restores them all). |
| 16a | **[needs engine]** *(M2f, 0.2.7)* Open a recording that has pauses and a transcript in the editor; do not press anything. Then do the same with a recording that has **no** transcript. | Within a few seconds the **Suggestions** tab lists pauses and filler words by itself (it never opens by itself, and nothing is cut: the edit list and Cmd+Z history are unchanged). Without a transcript only pauses are listed and the tab says "Looked for silences only: filler words need a transcript". Close the editor while it is still looking: nothing appears later and the app stays idle. Reopen the same recording without editing: the list is back at once. Accept one cut, close, reopen: it looks again (new revision). Audio recordings do the same. |
| 16b | Open the **AI** page, switch **Suggest cuts when the editor opens** off, open a recording. Switch it on again. | Off: the Suggestions tab stays empty until you press **Suggest cuts**. On: the next opened recording looks by itself. The choice survives a restart (setting `ai.auto_suggest_cuts`, default on; an agent can read it, only the person at the keyboard writes it). |
| 16c | **[needs engine]** *(M2i, 0.2.7)* Transcribe a recording with a quiet stretch and a laugh (a short recording where you whisper, then clap or laugh, then speak). Open the **Captions** tab (a video) and the **Transcript** tab (an audio recording). | Between the lines there are dimmed, plain (not italic) bracketed lines `[unclear]` or `[noise]`, each with a time chip (a click seeks) and, with speaker labels, the speaker. They have no text field: clicking the text does not open an editor, and the playing-word highlight and the find bar never land on them. There is **no Remove button** on them: sounds are not removable in 0.2.7. **Save SRT/VTT/TXT** and the TXT in the Transcript tab footer write the same `[unclear]` / `[noise]` lines. Type words over a sound's span in the Captions tab and Save: the sound line no longer shows. With a translation shown, no sound lines appear. |
| 17 | Open **Settings → AI**. | A card "AI has its own section" with **Open AI**, which opens the AI page. |
| 18 | Switch **Transcribe after each recording** off and on, restart. | The choice is kept. (The default is on at 16 GB of RAM or more.) |

## Report

Store it in `verification/` from the template (`verification/README.md`). Note anything that looks
cramped or wrong (spacing of the model rows, the caption list height, the progress bar).
