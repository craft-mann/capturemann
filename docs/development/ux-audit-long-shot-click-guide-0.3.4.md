# UX and QA flow audit: Long Shot (CAP-6) and Click Guide (GDE-1 to GDE-4), 0.3.4

Date: Nov 15, 2026. Method: reading only (specs, design boards, code, manual scripts, the QA reports of passes 1 to 3). Nothing was run: no build, no app, no screen. Every claim below is "from the code" unless it says **reported** (from a QA report) or **not determined** (could not be settled by reading). Where the spec says something is as designed but the result is odd, it is flagged anyway and marked **(as designed)**.

Sources read: `specs/features/01-screenshot.md` (CAP-6), `specs/features/19-click-guide.md`, `specs/02-agent-interface.md`, `tests/manual/CAP-6.md`, `tests/manual/GDE.md`, `docs/development/long-shot-debug.md`, `docs/development/design-requests-0.3.4.md`, boards `R034LongPick`, `LongRun`, `GuideRec`, `GuideWindow`, `Sidebar` (text only; `GuideExport`, `GuideEdit` are not in the folder, so the export sheet and editor were checked against the spec and code), `verification/QA-0.3.4-*` (long shot, click guide, summaries), and the code in `crates/cm-capture/src/long_shot/`, `crates/cm-daemon/src/long_shot_ops.rs`, `crates/cm-daemon/src/guide/`, `crates/cm-library/src/guide.rs`, `crates/cm-cursor/src/clicks.rs`, `apps/desktop/src/ui/long_shot/`, `ui/guide_view/`, `ui/app/system.rs`, `nav.rs`, `launcher/commands.rs`, `apps/overlay/src/{app,long_bar}.rs`, `crates/cm-cli/src/commands/guide.rs`.

---

## A. Flow inventory

Legend for the "Exit" column: S success, E Esc, X Stop button, ERR error, PERM permission missing.

### A1. Long Shot

Entry points (all verified in code unless noted):

| # | Entry | Code | Notes |
| --- | --- | --- | --- |
| LS-E1 | Sidebar tile "Long Shot" (2nd dock row) and the rail button | `ui/sidebar.rs extra_tile`, `nav.rs tile_state` | Disabled only with no library; Active while a run is open (click brings the HUD forward). Icon was missing in pass 3 (S1), registered since commit 4d786142. |
| LS-E2 | Launcher command "Long Shot" | `launcher/commands.rs:534` | |
| LS-E3 | App menu / menu-bar item "Long Shot" | `platform.rs` ids | |
| LS-E4 | Hotkey | `ShortcutAction::LongShot`, no default | Reads "Not set" until configured; undiscoverable by design (as designed). |
| LS-E5 | Tray | not determined (no Long Shot id found in `platform.rs` tray list; CAP-6.md step 10 names only launcher, menu bar, hotkey) | Likely none. |
| LS-E6 | Overlay mode chips (Region/Window/Display/Click Guide to Long Shot and back) | `apps/overlay/src/long_bar.rs`, `ui/long_shot/mod.rs after_pick` | Switching mode closes the overlay and starts the other mode. |
| LS-E7 | CLI `capturemann capture screenshot --long ...` / `--mode long` | `cm-cli`, `long_shot_ops.rs parse` | Auto only for agents. |
| LS-E8 | MCP `capture_screenshot { mode: "long", scroll?: "auto" }` | `long_shot_ops.rs`, spec 02 | Confirm tier for Auto; Hand refused (`needs_user`). |
| LS-E9 | "Try Hand scroll" toast button | `ToastAction::TryHandScroll` | Opens Long Shot again; does not preselect Hand (as designed, spec CAP-6 "differences" 3). |

**Flow LS-1: person, overlay, Auto scroll (happy path).**
1. Entry (any of E1 to E4). `start()` returns silently if a Long Shot is open or `capture_busy` (`ui/long_shot/mod.rs:257-270`); sets `capture_busy`.
2. Full-screen overlay on every display (`pick_long`), hover highlights the **window** under the pointer ("Scrollable area" chip, size, scale), or the user drags a region. Bottom bar: Region, Window, Display, Long Shot, Click Guide; Auto/Hand; Vertical/Horizontal; Start with Enter.
3. Click a window, press Enter or Start. On macOS without Accessibility, Enter/Start with Auto chosen opens the card (Grant access / Use Hand scroll) and does not start.
4. `begin()`: HUD (56 px, never in the frames) and side preview open; 400 ms wait for the overlay and confirm to leave the screen (`CAPTURE_DELAY_MS`, plus a second 400 ms sleep in the daemon for Auto, `long_shot_ops.rs:542`).
5. Auto loop: first frame, wait 250 ms, first frame again; scroll about 80 % of the area per step with the pointer parked at the centre (then four quarter points, own windows last); settle wait; stitch; repeat until `reached_end`, `limit_reached`, `no_overlap`, `nothing_scrolled`, stop.
6. Ends: image stored as a screenshot capture (`source: long_shot`, `frames`, `long_shot_end` in the manifest), toast with Open and Copy, library refreshed, `capture_busy` cleared.

States visible to the person: overlay (pick), HUD counter and size, side preview, "Paused", "Adding frame n...", "Scroll back up a little" (Hand), header/footer-kept tags, passive Agent HUD.

**Flow LS-2: person, Hand scroll.** Same until step 4. Then the person scrolls; a frame is taken at one third of the area moved or after ~100 to 150 ms of stillness; frames that do not join are left out and the second line says "Scroll back up a little" (amber dot) with a toast offering Stop and keep; Stop/Esc keeps the image. Preview cue "Keep scrolling" / "End of page · press Stop" (time rule).

**Flow LS-3: Pause and Resume.** HUD Pause holds the loop (`long_state { paused }`); Auto: the loop holds before its next step; Hand: nothing is taken, content scrolled meanwhile will not join (documented in CAP-6.md step 14).

**Flow LS-4: agent or CLI run.** Confirm dialog (generic "Allow this action?"), then Auto with no HUD in the CLI process; the app, if running, shows the passive HUD (Stop, count, Agent chip) from `capture.long_shot.started`; Esc is grabbed globally and stops every Long Shot.

**Flow LS-5: horizontal.** Direction chosen before Start; shown on the HUD; not switchable while running (not built).

**Exits and edge states** (what the code does):

| State | What happens | Verdict |
| --- | --- | --- |
| Success / end of page | "Long Shot saved" (success), library row, Open and Copy | OK |
| Esc | Global key grabbed while any run exists; run ends, image kept, toast "Stopped with Esc..." | OK, but see G-LS-9 (Esc is swallowed from the scrolled app) |
| Stop button | `stopped_by_user`; plain "Long Shot saved" with size and count | OK. Reported: HUD Stop gave `no_overlap` in 2 of 3 Windows runs |
| Size limit | "saved at the size limit" with the 16,384 px text | Text wrong for the 50 MP limit and for a user `max_px` (G-LS-12) |
| `no_overlap` (Auto) | Warning "Stopped where the page jumped" [Open, Try Hand scroll] | OK |
| `nothing_scrolled` | Warning, but the 1-frame image is still stored in the library | Clutter (G-LS-13) |
| Permission missing (macOS Accessibility) | Overlay card; daemon `permission_missing`; toast "Long Shot needs a permission" | OK |
| Wayland | Bar shows one inert "Hand scroll" label; daemon `not_implemented` for Auto | OK; portal prompt per frame not determined (spike S6) |
| Target window closes mid-run | `FrameSource::grab()` errors; `run_*` use `?` (`run.rs:135,137,154,373`), so the whole run returns `Err` and **nothing is stored** | **Data loss (G-LS-1)** |
| Display disconnected / sleep / lock | Same path: a grab error loses the run; no retry | Same |
| App quit mid-run | `quit_wait` knows only recordings and jobs (`ui/app/system.rs:15-23`); the process exits; frames are in memory only | **Data loss (G-LS-2)** |
| Crash / kill -9 | Nothing on disk until the end; no recovery (by design: no `recording.lock` analogue) | Data loss, undocumented |
| Second start while one runs | App: silent no-op (`start()` returns); sidebar tile active. Daemon (CLI/agent): no mutual exclusion (`register_kind`, `daemon.rs:131`): two Auto runs can scroll and capture at once; one Esc stops all | G-LS-8 |
| Recording running | Not checked anywhere in the Long Shot path. The scroller moves the pointer and the HUD is excluded only if the recording's exclusion path covers it (not determined) | G-LS-8 |
| Guide running | Tile stays Ready; Long Shot's overlay click and the scroller become steps in a running display guide (exclusion is only the guide bar's rectangle) | G-LS-8, G-GD-9 |
| Full-screen app / Spaces | Overlay level and HUD level over a full-screen app: not determined | QA |
| Multiple displays | HUD goes on the display holding the area; preview side by room; area spanning two displays: not determined; Windows second monitor not run | QA |
| HiDPI | Frames physical; the stored capture claims `scale: ONE` (`long_shot_ops.rs:643`) | G-LS-14 |
| Dark/light | App is dark only for floating surfaces; HUD on a light page not reviewed | low |
| Vietnamese | Drafts marked fuzzy; "Tay" for Hand; widths tested by `width_tests.rs` (model only) | QA on screen |

