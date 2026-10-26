# Test report

- **commit**: aa6e975
- **date**: 2026-10-26 06:25 UTC
- **rustc**: rustc 1.99.1 (58cf8f9c3 2026-10-13)
- **host**: Darwin arm64
- **result**: PASS: 3234 passed, 0 failed, 41 ignored

Ignored tests need real hardware (screen, camera, microphone), a real Whisper model or a real
account, so they never run in CI. Run them by hand: see `CLAUDE.md` and `tests/manual/`.

| Test binary | Passed | Failed | Ignored | Seconds |
|---|---:|---:|---:|---:|
| `cm_ai` | 97 | 0 | 0 | 4.8 |
| `model_store` | 12 | 0 | 0 | 0.2 |
| `real_model` | 0 | 0 | 2 | 0.0 |
| `recording` | 10 | 0 | 0 | 0.3 |
| `cm_audio` | 79 | 0 | 0 | 2.8 |
| `hardware` | 0 | 0 | 5 | 0.0 |
| `mic_fake` | 22 | 0 | 0 | 1.0 |
| `mic_mute` | 8 | 0 | 0 | 0.3 |
| `perf` | 0 | 0 | 1 | 0.0 |
| `cm_auth` | 47 | 0 | 1 | 0.1 |
| `cm_camera` | 33 | 0 | 0 | 0.1 |
| `mac_camera` | 1 | 0 | 1 | 4.4 |
| `recorder` | 21 | 0 | 0 | 0.0 |
| `cm_capture` | 88 | 0 | 0 | 0.4 |
| `real_display` | 0 | 0 | 5 | 0.0 |
| `real_window_thumbs` | 0 | 0 | 2 | 0.0 |
| `record_real` | 0 | 0 | 9 | 0.0 |
| `cm_cli` | 9 | 0 | 0 | 0.0 |
| `capturemann` | 0 | 0 | 0 | 0.0 |
| `ai` | 7 | 0 | 0 | 0.4 |
| `cli` | 52 | 0 | 0 | 1.8 |
| `cm_core` | 101 | 0 | 0 | 0.1 |
| `cm_cursor` | 44 | 0 | 3 | 1.4 |
| `cm_daemon` | 108 | 0 | 1 | 0.8 |
| `agent_layer` | 20 | 0 | 0 | 2.0 |
| `ai_engine` | 3 | 0 | 0 | 1.4 |
| `ai_ops` | 25 | 0 | 0 | 1.4 |
| `auto_upload` | 11 | 0 | 0 | 1.3 |
| `bundle_resend` | 9 | 0 | 0 | 4.3 |
| `camera_events` | 2 | 0 | 0 | 1.1 |
| `caption_options` | 3 | 0 | 0 | 0.1 |
| `confirm` | 18 | 0 | 0 | 0.2 |
| `e2e` | 10 | 0 | 0 | 0.3 |
| `edit_ops` | 14 | 0 | 0 | 0.4 |
| `edit_view_ops` | 5 | 0 | 0 | 0.2 |
| `export` | 11 | 0 | 0 | 0.8 |
| `export_real` | 6 | 0 | 0 | 6.6 |
| `fps_selftest` | 22 | 0 | 0 | 1.2 |
| `frames` | 4 | 0 | 0 | 0.2 |
| `jobs` | 10 | 0 | 0 | 0.2 |
| `library_move` | 10 | 0 | 0 | 0.6 |
| `library_ops` | 14 | 0 | 0 | 0.5 |
| `library_watch` | 15 | 0 | 0 | 1.0 |
| `publish` | 24 | 0 | 0 | 1.6 |
| `real_selftest` | 0 | 0 | 1 | 0.0 |
| `recording_engine` | 65 | 0 | 0 | 8.7 |
| `recording_rpc` | 13 | 0 | 0 | 2.6 |
| `recording_session` | 29 | 0 | 0 | 2.4 |
| `remote_delete` | 22 | 0 | 0 | 2.8 |
| `storage_cloud` | 10 | 0 | 0 | 0.3 |
| `storage_keychain` | 7 | 0 | 0 | 1.3 |
| `storage_ops` | 8 | 0 | 0 | 0.4 |
| `suggest_redactions` | 5 | 0 | 0 | 0.4 |
| `tool_policy` | 15 | 0 | 0 | 0.3 |
| `window_thumbnail` | 4 | 0 | 0 | 0.3 |
| `cm_desktop` | 1256 | 0 | 2 | 14.4 |
| `font_coverage` | 3 | 0 | 0 | 0.0 |
| `layout_guard` | 2 | 0 | 0 | 0.2 |
| `parity` | 3 | 0 | 0 | 21.7 |
| `text_input` | 2 | 0 | 0 | 0.0 |
| `cm_encode` | 23 | 0 | 0 | 0.6 |
| `mac_encode` | 0 | 0 | 2 | 0.0 |
| `pipeline_fakes` | 33 | 0 | 0 | 0.3 |
| `cm_i18n` | 37 | 0 | 0 | 1.6 |
| `catalogues` | 3 | 0 | 0 | 0.3 |
| `pseudo_env` | 1 | 0 | 0 | 0.0 |
| `cm_ipc` | 7 | 0 | 0 | 0.0 |
| `endpoint_env` | 1 | 0 | 0 | 0.0 |
| `socket_roundtrip` | 10 | 0 | 0 | 0.1 |
| `cm_jobs` | 8 | 0 | 0 | 0.0 |
| `queue` | 24 | 0 | 0 | 0.2 |
| `cm_library` | 29 | 0 | 0 | 1.1 |
| `bundle` | 19 | 0 | 0 | 0.3 |
| `forward_compat` | 2 | 0 | 0 | 0.1 |
| `library` | 19 | 0 | 0 | 2.4 |
| `replace_diff` | 8 | 0 | 0 | 2.0 |
| `cm_mcp` | 16 | 0 | 0 | 0.4 |
| `mcp` | 36 | 0 | 0 | 0.6 |
| `schema_parity` | 3 | 0 | 0 | 0.0 |
| `stdio` | 1 | 0 | 0 | 0.8 |
| `cm_media` | 104 | 0 | 0 | 0.2 |
| `caption_export` | 4 | 0 | 0 | 16.1 |
| `decode` | 4 | 0 | 1 | 1.1 |
| `edit_view_export` | 5 | 0 | 0 | 7.7 |
| `quick_export` | 45 | 0 | 3 | 21.2 |
| `cm_overlay` | 91 | 0 | 0 | 0.1 |
| `session` | 3 | 0 | 0 | 0.1 |
| `cm_publish` | 50 | 0 | 0 | 0.1 |
| `cm_render` | 37 | 0 | 1 | 2.6 |
| `compose` | 16 | 0 | 0 | 4.6 |
| `compose_caption` | 8 | 0 | 0 | 4.6 |
| `compose_cursor` | 5 | 0 | 0 | 4.7 |
| `compose_perf` | 1 | 0 | 0 | 6.5 |
| `fallback_fonts` | 5 | 0 | 0 | 0.1 |
| `golden` | 12 | 0 | 0 | 0.1 |
| `ocr_real` | 0 | 0 | 1 | 0.0 |
| `properties` | 23 | 0 | 0 | 0.2 |
| `cm_storage` | 122 | 0 | 0 | 0.5 |
| **Total** | **3234** | **0** | **41** | |
