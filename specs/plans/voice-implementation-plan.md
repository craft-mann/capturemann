# Voice and meetings: implementation plan

Status: **Planned Oct 26, 2026.** Nothing built. Behaviour: `../features/15-voice.md` (VOC-1 to VOC-3), `../features/07-ai.md` (AI-5, AI-7, AI-8, AI-9). Approach, models and spikes: `voice-and-meetings-plan.md`. Screens: the redesign kit boards V0 to V8 (`design/redesign/`, kit copy `capturemann-redesign 5/`); the kit's `CLAUDE.md` says which board builds which part.

No release dates (maintainer, Oct 26, 2026). Work in the order below; each stage leaves the app working and is merged when fmt, clippy, tests and `cm-i18n check` pass. Masters stay WAV.

## Rules for every package

- Actions first: GUI, CLI and MCP go through `cm-daemon` actions; each user-facing feature has an MCP tool and CLI command with identical behaviour (CLAUDE.md).
- Hardware behind traits with fakes; no test may record for real. Real-hardware checks are manual scripts in `tests/manual/` plus a report in `verification/`.
- UI strings through `cm_i18n` with Vietnamese; follow the board for the surface and `design/CLAUDE.md`.
- Speaker colours are `#8FB0FF`, `#7FD6C2`, `#F2B880`, `#C9A7F5`, then repeat; never status colours.

## Stage 0: foundation (blocks everything)

**P0.1 Audio kind.** Add `CaptureKind::Audio` (`cm-library/src/manifest.rs`), index it (`kind = audio`), and make code that assumes a `screen` track tolerate its absence (`index.rs`, `rec_edit.rs`, details and library views). Fixture: extend `make_fixture` with `--audio-only`.
**P0.2 Markers.** `manifest.markers: [{ at, label? }]` in session time (pauses removed), a revision-tagged edit, `add_marker` action, MCP tool and CLI `record marker`; also callable after stop with `capture_id` + `at`.
**P0.3 Audio-only session.** `RecordingSession` with no screen, camera or cursor track; `mode: "audio"` on `start_recording` with the `invalid_input` rule for screen-only fields (VOC-1.AC9); `preset`, `title`, `max_duration_s`; Confirm dialog text. Idle-sleep assertion and `reason: "system_sleep"` stop. Microphone-only must not request Screen Recording.
**P0.4 Settings.** `recording.consent_notice` (default on), `recording.audio_max_duration_s`, shortcut entries ⌥⇧V and ⌥⇧M (also fixes the CAP-2 table).
Tests: VOC-1.AC1, AC4, AC5, AC7 to AC9 with scripted devices; kill -9 recovery with the existing e2e example (`--tracks mic`). *Correction Oct 27, 2026:* `record_e2e` always adds the screen track and caps `--secs` at 30, so it cannot cover AC5 (5 minutes of audio only); AC5 is run by hand, see `tests/manual/VOC-1.md` section 10.

## Stage 1: audio recording (VOC-1)

**P1.1 Record sheet, Audio mode** (board V1): mode switch, five presets, mic and system sources with the live meter, "How many people?", "After you stop" row, consent line, disk and duration footer.
**P1.2 Compact HUD** (V2): timer, 14-bar meter, Pause, Mute, Marker, Discard; agent chip; ⌥⇧M and M; the marker toast.
**P1.3 Entry points:** tray, launcher (LNC-1), sidebar mic button and rail, ⌥⇧V.
**P1.4 Library Audio** (V3): sidebar item with count, waveform thumbnails (peaks computed after stop, cached in the bundle), status badges, transcript search.
P1.1 to P1.3 touch `apps/overlay` and `apps/desktop` recording code; P1.4 touches the library UI. Disjoint, so they can run as parallel worktree agents after Stage 0.
Manual: `tests/manual/VOC-1.md` (permissions, consent, HUD, lid hint) and a 3-hour soak report (AC6).

## Stage 2: audio editor and export (VOC-2)

**Status Oct 26, 2026:** P2.1 and the UI of P2.3 built (tested with fakes, not checked by eye; see VOC-2 "As built, UI" in `../features/15-voice.md`); the Speakers and Report tabs are stubs for Stages 5 and 6; P2.2 (FFmpeg rebuild) is the maintainer's.

**P2.1 Editor shell for `kind: "audio"`** (V4): "now speaking" card, lanes (Markers, one waveform per track, suggested cuts), overview strip, Transcript tab in wide mode, tab set `audio`. Reuses the caption editor and the R3c wide mode.
**P2.2 FFmpeg:** add the `wav` muxer and `pcm_s24le` encoder to `packaging/ffmpeg/build-macos.sh` (lists and self-check), rebuild with `make ffmpeg`.
**P2.3 `export_audio`** job (V7): M4A and WAV, Mix / Mic / System, `loudnorm` to −16 LUFS, tags, optional copy folder; "Export audio only" for screen recordings (Output tab and Details); done toast. Golden tests with `ebur128`.
Manual: `tests/manual/VOC-2.md`.

## Stage 3: Whisper models and translation to English (AI-1, AI-5)

