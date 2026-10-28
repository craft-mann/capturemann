# Intel Macs: implementation plan (release 0.2.1)

Status (Oct 28, 2026): **draft, not started.** Written after 0.2.0 shipped (Apple Silicon only). The maintainer has an Intel Mac for testing. Platform order is unchanged (maintainer, Oct 25, 2026): Intel Macs, then Windows, then Linux. No release dates (maintainer rule).

Goal: one CaptureMann release that runs on both Apple Silicon and Intel Macs (macOS 13+), with every 0.2.0 feature working on Intel, and with defaults that suit slower hardware.

## Why this is small

The app code is already macOS-only and uses the same APIs on both chips: ScreenCaptureKit, AVFoundation, VideoToolbox, Core Audio, `wgpu` on Metal. The work is in four places: native components built for arm64 only, the bundle and release scripts, performance defaults, and testing on real Intel hardware. What was found in the repo (Oct 28, 2026):

| Part | Today | Needed |
|---|---|---|
| `packaging/ffmpeg/build-macos.sh` | refuses to run unless `uname -m` is arm64; `-arch arm64`, libvpx `--target=arm64-darwin22-gcc` | an x86_64 build (needs `nasm`), then `lipo` |
| `packaging/llama/build-macos.sh` | arm64 check, `CMAKE_OSX_ARCHITECTURES=arm64`, Metal | an x86_64 build; decide Metal or CPU on Intel |
| `packaging/sherpa/fetch-macos.sh` | arm64 check, `osx-arm64-static-lib` archive | the x86_64 static libraries with their own pinned SHA-256 (check the release has `osx-x86_64`; if not, build ONNX Runtime and sherpa-onnx) |
| `cm-ai` (`whisper-rs` with `metal`) | arm64 Metal | portable x86 CPU flags (no host-only `-march=native`), Metal fallback to CPU |
| Rust workspace | built for the host | `aarch64-apple-darwin` and `x86_64-apple-darwin`, joined with `lipo` |
| `packaging/macos/bundle.sh`, `verify-release.sh` | verify-release fails any Mach-O that is not `arm64` only | assemble a universal app; accept `arm64 x86_64` |
| `release.yml`, `Makefile` | `macos-14` arm64 runner, DMG named `...-macos-arm64.dmg` | universal build, new DMG name, Homebrew cask |

## Decisions needed from the maintainer

1. **One universal DMG or two DMGs?** Universal was the earlier decision. It makes one download and one cask, but the app grows. The 0.2.0 app is about 181 MB, and the executables are roughly half of it (ONNX Runtime alone is 75 MB of static library). Models are data and are not duplicated. Expect somewhere around 280 to 350 MB, to be measured in Stage 2. Fallback: two DMGs (`-arm64`, `-x86_64`), each as small as today, and the cask picks by architecture. **Recommendation:** build universal in Stage 2, measure, and fall back to two DMGs only if the size hurts.
2. **Is the first-use setup sheet part of 0.2.1?** It is already planned for 0.2.1, and it matters more on Intel because those users need to be steered to lighter models. **Recommendation:** include it, but as the last stage so Intel does not wait for it.
3. **Oldest supported Mac.** macOS 13 requires roughly 2017 and later Intel Macs. Macs without a hardware H.264 encoder (before about 2016) are already unsupported. Say in the docs which year range was tested, based on the Mac you have.

## Stage 0: spike on the Intel Mac (blocks the rest)

Do this first and cheaply. It tells us what is broken before any packaging work.

**I0.1** Cross-compile the workspace for `x86_64-apple-darwin` on the Apple Silicon Mac (`rustup target add x86_64-apple-darwin`, `--no-default-features` so `sherpa` and the text engine are not needed yet). Copy the dev-signed app to the Intel Mac.
**I0.2** On the Intel Mac, run with fakes and the synthetic fixture only (`make_fixture`). Check that the app starts, `wgpu` picks a GPU, the editor preview and export (headless compositor) work, and note the Mac model, year, GPU and macOS version in the report.
**I0.3** Hardware checks, run by the maintainer, one at a time (CLAUDE.md rules apply; I never run them): screen recording, camera, microphone, system audio, cursor, hardware H.264 encoder, 2 s fragments, permissions prompts. Result per item, as reported.

