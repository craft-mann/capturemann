# Release 0.3.3: plan (draft)

Status: **draft written Nov 11, 2026; nothing built.** Follows 0.3.2 (in progress, `release-0.3.2-plan.md`).

## Scope

1. **In-app update (moved from 0.3.2 by the maintainer, Nov 11, 2026).** Everything: Velopack Apply on Windows and macOS, and the opt-in notify-only check (also the Linux path, with the `apt` command). Start from the section "In-app update with Velopack" in `release-0.3.2-plan.md` (analysis, conflicts with the Oct 25, 2026 "no network call" decision and NFR-9, the six decisions still open) and the spike reports `docs/development/velopack-spike-0.3.2.md` (macOS, GO WITH CHANGES) and `docs/development/velopack-spike-win-0.3.2.md` (Windows, when it lands). Decisions still needed: opt-in default off; Linux notify-only; feed host (a release tagged `updates`); Windows signing before Apply (depends on the SignPath outcome of 0.3.2); first-install formats; the three `deny.toml` questions (`webpki-roots`, `derivative`, `zip`); our own `UpdateSource` that sends no identifiers and verifies minisign. The maintainer must run the permission-survival check on a Developer ID build. Order: the opt-in check first (small, no packaging change), Apply after.
2. **Windowed audio** (flat memory for long files and speakers), moved from 0.3.2.
3. **PhoWhisper**, only if the maintainer wants it again: dropped from 0.3.2 because the models are fine-tuned without timestamp tokens, so adopting them means rebuilding word timing and decoding (captions, seek, speakers and reports depend on word times).

## Carried over from 0.3.2

Whatever of the 0.3.2 carry-over list is not done by release day: the speech-detector second opinion for the outro guard, "Always ignore this line", `estimate_vs_real` on Windows, Intel Mac hardware, Windows 10, a 16 GB Mac for long recordings, the cm-overlay exit race, the keyboard check of the shortcut fixes on macOS, Windows 11 and Linux X11 (`tests/manual/SHORTCUTS-1.md`).
