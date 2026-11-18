# QA 0.3.6, Windows 11, lab candidate built from `898bc620` (PARTIAL)

- **Date:** 2026-11-18, about 15:55 to 17:50 local (UTC+7). **Who:** Claude (QA agent), driven from the Mac.
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, one internal display 1920x1080 at 150 %, user `trung` logged in on the console. Lab lock: `C:\lab\screen.lock` held for the whole run and removed at the end.
- **Build under test:** a lab candidate, **not the shipping package** (the Windows files come from the GitHub "Windows package" workflow). `git archive` of `898bc620` (version 0.3.6) unpacked to `C:\wt\main036`, `cargo build --release --locked --no-default-features` for `cm-daemon`, `capturemann`, `cm-mcp`, `cm-overlay`, `cm-regress` and `cm-desktop` (GUI subsystem; no speaker engine). Staged in `C:\lab\q36\pkg` with the `ffmpeg.exe` and `ffprobe.exe` of the old 0.2.7 lab package (Nov 4); no `llama-server.exe`, no licence files. SHA-256 are in the regression reports (`capturemann.exe` `099a3014...`, `cm-desktop.exe` `adc4d802...`).
- **`main` moved while I tested.** HEAD is now `c69ef16e`, which holds fixes for findings the macOS pass made (picker keyboard, pseudo-locale layout, `list_presets` row in the spec table, `record start` refusing a screenshot preset before asking). My build does not have them; where a finding below is the same, it says so.
- **Isolation:** the app and every CLI call used `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR` (`C:\lab\q36\env\...`), pipe `\\.\pipe\q36`, `CAPTUREMANN_SECRETS=file`. A second instance for Long Shot (`...\envb`, pipe `q36b`) carried `CM_LONG_SHOT_DEBUG_DIR` and `CM_LONG_SHOT_TRACE_DIR`. Edge ran as a guest profile.
- **Incidents (please read):**
  1. Once I ran `capturemann library list` in a shell where my isolation variables were not yet set. It reached the lab laptop's default library (it started a headless `cm-daemon` with the default settings, which I stopped) and printed about 20 titles, among them `Audio recording 2026-11-17 17.02`, the title of the recording named in the 0.3.6 plan. I read titles only, opened nothing, and copied nothing into this report.
  2. I killed the overlay process by hand once (`Stop-Process cm-overlay`) while a region picker was open; that produced the long error toast described under W-5 and is my doing.
  3. At cleanup I deleted `C:\lab\ls7.ps1`: an older lab script of that name existed and my own `ls7.ps1` overwrote it when I pushed (it is not in the repository, I cannot restore it). I also deleted and then restored `C:\lab\qa-lib.ps1` (identical copy put back).
  4. The macOS agent's commit `22c8a3c4` swept two of my early pictures into the repository (`g-picker.png`, `s-start.png`); I removed the first and renamed the second (`windows-dock-english.png`) in this commit.
- **What I could not see.** The Quick Capture panel, the after-capture bar, the Edit preset sheet, the toasts, the confirm dialogs and the region picker are overlay windows with display affinity `EXCLUDEFROMCAPTURE` (the overlay log says so); `PrintWindow` gives a black picture. So there is **no picture of the panel, the bar, the sheet or the picker**: those were checked through UI Automation (names, state, rectangles) and the log. Pictures of the studio window, the Settings page, the Boards sheet, the dock and the tray menu are in `verification/QA-0.3.6-shots/windows-*.png`.
- **Input limit.** The region picker did not react to injected mouse drags (`mouse_event`, absolute and relative; Esc and a click worked in the Display picker). So no region screenshot was taken through the picker; Region from the panel is **not run**.

## Quick Capture

