# QA 0.3.6, Linux x86_64 (X11 and Wayland), PARTIAL

- **Date:** 2026-11-18. **Who:** Claude (QA agent), driven from the Mac. **Machine:** ThinkPad T490, Intel UHD 620, Ubuntu 24.04, GNOME 46, one display (1920x1080). Both session types: X11 (the lab default) and GNOME Wayland (switched with AccountsService and back; the laptop is left on X11).
- **Files under test:** an unsigned candidate built on the laptop by `packaging/linux/release-build.sh --commit c69ef16e3f0e159c30bfaf6056fb6f1f97398afe` (product code of `main` HEAD `23a7271d`; the commits after `c69ef16e` are QA files only): `capturemann_0.3.6_amd64.deb` sha256 `13cde6c1626cb6e5af1dcb04923faabc3a125de39ceb3ba213586db0d2ed5f38`, `capturemann-0.3.6-linux-x86_64.tar.gz` sha256 `c0a17ae41cc2eadf06fa566aa6ea3ca95d0a775c9352a272bd98487afab97bba` (`check-package.sh` passed inside the build). Tested on the installed `.deb` (`/opt/capturemann/lib/capturemann`). The old 0.3.5 `.deb` was replaced; no per-user install in `~/.local`.
- **Isolation:** every run used its own `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET` and `CAPTUREMANN_SECRETS=file`; the maintainer's data was never touched. Test folder `~/qa036` and the helper scripts were deleted at the end.
- **Pictures:** `verification/QA-0.3.6-shots/x11-*.png` and `wl-*.png`. X11 pictures are of the real desktop (`import -window root`). On Wayland the pictures come from the desktop portal (through a small helper), and the panel is visible in them (see L-8). The panel, bar, sheet and picker on X11 are visible to `import` because they are plain X windows.
- **Wayland input:** `xdotool` cannot drive native Wayland windows, so I injected keys and the pointer through Mutter's `RemoteDesktop` D-Bus API (a session kept by a small Python helper, no prompt). On X11 I used `xdotool`.

## Regression (`cm-regress`, all on the installed `.deb`, `--commit c69ef16e`, 0 failed everywhere)

| Tier and session | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard, X11 | 57 | 0 | 7 | `L1-linux-0.3.6-regression-standard-x11-lab-c69ef16e-deb-2026-11-18-claude.md` |
| full, X11 | 64 | 0 | 16 | `L1-linux-0.3.6-regression-full-x11-lab-c69ef16e-deb-2026-11-18-claude.md` |
| hardware (`--allow screen,sound,mic,camera`), X11 | 66 | 0 | 22 | `L1-linux-0.3.6-regression-hardware-x11-lab-c69ef16e-deb-2026-11-18-claude.md` |
| standard, Wayland | 55 | 0 | 9 | `L1-linux-0.3.6-regression-standard-wayland-lab-c69ef16e-deb-2026-11-18-claude.md` |
| full, Wayland | 61 | 0 | 19 | `L1-linux-0.3.6-regression-full-wayland-lab-c69ef16e-deb-2026-11-18-claude.md` |
| `linux.deb_install` (the package purged first, `--installer` the same `.deb`, programs from a `dpkg -x` copy) | 1 | 0 | 0 | `L1-linux-0.3.6-regression-full-x11-lab-c69ef16e-deb-install-2026-11-18-claude.md` |

The skips are the ones the suite always has on Linux (cases "not implemented for linux", no second monitor, no model, no tone player, no soak). The suite has no Quick Capture case; everything below was done by hand. aarch64 was not run.

## Results by item

Legend: PASS = seen working in this run; FAIL = defect below; NOT RUN.

### Quick Capture, QC-1

