# L1 editor, library and settings on Linux (early-build scope)

- **Date:** 2026-10-31
- **Who:** Claude (agent); nothing by a person; VM only
- **Machine:** QEMU VM, Ubuntu 24.04 aarch64, 4 CPUs, software Vulkan (lavapipe). X11: private Xvfb :97 (1600x900) with openbox, XInput has no scroll valuators. Wayland: the shared headless GNOME 46 shell (1280x720), input through Mutter RemoteDesktop (`/tmp/wlclick.py`, first key of each session is lost, so a dummy Shift tap goes first), pictures with `gnome-screenshot`. The Wayland session is shared: other agents' windows and a stale toast appear in frames.
- **Build:** branch `worktree-agent-a7671a7b361e546b7`, debug, `cargo build -p cm-desktop --no-default-features`; private `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_SOCKET` under `/tmp/ed`. Captures made with `capturemann capture screenshot --display/--region`.
- **IME:** `ibus` + `ibus-unikey` (apt), `ibus-daemon -d -r -x`, engine `Unikey`, app started with `XMODIFIERS=@im=ibus`.
- **Screenshots:** `L1-editor-linux-2026-10-31-shots/`
- **Tests:** `cargo test -p cm-desktop --no-default-features --lib` 1439 passed; `cargo clippy -p cm-desktop --no-default-features --all-targets --locked -- -D warnings` clean (Linux VM); xim-ctext copy: 6 tests.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Start on X11, first-run permissions page | PASS | `a04`; Linux wording ("Linux does not ask per app"), Ctrl Alt Shift hints |
| 2 CLI captures appear live in the library | PASS | `a05` |
| 3 Open a screenshot in the editor | PASS | `a06`; tool rail, inspector, Ctrl/Shift/C hints |
| 4 Arrow, rectangle, text (Latin), highlight, blur, pixelate, crop | PASS | `a07`, `a09`, `a10`; render.png is the cropped 310x138 result |
| 5 Undo / redo: Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y, toolbar buttons (X11) | PASS | counts in manifest and picture |
| 6 Select, Ctrl+C / Ctrl+V of an annotation, arrows move, Delete, undo | PASS | |
| 7 Copy image (button, X11) | PASS | `a13`; PNG on the X11 clipboard, 310x138 |
| 8 Save as: portal dialog, default name, write | PASS | X11 run: dialog is shown by the portal on the Wayland session (VM artefact: nothing on Xvfb); Wayland run: Return saved `/home/dev/Terminal region.png` 310x138 (`sv2`). Default folder is home because `~/Pictures` does not exist (no xdg-user-dirs) |
| 9 Zoom: Ctrl + wheel (X11) | PASS | 605 % after 3 notches |
| 10 Shortcut popover shows Ctrl | PASS | `a14` (not copied) |
| 11 Library: search (Ctrl+F), live filter, clear | PASS | |
| 12 Library: context menu, Show in folder | PASS | Files opened with the item selected (portal OpenDirectory) |
| 13 Library: Delete to Trash | PASS | `a17`, `a18`; item in `~/.local/share/Trash/files` with `.trashinfo`, toast, count updated |
| 14 Settings pages (General, Capture, Webcam, Destinations, Permissions, Privacy, Shortcuts) | PASS with findings | `settings-*`, `a19`; see problems 4 to 6 |
| 15 Fonts | PASS | Geist is bundled and used (no system Geist: `fc-list` 0) |
| 16 Theme follows `color-scheme` live (X11) | PASS | `t1` dark while set to prefer-dark, back to default restored |
| 17 Text field paste, Vietnamese precomposed typing | PASS | |
| 18 Text field Ctrl+C / Ctrl+X | FAIL before, PASS after fix | commit "Ctrl/Cmd+C and Ctrl/Cmd+X in text fields" |
| 19 Vietnamese Telex through IBus (X11): search field | FAIL before (app quit), PASS after fix | panic in `zed-xim` on Latin-1 compound text; `ime2` "Việt Nam tôi yêu" |
| 20 Vietnamese Telex in a text annotation | FAIL before (nothing typed), PASS after fix | `ti1` "Việt Nam" in the annotation |
| 21 Destinations: YouTube card | FAIL before, PASS after fix | name wrapped as YouT/ube (`s8`, `y1`) |
| 22 Wayland: edit then Undo | FAIL before, PASS after fix | clipboard copy in the save queue stalled Undo with "Saving..." |
| 23 Wayland: Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y, undo/redo buttons, draw, text | PASS | manifest op counts 10, 11, 10, 11 |
| 24 Wayland: wheel direction | PASS | positive axis moves content up (scrolls down) with `natural-scroll=false` (`sc1`) |
| 25 Wayland: portal Save as / reveal | PASS | |
| 26 X11 wheel in Xvfb | PASS with note | only button 4/5 events: scrolls Settings; XInput scroll valuators absent (GPUI logs "Found no xinput mouse pointers") |
| 27 Window resize, minimum size | PARTIAL | at 720 px high the tool rail scrolls (Crop is below the fold; by design `overflow_y_scroll`); 720x480 minimum not driven |
| 28 Cursor shapes | not run | no way to see the pointer in these frames |
| 29 Audio editor playback on Linux | not run | recording is hidden; the VM has no sound card (`aplay -l`: none), playback goes through `cm_audio::playback::PreviewAudio` (cpal), which only a machine with an audio device can check |
| 30 Wayland text-input (IME) | not run | needs IBus in the GNOME session; text via keysym injection only |
| 31 KDE, HiDPI, real GPU | not run | |

## Problems found and fixed (commits on this branch)

1. Text fields had no Copy or Cut (any OS): added Ctrl/Cmd+C and +X for select-all.
2. IBus on X11 killed the app on the first accented letter: patched `vendor/xim-ctext`.
3. Text annotation ignored input methods: canvas registers as a text input.
4. YouTube card in Settings > Destinations squeezed its text.
5. Clipboard copy inside the editor save queue froze Undo and saves on GNOME Wayland.

## Not fixed (listed)

- Wording from macOS on Linux: "A folder on this Mac" (Destinations), "Starts CaptureMann in the menu bar" (Launch at login), "Needs FFmpeg (brew install ffmpeg)", "Uses the Screen Recording permission", "orange microphone dot" (Webcam and audio), "Not signed in on this Mac", Privacy lists macOS bundle ids (1Password and so on) for "Apps that never appear in recordings".
- Record, Webcam and audio settings and the recording buttons are visible although recording is hidden in the early build (recording UI not in this task).
- Wayland clipboard: `Copy image` fails on GNOME 46 (no data-control protocol; helpers `wl-copy`/`xclip` missing or failing); the toast says it could not copy. Clipboard files are owned by another task.
- Save as uses home when `~/Pictures` is missing (xdg-user-dirs not read).
- The "Pointer" readout in the editor inspector shows coordinates outside the image.
- Toast pop-up windows on X11 without a compositor have square black corners behind the rounded card.
- Save-as and other portal calls fail silently (no toast) if no portal is running.
- Wayland first-key loss and focus loss in the shared session are test-harness issues, not app bugs.

## Verdict

PARTIAL: the screenshot, annotation, undo, copy, save, library, trash, reveal, settings and theme paths work on X11 and GNOME Wayland in a VM after five fixes; audio playback, Wayland IME, cursors, KDE and real hardware not run.
