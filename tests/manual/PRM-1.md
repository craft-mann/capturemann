# PRM-1 manual script: permissions

Needs a person at a Mac. These steps change real grants, so use a throwaway bundle id or accept
re-granting afterwards. Never run `tccutil` against the bundle id of your daily copy.

## Setup
- `make bundle` (signing mode is printed), open `target/bundle/CaptureMann.app`, or run your own
  instance with separate `CAPTUREMANN_SOCKET`, `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`.
- Delete `permissions.onboarded` from `settings.json` in the config dir to see the first launch again.

## Without touching real grants (any machine)
Run the debug build with the fake provider:
```
cm-desktop --demo-permissions missing --demo-permissions-window     # nothing granted
cm-desktop --demo-permissions partial --page settings-permissions   # screen ok, camera denied
cm-desktop --demo-permissions restart --demo-permissions-window     # granted, restart required
cm-desktop --demo-permissions granted --demo-permissions-window     # all granted
cm-desktop --demo-permissions windows --page settings-permissions   # Windows list
CM_DEBUG_SCREEN_PERMISSION=missing cm-desktop                       # Record sheet failure state
CM_DEBUG_TARGETS_ERROR=internal cm-desktop                          # Retry state
```
Expected: pills and buttons as in PRM-1; "Grant access…" on the fake provider turns *Not asked yet*
into *Granted* and writes nothing to the OS.

## Startup rule and auto-close (PRM-1.AC5, AC11)
Use your own instance (separate `CAPTUREMANN_CONFIG_DIR`, so `permissions.onboarded` starts unset). Fake provider:
1. `--demo-permissions granted`, fresh config dir: no setup screen, no banner; `settings.json` now has
   `"permissions.onboarded": true`.
2. `--demo-permissions missing`, onboarded set or not: the main window opens on the setup screen on every launch. Click another sidebar item and
   relaunch: it shows again (Screen Recording still missing).
3. `--demo-permissions partial` (screen ok, camera denied), fresh config dir: the main window opens on the setup screen with
   "Continue" enabled and "Skip optional". Click another sidebar item, relaunch: library (onboarded recorded).
   Repeat with a fresh dir and press "Skip optional": same.
4. `--demo-permissions-window --demo-permissions granted`: the screen shows "All set" and goes to the library after about 1 s.
5. Real grants, setup screen showing with only Screen Recording missing: grant it (restart required row), restart; the new
   process reads *Granted* (not restart required) and, with the others granted, shows no setup screen. The restart
   state is never written to `settings.json`.
6. Settings > Permissions > "Reopen setup" shows the setup screen again (in the main window).

## First launch (PRM-1.AC5)
1. Remove the CaptureMann entry from Screen Recording (System Settings > Privacy & Security), delete
   `permissions.onboarded`, start the app. The main window opens on "CaptureMann needs a few permissions"; no
   OS dialog appeared by itself (AC8).
2. Screen Recording shows *Not granted*, "Continue" is off. Press "Grant access…": macOS shows its
   dialog once. Choose "Open System Settings", enable CaptureMann.
3. Within about a second the row reads *Granted — restart required* and "Restart CaptureMann" is the
   primary button (AC6). Press it: the app quits and the bundle reopens about 2 s later. The setup screen
   does not reappear (if it does, a required permission is still missing).

## No popup, entry points, --background (PRM-1.AC12)
1. Launch with a required permission missing: only the main window exists, on the setup screen; the sidebar is
   dimmed. No second window appears. Click "Recordings": the library section opens, the banner reminds you.
2. Launch with `--background` and Screen Recording missing: the main window opens on the setup screen. With
   everything granted, `--background` opens no window.
3. Tray "Permissions…", launcher "Permissions…", the banner and toast "Open Permissions…" buttons and the Record sheet
   button bring the main window to the front on Settings > Permissions (it opens if it was closed).
4. Agent confirmation dialog opened while the library window is frontmost: it appears in front, centered.
   Toasts never take keyboard focus.

## Optional permissions (AC3, AC4, AC7)
1. Microphone *Not asked yet*: press "Grant access…", allow. Pill turns *Granted*.
2. Camera: press "Grant access…", deny. Pill turns *Denied*; the only button is "Open System
   Settings" and it opens the Camera pane. Turn it on there; the pill follows within 2 s.
3. Input Monitoring *Not granted*: press "Grant access…", then "Open System Settings" opens the
   Input Monitoring pane. After granting, quit and reopen; the row reads *Granted*.
4. In the Record sheet with Camera *Not asked yet* (reset it in System Settings or use a fresh
   bundle id), turn the webcam toggle on: the OS dialog appears at that moment, not before.

## Revoking (AC2, AC9)
1. With the app running and Screen Recording granted, switch CaptureMann off in System Settings.
   Within 2 s on Settings > Permissions (or at the next sheet open) the row reads *Not granted*. The
   library banner "Screen Recording permission is missing" appears.
