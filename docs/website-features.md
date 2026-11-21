# CaptureMann: feature list for the website (as of 0.3.6)

Source material for the feature page, grouped by module. Everything here is built and in the app. Items marked
**Preview** are in the app with a Preview label and a Send feedback button. Items marked **New in 0.3.6** ship with
0.3.6 (not released until the tag exists). A short list of things that are **not** in the app is at the end; keep
those off the page or label them "planned".

Platform notes in brackets are real limits; the website can show them as footnotes. Systems: macOS 13+ (Apple Silicon
and Intel), Windows 11 x64, Ubuntu 24.04+ (x86_64 and aarch64). Free, open source (Apache-2.0), no account, no
telemetry.

---

## 1. Screenshots

- **Capture a display, a window or a region.** Drag a region with a magnifier and live pixel coordinates; nudge with
  the arrow keys. [Linux Wayland: windows through the desktop's own picker.]
- **Global shortcuts and a tray or menu-bar icon.** One keystroke from anywhere; rebind every shortcut in Settings.
  [Linux: X11 only.]
- **Delayed capture** of 3, 5 or 10 seconds, for menus and hover states.
- **Copy and save in one step.** The shot goes to the clipboard and into your library (or a folder you choose) at once.
- **Long Shot (scrolling capture)** *(Preview)*: a whole web page, spreadsheet or chat thread as one image.
- **Click Guide** *(Preview)*: click through a task once and get a numbered step-by-step guide with a screenshot of
  each click; edit the steps and export Markdown with images. Agents can start one too.
- **Boards** *(Preview)*: put two or more screenshots side by side with labels (before and after, or a sequence) in
  one picture.
- **Copy text from a screenshot.** Select the text in the image and copy it; runs on your device.
  [Windows: the OCR languages installed in Windows. Linux: the system `tesseract`.]
- **Import pictures.** Bring in PNG and JPEG (phone screenshots, mock-ups) and treat them like your own shots.

## 2. Annotation and editing of screenshots

- **Drawing tools:** arrow, line, rectangle, ellipse, text, highlighter, step badge, crop. Lines and shapes can be
  solid, dashed or dotted.
- **Step badges** with a label, numbers or letters, a start number and drag-to-reorder.
- **Callouts and stamps:** speech-bubble callouts; stamps for check, cross, exclamation, question, "Before" and
  "After".
- **Spotlight:** dim everything except the part that matters.
- **Zoom Lens:** a zoomed copy of a small detail next to it, with a line back to its source.
- **Blur and pixelate** for emails, tokens and faces.
- **Auto-redact:** finds personal data and secrets on the screen (21 kinds, including payment cards, ID numbers,
  health and bank identifiers, credentials) and proposes blurs for you to review. Measured against a published
  benchmark. [Linux: uses the system `tesseract`.]
- **Retype:** change the text in a screenshot without taking it again.
- **Move area:** move or remove a button, icon or block in a screenshot.
- **Blueprint:** replace UI text with neutral bars, so a guide still fits when a label changes.
- **Non-destructive:** your original is kept, undo and redo are unlimited, every change is a revision.
- **Bulk export:** resize, convert, watermark or rename many screenshots at once.

## 3. Quick Capture *(Preview, New in 0.3.6)*

- **One slim bar** (⌥⇧C on macOS, Ctrl+Alt+Shift+C on Windows and Linux; also from the tray or menu-bar icon, the
  sidebar and the Launcher): choose screenshot, video or audio; choose region, window, display or Long Shot; flip the
  options that fit the kind; press Capture or Record.
- **Everything has a key** printed under its icon (1 2 3 for the kind, R W D L for the target, letters for options).
- **Collapses to a compact form** (key O) and remembers it. Stays open until you close it. Drag it where you want it.
  [Linux X11: not draggable.]
- **Presets:** save a setup with a name and optional shortcut. Four starters ship: Bug report, Docs shot, Demo video,
  Meeting. Pick one with P or Ctrl+1 to Ctrl+4.
- **After-capture bar:** after a screenshot you can Edit, Copy, Save, Copy text, extend it as a Long Shot, or record
  exactly that area, each one key away.
- **For agents and scripts:** a preset runs by name through MCP and the CLI.

## 4. Screen recording

- **Record a display, a window or a region** at up to native resolution (4K and 5K), at 30 or 60 fps. No resolution cap.
  [Linux: window recording on X11 only. GNOME records without the share dialog; other desktops ask through the portal.]
- **Pause and resume** with no dead time in the result; a countdown before the start.
- **Always-visible recording indicator** with a one-click stop; the control bar is draggable and collapsible and is
  never in the video.
- **Excluded windows:** password managers and private apps never appear in a recording.
- **Cursor telemetry** recorded separately, so the cursor can be enlarged, smoothed and used for auto-zoom later.
  [Linux: X11 only.]
- **Crash-safe:** a crash or power loss costs at most a couple of seconds; the recording is recovered at the next start.
- **Webcam:** your face in a floating bubble while you record, or the camera as its own track. Choose the camera, see a
  preview, mirror it. [Linux: camera track, no live bubble.]
- **Audio sources:** microphone with a level meter and noise suppression, and system audio, each saved as its own
  track and mixed on export. Mute the mic while recording.

## 5. Video editing

- **Trim, cut and split**, and **per-segment speed** (speed up installs and loading).
- **Layout:** frame the screen on a background with padding and rounded corners; the webcam bubble can change per
  segment, move, resize, reshape, hide, or go camera-only for an intro, all after recording.
- **Cursor and auto-zoom:** a larger, smoother cursor with click highlights, and zoom regions that follow your clicks.
- **Reframe to 9:16 and 1:1** for Shorts, TikTok and Reels, with a fixed window, follow-the-cursor or fit.
- **Suggestions:** auto-zoom where you paused or clicked, and long pauses and "um/uh" fillers proposed for removal.
- **Import video and audio:** MP4, MOV, MKV and more (and MP3) become ordinary captures you can edit, transcribe,
  export and publish. The original is left intact unless you choose to move it.
- **Non-destructive:** the recording is never altered; edits live in the project and every change can be undone.

## 6. Audio recording (meetings, interviews, voice notes)

- **Audio-only recording** of the microphone and system audio, with a compact control bar, markers you can drop
  during the recording, and a consent line for recording others.
- **Audio editor** with the waveform, markers, transcript, speakers and report in one view; cut the small talk.
- **Export to M4A or WAV.**
- **Meeting-ready:** transcript, speaker labels and a written report straight from the recording (see 7).

## 7. Local AI: transcripts, speakers, reports

Everything runs on your device. Models download only when you press Download.

- **Transcripts and captions** with Whisper, for screen and audio recordings, including multi-hour recordings
  (read piece by piece as a background job, with progress and partial results). Export SRT, VTT, TXT, or burn captions
  into the video.
- **Languages:** tell the app which languages you speak, so a short clip is not heard as a related language.
  English and Vietnamese are the languages tested most.
- **Caption and transcript editor:** fix words, names and grammar; optional punctuation and paragraphs without changing
  the words.
- **Translation** of transcripts and captions.
- **Speaker labels:** who spoke when, even with everyone on one microphone; rename and merge speakers.
- **Reports by a built-in text model** (no cloud, no account): meeting minutes, interview notes, 1:1 notes and more, with
  decisions, action items and links to the exact moment in the recording.
- **Doubtful text is flagged** as `[unclear]` instead of invented where the guard can tell (see limits).

## 8. Export

- **MP4** at source resolution or a preset; **platform presets** for YouTube, Shorts, TikTok and Reels.
- **GIF and WebM** for a pull request or a docs page.
- **Choose the size and format** and see the expected file size, and the largest it can be, before you export.
- **Runs in the background** with progress; keep editing or record again. A restart does not lose the job.
  [Linux: H.264 with a hardware encoder, otherwise a clearly labelled VP9.]

## 9. Library and storage

- **Library of self-contained capture bundles** in a folder you choose (an external drive or a NAS works), with
  search. Each capture is a folder you can open without the app.
- **Save copies to a folder, drive or NAS** automatically.
- **Google Drive:** auto-upload when you stop, resumable and surviving a restart, a share link copied when it is done,
  and a choice of what to upload (the original, the export, or both).
- **Publish to YouTube** (private by default): title, description, chapters, tags, privacy, thumbnail and playlist.
  You bring your own Google credentials.
  [Linux: Drive and YouTube not run on Linux yet.]

## 10. For AI agents and scripts

- **90 MCP tools** plus resources and prompts, and an identical **`capturemann` command-line tool**: every feature in
  the app is also available to your agent and your scripts, with the same results, undo and confirmations.
- **You stay in control:** anything that records you, publishes or deletes needs an on-screen confirmation; you can set
  any tool to "ask first" in Settings; each agent change is one undo step and appears live.
- **A Claude plugin** is available from the repository.

## 11. Everyday app

- **Launcher:** one hotkey opens a searchable list of everything CaptureMann can do.
- **Permissions made simple:** the app says which permissions it needs and why, and gives a button for each missing one.
- **Diagnostics:** one button copies a checked, redacted text for a bug report.
- **English and Vietnamese interface.**
- **Secrets in the system keychain** (Keychain, Credential Manager, Secret Service).
- **Private by design:** no telemetry, and no network traffic unless you connect a destination or download a model.
- **Install:** DMG on macOS, installer on Windows, `.deb`, tarball and an APT repository on Linux.

---

## Honest limits (do not hide these on the page)

- **Preview features:** Long Shot, Click Guide, Boards, Quick Capture. They work but are young.
- **Windows files are unsigned** (SmartScreen may warn). macOS builds are signed and notarized.
- **Not tested yet:** Intel Mac hardware, Windows 10, mixed-DPI monitors, NVIDIA and AMD encoders, KDE.
- **Invented words:** over very quiet audio with only room noise, Whisper can still produce a made-up sentence; the guard
  catches most cases but not all.
- **Long Shot** can lose its first scroll step on Windows (it recovers).

## Not in the app (planned or out of scope; leave off the page or mark "planned")

- Instagram, LinkedIn, X, TikTok and Facebook publishing; scheduled publishing; OneDrive and Dropbox.
- Drawing on screen while recording; camera background blur; tags and folders in the library; live transcript while
  recording; AI-written titles, descriptions and chapters.
- Cloud AI providers (the text model is the built-in local one).