| Item | Result | What I did and saw |
| --- | --- | --- |
| QC-1 shortcut | PASS | Ctrl+Alt+Shift+C opens a 550x611 panel (UI Automation window "Quick Capture"); a second press closes it; the Launcher chord (Ctrl+Alt+Shift+Space) still opens the Launcher. A freshly started app took the chord only after a moment (once the panel did not open for about 9 s). |
| QC-1 tray item | PASS with finding W-2 | Right click on the tray icon: **Quick Capture** is the first item (seen in Vietnamese, "Chụp nhanh…", picture `windows-tray-menu-vi.png`), then region, window, display, delay submenu, Record, audio, pause, stop, Open library, Open launcher, New Click Guide (Preview), Import, Recent captures, **Presets** submenu ("Mẫu"), Settings, Permissions, Quit. The panel opened, but not under the icon (W-2). |
| QC-1 dock button | PASS | The neutral "Capture…" button with the keycap C under "Record…" opens the panel; the five tiles and Long Shot and Click Guide tiles are as before. Its accessible name is inconsistent (W-6). |
| QC-1 Launcher row | PASS | Typing "quick" lists "Quick Capture, preview, Kind, target and options in one panel, shortcut Ctrl+Alt+Shift+C" with the Preview image; Enter opens the panel. |
| QC-1.AC1/AC3 layout, kinds | PASS (by UI Automation) | Screenshot: Region, Window, Display, Long Shot; tiles Editor, Copy, Auto-redact, Delay; bar "Capture". Video: Region/Window/Display, Mic, System audio, Webcam, Countdown; "Record". Audio: Mic, System audio, Transcribe, Markers, "Record", no target row. Keys 1, 2, 3 switch kinds. The panel opens on the last kind. |
| QC-1.AC6 tiles remembered, Settings untouched | PASS | Delay 5 s and Copy off in the panel were there after Esc and reopen and are stored in `quick_capture.json`; `settings.json` stayed `{"permissions.onboarded": true}`; Settings > General still shows "Copy image to clipboard after capture". |
| QC-1.AC7 keys and names | PASS | Names carry the state ("Copy, on", "Delay, Off", "Region, selected"); the Delay tile opens a menu of Off, 3 s, 5 s, 10 s; Esc in the menu goes back, Esc again closes; P opens the picker, Esc in the picker goes back. (Tab and Up/Down were not checked one by one.) |
| QC-1.AC8 Preview label, Send feedback | PASS (presence) | Image "Preview. New and still changing." and the button "Send feedback" are in the panel. The browser page was not opened. |
| QC-1.AC2 Display with Copy on | PASS | Panel > Display > Enter opens the display picker (the same overlay as the plain Display hotkey, which also waits for a click or Enter); a click selected the display; with Copy on, the clipboard held a 1920x1080 image. |
| QC-3.AC2 toast, AC5 Undo | PASS | Copy on, Editor off: toast "Copied to clipboard" with **Edit** and **Undo**, gone after about 6 s. Undo: library count back to what it was; the clipboard image stays. |
| QC-3.AC1 bar | PASS | Copy and Editor off: bar window 1025x89 at the bottom of the display, toolbar "After capture" with Edit E, Copy C, Save S, Text T, Long Shot L, Record R. |
| QC-3 keys | PASS | **C** copied (1920x1080 clipboard image) and closed the bar; **S** opened Save As (no save folder set) and closed the bar; **T** put the text of the screen on the clipboard ("Copied 144 words", 834 characters); **E** opened the annotation editor; **L** opened the Long Shot HUD; **R** started a recording after a 3 s countdown with the normal HUD (stopped with `record stop`, 15.6 s, screen, cursor and mic tracks). **Esc** closed the bar. |
| QC-3.AC6 bar timing | PASS | With the pointer away the bar closed after about 20 s. (Staying open under the pointer was not timed.) |
| QC-3.AC7 bottom edge | not run | Region picker not drivable; the Display capture's bar sat at y 907 to 996, inside the display. |
| QC-4.AC1 picker fills the panel | PASS | "Bug report" gave Region, Editor on, Copy on, Auto-redact on and the bar "Capture · Bug report"; "Demo video" gave Display, Mic on, Webcam on, Countdown 3 s and "Record · Demo video". Rows lower in the list could not be picked by UI Automation or a click (the row under the click was "Save current as preset", see W-7, same as macOS D1). |
| QC-4.AC2 Save current | PASS | "Save current as preset…", name "QA Preset", Save: the preset is the picked one, the bar reads "Capture · QA Preset", `presets.json` has the same setup. |
| QC-4.AC3 unknown fields kept | PASS | I added `"team_top"` to the file and `"team": "docs"` to a preset by hand; after a save from the panel both were still there. The invalid preset stayed in the file, moved to the end. |
| QC-4.AC4 invalid preset | PASS | A video preset with `"target": "long_shot"` was skipped; `capturemann presets list` printed "Notice: presets.json: skipped 1 preset that is not valid: "Broken video" (a video preset cannot use a Long Shot)" and Settings > Capture & recording > Presets shows the same line in amber under the list. The other presets loaded. |
| QC-4.AC5 shortcut runs the preset | PASS | A preset with `"shortcut": "Ctrl+Alt+Shift+B"` ran from the chord at once (no panel). The shortcut **conflict check** (Change in the sheet) was **not run** because of W-1. |
| QC-4.AC7 Save to | PASS | `"save_to": "C:\lab\q36\bugs\{date}"` (a top-level key of the preset, not under `after`; I got that wrong first): the capture was in the library and as `C:\lab\q36\bugs\2026-11-18\CaptureMann 2026-11-18 at 16.35.49.png` (848 KB, the local date), and the editor opened with the Step badge tool selected. |
| QC-4 Settings page | PASS with finding W-1 | Presets at the top of Capture & recording (picture `windows-settings-presets.png`): Move up, Move down, Edit, Delete per row; New preset. The Edit sheet opens as its own window with name, kind, target, Copy, Auto-redact, Open in editor, the tool, Save to folder, Change, Delete, Cancel, Save preset. |
| QC-4.AC6 starters stay deleted | **not run** | |
| QC-5.AC2 `list_presets` | PASS | `capturemann presets list` and MCP `tools/call list_presets`: id, name, kind, target, options, after, summary, notice. The middle dot of the summary is mangled only in the PowerShell capture. |
| QC-5 tool count | PASS | `tools/list` over stdio from `cm-mcp.exe`: **90 tools**, `list_presets` among them. |
| QC-5.AC1/AC3 preset on capture | PASS | `capture screenshot --display 65537 --preset "Bug report"`: result as the call with the preset's options (display capture, copied to the clipboard); with `--delay 2` it took 2.26 s; `--preset` alone captured the primary display; the same through MCP with `delay_s: 1`. |
| QC-5.AC5 unknown preset | PASS | CLI and MCP: `invalid_input`, `unknown preset "nope"`. A video preset on `capture_screenshot`: `invalid_input` naming the kind; a preset without a window ("Docs shot"): `invalid_input`, "add --window". |
| QC-5.AC4 recording preset confirm | PASS | `record start --preset "Demo video" --display 65537`: the dialog "Start recording?" lists Display 1 1920x1080, 30 fps, microphone and camera on, countdown 3 s, save folder, disk space; Cancel gave `user_declined`. (`record start --preset "Bug report"` printed "Waiting for confirmation... up to 5 minutes" and then refused with `invalid_input`; fixed on main by `3cfd486f`.) |
| QC-1 step 32 Vietnamese | PARTIAL | Dock tile "Hướng dẫn thao tác" wraps on two lines and the Preview pill ("Xem trước") sits clear of the icon (picture `windows-dock-vietnamese.png`, P-9). Panel and picker texts are translated ("Mẫu", "Không dùng mẫu", "Lưu thiết lập hiện tại thành mẫu…"); the panel itself cannot be pictured. Starter preset names stay English. |

