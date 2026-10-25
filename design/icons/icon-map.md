# CaptureMann icon map

Generated from the inventory at commit `8618ef8`. One row per element (or group of elements). Icon names are files in `icons/` unless they start with `rec-`, `launcher-` or `Tray`, which already exist in the app.

Priority: **P1** = add now · P2 = leading icon on a text button · keep = already correct · text only = deliberately no icon · planned = for controls not built yet.


## Studio sidebar

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Recordings | `film` | **P1** | 18 px | nav item, leading |
| Screenshots | `image` | **P1** | 18 px | image.svg is already registered but unused |
| Published | `send` | **P1** | 18 px | same glyph as the Published empty state |
| AI & Agents | `sparkles` | **P1** | 18 px |  |
| Settings | `settings` | **P1** | 18 px |  |
| MCP card title (ready / offline) | `bot` | **P1** | 14 px | agent blue when ready, mist when offline; keep the 8 px dot or replace it |
| Recordings hint: notice | `bot` | keep | 16 px |  |
| Recordings hint: Open AI | `sparkles` | P2 | 14 px |  |

## Studio page header

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Display | `monitor` | P2 | 16 px |  |
| Window | `app-window` | P2 | 16 px |  |
| Capture region | `cm-capture-region` | P2 | 16 px | brand crop corners + centre crosshair |
| Record… | `cm-record` | P2 | 16 px | on the rec-red Primary button the glyph is ink |

## Library

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Search field | `search` | keep | 16 px |  |
| Filter: All | `layout-grid` | **P1** | 14 px | chip leading icon |
| Filter: Recordings | `film` | **P1** | 14 px |  |
| Filter: Screenshots | `image` | **P1** | 14 px |  |
| Filter: Uploaded (disabled) | `cloud-upload` | **P1** | 14 px |  |
| Filter: Published (disabled) | `send` | **P1** | 14 px |  |
| Sort: Date / Title / Duration / Size | `calendar` / `arrow-down-a-z` / `clock` / `hard-drive` | P2 | 14 px | optional; the direction matters more |
| Sort direction (replaces text ↓ / ↑) | `arrow-down` / `arrow-up` | **P1** | 14 px | trailing, on the active sort chip |
| Card: duration badge | — | text only |  | keep mono text |
| Card: Broken pill | `triangle-alert` | **P1** | 12 px | replaces the dot inside the pill |
| Card: No preview placeholder | `image` | **P1** | 24 px | film for recordings |
| Card: Unreadable placeholder | `file-x-2` | **P1** | 24 px |  |
| Context menu: Open | `external-link` | **P1** | 16 px | in-app menu (not native), so icons are fine |
| Context menu: Copy link | `link` | **P1** | 16 px |  |
| Context menu: Reveal in Finder / Show in folder | `folder-open` | **P1** | 16 px |  |
| Context menu: Rename… | `pencil` | **P1** | 16 px |  |
| Context menu: Delete… | `trash-2` | **P1** | 16 px | warning colour like the label |
| Delete dialog: Delete capture | `trash-2` | P2 | 16 px |  |
| Banner: Library unavailable | `hard-drive` | **P1** | 18 px | banner leading icon, warning |
| Banner: Screen Recording missing | `shield-alert` | **P1** | 18 px |  |
| Banner: Grant access… / Open System Settings / Restart / Open Permissions… | `shield-check` / `external-link` / `rotate-cw` / `shield-check` | P2 | 16 px |  |
| Banner: CLI and agents offline | `bot` | **P1** | 18 px | agent blue |
| Banner: newest 5,000 shown | `info` | **P1** | 18 px |  |
| Empty: Nothing captured yet | `images` | **P1** | 32 px | empty-state tile like Published |
| Empty: Library unavailable | `hard-drive` | **P1** | 32 px |  |
| Empty: Couldn't load | `circle-alert` | **P1** | 32 px |  |
| Empty: Loading library… | `loader-circle` | **P1** | 20 px | rotate while loading; static with reduced motion |
| Empty: No matches | `search-x` | **P1** | 32 px |  |
| Empty: Try again | `rotate-cw` | P2 | 16 px |  |
| Empty: Clear search | `x` | P2 | 16 px |  |

