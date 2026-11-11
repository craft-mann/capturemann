# Release 0.3.3: plan (draft)

Status: **draft written Nov 11, 2026; nothing built.** Follows 0.3.2 (in progress, `release-0.3.2-plan.md`).

## Scope

1. **In-app update (moved from 0.3.2 by the maintainer, Nov 11, 2026).** Everything: Velopack Apply on Windows and macOS, and the opt-in notify-only check (also the Linux path, with the `apt` command). Start from the section "In-app update with Velopack" in `release-0.3.2-plan.md` (analysis, conflicts with the Oct 25, 2026 "no network call" decision and NFR-9, the six decisions still open) and the spike reports `docs/development/velopack-spike-0.3.2.md` (macOS, GO WITH CHANGES) and `docs/development/velopack-spike-win-0.3.2.md` (Windows, when it lands). Decisions still needed: opt-in default off; Linux notify-only; feed host (a release tagged `updates`); Windows signing before Apply (depends on the SignPath outcome of 0.3.2); first-install formats; the three `deny.toml` questions (`webpki-roots`, `derivative`, `zip`); our own `UpdateSource` that sends no identifiers and verifies minisign. The maintainer must run the permission-survival check on a Developer ID build. Order: the opt-in check first (small, no packaging change), Apply after.
2. **Windowed audio** (flat memory for long files and speakers), moved from 0.3.2.
3. **PhoWhisper**, only if the maintainer wants it again: dropped from 0.3.2 because the models are fine-tuned without timestamp tokens, so adopting them means rebuilding word timing and decoding (captions, seek, speakers and reports depend on word times).

### Spike results for item 1 (Nov 11, 2026)

macOS: `docs/development/velopack-spike-0.3.2.md` (GO WITH CHANGES). Windows: `docs/development/velopack-spike-win-0.3.2.md` (GO WITH CHANGES; ThinkPad, public 0.3.1 zip as payload, velopack and vpk 1.2.161). Windows findings to carry:
- The updater silently kills every process whose exe is in the install folder (daemon, overlay, MCP server: about 1.4 s down); a terminal whose working directory is `current\` makes the update fail after 10 retries and the old version restarts.
- No rollback and no health check: a new version that will not start leaves a dead install; vpk packs any exe without checking it uses Velopack, so CI needs its own smoke test.
- Defaults to change: `run()` auto-applies a downloaded package at startup (set `set_auto_apply_on_startup(false)`, else the Confirm tier is bypassed); the stock HTTP source has no timeout and sends a persistent random `stagingId`.
- Integrity on unsigned Windows is only the feed SHA-256 over HTTPS; a minisign check works only inside a custom `UpdateSource` (delta files are deleted after patching and the rebuilt full package differs from the published one).
- Numbers: Velopack install 2.1 s (NSIS 17.2 s), no admin, 373 MB on disk (NSIS 246 MB), Setup.exe 119.9 MB (NSIS 86.6 MB); the 0.2.9 to 0.3.1 delta is 28.7 MB (25 % of full), the 33 MB speaker models never travel in a delta; rebuilding from a delta costs about 7 s CPU on the T490; skipped versions work through chained deltas; `run()` costs 2 to 3 ms and 2.8 MB.
- Coexistence: Velopack Setup does not detect an NSIS install (two "CaptureMann" entries); a hook that runs the NSIS uninstaller silently works (6 s, keeps settings); NSIS and flat-zip installs return `NotInstalled` and can never self-update; do not use pack id `CaptureMann` (collides with the NSIS Apps key); Setup silently downgrades over a newer install; feed JSON must have no UTF-8 BOM.
- Hosting correction: GitHub shows v0.3.1 as a full release and `releases/latest/download` resolves to it, so "all 0.3.x are pre-releases" is out of date; old deltas live in older releases (a client several versions behind falls back to the full package).
- The maintainer must check Edge, SmartScreen and the non-silent Setup on a real desktop, choose the Windows integrity policy (SignPath, minisign in a custom source, or notify-only), decide the NSIS migration (hook removes the old copy or only warns), pick the pack id, approve the winget manifest change (`InstallerType: exe`, `--silent`), and if SignPath goes ahead spike signing all 13 files with `--signTemplate` on a Windows runner.

## Carried over from 0.3.2

Known by name only, no reproduction (skipped for 0.3.2 by the maintainer, Nov 11, 2026; fix when it comes back): process-tree kill on cancel (a cancelled job may leave a child process such as ffmpeg running), the redaction box left-edge trim.


Whatever of the 0.3.2 carry-over list is not done by release day: the speech-detector second opinion for the outro guard, "Always ignore this line", `estimate_vs_real` on Windows, Intel Mac hardware, Windows 10, a 16 GB Mac for long recordings, the cm-overlay exit race, the optional keyboard check of the shortcut fixes on Windows 11 and Linux X11 (`tests/manual/SHORTCUTS-1.md`; the macOS report is closed).
