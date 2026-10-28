# Intel Macs: implementation plan (release 0.2.1)

Status (Oct 28, 2026): **draft, not started.** Written after 0.2.0 shipped (Apple Silicon only). **Decided (maintainer, Oct 28, 2026): two DMGs, one per chip, not a universal app (install size matters); the project website offers the exact download for each Mac. The maintainer has no Intel Mac at hand yet and will find one later**, so the build work (Stages 1 and 2) can start first and be checked under Rosetta, while everything that needs the real Mac (Stage 0 hardware checks, Stage 3, acceptance) waits. Platform order is unchanged (maintainer, Oct 25, 2026): Intel Macs, then Windows, then Linux. No release dates (maintainer rule).

Goal: a release with a separate DMG for Intel Macs (macOS 13+) next to the unchanged Apple Silicon DMG, with every 0.2.0 feature working on Intel and defaults that suit slower hardware. Each DMG holds a single architecture, so the Apple Silicon download stays about 181 MB or smaller.

## Why this is small

The app code is already macOS-only and uses the same APIs on both chips: ScreenCaptureKit, AVFoundation, VideoToolbox, Core Audio, `wgpu` on Metal. The work is in four places: native components built for arm64 only, the bundle and release scripts, performance defaults, and testing on real Intel hardware. What was found in the repo (Oct 28, 2026):

| Part | Today | Needed |
|---|---|---|
| `packaging/ffmpeg/build-macos.sh` | refuses to run unless `uname -m` is arm64; `-arch arm64`, libvpx `--target=arm64-darwin22-gcc` | an x86_64 build (needs `nasm`) in its own output folder |
| `packaging/llama/build-macos.sh` | arm64 check, `CMAKE_OSX_ARCHITECTURES=arm64`, Metal | an x86_64 build in its own folder; decide Metal or CPU on Intel |
| `packaging/sherpa/fetch-macos.sh` | arm64 check, `osx-arm64-static-lib` archive | the x86_64 static libraries with their own pinned SHA-256 (check the release has `osx-x86_64`; if not, build ONNX Runtime and sherpa-onnx) |
| `cm-ai` (`whisper-rs` with `metal`) | arm64 Metal | portable x86 CPU flags (no host-only `-march=native`), Metal fallback to CPU |
| Rust workspace | built for the host | a separate build for `x86_64-apple-darwin` (cross-compiled on the Apple Silicon Mac) |
| `packaging/macos/bundle.sh`, `verify-release.sh` | verify-release fails any Mach-O that is not `arm64` only | assemble one app per architecture; check that every Mach-O matches the architecture of the DMG being checked (`arm64` or `x86_64`) |
| `release.yml`, `Makefile` | `macos-14` arm64 runner, DMG named `...-macos-arm64.dmg` | a second job or step for `...-macos-x86_64.dmg`, the Homebrew cask per architecture, the website download buttons |

## Decisions needed from the maintainer

1. **Decided: two DMGs** (`CaptureMann-vX.Y.Z-macos-arm64.dmg` unchanged, new `CaptureMann-vX.Y.Z-macos-x86_64.dmg`). No universal build, no `lipo` of the app. Consequences: the website must send each user to the right file (a browser cannot tell Apple Silicon from Intel reliably, so show both buttons with a one-line "Apple menu > About This Mac: Chip" hint); the Homebrew cask picks by architecture (`on_arm` and `on_intel`); each release has two `.sha256` files and two notarizations; the release notes list both.
2. **Is the first-use setup sheet part of 0.2.1?** It is already planned for 0.2.1, and it matters more on Intel because those users need to be steered to lighter models. **Recommendation:** include it, but as the last stage so Intel does not wait for it.
3. **Oldest supported Mac.** macOS 13 requires roughly 2017 and later Intel Macs. Macs without a hardware H.264 encoder (before about 2016) are already unsupported. Say in the docs which year range was tested, based on the Mac you have.

## Stage 0: spike on the Intel Mac (needs the real Mac; blocks Stage 3 and acceptance)

The maintainer has no Intel Mac yet. **Until one is available, do Stages 1 and 2 (they only need the Apple Silicon Mac and Rosetta), and keep Stage 0 hardware checks, Stage 3 and acceptance for later.** The cheap fixture run in I0.1 and I0.2 can happen as soon as any Intel Mac is available, and tells us what is broken before the rest of the testing.

**I0.1** Cross-compile the workspace for `x86_64-apple-darwin` on the Apple Silicon Mac (`rustup target add x86_64-apple-darwin`, `--no-default-features` so `sherpa` and the text engine are not needed yet). Copy the dev-signed app to the Intel Mac.
**I0.2** On the Intel Mac, run with fakes and the synthetic fixture only (`make_fixture`). Check that the app starts, `wgpu` picks a GPU, the editor preview and export (headless compositor) work, and note the Mac model, year, GPU and macOS version in the report.
**I0.3** Hardware checks, run by the maintainer, one at a time (CLAUDE.md rules apply; I never run them): screen recording, camera, microphone, system audio, cursor, hardware H.264 encoder, 2 s fragments, permissions prompts. Result per item, as reported.

Exit: a short report in `verification/INTEL-SPIKE-<date>-maintainer.md` with pass or fail per item and the Mac model. Any fail becomes a work item before Stage 1.

Also try running the x86_64 build under Rosetta on the Apple Silicon Mac for quick smoke tests. Rosetta is not Intel hardware (AVX2 support depends on the macOS version), so it never replaces the real Mac for acceptance.

## Stage 1: native components for x86_64

