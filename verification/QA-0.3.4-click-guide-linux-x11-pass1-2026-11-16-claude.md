# QA 0.3.4 Click Guide (GDE) on Linux X11 (PARTIAL)

- **Date:** 2026-11-16, Claude (agent). Same machine, session and build as `QA-0.3.4-long-shot-linux-x11-pass1-2026-11-16-claude.md` (main `54a5bc68`).
- **Method:** `capturemann guide start --window <Firefox id> --countdown 1` (approved in the app's dialog), then 5 pointer clicks sent with xdotool on my own fixture page (the long-shot test page, nothing private), `guide stop`, `guide export` to markdown, html and pdf.

| Case | Result | Evidence |
| --- | --- | --- |
| Clicks are heard on X11 | PASS | 5 steps, click points window-relative and matching the 5 clicks: (434,331), (634,451), (334,571), (834,231), (534,731) = screen positions minus the window origin (66,69) |
| Step pictures | PASS | step 1 picture looked at: window shot with a numbered red ring "1" on the click; the approval dialog is not in it |
| Draft titles | PARTIAL | from the text near the click: `Click "ROW 0006) EEE"`, `Click here`, `Click "ROW 0012 go"`: the OCR picks up the bar chart as noise |
| Export markdown | PASS | `guide.md` + `images/step-01..05.png`, headings `1. Click ...` |
| Export html | PASS (file written; not opened) | `QA guide.html`, 1 file |
| Export pdf | PASS (text read) | `QA guide.pdf`; `pdftotext` returns the 5 headings. Layout not looked at. |
| Library: Click Guides count and chip | PASS | after the run the sidebar and the chip showed 1 (seen in the Vietnamese UI) |
| The guide bar, countdown, step toast | not run / not seen | a CLI-started guide shows no bar; floating windows not looked at |
| Wayland: Click Guide disabled with a reason (AC6) | not run | no Wayland session |

Verdict: PARTIAL.
