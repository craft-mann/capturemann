# Linux parity matrix for 0.3.0

Status: **audit, Nov 8, 2026** (read-only pass over the code, the specs, the plans and the stored Linux reports; nothing
built, nothing run). Input to task 7 ("parity sweep") of [`release-0.3.0-plan.md`](release-0.3.0-plan.md) and the
"no longer an early build" statement the release makes (decision O5).

How to read it. One row per user-facing requirement (the FR / AC areas of `specs/features/*.md`, plus the product
surfaces that have no ID: window frame, tray, Settings pages, packaging and update). The Windows and macOS columns are
the state the 0.2.8 and 0.2.9 reports give (`verification/L1-windows-0.2.9-*`, `L1-macos-0.2.9-*`, both PARTIAL with
0 failures), shortened to **works** (passed in a stored regression or hardware report), **built** (tested with fakes,
not in a stored report), **partial**, **missing**, or **n/a** (not built on any system). The Linux column is the state
of the code on `main` at `45ef59b1` plus the stored Linux reports (the ThinkPad T490, Ubuntu 24.04, GNOME 46, X11 and
Wayland, Intel UHD 620; the aarch64 VM; CI). Nothing in this file is "verified": every Linux cell that says "works"
rests on a PARTIAL report or on CI, and the report is named in the Evidence column.

Columns: **Task** is the 0.3.0 task (1 to 11) that owns the gap, or **NEW-x** (see "Proposed new tasks"); **Size** is
S (a day), M (two to four days), L (a week or more); **HW** says which real hardware the check needs (none, screen,
mic, camera, sound, GPU, 2 monitors, KDE, ARM).

Rules kept: no "verified" without a report; IDs are not renumbered; a cell that names a file is a claim checked
against that file on Nov 8, 2026 (see "What I could not verify" for the rest).

## 1. Screenshots, annotation, clipboard, launcher