Each script gets an architecture switch (`CM_ARCH=arm64|x86_64`, default the host) instead of a hard refusal. Cross-building on the Apple Silicon Mac is the plan; the Intel Mac is for testing, not for building.

**I1.1 FFmpeg** (`make ffmpeg`): `brew install nasm`; `-arch x86_64`, `--arch=x86_64 --enable-cross-compile`, libvpx `--target=x86_64-darwin22-gcc`, libopus; same component list and the same LGPL self-checks (no GPL, no nonfree, no libx264). Output goes to `out/x86_64/` next to the existing arm64 output; no `lipo`. Keep the 0.2.0 licence, `BUILD-INFO` and `SOURCE-OFFER` files accurate for both.
**I1.2 llama-server** (`make llama`): `CMAKE_OSX_ARCHITECTURES=x86_64`, `GGML_NATIVE=OFF` with an explicit portable x86 baseline (AVX2 and FMA if the oldest supported Mac has them; otherwise a lower baseline), Metal on or off per the I0 result and I3.1. Keep it static with no libcurl or OpenSSL. Output goes to `out/x86_64/`.
**I1.3 sherpa-onnx** (`make sherpa`): fetch the x86_64 static libraries, pin their SHA-256 in the script, keep the ONNX Runtime notice. Libraries are kept per architecture (`out/lib` for arm64 as today, `out/x86_64/lib` for Intel) and the Cargo build picks the folder from the target.
**I1.4 whisper.cpp** (via `whisper-rs`): confirm the x86_64 build uses portable flags and that Metal initialisation failure falls back to CPU instead of aborting.
Tests: the engine tests already use scripted platforms, so they run unchanged on x86_64 under Rosetta. Real-engine checks (`real_model`, `real_diarize`, the text engine) run on the Intel Mac by the maintainer, with the commands in CLAUDE.md. No test downloads a model.

## Stage 2: bundle, signing and release scripts

**I2.1 `bundle.sh`:** `CM_ARCH=arm64|x86_64` (default the host) selects the Rust target, the FFmpeg, `llama-server` and sherpa folders, and the output name; the app is assembled, signed and hardened as today with only that architecture. `make bundle-dev` keeps working on the host architecture so the dev loop stays fast. Cross-building the Intel app on the Apple Silicon Mac is the release path.
**I2.2 `verify-release.sh`:** take the expected architecture as an argument (default arm64, so 0.2.0 behaviour is unchanged) and fail on any other. Keep the signature, entitlements, licence and minimum-macOS checks.
**I2.3 DMG, cask and website:** the Intel DMG is `CaptureMann-vX.Y.Z-macos-x86_64.dmg` with its own `.sha256`; update `Makefile` (`DMG` per architecture, `make release` takes `CM_ARCH`), `dmg.sh`, the README download lines, `docs/development/releasing.md`, `packaging/macos/README.md` and the Homebrew cask (`craft-mann/homebrew-tap`, one `url` and `sha256` per architecture). The site (`capturemann-site`) gets two download buttons, the "About This Mac" hint and system requirements; that repo is handled separately.
**I2.4 Size:** record the app and DMG size of each architecture in `BUILD-INFO` and the release notes. The arm64 DMG must not grow beyond what the 0.2.0 content requires (nothing from the x86_64 build may leak into it); `verify-release.sh` already checks the architecture, add a check that the arm64 app contains no `x86_64` folder.
**I2.5 `release.yml`:** build both DMGs as two jobs (matrix on `CM_ARCH`). Cross-build the Intel app on the arm64 runner; use an Intel runner only to run smoke tests if one still exists (check which hosted Intel runner images are available when this is built). The workflow stays unsigned without secrets, as today. Also make it manual-only for releases made by hand (the open item from 0.2.0).

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

- Two DMGs: every Mach-O in each app matches that DMG's architecture, signed, hardened, notarized and stapled; `verify-release.sh` passes for both. The arm64 DMG is no larger than it needs to be (compare with 0.2.0's 86.6 MB).
- On the Intel Mac, as reported: install from the DMG, record screen, camera, microphone and system audio, edit, export, transcribe, label speakers, write a report, translate.
- Defaults on Intel are chosen from I3.1 measurements, and the numbers are stored in the report.
- Apple Silicon behaviour is unchanged: the 0.2.0 checks still pass on the Apple Silicon Mac.
- `cargo fmt`, `clippy -D warnings`, workspace tests, `cm-i18n check` and `cargo deny` pass; no GPL or AGPL in either architecture.

## Risks

- **Slow inference on older Intel Macs.** Mitigation: Stage 3 defaults and time estimates; the app still works, only slower.
- **Metal on old Intel or AMD GPUs** can misbehave in ggml. Mitigation: CPU fallback (I1.2, I1.4) tested on the actual GPU.
- **No x86_64 sherpa-onnx static library** published. Mitigation: build it ourselves; adds a few days.
- **Only one Intel Mac** (once found) means one GPU and one CPU generation tested. State exactly which Mac in the report and in the system requirements.
- **Wrong download.** An Intel user installing the arm64 DMG gets a clear macOS error, and the reverse may run slowly under Rosetta. Mitigation: the website hint, the README, and the file names.
- **No Intel Mac yet.** Stages 1 and 2 can be built and smoke-tested under Rosetta, but nothing is called working on Intel until the real Mac has run Stage 0 and the acceptance list. The release waits for it.
- **Rosetta hides real problems.** Mitigation: acceptance only on the real Mac.

## Not in 0.2.1

Windows, Linux, VOC-3 live transcript, Ollama and LM Studio (backlog).
