# Demo video script: CaptureMann 0.1.0

Target length 90 to 120 seconds, 1080p or higher, one take of narration over screen captures. Plain
tone: say what the app does and what you see, no superlatives. Only features that exist in 0.1.0 are
shown. Record the demo with CaptureMann itself where you can; the agent part is a terminal.

Before filming, check each step once in the real build you will ship. Several UI parts are tested
with fakes and not yet confirmed by eye (see the README status line); if a step misbehaves, cut it
from the video instead of working around it.

## Prepare the Mac

- Apple Silicon Mac, macOS 13 or later, the release DMG installed in Applications (not a debug
  build), a new user account or a clean library: Settings, Library, point it at an empty folder.
- Permissions granted beforehand: Screen Recording (then restart the app), Microphone, Camera. Input
  Monitoring only if you want click effects.
- Display at 1920x1080 or 2560x1440 scaled; hide desktop icons; Do Not Disturb on; menu bar clean.
- Two or three plain app windows to record: for example a code editor with a short file and a
  browser showing a documentation page. No personal data, no real accounts visible.
- A Whisper model already downloaded (AI page, `small` is a good choice) so the demo does not wait
  for a download. Downloads happen only on your click; show that in the narration, not on screen.
- YouTube connected through your own Google client ([google-setup.md](google-setup.md)) on a test
  channel, with a playlist optional. Publishing is private by default; keep it private in the video.
- A terminal with a large font, a light theme, and `capturemann` on the PATH:
  `export PATH="/Applications/CaptureMann.app/Contents/MacOS:$PATH"`.
- An MCP-capable agent client configured with
  `{ "mcpServers": { "capturemann": { "command": "/Applications/CaptureMann.app/Contents/MacOS/capturemann", "args": ["mcp"] } } }`.
  Rehearse the agent prompt; the confirmation dialog appears in the app, so keep the app visible.
- Microphone for narration: record the voice separately, or record it in CaptureMann and cut.

## Shot list

| # | Time | Shot | On-screen text |
|---|---|---|---|
| 1 | 0:00-0:08 | Desktop with the menu-bar icon; open the menu | CaptureMann 0.1.0 |
| 2 | 0:08-0:20 | Window picker: press ⌥⇧2, hover a window, click it; the screenshot opens in the editor, add an arrow and blur | Screenshot of a window, ⌥⇧2 |
| 3 | 0:20-0:40 | Press ⌥⇧R: the start sheet. Choose the window, microphone on, webcam bubble on; Record; countdown; bubble moved to a corner; type and click for a few seconds | Record a window with a webcam bubble |
| 4 | 0:40-0:48 | Press ⌥⇧P to pause, then resume, then ⌥⇧R to stop | Pause and resume, ⌥⇧P |
| 5 | 0:48-1:10 | Library: open the recording. Editor: drag the trim handles, add a zoom region on the click, play the preview | Trim and zoom; edits never change the original |
| 6 | 1:10-1:25 | AI page: Transcribe, captions appear; fix one word in the caption editor, turn on burned-in captions | Captions with Whisper, on this Mac |
| 7 | 1:25-1:35 | Export: choose the YouTube 1080p preset; progress; Reveal in Finder | Export to MP4, YouTube, vertical, GIF |
| 8 | 1:35-1:45 | Publish: title, description, privacy Private; the confirmation dialog; confirm; the library shows Published | Publish to YouTube, private by default |
| 9 | 1:45-2:00 | Terminal: the agent records, exports and publishes (see below); the confirmation dialog in the app | Every feature is also a CLI command and an MCP tool |

If you are over 120 seconds, drop shot 8 (publish) or shorten shot 2.

## Narration

Read at an even pace; each line goes with the shot of the same number.

1. "CaptureMann is a free, open-source screen recorder for macOS. It runs in the menu bar."
2. "Press Option Shift 2 to pick a window. The screenshot opens in the editor, where you can annotate
   it and blur sensitive text."
3. "For a recording, open the start sheet, choose what to capture, and turn on the microphone and the
   webcam bubble. The bubble is not baked into the screen video, so you can change it later."
4. "You can pause and resume. If the app crashes, at most the last two seconds are lost."
5. "In the editor you can trim, cut, change speed, and add zoom regions. The original files are
   never modified, and every change can be undone."
6. "Captions are made on your Mac with Whisper. The model downloads only when you ask. You can fix
   words, and burn the captions into the video."
7. "Export to MP4 with presets for YouTube, vertical and square video, or to GIF and WebM."
8. "Publishing to YouTube asks for your confirmation every time, and the video is private by
   default."
9. "Everything the app does is also available to scripts and AI agents, through a command line and
   an MCP server. Recording, publishing and deleting still need you to confirm in the app."
10. (Closing, over the library) "CaptureMann is open source under Apache 2.0. Version 0.1.0 is for
    Apple Silicon Macs with macOS 13 or later. Find it on GitHub: craft-mann slash capturemann."

## Exact steps in the app

These are the steps as the app works today (see [user-guide.md](user-guide.md)).

1. Menu-bar icon: click it to show the menu (shot 1).
2. ⌥⇧2 opens the window picker. Click a window. The capture is saved to the library and opens in
   the annotation editor (arrow, rectangle, text, blur; Auto-redact if the page has an email).
3. ⌥⇧R opens the start sheet: pick the window, microphone, webcam bubble on, resolution and 30 or
   60 fps; press Record. A countdown runs, then the recording HUD shows the time. Drag the bubble
   to a corner; its toolbar (size, shape, mirror, camera off) appears above it.
4. ⌥⇧P pauses and resumes. ⌥⇧R stops. The recording appears in the library.
5. Open the recording. Trim with the handles on the timeline, add a zoom region on a click (or
   accept a suggestion on the AI page), press Play.
6. AI page: choose the installed model, Transcribe. In the caption editor edit a line; use the show
   and burn-in switches for captions.
7. Export: choose the YouTube 1080p preset; wait for the job; reveal the file.
8. Publish (from the recording; check where the button is in the build you film): fill title and description, leave privacy on private, confirm the
   dialog that shows exactly what will be sent.
9. Terminal and agent, for example:
   ```sh
   capturemann devices
   capturemann record start --display 1
   # wait 10 seconds, do something visible
   capturemann record stop
   capturemann library list
   capturemann ai transcribe <id>
   capturemann export <id> --preset youtube-1080p --wait
   ```
   `record start` asks for confirmation in the app; confirm it there.
   Then show the same through MCP: ask the agent "record my screen for 10 seconds, export it for
   YouTube 1080p, and tell me the file path". The agent calls `start_recording`, `stop_recording` and
   `export_video`; the confirmation dialog appears in the app.

Command names and flags are in [`specs/02-agent-interface.md`](../specs/02-agent-interface.md); run
`capturemann --help` before filming and use what it prints if it differs from this page.

## After filming

- Check frames for personal data (notifications, file names, account names, the Google consent
  screen) and blur or cut.
- Add the on-screen text from the table in the editor of your choice; keep it to short phrases.
- Captions: export the narration as SRT with CaptureMann itself and upload the file with the video.
- Do not claim anything the video does not show. In particular, do not say it is notarized unless
  the release notes say so.
