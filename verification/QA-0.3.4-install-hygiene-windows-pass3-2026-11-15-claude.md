# QA 0.3.4 build, installer and hygiene on Windows, pass 3 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11 Pro 10.0.26200.
- **Source:** main `5e832011` (`git archive HEAD` to `C:\wt\main`; `packaging\windows\build-windows.ps1` through `C:\lab\rc2_build.ps1`, bundles in `C:\wt\bundles`, `CM_NO_SPEAKER_MODELS=1`, LTO off).
- **Files (in `C:\wt\out`, unsigned):** `capturemann-0.3.4-windows-x64-setup.exe` 51,272,450 bytes SHA-256 `9febf0322b639f3be59b2fee44bdd20d14f33dc17bcb65c5507780cf9f699dc8`; `capturemann-0.3.4-windows-x64.zip` 73,170,428 bytes SHA-256 `38abeef6be9b82faf3ca29c92426f9952dc81467e0e4e865423e91f16dfa5151`.

| Step | Result | Evidence |
| --- | --- | --- |
| Silent install over the previous copy (`/S`) | PASS | exit 0; path `C:\Users\trung\AppData\Local\Programs\CaptureMann`; `capturemann --version` = `capturemann 0.3.4`; `cm-desktop.exe` SHA-256 `A86B7B49C47EA4199E918A68F0E0703BF9E0ECBF626A4D0E28A7FADEC078F27C`; uninstall entry `CaptureMann 0.3.4`; Start menu folder present |
| Open With registry entries | PARTIAL | `HKCU\Software\Classes\<ext>\OpenWithProgids` lists only Appx ids for the picture and video types and nothing for audio and bundle types; the earlier report found the handlers through the application key, which I did not re-read; not judged a defect |
| Version label, sidebar tiles Long Shot and Click Guide, Agents & CLI icon with a state dot | PASS (looked at in a screenshot of the app) | `v0.3.4`; tiles present; blue dot on the Agents icon; UI Automation name `Agents & CLI, MCP ready` |
| Library path of 345 characters in the app | PASS | immediate banner "Library unavailable — connect C: ... The library folder's path is too long (345 characters; at most 180 work, and no folder name may be longer than 255): choose a shorter path"; the app opens, sidebar counts hidden |
| Same long path through the CLI (it starts its own daemon) | FAIL (low) | 15 s wait, then `daemon_unavailable: started a daemon but it did not accept connections ... within 15 s`; no mention of the path |
| HUD and bubble drag (B2, B3) | NOT RUN | covered by the regression case `desktop.hud_drag`, which was postponed |
| Library hover-select, selection bar | NOT RUN | not driven |
| cm-regress full and hardware | NOT RUN | postponed by the maintainer's decision during this run |

## Verdict

PARTIAL
