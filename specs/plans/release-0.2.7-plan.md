# Release 0.2.7 plan: macOS stabilisation and faster, more accurate AI

Status: **draft** (Nov 3, 2026), nothing built. Scope set by the maintainer on Nov 3, 2026, right after 0.2.6:

- **0.2.7 is macOS only:** stabilisation, plus faster and more accurate **transcription**, **speaker detection** and
  **report writing**.
- **Windows catch-up moves to 0.2.8, Linux catch-up to 0.2.9** (kept below under "Later releases"). The 0.2.7 release
  does not change the Windows and Linux early builds; they stay at their 0.2.6 state and the release text says so.
- The work is split into two tracks that run **at the same time** in separate git worktrees, each driven by its own
  agent (see "How the agents work").

Rules that apply to every track (from `CLAUDE.md` and the maintainer): general techniques only, no phrase lists or
one-recording fixes (open source); **nothing is called "verified", "accurate" or "faster" without a stored report in
`verification/`** (benchmark reports in `verification/benchmarks/`); no GPL/AGPL in shipped binaries; `target_os` only
in platform code (`python3 scripts/check_target_os.py`); every user-facing action has an MCP tool and a CLI command;
UI text through `cm_i18n`; one change per commit with the two trailer lines; no failing tests merge to `main`; the
maintainer pushes `main`, signs, publishes and notarizes.

## Where 0.2.6 left things (inputs to this plan; the Linux and Windows rows feed 0.2.8 and 0.2.9)

| Area | State at 0.2.6 | Source |
| --- | --- | --- |
| macOS Intel | Issue 21: Record panel, webcam bubble and (before 0.2.6) the export toast stay on top on an Intel Mac; only the toast wording is fixed, the stuck windows are unexplained, the reporter was asked to retest 0.2.6 and send the log | issue 21 |
| Transcription | Whisper large-v3-turbo on CPU/GPU; long recordings are cut into pieces, looping pieces are split and re-run (29 Whisper calls for 17 pieces on 88 min, cost unmeasured); Vietnamese is not measured | `docs/project/performance.md` |
| Speaker detection | 17 % diarization error on held-back English clips; Vietnamese not measured; an unknown speaker count can run the engine twice (88 min: 265 s plus 263 s); memory grows with length (0.6 GB at 10 min, 3.5 GB at 150) and Cancel is ignored until the run ends; `window_shift_ratio` 0.1 is the main cost | `docs/project/performance.md`, `verification/benchmarks/speaker-detection-2026-10-29.md` |
| Reports and translation | Built-in text model (Gemma 4 E2B); an 88 min Vietnamese-to-English translation took 691 s and 6.5 GB; repeated-text stand-ins failed with `output_too_long` and lost their partial result; report accuracy never measured | `docs/project/performance.md` |
| Linux | `.deb`/tarball early build, no recording (code exists behind `CM_LINUX_RECORDING=1`, never seen on a real desktop); Wayland window capture and global shortcuts have no path on GNOME 46; aarch64 only in a VM | `docs/development/linux-early-build.md`, `verification/LINUX-1-*` |
| Windows | early build, unsigned, no recording, no text recognizer, no speaker engine, no bundled FFmpeg; picker app names, 1 px size difference, "Unknown Monitor 65537"; SmartScreen and Windows 10 never run | `verification/L1-windows-thinkpad-2026-11-02-claude.md`, `specs/plans/release-0.3-0.4-platforms-plan.md` |

## Track M1: macOS stabilisation (agent `mac-stability`)

Goal: nothing a user can see gets stuck, and failures leave a report.

1. **Issue 21 (Intel).** Reproduce in the pieces we can: the Record panel is closed by the code on Start, the setup
   bubble on Start, the toast on dismiss. Add the missing evidence first: log lines at each close and each window
   creation/destruction (`cm_log`), a "Copy diagnostics" check that lists the live app windows, and a watchdog test
   with a scripted window host that fails to close. Then fix what the logs show. Do not guess a fix without a log.
   Wait for the reporter's 0.2.6 log; ask on the issue if none arrives.