### A2. Click Guide

Entry points:

| # | Entry | Code | Notes |
| --- | --- | --- | --- |
| CG-E1 | Sidebar tile "Click Guide" and rail button | `nav.rs:533-540` | Disabled while a Long Shot or recording runs, or no click events. |
| CG-E2 | Launcher "New Click Guide" | `launcher/commands.rs:577`, `guide_off` from `recording_off` | Disabled during a recording. No launcher entry for "Click Guide of a window..." (reported pass 3). |
| CG-E3 | Tray "New Click Guide" | `platform.rs:609` | Not blocked during a recording or Long Shot (G-GD-8). |
| CG-E4 | App menu "New Click Guide" and "Click Guide of a window..." | `commands.rs:52,77,78` | Same. |
| CG-E5 | Library header (All captures, Click Guides): "Click Guide of a window..." then "New Click Guide"; empty state | `library_view.rs:1161-1237` | `disabled_reason()` checks only the click-event gate, not a recording or Long Shot. |
| CG-E6 | Overlay mode chip "Click Guide" | `after_pick "guide"` | |
| CG-E7 | CLI `capturemann guide start/stop/get/set/export` | `cm-cli commands/guide.rs` | On a running app the same daemon is used. **No bar is shown for this start** (G-GD-1). |
| CG-E8 | MCP `start_guide`, `stop_guide`, `get_guide`, `set_guide`, `export_guide`; plugin skill `write-a-how-to` | `cm-mcp/guide_tools.rs` | Confirm on start; the rest Free. |
| CG-E9 | Toast buttons "Pick another window", "Stop and open guide" | `toasts.rs:648` | After a window closes. |
| CG-E10 | Hotkey | none (spec: "not built") | |

**Flow CG-1: person, display guide.**
1. Entry. `start_on` returns silently when the bar is already open (`hud.rs:306-308`).
2. Preflight gate (`preflight.rs`): Ready; macOS needs Input Monitoring: card "Click Guide needs Input Monitoring" [Grant access, Cancel], no bar; Wayland: toast "Click Guide is not available here" with the reason.
3. The bar opens on the **display under the pointer**, centred at the top, 58 px, as wide as its text (max 720 and display less 48). Its rectangle goes to the daemon as the only click exclusion (`hud.rs:368`).
4. `start_guide { display_id, countdown_s: 3 }`: the **guide folder is created and indexed at once** (`session.rs:457-470`), `capture.created` is published, then a 3 s countdown ("Starting... 3", Stop or Esc cancels; Esc is a global key during it).
5. Listening: macOS event tap, Windows `WH_MOUSE_LL`, X11 XInput2 raw, with no key events in the mask (`cm-cursor/clicks.rs:3-6`). Each mouse-down inside the target not paused, not in the bar: frame taken on the listener thread at once, sent to the worker (PNG, OCR, title, ring and badge ops, proposals flag, step written to disk).
6. Bar controls: Pause/Resume, Note step, Undo step, Stop. Double click amends the last step to "Double-click". Right/middle noted.
7. Stop: worker drained (up to 60 s), 0 steps = guide removed + toast "No steps were taken"; else poster written, row refreshed, the editor opens.
8. Editor (`guide_view/mod.rs`): left list (drag, Move up/down, delete, duplicate, note step), right the step (title, note, picture, Edit image, Retake, Duplicate, Delete, Focus crop), title and intro on top, Undo/Redo, Copy as rich text, Export.

**Flow CG-2: person, window guide.** As CG-1 from "Click Guide of a window...": the window picker with the Start guide button; the window's rectangle is read **once** at start (`guide/clicks/sys.rs:95-102`) and a 500 ms watcher pauses the guide after two misses (S16).

**Flow CG-3: window closed.** Daemon pauses itself (`window_closed: true`), bar swaps controls for "Window closed" and "Pick a window...", toast with Pick another window / Stop and open guide; Resume refused (`invalid_input`). Reported pass 3: "Stop and open guide" left the bar open (C2; fixed in code after the report, not seen again).

**Flow CG-4: edit.** Title/intro/step fields commit **only on Enter** (`guide_view/mod.rs:226-246`) and when another action calls `commit`; every change is a `set_guide` revision with undo; Edit image goes to the annotation editor and back.

**Flow CG-5: export.** Sheet (Markdown, HTML, PDF tiles; Save to; headings; image size; intro, notes, note steps; names steps still having redaction proposals with Review step n; exporting anyway allowed). Copy as rich text is a separate button that writes HTML plus text to the clipboard.

**Flow CG-6: agent.** `start_guide` Confirm (dialog "Clicks only. Keystrokes are never recorded." per board and GDE.md section 7), then clicks, `stop_guide`, `get_guide`, `set_guide` (titles, notes, order, delete), `export_guide`.

**Exits and edge states:**

| State | What happens | Verdict |
| --- | --- | --- |
| Stop | See CG-1.7 | OK |
| Esc | Cancels the countdown only. After the countdown Esc does nothing, and there is no keyboard way to Stop (bar takes no focus; no hotkey; no tray Stop) | G-GD-5 |
| Error while recording (frame fails, disk full, 500-step cap, PNG fails) | Daemon publishes `guide.error`; **no consumer in the app** (`git grep guide\.error` finds only `session.rs:159,249`). The click is lost silently | **G-GD-3** |
| Permission missing | Card (macOS), toast (daemon `permission_missing`) | OK |
| Wayland | Entry points disabled with reason; CLI `not_implemented` | OK. XWayland-only sessions are classified Wayland by `is_wayland_session` |
| Window closed | See CG-3 | OK |
| Window moved or resized | Rect fixed at start: clicks in the new place are "outside" (ignored), clicks in the old place count, the ring is placed on a frame of different geometry | **G-GD-6** |
| Window covered by another app | Click inside the rect counts even if it hit the covering app; the frame shows the followed window without it | G-GD-6 |
| Display disconnected | Capture fails: `guide.error`, silent | G-GD-3 |
| Display guide, click on another display | Ignored silently (`c.outside`) | G-GD-11 |
| App quit mid-run | Not in `quit_wait`; steps already written survive (written per click), the worker queue's last step(s) may not; no "a guide is recording" warning | G-GD-7 |
| kill -9 / crash | The guide folder exists and is indexed from the countdown on; there is no lock file and no recovery pass for guides (not found in `cm-library`/`cm-daemon`); an interrupted step may leave a half-written folder (GDE-2.AC2 orphan rule: not determined) | G-GD-7 |
| Second start | App: silent no-op. Daemon: `busy` ("a Click Guide is already recording") | Inconsistent feedback |
| Recording running | Sidebar and launcher disabled; tray, app menu, library header, CLI, MCP allowed | **G-GD-8** |
| Locked screen / sleep | Not determined; capture likely errors silently | QA |
| Full-screen app / Spaces | Bar level is default panel (`harden(window,false)`, no level set): visibility over a full-screen app or on another Space not determined | QA |
| HiDPI | Click in physical px with `scale`; bar exclusion converted per monitor (tests exist) | OK in model; QA on 125/150 % and second monitor |
| Vietnamese | Bar tested to 672 pt; export titles keep Vietnamese; PDF uses Geist (has Vietnamese) | OK |
| CJK, emoji, RTL titles | PDF has Geist only (no CJK, Arabic, Hebrew, emoji); `TextDirection` imported but Latin-only fonts | G-GD-14 |

---

## B. Gaps, per flow

IDs map to proposals in section C. "Evidence" gives file:line.

### B1. Long Shot

**G-LS-1. A grab or scroll error discards the whole run (data loss).** `run_auto_ex` and `run_hand_ex` propagate `grab()`/`scroll()` errors with `?` (`run.rs:135,137,153,154,373`). When the window is closed, the display goes away, the machine sleeps or the capture API fails once, the person loses everything and sees "Long Shot failed". For Hand scroll that can be minutes of work. The engine already knows how to keep frames on every other ending. Same for a Wayland portal frame that is denied halfway.

**G-LS-2. Quit mid-run loses the image, silently.** `quit_wait(recording_active, running_jobs)` has no Long Shot input (`system.rs:15-23`). Cmd+Q or tray Quit during a run closes the app; the HUD is closed with it. Also no warning in the tray menu.

