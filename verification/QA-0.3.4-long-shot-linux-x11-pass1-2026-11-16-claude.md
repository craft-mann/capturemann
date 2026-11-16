# QA 0.3.4 Long Shot (CAP-6) on Linux X11 (PARTIAL)

- **Date:** 2026-11-16, Claude (agent), ThinkPad T490 (Intel UHD 620), Ubuntu 24.04, GNOME 46, **X11 session**, 1 display 1920x1080 at scale 1.0. The maintainer was near the machine.
- **Build:** main `54a5bc68`, x86_64 tarball `capturemann-0.3.4-linux-x86_64.tar.gz` (SHA-256 `5de8f3cf523631259021c70202de8f6b7583a9b0d9f39ef8265785cb3b360efe`), installed with install.sh to `~/.local`; `capturemann --version` = 0.3.4. (A first build of `92dbc542` did not compile on Linux; see the summary.)
- **Method:** the shipped CLI against the daemon hosted by `cm-desktop` (private library, config, socket); the app's "Allow this action?" dialog answered with a pointer click (xdotool); Firefox 155 (snap) showing `tests/fixtures/long-shot/long-page.html`; result pictures looked at by eye and read with `capturemann text` (tesseract; it misreads digits, so the row order was checked by eye).
- **Not seen:** the Long Shot overlay bar, HUD and side preview (CLI-started runs show none; the overlay flow needs UI driving that was not done); Esc and Stop during a run.

| Case | Result | Evidence |
| --- | --- | --- |
| Auto, `?screens=5`, whole Firefox window | PASS | `reached_end`, 7 frames, 1280x4319, header 139 px once; rows 0001 to 0095 each once, in order (looked at). Small scrollbar-thumb marks repeat at the right edge at the seams (cosmetic). The approval dialog was not in the image. |
| Fixed header and footer (`bands=1`) | PASS | 7 frames, 1280x4361, header_px 133, footer_px 48; "HEADER - stays on screen" once at the top, "FOOTER" once at the bottom, rows 0001 to 0095 once (looked at) |
| `--max-px 2000` | PASS | `limit_reached`, 3 frames, 1280x2000, warning "Reached the size limit of 2000 px" |
| Page that jumps (`?screens=12&jump=1`, AC3) | **FAIL** | `reached_end`, 14 frames, 1280x9501, **no warning, no note**. The picture goes from ROW 0126, a cut-off ROW 0127, straight to ROW 0156: 28 rows (0128 to 0155) are missing and nothing tells the user. AC3 wants "Stopped where the page jumped" and the part before the jump kept. (On Windows the same page gave `no_overlap`.) |
| Horizontal, whole window, `?wide=1` | PARTIAL | `no_overlap`, 9 frames, 8252x873, note "Stopped: the next part did not line up"; whether the page's end was reached was not checked. Better than Windows (1 frame). |
| Hand scroll from an agent/CLI (AC7) | PASS | `invalid_input`, `data.reason` = `needs_user` |
| Agent tier | PASS | with no app running the CLI answers `user_declined` / `no_confirmer`; with the app the "Allow this action?" dialog appears (Tool capture_screenshot, Target `{..}`, Direction vertical, Scroll auto). UX: the Target row shows the raw `{..}` instead of the window name. |
| AC5 Wayland Hand scroll | not run | no Wayland session was started |
| Overlay bar, HUD, side preview, Esc, Stop, Hand scroll through the HUD | not run | not seen |

Verdict: PARTIAL.
