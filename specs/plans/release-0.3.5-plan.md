# Release 0.3.5: a light release (plan, draft, Nov 17, 2026)

Status: **final draft for review (Nov 17, 2026); nothing built, no open decision.** Scope set by the maintainer on Nov 17, 2026: a **light** release with five parts, each labelled **BUG**, **FEATURE** or **PROCESS** below:

| Part | Label | What |
| --- | --- | --- |
| 1 | BUG | 8 known bugs of 0.3.4 (section 2) |
| 2 | FEATURE | Line styles (solid, dashed, dotted) for arrow, line, rectangle, ellipse (ANN-12, section 3) |
| 3 | FEATURE | A Send feedback button on the About page, next to Source on GitHub (section 4) |
| 4 | PROCESS | A QA agent pass: an agent tests the app on each system and reports bugs (section 5) |
| 5 | FEATURE | Quick Capture: one panel for every kind of capture, an after-capture bar, presets, and a preset argument for agents and the CLI (QC-1, QC-3, QC-4, QC-5; section 5b) |

**Everything else is moved out of 0.3.5** (section 8). Bugs found by the QA pass or reported by users can enter the release only if they are P1 (safety or data loss) or a one-line fix; the rest go to 0.3.6. No other feature is added without the maintainer's word.

Sources: the Known limits of `docs/development/release-0.3.4-github-text.md`, `release-0.3.4-morning-report.md`, the two UX audits (`ux-audit-long-shot-click-guide-0.3.4.md`, `ux-audit-boards-bulk-0.3.4.md`), the UX proposal list (`ux-proposals-0.3.4-for-design.md`, items cited as CG-, LS-, BR-), `docs/project/status.md` and the `QA-0.3.4-*` reports in `verification/`. No user report of 0.3.4 had arrived when this was written. "Observed" below means seen in a stored QA run; "from the audit" means found by reading, not yet run.

## 1. Rules

1. **Light means light:** the only new features are ANN-12, the About feedback button and Quick Capture (QC-1, QC-3, QC-4, QC-5). MCP goes from 89 to **90 tools** (the new read tool `list_presets`); ANN-12 adds a field to existing ops, not a tool.
2. **Every fix has a test or a stored check** that fails before and passes after (a unit test, a `cm-regress` case, or a `tests/manual/` script with a report). Say "verified" only with a report in `verification/`.
3. **Scoped tests while developing, the full gate once on the final tree** ([[feedback-full-gate-once-before-release]]); clippy with the CI toolchain; Trivy gate and `cargo deny` before each merge to `main`.
4. **Windows files come from the GitHub "Windows package" workflow** (it has the speaker engine); the lab build does not.

## 2. Part 1, BUGS (8 items)

All are defects or safety gaps in features that already shipped. Order: safety and data first. Each row is the full requirement: what is wrong, and exactly what to build.

