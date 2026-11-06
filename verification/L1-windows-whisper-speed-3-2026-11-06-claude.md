# Windows lab: transcription speed with the /O2 package, round 3 (PARTIAL)

Status: **PARTIAL** (not "verified"). ThinkPad T490 (i7-8665U, Windows 11 build 26200), lock `C:\lab\lab.lock` taken as `verify7-agent`, released at the end.
Package: CI run 42069547556 (commit 2e299d68, adds `/O2 /Ob2 /DNDEBUG` to the ggml build), setup.exe SHA-256 `1cd60f05...f27e` checked, installed silently over the
laptop's install after stopping every `cm-*` process; installed `cm-daemon.exe` SHA-256 `79cc2633...23af` equals the one in the package zip.
`capturemann diagnostics`: CaptureMann 0.2.7, speech engine CPU features AVX2/FMA/F16C/BMI2 on (it shows no commit; the hash match is the proof of the build).
The maintainer's `cm-desktop` was started again in the user session afterwards (session 2).

## Speed (50 s synthetic two-voice clip `speech2.wav`, `capturemann ai transcribe --wait`, RUST_LOG=debug, private config/library/pipe, 8 threads)

| Model | `perf: transcribe` line | Before (round 2 package) |
| --- | --- | --- |
| ggml-small | 21.3 s wall, whisper 21.2 s, peak 0.9 GB, 116 words | 303.7 s (about 14x faster; the CI test build was 32 to 40 s) |
| ggml-medium (downloaded from the public ggerganov/whisper.cpp repo, 1533763059 bytes) | 76.8 s wall, whisper 76.7 s, peak 2.2 GB, 116 words | not measured before |

The /O2 fix works. The CI log of run 42069547556 has no CMake or compiler output (1217 lines, no `/O2`, no cl.exe lines), so the flag could not be confirmed from the log;
the timing is the evidence. The `CM_WINDOWS_CRT_STATIC=0` experiment is not needed.

## Hardware cases (cm-regress from tests run 42065663771, `--only ai.no_model,ai.speakers,ai.transcribe,rec.camera`, report `L1-windows-0.2.7-regression-hardware-verify7-2026-11-06-claude.md`)

3 passed, 1 failed. `rec.camera` PASS (room lit): 1280x720 h264, measured 26.1 fps, 5.59 s. `ai.transcribe` PASS (26.8 s job). `ai.speakers` PASS (2 speakers on the words).
`ai.no_model` FAIL, and it is a product bug, not a suite bug: with `CM_SPEAKER_MODELS_DIR` set (what `--speaker-models` does) and no speech model installed,
`capturemann ai transcribe` started a job with model `speaker-segmentation` (exit 0) instead of `no_model` (exit 2). Reproduced by hand, and the same call without the variable gives exit 2.
Cause: `Inner::pick_model` (`cm-daemon/src/ai/dispatch.rs`) took the first installed model without checking its kind. Fixed in source with a unit test (commit 65fbe944), not rebuilt.

## Minimum recording area (real Media Foundation encoder, screen only, 4 s each, installed package, `agents.tool_policy.start_recording=allow` in a private config)

18x18: `screen.mp4` h264, 18x18, 122 frames, 4.37 s. 20x20: h264, 20x20, 120 frames, 4.23 s. Both record and have frames.
The case `rec.region_min` (record.rs, commit c017edde) does the same inside cm-regress; it is not in the tests build 42065663771, so it has not run as a case.

## Not done

Second monitor cases (not connected). The new cm-regress case and the daemon fix need a Windows tests build and a package build, then `ai.no_model` and `rec.region_min` re-run.
Removed afterwards: private config/library folders, the medium model, helper scripts. Volume was not changed (no `sound` case ran).
