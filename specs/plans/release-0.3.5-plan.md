# Release 0.3.5: a light release (plan, draft, Nov 17, 2026)

Status: **final draft for review (Nov 17, 2026); nothing built, no open decision.** Scope set by the maintainer on Nov 17, 2026: a **light** release with five parts, each labelled **BUG**, **FEATURE** or **PROCESS** below:

| Part | Label | What |
| --- | --- | --- |
| 1 | BUG | 11 known bugs of 0.3.4 (section 2) |
| 2 | FEATURE | Line styles (solid, dashed, dotted) for arrow, line, rectangle, ellipse (ANN-12, section 3) |
| 3 | FEATURE | A Send feedback button on the About page, next to Source on GitHub (section 4) |
| 5 | FEATURE | Boards marked **Preview** (label and Send feedback button, section 4b) |
| 4 | PROCESS | A QA agent pass: an agent tests the app on each system and reports bugs (section 5) |

**Everything else is moved out of 0.3.5** (section 8). Bugs found by the QA pass or reported by users can enter the release only if they are P1 (safety or data loss) or a one-line fix; the rest go to 0.3.6. No other feature is added without the maintainer's word.

Sources: the Known limits of `docs/development/release-0.3.4-github-text.md`, `release-0.3.4-morning-report.md`, the two UX audits (`ux-audit-long-shot-click-guide-0.3.4.md`, `ux-audit-boards-bulk-0.3.4.md`), the UX proposal list (`ux-proposals-0.3.4-for-design.md`, items cited as CG-, LS-, BR-), `docs/project/status.md` and the `QA-0.3.4-*` reports in `verification/`. No user report of 0.3.4 had arrived when this was written. "Observed" below means seen in a stored QA run; "from the audit" means found by reading, not yet run.

## 1. Rules

1. **Light means light:** the only new features are ANN-12, the About feedback button and the Preview label on Boards. MCP stays at **89 tools**; ANN-12 adds a field to existing ops, not a tool. Quick Capture was moved to 0.3.6 on Nov 17, 2026 (section 8).
2. **Every fix has a test or a stored check** that fails before and passes after (a unit test, a `cm-regress` case, or a `tests/manual/` script with a report). Say "verified" only with a report in `verification/`.
3. **Scoped tests while developing, the full gate once on the final tree** ([[feedback-full-gate-once-before-release]]); clippy with the CI toolchain; Trivy gate and `cargo deny` before each merge to `main`.
4. **Windows files come from the GitHub "Windows package" workflow** (it has the speaker engine); the lab build does not.

