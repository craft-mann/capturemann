# L1 Windows early build on a GitHub runner (zip, installer, CLI, daemon, app, screenshots)

- **Date:** 2026-11-01
- **Who:** Claude (AI agent); nothing by a person
- **Machine:** GitHub `windows-latest` (Windows Server 2025 desktop session, 1024x768 Hyper-V monitor, no GPU, no camera, no microphone)
- **Build:** release, `packaging/windows/build-windows.ps1`, `--no-default-features`, static CRT; runs 41284759898, 41286371526, 41290935487 of workflow `windows-package.yml` (commit `8901aad` for the last)
- **Status:** PARTIAL: a CI runner is not a physical PC (no real GPU, no SmartScreen, no real user profile)

| Step | Result | Note |
| --- | --- | --- |
| Build zip (48 MB) and per-user NSIS installer (32 MB) | PASS | `cm-desktop`, `cm-overlay`, `capturemann`, `cm-mcp`, `cm-daemon`; programs link Windows system DLLs only (no vcruntime, no GPL libraries) |
| `THIRD_PARTY_LICENSES.md` for the Windows crate set | PASS (run 2) | run 1 fell back to the macOS list: the runner's Python read the files as cp1252; fixed with `PYTHONUTF8=1` |
| `.sha256` files readable by `sha256sum -c` | PASS (run 3) | run 1 and 2 had CRLF; fixed |
| Zip: `capturemann --version`, `targets` (1 display, windows listed), `diagnostics`, `library list` | PASS | the CLI started `cm-daemon.exe` next to it on a named pipe; secrets: system credential store |
| CLI screenshot of the display | PASS | 1024x768 px, equal to the display's size |
| `cm-desktop.exe` start, 25 s, screenshot of the desktop | PASS | stays up, window title "CaptureMann", the app draws its UI (first-run permissions page, then the library) |
| Silent install (`/S /D=`), run, silent uninstall | PASS | exit codes 0 |
| Real-display tests (`cm-capture`, Windows): display size, region equals crop | PASS | |
| Real-display tests: window capture size equals listed bounds | FAIL then fixed | Notepad bounds 754x512, image 747x510 (console window 1030x628 vs 1023x628). Cause: xcap's GDI window capture scales the whole window rectangle to the frame size and crops at the border offset, so its picture was shrunk and shifted 7 px. `sys/win_window.rs` now paints the window (`PrintWindow`) and cuts out the DWM frame; on a runner image 754x512 = bounds, and an alignment check against a display capture found the best shift (0, 0) (6 per mille of sampled pixels differ, an idle Notepad). An attempt using the capture item size was wrong and reverted. |
| Full `cargo test --no-default-features` on Windows (CI) | FAIL then fixed | one test used a Unix path (`user_dirs_file_rules`), now Unix-only; a clippy dead-code error on Windows (`command_for`) fixed |
| A physical Windows PC (clean install, SmartScreen, real GPU, high-DPI, uninstall leaves nothing) | NOT RUN | the ThinkPad |

Open items found: the app window's title bar sits partly above the screen on the 1024x768
runner (seen in `shots/desktop-with-app.png`); no Windows text recogniser (Auto-redact needs `tesseract`, the hint now names
`winget install UB-Mannheim.TesseractOCR`); "FFmpeg not found ... Homebrew" in diagnostics; no code signing (SmartScreen).

Update, same day: CI run 41322201474 on `fbd33d4` was green on all jobs including Windows (clippy, full tests, real-display tests, `windows-smoke.ps1`, trash test). `windows-smoke.ps1` also passed on the packaged binaries (package run 41321049994). Still PARTIAL: no physical PC yet.
