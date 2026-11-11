# Release 0.3.2: plan (draft)

Status: **draft, scope from the maintainer on Nov 11, 2026, nothing built.** Follows 0.3.1 (published Nov 11, 2026).

## Scope (maintainer)

1. **PhoWhisper on a Vietnamese recording: is it better?** The question is open again with the 0.3.1 pipeline (piece-by-piece
   transcription, large-v3 default for long files). What is known: on Nov 3, 2026 a PhoWhisper-small converted for whisper.cpp
   (`packaging/phowhisper/convert.sh`) was not usable with the app's settings (mean WER 60.9 % on FLEURS, mostly deleted words; Large v3
   Turbo 10.6 %); `phowhisper-medium` and `-large` are `pending_pin` (not converted); `verification/benchmarks/vietnamese-models-2026-11-03.md`,
   `broken-tokens-2026-11-03.md`. First step: a fair comparison on one real Vietnamese meeting recording of the maintainer (private: counts only
   in the report) with the maintainer's reference lines, against large-v3 and Turbo, in WER, speed, coverage, invented lines (the guard) and memory.
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