2. **Window lifetime audit.** Every floating surface (panel, bubble, toast, overlay, launcher) gets one owner, one close
   path and a test that a failed close is retried or reported. Applies to Windows and Linux hosts too (shared code in
   `apps/desktop`).
3. **Long-session health.** A 3-hour soak of recording plus the library open (`tests/manual/VOC-1.md` soak,
   `REC-*`), memory and file-handle growth recorded in a report.
4. **Intel confirmation.** Ask for one more Intel report or borrow a machine; until then the Intel DMG stays
   "built on Apple Silicon, checked under Rosetta" in every public text.
5. **Errors in the app log.** `desktop.log` shows `RefCell already borrowed` (3 to 47 per session, always in a burst of
   3) followed by `window not found`, nine times between Oct 30 and Nov 2, and once `timed out waiting on app_will_quit`
   (Nov 2). The pattern points at a re-entrant borrow while a window is closing (the same family as issue 21's windows).
   Find which action triggers it (log lines at each window close, see task 1), make the close path not borrow twice,
   and add a test with a scripted window host. The log lines carry no module name, so give them one.
6. Items from the 0.2.6 review that touch macOS only: none other known. Add any found in M1.

Done when: a stored soak report, window-lifetime tests in CI, issue 21 either fixed with a log-backed cause or still
open with the evidence listed.

## Track M2: macOS AI speed and accuracy (agent `mac-ai`)

Everything starts with a **benchmark that exists before the change**, stored under `verification/benchmarks/`, so
every claim is a before/after on the same data. Public wording: "measured on ...", never "accurate".

### M2a Transcription
- Build a word-error-rate (WER) harness over public clips with reference text (English only, O1; no private recordings in the
  repository). Report WER and real-time factor per model.
- Measure the cost of the repeat re-runs (performance.md item 3) and cut it: detect looping with a general rule,
  re-run only the looping piece with a changed temperature or without the previous text as prompt, not a phrase list.
- Long-recording speed: overlap of pieces, GPU vs CPU split, model choice by length. Target set after the baseline.
- Vietnamese: not in 0.2.7 (O1); the release text lists it as not measured.

### M2b Speaker detection
- **Baseline for 3+ speakers** (today only English, 14 clips); English only (O1).
- Remove the second engine run for unknown speaker counts (performance.md item 1): options are a grouping pass over
  the voice prints, or a first-pass cap that does not need a rerun. Needs a change in the vendored sherpa-onnx
  wrapper (`vendor/sherpa-onnx*`, trimmed copies: keep the no-download build rule).
- Windowed runs to cap memory and make Cancel work (item 2).
- Retune `window_shift_ratio` and the clustering threshold on a larger set than 14 clips (item 4), reporting DER
  next to time for each setting.

### M2c Reports and translation
- Measure report **speed** (tokens per second on M-series and Intel) and **quality**: define a checkable rubric
  (every item links to a timestamp that exists; all speakers named in the transcript appear; no invented names or
  numbers, checked by string match against the transcript) and score a fixed set of transcripts. Report the
  scores; no "accuracy" claim without the report.
- Fix `output_too_long` for translation (performance.md item 5): keep partial work, split the piece and retry
  (the reports path already splits).
- Candidate speed-ups to measure, not assume: prompt/KV cache reuse across pieces, smaller context for short
  pieces. The model stays Gemma 4 E2B (O2); a different quant of it needs a pinned URL and SHA-256 in
  `crates/cm-ai/data/text_models.json` (no `pending_pin` entries ship) and the maintainer's approval.

### M2d Findings from a real Vietnamese meeting (Nov 2, 2026) and the fixes they point to
Input: the maintainer's 45 min meeting `EmeSoft Weekly Meeting Nov 2.cmrec` (6285 words, `vi`, large-v3-turbo, 2 to 6
speakers). Analysed by script on Nov 3, 2026. Findings are about **kinds** of failure; the recording itself is
private and never enters the repository or a test (a synthetic or public clip reproduces each kind). English-only
claims stay (O1), but these failure kinds are language independent, so they are in scope.