**G-LS-3. Hands-off is never said.** Auto moves the real pointer to the centre/quarter points of the area before each scroll (`long_shot_ops.rs:263-275, 599`). The HUD second line says "Adding frame n..." (`model.rs:262`); nothing tells the person not to touch the mouse, scroll, or type. A moved pointer sends the wheel to another window (reported: covered target gives `nothing_scrolled`). Windows does not raise the target first (reported), macOS in CLI runs the app's own window often covers the centre (reported). The code now orders points to avoid own windows, but not other apps' windows.

**G-LS-4. The scroll area is the window, not the scrollable pane (as designed, board says finer).** Board `R034LongPick`: "Hovering highlights the scrollable area under the pointer, not the whole window; fixed headers stay outside it". Build: the area is the whole window (CAP-6 "Not built"). In apps with a sidebar and a content pane (Slack, Mail, Settings pages), the wheel at the window centre may scroll the wrong pane or nothing. The only fix is a region drag, which is not offered in the hint when a window is highlighted ("Click to use this area, or drag a region" is there, but nothing says why you would).

**G-LS-5. Overlay bar is mouse-only and has no accessibility tree.** `apps/overlay` has no AX exposure (reported: "no accessibility children"); `long_bar.rs` keeps an accessible-name field that nothing publishes. Keyboard: Enter starts, Esc cancels; Auto/Hand, direction and mode switches have no keys. A keyboard or screen-reader user cannot choose Hand scroll or Horizontal. Same for the HUD buttons on macOS/Windows: they have aria labels in GPUI (`hud.rs:264,284,439`) but the passive HUD and preview have no focus path (the HUD "never takes focus" by design).

**G-LS-6. Preview and counters are not available to assistive tech or testers.** Reported: contents of bar, preview and picker unreadable by AX. Consequence for QA: see section D observation methods.

**G-LS-7. HUD can fail to open and the run goes on invisible.** `open_windows` returns early when the picked display is not in the engine list or no GPUI display matches (`mod.rs:403-430`, `log::warn!`), but `begin()` still starts `capture_screenshot`. There is then no HUD, no Stop button; only Esc (global) works. No toast tells the person.

**G-LS-8. Concurrency rules live only in the UI.** App: `capture_busy` and `is_running`. Daemon: `register_kind` accepts any number of Long Shots (`daemon.rs:131`), and `long_states` is keyed by `request_id` which may be absent or repeated (`long_shot_ops.rs:489, 505`). `long_shot_state` uses `request_id` as map key, so two runs with the same id overwrite each other's state. Esc stops all. Nothing stops a Long Shot while a recording or a guide runs. The sidebar disables Click Guide for a Long Shot, but not Long Shot for a guide (`nav.rs:524-540`; the code comment says no engine rule is known).

**G-LS-9. Esc is swallowed from the scrolled app for the whole run.** Esc is a global hotkey while any run exists (`mod.rs:121-146`, `commands.rs:26-32`). In a terminal (vim, less), a web page with a dialog, a PDF in full screen or a game, Esc does not reach the app and ends the run. This is announced ("Esc to stop" on the HUD) but cannot be turned off. Release is not ref-counted: `release_escape` at the end of a Long Shot frees the key while a guide countdown or the delay pill may still need it (all use `delay_pill::{grab,release}_escape`, one id).

**G-LS-10. Hand scroll "End of page" is a guess (as designed).** A 1 s pause mid-page shows "End of page · press Stop" (spec difference 1). A person who pauses to read is told the page ended.

**G-LS-11. Pause semantics are surprising (as designed).** CAP-6.md step 14: Pause in Hand scroll does not stop the page from moving; content scrolled during Pause will not join. "Pause only holds the capture; it is not a way to skip content." There is no hint on the HUD while paused.

**G-LS-12. Limit wording is wrong in two cases.** "It reached 16,384 px, the longest image CaptureMann can edit" (`model.rs:358-365`) is also shown for the 50 MP limit (a wide window stops at fewer rows) and for a user's own `--max-px` (reported on macOS pass 3). Also nothing warns before the limit: the preview shows size but the HUD shows no "x of 16,384" bar except in the board ("6,840 / 16,384 px").

**G-LS-13. Failed runs are stored as captures.** `nothing_scrolled` and `no_overlap` with 1 frame produce a stored 1-frame image and "Long Shot saved"-style toasts or a warning with Open; the library gains an item the person did not want. No "discard" button. There is no way from the toast to delete it quickly (the Screenshot toast has Delete; the Long Shot toast has Open and Copy).

**G-LS-14. Retina scale and provenance are dropped.** `Capture { scale: ScaleFactor::ONE, display_id: None, window_id: None, kind: Region }` (`long_shot_ops.rs:636-650`). A 2x window becomes a 1x image in size-aware features (copy at reading size, Export selected at a given size, blueprint sizes). Window title/app provenance is not stored. Whether the library card title says "Long Shot" or "Screenshot ..." was not determined.

**G-LS-15. Huge images.** 16,384 px by up to 50 MP: Stitcher holds the image, `last_ok` frame clones and (per stitch) a thumbnail of the whole image (`on_stitch -> st.thumbnail`, PREVIEW_MAX 360x1200, `long_shot_ops.rs:572-581`), so the per-frame cost grows with image length. A 3,000 px wide window reaches 50 MP; RGBA = 200 MB plus the PNG encode (hundreds of MB peak). **Copy** puts a 50 MP image on the clipboard; many apps refuse or freeze. The editor/GPU (texture limit 16,384) is reported fine to 16,384 px only for 2,400 wide; 50 MP not run. Poll is 4 Hz and the daemon ships a base64 PNG preview each time it changes (`long_shot_state`). Not determined: peak RSS on a 16 GB Mac.

**G-LS-16. Disk full.** `store_capture_ex` stores after the run; a disk-full error there loses the run too (not determined whether the capture path maps `StorageFull`, `export.rs` does for guides). No pre-check of free space for an up to 200 MB RGBA / large PNG.

**G-LS-17. Reliability gaps that the person experiences as "random".** Reported open: repeated identical paragraphs give `no_overlap` (macOS), Windows Terminal stops at line 111 of 200, Chrome horizontal region one frame, grey window-corner notches at seams (fixed offline, not confirmed on a screen), `nothing_scrolled` on a fresh window. The toast "Nothing scrolled: check that the window can scroll" does not name the likely cause (another window on top, the pointer moved).

**G-LS-18. Agent vs human differences.** Agent: Auto only, Confirm dialog says "Take screenshots" in generic text ("Capturemann Cli is asking to run 'Take screenshots'. You set this tool to Ask first", reported) and does not say "moves your mouse and scrolls your window". `CM_LONG_SHOT_DEBUG_DIR` writes two full-resolution frames to disk if the env var is set in the process; the doc warns, the HUD does not.

**G-LS-19. Platform differences.** macOS: Accessibility required for Auto; Windows: `SendInput`, no raise; X11: XTest; Wayland: Hand only, may prompt per frame (not determined). Wheel step sizes vary per app; settle times measured only on Chrome/Edge. Windows 10, a second monitor, other DPI scales: not run.

**G-LS-20. Privacy.** A Long Shot of a window captures everything in it, including content below the fold the person never looked at (private tabs, email lists, chat history, a long invoice). The result is stored unredacted; Auto-redact is on demand in the editor, not suggested after a Long Shot. The toast offers Copy first, which puts the whole image (including parts never seen) on the clipboard in one click. The preview PNG pulls pixels through `long_shot_state`, which any client of the daemon socket with the right `request_id` can read while it runs (`long_shot_ops.rs:424-432`): the id is a client-chosen string (`new_request_id` in the app). Not determined: the socket's access control beyond file permissions.

### B2. Click Guide