| # | Bug | Evidence | Required behaviour after the fix |
| --- | --- | --- | --- |
| BUG-1 | **A Click Guide started by an agent or the CLI shows no bar, no Stop and no indicator** (only a toast). A guide the user starts in the app already has its bar; this is only the agent and CLI start. It breaks "the user can always see and stop recording". Known limit of 0.3.4 (the only way to stop it today is `capturemann guide stop`). | Observed on macOS and Windows (QA pass 6) | After the confirm dialog, an agent- or CLI-started guide shows the guide bar in a passive form, like the Long Shot HUD: recording dot, step count, the text "Started by agent {client}" (the real client, CLI or MCP) and a **Stop** button. No Pause, Note or Undo. The sidebar tile shows the guide as running, and the toast has a Stop button. A user-started guide is unchanged. |
| BUG-2 | **Markdown export overwrites silently:** two exports into one folder overwrite the first's `guide.md` and leave stale `images/`; a guide with 0 steps reports success. | Observed on Windows (a second export to the same place, `warnings: []`) | The export still writes straight into the folder the user picks, but every name carries the local date and time: `guide-YYYY-MM-DD-HHMMSS.md` and `images-YYYY-MM-DD-HHMMSS/step-NN.png`, and the links inside the Markdown point to that images folder. Nothing existing is overwritten, so there is no Replace question. A guide with 0 steps is refused with an error. The success toast has an **Open folder** button. The agent path (`export_guide`, BUG-3) uses the same names. |
| BUG-3 | **`export_guide` lets an agent write over any file** at Free tier (for example `~/Documents/report.docx`). | From the audit (not run) | `export_guide` refuses to write to a path where a file already exists. A path outside the library and the user's export folders needs a Confirm dialog that shows the exact path; inside them it runs immediately. |
| BUG-4 | **Step titles can carry personal data:** the nearest text within 80 px becomes the title (`Click "jane.doe@example.com"`) and reaches Markdown, HTML, the clipboard and agent results; hostile text can inject Markdown. | Observed on macOS and Windows | When the clicked button or object has an accessibility name (AX on macOS, UI Automation on Windows; on Linux the same if the platform gives one), the title is `Click "{that name}"`. When it has none, the title is built exactly as today (the nearest on-screen text). Markdown special characters in any title are escaped on export. Nothing else about titles changes. |
| BUG-5 | **Click Guide ignores a moved window:** the followed window's rectangle is read once, so after the window moves the ring and title are wrong (a step was titled Export after clicking Save); a click on a window covering the guided one becomes a step. | Observed on macOS | The followed window's rectangle is read again at every click. If it moved or resized since the last click, the guide pauses with a notice instead of recording a wrong step. A click that lands on a different window than the followed one is ignored. |
| BUG-6 | **Long Shot on Windows ends after 2 frames** on YouTube and carchargeos.com (Edge, 1280x900). Wikipedia, docs.rs and the essay page work. | Known limit of 0.3.4; cause unknown | Limited effort: reproduce on the ThinkPad, find why the scroll settle or frame compare stops, fix it, and add a `cm-regress` case on a local tall page with a sticky header and lazy images (no internet needed). If the cause is not found and fixed after a reproduction and one real fix attempt, it stays a named known limit in the release text and moves to 0.3.6. |
| BUG-7 | **The board sheet says "drag order = list order"** (Vietnamese "kéo để đổi thứ tự") but the order changes only with the arrow buttons; the pill says "Layout editable until Finish". | Observed on macOS (library-boards pass) | Copy only: the hint reads "Use the arrows to change the order" (EN and VI) and the pill reads "Preview". No drag handle is added. |
| BUG-8 | **The 0.3.4 Preview feedback link may lose its pre-filled text:** `.github/ISSUE_TEMPLATE/config.yml` has `blank_issues_enabled: false`, so `issues/new?title=...&body=...` can land on the template chooser. | Suspected; check first by opening the link as a user would | If the pre-filled text arrives, change nothing. If it is dropped, point the link at an issue template (`template=bug_report.yml` or `feature_request.yml`) or add a "Feedback" issue form. The same link is used by the About button (section 4). |

## 3. Part 2, FEATURE: line styles for shapes (ANN-12)

**What exists today.** Arrow, Line, Rectangle and Ellipse are already built (ANN-1: tools A, L, R, E; ops `arrow`, `line`, `rect`, `ellipse` in `crates/cm-core/src/annotation.rs`). They have colour and width and only a solid stroke; the renderer (`crates/cm-render/src/annot.rs`, `stroke_round`) has no dash support. The feature is **a stroke style: solid, dashed or dotted**, on all four shapes.