## Recording details

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Recordings / (crumb) | `chevron-left` | P2 | 14 px | optional back chevron before the crumb |
| Poster placeholder | `video` | keep | 28 px |  |
| Edit | `square-pen` | P2 | 16 px |  |
| Play export / Play in system player / Play original | `play` | P2 | 16 px | rec-play exists; use one play glyph app-wide |
| Export MP4 / Export again | `file-output` | P2 | 16 px |  |
| Publish… | `send` | P2 | 16 px |  |
| Reveal export / Reveal in Finder | `folder-open` | P2 | 16 px |  |
| Burn in captions (switch row) | `captions` | P2 | 16 px |  |
| Exports: Copy link / Open | `link` / `external-link` | P2 | 14 px |  |
| Exports: Export failed / Upload waiting (warning rows) | `triangle-alert` | **P1** | 14 px |  |
| Exports: Uploading / Exporting row | `loader-circle` | **P1** | 14 px |  |
| Exports: Uploaded row | `cloud-check` | **P1** | 14 px |  |
| Transcript: Transcribe | `audio-lines` | P2 | 14 px |  |
| Transcript: Open captions | `captions` | P2 | 14 px |  |
| Transcript: Download TXT / SRT / VTT | `download` | P2 | 14 px | one icon on the group, not each format |
| Transcript: Re-generate | `refresh-cw` | P2 | 14 px |  |
| Transcript: Open AI | `sparkles` | P2 | 14 px |  |

## Published

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Empty state tile | `send` | keep | 24 px |  |
| Privacy pill: Private / Unlisted / Public | `lock` / `link` / `globe` | **P1** | 12 px | replaces the dot; warning tint when privacy is locked |
| Open | `external-link` | P2 | 14 px |  |
| Copy link | `link` | P2 | 14 px |  |
| Status: processing | `loader-circle` | **P1** | 14 px |  |
| Status: processed / published | `circle-check` | **P1** | 14 px |  |

## AI page

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Tab: AI | `sparkles` | **P1** | 16 px |  |
| Tab: Agents & CLI | `bot` | **P1** | 16 px |  |
| Notice: No speech model | `bot` | keep | 20 px | consider sparkles for AI, keep bot for agents |
| Speech model row (identity) | `audio-lines` | **P1** | 18 px |  |
| Download / Download {size} | `download` | P2 | 14 px |  |
| Import from file | `file-input` | P2 | 14 px |  |
| Resume | `play` | P2 | 14 px |  |
| Use this model | `check` | P2 | 14 px |  |
| Remove / Remove model | `trash-2` | P2 | 14 px |  |
| Cancel (download) | `x` | P2 | 14 px |  |
| Status: Download failed | `triangle-alert` | **P1** | 12 px |  |
| Status: In use | `circle-check` | **P1** | 12 px |  |
| Status: Paused at n% | `circle-pause` | **P1** | 12 px |  |
| Auto transcribe switch row | `audio-lines` | P2 | 16 px |  |

## Settings nav

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| General | `sliders-horizontal` | **P1** | 16 px |  |
| Capture & recording | `cm-capture-region` | **P1** | 16 px |  |
| Webcam & audio | `webcam` | **P1** | 16 px |  |
| Destinations | `folder-output` | **P1** | 16 px |  |
| Permissions | `shield-check` | **P1** | 16 px |  |
| Privacy | `eye-off` | **P1** | 16 px |  |
| Shortcuts | `keyboard` | **P1** | 16 px |  |
| About · Apache-2.0 | `info` | **P1** | 16 px |  |

## Settings: General

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Theme: System / Dark / Light | `monitor` / `moon` / `sun` | **P1** | 14 px | icon + label segments |
| Library folder: Available / Unavailable | `circle-check` / `triangle-alert` | **P1** | 14 px |  |

## Settings: Capture & recording

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Resolution, Frame rate, Codec, Quality, Countdown segments | — | text only |  | values read best as text |
| Record the cursor (switch row) | `mouse-pointer-2` | P2 | 16 px |  |
| Create a single MP4 (switch row) | `file-output` | P2 | 16 px |  |
| Countdown rows | `timer` | P2 | 16 px |  |

