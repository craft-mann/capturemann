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
