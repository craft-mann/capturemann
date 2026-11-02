# L1 Windows early build on a physical PC (ThinkPad T490)

- **Date:** 2026-11-02
- **Who:** Claude (AI agent) drove the laptop from a Mac over ssh and inside the logged-in desktop session (a scheduled task with `/IT`, `keybd_event`, a GDI screen grab); the maintainer looked at the tray icon and the tray menu
- **Machine:** Lenovo ThinkPad T490, Intel UHD 620, one 1920x1080 panel at 150 % scaling (1280x720 logical), light taskbar, Windows 11 Pro, Windows PowerShell 5.1, dual boot with Ubuntu; not signed in to a domain
- **Build:** zip and per-user installer 0.2.6 from CI run 41432840948 (commit `c485c25`, workflow `windows-package.yml` through a temporary push trigger); the first checks (marked 0.2.5) used the package of run 41379754331 (commit `61f025e`)
- **Status:** PARTIAL (SmartScreen, the second monitor, Windows 10, a real GPU recording are not covered)

| Step | Result | Notes |
| --- | --- | --- |
| `.sha256` of the zip and the installer (both builds) | PASS | `Get-FileHash` equals the file |
| Unpack the zip; icon of `cm-desktop.exe` | PASS (0.2.6) / FAIL (0.2.5) | 0.2.5: generic icon. 0.2.6: `ExtractAssociatedIcon` gives the CaptureMann icon (dark tile, corner brackets, red dot); the title bar and the taskbar show it too |
| `scripts/ci/windows-smoke.ps1` on the unpacked zip (daemon on a named pipe, CLI, a display screenshot, diagnostics) | PASS (both builds, after a fix) | first run failed under Windows PowerShell 5.1: the CLI answered but `$p.ExitCode` was empty without reading the process handle; fixed in `9ee794a`. Display `65537 'Unknown Monitor 65537'`, physical 1920x1080, scale 1.5; the PNG is 1920x1080 |
| App window (0.2.5) | FAIL then fixed | title bar half above the top of the screen (a 1180x760 logical window on a 1280x720 logical screen); the CI runner's 1024x768 showed it too. Fixed in `2a6d582` (the size is fitted to the display) |
| App window (0.2.6) | PASS | in a screenshot the whole title bar is visible with the icon, minimise, maximise and close, the version reads v0.2.6, the window ends above the taskbar |
| Global hotkey `Ctrl+Alt+Shift+2` (window picker), `Esc` closes it | PASS | sent with `keybd_event` |
| Window picker list (0.2.5) | FAIL then fixed | the only entry was the taskbar as "Windows Explorer 1920x72". Fixed in `0a91a44` (the Alt-Tab rule: tool and never-active windows and cloaked ones are dropped; styles read on the laptop: `Shell_TrayWnd` 0x88, `Progman` 0x200080, CaptureMann 0x240100) |
| Window picker list (0.2.6) | PASS | with Notepad, Command Prompt and Settings open it lists those three with live previews; no taskbar. The app names in the left rail are poor: "C:\Program Files\Wi..." for Notepad (packaged app path), "Windows Terminal H...", "Application Frame H..." (cosmetic, not changed) |
| Window capture by Enter in the picker (0.2.6) | PASS | Notepad saved as 1426x721; the picture shows the whole window (title bar, toolbar, text area, status bar) with no black border and no shift. The picker listed 1426x720: one pixel less than the file (scale 1.5 rounding), not investigated |
| Display capture by `Ctrl+Alt+Shift+3` (0.2.5) | PASS | a 1920x1080 capture appears in the library. One later press after restarting the app with debug logging did nothing and left a `cm-overlay` process running; not reproduced, not explained |
| Result toast (0.2.6) | PASS | "Copied to clipboard / Saved to your library" with the close button, the hints and Annotate, Copy, Save as and Delete visible. With 0.2.5 no toast was seen in two grabs (unexplained; the only toast change in 0.2.6 is the height estimate) |
| Tray icon (0.2.6) | PASS | in the overflow panel the icon is a dark glyph on the light panel (0.2.5: white on white, reported by the maintainer; fixed in `d956572`: `SystemUsesLightTheme` picks the variant). The maintainer: clicking it shows the menu and the items work (0.2.5) |
| Installer silent install | PASS | `setup.exe /S` exit 0; 14 files in `%LOCALAPPDATA%\Programs\CaptureMann`; Start-menu entries "CaptureMann" (target the installed `cm-desktop.exe`) and "Uninstall CaptureMann"; uninstall entry `CaptureMann 0.2.6`, publisher "CaptureMann contributors" |
| Installer silent uninstall | PASS | exit 0; the install folder, the Start-menu folder and the uninstall entry are gone |
| SmartScreen warning on a downloaded file | NOT RUN | the files were copied over ssh and carry no "downloaded from the internet" mark; needs the real release download in a browser |
| Region capture, Settings, editor, text recognizer, second monitor, Windows 10, recording | NOT RUN | |

## Problems found

- Fixed with the commits above: title bar off screen, taskbar in the window list, icon of the exe and the window, white tray glyph on a light taskbar, smoke script under PowerShell 5.1.
- Not fixed: the picker's app names for packaged apps; the 1 px difference between the listed and the saved window size; the display name "Unknown Monitor 65537"; diagnostics say "CaptureMann releases include their own copy" for FFmpeg and `llama-server`, which the Windows package does not have; the Windows build has no text recognizer and no speaker engine (known).
- Unexplained: the missing toast with 0.2.5; the one hotkey press that did nothing with a leftover `cm-overlay`.

## Verdict

PARTIAL. The package installs, runs, captures a display and windows, shows the toast and the tray icon, and uninstalls cleanly on this laptop. Not yet checked: SmartScreen on a real download, more than one monitor and a Windows version other than 11.
