# QA 0.3.4 install on Windows, pass 5 (PARTIAL)

- **Date:** 2026-11-15. **Machine:** ThinkPad T490, Windows 11. **Build:** main `0bcac6a3` (`git archive`, `build-windows.ps1` via `rc2_build.ps1`, LTO off, `CM_NO_SPEAKER_MODELS=1`), about 20 minutes. Build started from ssh with `Start-Process` died with the ssh session; a scheduled task kept it alive.

| Case | Result | Evidence |
| --- | --- | --- |
| Silent install over the installed copy | PASS | exit 0, `capturemann 0.3.4`, 16 entries in `%LOCALAPPDATA%\Programs\CaptureMann`, `ffmpeg.exe` and `cm-desktop.exe` present, Open With key present, Start Menu entry `CaptureMann`, uninstall entry 0.3.4 |
| `cm-desktop.exe` SHA-256 | recorded | `979E002710ACF2DECBF6725CC7F99B4C747F6B31BCED8C97C8700A62960DD014` |
| Uninstall and reinstall, Open With use | NOT RUN | |

## Verdict

PARTIAL