2. Open Record: the sheet says "Screen Recording permission is missing" with the three buttons; Start
   is disabled; "No display found" does not appear.
3. Press a capture hotkey: the toast "Capture didn't start" names the permission and offers "Open
   Permissions…" and "Open System Settings".
4. Revoking while a recording runs: see the next section.
5. Stale grant hint (Input Monitoring). With a grant made for an older signature (the switch is on in
   System Settings but the card says "Not granted"; TCC logs `Failed to match existing code requirement`),
   press "Grant access…" or "Open System Settings" on the Input Monitoring card. Expected: under the note
   a sentence says to select CaptureMann, press the minus (−) button and grant access again. It does not
   show before a button was pressed in this run, nor once the permission is granted. Write down whether
   removing and re-adding the entry fixed it. No run recorded yet.

## Revoked during a recording (PRM-1.AC13)
Never run on a Mac yet: the automatic tests use fake sources, so what macOS really does is the thing to
write down here. Use a short recording of a screen that shows nothing private; delete it afterwards.
Run `capturemann record watch` in a second terminal to see the events (add `--topic 'recording.*' --topic 'camera.*'` to see camera plug events too). For each step
write down what you observed, even when it differs from "Expected": the spec wants the truth.
1. Screen Recording removed. Start a display recording with the microphone and camera on (the HUD
   timer runs). After about 10 s open System Settings > Privacy & Security > Screen Recording and
   switch CaptureMann off (macOS may ask to quit the app: choose "Later" or "Not now" if it offers
   it; if the app is terminated instead, skip to step 5).
   Expected: within a few seconds the HUD, countdown, brackets and bubble close and a toast reads
   "Recording stopped: Screen Recording permission was removed" with "Open Permissions…" (it opens
   Settings > Permissions) and "Open recording". Events: `recording.warning` with
   `code: permission_revoked, permission: screen`, then `recording.stopped` with
   `reason: permission_revoked, permission: screen` and a `message`.
2. The recording is in the library with a length close to what you saw on the timer (it may end a
   few seconds before you switched the permission off). Play it: the screen, camera and mic tracks
   all play to the end without a hang. `<library>/<recording>.cmrec/recording.lock` does not exist.
   `manifest.json` has `stop_reason: permission_revoked` and a `permission_revoked` entry in `warnings`.
3. Switch the permission back on, restart the app if the row says "restart required". Record again
   for 5 s: it works and the earlier take is unchanged.
4. Pause for a few seconds before step 1 in another run: the length and the pause gap of the saved
   recording are still right (the paused time is not counted).
5. If macOS ended the app instead of delivering an error: reopen it. The toast "Recovered recording
   from <time>" appears (REC-8) and the take is in the library. Note this in the report: it means
   the in-process handling never gets to run for this permission on this macOS version.
6. Microphone removed during a recording (Privacy & Security > Microphone). Expected: the recording
   keeps running; within 1 s (or after the 15 s `mic_silent` warning, if the OS keeps the stream
   open and delivers silence) a `recording.warning` with `permission: microphone` appears and the HUD
   shows a warning chip. Stop it normally: the mic file is present and plays up to the silence. If macOS
   ended the app, do as step 5.
7. Camera removed during a recording (Privacy & Security > Camera). Expected: the bubble shows the
   camera-off placeholder, a `recording.warning` with `permission: camera` appears, the screen
   recording continues, and the saved `camera.mp4` plays up to that moment. If macOS ended the app,
   do as step 5.
8. Input Monitoring or Accessibility removed during a recording: the recording continues, nothing
   stops, the cursor track keeps position samples without clicks.

## Ad-hoc re-sign symptom
1. `make bundle` with no "CaptureMann Local Dev" identity (ad-hoc), grant Screen Recording, restart.
2. Change any source line, `make bundle` again, open the new app: the grant is gone (row *Not granted*,
   banner shown). Switch the entry off and on, restart: works again.
3. Run `packaging/macos/create-dev-identity.sh` yourself, `make bundle` twice with a code change in
   between: the second build keeps the grant and `bundle.sh` printed "local dev identity found in the
   keychain".

## Hotkey start with a microphone or camera the OS has not allowed (R4-5)
Own instance, fake provider `--demo-permissions missing` is not enough (the engine checks the real
state), so use a throwaway bundle id or a Mac where the microphone was never asked:
1. Press Option+Shift+R with the microphone on in the sheet's remembered choices and the
   microphone "Not asked yet": the recording starts without the microphone and an info toast says
   "Recording without the microphone ... Turn it on in Settings -> Permissions". No system dialog opens.
2. With the microphone Denied: the same. With the camera on and Denied: the same, for the camera.
3. With Screen Recording missing: the toast "Recording didn't start" offers "Open Permissions..."
   and "Open System Settings", which opens the Screen Recording pane. A start that is refused for
   the microphone or camera (a start from the sheet, or an agent) opens the Microphone or Camera
   pane instead.

