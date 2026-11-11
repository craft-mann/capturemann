# 0.3.1 final morning report (night of Nov 10 to 11, 2026)

Written by the overnight orchestrator (a Claude fork) for the maintainer's review on Nov 11. It is updated during the night; the
**running log** at the end says how far it got. Nothing was pushed, tagged or published by the orchestrator. Wording rule: PARTIAL
reports only; "verified" is not used.

## Where things stand (updated as the night goes)

| System | CI on `5e1a0d9d` | Final-file regression | Files |
|---|---|---|---|
| macOS arm64 + x86_64 | green (`check (macos-latest)`, `bundle-macos`) | standard 43/0, full 45/0 on reruns (first full 43/2, cm-overlay exit race, fixed in `bded21fe`), hardware 54/0 | `~/release-0.3.1/macos` (both DMGs, notarized and stapled, `.sha256` checked) |
| Windows 11 | green (clippy, tests, `exp4`) | standard 44/0, full 58/0, ai.transcribe + ai.speakers + installer.silent 3/0 (PARTIAL, `L1-windows-0.3.1-regression-*final*`) | `~/release-0.3.1/win` (package of run 42838999379) |
| Linux aarch64 | green (ubuntu check) | VM `full` 37/0, tarball install 1/0, `.deb` install 1/0 | `~/release-0.3.1/linux/aarch64` (checked with `check-package.sh` in the VM: PASS both) |
| Linux x86_64 | green | not run | not built (ThinkPad stayed on Windows) (needs the ThinkPad in Ubuntu) |

## Commits made during the night (all local, nothing pushed)

`49d196d9`, `bded21fe` (earlier), and one commit with the Windows final-file reports and this report. Nothing pushed.

## What you must do, in order

See `docs/development/release-day-0.3.1.md`. In short: push the local commits (docs and reports only, code is identical to
`5e1a0d9d`), watch CI (docs-only commits still run the whole workflow), tag, sign the Linux files with minisign, upload, APT,
winget (not for 0.3.1, pull request 472327 is still open), Homebrew.

## Vietnamese strings that are still drafts (fuzzy) and are new in 0.3.1

Of 134 fuzzy entries in `locales/vi.po`, most are the 0.3.0 Linux/platform drafts (117 from 0.3.0) and plural forms that were already
marked. The ones added in 0.3.1 and not yet reviewed by you:

1. "The words for this part are still being made." (words line in the audio editor)
2. "The transcript is being written on {this_computer}. The first part is below and the rest follows by itself. …" (partial transcript note)
3. "Preparing the sound… {pct} %" (instant editor)
4. "Exporting {pct}%" (export button)
5. The guard strings on the AI page: "Add a phrase", "Leave out lines the audio does not hold", "Lines of a new transcript that contain one of
   these phrases are left out and counted. …", "Lines to leave out", "No phrases yet. Nothing is left out unless you add one.", "Part of a
   line, any capitals or accents", "Phrase to leave out", "Remove {phrase}", "Sometimes a transcript has a sentence that nobody said, over a
   stretch with no speech. …", "That phrase is already in the list.", "The list is full. Remove a phrase to add another.", "Type at least
   three letters first.", "Remove".

(You confirmed the Vietnamese wording of the export popover, the partial playback and the guard on your own Mac tonight; they are still
marked fuzzy in the file because I did not clear the flags.)

## Open decisions

- Nothing blocking. winget stays out of 0.3.1.

## Running log

- 22:14 started. macOS hardware tier (no sound) 54/0, `rec.system_audio` with sound 1/0 (dev bundle). aarch64 VM final files: PASS.
- Nov 11: Windows final-file regression reports committed. `windows-tests.yml` had 4 consecutive green runs on `5e1a0d9d`; a fifth was triggered (run 42929895419). Still open: Linux x86_64 build and regression, `estimate_vs_real`, `<FILL>` items in the release text (release day).