| Feature | Windows | macOS | Linux today | Evidence | What Linux needs | Task | Size | HW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CAP-1.AC1 display screenshot | works | works | **works** on X11 (`xcap`) and Wayland (portal Screenshot; monitors from Mutter D-Bus, one "Desktop" display on other compositors) | `crates/cm-capture/src/capturer.rs:179` (portal when `is_wayland()`), `sys/portal.rs`, `sys/mutter.rs`; `verification/LINUX-1-x11-thinkpad-2026-11-02`, `LINUX-1-wayland-thinkpad-2026-11-02`, `L1-linux-0.2.9-regression-hardware` (`shot.display` PASS) | KDE and wlroots monitor layout (today one "Desktop" display); a second monitor at its own scale | 10 | M | 2 monitors, KDE |
| CAP-1.AC2 region screenshot | works | works | **works** (X11 overlay and crop; Wayland overlay over the frozen portal picture) | `shot.region` PASS; Wayland step 14 PASS (maintainer at the screen) | fractional scale and HiDPI on Wayland not seen | 10 | S | screen |
| CAP-1.AC4 window screenshot | works | works | **works on X11** (titled windows, `_DESKTOP`/`_DOCK` left out); **missing on Wayland GNOME 46** (no window ids; `--pick` opens GNOME's chooser, which saves its own file and answers response code 2, so the app gets nothing and says `user_declined`) | `sys/x11_names.rs`, `sys/x11_kind.rs`; X11 step 12/14b PASS; Wayland step 14a **FAIL (desktop)**; `cm-regress` `shot.window` is `WIN_MAC` only (`crates/cm-regress/src/cases/*.rs`) | Task 4 experiment (ScreenCast portal window source for one frame, or reading the file the shell saved); the error for code 2 must say "the desktop's picker failed", not "declined" (task 2); `shot.window` enabled on Linux X11 | 4, 2, NEW-F | L | screen |
| CAP-1 own windows left out of the capture | works (`WDA_EXCLUDEFROMCAPTURE`) | works (macOS 14+) | **partial**: no per-window exclusion exists; the app hides its toast stack for ~1.5 s (`capture_cloak`); the library window stays in the picture on purpose; the Wayland pixel check was inconclusive | `apps/desktop/src/capture_cloak.rs`, `platform_window.rs:639`; `L1-windows-linux-2026-10-31` | a pixel check on a real Wayland desktop; the release text names the limit | 7 | S | screen |
| CAP-2 global hotkeys | works (`RegisterHotKey`) | works | **works on X11** (XGrabKey); **missing on GNOME 46 Wayland** (no GlobalShortcuts portal); the portal path (KDE, newer GNOME) is written and tested only against a fake portal; fallback = a desktop custom shortcut running `cm-desktop --launcher` / `--capture region\|window\|display` | `apps/desktop/src/platform_hotkeys.rs` (header), `instance.rs:97-110`; X11 step 15a PASS; Wayland step 15c PASS (the page says "Not available on this desktop") | one KDE machine or VM run of the portal path; the Shortcuts page says what works instead (bug in task 2: the Wayland page shows Recording rows and Change buttons with recording off) | 6, 2 | M | KDE |
| CAP-2 tray icon, recent captures, recording variant | works | works | **partial**: StatusNotifierItem served over D-Bus with `zbus` (no GTK); seen in GNOME's top bar with the AppIndicator extension; stock GNOME has no watcher (toast + launcher window as fallback); the delay check mark did not move in GNOME's bar and "Recent captures" showed "No captures yet" with seven in the library; light panels get the dark icon | `apps/desktop/src/platform_linux.rs`, `platform_linux_menu.rs`, `platform.rs:478-503`; X11 steps 19 PASS, 20 PARTIAL; `linux.tray_icon` registered (the case failed on "left programs running", a suite problem) | the two tray bugs (task 2); a light-panel icon (the SNI `IconThemePath` or a light pixmap); KDE panel run | 2, 6 | M | KDE |
| CAP-2 Launch at login | works (installed copies, `Run` key) | works (LaunchAgent) | **built**: XDG autostart `.desktop` under `~/.config/autostart`, starts the app with `--background`; tested with fakes, never seen on a desktop | `apps/desktop/src/login_item.rs:63-90, 152-172` | one real login on the T490 (X11 and Wayland) | 7 | S | screen |
| CAP-2 "Replace system screenshot shortcuts" | n/a | n/a (not built) | n/a | spec status line | nothing | — | — | — |
| CAP-3 magnifier, coordinates, size pill, nudging | works | works | **works on X11** (seen: size, "1x native", the magnifier); Wayland region mode runs the same overlay over the frozen picture (no window highlight, Wayland cannot list windows) | X11 step 11 PASS; `platform-acceptance.md` CAP-3 row | a Wayland run at scale 2 and fractional scale | 10 | S | screen |
| CAP-4 clipboard after capture | works (PNG + DIB) | works (PNG + TIFF) | **works**: `arboard` X11 selection kept for the app's life; `wayland-data-control` for wlroots/KDE; GNOME through XWayland; `wl-copy`/`xclip` when no clipboard can be reached; the headless daemon says it has no clipboard | `apps/desktop/src/platform_clipboard.rs`, `apps/overlay/src/platform_clipboard.rs`; X11 step 15 PASS (CLI); Wayland step 15 PASS (paste into Firefox, maintainer) | paste of an edited image (editor Copy) on both sessions; KDE data-control path never run | 7 | S | screen, KDE |
| CAP-4 "Also save a PNG to a folder" | built | built | **built, shared**; not seen on Linux | `apps/desktop/src/save_folder.rs` (shared) | one run | 7 | S | none |
| CAP-5 delayed capture (daemon, CLI, tray, corner pill) | works | works | **partial**: daemon and CLI shared; the tray delay toggle stored the setting but GNOME's bar did not redraw the check mark | X11 step 20 PARTIAL | task 2 bug; a `desktop.settings_applied` equivalent in `cm-regress` for Linux (today `WIN` only) | 2, NEW-F | S | screen |
| CAP-6 scrolling capture | n/a (Draft) | n/a | n/a | spec | nothing | — | — | — |
| ANN-1 drawing tools, text with IME | works | works | **works** (driven in the VM on X11 and GNOME Wayland: arrow, rect, text, highlight, blur, crop, undo/redo, copy, Save as; IBus/Telex on X11); the maintainer used the editor on the T490 X11 "and said it worked", the single steps were not listed | `verification/L1-editor-linux-2026-10-31`; X11 step 6 PARTIAL | the editor steps one by one with a label (task 2 names it); `tests/manual/ANN-1-IME.md` on Linux | 2 | S | screen |
| ANN-2.AC1–AC3 blur and pixelate | works | works | **works** (shared renderer, goldens in CI) | `crates/cm-render` tests (CI on ubuntu-24.04) | nothing | — | — | — |
| ANN-2.AC4 Auto-redact (recognizer) | works (`Windows.Media.Ocr`) | works (Apple Vision) | **partial**: the system `tesseract` program as a helper (not bundled; the `.deb` recommends `tesseract-ocr`); the editor button is dimmed with the install command when missing; recall below Vision on the synthetic dev split (84.5 % vs 95.3 %); x86_64 run only through `redact.fixture` | `crates/cm-render/src/ocr/tesseract.rs`; `packaging/linux/build-deb.sh:191-192`; `verification/L1-auto-redact-linux-2026-10-31`, `redact.fixture` PASS (T490) | task 8 re-run; the editor panel on a real desktop; `redact.window_text` enabled on Linux (today `WIN`); decide whether to bundle `tesseract` or keep "recommends" | 8, NEW-F | M | screen |
| ANN-3 undo, redo, revisions | works | works | **works** (shared; `lib.annotate_undo` PASS) | regression report | nothing | — | — | — |
| ANN-4 callouts, stamps | n/a (Draft) | n/a | n/a | spec | nothing | — | — | — |
| LNC-1 launcher | works | works | **partial**: window, matcher, commands built and shared; opened by the X11 hotkey and by `cm-desktop --launcher`; focus return to the previous app is limited on Wayland; the recording rows said "Not available yet" because recording is gated | X11 step 15a PASS; `platform-acceptance.md` LNC-1 | ungating recording (task 3) fixes the rows; a Wayland focus note in the release text | 3, 7 | S | screen |

## 2. Recording, camera, audio, voice

| Feature | Windows | macOS | Linux today | Evidence | What Linux needs | Task | Size | HW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| REC-1 record a display | works (WGC + MF H.264) | works | **partial, hidden**: portal ScreenCast + PipeWire into software VP9 by an FFmpeg child; the compositor's dialog picks the monitor the first time, a restore token (config folder, 0600) after; 10 s at 30 fps in the headless VM was clean; **on the T490 every `record start` was "killed by a signal"** (`rec.screen_cursor`, `rec.mic`, and `mcp.policy_allow` through the CLI), not reproduced or explained; the app's Record entry points are hidden unless `CM_LINUX_RECORDING=1` | `crates/cm-daemon/src/recording/sys/linux.rs:80-130`, `crates/cm-capture/src/sys/pipewire_linux.rs`, `sys/linux_recorder.rs`, `crates/cm-platform/src/capability.rs`; `verification/L1-recording-product-linux-2026-10-31`, `L1-recording-ui-linux-2026-10-31`; `L1-linux-0.2.9-regression-hardware` (FAIL rows) | triage the signal kill first (task 1); remove the gate; the 10-minute gate on X11 and Wayland with the sync measurement | 1, 3 | L | screen, GPU |
| REC-1 record a window | works | works | **missing** (`not_implemented`: "Recording a window or a region is not available on Linux yet") | `crates/cm-capture/src/record/pipewire.rs:205`, `sys/linux.rs:376`; `rec.region` FAIL | portal `SourceType::Window` where the compositor offers it (`portal.window` is already probed in `list_devices`); the UI names the versions that work | 4 | L | screen |
| REC-1 record a region (incl. 18 px minimum) | works | works | **missing** (same error) | same; `rec.region_min` FAIL | crop of the monitor stream (compositor-side crop through the portal where it exists, else a crop in the recorder before the encoder) | 4 | L | screen |
| REC-1 hardware H.264 encoder | works (MF) | works (VideoToolbox) | **missing for recording**: the screen track is hard-wired to `FfmpegVp9Encoder`; `vainfo` on the T490 lists H.264 encode but diagnostics say "software VP9" (`linux.vaapi_encoder` FAIL); the export side already has an `Encoder::Vaapi` policy | `sys/linux.rs:110` (`FfmpegVp9Encoder::create`), `crates/cm-media/src/tools.rs:401-423, 489`; `packaging/ffmpeg/build-linux.sh:57-58` (`h264_vaapi` when libva is present) | a VA-API (then NVENC) variant of the child-process encoder behind `VideoEncoder`, chosen when the device accepts the plan, software VP9 as the labelled fallback (O10); the `hardware_encoder_unavailable` text for Linux | 3 | L | GPU |
| REC-2 resolution up to native, size fallback | works (1080p30 gate; 4K lost frames on UHD 620) | works | **unknown on hardware**: software VP9 was measured at 720p in the VM only; `smaller_plans` exists; no 1080p or 4K figure | `linux-recording-plan.md` §8, R1 | the task 3 gate at the panel's native size; the 4K external case (`rec.external_4k`) with `--allow second_monitor` | 3, 10 | M | screen, 2 monitors |
| REC-3 30/60 fps, health monitor, 60 fps self-test | works | works | **partial**: 60 fps in the VM gave 600 frames at 0.57 core with 27 repeated frames (57 Hz virtual monitor); the self-test routes through the same `SourceFactory`, never run on Linux | `L1-recording-linux-2026-10-31`; `crates/cm-daemon/src/recording/selftest.rs` (shared) | one self-test run through the app (Confirm dialog) on the T490 | 3 | S | screen |
| REC-4 pause, resume, countdown | works | works | **partial**: the session removes the gap (shared); pause and countdown were driven in the headless VM through the app; never on hardware | `L1-recording-ui-linux-2026-10-31` | part of the task 3 gate (pause inside the 10-minute recording, A/V sync after resume measured) | 3 | S | screen, mic |
| REC-5 indicator, HUD, stop control | works (HUD, brackets, draggable/collapsible HUD in 0.2.9) | works | **different by design**: no floating HUD, countdown disc or brackets (they would be in the video and Wayland cannot place them); a banner in the Studio window (timer, pause, mute, discard, stop; countdown with skip and cancel), the tray's red state, and the desktop's own screen-share indicator; X11 may opt into the floating HUD with `CM_LINUX_FLOATING_HUD=1`; the 0.2.9 AC16/AC17 (collapse, display recovery) do not apply to the banner; a desktop with no indicator, no tray host and a hidden Studio window has none of them | `apps/desktop/src/recording/surface.rs:11-23`, `platform_window.rs:89`; `platform-acceptance.md` REC-5 row; `L1-recording-ui-linux-2026-10-31` | task 3 "fix what a real desktop shows" (banner controls, Start sheet keycaps overflow, "Start or stop from anywhere" line); a `cm-regress` banner case for Linux (today `desktop.hud_click_through` is `WIN_MAC`); the release text names the AC5 limit | 3, NEW-F | M | screen |
| REC-6 excluded windows (other apps) | partial (own windows; other apps "to be confirmed") | works | **cannot be done** on Wayland and through the portal on X11; the bubble and HUD are composited at export or shown as a banner instead | `linux-recording-plan.md` §1 "Excluding our own windows"; `crates/cm-capture/src/record/exclude.rs` has no Linux branch | the Record sheet and the release text say so; `exclusion` settings hidden or explained on Linux | 7 | S | none |
| REC-7 cursor telemetry track | works (gate with cursor) | works | **missing**: the pointer is embedded in the pixels (portal `CursorMode::Embedded`), no positions or clicks are logged; `cm-cursor` answers `not_implemented` on Linux; a request for a cursor track records the pointer in the picture and warns `cursor_in_video` | `crates/cm-capture/src/sys/pipewire_linux.rs:182-194`, `crates/cm-cursor/src/platform.rs:11-25`, `sys/linux.rs:47, 363-369, 427-429` | PipeWire cursor **metadata** mode (proven in the L0 spike) into `cm-cursor` (`spa_meta_cursor`: position, hotspot, bitmap), the screen stream switched to `Hidden`; clicks from the metadata are not available, so the click log needs a source (XInput2 raw events on X11; none on Wayland, state it) | 3 | L | screen |
| REC-8 crash-safe recording (`recording.lock`, kill -9) | works | works | **partial**: the lock is `File::try_lock` on every OS (CI); `kill -9` on Linux never run (`rec.kill9_recovery` failed before the start: a private config folder has no restore token, so the portal dialog waited for a person) | `L1-linux-0.2.9-regression-hardware`; plan task 3 | the kill test on the T490; `tests/manual/REC-8.md` (does not exist); the suite needs a way to pass a restore token or run the case with a person | 3, NEW-F | M | screen |
| REC-9 draw on screen while recording | n/a (Draft) | n/a | n/a | spec | nothing | — | — | — |
| CAM-1 webcam bubble while recording | works (MF, 30 fps lit room) | works | **missing**: `cm-camera` has no Linux backend (`unsupported.rs`, `not_implemented`); the Record sheet dims the webcam choice ("Recording with a webcam bubble is not available on Linux yet"); the live bubble window cannot be hidden from capture on Wayland anyway (composited at export is the existing design) | `crates/cm-camera/src/lib.rs:34-38`, `unsupported.rs`, `sys/linux.rs:43, 352, 424`; `rec.camera` FAIL | a V4L2 (or PipeWire camera portal) source behind the camera traits, MJPEG/YUYV to the shared converter, the camera track writer through the Linux encoder path; the bubble preview window as a Studio-window card where Wayland gives no floating window | 5 | L | camera |
| CAM-2 camera as its own track, 50 ms sync | works | works | **missing** (same) | same | same; sync measured with the flash stimulus | 5 | L | camera |
| CAM-3 edit the camera layout | works (shared) | works | **shared, built**; usable only on imported or foreign recordings until CAM-1 exists | `cm_core::camera_layout` | nothing beyond CAM-1 | 5 | — | GPU |
| CAM-4 camera picker, preview, mirror | works | works | **missing** (no device list; the Permissions page has no Camera row on Linux; Settings → Webcam & audio camera check cannot start) | `apps/desktop/src/permissions.rs:165`; `cm-camera` | enumeration through V4L2 (`/dev/video*`, `udev`) and hot-plug events | 5 | M | camera |
| CAM-5 background blur | n/a (Draft) | n/a | n/a | spec | nothing | — | — | — |
| AUD-1 microphone with device picker and meter | works (WASAPI) | works | **partial**: PipeWire is the default microphone source (no libasound/libpulse); the device list on Linux is **only the default PipeWire source** (`mics = vec![default_mic_device()]`), so the picker has one row; `needs_reopen` (follow a changed default) not done; mic drift 2.8 ppm in the VM; the T490 `rec.mic` start was killed by a signal | `crates/cm-audio/src/devices.rs:100-103`, `sys/linux_pipewire.rs`; `L1-audio-drift-linux-2026-10-31`; `linux-recording-plan.md` §6 task 5 | the PipeWire registry for the device list and names, default-device follow, a Bluetooth headset case; `tests/manual/AUD-1.md` on the T490 | NEW-B (or 3) | M | mic |
| AUD-2 system audio | works (process loopback, own sound left out) | works | **partial**: the monitor of the default sink through PipeWire (beeps 2.000 s apart in the VM); **own-process exclusion does not exist** (the monitor carries CaptureMann's own sounds); `rec.system_audio` needs `--allow sound` (not run) | `crates/cm-audio/src/devices.rs:145-151`, `sys/linux_pipewire.rs`; `L1-recording-product-linux-2026-10-31` | a run with a tone from another process on the T490 (before 23:00); the release text says own sounds are included (or a per-node capture that leaves out the app's own PipeWire node, which is larger work) | 3, 7 | M | sound |
| AUD-3 noise suppression | works | works | **works** (shared `nnnoiseless`) | CI | nothing | — | — | — |
| AUD-4 separate tracks, mixed on export | works | works | **works** (shared; `export.audio` PASS) | regression report | nothing | — | — | — |
| AUD-6 mute the microphone while recording | works | works | **built**: shared; the banner has the mute button; driven in the VM | `L1-recording-ui-linux-2026-10-31` | part of the task 3 gate | 3 | S | mic |
| Webcam & audio check page (DEVICE-CHECK) | works | works | **partial / missing**: the page is left out of the sidebar while recording is gated; the system-audio chime is played by `/usr/bin/afplay`, which does not exist on Linux ("other systems get afplay, whose start failure the check reports"); the camera check has no backend | `crates/cm-audio/src/chime.rs:5, 32, 188-191`; `apps/desktop/src/ui/settings/model.rs:52-96`; `docs/development/linux-early-build.md` ("the Webcam & audio page is left out") | a Linux chime player (`pw-play`, `paplay` or `aplay`, found like the opener) and the camera card once CAM-1 exists | NEW-C | S | sound, camera |
| VOC-1 audio-only recording, markers, consent line, compact HUD | works | works | **partial**: the daemon accepts `audio_only` on Linux (PipeWire only, no portal); the app's Audio mode, sidebar mic button and shortcuts are behind the same recording gate; **the idle-sleep guard is a no-op on Linux** (`sleep::sys::acquire` returns a dummy `Held`), so a 3-hour recording can be suspended by the desktop; the markers and the consent line are shared | `sys/linux.rs:311-359`, `crates/cm-platform/src/sleep.rs:155-162`; `recording/audio_sheet.rs:415` | ungate; a logind or portal `Inhibit` (idle + suspend) while recording; `tests/manual/VOC-1.md` short run (15 s and 1 h, the maintainer's priority rule) | 3, NEW-A | M | mic |
| VOC-2 audio editor and Export audio | works | works | **works** (shared; `import.audio` and `export.audio` PASS; the Linux FFmpeg has `wav`, `pcm_s24le`, `aac`) | regression report; `packaging/ffmpeg/build-linux.sh:54, 61` | one run of `tests/manual/VOC-2.md` on the T490 | 7 | S | sound |
| VOC-3 live transcript | n/a (Draft) | n/a | n/a | spec | nothing | — | — | — |