1. **Speech that got no words.** 29 gaps longer than 8 s add up to 546 s with no words; the speaker engine marks
   329 s of them as speech, and the gaps have normal signal level. Fix to measure: compare speech regions (VAD or the
   speaker engine) with word coverage; re-run uncovered speech regions on their own (a piece that starts at the
   region, no previous text as prompt); report coverage (seconds of speech with words / seconds of speech) in the
   benchmark. Baseline first.
2. **Repeated word with spread-out times.** One word said 7 times, 14 to 39 s apart, each on weak signal and
   high self-confidence, beside the gaps above. The loop detector (`pieces.rs`) only counts words in a row and needs
   8 of a single word, so it misses this. General rule to test: the same short text recurring while the gap before it
   is long and the signal under it is low (energy and speaker-engine agreement), and drop or re-decode such words.
   No phrase list.
3. **Broken characters.** 3 words with U+FFFD or stray symbols and 1 word with a Korean character, 102 words under
   5 % confidence. They sit at the start of the file (first 12 s) and in weak-signal stretches. `join_split_characters`
   already joins split bytes, so what remains is bytes that never form text. Fix to test: a token that decodes to
   U+FFFD, or a script that does not belong to the transcript's language set and has low confidence, is re-decoded
   (temperature fallback) or marked doubtful, never shown as text. Whisper runs greedy with one candidate and no
   fallback or no-speech settings today (`whisper.rs`): measure the fallback parameters (temperature steps,
   no-speech and log-probability thresholds, and a smaller or beam search) on the WER benchmark before changing them.
4. **Short repeated phrases (11 in the meeting)** look like real disfluency ("lý do lý do"). Keep; only the long
   or low-confidence ones are suspect. Do not collapse repeats shorter than the loop rule.
5. **Speakers:** the file has 6 named speakers though the Record sheet said 2 people; the speaker count was set
   afterwards by hand. M2b should report how the count was chosen next to the DER.

Done when: before/after reports for each of M2a, M2b, M2c are stored, the tests (with fakes) cover the new
splitting/cancel behaviour, and `tests/manual/AI-5/7/8/9.md` have been re-run on the new build.

## How the agents work (parallel worktrees)

Following the parallel-agent rules we already use (disjoint files, unique script names, I merge and retest).

```sh
# once per track, from the main checkout, after `git checkout main && git pull --ff-only public main`
git worktree add ../capturemann-wt/mac-ai        -b track/mac-ai
git worktree add ../capturemann-wt/mac-stability -b track/mac-stability
```

The `track/*` branches are **temporary work areas**; the maintainer's rule "work on `main`, no feature branches" is
about the shared line of work. Each agent's finished, tested commits are merged to `main` by me (fast-forward or
cherry-pick, keeping the commit messages), never pushed by the agent. Delete each worktree and branch after merging.

| Track | Owns (may change) | Must not touch without asking |
| --- | --- | --- |
| mac-ai | `crates/cm-ai`, `crates/cm-media` (transcription and report paths), `vendor/sherpa-onnx*`, `verification/benchmarks/`, `tests/manual/AI-*.md`, and only the job progress/cancel files in `crates/cm-daemon` named in its brief | `apps/desktop` UI beyond progress and cancel wiring; the rest of `cm-daemon` |
| mac-stability | `apps/desktop` (window lifetime, diagnostics), `apps/overlay`, `crates/cm-daemon` (recording lifecycle), soak scripts | `crates/cm-ai`, the job progress/cancel files given to mac-ai |

**Shared files with one owner** (agents add entries on their branch, I resolve at merge): `Cargo.lock` and workspace
`Cargo.toml` (me; version stays 0.2.6 until the release commit bumps all to 0.2.7), `locales/capturemann.pot` and
`vi.po` (regenerate with `cargo run -p cm-i18n -- extract` after the merge, not on the branch), `CHANGELOG.md` and
`docs/project/status.md` (me, from the agents' final reports), `THIRD_PARTY_LICENSES.md` (`make licenses` after a
`Cargo.lock` change).

**Hardware on the Mac:** recording tests (including the 3-hour soak in M1) need the maintainer's consent and a time
slot, stay as short as the check allows, and the output is deleted (`CLAUDE.md`). The agents never start one on their
own. The Linux/Windows laptop is not used in 0.2.7.