**G-GD-1. A guide started by an agent or the CLI has no bar, no indicator and no exclusion rectangle.** The bar exists only when the app starts the guide (`hud.rs start_on`). `main.rs:579` forwards only `capture.long_shot.*` events (`long_shot::on_event`); nothing listens to `guide.started`. So `start_guide` through MCP or `capturemann guide start`, after the Confirm dialog, listens to every click and takes a full-frame screenshot of the target with nothing on screen, no sidebar state ("running" is derived from the bar's holder, `hud.rs:444`), no Stop button, no exclusion for CaptureMann's own windows. The board's confirm dialog says "until you press Stop", which assumes the bar. The project rule that the recording indicator cannot be disabled (spec 02, Never tier) is violated in spirit. A Long Shot started by an agent got a passive HUD (S18); the guide did not.

**G-GD-2. Hard-to-see consent.** The Confirm dialog for an agent says (board) "Your clicks there become steps until you press Stop". For a display target the text "on Brightdesk" is the target's name; for a display target it must name the display and say every app on it is captured. Not determined whether the dialog distinguishes a display from a window.

**G-GD-3. Failures are silent.** `Worker::step` errors and `SinkState::on_click` capture errors publish `guide.error` and nothing in the app consumes it (`session.rs:155-163, 247-252`). Cases: disk full; the 500-step cap (`cm-library/guide.rs:56, 634` "a guide holds at most 500 steps"); a frame that cannot be taken (display asleep, window minimised, locked screen). The person keeps clicking, the counter does not move, no toast, no sound. The poll reads `recorded_steps`, so the counter is the only signal. The 500-step cap is also not shown anywhere (bar, editor, spec AC).

**G-GD-4. Spam clicks and memory.** The sink sends frames through an **unbounded** `mpsc::channel` (`session.rs:473`) while the worker does PNG encoding and OCR per step (about a second or more). A 4K or 5K display frame is 33 to 59 MB RGBA; 20 quick clicks queue 600 MB to 1.2 GB. Double-click detection only merges clicks within 400 ms and 6 px. The 100 ms rolling-buffer idea of S3 (spec "if frames arrive late") is not built; latency of the frame after mouse-down is a spike result, not a code guarantee. Not determined: the capture latency on each system.

**G-GD-5. No keyboard path to stop, and no global stop.** The bar takes no focus; Esc works only in the countdown (`hud.rs:563-575`). There is no hotkey for New Click Guide (spec: not built) and none for Stop; the tray has no Stop item (none found in `platform.rs`). A keyboard-only user, or a person whose bar is hidden (full-screen app, another Space, display unplugged), can only stop by clicking the sidebar tile (which needs the window), or the CLI. If the bar is on another display, the sidebar tile "brings the bar to the front" but that does not move it.

**G-GD-6. Window follow uses a fixed rectangle.** `guide/clicks/sys.rs:95-102` converts the window's bounds once; `cm-cursor/target.rs` documents "Window moves during the recording are not tracked". Moves and resizes are normal behaviour (dragging a dialog, snapping a window, an app that resizes itself). Effects: steps missing (clicks in the new place are `outside`), wrong steps (clicks in the old place), rings off the clicked control (the click is mapped to the old origin and the frame is the window's current content). A window covered by another app (a password manager popup, a notification) still counts clicks in its rectangle and its frame shows the followed window without the popup. No indicator tells the person that clicks were ignored (G-GD-11). The window watcher exists (500 ms, two misses) but only for "closed".

**G-GD-7. Quit and crash.** `quit_wait` ignores guides (`system.rs:15-23`): quitting while recording closes the app with the worker thread mid-queue. A step in flight (PNG + OCR) is lost; the empty guide folder indexed since the countdown remains (a 0-step guide that shows as "No preview"). No `recording.lock`-style marker, no recovery on next start, no "Recovered a Click Guide with N steps" toast (not found in code). The spec promises nothing here, but a tutorial that took 10 minutes is the typical data-loss case. Per-step writes make it milder than Long Shot.

**G-GD-8. Entry points disagree.** Sidebar and launcher disable Click Guide during a recording or Long Shot; tray, app menu, library header button and CLI/MCP do not (`hud.rs pick_window/start_on` only check the click gate; `library_view.rs:1174,1223` pass `disabled_reason()`; `start_guide` in the daemon checks nothing about recordings). Starting during a recording: the recording HUD's Pause/Stop clicks are steps (not excluded), and the person gets two overlapping "recording" modes.

**G-GD-9. Only the bar's rectangle is excluded.** Clicks on CaptureMann toasts (including the window-closed toast's own buttons if the guide were not paused), the delay pill, confirm dialogs, the recording HUD, the Long Shot overlay and CaptureMann's main window on the followed display all become steps (the spec: "A click inside the CaptureMann HUD is not a step"; the board adds "HUD, toasts and ring excluded from the shots"). Excluding from the **picture** (`exclude_from_capture`) is separate from excluding from **clicks**; the latter is only the bar.

**G-GD-10. Step titles come from on-screen text: privacy.** `draft_title` takes the nearest OCR line within 80 logical px, trimmed to 40 characters (`titles.rs:18-36`), with no check against Auto-redact detectors. The title is stored in `guide.json`, sent in the `guide.step_added` event (`session.rs:222-225`, to every subscriber, including MCP/CLI watchers if the bus is forwarded: not determined), shown in the library/editor, written into Markdown, HTML, PDF and the clipboard, and returned by `get_guide`. A click on an email address, a card number, a token, a customer name, a password-field label with a revealed value, produces `Click "jane.doe@example.com"` in plain text even when the picture's redaction proposals are accepted: the blur hides pixels, not the title. In the other direction, OCR text near the click from a hostile page (`Click "](https://evil)"`) becomes a title (see G-GD-13). The editor's title field shows it, so a screen share of the editor reveals it too.

**G-GD-11. Silent ignoring.** Clicks outside the followed window/display, while paused, on the bar, or while the window is closed add nothing and say nothing; the bar shows no "n ignored" or flash. A person who follows a window and clicks into a popup that is its own window (menu, dropdown, file dialog, tooltip) gets no step for exactly the click that matters; popups are separate windows on macOS and Windows and fall outside the window's rectangle or are not in the window capture. Not determined how the window capture treats child popups.

**G-GD-12. Editor semantics.**
- Retake (`guide_view/mod.rs:1289`, `session.rs:798-815`) takes a fresh frame of the followed target **now**: for a display guide the editor itself (not excluded) covers the display; for a window guide the window may be closed or changed. It **replaces** the picture and re-creates ring and badge from the stored click, so the person's own callouts, blurs and crop on that step are dropped (recoverable with undo, `.undo/` holds the old folder). No explanation, no confirm, and the caption "taken on mouse-down" no longer true afterwards.
- Text fields commit only on Enter (`mod.rs:226-246`); leaving the page with Back or quitting with an uncommitted edit loses it (`commit` is called by actions, not by blur or by leaving; not fully determined for the Back button).
- Delete is immediate, undoable (OK). No drag handle accessible name was confirmed (drag grips: not determined); Move up/Move down are the keyboard way.
- Step count: 100 or 500 steps rendering a list and thumbnails: not determined.
- A guide in progress is visible and editable in the library while recording; `set_guide` for the recording guide is refused (`busy`) except pause/note/remove_last, but delete/move/export through the library can still be tried (export refused, `export.rs:300`).

**G-GD-13. Export correctness and safety.**
- **Markdown folder.** `export_guide` writes `guide.md` and `images/step-NN.png` directly into `dest` (`export.rs:343-350`). The desktop passes the chosen folder as `dest` unchanged (`guide_view/export.rs:137-148`) although its comment says "Markdown writes into a folder of its own named after the guide". Exporting two guides into Downloads overwrites the first guide's images and `guide.md`; stale `step-NN.png` from a longer earlier export stay. No overwrite prompt in any format (HTML/PDF write to `<title>.html` or the given file).
- **Markdown escaping.** `md_inline` escapes only `<` and newlines; `md_alt` only brackets and `<`; notes and intro only `<` (`export.rs:62-86`). A title or note with `[text](url)`, `![](url)`, backticks, `*`, `_`, `#`, `|` or a leading `-` changes the rendered output (links, remote images, tracking pixels, broken headings). Sources: the person's own text (mostly harmless), an agent's `set_guide`, and OCR text from the screen. HTML is escaped (`html_escape`) and the lang is hard-coded `"en"` (`export.rs:155,159`): wrong for a Vietnamese guide.
- **Redaction proposals.** Exporting with pending proposals is allowed: the sheet names the steps; the files carry no blur for them. The warning for agents is only in `warnings[]` (`export.rs:322-332`). Default button is the export itself.
- **Rich text.** HTML with base64 PNGs in `public.html` (reported 142 KB for 2 steps); at 100 steps and 2x it is tens of MB; Google Docs, Notion and Word handle base64 images differently (reported: not pasted into a real app). `export_guide rich_text` also **returns the whole HTML with all pictures in the tool result** (`export.rs:385`), so an agent's context gets megabytes and gets unredacted-pending pictures, and the user's clipboard is overwritten with no confirmation (Free tier).
- **Arbitrary destination for agents.** `dest` is any path; for HTML/PDF a path with an extension is written as the file itself (`export.rs:357-359, 389-392`), overwriting any existing file such as `~/Documents/report.docx`. Tier is Free, no confirmation. The tool table says "writes only to the chosen folder" but there is no check that it is inside a folder the person chose.
- **File names.** `file_stem` does not cut to 255 bytes or handle Windows reserved names (`CON`, `NUL`) or a trailing space-dot sequence beyond trim; a very long OCR/agent title fails at write.
- **PDF.** Geist only (`pdf.rs:18-19`): no CJK, Thai, Arabic, Hebrew, emoji; missing glyphs not handled in code read. Tall images are scaled to `MAX_IMAGE_H` (readable? not determined). A step picture taller than the page is shrunk, and a heading is kept with its picture.
- **Memory.** `build_doc` renders all steps in memory first (`export.rs:231-262`), then base64 for HTML/rich text. 500 steps at 2x ~ 500 MB to 1 GB.
- **After export.** Toast "Guide exported: n steps written to path" has no Open or Reveal button; Markdown "path" is the chosen folder, not a named subfolder.