| Item | X11 | Wayland |
| --- | --- | --- |
| Hotkey Ctrl+Alt+Shift+C opens the panel; second press closes it | PASS (`x11-02-hotkey.png`) | n/a: GNOME 46 has no GlobalShortcuts portal; log "no GlobalShortcuts portal", toast "Global shortcuts are not available here", Settings > Shortcuts says "Not available on this desktop" on every row (`wl-17-shortcuts.png`) |
| Launcher keeps its chord; Launcher has a Quick Capture row (Preview, keycap) and one row "Preset: ..." per preset | PASS (`x11-40-launcher.png`) | not run (no hotkey; Launcher not reachable) |
| Tray (dbusmenu, ubuntu-appindicators): Quick Capture... is the first item when idle, then Capture region/window/display, Capture delay, Open library, Open launcher, New Click Guide, Recent captures, Presets submenu with the four names; while a recording runs Stop and Pause come first and Quick Capture follows the Record group | PASS (read through `com.canonical.dbusmenu.GetLayout`; the item opens the panel; a Presets entry starts that preset) | PASS (same menu; item opens the panel) |
| Dock button "Capture..." with its keycap | PASS (opens the panel) | PASS |
| Panel layout, kinds, tiles (`x11-10` to `x11-14`, `x11-27`) | PASS: header, kind tabs, target row, four tiles, action bar; Video bar red "Record", Screenshot neutral "Capture", Audio has no target row and the line "Mic and system sound, no screen" | PASS (`wl-03-dock-panel.png`) |
| Delay menu replaces the grid | PASS (`x11-12`) | not run |
| Keyboard: 1/2/3, Tab, Up/Down, P, Enter, Esc | **FAIL on X11** (L-1) | PASS: 1 switched the kind, Tab moved the ring, Down toggled Editor, P opened the picker, Down Down Enter chose "Docs shot" ("Capture · Docs shot"), Esc closed the panel (`wl-08-keyboard-in-panel-key1-tab-down-p-pick-esc.png`) |
| Panel opens near the pointer / under the tray icon | **FAIL** (L-2) | **FAIL** (L-2) |
| Panel not in the picture | PASS: the panel is gone before the picker opens and is not in the frozen frame (`x11-05-overlay.png`); a recording started from the panel shows no panel in the screen picture I took | PASS for the picker (`not in the frozen frame`); the panel is visible to the portal while it is open (L-8) |
| Preview label, Send feedback | PASS (seen); the browser click was not run | PASS (seen) |
| Settings unchanged by the panel (QC-1.AC6) | PASS: `settings.json` holds only the shortcut I changed; panel choices are in `quick_capture.json` | not run |
| Hotkey taken: "Couldn't register: used by another app" | PASS: a second X client holding Ctrl+Alt+Shift+C (XGrabKey) gave the log line `could not register Open Quick Capture: HotKey already registered`, the row shows "Couldn't register - used by another app" (`x11-52-shortcuts-taken.png`), and the tray item still opens the panel | n/a (nothing is registered) |
| Rebind: free chord accepted | PASS (Ctrl+Alt+Shift+7 kept, Launcher row and dock keycap followed) | n/a |
| Rebind: Launcher's chord refused with "Already used by Open launcher" | **FAIL / unreachable** (L-7) | n/a |
| Vietnamese (panel, Click Guide dock tile, picker) | PASS: everything fits at the default width; the Click Guide tile wraps to two lines and the Preview pill sits above it without covering the label (`x11-70-vi-dock-crop.png`, `x11-71-vietnamese-panel-...png`) | not run |
| Pseudo-locale | one finding (L-10); dock Record button label does not touch its icon (QC-1.AC10) (`x11-75`, `x11-76`) | not run |

### Captures from the panel, QC-3

