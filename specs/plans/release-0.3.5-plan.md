# Release 0.3.5: a light release (plan, draft, Nov 17, 2026)

Status: **final draft for review (Nov 17, 2026); all decisions made (D3, D4, D12, D30, ANN-12); nothing built.** Scope set by the maintainer on Nov 17, 2026: a **light** release with four parts, each labelled **BUG** or **FEATURE** below:

| Part | Label | What | Size |
| --- | --- | --- | --- |
| 1 | BUG | 8 known bugs of 0.3.4 (section 2) | about 4 days |
| 2 | FEATURE | Line styles (solid, dashed, dotted) for arrow, line, rectangle, ellipse (ANN-12, section 3) | about 3 days |
| 3 | FEATURE | A Send feedback button on the About page, next to Source on GitHub (section 4) | 1 to 2 hours |
| 4 | PROCESS | A QA agent pass: an agent tests the app on each system and reports bugs (section 5) | about 1 day per system, in parallel with the build |

**Everything else is moved out of 0.3.5** (section 8). Bugs found by the QA pass or reported by users can enter the release only if they are P1 (safety or data loss) or a one-line fix; the rest go to 0.3.6. No other feature is added without the maintainer's word.

Sources: the Known limits of `docs/development/release-0.3.4-github-text.md`, `release-0.3.4-morning-report.md`, the two UX audits (`ux-audit-long-shot-click-guide-0.3.4.md`, `ux-audit-boards-bulk-0.3.4.md`), the UX proposal list (`ux-proposals-0.3.4-for-design.md`, items cited as CG-, LS-, BR-), `docs/project/status.md` and the `QA-0.3.4-*` reports in `verification/`. No user report of 0.3.4 had arrived when this was written. "Observed" below means seen in a stored QA run; "from the audit" means found by reading, not yet run.

## 1. Rules

1. **Light means light:** MCP stays at 89 tools; the only new spec ID is ANN-12 (it adds a field to existing ops, not a tool or a surface).
2. **Every fix has a test or a stored check** that fails before and passes after (a unit test, a `cm-regress` case, or a `tests/manual/` script with a report). Say "verified" only with a report in `verification/`.
3. **Scoped tests while developing, the full gate once on the final tree** ([[feedback-full-gate-once-before-release]]); clippy with the CI toolchain; Trivy gate and `cargo deny` before each merge to `main`.
4. **Windows files come from the GitHub "Windows package" workflow** (it has the speaker engine); the lab build does not.

## 2. Part 1, BUGS (8 items)

All are defects or safety gaps in features that already shipped. Order: safety and data first.

| # | Bug | Evidence | Fix (proposed default) | Size | Old ref |
| --- | --- | --- | --- | --- | --- |
| BUG-1 | **A Click Guide started by an agent or the CLI shows no bar, no Stop and no indicator** (only a toast; a guide the user starts in the app does have its bar, so this is only the agent and CLI start). Breaks "the user can always see and stop recording"; the 0.3.4 release text lists it as a known limit (stop it with `capturemann guide stop`). | Observed on macOS and Windows (QA pass 6) | The same bar as a user-started guide, in a passive form like the Long Shot HUD: dot, step count, "Started by agent {client}", Stop (no Note, no Undo); the sidebar tile shows running; the real source (CLI or MCP) is named. | M | A1, CG-01, D4 |
| BUG-2 | **Markdown export overwrites silently:** two exports into one folder overwrite the first's `guide.md` and leave stale `images/`; a guide with 0 steps reports success. | Observed on Windows (a second export to the same place, `warnings: []`) | **Decided Nov 17, 2026 (D30):** keep writing straight into the chosen folder, but put the local date and time in the names so nothing is overwritten: `guide-YYYY-MM-DD-HHMMSS.md` and `images-YYYY-MM-DD-HHMMSS/step-NN.png` (the links in the file use that folder); same local-time rule as the default titles. Refuse a 0-step guide; "Open folder" on the success toast. No Replace or Keep both question is needed. The agent path (BUG-3) uses the same names. | S to M | A2, CG-23, D30 |
| BUG-3 | **`export_guide` lets an agent write over any file** at Free tier (for example `~/Documents/report.docx`). | From the audit (not run) | Refuse an existing file; a path outside the library or the user's export folders needs Confirm showing the exact path. | S to M | A3, CG-24, D12 |
| BUG-4 | **Step titles can carry personal data:** the nearest text within 80 px becomes the title (`Click "jane.doe@example.com"`) and reaches Markdown, HTML, clipboard and agent results; hostile text can inject Markdown. | Observed on macOS and Windows | **Decided Nov 17, 2026 (D3):** if the clicked button or object has an accessibility name (AX or UI Automation), use it; otherwise **keep the current logic** (the nearest text on screen). The fallback can still pick up personal text; this is accepted for 0.3.5 (it is already in the 0.3.4 text: check a guide before you share it). Cheap extras that do not change titles: escape Markdown in exports. | M | A4, CG-12, D3 |
| BUG-5 | **Click Guide ignores a moved window:** the followed window's rectangle is read once, so after a move the ring and title are wrong (a step was titled Export after clicking Save); a click on a window covering the guided one becomes a step. | Observed on macOS | Re-read the rectangle at each click; pause with a notice when it moves; ignore a click that lands on another window. | M | A5, CG-03 |
| BUG-6 | **Long Shot on Windows ends after 2 frames** on YouTube and carchargeos.com (Edge, 1280x900). Wikipedia, docs.rs and the essay page work. | Known limit shipped in 0.3.4; cause unknown | **Time-boxed to 1 day:** reproduce on the ThinkPad, find why the scroll settle or frame compare stops, add a `cm-regress` case on a local tall page with a sticky header and lazy images (no internet). If it is not fixed in the box, it stays a named known limit and moves to 0.3.6. | M to L | B1 |
| BUG-7 | **The board sheet says "drag order = list order"** (Vietnamese "kéo để đổi thứ tự") but the order changes only with the arrow buttons; the pill says "Layout editable until Finish". | Observed on macOS (library-boards pass) | Copy fix only: "Use the arrows to change the order" (EN, VI), pill "Preview". No drag handle. | S | C1, BR-05 |
| BUG-8 | **The 0.3.4 Preview feedback link may lose its pre-filled text:** `.github/ISSUE_TEMPLATE/config.yml` has `blank_issues_enabled: false`, so `issues/new?title=...&body=...` can land on the template chooser. | Suspected; **check first** (open the link as a user would) | If the text is dropped, point the link at a template (`template=bug_report.yml` or `feature_request.yml`) or add a "Feedback" issue form. Also decides the link for the About button (section 4). | S | new |

