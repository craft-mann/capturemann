# QA 0.3.6 macOS release DMGs (commit 96b85253), 2026-11-21, Claude

Verdict: PARTIAL (checks and a regression run, not a person's verification).

## DMG files (read-only use, nothing modified)
- arm64 `target/bundle/CaptureMann-0.3.6-arm64.dmg` sha256 7cdc5fdefedf60321d6b3b2fde7fb141bb5607296910d1092576d79566220586
- Intel `target/bundle-x86_64/CaptureMann-0.3.6-x86_64.dmg` sha256 c52e6256b7d81031bbfac2c7e21d53aa7ce006b240002007eb197ca8df2942ce
(both match the expected values)

## DMG checks (both pass)
`bash packaging/macos/verify-release.sh --arch <arch> <dmg>`: "result: passed, 0 warning(s)" for both (it mounts read-only and detaches; nothing installed). Plus by hand on each mounted app:
- `codesign --verify --deep --strict` ok; Developer ID Application: TRUNG TRAN (DJ2ALRFN66), hardened runtime, secure timestamp on all 7 Mach-O files; entitlements equal the repo file, no get-task-allow
- `spctl -a -vv` on the app: accepted, Notarized Developer ID; DMG accepted (verify-release); `stapler validate` on the DMG: ticket present (the app carries none, normal)
- version 0.3.6, bundle id dev.capturemann.app, LSMinimumSystemVersion 13.0
- `lipo -archs` of capturemann, cm-desktop, cm-mcp, cm-overlay, ffmpeg, ffprobe, llama-server: arm64 only in the arm64 DMG, x86_64 only in the Intel DMG; no file named for the other chip
- LGPL FFmpeg (no gpl/nonfree/libx264), ffprobe, SOURCE-OFFER and LICENSES; llama-server without curl/OpenSSL, llama.cpp licence; both speaker models match their pinned SHA-256 with notices; LICENSE, NOTICE, THIRD_PARTY_LICENSES.md; no cm-daemon in the bundle (expected)
- The Intel app was NOT run: checks only (no Intel hardware).

## cm-regress on the arm64 release app
App copied with `ditto` from the read-only mounted DMG to a temp folder. Headless cases used a release `cm-daemon` rebuilt from 96b85253 (the earlier binary from Nov 20 was stale; `cargo build --release -p cm-daemon -p cm-regress` was run first). `--allow screen`, `--timeout-scale 3`, `--installer <arm64 dmg>`.

| Tier | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard | 52 | 0 | 12 | `L1-macos-0.3.6-dmg-96b85253-regression-standard-2026-11-21-claude.md` |
| full | 58 | 1 | 21 | `...-full-...` |
| hardware | 59 | 0 | 29 | `...-hardware-...` |

Failure: `installer.dmg` in the full run: "hdiutil attach: Resource busy". Lab cause, not product: I still had the same DMG mounted from my own verification step. After detaching, the hardware run passed it, and a rerun of `--only installer.dmg` passed (`...-regression-installer-rerun-...`, 1/1). The full report is kept as it was.

Skips are the usual ones: cases not implemented on macOS (shot.window, redact.window_text, desktop.hud_*, recording_banner, recording_placement, no_console_flash, settings_applied, guide.real_clicks, rec.each_monitor, installer.silent, all linux.*), cases needing flags not given (mic, camera, sound, second monitor, 4K, soak; ai.transcribe/ai.speakers need --whisper-model and --speaker-models), and `regress.ffmpeg_missing_wording` (a Homebrew ffmpeg in /opt/homebrew/bin cannot be hidden).

## Not run
Intel Mac run (hardware absent), microphone, camera, system audio, second monitor, soak, transcription and speaker cases with real models, window-based screenshot and Auto-redact cases (not implemented on macOS), Retype and any manual scripts.

## Isolation
Fresh per-case library/config/socket/file secrets (the suite's own sandboxes). The maintainer's library `~/Movies/CaptureMann`: 386 bundle folders (find, depth 3) before and 386 after. The maintainer's dev app and its cm-mcp processes were running and not touched; no desktop case reported a collision. Mounts detached; temp dirs removed.
