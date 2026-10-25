# The FFmpeg that ships inside CaptureMann

`make ffmpeg` (`packaging/ffmpeg/build-macos.sh`) builds `ffmpeg` and `ffprobe` from pinned
upstream sources so that a user installs nothing: `make bundle` copies them into
`CaptureMann.app/Contents/MacOS`, signs them with the app's identity and puts the licence texts
and the source offer in `Contents/Resources/FFmpeg/`. `cm-media` looks for `ffmpeg` next to its own
executable before `PATH`, so a Homebrew FFmpeg never changes how an installed app behaves.

## What is in it

| Piece | Version | Licence | How it is used |
| --- | --- | --- | --- |
| FFmpeg | 9.0.2 | LGPL-2.1-or-later (`--disable-gpl --disable-nonfree --disable-version3`) | the two executables; separate programs, not linked into the app |
| libvpx | 1.17.0 | BSD-3-Clause + WebM patent grant | static, VP9 only, for WebM (EXP-3) |
| libopus | 1.6.1 | BSD-3-Clause | static, Opus audio for WebM |
| VideoToolbox | macOS | part of the OS | H.264 (and HEVC) encoding and decoding |

Not enabled: `libx264`, `libx265`, `fdk-aac` or any other GPL or nonfree component, networking,
capture devices, `ffplay`. H.264 encoding is Apple's; AAC is FFmpeg's own LGPL
encoder; `bundle.sh` refuses any FFmpeg whose build configuration says otherwise.

The component list (encoders, decoders, muxers, demuxers, parsers, filters, protocols) is the
minimum the product calls, taken from every `ffmpeg` and `ffprobe` invocation in `cm-media`,
`cm-ai` and `apps/desktop`; the build script ends with a self-check that fails if a required
encoder, decoder or filter is missing, or if anything but system libraries is linked. When a new
feature needs another component, add it to the lists at the top of `build-macos.sh`.

## Building

```sh
make ffmpeg          # about 5 to 10 minutes on Apple Silicon; output in packaging/ffmpeg/out
make bundle          # bundles it automatically (CM_NO_FFMPEG=1 skips it)
make bundle-release  # same, but fails unless FFmpeg and its licence files are bundled
```

Needs the Xcode command line tools, `curl`, `xz` and `shasum`; nothing is installed on the machine.
FFmpeg's `configure` wants `pkg-config`, so the script builds `pkgconf` from a pinned tarball in
its scratch folder (not shipped). Source tarballs are cached in `packaging/ffmpeg/.downloads` and
checked against the SHA-256 values in the script before each build. Everything under
`packaging/ffmpeg/out` and `.downloads` is git-ignored. `BUILD-INFO.txt` next to the binaries
records the versions, hashes, the exact `configure` line and the linked libraries.

**Architecture.** arm64 only, deployment target macOS 13. A universal binary needs a second build
for x86_64 (FFmpeg and libvpx use `nasm` for x86 assembly, which this script does not install) and
`lipo -create` of the two `ffmpeg` and the two `ffprobe`. Windows is not built (the Windows plan is
a separate FFmpeg with Media Foundation H.264 and the same libvpx and libopus).

## Where the pinned hashes come from

| File | Cross-check | Signature |
| --- | --- | --- |
| `ffmpeg-9.0.2.tar.xz` | equals the SHA-256 in Homebrew's formula (Oct 22, 2026) | the upstream `.asc` exists; **not verified**, `gpg` is not installed on the build machine |
| `libvpx-1.17.0.tar.gz` (GitHub release tag) | equals the SHA-256 in Homebrew's formula | none published |
| `opus-1.6.1.tar.gz` | equals Xiph's own `SHA256SUMS.txt` and Homebrew's formula | none checked |
| `pkgconf-2.5.1.tar.xz` (build tool) | recorded from the official distfile; no second source checked | not verified |

Before a public release, verify the FFmpeg `.asc` against the FFmpeg release signing key
(`gpg --verify ffmpeg-9.0.2.tar.xz.asc ffmpeg-9.0.2.tar.xz`) and note it here.

## What was verified, and how (Oct 23, 2026, one Apple M4 Pro Mac, macOS 26)