## Settings: Webcam & audio

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Shape: Circle / Rounded | `circle` / `square` | **P1** | 14 px |  |
| Size S / M / L | — | text only |  | letters stay |
| Mirror the preview | `flip-horizontal-2` | P2 | 16 px |  |
| Remember bubble position | `pin` | P2 | 16 px |  |
| Noise suppression | `audio-lines` | P2 | 16 px |  |

## Settings: Destinations

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Library folder row tile (replaces '~') | `hard-drive` | **P1** | 18 px | inside the 36 px glyph tile |
| Local folder row tile (replaces '~') | `folder` | **P1** | 18 px |  |
| Google Drive / OneDrive / Dropbox tiles | `letters G` / `O` / `D, or official logos` | text only |  | do not redraw third-party logos; use each provider's official asset under its brand rules, or keep letters |
| YouTube / TikTok / Facebook card titles | `official logos or none` | text only |  | same rule |
| Status: Online / Connected | `circle-check` | **P1** | 12 px |  |
| Status: Offline / Reconnect needed | `triangle-alert` | **P1** | 12 px |  |
| Status: Checking | `loader-circle` | **P1** | 12 px |  |
| Default pill | `star` | P2 | 12 px |  |
| Add local folder… | `folder-plus` | P2 | 14 px |  |
| Connect / Connect another / Connect account | `plug` | P2 | 14 px |  |
| Reconnect / Sign in again | `refresh-cw` | P2 | 14 px |  |
| Manage | `settings-2` | P2 | 14 px |  |
| Test connection | `zap` | P2 | 14 px |  |
| Make default | `star` | P2 | 14 px |  |
| Remove… / Disconnect… | `trash-2` / `unplug` | P2 | 14 px |  |
| Continue in browser | `external-link` | P2 | 14 px |  |
| Coming soon caption | `clock` | **P1** | 12 px |  |
| Upload automatically / What to copy / Link after upload segments | — | text only |  |  |

## Settings: Permissions

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Card: Screen Recording | `screen-share` | **P1** | 20 px |  |
| Card: Microphone | `mic` | **P1** | 20 px | rec-mic exists; prefer one mic glyph |
| Card: Camera | `camera` | **P1** | 20 px |  |
| Card: Input Monitoring | `keyboard` | **P1** | 20 px |  |
| State: Granted | `circle-check` | **P1** | 12 px |  |
| State: Not granted / Denied / Restricted / Restart required | `circle-alert` / `ban` / `lock` / `rotate-cw` | **P1** | 12 px |  |
| State: Checking… / Not asked yet | `loader-circle` / `circle-dashed` | **P1** | 12 px |  |
| Grant access… | `shield-check` | P2 | 16 px |  |
| Open System Settings / Open Settings | `external-link` | P2 | 16 px |  |
| Restart CaptureMann | `rotate-cw` | P2 | 16 px |  |
| Reopen setup / Check again | `list-checks` / `refresh-cw` | P2 | 14 px |  |

## Settings: Privacy

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Excluded app: remove | `x` | keep | 14 px |  |
| Add app | `plus` | P2 | 14 px |  |
| Fact: No telemetry | `eye-off` | **P1** | 18 px |  |
| Fact: Local-first | `hard-drive` | **P1** | 18 px |  |
| Fact: No network traffic by default | `wifi-off` | **P1** | 18 px |  |
| Fact: Agents stay on this computer | `laptop` | **P1** | 18 px |  |

## Settings: Shortcuts

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Change | `pencil` | P2 | 14 px |  |
| Couldn't register (warning) | `triangle-alert` | **P1** | 12 px |  |

## Agents & CLI

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| MCP ready / offline pill | `bot` | **P1** | 14 px |  |
| Copy / Copied | `copy` / `check` | P2 | 14 px |  |
| Tier pill: Allow / Ask first | `circle-check` / `hand` | **P1** | 12 px |  |
| Client policy: Allowed / Read-only / Blocked | `circle-check` / `eye` / `ban` | **P1** | 14 px | icon + label segments |
| Add client | `user-plus` | P2 | 14 px |  |
| Allow unknown clients | `circle-check` | P2 | 14 px |  |
| Decision pill: approved / for session / declined / timed out / refused | `check` / `check-check` / `x` / `clock` / `ban` | **P1** | 12 px |  |

