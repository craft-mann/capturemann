# Release 0.3.4: plan (draft, Nov 13, 2026)

Status: **scope decided Nov 13, 2026: hygiene + Magnify + canvas text selection and caret + CMYK JPEG conversion (section 4, recommended scope); **Phase 0 (spec text) written Nov 13, 2026 (section 6)**; questions 3 to 5 of section 5 and the defaults of section 6 await the maintainer; nothing built.** Written the day 0.3.3 was published (tag `v0.3.3` on `85df0235`). Inputs: the 0.3.3 plan and morning report (`release-0.3.3-plan.md`, `release-0.3.3-final-morning-report.md`), the 0.3.3 known limits, the carry-over list of `docs/development/release-day-0.3.3.md`, `docs/development/velopack-spike-0.3.2.md`, `docs/development/windows-signing-options.md`.

## 1. What 0.3.3 left open

| Item | Where it comes from | State |
| --- | --- | --- |
| **Windows regression of 0.3.3** | the lab laptop could not boot Windows on release day (Microsoft outage) | not run; the release text says so |
| **Plugin check against the installed app** (`tests/manual/PLUGIN-1.md`) | release-day step "Claude plugin [M]" | not run |
| **Blue "new" dots** on the new editor tools | decided: they go after one release | to remove in 0.3.4 |
| **Windows CI: a GPU test process died** (twice, a different test each time: `cm-daemon export_real`, `cm-render compose`) | CI runs on `57d8a515` and `85df0235` | rerun was green; cause unknown |
| **`cm-daemon` socket test flake** (`ipc::tests::r2_7_stale_socket...` under load) | local full run | passes alone |
| **Tests not re-run on the merged code** | 49 of 52 `cm-daemon` test binaries, slow GPU and export tests of `cm-render` and `cm-media` | run them once on a quiet machine |
| **Vietnamese strings** | about 227 fuzzy drafts in `locales/vi.po` | maintainer's review |
| **CMYK JPEG import** | refused (`jpeg_cmyk`); the spec says "convert or refuse" | decide |
| **File types** | `Info.plist` and the Windows installer do not register PNG and JPEG for "Open With" | missing |
| **winget** | pull requests 472327 (0.2.8) and 477195 (0.3.1) open | waiting |
| **Text on the canvas** | no real text selection, caret does not blink, a drag-select stops at the field edge | known gaps of the text-fields work |
| **Magnify (ANN-8)** | moved out of 0.3.3 | design in `release-0.3.3-plan.md` section 2 items 1 and 2 |
| **Clipboard paste of a picture as a capture** | stretch goal of 0.3.3, not built | |
| **WebP and HEIC import** | refused in 0.3.3 | needs a licence and size check (HEIC: patents, decoder) |
| **In-app update** | Velopack spikes: macOS "go with changes"; needs hosting and the maintainer's decisions | parked |
| **Windows code signing** | SignPath Foundation recommended; the application is the maintainer's | files unsigned |
| **Headless daemon / `capturemann mcp` starting the app** | tools answer `daemon_unavailable` without the app | design point: Confirm-tier dialogs need a screen |
| **Windowed audio** | flat memory for long files and speakers | no release assigned |
| **PhoWhisper** | "not considered for now" | stays out |

## 2. Lessons of 0.3.3 to build into the process

