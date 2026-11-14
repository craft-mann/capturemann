# Long Shot on Windows: SendInput scroll and stitching, spike S1 (partial)

- Date: Nov 14, 2026. Machine: ThinkPad T490 (Intel UHD 620), Windows 11, display 1920x1080 at 150 % scale, Edge, logged-in session.
- Branch: Track C2 of 0.3.4 (worktree `agent-a1e2d03186f80776a`), built on the Mac with `cargo xwin` (x86_64-pc-windows-msvc, release).
- What ran: `crates/cm-capture/examples/long_shot_probe.rs`, not the daemon or the overlay. It captures one Edge window through `Capturer`, scrolls it with the new `SendInput` scroller (`sys/long_scroll.rs`, pointer put at the window centre) and stitches with the 0.3.4 engine. Page: `tests/fixtures/long-shot/long-page.html` in a fresh Edge profile. Everything isolated under `C:\lab\ls`, removed afterwards. OCR used the installed 0.3.3 CLI with a private library, config and socket (the daemon was stopped).
- Verdict: PARTIAL. Not a "verified" report.

| Case | Page | Result |
| --- | --- | --- |
| CAP-6.AC1 (Auto scroll) | `?screens=5` | 7 frames, image 1597x4946, end `reached_end`. Row check (`long_shot_rows`: bar width of row i follows the page formula, constant 66 px pitch): 73 rows, 0 wrong. OCR (`capturemann text`): `ROW 0001` to `ROW 0073` each once, in order. |
| CAP-6.AC2 (fixed bands) | `?screens=5,bands=1` | 8 frames, image 1607x5090, header 192 px once at the top, footer 80 px once at the bottom (looked at the image), row check 73 rows, 0 wrong. |
| CAP-6.AC3 (jump) | `?screens=5,jump=1` | stopped after 3 frames at 2365 px with "Stopped: the next part did not line up", the part before kept (row check 34 rows, 0 wrong). Two runs, same result. |
| CAP-6.AC4 (limit, `--max-px 2000`) | `?screens=12` | `limit_reached`, image 2000 px, note shown, 28 rows, 0 wrong. The 16,384 px limit itself is only covered by the unit tests. |

## Found on the real window, fixed in the engine (unit tests added)

1. The scrollbar thumb, the window border and the rounded corners change between frames, so no two rows were equal and the first join failed. The engine now finds the join on the frame without its outer strips (48 px left, 64 px right at most), cuts the strips that hold the columns that differ, and the image is narrower by them (1597 of 1632 px).
2. The lines next to the fixed bands (window corners and edge line) differ in every frame: 8 lines at each end of the overlap are not compared, and the next frame's lines replace them.
3. A wheel step is not what was asked (Edge: about 1.5 times). `run_auto` now learns the real distance from each join and the first step is 30 % instead of 50 %.

## Not run / open

- Flaky target window: in about half of the early runs Edge was deactivated (title-bar buttons greyed) after the first frame, which changes the top rows and the first join fails ("did not line up", 1 frame); a console window started by the test script was the likely cause and the final runs launch the probe hidden. A real Long Shot has the same risk when the overlay or HUD takes the focus: the first frame's title bar and the later ones can differ. Not handled; a Long Shot of an inactive window is untested. About one run in six also ended `reached_end` with 1 frame (the scroll did not arrive).
- Not run: the daemon action and the overlay on Windows, Chrome, Excel, a PDF viewer, 100 % or 200 % scale, a second monitor, Windows 10, horizontal scroll, Hand scroll, a real 16,384 px image.
- X11 XTest scroller (`sys/x11_scroll.rs`): compiles (checked in a scratch crate against x11rb 0.13 on the Mac) but was not built with the crate or run on Linux.
- No screenshot of the final stitched images is stored (they were deleted from the laptop; two were looked at on the Mac).