## Transcripts (AI-1.AC12, AC12b, AC16), synthetic clips only

Speech: Windows SAPI (System.Speech, one English voice), scaled and mixed over uniform noise, 16 kHz mono. Model: the `ggml-medium.bin` already on the laptop (copied into my private config, nothing downloaded). Import with `--transcribe --language vi --wait`, the guard's language. Each 9 to 12 s clip took 10 to 15 minutes on this CPU.

| Clip | Words | `dropped` / `sounds` | Every word ends after it starts |
| --- | --- | --- | --- |
| `shape9` (2.0 to 3.5 s and 5.5 to 7.8 s of speech at -33 dBFS over -60) | 13, all inside the bursts (2.14 to 7.98 s) | none (no doubtful line) | yes |
| `room9` (room only, 9 s, -60 dBFS) | **15, from 8.59 to 9.00 s: "Hãy subscribe cho kênh Ghiền Mì Gõ Để không bỏ lỡ những video hấp dẫn"**, confidence 0.33 to 1.0 (the last 12 words 0.85 to 1.0) | none | yes |
| `speech12` (continuous speech -30 over -60) | 47 | none | yes |
| `quiet12` (-45 over -55) | 44, six words at 11.94 to 12.00 s | none | yes |

- AC16 holds on all four clips (0 words with an end at or before the start). Words still pile up at the end of a piece (`quiet12`: six words inside 60 ms), which AC16 allows.
- AC12b did not fire anywhere: no `dropped` or `sounds` key in any transcript. The rule's positive side was not exercised by a real invented line, as in the benchmark.
- **W-8 (known, same as macOS D4):** a clip with no speech at all still gets an invented outro sentence with high confidence. 0.3.6 does not change this (the guard cannot judge a recording with no speech reference).

