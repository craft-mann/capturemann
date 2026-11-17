# Release 0.3.6: Quick Capture (plan, draft, Nov 17, 2026)

Status: **scope only; nothing built.** Quick Capture was first added to 0.3.5 and moved to 0.3.6 by the maintainer on Nov 17, 2026, so 0.3.5 stays light (`specs/plans/release-0.3.5-plan.md`).

## 1. FEATURE: Quick Capture (QC-1, QC-3, QC-4, QC-5)

- **Spec:** `specs/features/21-quick-capture.md` (Phase 0 text is written, with acceptance criteria; Preview label). FR rows are in `specs/00-product-spec.md`.
- **Design pack:** `specs/capturemann-0.3.6-quick-capture/` (local, git-ignored): boards Q036Brief, Q036Panel, Q036After, Q036Presets, Q036Scope. The spec file wins where they differ.
- **Not in this release:** QC-2 (one smart crosshair) and QC-6 (pinned presets in the dock).
- **MCP:** one new read tool, `list_presets`, so the count goes from 89 to 90; the assertion in `crates/cm-mcp/tests/stdio.rs` and `docs/reference/` change with it.

**Decisions already made (Nov 17, 2026), all written into the spec:**

| Point | Decision |
| --- | --- |
| Shortcut | **⌥⇧C** (Ctrl+Alt+Shift+C on Windows and Linux) opens the panel; the Launcher keeps ⌥⇧Space. |
| Tray and menu bar | The native menu stays; **Quick Capture…** is its first item. The panel opens under the icon after that. |
| Panel options and Settings | The option tiles are remembered per kind and apply only to captures started from the panel or a preset; dock tiles, hotkeys and the tray keep using Settings; neither overwrites the other. |
| Preview | Quick Capture carries the Preview label and a Send feedback button, no Settings switch. |
| Starter presets | Ship Bug report, Docs shot, Demo video and Meeting, editable and deletable. |
| Scope | QC-1, QC-3, QC-4 and QC-5 (including `list_presets` and the `preset` argument for agents and the CLI). |

## 2. Also to plan for 0.3.6

The bugs and checks moved out of 0.3.5 (section 8 of the 0.3.5 plan), and whatever the QA agent passes and users report for 0.3.4 and 0.3.5.

## 2b. Findings from QA pass 1 on macOS (0.3.4 files, Nov 17, 2026; report `verification/QA-0.3.5-pass1-macos-2026-11-17-claude.md`, PARTIAL: no P1 found, 5 polish)

| Id | Finding | Seen |
| --- | --- | --- |
| F4 | `capture screenshot --window -` matches the first window whose title contains "-" with no ambiguity error, and captured an unrelated private window. Fixed in 0.3.5 as BUG-9 (the maintainer included it on Nov 17, 2026). | once |
| F1 | A text file named `.png` is rejected with "The video codec Png is not supported yet." (treated as video). | once |
| F2 | `annotate` accepts a rect of width 5,000,000 with no warning and stores it as a revision. | once |
| F5 | In Vietnamese the Click Guide tile label wraps to two lines and the "Xem trước" Preview pill touches the icon (related to D28, CG-11). | once |
| F3 | Long Shot leaves the target page scrolled about 150 px (known, D13). | once |

## 2c. BUG: invented transcript text and no [unclear] on short recordings (reported Nov 17, 2026)

Report: the maintainer's recording `Audio recording 2026-11-17 17.02` (9 s, recorded on the ThinkPad with 0.3.4, Vietnamese, no mute): the transcript holds invented text (a stock "thanks for watching" kind of line; medium model: 17 words with most confidences 0.2 to 0.5, large model on the Mac: 7 words, a different text of the same kind) and no `[unclear]` part, although the audio holds only two short bursts of sound (about 1.5 s and 2.3 s, around -33 dBFS) over a room level of about -60 dBFS, and the invented words sit partly over room-level audio.

Cause: the two checks that could catch it both skip short audio. `[unclear]` and `[noise]` come only from the gap check (`pieces.rs` `fill_gaps`), which needs a stretch of speech-like level of at least `GAP_MIN_SECS` (3 s) with no words; and the shape guard (AI-1.AC12, `guard.rs`) never judges audio under `MIN_AUDIO_SECS` (30 s). Both were tuned on long meetings; short clips were never measured.

Required (to be built and measured in 0.3.6; general techniques only, no phrase lists):
1. **Doubtful lines become `[unclear]`:** a line whose words are mostly low in confidence, over audio that is not speech-like (level near the room, or no speech evidence from the speaker engine or the level detector), is written as an `[unclear]` sound entry and its words are left out of `words`, so captions, reports and search never see them; the dropped line is counted and kept (like `dropped`, so nothing disappears silently); a Re-generate with the setting off brings it back. Thresholds are measured on the silence and quiet-speech sets (`silence_probe`, `quiet_silence`, `outro_guard` examples, the maintainer's library) and written into the spec with the numbers.
2. **Short audio:** if the measurements support it, the guard (AI-1.AC12) judges audio under 30 s when speech and room are clearly apart (this clip: about 27 dB) and enough non-silent frames exist; the minimum is measured, not guessed.
3. **Not changed:** the 3 s gap minimum stays (shortening it makes more `[noise]` entries).
4. Existing transcripts are unchanged; a Re-generate uses the fix.

## 3. To write before the build

Tracks, order and exit criteria (as in the 0.3.5 plan: Phase 0 done, parallel tracks on disjoint files, one full gate, QA agent passes on the candidates, `cm-regress` on all three systems). Open points to check when the build starts: "Long Shot extends this area" and "Record exactly this area" from the after-capture bar start a capture from a region that already exists, which is more than adding a surface.
