---
name: export-and-publish
description: Export a CaptureMann recording as MP4, WebM or GIF, copy it to a connected drive, or publish it to YouTube after the user confirms. Use when asked to export, share, upload or publish a recording.
---

# Export and publish

Read `capturemann-basics` first if you have not.

## Export a video

- `estimate_export` (read-only) gives `typical_bytes` (a best guess) and `max_bytes` (an upper bound) for a `format` (`mp4`, `webm`, `gif`), `size` and GIF `width`, plus the allowed `options`. Use it when the user cares about size.
- `export_video` with `capture_id` and a `preset`: `quick` (default), `youtube-4k`, `youtube-1440p`, `youtube-1080p`, `vertical-1080` (Shorts, Reels, TikTok), `square-1080`, `source`, `gif`, `webm`. `size` (`source`, 1440, 1080, 720, 480) picks the frame of an MP4 or WebM; a GIF takes `width` and is at most 30 s. `captions: "burn"` burns the captions into the picture (needs a transcript). It returns a `job_id` and an `estimate`; follow the job with `get_job`; the result has the file `path` and `size_bytes`. Two exports with different options are two jobs.
- Edits made before the export (`trim`, `cut_segments`, `set_speed`, `set_zooms`, `set_camera_layout`, `set_background`, `set_audio_mix`, `set_reframe`) are applied by the exporter; the master files are never changed.
- `export_captions` (`srt`, `vtt`, `txt`, `md`) and `export_audio` (`m4a`, `wav`) write next to the video. `export_video` fails with `not_found` when no FFmpeg is available (the release includes one).
- A recording that stops normally is exported automatically (`quick`); check `list_jobs` before starting another.

## Copy to a drive or folder

`list_destinations` shows the folders and drives the user connected in the app (Settings → Destinations). `upload` with the `capture_id`, the `destination` and a `request_id` copies the export there as a job and returns a remote id and share link where the provider has one. Agents cannot connect a destination; the user does that in the app. `upload` is Free, but it is the first step that sends data off the computer: say so.

## Publish to YouTube

1. `list_publish_accounts` shows the connected accounts (name, status, `default_privacy`, today's upload count and quota); it never returns a token. `get_platform_rules` with `platform: "youtube"` gives the limits, required fields, privacy options and the recommended export preset.
2. Collect `title`, `description`, `tags`, `privacy` and `made_for_kids` (required; ask the user, do not assume) and the file: `export_preset` (export first) or `export_path`.
3. `publish_video` is **Confirm tier**: the call pauses until the user clicks the action button in a CaptureMann dialog that shows the platform, account, title, privacy, made-for-kids answer, file and size, and today's upload count. Tell the user the dialog is coming. Give a `request_id`. It returns a `job_id`; the finished job has `url`, `video_id`, `status` and `privacy` as YouTube reports it.
4. **Privacy defaults to the account's default, which is never `public`.** Publish as `private` or `unlisted` unless the user explicitly asks for `public`. There is no tool that changes the privacy of a video already on YouTube; the user does that on YouTube.

Only YouTube is built; other platforms answer `not_implemented`. Everything is validated before the dialog, so an `invalid_input` names the field to fix.

## Deleting

`delete_capture` (Confirm) moves the bundle to the Trash; with `also_delete_remote: true` the dialog lists every remote copy that will be deleted permanently. Never call it to "clean up" without the user asking for that capture by name.
