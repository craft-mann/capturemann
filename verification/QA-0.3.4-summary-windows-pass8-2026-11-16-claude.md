# QA 0.3.4 Windows pass 8, summary (PARTIAL)

- **Date:** 2026-11-16, Claude (QA agent). ThinkPad T490, Windows 11, one monitor. Build: main `0e73b3c2`, rebuilt on the laptop (unsigned, no speaker engine), installed silently over the old copy; `capturemann 0.3.4`.
- **Checksums (SHA-256):** zip `DF7B1B4C041EE66DED964661C43EB9E262B104F01855A15805D6C7C4CA9A7D3F` (73,500,413 bytes); setup exe `647399B21B3A06F1E726ECAC0E3807D7F0A8392C0846E24E8F219C808CADBDC6` (51,531,610 bytes); installed `cm-desktop.exe` `35DF06B23D22F34CC45139FB9BB827954A747FC7BD1B5A2F29C0ACA1457B5081`; `cm-regress.exe` `C954169F630B04CCCC35760AF2F6268B8CCC5F362B31FDFF2454C82A2751CB3D`.

| Item | Result | Report |
| --- | --- | --- |
| Build and silent install, version 0.3.4 | PASS | this file |
| Long Shot, 5 real sites (Edge, own profile) | **FAIL 4 of 5** (paulgraham PASS; Wikipedia, YouTube, docs.rs 1 frame 3 of 3; carchargeos 8 frames once, 1 frame twice) | `QA-0.3.4-long-shot-real-sites-windows-pass8-2026-11-16-claude.md` |
| Long Shot, fixed sidebars with scrollbar | **FAIL** 3 of 3 | same |
| Long Shot, classic conhost | **2 of 4** (first run on a new window `nothing_scrolled`, repeat passes) | same |
| `cm-regress` full | PASS, 66 passed, 0 failed, 14 skipped | `L1-windows-0.3.4-regression-full-final4-2026-11-16-claude.md` |
| `cm-regress` hardware (screen, sound, camera) | PASS, 68 passed, 0 failed, 20 skipped; `rec.camera` passed first time | `L1-windows-0.3.4-regression-hardware-final4-2026-11-16-claude.md` |
| Park pointer on Windows | works (cursor parked 15 px outside the window; hand test: wheel with the pointer outside does not scroll Edge) | Long Shot report |

## Defects, ranked

1. Real pages end on one frame on Windows (Wikipedia, YouTube, docs.rs always; carchargeos mostly); the page does scroll, the first join fails. Acceptance test not met.
2. A large first step leaves almost no overlap on a gradient hero (carchargeos: shift 630 of 663 usable px); retry with a smaller step.
3. Fixed left and right bars with a scrollbar still fail.
4. Classic console: the first run on a fresh window is `nothing_scrolled`.
5. Size-limit note wrong ("50 megapixels" for a 16384 px height stop).
6. Toast for a CLI run says `Started by agent via MCP` (carried over).

## UX proposals

- Say what happened when a run ends on one frame and offer Hand mode.
- Retry the first join at half the step before `no_overlap`.
- Write a debug dump also when the first pair is refused with nothing accepted.

## Not run

Hand scroll, pass-7 Long Shot cases busy/closed/HUD/Esc/move/resize (script hung after the Windows Terminal case, stopped by hand), second monitor, mixed DPI, Windows 10, SmartScreen, NVIDIA/AMD encoders, microphone (`rec.mic` needs `--allow mic`), transcription and speaker cases (no model), the 4K external monitor, soak.

## Laptop state

Scheduled tasks, the `HKCU\Console\%%Startup` keys, private libraries, the Edge profile and all pushed scripts removed; new build installed, app not running. Build sources and output remain in `C:\wt` (`out`, `main`, `bundles`, `target`), as after the earlier builds.

## Verdict

PARTIAL