## Toasts

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Kind: Info | `info` | **P1** | 18 px | replaces the 8 px dot (keep dot colour as the icon colour) |
| Kind: Success | `circle-check` | **P1** | 18 px |  |
| Kind: Warning | `triangle-alert` | **P1** | 18 px |  |
| Kind: Error | `octagon-alert` | **P1** | 18 px | distinct shape from warning, not only colour |
| Kind: Agent | `bot` | **P1** | 18 px |  |
| Dismiss | `x` | keep | 16 px |  |
| Annotate | `pen-line` | P2 | 16 px |  |
| Copy | `copy` | P2 | 16 px |  |
| Save as… | `save` | P2 | 16 px |  |
| Delete | `trash-2` | P2 | 16 px |  |
| Copy link | `link` | P2 | 16 px |  |
| Open (link) / Open (capture) | `external-link` / `arrow-up-right` | P2 | 16 px |  |
| Play / Reveal | `play` / `folder-open` | P2 | 16 px |  |
| Retry | `rotate-cw` | P2 | 16 px |  |
| Reconnect… | `refresh-cw` | P2 | 16 px |  |
| Open Permissions… / Open System Settings | `shield-check` / `external-link` | P2 | 16 px |  |

## Confirm dialog

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Title icon: Start recording? | `cm-record` | **P1** | 24 px |  |
| Title icon: Publish video? | `send` | **P1** | 24 px |  |
| Title icon: Delete / Discard | `trash-2` | **P1** | 24 px |  |
| Title icon: Allow this action? | `bot` | **P1** | 24 px |  |
| Allow for this session (replaces text ✓) | `rec-check` | keep | 12 px | rec-check.svg is registered but unused today; use it here |

## Screenshot editor

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Tool rail (11 tools) | `mouse-pointer-2, move-up-right, slash, square, circle, type, highlighter, hash, droplet, grid-3x3, crop` | keep | 20 px |  |
| Undo / Redo / Revert / Zoom out / Zoom in | `undo-2` / `redo-2` / `rotate-ccw` / `zoom-out` / `zoom-in` | keep | 18 px |  |
| Fit (replaces maximize) | `shrink` | **P1** | 18 px | maximize is four corner brackets, the same shape as cm-capture-region; shrink also means Fit in the Reframe card |
| Save as… | `save` | P2 | 16 px |  |
| Reveal | `folder-open` | P2 | 16 px |  |
| Copy image | `copy` | P2 | 16 px |  |
| Blur / Pixelate stepper − / + | `minus` / `plus` | **P1** | 16 px | replaces text '-' '+' |
| Reset crop | `crop` | P2 | 16 px |  |
| Duplicate | `copy-plus` | P2 | 16 px |  |
| Delete | `trash-2` | P2 | 16 px |  |
| Save status: saved / saving / couldn't save | `circle-check` / `loader-circle` / `triangle-alert` | **P1** | 12 px |  |
| Back to library (failed screen) | `chevron-left` | P2 | 16 px |  |