## 3. Editing, playback, export, import

| Feature | Windows | macOS | Linux today | Evidence | What Linux needs | Task | Size | HW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| EDT-1 trim, cut, split | works | works | **built, shared**; the editor was driven in the VM with software rendering; never seen on a real GPU | `L1-editor-linux-2026-10-31`; `crates/cm-render` `wgpu` (Vulkan on Linux) | `tests/manual/EDT-1.md` on the T490 (Intel UHD 620, Mesa) | 7 | S | GPU |
| EDT-2 per-segment speed | works | works | **built, shared** (WSOLA stretch; export with FFmpeg) | CI | one export check in the run above | 7 | S | GPU |
| EDT-3 layout, background, padding | works | works | **built, shared** | CI goldens | same run | 7 | S | GPU |
| EDT-4 editable cursor and auto-zoom | works | works | **missing for Linux recordings** (no cursor log, see REC-7); works on recordings made elsewhere | REC-7 row | REC-7 | 3 | — | screen |
| EDT-5 reframe 9:16 and 1:1 | works | works | **built, shared** | CI goldens | same run | 7 | S | GPU |
| PLAY-1 preview playback (decoder, scrub) | works | works | **partial**: the preview decoder runs FFmpeg per track (shared); the bundled Linux FFmpeg has the `vp9` decoder (D-R4), so VP9 masters decode; playback of a Linux recording never seen on hardware; the editor's output device follows the system default | `packaging/ffmpeg/build-linux.sh:55` (`DECODERS=...vp9...`); `crates/cm-media/src/decode` (shared) | `tests/manual/PLAY-1.md` on the T490 with a VP9 recording and an imported H.264 file | 7 | S | GPU, sound |
| GPU compositor parity (M4 gate, `parity` test) | works (round 3: control MAE ≤ 0.64) | works | **unknown**: no `parity` run on Linux hardware; CI runs lavapipe without the pixel-parity export test; the regression reports say "GPU: n/a" | `.github/workflows/ci.yml:133-136`; regression report headers | `cargo test -p cm-desktop --test parity` on the T490 (Mesa Vulkan) with a stored result; the `cm-regress` Linux driver reports the GPU name | 10, NEW-D | M | GPU |
| EXP-1 MP4 export at source size (quick) | works (H.264 `h264_mf`) | works | **partial**: the export policy on Linux is VA-API H.264 then software VP9 in MP4 (`Encoder::LINUX`); on the T490 `quick` came out **VP9** although `vainfo` lists H.264 encode (cause not found in the repo: the bundled FFmpeg's `h264_vaapi`, `find_vaapi_device`, or `/dev/dri` access); colour tags BT.709 limited were right | `crates/cm-media/src/tools.rs:351-352, 422-423, 489, 500-501`; `export.quick` FAIL ("vp9 not h264") | find why VA-API was not chosen on a machine that has it (task 1); O10 wording in the export dialog and CLI ("which codec and why"); the `cm-regress` expectation follows O10 | 1 | M | GPU |
| EXP-2 platform presets (`youtube-1080p`, vertical, square) | works | works | **partial**: shapes and lengths right, codec VP9 (same cause); O10 says `youtube-1080p` makes WebM when there is no hardware H.264 | `export.presets` FAIL ("youtube-1080p: vp9 1920x1080") | the O10 behaviour (WebM container for the YouTube preset without hardware H.264) and its label | 1 | M | GPU |
| EXP-3 GIF and WebM | works | works | **works** (`webm` and `gif` right in `export.presets`) | regression report | nothing | — | — | — |
| EXP-4 background export job, progress, cancel, Reveal | works | works | **works** (shared jobs; Reveal through `xdg-open`, names `xdg-utils` when missing) | `crates/cm-platform/src/opener.rs`; regression report | one Reveal click on a real file manager | 7 | S | screen |
| IMP-1 probe and classify | works | works | **works** (`import.errors`, `import.dry_run`, `import.audio` PASS) | regression report | nothing | — | — | — |
| IMP-2 editing copy (constant rate, upright) | works (`h264_mf` hardware) | works (VideoToolbox) | **missing when a copy is needed**: `EncoderFamily::host()` is VideoToolbox on every non-Windows system, so Linux always ends in "no hardware video encoder" (`mkv`, rotated and VFR sources failed; plain H.264 `.mp4`/`.mov` passed because no copy was needed) | `crates/cm-media/src/import/prepare.rs:70-115, 336-338`; `crates/cm-daemon/src/import_ops.rs:951-957`; `import.formats`, `import.rotated`, `import.vfr` FAIL | a Linux `EncoderFamily` (VA-API H.264 where the device exists, software VP9 editing copy as the labelled fallback, O10) sharing the `Encoder` detection of `tools.rs`; the error text when neither exists | 1 | M | GPU |
| IMP-3 import job, CLI, MCP | works | works | **works** (shared) | regression report | nothing | — | — | — |
| IMP-4 remove the originals (Trash) | works | works | **built**: freedesktop.org Trash (`cm_platform::trash`, Linux branch) | `crates/cm-platform/src/trash.rs` | one real run (file lands in the Trash, restorable) | 7 | S | none |
| IMP-5 import in the app (sheet, drop, Open With, second launch) | works | works | **partial**: the sheet, drop and the second-launch hand-off over `instance.sock` are shared/built; **the `.desktop` entry has no `MimeType=`**, so file managers never offer "Open with CaptureMann"; `linux.second_launch` exited 0 after 31 ms (the case failed on "left programs running", a suite problem) | `packaging/linux/dev.capturemann.app.desktop`; `apps/desktop/src/instance.rs:97-110`; regression report | `MimeType=video/mp4;video/quicktime;video/x-matroska;audio/wav;...` plus `%F` in `Exec`; one drop and one Open With on GNOME Files | NEW-E | S | screen |
| MP3 and bare AAC import | missing (0.3.1) | missing | missing | `release-0.3.1-plan.md` item 7 | nothing in 0.3.0 (not Linux-only) | 0.3.1 | — | — |

## 4. AI, redaction

| Feature | Windows | macOS | Linux today | Evidence | What Linux needs | Task | Size | HW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| AI-1 transcript and captions (whisper.cpp) | works (CPU, `small` and `medium` runs) | works | **partial**: whisper.cpp built from source (CPU, no OpenMP, fixed baseline `GGML_NATIVE=OFF`); `ggml-medium` transcribed a 7 s clip on aarch64 in the VM; **x86_64 never ran a model** (`ai.transcribe` SKIP: no `--whisper-model`); `pkg.startup` PASS (no illegal instruction) | `verification/L1-local-ai-linux-2026-10-31`; regression report | one `ai.transcribe` run on the T490 with the pinned `ggml-small.bin`; the model download through the app on Linux | 8 | S | none |
| AI-3 auto-zoom suggestions | works | works | **missing for Linux recordings** (needs the cursor log, see REC-7) | `cm_ai::suggest::zoom` needs telemetry | REC-7 | 3 | — | — |
| AI-4 silence and filler cuts | works | works | **works** (shared) | CI | nothing | — | — | — |
| AI-5 translation | works | works | **built, shared**; the text-model path never ran on Linux (see AI-7) | spec status | one translation after AI-7 runs | 8 | S | none |
| AI-6 transcript editor, glossary | works | works | **works** (shared) | CI | nothing | — | — | — |
| AI-7 built-in text engine (`llama-server`) | works | works | **partial**: `llama-server` is built by `packaging/llama/build-linux.sh` (CPU, static) and **is in the 0.2.9 package** (the regression lists its SHA-256, `ai.llama_version` PASS), contrary to the plan's "it is not bundled yet"; **no text model was ever downloaded or run on Linux**; Vulkan untested | `L1-linux-0.2.9-regression-hardware` (file list); `.github/workflows/linux-package.yml:84-100` (`with_llama` input); `release-0.3.0-plan.md` task 8 | one download (Gemma 4 E2B) and one report through the CLI and the app on the T490; the plan's task 8 text corrected | 8 | S | none |
| AI-8 speaker labels (sherpa-onnx) | works (RTF 0.16) | works | **partial**: the `linux-x64` and `linux-aarch64` static libraries are pinned and the models ship in `lib/Resources/Models/speaker`; 2 speakers found on aarch64 in the VM; **x86_64 never ran** (`ai.speakers` SKIP); the model path is a macOS bundle layout (`<exe>/../Resources/Models/speaker`) | `crates/cm-ai/src/model.rs:336-337`; `L1-local-ai-linux-2026-10-31`; regression report | one `ai.speakers` run on the T490 (before 23:00, or with files, no sound); an XDG-style lookup is optional | 8 | S | none |
| AI-9 reports | works | works | **built, shared**; never run on Linux (needs AI-7) | spec status | the AI-7 run writes one report | 8 | S | none |
| AI-10 tidy-up | works | works | **built, shared**; never run on Linux | spec status | same run | 8 | S | none |
| AI-2 titles and chapters | n/a (Deferred) | n/a | n/a | spec | nothing | — | — | — |
| RED-1 benchmark | works (Vision/OCR) | works | **partial**: `redaction_bench` ran with `tesseract` on aarch64, dev split only (84.5 % / 92.7 % / 0.431) | `verification/benchmarks/redaction-0.2.5-linux-tesseract-dev-2026-10-31.md` | an x86_64 run; the held-out split read once | 8 | S | none |
| RED-2 kinds, RED-3 generated docs | works | works | **works** (shared rule data; `redact.fixture` PASS on the T490) | regression report | nothing | — | — | — |

## 5. Library, storage, publishing

| Feature | Windows | macOS | Linux today | Evidence | What Linux needs | Task | Size | HW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| LIB-1 library, search, watcher, rename, delete (Confirm) | works | works | **works** (`lib.*` PASS; delete to the freedesktop Trash) | regression report; `crates/cm-platform/src/trash.rs` | nothing | — | — | — |
| LIB-2 library location, free space, unplug | works | works | **built**: XDG video dir (`XDG_VIDEOS_DIR` from `user-dirs.dirs`), `statvfs` free space; the unplug case tested with fakes | `crates/cm-platform/src/dirs.rs`, `crates/cm-capture/src/disk.rs`; `L1-editor-linux-2026-10-31` (Show in folder, Trash) | `tests/manual/LIB-2.md` with a USB stick on the T490 | 7 | S | none |
| LIB-3 tags and folders | n/a (Draft) | n/a | n/a | spec | nothing | — | — | — |
| STO-1 local folders, external drives, NAS | works | works | **built, shared** (mount paths) | `crates/cm-storage` | `tests/manual/STO-1.md` with a mounted drive | 7 | S | none |
| STO-2 Google Drive (OAuth, keyring) | works | works (real account once) | **partial**: Secret Service through `keyring` with an unlocked keyring (CI runs it against `gnome-keyring`); `no_keyring` with what to start; `CAPTUREMANN_SECRETS=file` opt-in; the wording "in the system credential store" is wrong with no Secret Service (task 2); the browser opens through `xdg-open`; **never run against a real Google account on Linux** | `crates/cm-auth/src/secret.rs:45`, `file_store.rs:21`; `.github/workflows/ci.yml:156`; X11 steps 7 to 9 PASS (diagnostics part) | one connect, upload and share link on the T490 with the maintainer's account (network) | NEW-G, 2 | S | none |
| STO-3 auto-upload, STO-4 share link, STO-5 what to upload | works | works | **built, shared** | spec status | covered by the NEW-G run | NEW-G | — | — |
| PUB-1 connect YouTube | works | works | **built, shared**; never run on Linux | spec status | the NEW-G run | NEW-G | S | none |
| PUB-2 publish to YouTube | works | works | **built, shared**; never run on Linux; needs an MP4 the platform accepts (VP9 in MP4 or WebM, O10) | spec status | the NEW-G run with the export O10 produces | NEW-G, 1 | S | none |
| PUB-3 … PUB-7 | n/a (deferred/draft) | n/a | n/a | spec | nothing | — | — | — |

## 6. Agents, permissions, diagnostics, settings, i18n, window

| Feature | Windows | macOS | Linux today | Evidence | What Linux needs | Task | Size | HW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| AGT-1 one action bus, Unix socket, CLI, MCP (79 tools), JSON-RPC | works (named pipe) | works | **works** (`daemon.*`, `mcp.*` PASS except `mcp.policy_allow`, whose CLI start was killed by the same signal as the recordings) | regression report (`mcp.tools_list` PASS); `crates/cm-mcp/tests/stdio.rs:93` (79 tools); `crates/cm-mcp` has no `target_os` code | the REC-1 triage | 1 | — | — |
| AGT-2 confirmations, tiers, `agents.tool_policy` | works | works | **works** (`mcp.confirm_tier`, `settings.app_only_refused` PASS); the dialog is a desktop window (shared) | regression report | `mcp.policy_allow` re-run after the triage | 1 | — | — |
| AGT-3 one undo step per call, events | works | works | **works** (`lib.annotate_undo`, `mcp.events` PASS) | regression report | nothing | — | — | — |
| PRM-1 permissions screen and page | works (no screen grant; links to privacy pages) | works | **built, different by design**: no per-app prompts; the intro says so and lists what the system lacks (session, portal ScreenCast v4+ and Screenshot, PipeWire, Secret Service, VA-API) from `cm_platform::stack`; microphone only in the card list; no Camera row | `apps/desktop/src/permissions.rs:53-61, 165, 202-280`; `crates/cm-platform/src/stack.rs`; `tests/manual/LINUX-1.md` runs (X11 and Wayland, PARTIAL) | a Camera row once CAM-1 exists; the portal-revoked-mid-recording path on a real session | 5, 3 | S | screen |
| DIAG-1 log file, Copy diagnostics, `get_diagnostics` | works (OS build, GPU adapters and drivers) | works | **partial**: logs under XDG config, the stack lines (session, portal, PipeWire, Secret Service, VA-API, tesseract, FFmpeg, llama-server) are there and PASS; **no GPU name or driver line** (`sysinfo::lines` is empty off Windows); the recording line says "software VP9" even where VA-API exists; `cm-regress` reads no crash log on Linux | `crates/cm-platform/src/sysinfo.rs:3, 211-215`; `crates/cm-daemon/src/diagnostics.rs:181-194`; `linux.vaapi_encoder` FAIL; `zz.crash_events` SKIP | a GPU/driver line (`/sys/class/drm/*/device`, Mesa version from the `wgpu` adapter); the recorder line after task 3; `coredumpctl`/journal reading in the suite | NEW-D, 3 | S | GPU |
| SET: Settings pages (General, Capture & recording, Webcam & audio, Shortcuts, Permissions, Agents & CLI, Destinations, Privacy, About) | works | works | **partial**: with recording gated the Capture page shows only the screenshot delay and the Webcam & audio page is left out; Capture & recording still offers H.264/HEVC and quality choices that mean nothing for the VP9 recorder; Shortcuts on Wayland shows Recording rows and Change buttons while recording is off | `apps/desktop/src/ui/settings/model.rs:52-96`, `recording.rs:109`; `docs/development/linux-early-build.md` ("Known gaps") | ungating (task 3); codec and quality rows that match the Linux encoder set (task 3); the Shortcuts bug (task 2) | 3, 2 | S | screen |
| SET-RESET, language, appearance | works | works | **works** (shared) | CI | nothing | — | — | — |
| I18N: translated UI, IME (Telex, VNI), keycaps | works | works | **works**: shared catalogues; keycaps say "Super"; IBus on X11 seen; Wayland text-input not seen | `L1-editor-linux-2026-10-31`; `docs/development/linux-early-build.md` | `tests/manual/I18N-IME.md` on Wayland (GNOME text-input protocol through GPUI) | 7 | S | screen |
| Window frame (all app windows) | OS frame | OS frame | **built**: app-drawn 36 px bar on GNOME Wayland (minimise, maximise, close, drag, double-click, window menu, resize strips); X11 and KDE keep their own frame; pop-ups are override-redirect on X11 and in-window cards on Wayland | `apps/desktop/src/ui/csd.rs`, `platform_window.rs`; Wayland step 5a/5b/5c PASS (as reported) | KDE, fractional scaling, HiDPI, rounded corners and shadow not done | 10 | M | KDE |
| App id and desktop entry | installer registers | bundle id | **partial**: window class `dev.capturemann.app` (seen on X11), but the `.desktop` file says `StartupWMClass=app.capturemann.desktop`, which does not match; no `MimeType=` | `packaging/linux/dev.capturemann.app.desktop`; X11 step 5 (`WM_CLASS dev.capturemann.app`) | `StartupWMClass=dev.capturemann.app` (else GNOME shows two dock icons; not seen, see "could not verify"), `MimeType=`, `%F` | NEW-E | S | screen |
| Second launch and single instance | works (named pipe) | works | **works** (`instance.sock`; exits at once and shows the running window) | X11 step 15d, Wayland step 15d PASS | nothing beyond the suite's process tracking | NEW-F | — | — |
| Idle-sleep guard while recording | works (power request) | works | **missing** (no-op) | `crates/cm-platform/src/sleep.rs:155-162` | see VOC-1 | NEW-A | S | none |
| Lower process priority for transcription | n/a off macOS (no-op on Windows too) | works | same as Windows | `crates/cm-platform/src/priority.rs:129-135` | nothing for parity with Windows (a `nice`/`SCHED_IDLE` call is small if wanted) | — | S | none |

## 7. Packaging, distribution, update, release checks

| Feature | Windows | macOS | Linux today | Evidence | What Linux needs | Task | Size | HW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Package contents (programs, FFmpeg, `llama-server`, speaker engine and models, licences) | works (zip, installer) | works (DMG) | **partial**: tarball and `.deb` carry everything, built by `linux-package.yml` for x86_64 and aarch64 with `CM_REQUIRE_*=1`; **`pkg.files` FAIL: no `LICENSE`, `THIRD_PARTY_LICENSES` or `NOTICE` next to the programs** (the tarball puts licences under `share/doc/capturemann/`, so this is either a packaging gap or a suite expectation; triage) | regression report (`pkg.files`); `packaging/linux/build-linux.sh`; `L1-packaging-complete-linux-2026-10-31` | copies (or links) of the three files in `lib/capturemann`, or the suite reads `share/doc`; a package checker in CI (task 9) | 1, 9 | S | none |
| Signed download (minisign), checksums | SHA-256 only (unsigned) | notarized | **works** (minisign key in `packaging/linux/minisign.pub`, `sign-linux.sh`) | `packaging/linux/README.md` "Verifying a download" | kept | 9, 11 | — | — |
| Install, upgrade, remove | works (NSIS, winget) | works (DMG, Homebrew cask) | **partial**: `install.sh` and `.deb` proven in a clean container and once on the T490; `linux.deb_install` and `linux.tarball_install` SKIP in the 0.2.9 run (no `--installer`); **no repository**, so no `apt upgrade` | regression report; X11 steps 1, 2 PASS | the APT repository (O3) with an install, upgrade and remove check in CI for amd64 and arm64 | 9 | L | none |
| Update notice in the app | none on any system | none | none | — | nothing (parity) | — | — | — |
| Runtime requirements check (`check-runtime.sh`, glibc ceiling) | n/a | n/a | **works** (8 programs checked on the T490) | X11 step 1 | the package checker (architecture, glibc, no bundled `libwayland` or GPU libraries) | 9 | S | none |
| Debug symbols | n/a | dSYM | **built**: stripped programs, full copies in `packaging/linux/out/debug/` as a CI artifact | `docs/development/linux-early-build.md` item 6 | kept | 9 | — | — |
| Release regression (`cm-regress` full and hardware, X11 and Wayland, `.deb` and tarball, aarch64 VM) | done (0.2.9) | done (0.2.9) | **partial**: X11 only, hardware tier 35/17/25; Wayland and aarch64 not run; the Linux driver lacks window-shot, Auto-redact window, banner, settings-applied, installer-with-package and crash-log cases | `verification/L1-linux-0.2.9-regression-*` | the suite extensions (NEW-F) before task 11 | 11, NEW-F | M | screen, mic |
| Hardware matrix (NVIDIA, AMD, ARM device, two monitors, mixed scale) | partial (one laptop) | one Mac | **not run** | O9 | each a stored step or an explicit "not run" in the release text | 10 | — | GPU, 2 monitors, ARM |

## 8. Proposed new tasks

The plan's tasks 1 to 11 own most rows. These gaps have no owner, or sit inside a task whose text does not name them.
Each is small enough to add to the plan as its own numbered task or as a named sub-item.

- **NEW-A: Idle-sleep and suspend inhibitor on Linux** (`cm-platform::sleep`, Linux branch). A logind
  `org.freedesktop.login1.Manager.Inhibit("idle:sleep", ...)` file descriptor, or the portal `Inhibit` interface, held
  while a recording or an export runs, released on stop; a fake-based test and one real step (screen blanking off
  during a 10-minute recording on the T490). Owner: task 3 could take it; it is listed here because the plan's task 3
  text does not name it. Size S. Cites VOC-1 (the idle-sleep guard) and REC-1.
- **NEW-B: Audio devices through the PipeWire registry** (`cm-audio/src/sys/linux_pipewire.rs`,
  `devices.rs`): list every source and sink monitor with names, mark the default, follow a default change
  (`needs_reopen`, metadata `default.audio.source`/`sink`), so AUD-1's picker and the Bluetooth-headset case match
  Windows and macOS. Size M. Needs a microphone and a second input device.
- **NEW-C: Webcam & audio check page on Linux**: a chime player that exists on Linux (`pw-play`, then `paplay`, then
  `aplay`, found like `cm_platform::opener` and named when missing) and the camera card once task 5 lands; the
  Permissions page's Camera row. Size S. Needs a speaker and a camera.
- **NEW-D: Diagnostics and regression facts on Linux**: a GPU name and driver line (DRM sysfs and the `wgpu`
  adapter), the `cm-regress` Linux driver reporting the GPU instead of "n/a", a crash-log reader (`coredumpctl list`
  or the journal) for `zz.crash_events`, and the "left programs running" tracking bug that failed
  `linux.desktop_stays_up`, `linux.second_launch` and `linux.tray_icon` although each probe passed. Size S.
- **NEW-E: Desktop entry and app identity**: `StartupWMClass=dev.capturemann.app` (the file says
  `app.capturemann.desktop`), `MimeType=` for the media types import accepts plus `%F` on `Exec` (IMP-5 "Open With"),
  a tray pixmap for light panels. Size S. One run on GNOME Files and the dock.
- **NEW-F: `cm-regress` Linux coverage** (prerequisite to task 11): `shot.window` and `redact.window_text` on X11
  (today `WIN_MAC` / `WIN`), a banner case in place of `desktop.hud_click_through`, a `settings_applied` equivalent,
  `linux.deb_install` and `linux.tarball_install` run with the package files, a restore-token hand-over or a
  person-at-screen step for the recording cases (every case runs in a private config folder, so the portal dialog
  waits and `rec.kill9_recovery` fails before it starts), and a Wayland run of the whole suite. Size M.
- **NEW-G: Cloud and publishing run on Linux**: `tests/manual/STO-2.md`, `PUB-1.md` and `PUB-2.md` once on the T490
  with the maintainer's accounts, with GNOME Keyring and with `CAPTUREMANN_SECRETS=file`, report stored. Size S, needs
  the network and the maintainer. Nothing to build unless the run finds something.

Also worth a line in the plan: task 8's "bundle `llama-server` in the tarball and `.deb`" is already done (the 0.2.9
package carries it, `ai.llama_version` PASS); what is missing is a text-model download and run on Linux.