**G-GD-14. Accessibility and wording.**
- Bar: names are on buttons (reported readable: Stop, Pause, Note step, Undo step). "Window closed" amber label is not a named element (reported on Windows). Countdown not read. The dot's colour is the only signal for paused (grey) / closed (amber); the second line also says "Paused ·" (OK).
- Sidebar tiles: aria names and tooltips with the disabled reason (OK). Disabled tile: no Tab stop, hover only (as designed) so a keyboard user cannot read why.
- Title inconsistency: "Click Guide" (feature), "Click Guides" (nav), "guide" (library kind), "How-to" (launcher keyword). Library title default "Click Guide 2026-11-15 11.04" is in local time; two guides in the same minute collide in name (not determined if the index dedupes).
- Launcher: no "Click Guide of a window..." and no "Stop" command (reported).

**G-GD-15. Library integration.** Guide cards, sort without Duration, chip. Not determined: Select/Bulk (Export selected, Make a board, Delete) with a guide in the selection; upload/auto-upload destinations and publish for a guide (cm-jobs/cm-storage have no guide handling); library search text (do step titles go into search?); move/rename/delete tests are claimed in the spec.

**G-GD-16. Platform differences.** macOS: Input Monitoring; Windows: low-level hook (UIPI: clicks on an elevated app's window are not delivered: not determined) and physical-px exclusion; Linux X11: exclusion rectangle with a scaled display not built (spec); Wayland: disabled. Frame-after-mouse-down latency differs.

**G-GD-17. Not built but on the boards:** the 52 px click ring and step toast on screen (capture-excluded layer window per click), a keyboard shortcut. For a person this means no feedback that a click became a step except the counter.

### B3. Cross-cutting

- Two features, two different ways to run "passively for agents": Long Shot got the passive HUD, Guide did not (G-GD-1).
- Quit logic knows only recordings and jobs (G-LS-2, G-GD-7).
- Esc is shared by four things with one global id (delay pill, Long Shot, guide countdown) and a first-wins order (`commands.rs:26-32`).
- Floating surfaces have no AX tree on macOS for the overlay, preview and HUD; QA could only measure them. Same blind spot will hit users of VoiceOver.
- No CLI event stream for `guide.*` or `capture.long_shot.*` (only `recording watch`, `jobs watch` exist), so agents and QA cannot observe progress without the raw socket.

---

## C. Proposals

Effort: S under a day, M 1 to 3 days, L more. Risk is the chance of regressions or of arguing with the spec.

### P1: must fix before release

**P1-1. Show a passive bar for every guide the app did not start.** Problem: G-GD-1. Evidence: `main.rs:579` (only long shot events), `hud.rs:306`. Change: handle `guide.started`/`guide.stopped`/`guide.paused` in the app; if no bar is open, open the bar in a passive form (dot, "Step n", "Started by agent {client}", Stop, Pause; no Note/Undo) on the target's display, register the exclusion rectangle, set the sidebar tile Active, and toast "A Click Guide started by {client} is recording your clicks." with a Stop button. Include `by` in `guide.started` (it already has it). Also show the passive bar for CLI starts. Effort M. Risk low. Design request: "Click Guide bar for a guide an agent or the CLI started" (like S18's passive HUD: Stop, count, Agent chip, same placement). Spec text to add to GDE-4: "The user can always see and stop a guide."

**P1-2. Never discard a Long Shot because of a capture error.** Problem: G-LS-1. Evidence: `run.rs:135-154,373`. Change: in both loops treat a `grab()`/`scroll()` error after the first stitched frame as an end (`EndReason::CaptureFailed` with a note: "The window was closed or the screen went away"), return the image so far; only an error before the first frame is `Err`. Toast: "Long Shot saved. The window closed after {n} frames. Everything up to there is kept." Effort S. Risk low. Add tests with a failing `FrameSource`.

**P1-3. Tell the person when a click did not become a step.** Problem: G-GD-3. Evidence: `session.rs:155-163,247-252`, `cm-library/guide.rs:634`, no consumer. Change: app handles `guide.error` with a toast once per 10 s: "A click was not saved: {reason}" (reasons: "The guide is full (500 steps). Stop and start a new guide.", "The disk is full.", "The screen could not be captured."). The bar shows a red dot and the second line "Last click not saved" until the next step. At 450 steps warn "50 steps left". Daemon: put a stable `code` in the event. Effort M. Risk low.

**P1-4. Markdown export into its own folder, and no silent overwrite.** Problem: G-GD-13 first bullet. Evidence: `guide_view/export.rs:137-148` vs `guide/export.rs:343-350`. Change: the desktop (and CLI/agent default) appends `<file_stem(title)>/` to the chosen folder for Markdown; if the target exists, ask "Replace" / "Keep both" (the second adds " 2") and remove stale `images/step-*.png` the guide no longer has. HTML/PDF: same check for an existing file. Add an Open folder button to the success toast. Effort S to M. Risk low.

**P1-5. Keep sensitive OCR text out of titles, and make "export anyway" an explicit choice.** Problem: G-GD-10, G-GD-13 redaction. Evidence: `titles.rs:18-36`, `session.rs:201-215`. Change: run the same detectors as `suggest_redactions` on the candidate label; if it matches (email, card, key, phone, ...), use `Click here` (or the kind: `Click the email field`). Strip titles from the `guide.step_added` event (send only `has_proposals`), or keep them only for the in-app bus. In the export sheet, when steps have proposals, make the primary button "Review step n" and the secondary "Export without blurs"; the toast then says "n steps exported without blurs". For agents: `export_guide` with pending proposals returns `invalid_input` unless `allow_unreviewed: true`. Effort M. Risk medium (a new flag in the tool schema; spec 02 and tests).

**P1-6. Follow the window, not its first rectangle.** Problem: G-GD-6. Evidence: `guide/clicks/sys.rs:95-102`, `cm-cursor/target.rs` comment. Change: on each click, re-read the followed window's bounds (the watcher already lists targets every 500 ms; make it update a shared rect) and map click and `outside` with the current rect; if the size changed since the last step, note it in the step. Skip a click when another window is on top at that point (`capturer.window_at(point) != followed` where the platform allows), and say so in the bar's second line ("Ignored: another window is on top"). Until built, document "keep the window where it is" in the picker and bar copy (see P2-11). Effort M to L (platform listing differs). Risk medium.

**P1-7. Agents may not overwrite arbitrary files with `export_guide`.** Problem: G-GD-13 destination. Evidence: `export.rs:343, 357-359, 389-392`. Change: for `Actor::Agent`, refuse an existing file as `dest` (and refuse writing outside an existing directory the user has used: library, a destination folder in Settings, or the home Downloads/Documents/Desktop) with `invalid_input` and a reason; make writing outside those a Confirm with the exact path shown. Humans keep the sheet. Effort S to M. Risk low. Spec 02 line for `export_guide`: "writes only to the chosen folder" becomes true.

### P2: should fix

**P2-1. Quit warns about a running Long Shot or Click Guide.** G-LS-2, G-GD-7. Add them to `quit_wait` (new `QuitWait::LongShot`, `::Guide`): for a guide, stop it cleanly (worker drained, saved, poster) and quit; for a Long Shot, stop it and keep the image (toast "Long Shot saved before closing"), bounded at 10 s like recordings. Add a startup pass: a `.cmguide` with 0 steps and no recent write is removed; one with steps gets a toast "Recovered a Click Guide: n steps". Effort M.

**P2-2. Bound the frame queue and memory.** G-GD-4, G-LS-15. Use `sync_channel(4)` in the guide sink and, when full, drop the click with a visible "Too fast: wait for the step to save" hint (or coalesce); encode PNG in the sink thread only after a size check; for Long Shot compute the preview thumbnail incrementally (append the new rows) instead of resizing the whole image every stitch; refuse to Copy a Long Shot over 25 MP to the clipboard and offer "Save as" with a notice; stop Hand/Auto at 40 MP with a warning ("Wide windows stop earlier than 16,384 px"). Effort M.

**P2-3. One set of start rules, enforced in the daemon.** G-LS-8, G-GD-8. Daemon returns `busy` (`reason: recording_running | long_shot_running | guide_running`) for `start_guide` and Long Shot when the other mode is active, and for a second Long Shot; every entry point (tray, menu, library header, CLI, MCP) shows the same reason as the sidebar tile. `request_id` required for Long Shot state, generated by the daemon when missing and returned. The silent no-op for "already open" becomes a toast "A Long Shot is already open" with Bring to front. Effort M.