**Status Oct 26, 2026:** P3.2 backend built (tested with fakes); P3.1 entry added but unpinned (checksum, size, memory and URL check are the maintainer's), no UI. Details: AI-5 "as built" in `../features/07-ai.md`.

**P3.1** Pin `large-v3-turbo` and `large-v3` in `crates/cm-ai/data/models.json` (commit-pinned URL, SHA-256, size, `ram_mb`); AI page rows and labels (V8 speech section).
**P3.2** Whisper translate task in `cm-ai/src/whisper.rs`; the turbo rule (large-v3, else medium, else `translation_model_missing`); `translations/<lang>.json`; language switcher in the transcript; per-language exports; `translate_captions` tool and CLI. The text-model engine arrives in Stage 4.

## Stage 4: text model provider (AI-7), then the rest of AI-5

**Scope (maintainer, Oct 26 and 27, 2026): the built-in text model only.** The Remote and Anthropic providers, API-key entry, the first-send notice and the remote Confirm tier are deferred; the AI page shows None and Built in only, and the daemon treats any other provider value (`local` included) as `none` (done Oct 27, 2026: `TextModelConfig::from_settings`). The backend code for them exists from the first Stage 4 pass and stays dormant until the maintainer asks for it.

Run **spike S8** first (it picks the default model). Then: OpenAI-compatible client over loopback (for the built-in engine only; the Oct 27, 2026 decision dropped the Ollama and LM Studio presets and detection, and the Anthropic and remote keys are dormant); `ai.text.*` settings (app-only); schema-validated output with one retry; map-reduce chunking helper; Test button; AI page text-model section (V8); `get_text_model`, `test_text_model`. Then text-model translation to any language (AI-5.AC3 to AC7). Test server is scripted; loopback-only check.

**Built-in engine (maintainer decision Oct 27, 2026; built, tested with fakes).** **Release 0.2.0 ships the built-in engine as the only text-model provider (maintainer decision Oct 27, 2026):** the AI page, `ai.text.provider`, the tools and the CLI know `none` and `embedded` only; `local` is read as `none`, and the `ai.text.base_url` setting, server model listing and preset detection are removed. The text model does not depend on any other program: provider `embedded` runs llama.cpp's `llama-server` (MIT) as a helper program in the app bundle (not linked: ggml clashes with whisper.cpp), on `127.0.0.1`, started on the first text request, stopped when idle and at exit, with GGUF models downloaded on request like the Whisper ones (`text_models.json`, tiers 8, 16 and 32 GB, `pending_pin` placeholders until spike S8's research is pinned). New: `ai.text.embedded_model`, tools `list_text_models`, `download_text_model`, `select_text_model`, `remove_text_model` (CLI `ai text-models ...`), engine status in `get_text_model`, failure reasons `engine_missing`, `engine_failed`, `not_enough_memory`, `make llama`. Spec: AI-7 "Built-in engine" in `specs/features/07-ai.md`. Open: pins, `make llama`, real runs, the V8 board has no design for the Built in choice.

## Stage 5: reports (AI-9)

Templates as data in `crates/cm-ai/data/reports/` with `additionalProperties: false` schemas; citation check and `dropped_uncited`; interview schema with no score field; storage `reports/<id>.json` and `.md`; Report tab setup, report view and exports (V5, V6); tools `list_report_templates`, `generate_report`, `get_report`, `export_report`, `delete_report`; events. Review set scored by hand in `verification/`.

**Built (Oct 26, 2026, backend only, tested with fakes):** everything above except the Report tab, the report view and Copy (V5, V6) and the review set; plus `edit_report`. Oct 27, 2026 (tested with fakes): `check_report_item` (ticked tasks stored in the report, one undo step), the Report view's left column (V6) and the offer toast after a transcript (V1); see 07-ai.md, "As built, AI-9". Local text model only (remote deferred by maintainer decision). See "As built, AI-9" in `specs/features/07-ai.md`.

## Stage 6: speaker labels (AI-8)

Run **spike S7** first (model, licence, `ort` versus `sherpa-onnx`). Then: model download and licence display, diarization job, word assignment, `diarization.json`, rename, merge and reassign revisions, the Speakers lane and tab, `Speaker names` export switch, headset shortcut, `profile.display_name`; `diarize` and `set_speakers`. `ai.auto_speakers` stays off until S7 passes. No voiceprints on disk (AC6 check).

## Later (P2)

VOC-3 live transcript (needs its own board), custom report templates (embedded llama.cpp moved into AI-7 on Oct 27, 2026, as a helper program), per-app audio, FLAC or "Compact this recording".

**Later (backlog, maintainer decision Oct 27, 2026):** AI-7 user-chosen local server through an OpenAI-compatible URL: Ollama, LM Studio (presets, detection, a model picker, a loopback-only address box); and a provider with the user's own key. Marked *later*; not in release 0.2.0.

## Decisions still open

- `profile.display_name` or the macOS account name for "Me" (AI-8.AC7). Default if unanswered: the setting, falling back to "Me".
- Whether Stage 4 and Stage 3 swap places: translation to English works without a text model, so Stage 3 stays first.

## Docs to update as each stage lands

`docs/project/status.md`, `docs/project/plan.md`, `docs/user/user-guide.md`, `CHANGELOG.md`, `locales/`, `specs/plans/design-implementation.md` (traceability for V0 to V8), and the "planned, not built" markers in `specs/02-agent-interface.md`.
