# Windows CI: a GPU test process dies (investigation notes, Nov 14, 2026)

Status: not fixed, not reproduced on the lab laptop; cause is a hypothesis. Written for 0.3.4 (track F, hygiene).
Release 0.3.3's own CI needed two reruns of the Windows check for this; the main branch went green on the third.

## What was seen

Run 43362173287 (`ci.yml`, `check (windows-latest)`, commit "text fields: switch on the shared field in the rename
dialog and search"), the step `cargo test --workspace --locked --no-fail-fast ... --skip compose_ms_per_frame_1080p_and_4k
--skip m4_gate_export_matches_the_preview_frame_for_frame`:

```
Running tests\export_real.rs
running 6 tests
test export_video_renders_records_and_announces ... ok
test ai1_burn_needs_a_transcript_but_the_default_never_blocks ... ok
test missing_ffmpeg_fails_the_job_once_with_the_hint ... ok
error: test failed, to rerun pass `-p cm-daemon --test export_real`
  process didn't exit successfully: ...export_real-....exe --skip ... (exit code: 2173)
note: test exited abnormally; to see the full output pass --no-capture to the harness.
```

* The three tests that were still running (`trim_then_export_video_renders_the_trimmed_range`,
  `export_starts_at_live_at_and_matches_the_reported_duration`, `ai1_burned_in_captions_follow_the_trim_and_the_choice`)
  each build the wgpu compositor and run FFmpeg; the whole **process** ended, so no test name, panic message or
  backtrace was printed (the harness captures output).
* Exit code 2173 (0x87D) is not an NTSTATUS crash code (those are 0xC0000xxx) and not a Rust panic (101): the process
  called `exit` or was ended by the runtime. A device-lost or driver abort in a software adapter fits; nothing in the log proves it.
* Per the 0.3.3 record, the failing test was a different one on each rerun, which points at timing and at the shared GPU
  path, not at one test.
* The runner has no GPU: wgpu falls back to the WARP software adapter of D3D12 (`windows-latest`; assumed, the log does
  not print the adapter). The product's own export path ran without such a death on the lab ThinkPad (Intel UHD 620, real
  driver): `L1-windows-0.3.3-*` reports, hardware tier 63/0. Those are `cm-regress` runs, not these test binaries.

## Hypothesis

Six `#[tokio::test]` functions run in parallel threads of one process, and each creates its own wgpu instance, adapter and
device on WARP and renders (some of them at 1080p). Concurrent device creation and heavy submission on WARP in a debug
build (opt-level 0 for our code; dependencies at opt-level 2 since 0.2.8) can end the process. macOS and Linux CI run the same tests
without trouble (a real Metal device, or Mesa lavapipe with its own locking).

The tests already avoid the two slowest GPU cases on Windows (`--skip` above); the lab laptop runs the parity test.

## Not done (cheap next steps, in this order)

1. Make the failure visible: run the Windows step once with `-- --nocapture` and `RUST_BACKTRACE=1`, and keep the
   run's minidump if Windows Error Reporting made one (`%LOCALAPPDATA%\CrashDumps`, enable `LocalDumps` in the workflow).
2. Take the question out of the test list: run only the GPU binaries (`export_real`, `edit_view_ops`, `cm-render`'s
   compositor tests, `cm-media` `edit_view_export`) with `--test-threads=1` on Windows, in their own `cargo test` invocation.
   If the dying stops, the cause is concurrent device use; then give the tests one shared device
   (a `OnceLock` in `tests/common`), which also makes them faster everywhere.
3. If it still dies single-threaded, force the adapter (`WGPU_BACKEND=vulkan` with lavapipe is not on the runner; try
   `WGPU_BACKEND=gl` or `dx12` with `WGPU_DX12_COMPILER=fxc`) and compare.

Do not add a blanket retry of the step: it hides real crashes of the compositor. A retry that re-runs only
the failed binary and records "retried" in the job summary would be acceptable if step 1 shows nothing.

## Related

`docs/development/regression.md` (the lab runs the real hardware path), `specs/plans/release-0.3.4-plan.md` section 9.2.
The macOS GPU slot convention (`/tmp/cm-gpu`) is for local agents on one Mac and plays no part in CI.