## The fixes

| Item | Result | What I did and saw |
| --- | --- | --- |
| F1 | PASS | `import fake.png` and `fake.jpg` (text files): `invalid_input`, `reason: unreadable_file`, "The file can't be read. It may be damaged, cut short or not a picture." (`kind: image`). `fake.mp4` (control): same reason, "not a media file". |
| F2 | PASS | `annotate` with `rect [10,10,5000000,50]` and an arrow to x 9,000,000 on an 800x600 picture: `invalid_input`, "ops[0]: the shape lies far outside the 800x600 picture; keep it within 6400 px of the picture"; a rect at 700,500 size 300x300 is accepted. |
| F3 | PASS | Auto scroll Long Shot (maximized Edge, local page, YouTube): the result carries `page_offset {moved_px: 15376, restored: true}`; the page was back at the top. |
| P-2 | PASS | Two windows titled "QA Target One" and "QA Target Two": `--window "QA Target"` gave `invalid_input`, `reason: ambiguous_target`, text "matches 2 windows: 851990 "QA Target Two"; 460212 "QA Target One". Pass the window id", and JSON `matches` with id and title only (the app name is dropped, other windows are not listed). |
| P-3 message | PASS | A page with a fixed element changing every 30 ms: `end: no_overlap`, `note: "Stopped: the next part did not line up: the page changed between two parts (moving, loading or covered content)"`, `why`, the 1 frame kept (1557x969), `page_offset {moved_px: 0, restored: true}`. Trace: `QA-0.3.6-windows-longshot-trace-noisy-page.txt`. |
| P-3 Windows cause | **not reproduced, no cause found** | See "Long Shot" below. |
| P-4 | PASS | A 2000x4000 noise PNG (24 MB, 32 MB rendered): `annotate --out` wrote the 32,006,508 byte file; JSON `image: {omitted: true, reason: "too_large"}`, `out` set; no `internal`. |
| P-5 | PASS | `guide export <id> --format markdown --dest C:\lab\q36\outside` outside the library: the dialog row reads `Path: C:\lab\q36\outside` (no `\\?\`); a `..` path shows the resolved `C:\lab\q36\outside\sub`. Cancel gave `user_declined`, `reason: ask_first`. |
| P-6 | PASS | Guide editor at the default window: the rail now draws a thin vertical scroll thumb at its right edge while it overflows (picture `windows-guide-editor-rail.png`; the rail's AI models, Agents and Settings buttons are still below the window edge, 900 to 1128 px against a 1000 px window). The wheel did not move the buttons in my test (injected wheel over the rail at x 200; 0.3.5 scrolled it with the pointer at the same place), so the scrolling itself is not confirmed here. |
| P-7 | PASS | Library, two screenshots selected, Make a board: the sheet and the Boards intro card show; Cancel: both are gone. |
| P-8 | **partly** (W-4) | A Click Guide selected with screenshots: "Make a board" is drawn dimmed, but UI Automation has no Button for it: the element at that spot is an unnamed Text, `IsEnabled: True`. It is no longer an enabled-looking button, but a screen reader says nothing about it. The plan says "reports as disabled". |
| P-9, F5 | PASS | Language Tiếng Việt: the Click Guide tile wraps to two lines ("Hướng dẫn thao / tác"), the tile is taller, the Preview pill ("Xem trước") sits above the icons without overlap, at the default width. |

### Long Shot, P-3 on Windows

All runs: `capture screenshot --window <id> --long --max-px 16384`, Allow clicked through UI Automation, `CM_LONG_SHOT_DEBUG_DIR` and `CM_LONG_SHOT_TRACE_DIR` set on the instance that ran them, Edge guest profile, other CPU load present (the transcripts were running for the first runs).

| Run | Window | Result |
| --- | --- | --- |
| local tall page, one tab | maximized (1920x1008) | 34 frames, `limit_reached`, 90 s |
| local tall page, two tabs (second active) | maximized | 34 frames, `limit_reached`, 90 s |
| YouTube `@Google/videos`, two tabs | maximized | 34 frames, `limit_reached`, 211 s |
| YouTube, one tab (fresh window) | restored (1557x969) | 62 frames, `limit_reached`, 161 s, 12 refused-frame dumps |
| local page with a changing fixed block | restored | 1 frame, `no_overlap` with the plain reason above |

- **The 0.3.5 case (maximized Edge, second tab, YouTube, 2 frames, `no_overlap`) did not happen in these runs.** What differs between a failing and a passing window could therefore not be compared, and I do not claim a cause.
- What the trace shows in every run: the **first step of 302 px (290 px restored) gives `NoOverlap` because the frame did not change** (the scroll had not moved the page yet), "first pair looked at again" twice with the same frame, then "scrolled back by 151 (retry 1) -> Added". The engine recovers. On YouTube more steps need one or two scroll-back retries (restored window: steps 2, 3 and others `NoOverlap`, then `Added`); a page that shifts its layout while loading would exhaust the two retries at a step. The 2-frame stop of 0.3.5 fits "the first step lost, then the second pair refused twice" but the logs of the failing run were never kept. Next step for whoever sees it again: run with the two environment variables set and keep `refused-NN.txt` plus `trace.txt` (the first lines of a passing run are in `QA-0.3.6-windows-longshot-trace-youtube-restored-window.txt`).

## Regression (`cm-regress` against the lab package, `--allow screen`, `screen,camera,mic,sound` for the hardware tier)

| Tier | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard | 52 | 5 | 7 of 64 | `L1-windows-0.3.6-lab-898bc620-regression-standard-2026-11-18-claude.md` (+ `.json`) |
| full | 60 | 6 | 14 of 80 | `...-full-...` |
| hardware | 63 | 6 | 19 of 88 | `...-hardware-...` |

Failures, all repeated in every tier they run in:
- `pkg.files` and `ai.llama_version`: the lab folder has no `llama-server.exe`, licence files or `FFmpeg\` folder. **Lab package, not the product.**
- `import.audio` ("The MP3 file can't be read"): the staged `ffmpeg.exe` is the Nov 4 build, which has no MP3 decoder in its configure line (MP3 import came in 0.3.1). **Lab package.**
- `export.quick` and `export.presets` ("h264_mf export is badly tagged: color_primaries ''"): most likely the same old `ffmpeg.exe` (not proved: I had no current Windows FFmpeg to run the case against). **To be confirmed on the CI package.**
- `mcp.tools_list` ("tools the spec table does not list: list_presets"): the spec row was added after `898bc620` (`91b817cb`); same as macOS D3. Passes on current main in principle.
- The hardware tier ran camera, microphone and the 15 s soak-free hardware cases without a failure of its own; skipped: second monitor, `guide.real_clicks`, external 4K, each monitor, soak, `ai.speakers` (no speaker models), installer cases, Linux and macOS cases.

## Findings (nothing fixed)

Severity: **medium** (a user is blocked or misled), **low** (a user notices), **note**.

| ID | Sev | Finding and steps | Also on |
| --- | --- | --- | --- |
| W-1 | medium | **The Edit preset sheet does not fit a 1080p 150 % screen and its footer covers a field.** Settings > Capture & recording > Presets > Edit. The window is 936 px high (y 31 to 978); at the top the Save to folder field is at y 859 to 913 and the Delete, Cancel and Save preset buttons at y 879 to 939, so a click on the field presses Cancel or Delete (the sheet closed at once in my test), and the Shortcut row ("Change") is at y 1023, below the window. The content scrolls with the wheel (the field moved to y 646), but nothing shows that. Presets cannot get a Save to folder or a shortcut with the pointer until the user scrolls; the shortcut conflict check could not be tried. | |
| W-2 | low | The tray item **Quick Capture…** opens the panel where the menu item was clicked (550x611 at x 1254, y 13: near the pointer, top right), not above the taskbar icon as QC-1 step 2 says. | |
| W-3 | low | The accessible name of the dock "Capture…" button is "Capture…, Ctrl Alt Shift C" while the others read "Record, start or stop shortcut CtrlAltShiftR" and "Capture region, shortcut CtrlAltShift1". | |
| W-4 | low | P-8 is only partly done: the disabled Make a board is an unnamed Text with `IsEnabled: True`, so a screen reader neither lists nor explains it. | |
| W-5 | low | When the overlay is killed or times out the error toast shows the overlay's whole log ("Capture didn't start. Capture failed: [INFO cm_core::logging] overlay 0.3.6 started [INFO cm_overlay] ...") and stays until dismissed; the shorter first toast is fine. Seen after the 120 s watchdog (picker left open) and after my kill. | |
| W-6 | low | Boards: the intro card ("Boards Preview ... Got it") draws over the right column of the New board sheet while the sheet is open (picture `windows-board-sheet-intro-overlap.png`: it hides the top of the Gap block). | 0.3.5 R-5 related |
| W-7 | medium (macOS D1) | In the preset picker the lower rows (Meeting, Docs shot) could not be reached by pointer or UI Automation; the click landed on "Save current as preset…". | macOS D1, fixed on main (`ffc8057b`) |
| W-8 | medium (known) | Room-only audio still gets an invented outro sentence (above). Words crowd into the last 0.4 s of the clip. | macOS D4 |
| W-9 | low | `record start --preset "Bug report"` (a screenshot preset) printed "Waiting for confirmation ... up to 5 minutes" before refusing. | fixed on main (`3cfd486f`) |
| W-10 | note | Quick Capture's panel and the Edit sheet close themselves when another window takes the focus (log: "quick_capture window ... closed itself after 2.7s", "quick_sheet ... after 3.8s", 24 s); on this machine every script that ended or minimised a window closed them. | macOS D5 |
| W-11 | note | The region picker, the panel, the bar, the sheet, toasts and dialogs are invisible to screenshots by design, so QA (and bug reports with pictures) cannot show them; the Mac agent hit the same. A debug switch that lifts the exclusion would let QA picture them. | |
| W-12 | note | Long Shot: the first step is always lost on this machine (see above); recovery works. | |

## Not run

Region screenshot from the panel and QC-3 bottom edge (the picker did not take injected drags), Auto-redact in the panel (needs a picture with an email), the Window and Long Shot targets of the panel, Video and Audio recording started from the panel (only the bar's R), QC-4.AC6 (starters stay deleted), the shortcut conflict check and Save to in the sheet (W-1), Settings > Shortcuts rebind and "Couldn't register", Tab/Up/Down in the panel one by one, the Send feedback browser page, Vietnamese and pseudo-locale pictures of the panel, `CM_LONG_SHOT_*` on the hand scroll mode, the real invented-line case for AC12b, the 0.3.6 CI package itself (this is a lab build), Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA/AMD encoders.

## Cleanup

Stopped every test process (app, daemons, overlay, Edge). Removed `C:\lab\q36` (private library, config, clips, imported pictures, Edge profile, the `bugs` folder), `C:\lab\regress-out\q36-*`, `C:\wt\main036`, the source archive, the pushed `*.ps1` scripts (see incident 3), `C:\lab\screen.lock`, the `run` folder and any `cm-lab-run-*` task. Kept `C:\wt\target` (build cache). The volume is back to 0.62 unmuted (0.3 during the hardware tier). The laptop is at the Windows desktop, not rebooted. The default library and settings of the laptop were not changed except as in incident 1.

## Verdict

PARTIAL. On the 0.3.6 lab build every new feature that could be driven works (panel, bar, toast, Undo, presets file, picker, Save to, shortcut run, `list_presets`, `--preset`, 90 tools, confirm dialog, F1 to F3, P-2 to P-5, P-7, P-9), and `cm-regress` has no failure that is not explained by the lab package or by the spec row fixed after this build. Open: W-1 (the preset sheet at this resolution), the P-3 Windows cause (not reproduced), the unfinished P-8, W-8, and the not-run list.
