# QA 0.3.4 Long Shot on Windows, pass 5 (PARTIAL, mostly NOT RUN)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %. **Build:** main `0bcac6a3`, installed 0.3.4.

| Case | Result | Evidence |
| --- | --- | --- |
| Windows Terminal, 400 numbered lines, scrolled to the top, Auto, `capture screenshot --window QA-WT --long` | PASS | `reached_end`, 18 frames, 1721x11427, header 76, footer 20, 7.9 s; I looked at the whole image: lines 001 to 400 and "END OF OUTPUT", each once, in order. OCR: 394 of 399 numbers read, no duplicates, none out of order (missing 4, 7, 40, 47, 200 are OCR misses). The earlier ending at line 111 does not happen. |
| Classic console host, covered target, HUD Stop, Esc, window moved/resized/closed (`window_changed`), second start, Hand scroll, passive HUD of a CLI run, sidebar widths | NOT RUN | harness time (below) |

## Harness finding (not a product defect, but a trap)

With Windows Terminal as the default terminal ("Let Windows decide" on this Windows 11, no `HKCU\Console\%%Startup` key), once Windows Terminal had been started, every console program started from the hidden scheduled-task session died at once with exit `0xC0000142` (`capturemann.exe` printed nothing). That is the pass-4 "hung wt.exe" symptom. Setting `DelegationConsole` and `DelegationTerminal` to the console host GUID `{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}` fixed it; I removed the key again at the end. Also: Out-Host output from a hidden PowerShell fails (`0xE9`), so scripts must write to files.

## Verdict

PARTIAL