**Each agent's brief** names the goal, the files it owns, the baseline report to create first, the commands to run
(`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, `python3 scripts/check_target_os.py`; the CI Linux and Windows jobs must stay green), what it must not do (run `make bundle` over
`target/bundle`, push, publish, say "verified" without a report) and the report it hands back: commits, tests run,
reports written, what was not done.

## Order and checkpoints

1. **Week 1:** baselines and reproductions only (M2 benchmarks, M1 logging). No behaviour change merges before its
   baseline report exists. Decisions O1, O2 and O6 are made (see below).
2. **Week 2:** fixes and speed-ups with before/after reports; M1 posts a "shared code done" note when its
   `apps/desktop` window-lifetime commits are merged. "No measurable gain" is an acceptable result for any speed-up: the
   report is stored and the change is not merged.
3. **Week 3:** the soak (with the maintainer's slot), the Intel check, the manual scripts `AI-5/7/8/9` re-run, full test
   run on all three CI OSes (the Linux and Windows jobs must stay green even though nothing new ships for them).
4. **Release:** release text from the reports; macOS packages and DMGs by the pattern of 0.2.6 (draft, user signs, user
   publishes, I check one download, cask script, status entry). The Windows and Linux early builds are not rebuilt
   for 0.2.7 unless a shared-code change forces it; the release text says that they stay at 0.2.6. Version bump to
   **0.2.7** is the release commit.

CI: shared-code commits from M1 touch `apps/desktop`, which Linux and Windows also build, so the CI jobs for both
(`ci:windows` label on a branch) run before merge.

## Decisions (made by the maintainer on Nov 3, 2026)

- **O1** Vietnamese: **English only** in 0.2.7. Baselines, WER and DER cover English; Vietnamese is listed as not
  measured in the release text. No Vietnamese corpus work, no Vietnamese-specific model change.
- **O2** Text model: **keep Gemma 4 E2B.** M2c measures and speeds up the current model; no second model, no new pin.
  Remove the "larger model" candidate from the speed-up list.
- **O6** Issue 21: **wait for the reporter's 0.2.6 log.** No cutoff and no diagnostics-only build. The logging and
  window-lifetime work (M1) still ships; the release text does not claim a fix for Issue 21 unless a log-backed cause is
  found. If the log arrives late, it goes into 0.2.8 or a 0.2.7.x.
- Windows work may touch `crates/cm-ai` in Windows-only files and separate commits (0.2.8 plan); `mac-ai` is not
  blocked by it, and the maintainer merges one track at a time.

Moved to the 0.2.8 and 0.2.9 plans: O3 (Linux packaging), O4 (Windows signing), O5 (recording on Linux and Windows).

## Risks

- Accuracy work without a good Vietnamese set can only be claimed for English; the release text must say so.
- A change inside the vendored sherpa-onnx wrapper must keep the no-download, no-GPL build rules; it goes in its own
  commit with its own before/after report.
- Windowed speaker runs (M2b) can split one speaker across windows; the gate is that DER on the held-out English clips
  does not get worse than a tolerance set from the baseline. Retuning uses a tuning set and a separate held-out set.
- The report rubric (M2c) is a floor check (timestamps exist, speakers present, no invented names or numbers); it does
  not measure whether claims are right, and the release text must not call it accuracy.
- Two tracks edit near `apps/desktop` and `cm-daemon`; the ownership table gives the shared job files to `mac-ai` by name,
  and M1's shared-code commits merge first, then `mac-ai` rebases.
- The Linux and Windows early builds get no fixes in 0.2.7; any user report of a bug there is logged for 0.2.8 or 0.2.9.

## Later releases (not part of 0.2.7)

Moved out on Nov 3, 2026 so 0.2.7 can stay on macOS. Each has its own plan, owned by a separate session and agent that
runs alongside 0.2.7 on its own worktree (shared code is merged after M1's, see each plan):

- **0.2.8, Windows:** `specs/plans/release-0.2.8-plan.md`
- **0.2.9, Linux:** `specs/plans/release-0.2.9-plan.md`

The one test laptop (Linux and Windows dual boot) is not used by the 0.2.7 tracks. O3, O4, O5 live in those plans.
