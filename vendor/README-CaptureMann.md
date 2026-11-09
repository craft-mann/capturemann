# vendor/

Trimmed copies of two crates (and two patched ones, `xim-ctext` and `whisper-rs-sys`, below), used as path dependencies of `cm-ai` (feature `sherpa`). They are not
workspace members (`exclude = ["vendor"]` in the root `Cargo.toml`), so `clippy` and `test` do not run
on them, while `cargo fmt --all`, `cargo deny` and `make licenses` still read them.

| Directory | Upstream | What changed |
| --- | --- | --- |
| `sherpa-onnx-sys` | `sherpa-onnx-sys` 2.0.1 on crates.io (Apache-2.0, https://github.com/k2-fsa/sherpa-onnx) | `build.rs` is new: it only links the static libraries in `SHERPA_ONNX_LIB_DIR` and fails with a clear message when they are missing; no download, no unpacking. The four build-dependencies (`ureq`, `zip`, `bzip2`, `tar`) and the `shared` feature are removed. `src/` (the FFI declarations) is unchanged apart from rustfmt layout and one addition in `offline_speaker_diarization.rs`: the declaration of `SherpaOnnxOfflineSpeakerDiarizationProcessWithCallback` (already in the static library, Oct 29, 2026). |
| `sherpa-onnx` | `sherpa-onnx` 2.0.1 on crates.io (Apache-2.0) | `Cargo.toml`: the `static`/`shared` features are gone and the sys crate is a path dependency. `src/` is unchanged apart from rustfmt layout and `OfflineSpeakerDiarization::process_with_progress` in `offline_speaker_diarization.rs` (progress callback of the diarization run, Oct 29, 2026; the library ignores a request to stop, see the doc comment). |

Why: the upstream build script downloaded a prebuilt archive during `cargo build` (no checksum) and
unpacked it with crates whose licences (CDLA-Permissive-2.0, bzip2-1.0.6) the project did not want in
its dependency graph. The archive is now fetched by `packaging/sherpa/fetch-macos.sh` (`make sherpa`),
pinned by SHA-256.

`cargo fmt --all` also formats path dependencies, so the sources were laid out with the project's
rustfmt (line breaks and indentation only; upstream uses a different rustfmt configuration). Keep them
formatted, do not hand-edit them.

Both directories keep upstream's `LICENSE` (Apache-2.0). They are listed in `NOTICE` and in
`THIRD_PARTY_LICENSES.md`.

To update: copy the new version of both crates from the cargo registry, re-apply the two changes above
(the diff against the crates.io files is the table), update the archive pin in
`packaging/sherpa/fetch-macos.sh`, run `make sherpa` and
`cargo test -p cm-ai --test real_diarize -- --ignored --nocapture` (see the test's header).

## xim-ctext (patched copy, Oct 31, 2026)

`xim-ctext` 0.3.0 on crates.io (MIT, https://github.com/Riey/xim-rs), used by `zed-xim` (GPUI's X11 input method
client), wired in with `[patch.crates-io]` in the root `Cargo.toml`. What changed: `compound_text_to_utf8` no
longer returns an error; the strict decoder runs first and, when it fails, escape sequences it cannot read are
dropped and the rest is read as UTF-8 or else ISO 8859-1; `src/main.rs` (a dump demo) is removed; four tests
added. Why: with `XMODIFIERS=@im=ibus` (the Ubuntu X11 default) and a Vietnamese engine, IBus sends the draft
"Viê" as the bytes `56 69 EA` (Latin-1, no escape), upstream said "not valid UTF-8" and `zed-xim` called
`expect("Encoding Error")`: CaptureMann quit on the first accented letter (VM, ibus-unikey, Oct 31, 2026).
Drop this copy when GPUI's `zed-xim` handles Latin-1 compound text. `make licenses` lists the crate as before.

## whisper-rs-sys (patched copy, Nov 9, 2026)

`whisper-rs-sys` 0.15.0 on crates.io (Unlicense, https://codeberg.org/tazz4843/whisper-rs; it carries whisper.cpp
1.8.3 and ggml, MIT, https://github.com/ggml-org/whisper.cpp), the FFI crate under `whisper-rs` 0.16 (`cm-ai`),
wired in with `[patch.crates-io]` in the root `Cargo.toml`. At the time of vendoring, 0.15.0 and whisper-rs
0.16.0 were the newest releases on crates.io (checked with `cargo search`), so no published version has the guard.

**Why.** Word times come from whisper.cpp's DTW alignment (`dtw_token_timestamps`). After every decode window
`whisper_full_with_state` calls `whisper_exp_compute_token_level_timestamps_dtw(..., n_frames, 7, ...)` with
`n_frames = min(min(3000, seek_delta), seek_end - seek)`, and the alignment keeps `n_frames/2` audio tokens.
`median_filter()` then asserts `filter_width (7) < n_audio_tokens`, which fails whenever the window's decoded
segments end within its first 160 ms (`seek_delta < 16`: the window's last timestamp token is `<|0.00|>` to
`<|0.14|>`, for example a stray token for a click at the start of a window, then silence). `WHISPER_ASSERT` is
`abort()`: the whole process quits, no error reaches the caller. The desktop app quit this way transcribing a
73-minute imported MP3 on Nov 9, 2026 (crash report: `abort <- median_filter(.cold.1) <- ggml_graph_compute <-
whisper_full_with_state <- WhisperState::full <- cm_ai::whisper::run`); it is latent in every release that
enabled DTW (0.2.x and 0.3.0), not specific to MP3 or to the audio, and a long recording reaches such a window
sooner or later. Reproduced on this Mac with a synthetic clip (20 ms of noise, then silence) in 3 s:
`crates/cm-ai/tests/real_dtw_short_window.rs` (ignored; needs a model file, downloads nothing).

**What changed** (the complete diff against the crates.io files; everything else is byte for byte upstream):

1. `whisper.cpp/src/whisper.cpp`, in `whisper_exp_compute_token_level_timestamps_dtw`: the assertion
   `n_frames <= n_audio_ctx * 2` and the one in `median_filter` are replaced by one early return before anything
   is allocated, and the `DECODER FAILED` assertion by a return that frees the ggml context. A skipped window's
   tokens keep `t_dtw = -1`, which `cm_ai::whisper::build_words` already reads as "no DTW" and falls back to the
   model's default token times for that window alone (`dtw_ok`; tested with scripted tokens in
   `a_window_without_dtw_falls_back_alone_and_the_others_keep_dtw`).

   ```diff
   @@ -8812,9 +8812,21 @@
    {
        const int n_audio_ctx = state->exp_n_audio_ctx > 0 ? state->exp_n_audio_ctx : ctx->model.hparams.n_audio_ctx;
        WHISPER_ASSERT(medfilt_width % 2);
   -    WHISPER_ASSERT(n_frames <= n_audio_ctx * 2);
        WHISPER_ASSERT(ctx->params.dtw_aheads_preset != WHISPER_AHEADS_NONE);

   +    // CaptureMann patch (vendor/README-CaptureMann.md): a window the alignment cannot use keeps
   +    // t_dtw = -1 on its tokens instead of aborting the process. Upstream 1.8.3 asserts
   +    // n_frames <= 2*n_audio_ctx here and filter_width < n_audio_tokens (= n_frames/2) in
   +    // median_filter(); the second fails whenever the window's decoded segments end within its
   +    // first 160 ms (a stray token at the start of a window, then silence), which a long recording
   +    // reaches sooner or later, and WHISPER_ASSERT is abort(). The caller then falls back to the
   +    // default token times for this window.
   +    if (n_frames / 2 <= medfilt_width || n_frames > n_audio_ctx * 2) {
   +        WHISPER_LOG_WARN("%s: DTW skipped for a window of %d frames (median filter width %d, audio context %d); its tokens keep t_dtw = -1\n",
   +                __func__, n_frames, medfilt_width, n_audio_ctx);
   +        return;
   +    }
   +
        // FIXME: Allocating mem everytime we call this func
   @@ -8854,8 +8866,11 @@
        if (!whisper_decode_internal(*ctx, *state, state->batch, n_threads, true, nullptr, nullptr)) {
   -        WHISPER_LOG_INFO("DECODER FAILED\n");
   -        WHISPER_ASSERT(0);
   +        // CaptureMann patch: a failed alignment decode leaves this window without DTW times
   +        // (upstream: WHISPER_ASSERT(0), an abort).
   +        WHISPER_LOG_WARN("%s: DTW skipped: the alignment decode failed; its tokens keep t_dtw = -1\n", __func__);
   +        ggml_free(gctx);
   +        return;
        }
   ```

   The second condition (`n_frames > n_audio_ctx * 2`) covers the `audio_ctx` decode option (a reduced encoder
   context; a measurement option of `DecodeParams`, not the shipped default), where the same function asserted
   on every window longer than the context. The `n_audio_tokens <= aheads_cross_QKs->ne[1]` assertion further
   down is implied by it (`ne[1]` is `n_audio_ctx`) and is left as it is.

2. `Cargo.toml`: the crates.io manifest with `publish = false`, an empty `[workspace]` table and a header
   comment; same name, version, features, `links` and build-dependencies (`bindgen`, `cfg-if`, `cmake`,
   `fs_extra`, `semver`), so `Cargo.lock` only changes the crate's source.

3. Left out of `whisper.cpp/ggml/src/` (about 2.6 MB): the backends that no Cargo feature of this crate can turn
   on: `ggml-cann`, `ggml-hexagon`, `ggml-musa`, `ggml-opencl`, `ggml-rpc`, `ggml-webgpu`, `ggml-zdnn`,
   `ggml-zendnn` (ggml's CMake only enters a backend folder when its `GGML_<NAME>` option is on, and
   `ggml-backend-reg.cpp` includes their headers under `#ifdef GGML_USE_<NAME>`; the headers stay in
   `ggml/include`). The backends with a feature (`cuda`, `hipblas`, `intel-sycl`, `metal`, `openblas`,
   `vulkan`, CPU, BLAS) are complete. Setting `GGML_CANN=ON` or another removed backend through the
   build script's environment pass-through would fail at configure time. Also left out: the registry's
   `.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig`.

`build.rs`, `wrapper.h`, `src/` (`lib.rs`, the fallback `bindings.rs`) and every other file are unchanged, so the
cmake build (macOS with Metal, Linux, Windows) is the upstream one. Tested on this Mac only: `cargo test -p cm-ai`,
the ignored real-engine test above (7 synthetic clips, 3 windows skipped DTW, every run completed; the unpatched
build aborted on the second clip) and `cargo clippy --workspace`. The Linux and Windows builds of the vendored
crate have not been run yet.

Upstream status: no fix was found in 1.8.3; whether a later whisper.cpp release has one could not be checked
from here (no network except crates.io). When a `whisper-rs-sys` with a guard in this function is released,
drop the copy and the `[patch.crates-io]` line; otherwise, to update, copy the new crate from the cargo
registry, re-apply change 1 (the diff above), keep change 2 and trim as in change 3, then run the two
ignored real-model tests of `cm-ai` (`real_model`, `real_dtw_short_window`).

`THIRD_PARTY_LICENSES.md` (`make licenses`) lists the crate as before (its licence file is `whisper.cpp/LICENSE`).