## 9. Cannot be done on Linux (reason), or not the same by design

These need an explicit, reasoned entry in the release text (O5), never a silent gap.

| Feature | Why | What the product does instead |
| --- | --- | --- |
| REC-6 exclude another app's window from a recording | Wayland compositors hand the app a composed stream; no per-window exclusion exists, and the portal path on X11 gives the same stream | The sheet hides the exclusion controls or explains them; the bubble and HUD are composited at export or shown in the Studio window |
| REC-5 floating HUD, countdown disc and brackets hidden from the recording | Same reason: a floating window would be in the video; Wayland also gives no always-on-top or "do not focus" window | The banner in the Studio window, the tray's red state and the desktop's own screen-share indicator; `CM_LINUX_FLOATING_HUD=1` on X11 (then visible in the video) |
| REC-5.AC5 "no recording with the indicator hidden" | A compositor without a panel, no tray host and a closed Studio window shows nothing; the app cannot force an indicator on Wayland | The release text states the limit per desktop |
| CAP-1 window list and window screenshot on Wayland GNOME 46 | Wayland apps cannot list or address other windows; GNOME 46's portal frontend does not return the file its picker saves (response code 2) | X11 works; on Wayland `--pick` where the compositor's picker works (claim only the versions tested); task 4 tries the ScreenCast window source |
| CAP-2 global hotkeys on GNOME 46 Wayland and older | No GlobalShortcuts portal; XGrabKey through XWayland fires only while an XWayland window has focus | The desktop's own custom shortcut running `cm-desktop --launcher` / `--capture ...`; the portal on KDE and newer GNOME (task 6) |
| CAP-2 tray on stock GNOME | GNOME Shell has no StatusNotifier host without the AppIndicator extension | Toast and the launcher window; works with the extension and on KDE, XFCE, Cinnamon, MATE |
| REC-7 click events in the cursor track on Wayland | No global input access for apps; PipeWire cursor metadata carries position and bitmap only | Positions from the metadata (task 3); clicks on X11 through XInput2 raw events if built; the editor's click effects off where there are none |
| AUD-2 leaving the app's own sound out of system audio | The sink monitor carries every stream; excluding one node means capturing each other stream separately (not built) | State it; the chime check uses a separate process anyway |
| PRM-1 per-app permission panes and deep links | Linux has no per-app Screen Recording, Microphone or Input Monitoring grants | The setup screen lists the stack facts instead |
| Software H.264 encoder (recording, import copy, export) | GPL `x264` and the OpenH264 patent exposure (maintainer decisions Oct 24 and D-R1) | VA-API or NVENC H.264 where present, software VP9 labelled (O10) |
| LNC-1 focus return to the previous app on Wayland | No activation-token path for the previous window from a global shortcut the app does not own | Documented limit |
| Toasts above other apps on Wayland | No always-on-top; in-window cards while the Studio window exists | Documented limit |

