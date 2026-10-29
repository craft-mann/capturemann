# Test report

- **commit**: eb05939
- **date**: 2026-10-29 04:38 UTC
- **rustc**: rustc 1.99.1 (58cf8f9c3 2026-10-13)
- **host**: Darwin arm64
- **result**: PASS: 3664 passed, 0 failed, 44 ignored

Ignored tests need real hardware (screen, camera, microphone), a real Whisper model or a real
account, so they never run in CI. Run them by hand: see `CLAUDE.md` and `tests/manual/`.

| Test binary | Passed | Failed | Ignored | Seconds |
|---|---:|---:|---:|---:|
| `cm_ai` | 245 | 0 | 0 | 2.9 |
| `model_store` | 14 | 0 | 0 | 0.3 |
| `real_diarize` | 0 | 0 | 2 | 0.0 |
| `real_model` | 0 | 0 | 2 | 0.0 |
| `real_stream` | 0 | 0 | 1 | 0.0 |
| `recording` | 12 | 0 | 0 | 0.4 |
| `text_model` | 25 | 0 | 0 | 1.0 |
| `cm_audio` | 83 | 0 | 0 | 5.1 |
| `hardware` | 0 | 0 | 5 | 0.0 |
| `mic_fake` | 22 | 0 | 0 | 0.9 |
| `mic_mute` | 8 | 0 | 0 | 0.3 |
| `perf` | 0 | 0 | 1 | 0.0 |
| `cm_auth` | 47 | 0 | 1 | 0.2 |
| `cm_camera` | 33 | 0 | 0 | 0.1 |
| `mac_camera` | 1 | 0 | 1 | 5.2 |
| `recorder` | 21 | 0 | 0 | 0.0 |
| `cm_capture` | 89 | 0 | 0 | 0.5 |
| `real_display` | 0 | 0 | 5 | 0.0 |
| `real_window_thumbs` | 0 | 0 | 2 | 0.0 |
| `record_real` | 0 | 0 | 9 | 0.0 |
| `cm_cli` | 9 | 0 | 0 | 0.0 |
| `capturemann` | 0 | 0 | 0 | 0.0 |
| `ai` | 7 | 0 | 0 | 0.4 |
| `ai_speakers` | 20 | 0 | 0 | 2.6 |
| `ai_translate` | 10 | 0 | 0 | 0.8 |
| `cli` | 54 | 0 | 0 | 1.8 |
| `cm_core` | 101 | 0 | 0 | 0.1 |
| `cm_cursor` | 44 | 0 | 3 | 1.4 |
| `cm_daemon` | 116 | 0 | 1 | 0.8 |
| `agent_layer` | 20 | 0 | 0 | 2.4 |
| `ai_engine` | 4 | 0 | 0 | 1.8 |
| `ai_engine_translate` | 1 | 0 | 0 | 0.6 |
| `ai_ops` | 28 | 0 | 0 | 2.5 |
| `ai_reports` | 6 | 0 | 0 | 0.3 |
| `auto_upload` | 11 | 0 | 0 | 1.2 |
| `bundle_resend` | 9 | 0 | 0 | 4.4 |
| `camera_events` | 2 | 0 | 0 | 1.1 |
| `caption_options` | 3 | 0 | 0 | 0.1 |
| `confirm` | 18 | 0 | 0 | 0.2 |
| `e2e` | 10 | 0 | 0 | 0.3 |
| `edit_ops` | 14 | 0 | 0 | 0.4 |
| `edit_view_ops` | 5 | 0 | 0 | 0.1 |
| `export` | 11 | 0 | 0 | 0.8 |
| `export_audio` | 2 | 0 | 0 | 0.8 |
| `export_real` | 6 | 0 | 0 | 8.5 |
| `fps_selftest` | 22 | 0 | 0 | 1.3 |
| `frames` | 4 | 0 | 0 | 0.2 |
| `jobs` | 10 | 0 | 0 | 0.1 |
| `library_move` | 10 | 0 | 0 | 0.6 |
| `library_ops` | 14 | 0 | 0 | 0.5 |
| `library_watch` | 15 | 0 | 0 | 1.0 |
| `publish` | 24 | 0 | 0 | 1.6 |
| `real_selftest` | 0 | 0 | 1 | 0.0 |
| `recording_engine` | 65 | 0 | 0 | 7.6 |
| `recording_rpc` | 13 | 0 | 0 | 2.6 |
| `recording_session` | 29 | 0 | 0 | 2.6 |
| `remote_delete` | 22 | 0 | 0 | 1.9 |
| `storage_cloud` | 10 | 0 | 0 | 0.1 |
| `storage_keychain` | 7 | 0 | 0 | 1.2 |
| `storage_ops` | 8 | 0 | 0 | 0.5 |
| `suggest_redactions` | 5 | 0 | 0 | 0.3 |
| `text_engine` | 9 | 0 | 0 | 0.1 |
| `tool_policy` | 15 | 0 | 0 | 0.2 |
| `voice_audio` | 16 | 0 | 0 | 1.1 |
| `window_thumbnail` | 4 | 0 | 0 | 0.3 |
| `cm_desktop` | 1393 | 0 | 2 | 8.5 |
| `font_coverage` | 3 | 0 | 0 | 0.0 |
| `layout_guard` | 2 | 0 | 0 | 0.2 |
| `parity` | 3 | 0 | 0 | 27.1 |
| `text_input` | 2 | 0 | 0 | 0.0 |
| `cm_encode` | 24 | 0 | 0 | 0.6 |
| `mac_encode` | 1 | 0 | 2 | 7.3 |
| `pipeline_fakes` | 33 | 0 | 0 | 0.3 |
| `cm_i18n` | 37 | 0 | 0 | 1.3 |
| `catalogues` | 3 | 0 | 0 | 0.4 |
| `pseudo_env` | 1 | 0 | 0 | 0.0 |
| `cm_ipc` | 7 | 0 | 0 | 0.0 |
| `endpoint_env` | 1 | 0 | 0 | 0.0 |
| `socket_roundtrip` | 10 | 0 | 0 | 0.1 |
| `cm_jobs` | 8 | 0 | 0 | 0.0 |
| `queue` | 24 | 0 | 0 | 0.2 |
| `cm_library` | 35 | 0 | 0 | 1.3 |
| `audio` | 4 | 0 | 0 | 0.2 |
| `bundle` | 19 | 0 | 0 | 0.3 |
| `forward_compat` | 2 | 0 | 0 | 0.1 |
| `library` | 19 | 0 | 0 | 2.4 |
| `replace_diff` | 8 | 0 | 0 | 2.1 |
| `cm_mcp` | 16 | 0 | 0 | 0.4 |
| `mcp` | 37 | 0 | 0 | 0.6 |
| `schema_parity` | 3 | 0 | 0 | 0.1 |
| `stdio` | 1 | 0 | 0 | 0.9 |
| `cm_media` | 119 | 0 | 0 | 0.4 |
| `audio_export` | 5 | 0 | 0 | 2.3 |
| `caption_export` | 4 | 0 | 0 | 15.9 |
| `decode` | 4 | 0 | 1 | 0.2 |
| `edit_view_export` | 5 | 0 | 0 | 15.7 |
| `quick_export` | 45 | 0 | 3 | 21.9 |
| `cm_overlay` | 91 | 0 | 0 | 0.1 |
| `session` | 3 | 0 | 0 | 0.1 |
| `cm_publish` | 50 | 0 | 0 | 0.1 |
| `cm_render` | 37 | 0 | 1 | 2.6 |
| `compose` | 16 | 0 | 0 | 12.8 |
| `compose_caption` | 8 | 0 | 0 | 12.0 |
| `compose_cursor` | 5 | 0 | 0 | 12.8 |
| `compose_perf` | 1 | 0 | 0 | 14.3 |
| `fallback_fonts` | 5 | 0 | 0 | 0.1 |
| `golden` | 12 | 0 | 0 | 0.1 |
| `ocr_real` | 0 | 0 | 1 | 0.0 |
| `properties` | 23 | 0 | 0 | 0.2 |
| `cm_storage` | 122 | 0 | 0 | 0.6 |
| **Total** | **3664** | **0** | **44** | |