| Item | X11 | Wayland |
| --- | --- | --- |
| Region, Copy on, Editor off: picker, capture, clipboard holds a PNG (9,551 bytes), toast "Copied to clipboard" with Edit and Undo | PASS (`x11-30-toast.png`); toast closes after about 11 s and stays while the pointer is over it | the first Region run went through the Docs shot preset: editor opened and toast "Saved to your library / Docs shot" with Edit and Undo (`not a Copy run`) |
| Undo on the toast removes the capture from the library | PASS | not run |
| After-capture bar (Copy and Editor off): six actions with keys, under the area, centred (`x11-33-after-bar.png`); closes by itself after 20.0 s; stays 25 s while the pointer is over it | PASS | not run |
| Bar keys (C, Esc) | **FAIL on X11** (L-1) | not run |
| Bar button (mouse), Copy | PASS: clicking Copy put the image on the clipboard and closed the bar | not run |
| Bar Edit, Save, Text, Long Shot, Record; bar at the bottom edge; second display | NOT RUN | NOT RUN |
| Auto-redact (made-up page with an email address, through the "QA save" preset) | PASS: the saved PNG shows the address blurred (`x11-61-preset-save-to-redacted.png`) | not run |
| Window and Display targets, Delay countdown from the panel | Window target opened the picker on Wayland (list empty, "Pick on screen" offered); others NOT RUN | |
| Video from the panel (Display, Mic, Countdown 3 s; and Region 300x200 with Mic and Webcam) | PASS: countdown banner, recording runs with the tracks chosen (screen, cursor, mic, camera), `record stop` saves the take; the Settings and the Record sheet were not changed | not run |
| Audio from the panel (Mic, Markers) | PASS: audio recording, toast "Markers are on / Add a marker Ctrl+Alt+Shift+M"; it started with a countdown (L-11) | not run |

### Presets, QC-4

| Item | X11 | Wayland |
| --- | --- | --- |
| Four starters seeded; `presets.json` holds them; picking Bug report, Demo video, Docs shot fills kind, target, tiles and the bar reads "Capture · Bug report" / red "Record · Demo video" (`x11-16-presets-picked-...png`) | PASS | PASS (Docs shot) |
| Picker shows every row, nothing cut off (QC-4.AC8) | **FAIL** (L-3) | **FAIL** (L-3) |
| Settings > Capture & recording > Presets: rows with up/down, Edit, Delete, New preset (`x11-44-settings-capture.png`) | PASS | not run |
| Edit sheet (`x11-45-edit-sheet.png`) | **FAIL** (L-4, L-1) | not run |
| Hand-edited file: a video preset with `"target": "long_shot"` skipped, the others load, one notice names it | PASS (log `skipped 1 preset that is not valid: "QA bad" (a video preset cannot use a Long Shot)`) | PASS (toast "Some presets were skipped", `wl-` picture not stored) |
| Preset shortcut runs the preset directly (a preset with `shortcut` and `save_to`) | PASS: Ctrl+Alt+Shift+B opened the region picker with no panel | n/a (not registered on Wayland, by design) |
| Save to `.../{date}`: PNG in the folder with the local date and the capture also in the library | PASS | not run |
| "Save current as preset...", delete starters stay deleted, unknown fields kept on save, shortcut conflict check in the sheet | NOT RUN (the name field cannot be typed into on X11, L-1) | NOT RUN |
| Preset picked in the panel changes the remembered tiles | **observation** L-9 | seen again |

### Agents and CLI, QC-5

All PASS on X11 against the running app: `capturemann presets list` (and `--json`, with summaries); `capture screenshot --preset "Bug report"` captured the primary display and copied it; `--preset ... --delay 2` took about 2.7 s (the explicit argument wins); an unknown preset gives `error[invalid_input]: unknown preset "nope"`; a video preset on `capture screenshot` and a screenshot preset on `record start --preset "Bug report"` give `invalid_input` with a plain reason before any confirmation (QC-5.AC7); a Window preset alone says `add --window <ID|TITLE>`. MCP over stdio (`cm-mcp`): `tools/list` returns **90** tools; `list_presets` is read-only (`readOnlyHint: true`); `capture_screenshot` and `start_recording` have `preset`; `list_presets` returns the presets; `capture_screenshot` with `preset: "nope"` gives `invalid_input`. The Long Shot confirmation dialog for an agent-started run appeared and "Allow" ran it (AGT-2 unchanged).