## 10. What I could not verify (from the repo alone)

- **Why every `record start` on the T490 was "killed by a signal"** (`rec.screen_cursor`, `rec.mic`, and the CLI in
  `mcp.policy_allow`). The VM runs were clean; nothing in the reports names the signal or the process (CLI or
  daemon). First item of task 1; a `coredumpctl` reading on the laptop is the quickest route.
- **Why the export chose VP9 on a machine whose `vainfo` lists H.264 encode.** `encoder_candidates_for` wants
  `h264_vaapi` in the bundled FFmpeg *and* a `/dev/dri/renderD*` node; the package workflow installs `libva-dev`, so
  the build should have `h264_vaapi`, but I cannot see the shipped binary, the probe's output, or whether the lab user
  is in the `render` group.
- **Whether the three `linux.*` desktop cases failed only on process tracking.** Each probe passed (window seen, second
  launch exit 0, SNI registered) and the verdict was "left programs running: cm-desktop"; the case code kills the
  tree after the case (`cases/linux.rs:223, 255`), so a race or a double-fork is likely, not shown.
- **The `pkg.files` licence failure**: the tarball's licence files live under `share/doc/capturemann/` (packaging
  report), the suite looked in the program folder. Whether the `.deb` on that laptop had them at all is not in the
  report.
