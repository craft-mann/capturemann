# Release 0.3.5: bug fixes and stabilisation (plan, draft, Nov 17, 2026)

Status: **draft, nothing built, waiting for the maintainer's review.** Scope decided by the maintainer on Nov 17, 2026, right after 0.3.4 was published: **bug fixes, stabilisation and feedback from 0.3.4; no new features, with one enhancement added the same day: line styles for the drawing shapes (track E, ANN-12).** Every other item below is a defect, a safety gap or a missing check in something that already shipped. Where a fix needs a product decision (D1 to D31 in `docs/development/ux-proposals-0.3.4-for-design.md`), the plan proposes a default and lists it in section 6; a default is not a decision until the maintainer says so.

Sources: the Known limits of `docs/development/release-0.3.4-github-text.md`, `docs/development/release-0.3.4-morning-report.md`, the two UX audits (`ux-audit-long-shot-click-guide-0.3.4.md`, `ux-audit-boards-bulk-0.3.4.md`), the UX proposal list (131 items: 14 P1, 54 P2, 63 P3), `docs/project/status.md`, and the `QA-0.3.4-*` and `L1-*-0.3.4-*` reports in `verification/`. No user report of 0.3.4 had arrived when this was written; section 5 is the intake for those.

## 1. Rules for this release

1. **No new features except track E (ANN-12), no new tools, no new ops.** MCP stays at 89 tools (ANN-12 adds a field to existing ops, not a tool); the only new spec ID is ANN-12. A fix may add an error code or a toast, not a surface. If a fix cannot be done without a feature, it moves to section 7. Further enhancements need the maintainer's word.
2. **Safety first.** Items where the user cannot see or stop a capture, or where data is overwritten or leaked, come before polish.
3. **Every fix has a test or a stored check** that fails before and passes after (a unit test, a `cm-regress` case, or a `tests/manual/` script with a report). "Verified" only with a report in `verification/`.
4. **Scoped tests while developing, the full gate once on the final tree** ([[feedback-full-gate-once-before-release]]); run clippy with the CI toolchain; run the Trivy gate and `cargo deny` before each merge to `main`.
5. **Windows files come from the GitHub "Windows package" workflow** (it has the speaker engine); the lab build does not.

## 2. Track A: safety and data (P1, do first)

| # | Item | Today | Fix (default) | Size | Refs |
| --- | --- | --- | --- | --- | --- |
| A1 | **Agent- or CLI-started Click Guide shows no bar** | After the confirm dialog the guide records every click with no bar, no Stop and no indicator; only a toast. Breaks "the user can always see and stop recording". | Passive bar like the Long Shot HUD: dot, step count, "Started by agent {client}", Stop (no Note, no Undo); the tile shows running; the toast gets Stop. The source (CLI or MCP) is the real one. | M | CG-01, D4 |
| A2 | **Markdown export overwrites** a guide's `guide.md` and leaves stale `images/` | Two guides in one folder overwrite each other; a 0-step guide reports success. | Export into a subfolder named after the guide; if it exists, ask Replace or Keep both; refuse a 0-step guide; "Open folder" on the success toast. | S to M | CG-23, D30 |
| A3 | **`export_guide` may overwrite any file** at Free tier | An agent can write over `~/Documents/report.docx`. | Refuse an existing file; a path outside the library or the user's export folders needs Confirm with the exact path. | S to M | CG-24, D12 |
| A4 | **Step titles carry personal data** | The nearest text within 80 px becomes a title (`Click "jane.doe@example.com"`), and reaches Markdown, HTML, clipboard and agent results; hostile text can inject Markdown. | Default: the clicked control's accessibility name (AX or UI Automation) when there is one; otherwise `Click here`; run the redaction detectors on any OCR candidate; escape Markdown; keep titles out of broadcast events. This changes titles only, not the shipped redaction decisions. | M | CG-12, D3 |
| A5 | **Click Guide ignores a moved window** | The followed window's rectangle is read once; after a move the ring and title are wrong (seen: step titled Export after clicking Save). | Re-read the rectangle per click; pause with a notice when it moves; ignore a click that lands on a window covering the followed one. | M | CG-03 |
| A6 | **Clicks fail silently** at the cap and on disk full | At 500 steps (and on a failed frame) clicks vanish with no signal. The 450 and 500 toasts exist in code and were never seen on a screen. | Toast once per 10 s "A click was not saved: {reason}"; verify the 450 and 500 toasts on a real screen on all three systems. | S to M | CG-02 |
| A7 | **Quitting discards a Long Shot or a guide** without asking | Quit while running throws the take away. | Default: stop cleanly, keep the result, show a toast on next launch. | S | LS-13, CG-10, D5 |
| A8 | **Agent confirmations over a window being captured** | A Confirm dialog can appear on top of the window Long Shot or Click Guide records. | Never on the captured display; answer `busy` without a dialog (built); the dialog appears only before the target is chosen, or on another display or a corner. | S | LS-27, D29 |

