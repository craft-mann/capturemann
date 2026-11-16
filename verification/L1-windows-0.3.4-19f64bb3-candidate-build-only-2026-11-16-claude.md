# L1 windows 0.3.4 candidate build of 19f64bb3 (build only, no regression run)

- **Date:** 2026-11-16
- **Who:** lab agent (Claude)
- **Verdict:** PARTIAL, and weaker than a regression report: the files were built, nothing was run on them.
- **Machine:** lab ThinkPad T490, Windows 11 Pro.
- **Source:** `git archive` of commit 19f64bb3af14b7478e9a18f9770666f67162e2af, unpacked to `C:\wt\main`, built with `C:\lab\rc2_build.ps1` (`packaging\windows\build-windows.ps1`, `CM_NO_SPEAKER_MODELS=1`, release profile with LTO off), run over ssh in the foreground (build 21:5x to 22:15, exit 0 for the package and for `cm-regress`).
- **Candidate files of commit 19f64bb3 (unsigned, lab build, not the CI package):**
  - `capturemann-0.3.4-windows-x64-setup.exe` 51,634,744 bytes, SHA-256 `aff186617f27af57a39c4b4a709ff7461769b9ffce00e6ca4fd8bd40e670112b` (equals the `.sha256` file the build wrote and the sum recomputed on the Mac after copying)
  - `capturemann-0.3.4-windows-x64.zip` 73,765,992 bytes, SHA-256 `93782bed27ef750f231670be4c2e3f156652770a96d80b1c5117aa1ef1ab03fb` (same checks)
  - Copies: `/Users/trungtran/cm-release-0.3.4/windows/` with `.sha256` files.
- **Not run (blocked):** installation, `cm-regress` full and hardware, Long Shot in Edge, boards preview and export check. After the laptop booted into Windows at 21:45 nobody was logged in on the console (`quser`: no user; Winlogon `AutoAdminLogon=0`), so `win-run-in-session.ps1` stopped with "nobody is logged in on the console" and the screen, sound and camera tiers cannot run. Setting up auto-logon was not done (the permission system refused changing the sign-in settings); the owner decides.
- Both of the previous 596c7956 files differ from these (different sums), so the 596c7956 reports do not cover these files. Source differences between the two commits: `apps/desktop/src/ui/board_sheet.rs`, `apps/desktop/src/ui/editor_view/touch_panels.rs`, `crates/cm-render/src/compose_board.rs`, `crates/cm-render/tests/board_preview_034.rs`.
