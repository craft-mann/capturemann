---
name: record-screen
description: Start, pause, mark and stop a screen recording with CaptureMann, with optional camera, microphone and system audio. Use when the user asks to record the screen, a window, a demo or a tutorial.
---

# Record the screen

Read `capturemann-basics` first if you have not.

## Before starting

- `list_devices` shows cameras, microphones, whether system audio is supported, and the permission states (`screen_recording_permission`, `camera_permission`, `microphone_permission`). Checking never prompts. A missing permission makes `start_recording` fail with `permission_missing`; the user grants it in the OS settings and in CaptureMann.
- `list_targets` for the display or window to record; `get_settings` for the current defaults (size, fps, codec, camera, audio). Missing `start_recording` fields come from the settings.
- **Consent.** If other people will be recorded (a call, a meeting, someone's camera or voice), remind the user to tell them. Do not start a recording of a call without that reminder.

## Start

`start_recording` is **Confirm tier**: the call pauses until the user clicks **Allow** in a CaptureMann dialog on their screen (up to 5 minutes). Before calling it, tell the user that the dialog is about to appear and what it will say. The dialog lists the target, output size and fps, screen, microphone, camera, system audio, cursor, where the file goes and the countdown; what it shows is exactly what runs. Cancel returns `user_declined`; no answer returns `confirmation_timeout`. Do not call again in a loop; ask the user.

Parameters (see the schema): `target` (`{ display_id }`, `{ window_id }` or `{ display_id, rect }`), `size`, `fps` (30, or 60 after a passing `run_fps_selftest`, itself Confirm), `codec`, `quality`, `camera`, `mic`, `system_audio`, `cursor`, `max_duration_s`, `countdown_s`, `title`, `exclude`. A recording started by an agent shows the same border, timer, Stop button and tray indicator as a manual one, plus a toast "Started by <agent>" with its own Stop button: the user can always stop it.

The user may have set `start_recording` to Allow in Settings → Agents & CLI: then no dialog opens. Still announce the start.

## While recording

- `get_recording_status`: state, recorded time, late-frame percentage, mic level, disk minutes left, warnings.
- `pause_recording` / `resume_recording`; `set_mic_muted` (the file holds silence for the muted span).
- `add_marker` with a `label` marks a moment ("Decision", "Bug appears"); markers are kept in the recording and used by reports.
- Events `recording.*` arrive as MCP notifications (logger `capturemann.events`).

## Stop

`stop_recording` returns the `capture_id`, duration and tracks (`already_stopped: true` if it ended on its own: disk full, max duration, window closed). The recording is exported automatically unless the user turned that off (`export.auto_on_stop`); watch `export.*` events or `list_jobs`. Then: `transcribe`, the edit tools (`trim`, `cut_segments`, `set_zooms`, ...), `export_video` (see `export-and-publish`).

## Do not

- Do not start a recording the user did not ask for, and never try to hide that one is running.
- Do not record to "check something" or "take a look at the screen"; use `capture_screenshot` and say so.
- There is no tool to discard a take: if the user wants it gone, `delete_capture` (Confirm) after it stopped.