## Recording editor

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Recording details (header) | `info` | P2 | 16 px |  |
| Play / Pause | `rec-play` / `rec-pause` | keep | 18 px |  |
| Timeline: In (I) / Out (O) | `cm-trim-in` / `cm-trim-out` | **P1** | 16 px | keep the key letter in the tooltip |
| Timeline: Split (S) | `cm-split` | **P1** | 16 px |  |
| Timeline: Cut (⌫) | `scissors` | **P1** | 16 px |  |
| Timeline: Restore (R) | `archive-restore` | **P1** | 16 px |  |
| Timeline: Undo / Redo | `undo-2` / `redo-2` | **P1** | 16 px | same glyphs as the screenshot editor |
| Timeline: Snap on/off | `magnet` | **P1** | 16 px | toggle; pressed background when on |
| Timeline: zoom − / + | `zoom-out` / `zoom-in` | **P1** | 16 px |  |
| Zoom chip: follow cursor (replaces ⊙) | `locate` | **P1** | 12 px |  |
| Card title: Speed | `gauge` | **P1** | 16 px |  |
| Card title: Camera | `webcam` | **P1** | 16 px |  |
| Card title: Zoom | `zoom-in` | **P1** | 16 px |  |
| Card title: Cursor | `mouse-pointer-2` | **P1** | 16 px |  |
| Card title: Reframe | `ratio` | **P1** | 16 px |  |
| Card title: Background | `wallpaper` | **P1** | 16 px |  |
| Card title: Captions | `captions` | **P1** | 16 px |  |
| Card title: Suggestions | `sparkles` | **P1** | 16 px |  |
| Card title: Output | `file-output` | **P1** | 16 px |  |
| Camera layout: Bubble / Screen only / Camera full / Side by side | `cm-layout-bubble` / `cm-layout-screen` / `cm-layout-camera` / `cm-layout-split` | **P1** | 20 px | icon-over-label segments |
| Bubble position: 4 corners | `cm-corner-top-left` / `-top-right` / `-bottom-left` / `-bottom-right` | **P1** | 18 px | icon-only segments with aria labels |
| Bubble shape: Circle / Rounded | `circle` / `square` | **P1** | 16 px |  |
| Ring / Mirror camera / Smooth move | `circle-dot` / `flip-horizontal-2` / `blend` | P2 | 16 px |  |
| Preview aspect: 16:9 / 9:16 / 1:1 | `rectangle-horizontal` / `rectangle-vertical` / `square` | **P1** | 16 px | icon + label |
| Reframe mode: Fit / Fixed / Follow | `shrink` / `pin` / `locate-fixed` | **P1** | 16 px |  |
| Steppers − / + (zoom, cursor, reframe, background) | `minus` / `plus` | **P1** | 14 px |  |
| Add zoom (Z) | `zoom-in` | P2 | 14 px |  |
| Follow cursor (switch) | `locate` | P2 | 16 px |  |
| Remove zoom | `trash-2` | P2 | 14 px |  |
| Click effect: None / Ring / Ripple | — | text only |  |  |
| Hide when idle | `eye-off` | P2 | 16 px |  |
| Shadow (switch) | `blend` | P2 | 16 px |  |
| Captions: Add at playhead | `plus` | P2 | 14 px |  |
| Captions: Save SRT / VTT / TXT | `download` | P2 | 14 px |  |
| Captions: Save changes | `save` | P2 | 14 px |  |
| Caption line issue (replaces '!') | `circle-alert` | **P1** | 12 px |  |
| Caption line: Split / Merge with next / Remove | `cm-split` / `merge` / `trash-2` | P2 | 14 px |  |
| Suggest zooms / Suggest cuts | `sparkles` | P2 | 14 px |  |
| Accept / Accept all | `check` / `check-check` | P2 | 14 px |  |
| Dismiss suggestion (replaces Unicode ✕) | `x` | **P1** | 14 px |  |
| Export MP4 / Publish… | `file-output` / `send` | P2 | 16 px |  |

## Publish sheet

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Connect YouTube | `plug` | P2 | 16 px |  |
| Quota line | `gauge` | **P1** | 14 px |  |
| Video to upload: file row | `file-video` | **P1** | 18 px |  |
| Export MP4 | `file-output` | P2 | 16 px |  |
| Upload captions (switch) | `captions` | P2 | 16 px |  |
| Visibility: Private / Unlisted / Public | `lock` / `link` / `globe` | **P1** | 16 px | icon + label segments |
| Public confirmation (switch) | `triangle-alert` | P2 | 16 px |  |
| Choose image… | `image-plus` | P2 | 16 px |  |
| Remove thumbnail | `x` | P2 | 16 px |  |
| Publish… | `send` | P2 | 16 px |  |
| Retry / Edit and try again / Open Destinations | `rotate-cw` / `square-pen` / `folder-output` | P2 | 16 px |  |
| Done: Open / Copy link | `external-link` / `link` | P2 | 16 px |  |
| Uploaded headline | `circle-check` | **P1** | 20 px |  |
| Warnings list | `triangle-alert` | **P1** | 14 px |  |

## Record sheet

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Close | `x` | keep | 16 px |  |
| What to record: Display / Window / Region | `monitor` / `app-window` / `cm-capture-region` | **P1** | 18 px | icon-over-label segments |
| Choose a window… / Choose a region… / Choose another… | `mouse-pointer-click` | P2 | 16 px |  |
| Webcam bubble row | `cm-webcam-bubble` | **P1** | 18 px | replaces rec-video here: the row is about the bubble, not the camera |
| Microphone row | `rec-mic` | keep | 18 px |  |
| System audio row | `volume-2` | **P1** | 18 px | FIX: today it borrows rec-play (a play triangle) |
| Retry | `rotate-cw` | P2 | 16 px |  |
| Start recording (replaces ↵ glyph) | `cm-record` | P2 | 16 px | keep the ↵ keycap after the label |

