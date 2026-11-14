# GDE (Click Guide quality fixes) report, PARTIAL

- **Date:** 2026-11-14
- **Who:** Claude (agent)
- **Machine:** Apple Silicon Mac, macOS 26, one 1512x982 point display
- **Build:** worktree branch worktree-agent-ad2c6a94e1eb37ffe, release bundle `CaptureMann-dev.app` (dev id, local dev identity), isolated library, config and socket under /tmp
- **Fixture:** a small AppKit window with four labelled buttons as the click target (not committed)

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Launch the dev bundle with isolated data | PASS | socket created, window listed, Screen Recording and Input Monitoring show Granted |
| `capturemann guide start --window ...` | BLOCKED | the app showed the macOS system prompt "allow CaptureMann-dev to record your screen" (Allow / Open System Settings). I did not answer an OS permission prompt, so no real click was generated and no guide was recorded |
| Real clicks, steps, Markdown/HTML/PDF export from a real run | not run | blocked by the step above |
| PDF layout of the new writer, 3 Vietnamese steps with 1200x800 pictures (unit-test fixture), rendered with PDFKit and looked at | PASS | frame around the picture, heading stays with its picture, ordinary (filled) text; found and fixed two bugs of my first attempt (outlined text after the frame, heading left alone at the page bottom) |
| Daemon tests with fakes: Retina display (scale 2) gives 2400x1600 crop and names text 130 px (65 pt) away | PASS (fakes) | `gde_1_retina_step_crops_in_logical_px_and_names_text_in_logical_px` |

## Acceptance criteria

None claimed as verified.

## Problems found

- The maintainer's PDF: the title "Click here" was correct for the 80 physical px rule (the click was about 90 physical px, 45 pt, from "No preview"), the OCR itself was wired (daemon default is the system recognizer). The rule is now 80 logical px.
- "No preview" and "356 B": the picture was taken while the card was the empty just-created guide (356 B, no poster yet). After stop the Library never reloaded the card (no event) and, worse, the card asks `get_capture` for the poster folder, which refuses a guide, so it said "No preview" forever. Fixed (see commit), not seen on screen.
