# Manual test scripts

Hands-on checks for what automation cannot see: real screens, devices, accounts and Google's own pages.
Each script lists its setup, privacy notes and pass criteria. After a run, store a report in
[`verification/`](../../verification/README.md) (template in its README); say "verified" only with one.
Automated tests are listed in the [quality report](../../reports/README.md); hardware tests marked `#[ignore]`
are run by hand, see [`CLAUDE.md`](../../CLAUDE.md).

## Capture and annotation

| Script | What it checks |
|---|---|
| [`ANN-2`](ANN-2.md) | Auto-redact (emails and keys) in the annotation editor |
| [`CAP-1-overlay`](CAP-1-overlay.md) | CAP-1 / CAP-3 manual script: the overlay picker (`apps/overlay`, `cm-overlay`) |
| [`CAP-1`](CAP-1.md) | Capture display, window, region (`cm-capture` library side) |
| [`CAP-4`](CAP-4.md) | The clipboard (PNG and TIFF) and "Also save a PNG to a folder" |
| [`CAP-5`](CAP-5.md) | Delayed capture (toolbar choice, tray submenu, countdown pill) |
| [`CAP-EDIT`](CAP-EDIT.md) | The caption editor (redesign R3c) |
| [`CAP-WINDOW`](CAP-WINDOW.md) | The window picker (board R19, screenshot of a window) |

## Recording, webcam and audio

| Script | What it checks |
|---|---|
| [`AUD-1`](AUD-1.md) | Microphone, system audio, noise suppression (`cm-audio`) |
| [`AUD-MUTE`](AUD-MUTE.md) | Mute the microphone while recording (AUD-6) |
| [`CAM-1`](CAM-1.md) | Camera plug and unplug (`camera.connected`, `camera.disconnected`) |
| [`CAM-2`](CAM-2.md) | Camera track, camera/screen sync (`cm-camera`) |
| [`CAM-3`](CAM-3.md) | Edit the camera layout after recording |
| [`DEVICE-CHECK`](DEVICE-CHECK.md) | Settings, Webcam & audio (board R13b, CAM-4.AC4, AC5, AUD-1) |
| [`REC-1`](REC-1.md) | REC-1 / REC-2 / REC-3 / REC-4 / REC-6 / REC-8 manual script: the screen track (`cm-capture` + `cm-encode`) |
| [`REC-5`](REC-5.md) | Recording UI (HUD, countdown, area indicator, webcam bubble, tray, toasts) |
| [`REC-7`](REC-7.md) | Cursor telemetry (`cm-cursor`) |
| [`REC-WINDOW`](REC-WINDOW.md) | Record sheet > Window (board R20) |

## Editing, playback and export

| Script | What it checks |
|---|---|
| [`EDT-1`](EDT-1.md) | Trim, cut, restore, split and undo |
| [`EDT-2`](EDT-2.md) | Per-segment speed, pitch by ear, audio muted above 2x |
| [`EDT-3`](EDT-3.md) | Background, padding, corner radius and shadow |
| [`EDT-4`](EDT-4.md) | Editable cursor and zoom regions |
| [`EDT-5`](EDT-5.md) | Reframe to 9:16 and 1:1 |
| [`EXP-1`](EXP-1.md) | The interim `quick` export (one combined MP4 after recording) |
| [`EXP-2`](EXP-2.md) | EXP-2 / EXP-3 manual script: platform presets, GIF and WebM, played by real players |
| [`M4-GATE`](M4-GATE.md) | M4 gate checklist: editing, compositor and export |
| [`PLAY-1`](PLAY-1.md) | Editor preview playback, scrubbing, sync and sound |

## Library and storage

| Script | What it checks |
|---|---|
| [`LIB-1`](LIB-1.md) | Library card context menu, filter chips and the remote-copy question on Delete |
| [`LIB-2`](LIB-2.md) | The library watcher (bundles changed outside the app) |
| [`LIB-MOVE`](LIB-MOVE.md) | Change the library folder (LIB-2.AC5 to AC13) |
| [`STO-1`](STO-1.md) | Local folder, external drive and NAS destinations |
| [`STO-2`](STO-2.md) | Google Drive (own Desktop-app client) |
| [`STO-3`](STO-3.md) | Automatic upload after recording or export, hold, sleep and Retry |
| [`STO-4`](STO-4.md) | Share link after upload, link modes and Copy link |

## Publishing

| Script | What it checks |
|---|---|
| [`PUB-1`](PUB-1.md) | Connect YouTube with your own Google client, from scratch |
| [`PUB-2`](PUB-2.md) | Publish to YouTube with every field of the sheet |
| [`PUB-CANCEL`](PUB-CANCEL.md) | Cancel for a running publish (Publish page) |

## AI and languages

| Script | What it checks |
|---|---|
| [`AI-CAPTIONS`](AI-CAPTIONS.md) | AI-CAPTIONS: burned-in captions in the exported video (by eye and ear) |
| [`AI-UI`](AI-UI.md) | AI-UI: the AI page, the notice and the caption editor (by eye) |
| [`I18N-IME`](I18N-IME.md) | I18N-IME: input-method composition in the text fields |

## Voice and meetings (0.2.0)

| Script | What it checks |
|---|---|
| [`VOC-1`](VOC-1.md) | Audio recording: sheet, HUD, markers, consent line, kill -9, 3-hour soak |
| [`VOC-2`](VOC-2.md) | The audio editor and Export audio |
| [`AI-5`](AI-5.md) | Translation |
| [`AI-7`](AI-7.md) | The built-in text model on the AI page |
| [`AI-8`](AI-8.md) | Speaker labels |
| [`AI-9`](AI-9.md) | Reports, the report view and the review set |
| [`FIRST-RUN-0.2`](FIRST-RUN-0.2.md) | A clean Mac, no models: install, record, download, report, speakers, translate, remove |
| [`UPGRADE-0.2`](UPGRADE-0.2.md) | A 0.1.0 library opened by 0.2.0 (index version 2 to 3, bundles untouched) |

## Agents, permissions and settings

| Script | What it checks |
|---|---|
| [`AGT-POLICY`](AGT-POLICY.md) | Per-tool permissions (Allow / Ask first) |
| [`PRM-1`](PRM-1.md) | Permissions |
| [`SET-RESET`](SET-RESET.md) | Settings -> Shortcuts "Reset to defaults" (board R16) |