### Transcript fixes (AI-1.AC12, AC12b, AC16) with synthetic data only, no model

- `cargo test -p cm-ai --no-default-features --lib -- guard wordtimes pieces` on the laptop: **64 passed**, 0 failed (includes `doubtful_words_over_the_room_become_an_unclear_sound`, `short_audio_has_a_reference_when_speech_and_room_are_clearly_apart`, `unsure_short_text_over_the_room_stays`, `every_word_ends_after_it_starts_and_the_last_takes_the_piece_end`). `cargo test -p cm-ai --no-default-features --test recording`: **8 passed**.
- With a 9 s synthetic tone imported (`imported ... -> rec_...`), `capturemann ai transcribe` says `no_model` and "Nothing is downloaded automatically" (no model on this laptop, none downloaded).
- NOT RUN: the guard on a real transcript (needs a model), so a doubtful line on this hardware was not seen.

### Fixes

| Fix | Result |
| --- | --- |
| F1 a text file named `.png` | PASS: `import` and `--dry-run` say "The file can't be read. It may be damaged, cut short or not a picture." (`reason: unreadable_file`), no codec message |
| F2 `annotate` far outside the image | PASS: `ops[0]: the shape lies far outside the 700x200 picture; keep it within 5600 px of the picture`, exit 2; a shape 300 px outside is accepted |
| F3 / AC17 Long Shot leaves the page where it was | **FAIL** (L-5) |
| P-2 ambiguous window | PASS: ``QAWIN` matches 2 windows: 46138334 "QAWIN"; 48235486 "QAWIN". Pass the window id to choose one.` |
| P-3 Long Shot says why and keeps frames | NOT SEEN: the runs on a 300-line text window ended `reached_end` with 29 and 30 frames; no failure to join was provoked. The `note` and `long_shot` fields are in the answer (`note: null`) |
| P-4 `annotate --out` on a large image | PASS: an 8000x6000 (48 MP) imported PNG, a rect and a text op, `--out` wrote a 944 KB PNG in under 1 s, twice, no `internal` |
| P-7 Boards intro card closes with the sheet | NOT RUN |
| P-9 / F5 Vietnamese Click Guide tile and Preview pill | PASS (see above) |
| QC-1.AC9 / QC-4.AC8 picker keyboard | AC9 PASS on Wayland (Down Down Enter chose the preset, Esc closed the panel); not reachable on X11 (L-1); AC8 FAIL (L-3) |

### Existing behaviour

Dock tiles, Launcher, the recording banner, Region screenshot overlay, `list_targets`, library, import, annotate, export and recording worked as before in the runs above; the regression suite has 0 failures on both sessions. Long Shot on Linux ran end to end for the first time in my hands: a 300-line text window gave a 822x8205 and a 822x8321 px image in about 50 s, with header and footer detected (101 px and 67 px), 29 and 30 frames, `reached_end`.

## Defects

Severity: S1 blocks use; S2 a feature does not work as specified on this system; S3 wrong or incomplete behaviour with a way round; S4 cosmetic or note.