## Recording HUD

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Stop (today a drawn div) | `cm-stop` | **P1** | 18 px | filled; ink on the rec-red button |
| Pause / Resume | `rec-pause` / `rec-play` | keep | 18 px |  |
| Mic on / off | `rec-mic` / `rec-mic-off` | keep | 18 px |  |
| Webcam show / hide | `rec-video` / `rec-video-off` | keep | 18 px |  |
| Draw (coming soon) | `rec-pencil` | keep | 18 px |  |
| Restart | `rec-restart` | keep | 18 px |  |
| Discard | `trash-2` | **P1** | 18 px | x reads as 'close the HUD'; discard deletes the take |
| Chip: Agent · {client} | `bot` | **P1** | 12 px |  |
| Chip: Dropping frames / engine warning | `triangle-alert` | **P1** | 12 px |  |

## Webcam bubble toolbar

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Size S / M / L | — | text only |  | letters stay |
| Shape toggle (today a drawn box) | `circle` / `square` | **P1** | 14 px | shows the shape you'll switch to |
| Mirror (replaces ⇆ glyph) | `flip-horizontal-2` | **P1** | 14 px |  |
| Hide the bubble | `rec-video-off` | keep | 14 px |  |

## Launcher

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Search | `search` | keep | 20 px |  |
| Record screen + webcam | `logo mark` | keep | 16 px |  |
| Record region / Screenshot region | `launcher-frame → cm-capture-region` | keep | 16 px | same drawing; adopt the cm- name when convenient |
| Screenshot window | `launcher-window → app-window` | keep | 16 px |  |
| Screenshot display | `launcher-monitor → monitor` | keep | 16 px |  |
| Open library / Open settings | `launcher-library` / `launcher-settings` | keep | 16 px |  |
| Permissions… (today shares the settings icon) | `shield-check` | **P1** | 16 px |  |
| Publish last recording… | `launcher-upload` | keep | 16 px |  |
| Save to cloud drive | `cloud` | keep | 16 px |  |
| Copy MCP server config | `launcher-terminal` | keep | 16 px |  |
| Actions: Without webcam / Screen + mic only / Screen only | `video-off` / `mic` / `volume-x` | **P1** | 16 px |  |
| Actions: Record at 60 fps / Start in 5 s / With 5 s delay | `gauge` / `timer` / `timer` | **P1** | 16 px |  |
| Actions: Include webcam / Without audio | `cm-webcam-bubble` / `volume-x` | **P1** | 16 px |  |
| Actions: Take screenshot / Save only / Copy and save / Copy only | `cm-capture-region` / `save` / `copy` / `copy` | **P1** | 16 px |  |

## Tray

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Idle | `TrayTemplate (existing)` | keep | 16 px |  |
| Recording (also Starting, Countdown, Stopping) | `tray-recording (existing)` | keep | 16 px |  |
| Paused (new state) | `cm-tray-paused` | **P1** | 16 px | new PNG/ICO in tray/; needs a TrayState::Paused |
| Attention: error or reconnect needed (new state) | `cm-tray-attention` | **P1** | 16 px | new; e.g. upload failed or library offline |
| Tray menu and menu-bar menu items | — | text only |  | native menus stay text-only (macOS convention) |

## Planned

| Element | Icon | Priority | Size | Note |
|---|---|---|---|---|
| Auto-redact | `scan-eye` | planned | 20 px |  |
| Audio gain / mute / solo | `volume-2` / `volume-x` / `headphones` | planned | 16 px |  |
| Loop | `repeat` | planned | 16 px |  |
| Frame step back / forward | `step-back` / `step-forward` | planned | 16 px |  |
| Timeline Fit | `shrink` | planned | 16 px |  |
| Export preset picker | `file-video` | planned | 16 px |  |
| Export Cancel / Retry / Copy path | `x` / `rotate-cw` / `copy` | planned | 16 px |  |
| Library rescan | `refresh-cw` | planned | 16 px |  |