**Decisions the bugs need (defaults proposed; "ok" accepts):**

| D | Question | Default |
| --- | --- | --- |
| D3 | BUG-4: accessibility name, else the current nearest-text logic | **Decided Nov 17** (revised) |
| D4 | BUG-1: passive bar with Stop, no Pause, for agent- and CLI-started guides | **Decided Nov 17**: in 0.3.5 |
| D12 | BUG-3: refuse an existing file; Confirm with the exact path outside known folders | **Decided Nov 17**: default |
| D30 | BUG-2: write into the chosen folder with date and time in the names | **Decided Nov 17** (revised) |

## 3. Part 2, FEATURE: line styles for shapes (ANN-12)

**What exists today.** Arrow, Line, Rectangle and Ellipse are already built (ANN-1: tools A, L, R, E; ops `arrow`, `line`, `rect`, `ellipse` in `crates/cm-core/src/annotation.rs`). They have colour and width and only a solid stroke; the renderer (`crates/cm-render/src/annot.rs`, `stroke_round`) has no dash support. The feature is **a stroke style: solid, dashed or dotted**, on all four shapes.

| Point | Proposal |
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

**Decided Nov 17, 2026:** all the defaults above (dash numbers, solid arrow head, last style remembered per tool).

## 4. Part 3, FEATURE: Send feedback button on the About page

**Today.** Settings > About (`apps/desktop/src/ui/settings/general.rs`, `about()`) has two secondary buttons in one row: `craftmann.app` and `Source on GitHub`. A Send feedback button and its sending code already exist for the Long Shot and Click Guide Preview card (`ui/preview_label.rs`: `feedback_url`, `send_feedback`, `url_encode`).

**Work (1 to 2 hours):**
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
| Pass 2 (release candidates) | Re-run the same script on the candidates: the 8 bugs and ANN-12 (every shape, every style, preview against export) and the About button. |
| Method | Follow the existing manual scripts in `tests/manual/` and the `cm-regress` desktop probes where they exist; the agent describes what it sees in text and stores screenshots only in `verification/QA-0.3.5-shots/` (never your real library; a private library per run, [[user-describes-in-text]] applies to attachments). |
| Consent | **Real screen, camera or microphone runs need the maintainer's ask each time** ([[recording-tests-need-consent]]); the brief to the agent says so. No recording tests are run unless you ask. |
| Output | One report per pass and system as `verification/QA-0.3.5-<area>-<system>-<date>-claude.md` (template in `verification/README.md`), each finding with steps, expected, seen, severity, and a proposed class: **P1** (safety or data loss), **bug** or **polish**. |
| Triage | I read the reports and add findings to this plan as `QA-n` rows under section 2 only if P1 or a one-line fix; everything else goes to `docs/project/status.md` backlog for 0.3.6. You review the list before anything is built. |
| Limits | An agent sees text and screenshots, not taste: it cannot judge the Long Shot and Click Guide looks (the "never seen on a screen" items stay a human look, moved out). It runs one pass, not a soak. |

## 6. Order and gates

1. **Phase 0 (half a day, spec text only):** ANN-12 in `specs/features/02-annotation.md` and the FR table; check BUG-8 on GitHub.
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

**Features and design decisions:** board drop tile and drag handle, the board stamp label and Auto style (BR-02, BR-03, D2, D7, D8), the on-screen click ring (D27), Pause on password fields and a never-record app list (D16), the Long Shot stop key, start-from-top and scroll-pane picking (D13), tiled Long Shot over 16,384 px, rich-text cap and an agent clipboard Confirm (D11), REC-9 draw on screen, video Click Guide, DOCX and PPTX guide export, WebP and HEIC import, clipboard paste as a capture, in-app update (Velopack), Windows code signing, a headless daemon, windowed audio, the full Touch-up and Blueprint versions. PhoWhisper stays out. The remaining decisions D1 to D31 stay open and do not block 0.3.5.

## 9. For the maintainer outside the code

winget pull requests 472327 and 477195; the APT repository update with the 0.3.0 to 0.3.5 `.deb` files; the plugin line (held by the directory until the organization is Verified); the Windows licence list; the ThinkPad booted and logged in before QA pass 2 and the Windows regression; and nothing else: no decision is open.