| Point | Requirement |
| --- | --- |
| Op field | New optional `style` on the four ops: `"solid"` (default), `"dashed"`, `"dotted"`. Absent means solid and solid is not written, so existing manifests and goldens do not change. |
| Older apps | An app without ANN-12 draws the op solid and keeps the unknown field (the op's `extra` is preserved), so a 0.3.4 app does not lose the style when it saves (as ANN-9 AC7). |
| Look | Dashed: dash 4 x width, gap 2.5 x width, butt caps. Dotted: round dots (dash 0, gap 2 x width, round cap), minimum width 2 px so a dot stays visible. Measured along the path from the first point; a rectangle's dash phase keeps running round the corners. |
| Arrow | The shaft takes the style; **the head is always solid.** |
| Ellipse glow | The click-marker halo (`glow`) is unchanged; the style applies to the ring. |
| Editor | A three-button control (solid, dashed, dotted) in the shape panel for the four tools and for a selected shape; the last style is remembered per tool like colour and width; one undo step (ANN-1 AC5). |
| Agents | `annotate` ops take `style`; MCP schema, CLI docs and `docs/reference/` regenerated (`docs/reference/gen_agent_reference.py`); the tool count stays 89. An unknown value gives `invalid_input` naming the op index. |
| Export | The same renderer drives preview and export; add goldens at 1x and 2x per shape and style. |
| Not in scope | Custom dash lengths, double lines, arrow-head styles, freehand pen, styles for Highlighter, Callout or Spotlight. |

**Acceptance criteria (written first, in `specs/features/02-annotation.md`, ANN-12):** AC1 a dashed rectangle exports with the dash pattern, the same at 1x and 2x; AC2 a dotted line is round dots spaced 2 x width; AC3 a dashed arrow has a solid head; AC4 `style` absent renders exactly as before; AC5 changing a selected shape's style is one undo step and changes only that op; AC6 an agent op with `style: "dashed"` renders like the GUI; AC7 a build without ANN-12 draws it solid and writes the field back unchanged.

**Files:** `cm-core/src/annotation.rs`, `cm-render/src/annot.rs`, `apps/desktop/src/editor/**` (shape panel), `cm-mcp` schema, `locales/` (three labels, EN and VI), `tests/golden/annotation/`. Merge after BUG-1 to BUG-5 touch `cm-mcp/src/tools.rs`, to avoid a conflict.

## 4. Part 3, FEATURE: Send feedback button on the About page

**Today.** Settings > About (`apps/desktop/src/ui/settings/general.rs`, `about()`) has two secondary buttons in one row: `craftmann.app` and `Source on GitHub`. A Send feedback button and its sending code already exist for the Long Shot and Click Guide Preview card (`ui/preview_label.rs`: `feedback_url`, `send_feedback`, `url_encode`).

**Work:**
- Add a third secondary button **Send feedback** in the same row, after `Source on GitHub`, built like its neighbours (`button_with_icon`).
- Add `general_feedback_url(version)` next to `feedback_url`, reusing `url_encode`, or generalise `feedback_url`. The app version is the only thing filled in; **no logs, paths, screenshots or library data**, and nothing is sent before the click.
- One string with its Vietnamese entry; `cargo run -p cm-i18n -- extract` and `check`. The issue title and body stay English (`i18n-ignore`, as today).
- One unit test modelled on `the_feedback_url_carries_the_feature_and_the_version_only`.
- Optional icon: `message-square.svg` exists in `apps/desktop/assets/icons/` but is not registered in `ui/paint.rs`; skip the icon if registering it takes more than a few lines.
- The link target depends on BUG-8 (the template chooser).

## 5. Part 4, PROCESS: QA agent pass

An agent tests the 0.3.4 and 0.3.5 builds the way the `QA-0.3.4-*` passes did, and reports bugs. It is not a feature of the app.

| Point | Plan |
| --- | --- |
| Builds | The release candidate on each system: macOS (the notarized app copy or the dev bundle, screen unlocked, `caffeinate -d -i`), Windows (the CI package on the ThinkPad, logged in), Linux x86_64 (ThinkPad). Aarch64 only if time allows. |
| Pass 1 (start of the release) | Run on the **0.3.4 files** while the fixes are built: screenshot, annotation, library, Settings > About, import, export, Click Guide and Long Shot on the tested pages, with the main focus on the shipped Preview features. Finds bugs for the list below early. |
| Pass 2 (release candidates) | Re-run the same script on the candidates: the 8 bugs, ANN-12 (every shape, every style, preview against export), the About button and Quick Capture (panel per kind, the after-capture bar, a preset made, used and deleted, `capture_screenshot { preset }`, the shortcut). |
| Method | Follow the existing manual scripts in `tests/manual/` and the `cm-regress` desktop probes where they exist; the agent describes what it sees in text and stores screenshots only in `verification/QA-0.3.5-shots/` (never your real library; a private library per run, [[user-describes-in-text]] applies to attachments). |
| Consent | **Real screen, camera or microphone runs need the maintainer's ask each time** ([[recording-tests-need-consent]]); the brief to the agent says so. No recording tests are run unless you ask. |
| Output | One report per pass and system as `verification/QA-0.3.5-<area>-<system>-<date>-claude.md` (template in `verification/README.md`), each finding with steps, expected, seen, severity, and a proposed class: **P1** (safety or data loss), **bug** or **polish**. |
| Triage | I read the reports and add findings to this plan as `QA-n` rows under section 2 only if P1 or a one-line fix; everything else goes to `docs/project/status.md` backlog for 0.3.6. You review the list before anything is built. |
| Limits | An agent sees text and screenshots, not taste: it cannot judge the Long Shot and Click Guide looks (the "never seen on a screen" items stay a human look, moved out). It runs one pass, not a soak. |

## 5b. Part 5, FEATURE: Quick Capture (QC-1, QC-3, QC-4, QC-5)

Source: the design pack `capturemann-0.3.6-quick-capture/` at the repository root (local, untracked: `release-0.3.6-quick-capture-spec.md` and the boards `png/Q036Brief`, `Q036Panel`, `Q036After`, `Q036Presets`, `Q036Scope`, with `html/` and `source/`). The pack was written for 0.3.6 and is brought into 0.3.5 by the maintainer on Nov 17, 2026. The spec file for the build is `specs/features/21-quick-capture.md` (written in Phase 0, in the format of `specs/README.md`; it replaces the pack's text as the source of truth). Smart kind (QC-2) and pinned presets in the dock (QC-6) are **not** in 0.3.5.

**What it is.** One small panel that opens from anywhere and holds the kind (Screenshot, Video, Audio), the target, four option tiles and one Capture or Record button; an after-capture bar that lets the user decide what happens to a screenshot after it is taken; presets (named setups) that people, agents and the CLI can use. It uses the app's own names; no competitor's feature name is used.

**Requirements (final):**

| Id | Requirement |
| --- | --- |
| QC-1 Panel | A vertical card, 340 to 360 px wide, radius 18, the HUD surface, top to bottom: (1) header: CaptureMann mark, "Quick Capture", the preset picker pill, a settings button; (2) three kind tabs (Screenshot, Video, Audio) with icons, only the active tab shows its name, the panel opens on the last-used kind; (3) target: for Screenshot Region, Window, Display and Long Shot, for Video Region, Window, Display, for Audio the one-line note "Mic and system sound, no screen"; (4) a 2x2 grid of 52 px option tiles (below); (5) a 44 px full-width action bar with the shortcut inside it: neutral "Capture" for Screenshot, rec red "Record" for Video and Audio (red still means recording only); with a preset picked it reads "Capture · {preset}". Region and Window use the existing pickers (CAP-1); Long Shot uses CAP-6. The panel closes as soon as the capture starts and is never in the capture. |
| QC-1 Option tiles | Screenshot: Editor (open in editor after capture), Copy (copy to clipboard), Auto-redact, Delay. Video: Mic, System audio, Webcam, Countdown. Audio: Mic, System audio, Transcribe, Markers. A tile click toggles on and off; value tiles (Delay, Mic, Countdown) open a small menu; an on tile has a filled icon circle. The choices are remembered **per kind** and apply to captures started **from the panel or from a preset**. Captures started by the existing dock tiles, hotkeys and tray items keep using Settings (CAP-4 "After capture", CAP-5 delay, the recording sheet); the two never overwrite each other. |
| QC-1 Opening | Four ways: the tray or menu-bar menu gets a first item **Quick Capture…** (the native menu stays, CAP-2 says it is not custom-drawn); a **Capture…** button in the sidebar dock under Record… (a neutral primary button, with its keycap); a **Quick Capture** row in the Launcher (Capture group); and the global shortcut **⌥⇧C** on macOS and **Ctrl+Alt+Shift+C** on Windows and Linux (action `open_quick_capture`, rebindable in Settings > Shortcuts, conflict-checked like the others; the Launcher's ⌥⇧Space is unchanged). From the tray it opens under the icon; from a shortcut or the dock it opens near the pointer on the display that has it. The five dock tiles and every existing shortcut (⌥⇧1 to 4, R, M, L) keep working exactly as before. |
| QC-1 Keys | Enter or Space captures or records; 1, 2, 3 switch the kind; Tab moves through the tiles; Esc closes. Every control has an accessible name; the panel is excluded from screen capture and recording like the HUD. If the shortcut cannot be registered, Settings shows "Couldn't register: used by another app" and the tray item still works (as CAP-2.AC3). |
| QC-1 Preview | Quick Capture carries the **Preview** label and a **Send feedback** button, like Long Shot and Click Guide in 0.3.4 (a `PreviewFeature` entry; the same feedback form as the About button, with the feature name Quick Capture). No Settings switch. |
| QC-3 After-capture bar | After a **screenshot started from the panel or a preset**, when both "Editor" and "Copy" are off for that capture, a bar sits under the captured area (above it near the bottom edge) with Edit (E), Copy (C), Save (S), Text (T, copies recognised text, ANN-5), Long Shot (L, extends this area by scrolling, CAP-6) and Record (R, records exactly this area, countdown, then the normal HUD). Esc or a click elsewhere closes it and the capture stays in the library; it never auto-closes while the pointer is over it. When "Editor" or "Copy" is on, there is no bar: the capture goes where the panel says and a toast offers **Edit** and **Undo** for 6 seconds (Undo removes the capture from the library). The screenshot is always saved to the library first, so no choice loses it. |
| QC-4 Presets | A preset is a named setup: kind, target (not the area itself), the kind's option tiles, where to save, what to do after (open the editor with a chosen tool, copy, or nothing), an optional shortcut. Stored in `presets.json` in the app's settings folder; unknown fields are kept; a file edited by hand with an invalid preset skips only that preset and shows one notice. Picked from the header picker in the panel, which also holds **Save current as preset…** and **Edit presets…**; also listed in the Launcher ("Preset: {name}"), the tray menu and Settings > Capture > Presets (reorder, edit, delete). The edit sheet has Name, kind and target, the toggles (Copy to clipboard, Auto-redact, Open in editor, Then: a tool, Save to: a folder with `{date}`), a Shortcut with Change, and Save preset, Cancel, Delete. **Starter presets, editable and deletable: Bug report** (Region, copy, auto-redact, then the step badge tool), **Docs shot** (Window, editor, cursor shown, PNG at 2x), **Demo video** (Display, mic, webcam, 3 s countdown), **Meeting** (Audio, mic and system, transcribe after). Deleting a starter does not bring it back. |
| QC-5 Agents and CLI | `capture_screenshot` and `start_recording` take an optional `preset` (the preset's name or id); a new read-only tool **`list_presets`** returns the presets; CLI: `capturemann capture screenshot --preset "Bug report"` (and the matching `record` argument), `capturemann presets list`. An explicit argument in the same call wins over the preset's value. **The confirm rules are unchanged:** a preset that records still needs the on-screen confirmation, and the confirm dialog shows the plan the preset resolves to; a preset cannot loosen a locked tool or change `agents.tool_policy`. An unknown preset name gives `invalid_input` naming it. `docs/reference/` is regenerated (`gen_agent_reference.py`) and the tool-count assertion in `crates/cm-mcp/tests/stdio.rs` becomes 90. |
| Language | All UI text through `cm_i18n` with Vietnamese entries; the pseudo-locale check for the 360 px width; brand marks follow the theme's text colour (OD-21). |

**Acceptance criteria (written in Phase 0 in `21-quick-capture.md`, each with a test or stored check):** panel contents and order per kind; panel opens on the last kind; Enter captures; the panel is not in the screenshot; the Launcher shortcut is unchanged and ⌥⇧C opens the panel; the existing dock tiles and shortcuts behave as before; option choices are remembered per kind and do not change Settings; the after-capture bar shows only when Editor and Copy are off and offers the six actions; Record from the bar records exactly the captured area; the toast Undo removes the capture; a preset fills kind, target and options; Save current as preset round-trips through `presets.json`; unknown fields survive; an invalid preset is skipped with one notice; `capture_screenshot { preset }` equals the same options set by hand; `list_presets` lists them; a recording preset still asks for confirmation; an unknown preset is `invalid_input`; the Preview label and feedback button are present.

**Files:** new `ui/quick_capture/**` (panel, preset editor, after-capture bar) and `presets.json` handling in `apps/desktop`; a presets module in `cm-core` (model, validation, file read and write, display-free and testable); `cm-daemon` actions (`list_presets`, preset resolution for `capture_screenshot` and `start_recording`); `cm-mcp` tool and schema; `cm-cli` commands; `apps/desktop/src/shortcuts.rs` (`open_quick_capture`), tray menu, dock, Launcher rows; `cm-core/src/features.rs` (the Preview entry); `locales/`; `docs/reference/`. It touches the tray and the shortcut table, which Long Shot and Click Guide also use: merge it after BUG-1 to BUG-5 and the ANN-12 schema change in `cm-mcp/src/tools.rs`.

**Checks that need a screen (QA pass and a human look):** the panel in all three kinds in both themes, English and Vietnamese; the after-capture bar position near the bottom edge and on a second display; the shortcut on each system; the tray item on Windows and Linux; a preset recording started by an agent shows its confirmation.

## 6. Order and gates

1. **Phase 0 (spec text only):** ANN-12 in `specs/features/02-annotation.md`; Quick Capture as `specs/features/21-quick-capture.md` (QC-1, QC-3, QC-4, QC-5 with acceptance criteria, in the feature template) and the FR table rows; check BUG-8 on GitHub.
2. **In parallel:** BUG-1 to BUG-7 (Click Guide code, Long Shot on Windows, copy); ANN-12 (`cm-core`, `cm-render`, editor); the About button; QA pass 1. One `target/` per agent, disjoint files, worktrees removed when merged; I merge and retest myself.
3. **Full gate once** on the final tree: `fmt`, clippy with the CI toolchain, `cargo test --workspace --locked`, `cargo deny`, Trivy, `cargo run -p cm-i18n -- check`, `python3 scripts/check_target_os.py`.
4. **Release candidates** on all three systems: macOS DMGs signed and notarized by the maintainer, Windows from the CI workflow (unsigned), Linux tarballs and `.deb` files signed with minisign.
5. **QA pass 2, then `cm-regress`** on the final files of each system, reports in `verification/` as `L1-*-0.3.5-*`.
6. **Release day:** copy `docs/development/release-day-0.3.4.md` as `release-day-0.3.5.md`, check asset names with `scripts/check-release-assets.sh`, publish, update the Homebrew cask, the APT repository, CLAUDE.md, `status.md` and the morning report.

## 7. Exit criteria

- BUG-1 to BUG-5 and BUG-7 fixed with a test or stored check; BUG-6 fixed or named again as a known limit with what was tried; BUG-8 checked and fixed if the link is dropped.
- ANN-12: its acceptance criteria cited by tests, goldens for each shape and style, existing goldens unchanged, one look at the control on macOS and Windows with a stored report.
- The About button opens the pre-filled form and sends nothing before the click.
- Quick Capture: its acceptance criteria cited by tests; the panel, the after-capture bar and a preset run seen on a screen on macOS and Windows with a stored report; the Launcher shortcut and every existing shortcut unchanged; MCP tool count 90 asserted.
- QA pass 1 and pass 2 reports stored; every P1 finding fixed or named.
- No regression failure on any system; skipped cases listed by name; version 0.3.5; MCP tool count 90.
- The release text says only what a stored report supports.

## 8. Moved out of 0.3.5 (to 0.3.6 or later)

**Bugs and gaps, deferred (none is a safety or data-loss item unless it is marked):**
- Click Guide: failed clicks are silent at the 500-step cap and on disk full, and the 450 and 500 toasts were never seen (CG-02); quitting while a capture runs discards it (LS-13, CG-10, D5); an agent confirmation can cover the captured window (LS-27, D29); the editor commits typing only on Enter (CG-20); export-sheet switches look like plain squares (CG-30).
- Long Shot: fixed panels repeat in every slice of very long pictures; the Edge sync dialog on Windows.
- Windows: the busy-pipe tooltip wording; the one-off 30 to 45 s "loading library" hang (seen once, not reproduced).
- Boards and bulk export: scaling warnings under the size chip (BR-06, D9), Title and Captions below the fold (BR-04), the bulk-export size estimate, scroll cues for the Retype, Move area and Blueprint panels and the "Delete" label (D6).
- **The looks nobody has seen on a screen** (LS-18, CG-18): one human look at the Long Shot and Click Guide bars, HUD, toasts and end states.

**Checks:** the hardware and platforms never run (Intel Mac, Windows 10, SmartScreen, second monitor, mixed DPI, NVIDIA and AMD encoders, Wayland, aarch64 with a desktop, the Windows 5-minute soak, a Drive account); the Windows CI GPU test crash and the `r2_7_stale_socket` test flake; the slow tests not rerun on the merged code; the plugin check; the fuzzy Vietnamese strings (D28, D20).

**Features and design decisions:** board drop tile and drag handle, the board stamp label and Auto style (BR-02, BR-03, D2, D7, D8), the on-screen click ring (D27), Pause on password fields and a never-record app list (D16), the Long Shot stop key, start-from-top and scroll-pane picking (D13), tiled Long Shot over 16,384 px, rich-text cap and an agent clipboard Confirm (D11), REC-9 draw on screen, video Click Guide, DOCX and PPTX guide export, WebP and HEIC import, clipboard paste as a capture, in-app update (Velopack), Windows code signing, a headless daemon, windowed audio, the full Touch-up and Blueprint versions. PhoWhisper stays out. The remaining decisions D1 to D31 stay open and do not block 0.3.5.

## 9. For the maintainer outside the code

winget pull requests 472327 and 477195; the APT repository update with the 0.3.0 to 0.3.5 `.deb` files; the plugin line (held by the directory until the organization is Verified); the Windows licence list; the ThinkPad booted and logged in before QA pass 2 and the Windows regression; for review of this plan.
