# INTEL-0.2.1 manual script: the Intel (x86_64) build on a real Intel Mac

The Intel build is made and checked on an Apple Silicon Mac (and run under Rosetta there). Nothing in
it has run on Intel hardware until someone runs this script. Report in `verification/` (template in
its README); say "verified" only with one. The reports are what lets the release notes drop "not yet
tested on a real Intel Mac".

Recording rules (CLAUDE.md): this records your screen, camera and microphone. Run it only when you
choose to, keep takes short, record only yourself or people who agreed, and delete the recordings at the
end. Use a library that is not your daily one (`CAPTUREMANN_LIBRARY=<dir>` with the CLI, or a fresh user
account for the app).

Setup:

1. An Intel Mac with macOS 13 or later. Write down in the report: model and year, CPU, memory, GPU
   (integrated or AMD), macOS version, free disk, and whether it has the Apple T2 chip.
2. `CaptureMann-v0.2.1-macos-x86_64.dmg` and its `.sha256` file (or `brew install --cask
   craft-mann/tap/capturemann`, which must pick the Intel build: check with
   `lipo -archs /Applications/CaptureMann.app/Contents/MacOS/cm-desktop`, expected `x86_64`).
3. Part A needs nothing but the app. Part B downloads models (about 4.4 GB for the speech and text
   models in step 12 of `FIRST-RUN-0.2.md`; use the smaller ones here).

## Part A: the app and the capture hardware

1. **Install.** `shasum -a 256 -c` prints OK. Open the DMG, drag to Applications, open the app. Expected:
   starts without "damaged" or "bad CPU type" messages; `spctl --assess --type execute --verbose
   /Applications/CaptureMann.app` says accepted (Notarized Developer ID, once the build is notarized).
   Every executable is x86_64: `for f in /Applications/CaptureMann.app/Contents/MacOS/*; do lipo -archs "$f"; done`.
2. **Window and preview.** The main window draws, text is sharp, scrolling is smooth. Note which GPU the
   app chose (Activity Monitor or a log line) and any glitch.
3. **Permissions.** The permissions screen; grant Screen Recording (and restart), Microphone, Camera and
   Input Monitoring one at a time. Expected: each prompt appears once, the card turns Granted.
4. **Screenshot.** Area, window and full screen. Expected: files in the library, correct size and
   colours (compare with a macOS screenshot of the same area).
5. **Screen recording.** 10 seconds at 30 fps, then 60 fps. Expected: the hardware H.264 encoder is used
   (note the model name the app reports), the file plays, no dropped-frame warning at 30 fps. Run the
   60 fps self-test and write down its honest answer (it may say no on a slow GPU; that is a pass if the
   message is clear).
6. **Camera, microphone, system audio, cursor.** One 10 second recording with all of them. Expected: the
   webcam bubble is not in the screen track, the editor shows each as its own track, audio is in sync
   (clap once in view of the camera), the cursor log is present.
7. **Pause, resume, kill.** Pause for 5 seconds. In a second recording `kill -9` the app mid-take and
   reopen it: the take is recovered (at most one 2 second fragment lost).
8. **Editor and export.** Trim, cut, speed 2x, a zoom, then export MP4 (H.264) and WebM. Expected: the
   preview matches the export frame for frame (the parity look), exports finish, both play. Note the
   export time of a 1 minute take. A 10 minute export, to look for stalls (there is no stall timeout):
   note the time and whether it finishes.
9. **Audio recording and editor.** The Online meeting preset, 1 minute, add a marker. Expected: HUD meter
   moves, the waveform draws, `Export audio` works.

## Part B: models and speed (fills the plan's Stage 3 numbers)

Use the same 11 minute test meeting as for 0.2.0 if you have it (the CLI: `capturemann ai transcribe`).
Write down wall-clock time and, from Activity Monitor, peak memory for each:

10. **Speech:** Tiny, Base, Small, Large v3 Turbo. Note whether the GPU (Metal) is used and compare with
    CPU only (Settings > AI, if the switch exists; otherwise note what the app does). If Metal fails the
    app must fall back to the CPU by itself, not stop.
11. **Speakers:** Find speakers on the same recording (the two models are inside the app).
12. **Text model:** Qwen3.5 2B and Gemma 4 E2B: a Meeting minutes report, and a Vietnamese translation
    of 20 lines. Expected: `llama-server` starts (it is CPU only on Intel), the Test button says
    "Structured output works" and "only talks to 127.0.0.1". Note the time of each.
13. **Result:** for each model write "usable" (finishes in a time you would wait for) or "too slow", with the
    numbers. These decide the Intel defaults (plan Stage 3, I3.2).

## Part C: libraries

14. **Upgrade.** Open a 0.2.0 library made on an Apple Silicon Mac (copy it first): captures open, edit,
    export. Then the other way: a library made here opens on the Apple Silicon Mac.

## What to send back

The report with the Mac details, a pass or fail per step, the numbers of Part B, and any crash log
(`~/Library/Logs/DiagnosticReports`). A failing step becomes a work item in the Intel plan.