Exit: a short report in `verification/INTEL-SPIKE-<date>-maintainer.md` with pass or fail per item and the Mac model. Any fail becomes a work item before Stage 1.

Also try running the x86_64 build under Rosetta on the Apple Silicon Mac for quick smoke tests. Rosetta is not Intel hardware (AVX2 support depends on the macOS version), so it never replaces the real Mac for acceptance.

## Stage 1: native components for x86_64

Each script gets an architecture switch (`CM_ARCH=arm64|x86_64`, default the host) instead of a hard refusal. Cross-building on the Apple Silicon Mac is the plan; the Intel Mac is for testing, not for building.

**I1.1 FFmpeg** (`make ffmpeg`): `brew install nasm`; `-arch x86_64`, `--arch=x86_64 --enable-cross-compile`, libvpx `--target=x86_64-darwin22-gcc`, libopus; same component list and the same LGPL self-checks (no GPL, no nonfree, no libx264). Output goes to `out/x86_64/` and a `lipo -create` step produces the universal `ffmpeg` and `ffprobe`. Keep the 0.2.0 licence, `BUILD-INFO` and `SOURCE-OFFER` files accurate for both.
**I1.2 llama-server** (`make llama`): `CMAKE_OSX_ARCHITECTURES=x86_64`, `GGML_NATIVE=OFF` with an explicit portable x86 baseline (AVX2 and FMA if the oldest supported Mac has them; otherwise a lower baseline), Metal on or off per the I0 result and I3.1. Keep it static with no libcurl or OpenSSL. `lipo` into one universal `llama-server`.
**I1.3 sherpa-onnx** (`make sherpa`): fetch the x86_64 static libraries, pin their SHA-256 in the script, keep the ONNX Runtime notice. Link per architecture; a universal `cm-desktop` is built as two Cargo builds joined by `lipo`.
**I1.4 whisper.cpp** (via `whisper-rs`): confirm the x86_64 build uses portable flags and that Metal initialisation failure falls back to CPU instead of aborting.
Tests: the engine tests already use scripted platforms, so they run unchanged on x86_64 under Rosetta. Real-engine checks (`real_model`, `real_diarize`, the text engine) run on the Intel Mac by the maintainer, with the commands in CLAUDE.md. No test downloads a model.

## Stage 2: bundle, signing and release scripts

**I2.1 `bundle.sh`:** build both Rust targets, `lipo` every Mach-O (`cm-desktop`, `cm-overlay`, `capturemann`, `cm-mcp`, `cm-daemon` and the helper programs), sign, harden. `make bundle-dev` keeps working on the host architecture only so the dev loop stays fast; `CM_UNIVERSAL=1` (set by `make bundle-release`) builds both.
**I2.2 `verify-release.sh`:** accept `arm64 x86_64` for every Mach-O and fail if any file is single-architecture in a universal release. Keep the signature, entitlements, licence and minimum-macOS checks.
**I2.3 DMG and cask:** name `CaptureMann-vX.Y.Z-macos-universal.dmg` (or two names under decision 1); update `Makefile`, `dmg.sh`, the README download line, `docs/development/releasing.md`, `packaging/macos/README.md` and the Homebrew cask (`craft-mann/homebrew-tap`).
**I2.4 Size:** record the app and DMG size in `BUILD-INFO` and in the release notes. If over the budget chosen in decision 1, switch to two DMGs.
**I2.5 `release.yml`:** build both architectures. Cross-build on the arm64 runner; use an Intel runner only to run the smoke tests if one is available (check which hosted Intel runner images still exist when this is built). The workflow stays unsigned without secrets, as today. Also make it manual-only for releases made by hand (the open item from 0.2.0).

## Stage 3: performance and defaults on Intel

