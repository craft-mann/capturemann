# Release 0.3.6: Quick Capture and stabilisation (plan)

Status: **build started Nov 18, 2026.** Quick Capture was moved here from 0.3.5 by the maintainer on Nov 17, 2026. The plan states final requirements only; behaviour and acceptance criteria are in the feature spec.

## 1. Quick Capture (QC-1, QC-3, QC-4, QC-5)

Spec: `specs/features/21-quick-capture.md` (the spec file wins over the design pack `specs/capturemann-0.3.6-quick-capture/`, local and git-ignored). FR rows are in `specs/00-product-spec.md`. QC-2 and QC-6 are not in this release.

- The shortcut ⌥⇧C (Ctrl+Alt+Shift+C on Windows and Linux) opens the panel; the Launcher keeps ⌥⇧Space.
- The native tray and menu-bar menu stays, with **Quick Capture…** as its first item; the panel opens under the icon.
- Panel option tiles are remembered per kind and apply only to captures started from the panel or a preset. Dock tiles, hotkeys and the tray keep using Settings. Neither overwrites the other.
- Quick Capture carries the Preview label and a Send feedback button, with no Settings switch.
- Four starter presets ship: Bug report, Docs shot, Demo video and Meeting. They are editable and deletable.
- `list_presets` is the one new MCP tool (90 tools); `capture_screenshot` and `start_recording` take `preset`; the CLI gets `--preset` and `capturemann presets list`. The assertion in `crates/cm-mcp/tests/stdio.rs` and `docs/reference/` change with it.
- The after-capture actions "Long Shot extends this area" and "Record exactly this area" start a capture from a region that already exists; they reuse the existing capture actions with that region as the start.

## 2. Transcripts: invented text on short recordings

Report: the maintainer's recording `Audio recording 2026-11-17 17.02` (9 s, ThinkPad, 0.3.4, Vietnamese): the transcript holds invented text of the "thanks for watching" kind and no `[unclear]`, though the audio holds two short bursts (about 1.5 s and 2.3 s, around -33 dBFS) over a room level near -60 dBFS. The gap check (`pieces.rs` `fill_gaps`, minimum `GAP_MIN_SECS` 3 s) and the shape guard (AI-1.AC12, `guard.rs`, minimum `MIN_AUDIO_SECS` 30 s) both skip short audio.

Requirements (general techniques only, no phrase lists):
1. A line whose words are mostly low in confidence, over audio that is not speech-like (level near the room, or no speech evidence from the speaker engine or the level detector), is written as an `[unclear]` sound entry. Its words are left out of `words`, so captions, reports and search never see them. The dropped line is counted and kept, like `dropped`, and a Re-generate with the setting off brings it back. The thresholds are measured on the silence and quiet-speech sets (`silence_probe`, `quiet_silence`, `outro_guard` examples) and written into the spec with the numbers.
2. The guard judges audio under 30 s when speech and room are clearly apart and enough non-silent frames exist; the minimum is measured.
3. The 3 s gap minimum stays.
4. Existing transcripts are unchanged; a Re-generate uses the fix.
5. Every word has an end after its start; the last word of a piece gets the end of its piece (P-1).

## 3. Fixes carried over from 0.3.4 and 0.3.5 QA

Reports: `verification/QA-0.3.5-pass1-macos-2026-11-17-claude.md`, `QA-0.3.5-pass2-macos-2026-11-17-claude.md`, `QA-0.3.5-pass2-windows-2026-11-17-claude.md`, `QA-0.3.5-windows-ci-package-9dcfb5c3-2026-11-17-claude.md`.

- F1: a text file named `.png` is rejected as an unreadable image, never as an unsupported video codec.
- F2: `annotate` rejects a shape whose size is outside the image by a wide margin with `invalid_input`.
- F3: Long Shot leaves the target page where it was before the capture, or says in the result how far it moved the page.
- P-2: the ambiguous-window CLI error lists only the matching windows' ids and a short title.
- P-3: when Long Shot cannot join frames it says why and keeps the frames it has; the Windows case (maximized Edge window with a second tab, `no_overlap` after 2 frames) has a found cause.
- P-4: `annotate --out` on a large image never answers `internal`; it answers `invalid_input` with a reason, or stores the file.
- P-5: the Windows export dialog never shows the `\\?\` extended-path prefix.
- P-6: the Windows rail shows a visible cue that it scrolls, or has enough room.
- P-7: the Boards intro card closes with the sheet.
- P-8: the disabled Make a board button reports as disabled to UI Automation.
- P-9 and F5: in Vietnamese the Click Guide tile text and the Preview pill fit at the default width without overlap.

## 4. Order of work and exit

Phase 0 text is written. The presets core (QC-4, QC-5) is built first because the panel (QC-1) and the after-capture bar (QC-3) use its types. The transcript work and the small fixes run in parallel on disjoint files. During development only fmt, clippy and the tests of the touched crates run (plus the i18n check, and the MCP tool count when tools change). The full gate runs once, on the final tree, before release. After it: QA agent passes on the candidates and `cm-regress` on all three systems, with reports in `verification/`.