**P2-4. Exclude all CaptureMann surfaces from click capture, not only the bar.** G-GD-9. Register the rectangles of every CaptureMann window on the followed display (HUDs, toasts, dialogs, delay pill, main window) as exclusions and refresh them on open/close; or drop a click if `own_window_rects()` (already exists, `capturer.rs:275`) contains it. Effort S to M.

**P2-5. Keyboard and assistive paths.** G-GD-5, G-LS-5, G-LS-9. (a) A tray item and launcher command "Stop Click Guide" and "Stop Long Shot" shown only while running; (b) Setting for a Click Guide hotkey that toggles start/stop; (c) Long Shot overlay bar: Tab/arrow to reach Auto/Hand and Vertical/Horizontal, Space to toggle, with spoken names through the platform's accessibility API (macOS NSAccessibility on the overlay window; Windows UIA), or at least `A`/`H`, `V`/`⇧H` keys shown as keycaps; (d) an option "Esc stops Long Shot" with default on but a way to use another key (Settings), and release of Esc ref-counted per feature. Effort L overall, S for (a). Design request: key hints on the Long Shot bar, a Stop affordance in the tray menu.

**P2-6. Tell the person what Auto does to the mouse.** G-LS-3. HUD second line in Auto: "Hands off: scrolling for you" for the first 3 frames, then size; the confirm dialog for an agent names the effect: "This moves your pointer and scrolls {window}." Raise the target before the first scroll on Windows and X11 (macOS needs care with focus); pause the run (not fail) when the pointer is moved by the user (compare the parked point to the actual position) and show "Mouse moved: put it back or press Stop". Effort M. Risk medium (focus stealing).