This is where Intel Macs differ most from Apple Silicon: no unified memory, older or integrated GPUs, and slower CPU inference.

**I3.1 Measure** on the Intel Mac, with the CLI on the 11-minute test meeting (the same one used for 0.2.0): transcription with Tiny, Base, Small, Large v3 Turbo; speaker labels; a Gemma report; Vietnamese translation. Record wall-clock time and peak memory. Compare Metal against CPU for Whisper and `llama-server` where Metal works.
**I3.2 Defaults by hardware:** extend the existing `min_memory_gb` idea in `text_models.json` and the speech-model recommendation. Likely outcome (to be confirmed by I3.1): recommend a smaller Whisper model and Gemma 4 E2B or Qwen3.5 2B on Intel, keep Large v3 Turbo available but not recommended, and show a plain "this will be slow on this Mac" line using a time estimate instead of a warning.
**I3.3 Job behaviour:** check the chunked report (45-minute test) and export do not time out or run out of memory; the export has no stall timeout (known gap), so test a long export on the Intel Mac and decide whether to add the timeout now.
**I3.4 Encoders and presets:** check which H.264 and HEVC presets the Intel hardware encoder accepts, and that the 60 fps self-test gives an honest result on slower GPUs.
Any new UI string goes through `cm_i18n` with Vietnamese; errors use the shared codes.

## Stage 4: first-use setup sheet (decision 2)

One place to get the speech and text models the first time a feature needs them, with the architecture-aware recommendation from I3.2. Follows the existing spec and `design/CLAUDE.md`. Keep it behind its own stage so an Intel release does not wait on it.

## Stage 5: tests, docs and release

**I5.1 Manual script** `tests/manual/INTEL-0.2.1.md`: clean install from the DMG on the Intel Mac, permissions, a recording with every source, editor and export, audio recording and editing, speakers, report, translation, and an upgrade from a 0.2.0 library. Report in `verification/` (template in its README); nothing is called "verified" without it.
**I5.2 Upgrade check:** a 0.2.0 library created on Apple Silicon opens on Intel and the reverse (bundles are plain files, so this should hold; test it, do not assume).
**I5.3 Docs:** README and site ("Apple Silicon only" wording, system requirements, download buttons), `CHANGELOG`, `docs/project/status.md` and `plan.md`, `specs/00-product-spec.md` platform lines, third-party licence generation for the x86_64 components.
**I5.4 Release:** `make release` from the release machine, notarize, staple, `spctl` check on both Macs, Homebrew cask, GitHub release notes with the supported Mac range. The owner pushes.

## Acceptance (the release is ready when)

- Every Mach-O in the app has both architectures, signed, hardened, notarized and stapled; `verify-release.sh` passes.
- On the Intel Mac, as reported: install from the DMG, record screen, camera, microphone and system audio, edit, export, transcribe, label speakers, write a report, translate.
- Defaults on Intel are chosen from I3.1 measurements, and the numbers are stored in the report.
- Apple Silicon behaviour is unchanged: the 0.2.0 checks still pass on the Apple Silicon Mac.
- `cargo fmt`, `clippy -D warnings`, workspace tests, `cm-i18n check` and `cargo deny` pass; no GPL or AGPL in either architecture.

## Risks

- **Slow inference on older Intel Macs.** Mitigation: Stage 3 defaults and time estimates; the app still works, only slower.
- **Metal on old Intel or AMD GPUs** can misbehave in ggml. Mitigation: CPU fallback (I1.2, I1.4) tested on the actual GPU.
- **No x86_64 sherpa-onnx static library** published. Mitigation: build it ourselves; adds a few days.
- **Only one Intel Mac** means one GPU and one CPU generation tested. State exactly which Mac in the report and in the system requirements.
- **App size** with universal binaries. Mitigation: decision 1.
- **Rosetta hides real problems.** Mitigation: acceptance only on the real Mac.

## Not in 0.2.1

Windows, Linux, VOC-3 live transcript, Ollama and LM Studio (backlog).
