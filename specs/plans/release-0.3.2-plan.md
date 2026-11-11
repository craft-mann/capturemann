# Release 0.3.2: plan (draft)

Status: **plan written Nov 11, 2026 (scope from the maintainer); decisions below made the same day; nothing built.** Follows 0.3.1 (published Nov 11, 2026).

## Decisions (maintainer, Nov 11, 2026)

- **Systems:** 0.3.2 ships on all three platforms (macOS, Windows, Linux); version bump to 0.3.2, release process and release material as for 0.3.1.
- **PhoWhisper:** benchmark the equivalent PhoWhisper models against the original Whisper models, Vietnamese transcription only: PhoWhisper-medium vs Whisper medium, PhoWhisper-large vs Whisper large-v3 Turbo. PhoWhisper-small is out of scope (it makes no difference); do not compare small with Turbo. Purpose: decide whether to bring PhoWhisper into the app.
- **Windows signing:** the maintainer checks SignPath Foundation (eligibility, application); the research table still covers the other options.
- **Thumbnail:** no migration of existing bundles is required.
- **Carry-overs:** windowed audio moves to 0.3.3; the redaction box left-edge trim and the process-tree kill on cancel are the must-haves, the rest best-effort.

## Scope (maintainer)

1. **PhoWhisper on a Vietnamese recording: is it better?** The question is open again with the 0.3.1 pipeline (piece-by-piece
   transcription, large-v3 default for long files). What is known: on Nov 3, 2026 a PhoWhisper-small converted for whisper.cpp
   (`packaging/phowhisper/convert.sh`) was not usable with the app's settings (mean WER 60.9 % on FLEURS, mostly deleted words; Large v3
   Turbo 10.6 %); `phowhisper-medium` and `-large` are `pending_pin` (not converted); `verification/benchmarks/vietnamese-models-2026-11-03.md`,
   `broken-tokens-2026-11-03.md`. First step: a fair comparison on one real Vietnamese meeting recording of the maintainer (private: counts only
   in the report) with the maintainer's reference lines, against the original Whisper medium and large-v3 Turbo (equivalent sizes, see Decisions), in WER, speed, coverage, invented lines (the guard) and memory.
2. **The screenshot thumbnail must reflect the redaction.** A screenshot with Auto-redact boxes applied (or drawn by hand) must show the
   redacted image in the library thumbnail, not the original. Check where the thumbnail is made (library index, `.cmshot` bundle) and that
   it is rebuilt when a redaction is added, changed or removed; a test with a known pixel region. Spec: `specs/features/09-library.md`, the redaction
   feature file.
3. **Sign the Windows release (a community or low-cost route) so it distributes better.** 0.3.1's installer shows Edge's "isn't commonly
   downloaded" warning (`verification/L1-windows-0.3.1-download-smartscreen-edge-2026-11-11-maintainer.md`). Options to compare, with cost,
   eligibility, who holds the key, and effect on SmartScreen (reputation is not instant, even signed): SignPath Foundation (free for open
   source; needs a public build pipeline and acceptance, decision D-W4 of Oct 30, 2026), Azure Artifact Signing, an OV or EV certificate,
   winget (which does not remove SmartScreen for a browser download), Microsoft Store (MSIX). Output: a recommendation and, if chosen,
   signing in `windows-package.yml`.

## Carried over from 0.3.1

Windowed audio (flat memory for long files and speakers), process-tree kill on cancel, the redaction box left-edge trim, a speech-detector
second opinion for the outro guard (Vietnamese only today), an "Always ignore this line" action, `estimate_vs_real` on Windows, Intel Mac
hardware, Windows 10, a 16 GB Mac for long recordings, the cm-overlay exit race (grace added in `bded21fe`, unexplained), winget (PR 472327
and 477195 open).

## Order and first steps (for the session that starts 0.3.2)

Work on `main`; the maintainer pushes, tags and publishes. Follow `docs/development/release-day-0.3.1.md` for the release; ask only product or scope questions.

1. **Thumbnail (item 2), first: small, clear test.**
   - Find where the `.cmshot` thumbnail is made (`cm-library`, `cm-render`) and which redaction path skips it (Auto-redact apply, hand-drawn box, remove).
   - Fix: rebuild the thumbnail from the edited image whenever redaction changes (add, change, remove, undo).
   - Test: a bundle with a known solid region; after a redaction the thumbnail pixels in that region are the redaction fill; after undo they are the original. Cite the AC id in the test and commit.
   - Spec: add an AC to the redaction feature file and to `specs/features/09-library.md`; status line in `docs/project/status.md`.
2. **Windows signing (item 3), research with no build.**
   - Table: cost, eligibility, who holds the key, CI fit, SmartScreen effect, for SignPath Foundation, Azure Artifact Signing, OV/EV certificate, Microsoft Store (MSIX). Check current conditions on the vendors' pages; do not rely on memory.
   - Recommendation to the maintainer (it needs an account or a payment, so it is their decision). If chosen: signing step in `windows-package.yml`, installer and CLI/app executables signed, `release-files.sh` and the release text updated, a clean-PC download check with the SmartScreen screens recorded in `verification/`.
   - Expect reputation to build slowly even when signed; say so in the release text.
3. **PhoWhisper (item 1), needs the maintainer.**
   - Need: one real Vietnamese recording and its reference lines (private, counts only in the report); ask which file.
   - Convert medium and large with `packaging/phowhisper/convert.sh` (pins in the script; check the converted tokens first, see `broken-tokens-2026-11-03.md`); compare each with its Whisper equivalent (medium, large-v3 Turbo) on WER, speed, coverage, invented lines (guard) and memory; store a benchmark in `verification/benchmarks/`.
   - Only if a model clearly wins: pin it in `crates/cm-ai/data/models.json` (`pending_pin` today) and upload to the Hugging Face org `craftmann`.
4. **Carry-over bugs** (section above): the process-tree kill on cancel and the redaction box left-edge trim first (known bugs); windowed audio is moved to 0.3.3 (decision above).

## Release gate (as in 0.3.1)

CI green on three systems; `exp4_ac4_cancel_deletes_partial_output` on five consecutive `windows-tests.yml` runs; a stored `cm-regress` report of the final files per system; release text with no `<FILL>` left; assets named `CaptureMann-v<ver>-macos-<arch>.dmg` for the cask. Lab: ThinkPad T490 addresses in `~/.config/capturemann-lab`; aarch64 VM through `/tmp/cm-vm.sh`.
