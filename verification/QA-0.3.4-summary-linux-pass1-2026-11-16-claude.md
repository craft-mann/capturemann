# QA 0.3.4 Linux pass 1 summary (PARTIAL)

- **Date:** 2026-11-16, Claude (agent), ThinkPad T490, Ubuntu 24.04 GNOME 46, X11 session only.
- **Build tested:** main `54a5bc68e419e6370c11ef4d1eeb4478e5594cdb`, x86_64.
  - `capturemann-0.3.4-linux-x86_64.tar.gz` SHA-256 `5de8f3cf523631259021c70202de8f6b7583a9b0d9f39ef8265785cb3b360efe`
  - `capturemann_0.3.4_amd64.deb` SHA-256 `e65131755c4c7515d8445fd5a1fba36450a81ac4ea9c8b7b982264dec7d0b9c6`
  - built with `release-build.sh --jobs 4` (about 18 min warm), `check-package.sh` passed on both, `capturemann --version` = 0.3.4.
- **Earlier attempt, main `92dbc542` (no files shipped):** did not compile on Linux (`crates/cm-capture/src/sys/long_scroll.rs:385`, E0432 `super::session`), and `build-linux.sh` had lost its `set -euo pipefail` (glued to a comment on line 37), so the failed build packaged the old 0.3.3 binaries under the name 0.3.4 and `check-package.sh` passed. Both fixed on main in `54a5bc68`.

| Report | Result |
| --- | --- |
| `L1-linux-0.3.4-54a5bc68-regression-standard-x11-2026-11-16-claude.md` | 57 passed, 0 failed, 7 skipped of 64 |
| `L1-linux-0.3.4-54a5bc68-regression-full-x11-2026-11-16-claude.md` (with the tarball install case) | 65 passed, 0 failed, 15 skipped of 80 |
| `L1-linux-0.3.4-54a5bc68-regression-deb-install-x11-2026-11-16-claude.md` (`linux.deb_install` only) | 1 passed |
| `QA-0.3.4-long-shot-linux-x11-pass1-2026-11-16-claude.md` | PARTIAL: Auto, bands, limit, Hand refusal pass; jump page FAIL (silent gap); horizontal PARTIAL |
| `QA-0.3.4-click-guide-linux-x11-pass1-2026-11-16-claude.md` | PARTIAL: 5 real clicks, steps, md/html/pdf export pass; bar not seen |
| `QA-0.3.4-annotation-ops-ui-linux-x11-pass1-2026-11-16-claude.md` | PARTIAL: ops, local-time titles, sidebar pass; one Vietnamese clip |

## Defects, ranked
1. (fixed on main) Linux compile error and packaging script without `set -e`; check-package now compares versions.
2. Long Shot on a page that jumps (`?jump=1`, X11, Firefox) ends `reached_end` with no warning and 28 rows missing (one row cut): AC3 fail, silent content loss.
3. Vietnamese: the "Hướng dẫn theo thao tác" tile clips its second line.
4. Retype `weight: 700` looks regular (unconfirmed).
5. The approval dialog shows the target as `{..}`.
6. Horizontal Long Shot of a whole window stops with `no_overlap` after 9 frames.

## UX proposals
- Show the window name, not `{..}`, in the approval dialog.
- Fall back to "Click here" when the recognised text near a click is noise.
- Let the Click Guide tile label shrink or ellipsize in Vietnamese.

## Not run
Wayland session (AC5, Click Guide disabled state), the Long Shot overlay bar, HUD and preview, Esc and Stop, Hand scroll through the HUD, the guide bar and countdown, the window picker, KDE, AMD/NVIDIA, a second monitor, sound cases, `ai.transcribe`, `ai.speakers`, the 5-minute soak, aarch64 (VM not used), minisign signing and the APT update.

## Housekeeping
The laptop keeps the installed 0.3.4 in `~/.local`; the private library, config and scripts were deleted. The release output for `54a5bc68` is in `~/wt/release-out/54a5bc68-x86_64/` (needed for signing).
