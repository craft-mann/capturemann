# Speaker engine teardown abort: stress before and after (Nov 10, 2026)

Status: PARTIAL. One Mac, synthetic audio only; the abort was not reproduced, so the fix is
structural and its effect on the real failure is unverified.

## The crash

Maintainer's dev app, Nov 10, 2026 16:04 +07, macOS 26.6.2, Apple Silicon. "Find speakers" on a
4 h 13 min audio capture with the automatic count, 76 % after about 9 minutes. SIGABRT from
`std::terminate`: `onnxruntime::logging::LoggingManager::~LoggingManager` (a C++ exception out of
a destructor) <- `Environment::~Environment` <- `OrtEnv::Release` <- `OfflineSpeakerSegmentationPyannoteModel::Impl::~Impl`
<- `SherpaOnnxDestroyOfflineSpeakerDiarization` <- `SherpaEngine::run` <- `run_windowed`. In the
crash report no other thread was inside ONNX Runtime (one tokio worker was faulting, the others
idle), so the overlap with another thread is not shown; the lifetime is: each 10-minute window
created a diarizer and destroyed it, and ONNX Runtime's environment is created with the first
object and destroyed with the last, about 25 times in this run.

## Change

`crates/cm-ai/src/speaker_engine/ort_guard.rs`: every sherpa object (diarizer, voice-print
extractor) is created and destroyed under one process-wide lock; the first creation also leaks a
voice-print extractor (28 MB model) so the environment never loses its last reference. Running a
window is not under the lock, so stop and cancel work as before; one engine per window keeps the
memory behaviour of the windowed design.

## Stress (`crates/cm-ai/tests/real_teardown_stress.rs`)

Audio: 2 h 2 min of five macOS `say` voices rendered to files (no recording of a person, no
playback), cut in 60 s windows; each run creates the engine, runs the automatic count, drops it.

| State | Runs | Threads | Aborts | Wall time |
| --- | --- | --- | --- | --- |
| Before (engine created and destroyed per run, no lock) | 40 | 3 | 0 | 218 s |
| After (lock + keep-alive environment) | 60 | 3 | 0 | 377 s |

After the change the process made exactly 1 keep-alive holder (`ort_keepers() == 1`). The
per-run time was 5.4 s before and 6.3 s after on this Mac (one measurement each, debug build of
our code, 3 runs sharing the cores); the extra cost is not separated into lock wait and noise.
Peak memory with the keep-alive (a constant extra 28 MB model) was not measured.

## Not verified

- The real abort did not reproduce here, with or without the change: it may need the ORT state
  after a long run or the maintainer's build; the fix is not shown to remove it.
- Other platforms (Windows, Linux link the same sherpa-onnx), a 4 h run end to end.
