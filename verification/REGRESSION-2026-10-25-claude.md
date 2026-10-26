# Regression run, Oct 25, 2026 (macOS, Apple M4 Pro, build of `main` plus the fixes below)

- **Who:** Claude, on the maintainer's request ("run the screen, camera and mic tests, this is fine").
- **Not covered:** anything by eye (UI), Drive/YouTube accounts, long soaks, optical/acoustic sync (NFR-3), 4K+camera.

| Check | Result |
|---|---|
| fmt, clippy `-D warnings`, `cargo deny`, `cm-i18n check` | clean |
| `cargo test --workspace` | 2,834 passed before the fixes; the 2 failures were doctests that could not link (Xcode licence not accepted), re-run clean afterwards |
| `record_e2e` screen+cursor+mic+camera+system, 10 s with a 2 s pause | 74 pass, 0 fail, 1 warn (system audio was digital silence: nothing playing), NFR-3 skipped |
| `record_e2e` 20 s x2 | 65 pass, 0 fail; late frames 0 to 0.17 % |
| `record_e2e` 6 s | one fail: 2 late of 166 frames = 1.2 % (NFR-2) while `cargo` was compiling; not repeated in 3 further runs |
| `record_e2e` native 3024x1964, 60 fps, HEVC, camera, 15 s | 58 pass, 0 fail; late 0.34 % |
| `record_e2e --kill9-test` | 10 pass: recovered, 1.26 s lost |
| Ignored hardware tests (`--ignored`, one at a time) | 35 pass; `real_fps_selftest` failed because the screen was locked (no displays), not re-run; the 5K blur budget fails in a debug build and passes in release (55 ms); 2 `ignore`d doctests do not compile and are not tests |
| CLI against a headless daemon on a throwaway library | list/search/get/settings/edit (trim, split)/undo/redo/export (quick, youtube-1080p, vertical, gif, webm; ffprobe checked)/jobs/ai/devices/targets; exit codes 0, 2, 3 as documented; `--request-id` replays; confirm-gated `record start` and `library rm` refused with `no_confirmer` |
| MCP over stdio and HTTP with a scripted client | `initialize`, 53 tools, 3 resources + 3 templates, 6 prompts, tool calls, gated and app-only refusals, unknown tool/resource errors; HTTP: bearer token, session id, wrong token 401, foreign Origin rejected; `capturemann mcp` launches the same server |

## Bugs found and fixed in this run

1. `cm-daemon` and `cm-mcp` ignored `CAPTUREMANN_SOCKET` (the app, CLI and overlay honoured it). Shared `Endpoint::from_env_or_default`.
2. `set_settings` by an agent accepted any key outside the deny list (`nonsense.key` was stored). Agents may now write only the families `recording`, `camera`, `audio`, `editor`, `cursor`, `capture`, `appearance` (app-only keys inside them stay refused).
3. `get_capture` / the capture resource on a recording said "id: not a screenshot"; it now names `capture_id` and points to `list_library`.

## Also done after the run

4. The keychain vault (`cm-auth` `VaultStore`) read the keychain again on every missing key and on every write's legacy check; each read can ask for the password. A miss is now remembered for 30 s, a read made in the last 30 s is not repeated, and the old-layout item is looked for once per key (test: 150 lookups with two absent keys make at most 3 keychain reads). Tested with a counting fake; the real prompt count is for the maintainer to confirm.

## Open

- `real_fps_selftest` re-run with the screen unlocked: passed (600 frames at 3024x1964 HEVC, 0 dropped, 0 stalls, "60 fps is stable on this machine at 1964p").
- A locked screen makes `list_targets` return no displays and recording say "No display found"; a clearer message would help.

## Second hardware pass (screen unlocked, merged `main` with the zoom-range and tray-Attention branches)

`cargo test --workspace --locked --lib --bins --tests -- --ignored --test-threads=1`: 36 passed, 1 failed. The failure is
`r1_8_five_k_full_frame_blur_is_under_the_budget`, which is marked "release build only" (949 ms in debug, 55 ms in release). `real_fps_selftest`
passed. Full non-ignored suite on the same tree: 2,854 passed, 0 failed, 39 ignored; fmt, clippy, `cargo deny`, `cm-i18n check` clean.