## 3. Track B: Windows Long Shot and recording (P1 to P2)

| # | Item | Today | Fix (default) | Size |
| --- | --- | --- | --- | --- |
| B1 | **Long Shot on Windows ends after 2 frames** on YouTube and carchargeos.com (Edge, 1280x900); Wikipedia, docs.rs and the essay worked | Known limit shipped in 0.3.4. Cause not found. | Reproduce on the ThinkPad with the page set recorded; find why scroll settle or the frame diff stops; add a `cm-regress` case on a local tall page with a sticky header and lazy images so the fix has a check that needs no internet. If it cannot be fixed, say so in the release text. | M to L |
| B2 | **Long Shot repeats fixed panels** (sticky headers, side bars) in every slice of very long pictures | Known limit. | Detect rows that stay identical between frames and keep only the first copy; test with a synthetic page that has a fixed header and footer. | M |
| B3 | **Windows busy-pipe tooltip wording** | The text shown when the pipe to the daemon is busy is wrong or unclear. | Copy fix in `locales/` (EN and VI); check the string in the app. | S |
| B4 | **Edge sync dialog** appears during Long Shot on Windows | Seen in QA pass 6. | Find out whether the app opens it or the page does; if the app, stop it. | S, after a look |
| B5 | **"Loading library" hang of 30 to 45 s**, seen once on the dev bundle | Not reproduced. | Add timing logs around library open; try five cold starts on the dev bundle and the notarized copy; fix only if reproduced, otherwise record the null result. | S |

## 4. Track C: boards, bulk export and editor (P1 to P2)

The audit rated 6 board items P1. Boards stay as built (flat band label, no drop tile; "boards plainer than the design" stays a known limit). This track fixes what misleads or loses data, not the look.

| # | Item | Fix (default) | Size | Refs |
| --- | --- | --- | --- | --- |
| C1 | Hint says "drag order = list order" but order changes only with the buttons; pill says "Layout editable until Finish" | Copy fix: "Use the arrows to change the order" (EN, VI), pill "Preview". No drag handle. | S | BR-05 |
| C2 | Extreme ratios and silent downscaling (a 12 x 1900 px strip scaled to 16,384; a 22 x 300 sliver) | One line under the size chip: "Pictures are scaled to the shortest: 1440 px high" and "Scaled down to fit 16384 px"; no new layout. | S | BR-06, D9 |
| C3 | Title and Captions are below the fold with no cue | Move Title after Arrangement; add a scroll cue (fade); no redesign. | S | BR-04 |
| C4 | Bulk export size estimate is wrong or missing | Check the estimate against real output on the fixture set; fix the number, not the UI. | S | BX |
| C5 | Editor: Retype, Move area and Blueprint panels below the fold; "Delete" after a Move area move is ambiguous | Scroll cue; label the button for what it does (decide D6). | S | ED-06, D6 |
| C6 | Click Guide editor fields commit only on Enter; Back or quit loses typing | Commit on blur, row change, Back and quit. | S | CG-20 |
| C7 | Click Guide export sheet switches drawn as plain squares | Use the app's switch style. | S | CG-30 |

## 4b. Track E: line styles for shapes (enhancement, ANN-12)

**What exists today.** Arrow, Line, Rectangle and Ellipse are already built (ANN-1: tools A, L, R, E; ops `arrow`, `line`, `rect`, `ellipse` in `crates/cm-core/src/annotation.rs`). They have colour and width but only a solid stroke; the renderer (`crates/cm-render/src/annot.rs`, `stroke_round`) has no dash support. So the request is **a stroke style: solid, dashed or dotted**, on all four shapes.

**Design (proposal, nothing built):**

