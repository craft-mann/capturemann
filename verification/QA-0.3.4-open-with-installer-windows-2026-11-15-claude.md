# QA 0.3.4 Open With, file registration and the NSIS installer on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine:** ThinkPad T490, Windows 11 Pro 10.0.26200 (the laptop's own CaptureMann 0.3.3 install stays in `%LOCALAPPDATA%\Programs\CaptureMann`)
- **Build:** installer `capturemann-0.3.4-windows-x64-setup.exe` (51,197,327 bytes) built on the laptop by `packaging\windows\build-windows.ps1` with NSIS from main `26a05100`; no speaker models (`CM_NO_SPEAKER_MODELS=1`)
- **Method:** a script that exports the registry keys the installer shares with a real install (and the Start menu folder), runs `setup.exe /S /D=C:\lab\qa\inst`, reads the registry, runs the installed programs, starts the registered Open with command with a PNG path, runs `uninstall.exe /S`, imports the saved keys. `cm-regress --installer` (case `installer.silent`) does the same with its own checks: see the regression report in this folder for its result.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Silent install into a chosen folder | PASS | exit 0; folder holds `capturemann.exe`, `cm-daemon.exe`, `cm-desktop.exe`, `cm-mcp.exe`, `cm-overlay.exe`, `ffmpeg.exe`, `ffprobe.exe`, `llama-server.exe`, `Llama\`, `FFmpeg\`, `doc\`, `app.ico`, `path.ps1`, `README.txt`, `uninstall.exe` |
| Installed programs run | PASS | `capturemann diagnostics` printed `CaptureMann 0.3.4` |
| Registered handlers | PASS (what the installer promises) | `HKCU\Software\Classes\Applications\cm-desktop.exe\shell\open\command` = `"C:\lab\qa\inst\cm-desktop.exe" "%1"`; `OpenWithList\cm-desktop.exe` for `.png .jpg .jpeg .mp4 .mov .m4v .mkv .m4a .wav .mp3`. Per user, never the default program. |
| File types of the library bundles (`.cmrec`, `.cmshot`, `.cmguide`) | not registered | `reg query HKCU\Software\Classes\.cmrec`, `.cmshot` and `.cmguide` find nothing. The installer does not register them (only the import formats appear in Open with). Not a defect against any spec text I found; noted because a double-click on a bundle folder is a path, not a file type. |
| The Open with command with a PNG path | PASS (starts) | the app started, window title `CaptureMann`, process alive after 12 s; whether the image was imported into the library was not checked in this run. Forwarding to a running instance was seen in the earlier report. |
| Silent uninstall | PASS | exit 0; the install folder is gone |
| The machine's own registration restored | PASS | `HKCU\Software\CaptureMann\InstallDir` = the 0.3.3 folder again; the Start menu folder is back |
| Installer run as `cm-regress` `installer.silent` | see the full-tier report of the final build | |

## Verdict

PARTIAL (Windows 10, SmartScreen, an installer launched by double click and the wizard pages were not run)
