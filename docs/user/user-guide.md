# CaptureMann user guide

CaptureMann lives in the menu bar. Everything below is also available from the command line and to
AI agents (see [Agents and the CLI](#agents-and-the-cli)).

## First run

On first launch CaptureMann opens a permissions screen. It lists what it can use and has one
button per permission; nothing is requested until you press one.

| Permission | Needed for |
|---|---|
| Screen Recording | screenshots of other apps, screen recording, system audio |
| Microphone | microphone track |
| Camera | webcam bubble |
| Input Monitoring | optional: draw-on-screen shortcut and click effects need key and mouse events |

After granting Screen Recording, quit and reopen CaptureMann (the screen has a "Restart
CaptureMann" button).

## Screenshots

| Shortcut | Action |
|---|---|
| ⌥⇧1 | Capture a region |
| ⌥⇧2 | Capture a window |
| ⌥⇧3 | Capture a display |
| ⌥⇧A | Annotate the last capture |
| ⌥⇧Space | Open the launcher (every action in one searchable list) |
| ⌥⇧L | Open the library |

Every screenshot is saved to your library. Open it to annotate: arrows, rectangles, ellipses, text,
pen, highlight and blur, plus **Auto-redact**, which finds emails, keys and tokens, phone numbers
and IP addresses and offers to blur them (you accept or dismiss each one). You can also copy to
the clipboard, or save a PNG to a folder you choose (Settings → Capture).

## Recording

| Shortcut | Action |
|---|---|
| ⌥⇧R | Start or stop recording |
| ⌥⇧P | Pause or resume |
| ⌥⇧D | Discard the take |
| ⌥⇧T | Restart the take |

Press ⌥⇧R (or the Record button) to open the start sheet: choose a display, window or region,
the microphone, system audio, the webcam bubble and the resolution (up to your display's native
size) at 30 or 60 fps. A short countdown follows. The recording HUD shows the time and the
controls; the webcam bubble can be moved and resized, and its toolbar (size, shape, mirror,
camera off) appears above it. The bubble is not part of the screen video, so you can change it
later in the editor.

Recordings are saved in 2 second pieces: if the app or the Mac crashes, at most the last 2 seconds
are lost and the take is recovered on the next launch.

## Editing and exporting

Open a recording from the library. The editor can trim, cut sections, split, change speed, set a
background and padding, add zoom regions (or accept suggestions), restyle the cursor and its click
effects, move or hide the webcam, mix audio levels and reframe to 9:16 or 1:1. Every change is
non-destructive and undoable; the original files are never modified.

**Export** renders one file: MP4 (the default), presets for YouTube (1080p, 1440p, 4K), vertical
and square video, or GIF and WebM. Audio presets normalise loudness to -14 LUFS.

## Captions (local AI)

The AI page downloads a Whisper speech model **only when you press Download** (sizes from 78 MB to
3 GB). Transcription then runs entirely on your Mac. You get a caption editor (split lines, find and
replace, a glossary for names and terms), SRT, VTT and TXT downloads, captions shown in the preview
and optionally burned into the exported video, plus suggestions for zoom regions and for cutting
silences and filler words.

## Recording audio (0.2.0, preview)

Open the Record sheet and switch to **Audio** (or press ⌥⇧V). Pick a preset (In the room, Online
meeting, Lecture or talk, Product demo, Voice note), then the microphone, system audio or both. The
microphone alone does not need the Screen Recording permission; system audio does. The sheet shows
how much disk the recording will use. **Tell everyone they are being recorded**: the sheet reminds
you, and you can switch the reminder off.

The recording bar has Stop, the timer, a level meter, Pause, Mute mic, **Marker** and Discard. Press
**M** (or ⌥⇧M from any app) to mark a moment; type a label if you want one. Markers show in the
editor and in reports. A laptop with the lid closed goes to sleep and the recording stops with what
it had, so keep the lid open. Audio recordings stop by themselves after 4 hours (a setting).
Recordings appear under **Audio** in the library, and a transcript makes them searchable.

## The audio editor and Export audio (0.2.0, preview)

An audio recording opens in its own editor: a card showing who is speaking and the line being said,
lanes for markers, speakers and one waveform per track, and an inspector on the right. Space plays,
**J** and **L** jump back and forward 5 seconds, **K** pauses and **M** adds a marker. Select a range
and press Delete to cut it (R restores); **Suggest cuts** proposes silences and filler words.

**Export audio…** makes an M4A (the default) or a WAV of the recording with your cuts applied: the mix
or one track, loudness evened out for M4A, and an optional copy in a folder you choose. The same
sheet is available as **Export audio only** for screen recordings.

## Translation, speakers and reports (0.2.0, preview)

- **Translate.** In the Transcript tab press **Translate…** and choose a language. English comes from
  Whisper's translate mode and needs the **large-v3** model (download it on the AI page; the fast
  `large-v3-turbo` model cannot translate). Other languages need a text model (below). The original
  stays; switch languages with the language switcher, edit a translation like the original and export
  captions per language.
- **Text model.** On the AI page, choose **Built in**, download the model for your Mac (the size and
  licence are shown first) and press **Test**. The model runs inside CaptureMann and your text stays
  on your own computer. There is no cloud option and no other server. Which model works best is
  still being tested.
- **Reports.** The Report tab writes meeting minutes, interview notes, one-on-one notes, lecture
  notes, a product recap or a plain summary from the transcript. Every item has a time that jumps to
  that moment, so you can check it. Edit the Markdown, regenerate, and export as Markdown or text.
  Interview notes are notes only: CaptureMann does not score people.
- **Speakers.** The Speakers tab will show who spoke when, with names you can change. **This part is not
  available yet:** it needs a speaker model that has not been chosen, and the tab says so.

These features are tested with scripted stand-ins, not yet on real recordings by a reviewer, so
expect rough edges and report them.

## Library, storage and publishing

The library is a folder of capture bundles you can browse, search and filter (Uploaded,
Published). Settings → Destinations adds local folders and Google Drive; a capture can upload
automatically when you stop recording. **Publish** sends an exported recording to YouTube with a
title, description, tags, playlist and thumbnail; it asks for your approval every time and defaults
to not public. See [Connect Google Drive and YouTube](google-setup.md).

## Languages

Settings → General → Language: English or Tiếng Việt. The app changes language only when you pick
one. Want another language? See [`locales/README.md`](../../locales/README.md).

## Agents and the CLI

CaptureMann exposes every action to AI agents over MCP and to scripts through the `capturemann`
command, both inside the app bundle (`CaptureMann.app/Contents/MacOS`).

**Install the command.** With Homebrew it is linked for you. With the DMG, link it once (or call the
full path `/Applications/CaptureMann.app/Contents/MacOS/capturemann`):

```sh
sudo ln -s /Applications/CaptureMann.app/Contents/MacOS/capturemann /usr/local/bin/capturemann
```

The command talks to the running app, so start CaptureMann first.

```sh
capturemann library list                 # the library
capturemann capture screenshot --window "Visual Studio Code"
capturemann export <id> --preset youtube-1080p --wait
capturemann mcp                          # MCP server on stdio (add --http for local HTTP)
```

Free actions run immediately. Actions that record, publish or delete need your confirmation in the
app, and an agent cannot change privacy, shortcut or storage settings. Settings → Agents & CLI shows
connected clients and lets you limit or block each. Details:
[`specs/02-agent-interface.md`](../../specs/02-agent-interface.md).

## Privacy

No telemetry. No network traffic unless you connect a destination or download a model. Your
Google client credentials and sign-ins are stored in the macOS Keychain. See Settings → Privacy
for the full list of what the app does, and <https://www.craftmann.app/capturemann/privacy/> for the
privacy policy.

## Troubleshooting

- **"Screen Recording permission is missing"**: switch CaptureMann off and on in System Settings →
  Privacy & Security → Screen Recording, then restart the app. Builds you compile yourself need a
  stable signing identity, see [macOS permissions](macos-permissions.md).
- **The keychain asks for your password**: choose **Always Allow**; locally built apps ask once
  after each rebuild.
- **No display found**: the screen is locked or asleep. Unlock it and try again.