| Point | Proposal |
| --- | --- |
| Op field | New optional `style` on `arrow`, `line`, `rect` and `ellipse`: `"solid"` (default), `"dashed"`, `"dotted"`. Absent means solid and solid is not written, so existing manifests and goldens do not change. |
| Older apps | An app without ANN-12 draws the op solid and keeps the unknown field (the op's `extra` is preserved), so a 0.3.4 app does not lose the style when it saves. Add the AC for this as in ANN-9 AC7. |
| Look | Dashed: dash 4 x width, gap 2.5 x width, butt caps. Dotted: round dots (dash 0, gap 2 x width, round cap). Both scale with the width and are measured along the path, starting at the first point. Corners of a rectangle keep the dash phase running (no reset per side). Minimum width 2 px for dotted so a dot stays visible. |
| Arrow | The shaft takes the style; **the head is always drawn solid** so it stays readable. |
| Ellipse glow | The click-marker halo (`glow`) stays as is; style applies to the ring. |
| Editor | A three-button style control in the shape panel (solid, dashed, dotted) for the four tools and for a selected shape; the last style is remembered per tool like colour and width (ANN-1 defaults); changing it is one undo step (ANN-1 AC5). |
| Agents | `annotate` ops take `style`; the MCP schema, CLI `--ops` docs and `docs/reference/` (regenerate with `docs/reference/gen_agent_reference.py`) are updated; `tools.json` changes, the tool count does not. Unknown value: `invalid_input` naming the op index. |
| Export | The same renderer drives preview and export, so parity holds without extra work; add goldens at 1x and 2x for each shape and style. |
| Not in scope | Custom dash lengths, double lines, arrow-head styles, freehand pen (still P2 in ANN-1), line styles on Highlighter, Callout or Spotlight. |

**Acceptance criteria to write in Phase 0 (`specs/features/02-annotation.md`, ANN-12):**
- AC1 a dashed rectangle exports with the dash pattern and the same golden at 1x and 2x;
- AC2 a dotted line is round dots, spaced 2 x width;
- AC3 an arrow with `dashed` has a solid head;
- AC4 `style` absent renders exactly as before (existing goldens unchanged);
- AC5 changing the style of a selected shape is one undo step and changes only that op;
- AC6 an agent op with `style: "dashed"` renders the same as the GUI;
- AC7 a build without ANN-12 draws it solid and writes the field back unchanged.

**Size:** M (core field and validation S, renderer M with the dash phase around corners and ellipse, editor control S, schema, docs and goldens S). Files: `cm-core/src/annotation.rs`, `cm-render/src/annot.rs`, `apps/desktop/src/editor/**` (shape panel), `cm-mcp` schema, `locales/` (three labels, EN and VI), `tests/golden/annotation/`. A separate track from A to D; it does not touch the guide or capture code.

## 5. Track D: intake, carry-over and checks that were never run

1. **0.3.4 user feedback.** Triage every new GitHub issue and message into this plan or section 7. Reserve about a third of the time for it. Nothing has arrived yet.
2. **Looks never done by a person** (LS-18 and CG-18, both P1, size S): one human look at the Long Shot and Click Guide bars, HUD, toasts and end states, both themes, English and Vietnamese, on a real screen. Store the report.
3. **Not run in 0.3.4 and still open:** Intel Mac hardware, Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA and AMD encoders, Linux Wayland and the Linux hardware tier, Linux aarch64 with a desktop, the Windows 5-minute soak, a Drive account for the paused-upload delete (B1 of 0.3.4). Run what the lab allows; list the rest again in the release text.
4. **Test flakes:** the Windows CI GPU test process dying (twice in 0.3.3, different tests), the `cm-daemon` `r2_7_stale_socket` test under load; find the cause or mark them honestly. Run the 49 `cm-daemon` test binaries and slow GPU tests once on a quiet machine.
5. **Plugin check** against the installed app (`tests/manual/PLUGIN-1.md`) if the plugin line is still held.
6. **Vietnamese review:** the fuzzy drafts in `locales/vi.po` and the one name for Click Guide (D28, D20); maintainer's review, we apply it.
7. **Docs:** correct the 0.2.9 and 0.3.x statements that the HUD drag works on Windows if the 0.3.4 B2/B3 fix has no stored human check; keep the 0.3.4 text's Known limits in step with what ships.

## 6. Decisions the maintainer should make before the build

Only the ones that block a fix above. Defaults are in the tables; "ok" means all defaults.

| D | Question | Default |
| --- | --- | --- |
| D3 | Step titles: accessibility name, else `Click here`? | yes (A4) |
| D4 | Passive bar for agent-started guides, with no Pause? | yes (A1) |
| D5 | Quit while running: stop cleanly and keep the result? | yes (A7) |
| D6 | "Delete" after Move area: separate buttons for remove contents and undo move? | one button, labelled by what it does (C5) |
| D9 | Board scaling: keep the shortest-height rule, add the warning only? | yes (C2) |
| D12 / D30 | Agent export writes: refuse existing files; confirm outside known folders; subfolder and Replace or Keep both in the app? | yes (A2, A3) |
| D29 | Where an agent Confirm may appear | not over the captured display (A8) |

All other D items (new surfaces, new features, board look, library chips, Vietnamese wording beyond D20 and D28) stay open and do not block 0.3.5.

## 7. Moved out (needs a feature, not a fix)

Board drop tile and drag handle, Edit board after creation, the S10 stamp label and Auto style for boards (BR-02, BR-03), the on-screen click ring (D27), Pause on password fields and a never-record list (D16), Long Shot stop key, start-from-top and scroll-pane picking (D13), tiled Long Shot over 16,384 px, rich-text cap and agent clipboard Confirm (D11, a small feature), REC-9 draw on screen, video Click Guide, DOCX and PPTX guide export, WebP and HEIC import, clipboard paste as a capture, in-app update (Velopack), Windows code signing, headless daemon, windowed audio, Touch-up and Blueprint full versions. PhoWhisper stays out.

## 8. Order, parallel work and gates

1. **Phase 0 (half a day): ANN-12 spec text and the frozen `style` field** in `specs/features/02-annotation.md` and the FR table (P1), spec text only.
   **Day 0: triage and reproduction.** Reproduce B1 on the ThinkPad and A1, A5, A6 on the Mac and Windows; write the failing test or `cm-regress` case for each fix first. Collect the maintainer's answers to section 6.
2. **Tracks run in parallel, disjoint files, one `target/` per agent** (worktrees, remove them when merged): A (`cm-daemon`, `cm-library` guide code, `ui/guide_view`, `cm-mcp` guide tools), B (`cm-capture`, Long Shot stitching, locales), C (`cm-render` compose, library and editor views, locales), D (checks and docs), E (`cm-core` annotation, `cm-render/src/annot.rs`, the editor shape panel; touches `cm-mcp` schema, so merge after A to avoid a conflict in `tools.rs`). Merge each track as it is green; I merge and retest myself.
3. **Full gate once** on the final tree (`fmt`, clippy with the CI toolchain, `cargo test --workspace --locked`, `cargo deny`, Trivy, `cargo run -p cm-i18n -- check`, `python3 scripts/check_target_os.py`).
4. **Release candidates** on all three systems: macOS DMGs built and notarized by the maintainer, Windows from the CI workflow, Linux tarballs and `.deb` files signed with minisign.
5. **Regression** with `cm-regress` on the final files of each system (macOS with the screen unlocked, `caffeinate -d -i`; Windows needs the ThinkPad booted and logged in; Linux x86_64 and the aarch64 VM), reports stored in `verification/` as `L1-*-0.3.5-*`.
6. **Release day** follows `docs/development/release-day-0.3.4.md` (copy it as `release-day-0.3.5.md` and adjust): check asset names with `check-release-assets.sh`, publish, update the Homebrew cask, the APT repository, CLAUDE.md, `status.md` and the morning report.

## 9. Exit criteria

- Section 2 (A1 to A8) fixed, each with a test or stored check, or moved to section 7 with the maintainer's agreement.
- B1: fixed with a check, or named in the release text as a known limit with what was tried.
- No regression failure on any system; skipped cases listed by name.
- Track E: ANN-12 acceptance criteria cited by tests, goldens for each shape and style, one look at the editor control on macOS and Windows with a stored report; existing goldens unchanged.
- Version 0.3.5 in `Cargo.toml`, the bundle and the installers; MCP tool count unchanged at 89.
- Release text says only what a stored report supports.

## 10. Open items for the maintainer (not code)

winget pull requests 472327 and 477195; the APT repository update with the 0.3.0 to 0.3.5 `.deb` files; the plugin line (held by the directory until the organization is Verified); the Windows licence list; the ThinkPad logged in before the Windows regression.
