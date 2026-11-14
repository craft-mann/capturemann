# QA 0.3.4 canvas text selection (ANN-5 area) on Windows (NOT RUN)

- **Date:** 2026-11-15
- **Who:** Claude (agent)
- **Result:** not run.
- **Reason:** canvas text selection and the caret are editor UI behaviour (click and drag inside a text annotation on the canvas, Shift-arrows, double-click to select a word). The overnight scripts drive the CLI, the daemon and window-level mouse clicks, but there is no hook for reading or asserting the editor's text selection (the canvas is drawn by GPUI, not by a Windows control, and floating or in-canvas selection highlights were not checked by pixels). Building that automation was out of the time box.
- **What was run nearby:** the daemon side, `recognize_text` / `capturemann text`, ran on many captures (see the Blueprint, Retype and Long Shot reports): Windows OCR returned correct text each time.
- **How to run it:** open a screenshot in the editor, add a Text annotation, type two words, double-click one of them, press Shift+Right, copy; paste elsewhere.

## Verdict

NOT RUN