| ID | Sev | Where | Defect and steps |
| --- | --- | --- | --- |
| L-1 | **S2** | X11 | **No floating window takes the keyboard.** The Quick Capture panel, the after-capture bar, the Edit preset sheet, the Launcher and the toasts are X windows with override-redirect set and `_NET_WM_WINDOW_TYPE_NOTIFICATION` (`xwininfo -id <w>`: "Override Redirect State: yes"), so the window manager never gives them the focus and a click does not either. Steps: X11 session, focus another window (or the library window), press Ctrl+Alt+Shift+C, press 2 or 3: the keys go to the focused app (in the library window they typed "23" into the search field; in an `xev` window `xev` saw them). Esc and Enter do not reach the panel; the panel closes only with the hotkey again. On the bar, C and Esc do nothing (clipboard unchanged, bar stays until the 20 s timer). In the Edit preset sheet the Name and Save to fields cannot be typed into (clicked into the field, `xdotool type abc`: nothing). The Launcher shows the same (typing "quick" does nothing; pre-existing in 0.3.5 by the same window kind, but Quick Capture and the preset sheet depend on it). Mouse use works. On Wayland the panel takes the keyboard correctly. Affects QC-1.AC7, QC-3 keys and AC3, QC-4 sheet and "Save current as preset..." (name entry). Pictures: `x11-04-key3.png`, `x11-45-edit-sheet.png`, `x11-47`/`x11-48` not stored, `x11-40-launcher.png`. |
| L-2 | S3 | X11, Wayland | **Placement ignores the pointer and the tray icon.** X11: with the pointer at (300,300), (1700,900), (960,20), (100,1070), (1500,200) the panel always opens at +786+324 (screen centre) from the hotkey; from the tray item it opens at +786+12 (centred, not under the icon at the right of the top bar). Wayland: from the dock button, and from the tray item with the pointer at (1500,600), it opens at about (117,82), the top-left of the library window. QC-1 says near the pointer on the display that has it, and under the icon from the tray. (The after-capture bar is placed correctly under the area on X11.) |
| L-3 | S3 | X11, Wayland | **The preset picker is cut off (QC-4.AC8 not met).** Open the picker (click the pill, or P on Wayland): the panel grows only from 400 to 411 px (`xwininfo`: 352x411) and "Meeting" is not visible until the list is scrolled (wheel works). The row is about 55 px high with its summary line, the fix assumes 48 px (`quick/place.rs` `picker_height`, `ROW: 48.0`). Wayland shows the "Meeting" label with its summary cut. With a sixth preset it is worse. Pictures `x11-14-picker.png`, `x11-15-picker-scrolled.png`. |
| L-4 | S3 | X11 | **Edit preset sheet: the Name field of an existing preset is empty** (Settings > Capture & recording > Presets > Edit on "Bug report": the field shows only the caret, `x11-47` / `x11-45-edit-sheet.png`), so Save preset would have no name to keep, and the field cannot be filled on X11 (L-1). Not checked on Wayland. The sheet (window 440x640) is also taller than the 760 px library window: the footer (Delete, Cancel, Save preset) covers the "Save to" field until the sheet is scrolled with the wheel and nothing shows that it scrolls (the Windows report's W-1, reproduced here at 100 % scale, `x11-45-edit-sheet.png`, `x11-46-sheet-scrolled.png`). |
| L-5 | S3 | X11 | **Long Shot AC17 says `restored: true` but the page was not restored.** `capturemann capture screenshot --window <gnome-text-editor with a 300-line file, scrolled to line 1> --long --json` (Allow in the dialog): the answer holds `"page_offset": {"moved_px": 7679, "restored": true}`, but the editor is left at line 72 (about 1,870 px down); the first run (without `--json`) ended at line 74. It stays there after 3 more seconds, so it is not an animation in progress. `x11-93-before-long2.png`, `x11-94-after-long2.png`. The claim should be false (or the scroll back should finish). Also: `moved_px` 7679 is more than the page can scroll (about 7,300 px). |
| L-6 | S3 | X11 | **A stale toast "Capture removed" appears with the after-capture bar.** Steps: Copy on, take a region, press Undo on the toast ("Capture removed" shows and closes after about 12 s); turn Copy off, take a region: the bar opens and a toast window (440x72 at the lower right) opens at the same time showing "Capture removed" again, without an action; it stayed for 84.7 s the first time and showed again on the next bar capture (picture `x11-33-after-bar.png` was taken a moment before the toast appeared; log: "toast window ... opened" right after "after-capture bar opened", no "toast queued" line). |
| L-7 | S3 | X11 | **The shortcut recorder cannot take a chord that is already registered.** Settings > Shortcuts > Open Quick Capture > Change, press the Launcher's chord (Ctrl+Alt+Shift+Space): the app's own global grab fires first and opens the Launcher, the recorder keeps showing "Press the new shortcut" (`x11-53-rebind-conflict.png`); the same for Ctrl+Alt+Shift+1. So "Already used by Open launcher" (QC-1 step 5, QC-4.AC5 conflict naming) cannot appear from the keyboard on X11. A free chord is accepted. The global hotkeys should be released while a row records. |
| L-8 | S4 | Wayland | **The panel is not excluded from capture on Wayland.** With the panel open, a portal screenshot of the desktop shows it (`wl-03-dock-panel.png` was taken that way). The panel closes before a capture starts, so a screenshot is clean, but a recording started with the panel open would show it; the spec says "excluded from screen capture and recording like the HUD". |
| L-9 | S4 | X11, Wayland | **Picking a preset overwrites the remembered panel options.** After picking "Docs shot" (Window, editor on, copy off) and closing the panel, `quick_capture.json` holds `shot_target: window`, `editor: true`, `copy: false`, and the next panel opens with them and without the preset; on Wayland the panel reopened as "Docs shot" with Region selected. The spec says the remembered tiles belong to the panel, a preset applies only while it is picked. |
| L-10 | S4 | pseudo-locale | In the target row the "Long Shot" label wraps to two lines and is clipped (and the first label is clipped on the left) at +30 % text (`x11-76-pseudo-locale-panel-shot-video.png`). Tiles, bar, header and dock Record button hold. |
| L-11 | S4 | X11 | An audio recording started with Record in the panel went through a "Starting in 2" countdown (the status said `countdown`, `starts in 1 s`), though QC-1 says a recording from the panel starts at once. It may be the countdown of another setting; I did not check which one. |

Notes that are not defects of 0.3.6: the Linux recording shows its banner in the library window and no floating HUD or camera bubble (so there is no HUD to check); the lingering "Capture didn't start" error toast after a timed-out overlay stayed on screen for more than a minute until the next toast; `capturemann targets` lists no window of CaptureMann itself.

## What I did not do

P-7 (Boards intro card), P-8 and P-5/P-6 (Windows only), QC-3 bar actions Edit, Save, Text, Long Shot and Record, the bar at the bottom edge and on a second display, the Delay countdown and Esc from the panel, Video Window and Long Shot targets from the panel, "Save current as preset..." and the unknown-field check (no typing on X11, and the Wayland run was short), shortcut conflicts in the preset sheet, Settings presets Delete (second click) and Move, the Send feedback browser page, the light theme, Vietnamese bar and sheet, Wayland: Launcher, Delay, Auto-redact, Copy-run clipboard (the `wl-paste` check ran after the Docs shot preset and found nothing copied, which is the preset's own setting), AI-1 on a real transcript (no model), aarch64, KDE, other distributions, AMD/NVIDIA, a second monitor, mixed DPI.

## Verdict

PARTIAL. On this laptop the Quick Capture features work with the mouse on X11 and with mouse and keyboard on Wayland, the presets, Save to, Auto-redact, the CLI and MCP parts (90 tools) and F1, F2, P-2, P-4, P-9 pass, and `cm-regress` has 0 failures in all six runs. The main open item is L-1 (no keyboard in any floating window on X11, which includes the preset sheet's text fields), followed by L-2 to L-7.

## Cleanup

Removed `~/qa036`, the helper scripts in `~/lab`, the extracted package copy and the test windows; the custom GNOME keybinding used for the "taken" test was reset; the `.deb` 0.3.6 is installed again (`capturemann --version` says 0.3.6); the session is back to X11 (AccountsService `Session=ubuntu-xorg`, `XSession=ubuntu-xorg`, as before); no product process is left running; the lab lock is gone.