## 2. Part 1, BUGS (11 items)

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
| BUG-9 | **A window given by a partial title can match the wrong window:** `capture screenshot --window -` (CLI and the same argument over MCP) takes the first window whose title contains the text, with no ambiguity error; it captured an unrelated private window. | Observed once (QA pass 1 on macOS, finding F4) | When more than one window title contains the given text, the call fails with `invalid_input` and `error.data.reason` = `ambiguous_target` (a reason, not a new top-level code: the 15 codes stay as decided in `specs/README.md`; add the reason to the Reasons list there and in `02-agent-interface.md`) and the message lists the matches (window id, app name, title) so the caller can pick one by id; an exact title match still wins over partial matches; one partial match works as today; no match is an error as today. Applies everywhere a window is chosen by title text (screenshot, Long Shot, recording and Click Guide targets in the CLI and MCP). Choosing a window in the app is unchanged. |
| BUG-10 | **Whisper writes text nobody said right after a long muted stretch:** in a Vietnamese online-meeting recording (16 min, mic muted 276 s and 83 s, digital silence) the same sign-off line ("Hãy subscribe cho kênh ...") was written 4 times, each within a few seconds to 40 s after the mic was unmuted, with confidence 1.0 and word times collapsed to 0 s. The existing guard (AI-1.AC12) did not drop them: its room level (10th percentile of all 20 ms levels) is -120 dB when muted time is exact zeros, so every line looks 100 dB above the room, and at those moments the mic does hold speech-level sound. | Observed (maintainer's recording of Nov 17, 2026; **fixed and merged: after Re-generate on the 0.3.5 dev bundle the maintainer reported the line gone, Nov 17, 2026; reported by the maintainer, no stored report**). Reproduced offline with the app's own engine and large-v3 on copies of the audio: with the muted stretch in the audio the line came back (2 times in a 140 s excerpt and 1 time in a 120 s excerpt); with the muted stretch cut out, the same speech gave 0 and the real words ("Còn những phần mới, ví dụ như là..."). | General technique, no phrase list (maintainer's rule of Oct 28, 2026): (1) before pieces are planned, find runs of digital silence (samples exactly zero) of 1 s or longer in the audio given to Whisper; do not give them to Whisper: transcribe the spans between them (as separate pieces, so a long zero run is never inside a Whisper window) and add each span's start back to its word times; the recorded mute ranges (`mic_muted` in the manifest) are the same thing and need no separate path. Runs under 1 s stay as they are. Silence found by level (not exact zeros) is NOT cut: an earlier trial that cut quiet stretches removed real speech (`docs/research/transcription-quality-study-0.2.7.md`, item c). (2) In the guard (AI-1.AC12) estimate the room level only from frames that are not digital silence, so a muted recording does not break it. (3) Word times, captions, the speaker labels and the stored `transcript.json` stay in session time; a transcript made before this fix is unchanged; a re-generate uses the fix. |
| BUG-11 | **Meeting minutes asked for in Vietnamese come out mixed English and Vietnamese.** In the maintainer's report (template Meeting minutes, language `vi`, built-in model gemma-4-e2b): the Summary section is English prose, while the Topics, Decisions and Action items are Vietnamese; the section headings and labels (Summary, Attendees, Topics, Key points, Decisions, Action items, Open questions, Marked moments, Owner, Nothing found, the line "Meeting minutes · date · model") are English. | Observed (report of the recording of Nov 17, 2026 16:09). **Fixed and merged: after Re-generate on the 0.3.5 dev bundle the maintainer reported the minutes no longer mixed English and Vietnamese, Nov 17, 2026; reported by the maintainer, no stored report.** Causes found in the code: when the output language equals the transcript's, the prompt says only "write the note text in the language of the transcript" and never names the language, while the rules, schema and template text are English, so a small model drifts into English for the free prose; the headings and labels come from the English template files and are never localized. | General techniques only (no phrase lists): (1) The system prompt always names the output language explicitly (the transcript's language or the chosen one, for example "Write all note text in Vietnamese"; names and terms the speakers said in another language may stay as spoken; quotes stay as spoken), and the same rule is repeated in the last lines of the user prompt after the transcript (`crates/cm-ai/src/report/prompt.rs`). (2) After the notes are checked against the transcript, each prose field is checked for being in the output language, with the same text model as a short yes or no question per section (no word lists, no per-language rules); a section that fails is regenerated once with a stronger instruction and replaces the first only if it passes; if it still fails, the first stays and the report records it (a `language_unsure` count, shown like `dropped_uncited`). (3) Section titles, field labels and the fixed words of the rendered report (headings, `Owner`, `Nothing found`, the header line) are shown in the report's output language through `cm_i18n` (the report's language code picks the catalogue, not the app's UI language; English when there is no catalogue); template ids, the JSON and the stored `content` do not change; Vietnamese entries are added for every template's titles and labels. (4) A report already made is unchanged; Re-generate uses the fix. |

## 3. Part 2, FEATURE: line styles for shapes (ANN-12)

**Seen by the maintainer on the 0.3.5 dev bundle (Nov 17, 2026): the line styles in the editor are OK (reported by the maintainer, no stored report).**

**What exists today.** Arrow, Line, Rectangle and Ellipse are already built (ANN-1: tools A, L, R, E; ops `arrow`, `line`, `rect`, `ellipse` in `crates/cm-core/src/annotation.rs`). They have colour and width and only a solid stroke; the renderer (`crates/cm-render/src/annot.rs`, `stroke_round`) has no dash support. The feature is **a stroke style: solid, dashed or dotted**, on all four shapes.

| Point | Requirement |
| --- | --- |
| Op field | New optional `style` on the four ops: `"solid"` (default), `"dashed"`, `"dotted"`. Absent means solid and solid is not written, so existing manifests and goldens do not change. |
| Older apps | An app without ANN-12 draws the op solid and keeps the unknown field (the op's `extra` is preserved), so a 0.3.4 app does not lose the style when it saves (as ANN-9 AC7). |
| Look | Dashed: dash 4 x width, gap 2.5 x width, butt caps. Dotted: round dots (dash 0, gap 2 x width, round cap), minimum width 2 px so a dot stays visible. Measured along the path from the first point; a rectangle's dash phase keeps running round the corners. |
| Arrow | The shaft takes the style; **the head is always solid.** |
| Ellipse glow | The click-marker halo (`glow`) is unchanged; the style applies to the ring. |
| Editor | A three-button control (solid, dashed, dotted) in the shape panel for the four tools and for a selected shape; the last style is remembered per tool like colour and width; one undo step (ANN-1 AC5). |
| Agents | `annotate` ops take `style`; MCP schema, CLI docs and `docs/reference/` regenerated (`docs/reference/gen_agent_reference.py`); ANN-12 adds no tool. An unknown value gives `invalid_input` naming the op index. |
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

## 4b. Part 5, FEATURE: Boards marked Preview

Added by the maintainer on Nov 17, 2026. Long Shot and Click Guide already carry the Preview label since 0.3.4 (design S24: a label plus a Send feedback button, no Settings switch; the stage registry is `crates/cm-core/src/features.rs`, the UI side is `apps/desktop/src/ui/preview_label.rs`). Boards (BRD-1: Pair, Grid, Strip and Free layouts, the Make a board sheet, the library cards and `create_board`; UI name **Boards**) get the same treatment:

- A `Feature` entry in the registry: id `boards`, name `Boards`, stage Preview, with its MCP tool (`create_board`) and CLI command (`board`), so the MCP tool listing shows "(preview)" and the CLI help shows `[preview]` for them, as for the other two.
- A `PreviewFeature::Boards` in the app with everything the other two have: the Preview label on the Make a board sheet and on the board's menu and library entries where a feature name appears, the Preview popover with its text and a **Send feedback** button (the feedback form of the About button, feature name Boards), and the first-use intro card (once per feature, remembered like the others).
- Text for the popover and the intro, in the style of the existing two: what it is (several screenshots in one picture), that it is new and still changing, that existing boards and screenshots stay readable.
- All strings through `cm_i18n` with Vietnamese entries; the existing registry tests (`every_registered_preview_has_a_preview_feature` and the tool/CLI marker tests) cover it.
- `specs/features/20-boards-and-bulk.md` and the Preview design note (`docs/development/design-s24-preview-label.md`) say Boards is Preview in 0.3.5; `docs/reference/` is regenerated (the tool description gets the preview marker). Bulk export is not marked.
- Moving Boards to stable later is a one-line change in the registry.

**Seen by the maintainer on the 0.3.5 dev bundle (Nov 17, 2026): the Boards Preview label and the intro card are OK (reported by the maintainer, no stored report).**

## 5. Part 4, PROCESS: QA agent pass

An agent tests the 0.3.4 and 0.3.5 builds the way the `QA-0.3.4-*` passes did, and reports bugs. It is not a feature of the app.

| Point | Plan |
| --- | --- |
| Builds | The release candidate on each system: macOS (the notarized app copy or the dev bundle, screen unlocked, `caffeinate -d -i`), Windows (the CI package on the ThinkPad, logged in), Linux x86_64 (ThinkPad). Aarch64 only if time allows. |
| Pass 1 (start of the release) | Run on the **0.3.4 files** while the fixes are built: screenshot, annotation, library, Settings > About, import, export, Click Guide and Long Shot on the tested pages, with the main focus on the shipped Preview features. Finds bugs for the list below early. |
| Pass 2 (release candidates) | Re-run the same script on the candidates: the 8 bugs, ANN-12 (every shape, every style, preview against export), and the About button. |
| Method | Follow the existing manual scripts in `tests/manual/` and the `cm-regress` desktop probes where they exist; the agent describes what it sees in text and stores screenshots only in `verification/QA-0.3.5-shots/` (never your real library; a private library per run, [[user-describes-in-text]] applies to attachments). |
| Consent | **Real screen, camera or microphone runs need the maintainer's ask each time** ([[recording-tests-need-consent]]); the brief to the agent says so. No recording tests are run unless you ask. |
| Output | One report per pass and system as `verification/QA-0.3.5-<area>-<system>-<date>-claude.md` (template in `verification/README.md`), each finding with steps, expected, seen, severity, and a proposed class: **P1** (safety or data loss), **bug** or **polish**. |
| Triage | I read the reports and add findings to this plan as `QA-n` rows under section 2 only if P1 or a one-line fix; everything else goes to `docs/project/status.md` backlog for 0.3.6. You review the list before anything is built. |
| Limits | An agent sees text and screenshots, not taste: it cannot judge the Long Shot and Click Guide looks (the "never seen on a screen" items stay a human look, moved out). It runs one pass, not a soak. |

## 6. Order and gates

1. **Phase 0 (spec text only):** ANN-12 in `specs/features/02-annotation.md`; check BUG-8 on GitHub.
2. **In parallel:** BUG-1 to BUG-7 (Click Guide code, Long Shot on Windows, copy); ANN-12 (`cm-core`, `cm-render`, editor); the About button; QA pass 1. One `target/` per agent, disjoint files, worktrees removed when merged; I merge and retest myself.
3. **Full gate once** on the final tree: `fmt`, clippy with the CI toolchain, `cargo test --workspace --locked`, `cargo deny`, Trivy, `cargo run -p cm-i18n -- check`, `python3 scripts/check_target_os.py`.
4. **Release candidates** on all three systems: macOS DMGs signed and notarized by the maintainer, Windows from the CI workflow (unsigned), Linux tarballs and `.deb` files signed with minisign.
5. **QA pass 2, then `cm-regress`** on the final files of each system, reports in `verification/` as `L1-*-0.3.5-*`.
6. **Release day:** copy `docs/development/release-day-0.3.4.md` as `release-day-0.3.5.md`, check asset names with `scripts/check-release-assets.sh`, publish, update the Homebrew cask, the APT repository, CLAUDE.md, `status.md` and the morning report.

## 7. Exit criteria

- BUG-1 to BUG-5 and BUG-7 fixed with a test or stored check; BUG-6 fixed or named again as a known limit with what was tried; BUG-8 checked and fixed if the link is dropped.
- ANN-12: its acceptance criteria cited by tests, goldens for each shape and style, existing goldens unchanged, one look at the control on macOS and Windows with a stored report.
- The About button opens the pre-filled form and sends nothing before the click.
- QA pass 1 and pass 2 reports stored; every P1 finding fixed or named.
- No regression failure on any system; skipped cases listed by name; version 0.3.5; MCP tool count 89.
- The release text says only what a stored report supports.

## 8. Moved out of 0.3.5 (to 0.3.6 or later)

**Bugs and gaps, deferred (none is a safety or data-loss item unless it is marked):**
- Click Guide: failed clicks are silent at the 500-step cap and on disk full, and the 450 and 500 toasts were never seen (CG-02); quitting while a capture runs discards it (LS-13, CG-10, D5); an agent confirmation can cover the captured window (LS-27, D29); the editor commits typing only on Enter (CG-20); export-sheet switches look like plain squares (CG-30).
- Long Shot: fixed panels repeat in every slice of very long pictures; the Edge sync dialog on Windows.
- Windows: the busy-pipe tooltip wording; the one-off 30 to 45 s "loading library" hang (seen once, not reproduced).
- Boards and bulk export: scaling warnings under the size chip (BR-06, D9), Title and Captions below the fold (BR-04), the bulk-export size estimate, scroll cues for the Retype, Move area and Blueprint panels and the "Delete" label (D6).
- **The looks nobody has seen on a screen** (LS-18, CG-18): one human look at the Long Shot and Click Guide bars, HUD, toasts and end states.

**Checks:** the hardware and platforms never run (Intel Mac, Windows 10, SmartScreen, second monitor, mixed DPI, NVIDIA and AMD encoders, Wayland, aarch64 with a desktop, the Windows 5-minute soak, a Drive account); the Windows CI GPU test crash and the `r2_7_stale_socket` test flake; the slow tests not rerun on the merged code; the plugin check; the fuzzy Vietnamese strings (D28, D20).

**Quick Capture (moved to 0.3.6 by the maintainer on Nov 17, 2026):** the panel, the after-capture bar, presets and the agent preset argument (QC-1, QC-3, QC-4, QC-5); the spec is `specs/features/21-quick-capture.md`, the plan `specs/plans/release-0.3.6-plan.md`.

**Features and design decisions:** board drop tile and drag handle, the board stamp label and Auto style (BR-02, BR-03, D2, D7, D8), the on-screen click ring (D27), Pause on password fields and a never-record app list (D16), the Long Shot stop key, start-from-top and scroll-pane picking (D13), tiled Long Shot over 16,384 px, rich-text cap and an agent clipboard Confirm (D11), REC-9 draw on screen, video Click Guide, DOCX and PPTX guide export, WebP and HEIC import, clipboard paste as a capture, in-app update (Velopack), Windows code signing, a headless daemon, windowed audio, the full Touch-up and Blueprint versions. PhoWhisper stays out. The remaining decisions D1 to D31 stay open and do not block 0.3.5.

## 9. For the maintainer outside the code

winget pull requests 472327 and 477195; the APT repository update with the 0.3.0 to 0.3.5 `.deb` files; the plugin line (held by the directory until the organization is Verified); the Windows licence list; the ThinkPad booted and logged in before QA pass 2 and the Windows regression; for review of this plan.