1. **Phase 0 is spec text and the frozen format only**, not code (it took about 2 hours and delayed the parallel tracks).
2. **One `target/` per agent** (with `CARGO_PROFILE_DEV_DEBUG=line-tables-only`): the shared target made five agents queue on one lock. The disk allowed four.
3. **Scoped tests while developing, the full gate once at merge, a time limit on every command.** The full `cargo test --workspace` took hours on this Mac (GPU compose tests 80 to 180 s each, 52 `cm-daemon` test binaries).
4. **Merge each track as it finishes**; merge bugs (the same icon registered twice) show up at the first compile of the merged tree.
5. **Run clippy with the toolchain CI uses** (`cargo +1.100.0 clippy`; this Mac's stable was one release behind and CI found four lints). Keep that toolchain installed.
6. **Check the release assets before uploading**: the macOS files must be `CaptureMann-v<version>-macos-<arch>.dmg` (the cask and README expect it; 0.3.3 first went up with the wrong names and was corrected). Add a script that compares the asset names with the cask URL pattern and `releasing.md`.
7. **A Windows regression needs the lab laptop booted into Windows with a console login**; plan it before release day, not on it.
8. **The tap repo has no SSH key configured**: set `core.sshCommand` there like the main repo, so the cask push needs no special step.

## 3. Candidate scope, grouped

- **A. Hygiene (always in), with the bug list of section 3b (B1, B2):** Windows regression of 0.3.3 and fixes; PLUGIN-1 check; remove the "new" dots; investigate the Windows CI GPU crashes (one `--no-capture` run, or skip-list the GPU tests there); the flaky socket test; the full test pass on a quiet machine; asset-name check script; image file types in the installers; Vietnamese review (maintainer).
- **B. Annotation (user-visible):** Magnify (ANN-8: about 3 days, the Phase 1 scale spike first); canvas text selection and caret blink; clipboard paste of a picture; CMYK JPEG converted instead of refused.
- **C. Platform and distribution:** in-app update (Velopack: about 3 to 5 days plus hosting and signing decisions, macOS first); Windows code signing (maintainer application, then CI changes); WebP import (HEIC later).
- **D. Agents:** `capturemann mcp` starts the app when it is closed (needs the Confirm-tier decision) or a headless daemon.
- **E. Voice:** windowed audio (flat memory for hours-long files).

## 3b. Bug list (maintainer reports; more to come)

| # | Report | First analysis (not confirmed by a run) | Planned fix |
| --- | --- | --- | --- |
| B1 | Deleting a recording fails with "couldn't delete the capture `reg...` is being uploaded (job `job...`)" (Nov 13, 2026) | Same cause as the caption-save bug fixed in 0.3.3: an upload job **paused** with `auth_expired` (Google sign-in expired) counts as active. 0.3.3 made edits ignore paused jobs but kept `ensure_idle_strict` for rename and delete, so delete and rename stay refused until the account is reconnected or the job is cancelled by hand. Two such paused uploads existed in the maintainer's library. | Delete cancels the capture's paused upload and publish jobs itself (after the existing delete confirmation) and goes on; a **running** or queued job still blocks with the same message. Rename needs no block for a paused job (jobs name the capture id, not the folder): check that and relax it. Test in `library_ops.rs` and a daemon integration test. Also show in the Library that an upload is "waiting for sign-in" with a Reconnect button, so the state is not a dead end. |
| B2 | Windows user: the recording HUD cannot be dragged while recording **audio** (Nov 13, 2026) | Dragging is `window.start_window_move()` on the HUD bar's background (`ui/recording/hud.rs`, 0.2.9; tested for screen recordings on Windows). The audio HUD may place its own children (meter, marker field, label row) over the bar or stop the press, or the native move may behave differently for that window kind on Windows. Needs a reproduction on the ThinkPad (Windows 11). | Reproduce on Windows with an audio recording; make every non-button part of the audio HUD a grip; add a `cm-regress` desktop probe case if it can be automated (`desktop.hud_click_through` is the nearest). |

## 4. Recommended scope (for discussion)

**0.3.4 = A + Magnify + the canvas text gaps + CMYK conversion**, about the size of 0.3.3 (a few days of agent work, a day of verification, the release). It keeps the release about the editor you just reviewed, adds no new risk to updates or signing, and finishes the promises of 0.3.3 (Magnify, text selection). In-app update, signing, headless daemon and windowed audio stay for 0.3.5 or later, each as its own release (they change how people receive and run the app).

Alternative scopes:
1. **Update release:** A + in-app update on macOS (+ Windows signing started). Larger, more risk; changes the installers and needs a hosting decision (`craftmann.app/updates/...`).
2. **Small hygiene release:** A only, in about two days; ships Windows verification and the process fixes, no new features.
3. **Agents release:** A + MCP starts the app (decision on Confirm-tier dialogs).

## 5. Questions for the maintainer

1. (Answered Nov 13: **the recommended scope**.) Which scope: the recommended one, or one of the alternatives (section 4)?
2. (Answered Nov 13: **convert to sRGB**.) CMYK JPEG: convert to sRGB (needs a colour-management step; `moxcms` is already in the tree) or keep refusing?
3. Does Magnify keep the decided rules (the loupe renders the source from `original.png` again at the zoom with the earlier ops; blur and pixelate always apply to every copy of the pixels)?
4. Windows code signing: has the SignPath Foundation application been sent? (It decides whether 0.3.4 can ship signed files.)
5. When can the lab laptop run Windows for the regression (a console login is needed)?

## 6. Phase 0 (spec text only; written Nov 13, 2026)

Written, no code: `features/02-annotation.md` (ANN-8 Magnify in full with the frozen op JSON `magnify { source, at, zoom, shape, border, border_width, connector, badge }` and AC1 to AC9; ANN-1 text selection, copy and paste, caret blink as AC9 to AC13), `features/18-import.md` (IMP-6: CMYK and YCCK JPEG converted, AC10 to AC12; file types in the installers, AC13), `features/09-library.md` (LIB-1 AC8 to AC10: delete and rename with a paused upload, "waiting for sign-in" with Reconnect), `features/15-voice.md` (VOC-1 AC12: the audio HUD drags on every system), `00-product-spec.md` (ANN-8 P1), `02-agent-interface.md` (`annotate` lists `magnify`, at most 4), `docs/development/releasing.md` (asset names and the `check-release-assets.sh` rule).

**Defaults I assumed (the maintainer answers or accepts):** up to 4 magnifiers per image; the source may lie outside a crop (a crop is only the visible window); text selection includes copy, cut and paste in 0.3.4; the loupe is the source scaled by the zoom and centred on `at`, nested loupes are skipped; Copy text reads magnified text twice (as drawn).

**Frozen:** the op JSON above; the limits (zoom 2 to 8, 4 per image, source at least 8x8, loupe at most 32 Mpx and 16 384 px a side, border width 0 to 32). After this the tracks may start.

## 7. Tracks (disjoint files; one `target/` per agent)

| Track | Scope | Files | Size |
| --- | --- | --- | --- |
| A | Magnify core and render: op, validation, scale parameter in the renderer, loupe with earlier ops and redactions, connector, badge, goldens, OCR test, MCP schema, CLI, 0.3.3 round-trip fixture | `cm-core/src/annotation.rs`, `cm-render/**`, `cm-mcp` schema, CLI | 2 days |
| B | Magnify editor (tool M, live loupe while dragging, inspector, handles) and the canvas text selection, copy and paste, caret blink | `apps/desktop/src/editor/**`, `ui/editor_view/**` | 2 days |
| C | CMYK conversion, installer file types (`Info.plist`, Windows installer, `.desktop`), B1 (paused-upload delete and rename, "waiting for sign-in" and Reconnect), B2 (audio HUD drag, after a reproduction on Windows), remove the "new" dots | `cm-media`, `cm-library` import, `cm-daemon/src/library_ops.rs`, `ui/library_view.rs`, `ui/recording/hud.rs`, `packaging/**` | 1.5 days |
| D | Hygiene: `scripts/check-release-assets.sh`, the Windows CI GPU crashes, the flaky socket test, a quiet-machine full test run, the plugin check, docs and manual scripts (`ANN-8.md`), `cm-regress` cases for Magnify and CMYK | `scripts/**`, `crates/cm-regress`, `tests/manual`, `docs/**`, `.github/workflows` | 1 day |

Order: Phase 0 (done), the four tracks in parallel, merge each track when it is green (fmt, clippy 1.100, scoped tests), one full gate on `main`, candidates, regression on all three systems (book the lab laptop in Windows first), release.
