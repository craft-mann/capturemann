---
name: meeting-notes
description: Record a meeting, interview or lecture with CaptureMann, transcribe it on this computer, label speakers and write minutes or a report. Use for meeting notes, summaries or transcripts.
---

# Meeting notes

Read `capturemann-basics` first if you have not. Everything here runs on this computer: speech, speaker and text models live inside the app or in its models folder. Nothing is sent to a cloud service.

## Record audio only

`start_recording` with `mode: "audio"` records the microphone (and system audio for an online meeting) with no screen, camera or cursor track. It is **Confirm tier**: tell the user the dialog "Record audio" will appear and that they must click Allow. Useful fields: `preset` (`room`, `online`, `lecture`, `product`, `note`; `online` and `product` add system audio), `mic`, `system_audio`, `people` (1 to 8, the expected number of speakers), `title`, `max_duration_s` (default 4 h). An audio recording of the microphone alone needs no Screen Recording permission.

**Consent:** the dialog itself says "Let everyone know they're being recorded." Remind the user to tell the other participants before the recording starts. During the meeting `add_marker` with a `label` marks decisions and action items; `set_mic_muted` mutes without stopping. `stop_recording` returns the `capture_id`.

An existing file (a call recording, an MP3 or M4A; from 0.3.3 also a PNG or JPEG picture, which becomes a screenshot) comes in with `import_media` (`paths`, optional `after: { transcribe, diarize, report_template }`). `remove_originals: true` is Confirm; prefer leaving the originals in place.

## Transcribe

1. `list_models`: a speech model must be installed (`installed: true`, one `active`). If none is, say so; `download_model` is Confirm and the only network use of the AI features. Never download without asking.
2. `transcribe` with the `capture_id` (optional `language`: `auto`, a code, or a list such as `vi,en`). Returns a `job_id`; follow it with `get_job`. After a recording stops the app usually transcribes by itself: check `get_transcript` first.
3. `get_transcript` returns words with times, captions and, for a long recording, `partial: true` with `covered_secs` while pieces are still being processed; edit, export and report tools answer `busy` until `transcript.ready`.
4. `normalize_transcript` (Free, local text model) adds punctuation and paragraph breaks into a sidecar without changing a word; `get_normalized_transcript` reads it.

## Speakers

`diarize` (a local job; `num_speakers`, or `min_speakers` and `max_speakers`) labels who spoke when; `get_transcript` then carries `speaker` per word and `speakers`. `set_speakers` renames (`rename: { "S1": "Alice" }`), merges or reassigns in one undo step. Ask the user for the names; do not guess identities from voices.

## Generate the report

1. `list_report_templates` lists the templates (for example `minutes`, `summary`, `interview`, `lecture`, `one_on_one`, `product_recap`) with their sections and `context_fields`.
2. `list_text_models` / `get_text_model`: the built-in text model must be downloaded and selected (`download_text_model` is Confirm; `select_text_model` is Free). Without one, `generate_report` fails with `invalid_input` / `no_text_model`.
3. `generate_report` with `template` and `context` (title, attendees, agenda). It is a job; the result names the `report_id` and `dropped_uncited` (statements the model could not tie to the transcript, left out on purpose).
4. `get_report` returns the report as JSON and Markdown; `export_report` writes `md` or `txt`; `check_report_item` ticks an action item; `edit_report` replaces the Markdown.

Transcripts and reports can be wrong. Quote the transcript's own words for decisions and commitments, point the user at the time in the recording, and never present a report as a verbatim record.

## Captions and text

`export_captions` writes `srt`, `vtt`, `txt` or `md` (with `speaker_names` for SRT); `export_audio` writes the sound as M4A or WAV. `translate_captions` (to English with the speech model, or any language with the text model) is a local job too.
