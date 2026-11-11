# L1 Windows 0.3.2 extra checks, candidate files of commit 4f96ba59 (Nov 11, 2026)

- **Verdict:** PARTIAL (scripted run by an agent, no person looked; no screen, sound, microphone or camera used)
- **Files:** `capturemann-0.3.2-windows-x64-setup.exe` (package run 43038532341), installed for the user on the ThinkPad (Windows 11 Pro 25H2 build 26200, Intel UHD 620)
- **Private data:** a throw-away library, config folder and socket under `C:\lab\scratch\x032` (removed afterwards); the screenshot bundle was hand-written (same layout as the regression's seeded fixture) with a PNG rendered by System.Drawing (two lines of Segoe UI 28 text).

| Check | Result |
| --- | --- |
| `capturemann text --help` | prints; options `--json`, `--region`, `--copy`, `--lines`, `--socket` |
| `capturemann text <id>` | exit 0; "Hello CaptureMann release check" / "Invoice 4821 total 99.50 USD" (both lines exact, 9 words) |
| `capturemann text <id> --lines --json` | `recognizer: "windows_ocr"`, 2 lines with boxes, confidence 1.0, no warnings, `truncated: false` |
| OCR languages on the laptop | Windows capability `Language.OCR~~~en-US` installed; user language list en-US and vi (vi has no OCR pack) |
| `capturemann diagnostics` recognizer line | `text recognizer: windows_ocr (available; reads en-US (the first language of your Windows language list with OCR); OCR languages installed: en-US)` |
| `capturemann mcp` over stdio | `initialize` answered (protocol 2025-06-18, serverInfo capturemann 0.3.2); `tools/list` returned 81 tools, `recognize_text` among them |

Not checked: a language other than en-US (none installed), a screenshot taken from the real screen, the plugin's `${user_config.capturemann_path}` substitution (the command `capturemann mcp` was run directly from the installed path).