**P2-7. Retake: say what it does and protect the work.** G-GD-12. Rename to "Retake picture" with a one-line explanation in the sheet ("Replaces this picture with the window as it is now. Your annotations on this step are lost; Undo brings them back"), disable it for a display guide when the editor is on that display or exclude the editor window from the capture (`own windows` excluded), and keep the old annotations (re-apply the step's ops except the ring if the click geometry matches). Effort M.

**P2-8. Autosave and commit on blur or leave.** G-GD-12. Commit title, intro, step title and note on blur, on selecting another row, on Back and on quit; show "Saved" quietly. Effort S.

**P2-9. Markdown escaping and file names.** G-GD-13. Escape `\ ` * _ [ ] ( ) # > | !` and backticks, and a leading `-`/digits+`.`, in `md_inline`, notes and intro (as plain paragraphs: either escape or put notes in a block that is not parsed as links); set `<html lang>` from the guide's language (or the UI language); `file_stem`: cut to 120 chars, prefix reserved Windows names, strip trailing spaces. Effort S.

**P2-10. Rich text and clipboard size.** G-GD-13. Cap rich text at about 20 steps or 8 MB with a message and a Markdown/HTML alternative; do not return `html` in the tool result unless asked (`include_html`); the agent clipboard write becomes Confirm ("This replaces your clipboard"). Effort S.

**P2-11. Copy and design for window following and ignored clicks.** G-GD-6, G-GD-11, G-GD-17. (a) Picker line: "Keep the window where it is; if you move it, move it back or start again." until P1-6 ships. (b) The bar counts ignored clicks and shows a subtle "3 clicks outside the window were not recorded" on its second line for 3 s after one; (c) the step toast and click ring of S3 on screen. Design requests: ignored-click state of the bar; a popup/menu rule (are menus separate windows that follow?). Effort S for (a), M for (b).

**P2-12. HUD failure is visible.** G-LS-7. If the HUD cannot be placed, toast once "Long Shot is running without a toolbar. Press Esc to stop." and open a fallback HUD on the primary display. Effort S.

**P2-13. Display guide follows the pointer's display at start only.** G-GD-11, from `pointer_display` in `hud.rs:341`. Show the display name on the bar's second line for display guides ("Display 2 · clicks only"), and ignored-click hint when clicks land on another display. Effort S.

**P2-14. Long Shot scroll area: pick the pane.** G-LS-4. Implement the board's "scrollable area under the pointer" (accessibility tree on macOS and UIA on Windows give scroll areas; fallback: the window), and when the wheel moved nothing at one point try another pane; the toast for `nothing_scrolled` should name the causes ("Another window may be on top, or the pointer was moved. Try Hand scroll or drag a region."). Effort L.

**P2-15. Agent confirm texts.** G-GD-2, G-LS-18. Dialog text must say what is affected and that the whole display is captured: "Capture {n} screenshots of {Display 1 / window name} each time you click, until you press Stop." and for Long Shot "Move your pointer and scroll {target}". Effort S.

**P2-16. Reliability of Auto on repeated text and terminals** (G-LS-17, reported open). Keep the P2 flag; the honest fix is the toast text (P2-14) plus offering Hand scroll in the same toast (already for `no_overlap`, add for `nothing_scrolled`). Effort S for the toast.

### P3: later

- **P3-1.** Library card title and provenance for Long Shot (window title, app, `scale` of the window) (G-LS-14).
- **P3-2.** Correct limit wording for 50 MP and user limits; a progress bar "x of 16,384 px" on the HUD as in the board (G-LS-12).
- **P3-3.** A Discard action on the Long Shot toast for runs with 1 frame; do not store `nothing_scrolled` at all unless the user asks (G-LS-13).
- **P3-4.** CLI `guide watch` and `capture watch-long` event streams (cross-cutting).
- **P3-5.** PDF: fonts with CJK/emoji/RTL fallback or a clear message when glyphs are missing (G-GD-14); HTML lang.
- **P3-6.** Hand scroll "End of page" via the scroller bar position when AX gives it (G-LS-10).
- **P3-7.** Library: guides in Select/Bulk/Boards/upload/publish: define behaviour and tests (G-GD-15).
- **P3-8.** An option "Pause on password fields" for Click Guide (macOS secure input, Windows password edit) so no frame is taken on a password click; also a list of apps never recorded (password managers).
- **P3-9.** Drag grip accessible names and list roles in the guide editor; VoiceOver pass.
- **P3-10.** Vietnamese strings reviewed by a native speaker (fuzzy drafts, "Tay" for Hand).
- **P3-11.** Detect a disk-full risk before a long run (needs about 2 x 200 MB free) and say so.

Counts: P1 7, P2 16, P3 11.

---

## D. Live-QA script (final bundle)

### D0. How to observe things while the windows are excluded from capture

| Surface | Method |
| --- | --- |
| Click Guide bar, toasts, confirm dialogs, pickers (macOS) | Accessibility tree: `System Events`, process `cm-desktop`, under a "system dialog" window (found in pass 3): read button names, text, enabled state; measure the window rectangle with `CGWindowListCopyWindowInfo` (owner `cm-desktop`/`CaptureMann overlay`, layer, bounds). |
| Same on Windows | UI Automation (invoke buttons, read names, `BoundingRectangle`); physical px, logical = physical / scale. |
| Linux | AT-SPI (`accerciser` or `dogtail`), xdotool/xwininfo for rectangles. |
| Long Shot overlay, preview panel, HUD contents | Not readable by AX (reported). Measure the window rectangles; read the daemon: `capturemann` JSON, the stored manifest, and the model values through `long_shot_state` (internal: use the socket directly). Take a **phone photo or an external capture device pointed at the screen** for the HUD text if a visual check is needed (the app cannot see itself). State "NOT SEEN" otherwise. |
| Daemon events | No CLI watch for `guide.*` or `capture.long_shot.*`: open the daemon socket (`CAPTUREMANN_SOCKET`) and subscribe through JSON-RPC (not determined how the subscribe call is named in the CLI-less case; ask the maintainer) or run the daemon in the debug harness. Say "events not observed" when you cannot. |
| Files | `manifest.json` (`kind`, `source: long_shot`, `frames`, `long_shot_end`), `guide.json` (steps, titles, `click`, `has_proposals`, `schema`), `steps/*.cmshot/`, `.undo/`, `thumbs/poster`; image size via `sips -g pixelWidth` / PowerShell `System.Drawing`; `capturemann text <id>` for OCR of a result. |
| CLI | `capturemann capture screenshot --long [--window ID|--region] [--horizontal] [--max-px N] [--hand]`, `capturemann guide start|stop|get|set|export`, `capturemann list --kind guide`, `capturemann text`. |
| Process | `ps`/Activity Monitor/Task Manager for RSS during big runs; disk free with `df`. |
| Logs | App log (log file path in Settings > About or the data dir): check that no title or text appears (`grep` the log for a test email). |

Test data: a throw-away library (`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET`), the fixture page `tests/fixtures/long-shot/long-page.html` (`screens=5`, `bands=1`, `jump=1`, `screens=20/30`), an AppKit/WinForms/GTK fixture window with 5 buttons plus a "Contact: jane.doe@example.com" label and a card number `4242 4242 4242 4242`, a Terminal with 250 numbered lines, a 120-paragraph page of repeated identical paragraphs, a 40-column 12,000 px page. Private data: none on screen; never the maintainer's apps.

What to record for every case: system, build hash, display (resolution, scale), result PASS/FAIL/NOT SEEN, evidence path, and for each defect a one-line repro. Report in `verification/` as PARTIAL (template in its README).

### D1. Long Shot cases

| # | Case | Setup | Steps | Expected | How to observe |
| --- | --- | --- | --- | --- | --- |
| L01 | Auto, unique text | Chrome fixture 5 screens | Sidebar tile, click the window, Start | Image with every number once, in order; `reached_end` | manifest `long_shot_end`, `frames`; `capturemann text`; size |
| L02 | Auto, fixed header/footer | `bands=1` | as L01 | Header once at top, footer once at bottom; HUD tags "Header kept once" | image strips; HUD NOT SEEN, tags by photo |
| L03 | Auto, jump | `jump=1` | as L01 | Warning toast "Stopped where the page jumped" [Open, Try Hand scroll]; frames before kept | toast text via AX; manifest `no_overlap` |
| L04 | Size limit | `screens=30` | as L01 | 16,384 px, toast "saved at the size limit"; text exact | AX toast; image height |
| L05 | 50 MP limit | wide window (>3,200 px) on a tall page | as L01 | Stops at 50 MP; toast must not say 16,384 if the image is shorter | image size; **expect FAIL on wording (G-LS-12)** |
| L06 | User limit | CLI `--max-px 2000` | run | 2,000 px; toast wording | CLI output, toast |
| L07 | Hand scroll, steady | fixture | Hand, Start, 12 wheel steps 500 ms apart, Stop | all rows once, frames = 12 to 13 | manifest, OCR |
| L08 | Hand, slow continuous | wheel steps every 250 ms for 15 s | as L07 | several frames (not 1); counter ticks (by photo) | manifest `frames` |
| L09 | Hand, flick past the overlap | fast scroll then stop | | amber dot, "Scroll back up a little", toast "The last part didn't line up" [Stop and keep]; scrolling back clears it | AX toast; manifest after Stop and keep |
| L10 | Hand, Esc | Hand run, press Esc | | image kept, toast "Stopped with Esc. Everything up to here is kept: n frames, w x h." | AX toast text |
| L11 | Pause and Resume | Auto run, press Pause for 5 s, Resume | | count frozen while paused; run continues | `long_shot_state` seq/frames; manifest |
| L12 | Pause then scroll by hand 2 screens, Resume (Hand) | | | next frame left out, "Scroll back up a little"; Stop keeps before | state `join_ok` |
| L13 | Stop button | Auto run, press Stop via AX/UIA | | image kept; `stopped_by_user` (Windows: report `no_overlap` if seen) | manifest |
| L14 | Horizontal | 40-col page | Horizontal, Start | columns once in order | OCR of labels |
| L15 | Horizontal whole browser window | | | toolbar cut as static strip; one image | image |
| L16 | Region drag | drag a region | Enter | area = region | manifest size |
| L17 | Window under pointer highlight | hover | | chip "Scrollable area", size, scale | NOT SEEN (overlay); measure overlay window |
| L18 | Accessibility missing (macOS) | remove permission | Auto, Start | card "Auto scroll needs Accessibility" [Grant access, Use Hand scroll]; no start | measure overlay; NOT SEEN content; CLI: `permission_missing` |
| L19 | Wayland | Wayland session | open Long Shot | one inert "Hand scroll" label; CLI Auto `not_implemented`; note prompt per frame | CLI, photo |
| L20 | CLI Auto | `capture screenshot --long --window ID` | confirm dialog Allow | passive HUD (Stop, count, Agent chip) 340x56; image returned | AX dialog; window rect |
| L21 | MCP Hand refused | `scroll:"hand"` | | `invalid_input`, `data.reason` `needs_user` | MCP output |
| L22 | Agent denied | Cancel the dialog | | no scrolling, no capture, clear error | CLI exit code |
| L23 | Double start (app) | run in progress | press hotkey, click tile again, launcher | no second overlay; observe if any feedback (expect none: G-LS-8) | window list |
| L24 | Double start (CLI x2) | two CLI Long Shots on two windows | | document behaviour (expect both run); Esc stops both | manifests |
| L25 | Same `request_id` twice | CLI `--request-id X` twice | | document; state overwrite | `long_shot_state` |
| L26 | Recording running + Long Shot | start a short screen recording (consent), start Long Shot | | document: HUD not in recording; image ok; pointer moves in recording | recording file frames |
| L27 | Guide running + Long Shot | guide on display, Long Shot overlay pick | | expect steps from the overlay click (G-GD-9) | `guide get` |
| L28 | Quit during run | Auto or Hand run; Cmd+Q / tray Quit | | expect image lost (G-LS-2); record whether a warning appears | library after relaunch |
| L29 | kill -9 during run | `kill -9 cm-desktop` | | nothing stored; relaunch clean; no half capture | library list |
| L30 | Target window closes mid-run | close the browser window during Auto, and during Hand | | expect run lost or kept (G-LS-1); record the toast | toast, library |
| L31 | Display disconnected / change resolution mid-run | unplug or change scaling | | same | |
| L32 | Sleep/wake mid-run | `pmset sleepnow` / lid, wake | | document | manifest |
| L33 | Lock screen mid-run | lock for 10 s | | document | |
| L34 | Window moves/resizes mid-run | drag the window during Hand and Auto | | document; frame size changes: refused? "frame of another size" | debug dump |
| L35 | Other window covers the target | put a console/Finder over the centre, Auto | | `nothing_scrolled` or success; wording of the toast | toast |
| L36 | User moves the mouse during Auto | wiggle for 3 s | | document (G-LS-3) | manifest |
| L37 | Full-screen app target | full-screen Chrome | | overlay and HUD visible? | measure windows |
| L38 | Two displays: window on the second, area spanning displays | | | HUD on the right display, preview side | window rects |
| L39 | HiDPI: 2x Mac, 150 % Windows | | | HUD size rules S20 (max 640, display less 48); stored image scale | window rect; manifest scale (expect ONE: G-LS-14) |
| L40 | Esc in the scrolled app | terminal with vim, Hand run, press Esc | | run ends (Esc swallowed from vim) | manifest |
| L41 | Esc ref-count | start a delayed capture pill, then a Long Shot, end the Long Shot, press Esc | | does Esc still cancel the pill? | |
| L42 | 16 GB Mac memory | 50 MP run | | peak RSS, time per frame near the end, UI lag | Activity Monitor |
| L43 | Disk nearly full | fill disk to under 300 MB, run to the limit | | error path; image saved or lost | toast, library |
| L44 | Copy of a 16,384 px image | toast Copy, paste into Preview/Mail/Slack | | pastes or fails; time | |
| L45 | Open in editor | Open | | editor loads 16,384 px; annotate, redact, export PNG | |
| L46 | Terminal 250 lines, repeated paragraphs, 12,000 px columns | | | record known open failures | manifest |
| L47 | Vietnamese UI | set language; every state | | strings fit; "Tay" / "Tự động" | AX, window rect |
| L48 | Dark/light page content | white page and dark page | | HUD readable | photo |
| L49 | Debug dump privacy | `CM_LONG_SHOT_DEBUG_DIR` set | | files hold screen content; no upload | dir listing |
| L50 | Log privacy | any run | | `grep` log for page text | log |

### D2. Click Guide cases

| # | Case | Setup | Steps | Expected | How to observe |
| --- | --- | --- | --- | --- | --- |
| G01 | Start from sidebar, launcher, tray, menu, library header, overlay chip | permission granted | each entry, Stop at once | bar opens; "No steps were taken" toast; nothing stored | AX bar, `list --kind guide` |
| G02 | Input Monitoring missing | revoke | start | card "Click Guide needs Input Monitoring" [Grant access, Cancel]; no bar; no guide folder | AX card, library |
| G03 | Grant while the card is open | | grant | card closes by itself in ~1 s and the guide starts | AX |
| G04 | Countdown cancel | start, press Esc or Stop in 3 s | | no guide stored; Esc works; clicks during countdown add nothing | `list` |
| G05 | 5 buttons | fixture window, display guide | click 5 controls | 5 steps in order, titles `Click "Name"`, ring within 2 px, badge upper-left | `guide get`, step image |
| G06 | Window guide | picker, Start guide | click inside and outside | only inside; popups/menus: record | `guide get` |
| G07 | Pause, 3 clicks, Resume, 1 click | | | 1 step | `guide get` |
| G08 | Click on bar (centre, ends); on 125/150 % and second monitor | Windows | | no step | step count |
| G09 | Click on a toast, the recording HUD, the Long Shot overlay, CaptureMann main window | display guide | | expect steps (G-GD-9) | `guide get` |
| G10 | Double click, right click, middle click | | | title `Double-click` etc.; single step | titles |
| G11 | Spam clicks | 20 clicks in 2 s on a 4K/5K display | | counter, memory, all steps present? order; app responsive | RSS, `guide get`, events |
| G12 | Hold the mouse (long press), drag | | | one step at mouse-down | |
| G13 | Note step, Undo step from bar | | | counts +1/-1; undo with 0 steps does nothing | AX count, `guide get` |
| G14 | Stop with 0 steps / 1 step / note only | | | discarded / kept / note-only guide kept or discarded (record) | library |
| G15 | 100 steps | scripted clicks (xdotool/cliclick) | | list scrolls, editor responsive, export ok, memory | RSS, time |
| G16 | 500 and 501 steps | scripted | | cap at 500; **expect silent loss of click 501** (G-GD-3) | `guide get`, UI |
| G17 | Disk full during a guide | fill disk | click | **expect silent failure**; record | `guide.error` event if observable |
| G18 | Window closes | window guide, close it | | within ~1.5 s paused, `window_closed`, toast, bar swap, resume refused | AX, `guide get`, `guide set --resume` |
| G19 | Toast "Stop and open guide" and "Pick another window" | | | bar closes, editor opens / picker opens (C2 regression) | AX, window list |
| G20 | Window moves while followed | drag the window 300 px, click | | record steps (expect wrong or missing: G-GD-6) | `guide get`, step image vs click |
| G21 | Window resized while followed | | | same | |
| G22 | Window covered by another app | open Notes over half the window, click in the covered area | | record | |
| G23 | Display guide: click on the second display | two displays | | ignored silently (record) | `guide get` |
| G24 | Display disconnected / scale change mid-guide | | | record | |
| G25 | Sleep/wake, lock/unlock | | | record whether steps resume | |
| G26 | Full-screen app target, Spaces switch | | | bar visible? Stop reachable? | window rect |
| G27 | Quit during recording | Cmd+Q / tray Quit | | steps saved? empty guide left? relaunch state | library, `guide get` |
| G28 | kill -9 during a guide | | | relaunch: guide listed with steps taken; no orphan step folders; no lock | folder listing, `steps/` vs `guide.json` |
| G29 | kill -9 during the countdown | | | empty guide folder? (expect left over) | library |
| G30 | Second start | click New Click Guide while running; CLI start; library header | | silent no-op in app; `busy` in CLI | AX, CLI |
| G31 | Start during a recording from each entry | | | sidebar/launcher disabled; tray/menu/header allowed (G-GD-8) | AX |
| G32 | Start during a Long Shot | | | same | |
| G33 | Agent start | MCP `start_guide` (display, window) | Allow in dialog | dialog text; **bar visible? Stop reachable? (expect none: G-GD-1)** | AX windows list, `guide get` |
| G34 | Agent start, deny | | | no guide folder | |
| G35 | CLI start/stop | `guide start`, click, `guide stop` | | same as G33 | |
| G36 | Titles: password-like label | label near the click: "Password", a revealed secret, an email, a card number, a token `sk-ant-...` | click each | titles contain the text (expect yes: G-GD-10); proposals flagged | `guide get` |
| G37 | Title privacy surface sweep | after G36 | | grep titles in: `guide.json`, library index/search, events, logs, Markdown, HTML, PDF text, clipboard text, MCP `get_guide`, thumbnails | greps |
| G38 | Redaction proposals + export | step with email | accept one blur, leave one pending, export MD/HTML/PDF | blurred step shows blur (OCR finds nothing); pending step unblurred; sheet names it; title still has the email | `capturemann text` on images, PDF text extract |
| G39 | Retake | window guide, edit a step (callout, blur), Retake | | picture replaced; annotations lost; undo restores; display guide retake captures what? (editor on top?) | step image, `.undo/` |
| G40 | Edit flow | reorder by drag, Move up/down, delete, duplicate, note step, focus crop, numbering A B C, undo/redo each | | numbering and titles follow; `.undo` emptied when history drops; no orphan in `steps/` | folder listing |
| G41 | Uncommitted text | type in step title, click Back (no Enter); type, quit | | lost? (G-GD-12) | `guide get` |
| G42 | Very long title (2,000 chars), 255-byte file name | set title | export HTML/PDF/MD | write succeeds or error text; PDF wraps | files |
| G43 | Emoji / RTL / CJK window title and step titles | window titled `测试 😀 مرحبا`; set step titles | follow, export all | bar name cut at 28 chars with tooltip; HTML ok; PDF glyphs (expect tofu); MD ok; file name | files, PDF text |
| G44 | Vietnamese UI and titles | | | bar 672 wide; collapse order; export Vietnamese titles; `lang` | AX, HTML |
| G45 | Markdown/HTML escaping | titles/notes: `<script>alert(1)</script>`, `` `code` ``, `[x](https://example.com)`, `![i](https://example.com/p.png)`, `# h`, `| a |`, `*b*`, `\`, `&amp;` | export MD (render on GitHub or `markdown-it`), HTML (open, check no script runs, no external request), rich text | HTML escaped; Markdown must show literal text; **expect link/image injection (G-GD-13)** | render, network log |
| G46 | Markdown folder reuse | export guide A then guide B into the same folder | | **expect overwrite** (G-GD-13) | file list |
| G47 | Export to existing file / read-only folder / full disk / folder that disappeared | | | error shown in the sheet, no partial files left | files |
| G48 | HTML offline, dark and print | | | images embedded, TOC, dark style, print preview breaks | browser |
| G49 | PDF page breaks | 1, 2 and 100 steps; A4 and Letter; tall step picture (portrait window) | | heading stays with picture, no clipped picture, page count | PDF viewer |
| G50 | Rich text paste | Mail, Notes, Google Docs (Chrome), Word, Notion, Slack, GitHub issue | paste | text + images; size; time; which apps drop images | per app |
| G51 | Rich text with 100 steps at 2x | | | clipboard size (MB), paste time, app freeze | Activity Monitor |
| G52 | Agent export | MCP `export_guide` to a path with an extension that exists (a scratch `.docx`), `rich_text` | | **expect overwrite / full HTML in result / clipboard replaced (P1-7, P2-10)** | file, MCP result size |
| G53 | Agent `set_guide` text from hostile OCR | set a title with Markdown link | export | see G45 | |
| G54 | Library | guide card, GUIDE badge, "No preview" for an empty one, sort, chips, search by step title, delete, rename, move library, bulk Export selected / Make a board with a guide selected, upload to a destination | | each works or says why not (G-GD-15) | AX, files |
| G55 | A 0.3.3 app opens the library | | | ignores `.cmguide`, no damage | |
| G56 | Accessibility | VoiceOver / Narrator through bar, editor, export sheet, tiles; keyboard only start to export | | every control named; Stop reachable? (expect not: G-GD-5); focus order | AX dump |
| G57 | Disabled tile reason | hover/keyboard on a disabled tile | | tooltip text equals the launcher reason | AX |
| G58 | Wayland | session with no click events | | all entries disabled with reason; CLI `not_implemented`; Long Shot Hand only | AX, CLI |
| G59 | Windows: elevated app, UWP, game window | click inside | | record (UIPI) | `guide get` |
| G60 | Time-in-title | two guides within a minute | | default titles collide? | library |

Reporting: one table per feature in the final report, with the case number, result, and the file or screenshot path; the defects above that are expected to fail (L05, L23, L24, L27, L28, L30, L35, L40, G09, G16, G17, G20, G23, G27, G29, G31 to G33, G36 to G38, G41, G43, G45, G46, G52) are written as "known gap, see audit G-xx" when they fail as predicted, and as "new" when the result differs.

---

## E. Not determined from reading

- Whether the daemon's event bus is forwarded to MCP/CLI clients (affects the title leak in `guide.step_added`).
- Capture latency after mouse-down on each system and whether frames show the pre-click state (spike S3 results are not in the files read).
- Whether the window capture includes child popups and menus on each OS.
- Bar visibility over full-screen apps and other Spaces (`level` is not set for the guide bar).
- Orphan handling after a crash between a step bundle write and `guide.json` (GDE-2.AC2 covers deletes, not crashes).
- Whether `StorageFull` is mapped on the screenshot store path for Long Shot (it is for guide export).
- Library card title and search behaviour for Long Shot and guide step titles; bulk and upload behaviour for guides.
- Whether the Esc id sharing between the delay pill, countdown and Long Shot loses a grab in real use.
- Daemon socket access rules beyond file permissions (the `long_shot_state` preview pull).
- Tray item for Long Shot.
- Everything on screen that QA could not see (Long Shot bar, HUD text, preview, overlay, countdown text).