- **The `StartupWMClass` mismatch** in the `.desktop` file against the window class `dev.capturemann.app`: a real
  effect (two dock icons, no pinning) is plausible but was not reported.
- **The GPU compositor on the T490** (Mesa Vulkan on Intel UHD 620): no `parity` run and the regression header says
  "GPU: n/a"; the editor was only seen with software rendering in the VM and "worked" by the maintainer's word on X11.
- **The Wayland clipboard positive path outside GNOME** (`wayland-data-control` on KDE or wlroots) and the KDE frame,
  tray and GlobalShortcuts portal: no KDE machine has run anything.
- **aarch64 on real hardware**: everything aarch64 is the QEMU VM.
- **Whether `llama-server` runs a model on Linux**: it is in the package and answers `--version`; no model was ever
  downloaded or run there.
- **Audio timestamps on real hardware**: the 21 ms resync fix was checked in the VM; drift, a microphone that reports
  a delay, and the picture/sound offset are unmeasured (the task 3 gate's `av-sync-analyze.py` covers it).

## 11. Ten gaps that decide "no longer an early build" (ranked)

1. Recording start is killed by a signal on the lab laptop (REC-1, AUD-1, AGT policy case): nothing else in the
   recording column can be judged until this is explained (task 1).
2. No hardware H.264 anywhere on Linux today: recording is VP9 only by code, import copies always fail, exports came
   out VP9 on a VA-API machine (REC-1, IMP-2, EXP-1/2; tasks 1 and 3, decision O10).
3. Window and region recording are `not_implemented` (REC-1; task 4).
4. No camera at all: no backend, no bubble, no picker, no camera check (CAM-1 to CAM-4; task 5).
5. No cursor telemetry track, so EDT-4 and AI-3 do not work on Linux recordings (REC-7; task 3).
6. Window screenshots have no working path on GNOME 46 Wayland and the portal failure reads as "declined" (CAP-1;
   tasks 4 and 2).
7. Global shortcuts exist only on X11; the portal path has never met a real desktop (CAP-2; task 6).
8. The idle-sleep guard is a no-op, so long audio and screen recordings can be suspended (VOC-1, REC-1; NEW-A).
9. The microphone picker lists one device and does not follow a default change (AUD-1; NEW-B).
10. Package and release hygiene: licence files not next to the programs, no APT repository, the Linux regression
    driver skips the cases that would show the HUD banner, window shots, installs and crashes, and nothing has run
    on Wayland or aarch64 as a suite (tasks 1, 9, 11; NEW-F).