* **Reproducible from scratch.** `build-macos.sh` was run again in an empty scratch folder with
  the cached tarballs; every checksum matched, the self-check passed, and `ffmpeg` (8.7 MB) and
  `ffprobe` (8.5 MB) came out. `-buildconf` shows `--disable-gpl --disable-nonfree --disable-version3`
  and only `libvpx` and `libopus`; `ffmpeg -L` says LGPL; the only video encoders are
  `h264_videotoolbox` and `hevc_videotoolbox` (the "x264" strings in the binary are the H.264
  decoder's SEI parsing, not an encoder); `otool -L` lists only `/usr/lib` and `/System/Library`
  libraries; minimum macOS 13.0, arm64.
* **The whole test suite with the bundled build doing the work.** `with-bundled-ffmpeg.sh cargo test
  --workspace --locked --no-fail-fast`: 1,933 tests passed, none failed (35 ignored: the hardware tests).
  In that run the bundled build served 599 `ffmpeg` and 253 `ffprobe` calls (every export, every
  decode the product makes, the Whisper audio decode, the preview decoder, the parity test's export
  side); Homebrew's `ffmpeg` served 213 calls that only make test fixtures (`lavfi`, `libx264`),
  extract a test frame (`select=`) or decode a WebM the export wrote (the product encodes VP9 and
  Opus but never decodes them, so the shipped build has no decoder for either).
* **Every preset through the CLI**, with `PATH` stripped to `/usr/bin:/bin` and `CM_FFMPEG`
  pointing at the build, on a 30 s fixture with a cut, a 2x segment and burned-in captions:
  `youtube-1080p`, `youtube-1440p`, `vertical-1080`, `square-1080`, `source`, `quick`, GIF and WebM
  all finished with the expected frame sizes, codecs (H.264 + AAC, VP9 + Opus, GIF), duration
  (23.8 s) and loudness (-14.0 and -16.0 LUFS measured by an independent FFmpeg); `youtube-4k` was
  refused for the 720p recording, as designed. Sizes and durations are identical to the same
  exports made by Homebrew's FFmpeg, and the time is the same within noise (for example
  `youtube-1080p` 12.0 s against 12.2 s through a debug-build daemon).
* **Real recordings decode.** The bundled `ffmpeg` decodes a real 3024x1964 HEVC screen file
  (hardware and software), a 1080p H.264 camera file and a `pcm_f32le` WAV microphone file, and the
  ignored Whisper test (`small` model) transcribes the real microphone file with only the bundled
  FFmpeg on `PATH`.
* **The app bundle.** `CM_BUNDLE_OUT=<scratch> make bundle` puts `ffmpeg` and `ffprobe` in
  `Contents/MacOS`, signed with the app's identity (`codesign --verify --deep --strict` passes), and
  the licence texts, `SOURCE-OFFER.txt` and `BUILD-INFO.txt` in `Contents/Resources/FFmpeg`. With
  `CM_HARDENED=1` the same checks pass, and the hardened `ffmpeg` still encodes with VideoToolbox,
  `libvpx-vp9` and `libopus`.
* **The lookup order.** A daemon started from inside that bundle, with a decoy `ffmpeg` first on
  `PATH` that logs every call, made no decoy calls while it exported MP4, WebM and GIF files: the
  copy next to the executable wins over `PATH`.

**Not verified:** a Mac that has never had Homebrew (here `PATH` was stripped, but the files exist
on disk), the FFmpeg `.asc` signature (no `gpg` here), an x86_64 or universal build, Windows,
notarization, and long recordings (the longest export was 30 s of 720p).

## Updating a version

Change the version, URL and SHA-256 at the top of `build-macos.sh` (take the hash from the
official site and compare it with a second source), run `make ffmpeg`, run the export tests with
`CM_FFMPEG=packaging/ffmpeg/out/ffmpeg` (see `tests/manual/M4-GATE.md`), and update the tables
above.

## Replacing FFmpeg (LGPL)

FFmpeg is a separate executable. Users can build their own and either replace
`Contents/MacOS/ffmpeg` and `ffprobe` in the app or set `CM_FFMPEG` to their executable (or to the
folder that holds it); `CM_FFMPEG` wins over the bundled copy. `SOURCE-OFFER.txt` in the bundle
names the exact sources and the configure line.
