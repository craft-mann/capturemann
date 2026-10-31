# L1-vp9-export-linux report

- **Date:** 2026-10-31
- **Who:** Claude (agent), run by script in the Linux VM; not a person at a keyboard
- **Machine:** Ubuntu 24.04 arm64 VM (QEMU on a Mac), Linux 6.8.0-147, gcc 13.3.0, no `/dev/dri` (no render node, no GPU), software GPU adapter for the compositor
- **Build:** branch `worktree-agent-a8c9f2d04a9f5edc3`, debug, `cargo test --no-default-features` (sherpa libraries are macOS-only)
- **FFmpeg:** the bundled LGPL build from `packaging/ffmpeg/build-linux.sh` (FFmpeg 9.0.2, libvpx 1.17.0, libopus 1.6.1, VAAPI on, built in the VM), and the distribution's `/usr/bin/ffmpeg` (Ubuntu, GPL, has libx264, h264_nvenc, h264_vaapi, libvpx-vp9)
- **Fixture:** VP9 in fragmented MP4, 640x360 at 30 fps, 3 s, made with `testsrc` + `libvpx-vp9 -movflags frag_keyframe+empty_moov` by the distribution's ffmpeg (the bundled build has no `lavfi`). `make_fixture` was not run.

This report covers the FFmpeg build, the VP9 decode and the exporter's choice. It does not cover the recorder, A/V sync, VAAPI encoding, or a real GPU.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1. `packaging/ffmpeg/build-linux.sh` with `vp9` in `DECODERS` and `REQUIRED_DECODERS` (`CM_FFMPEG_JOBS=2`) | PASS | Self-check line: "LGPL configuration, required components present, expected libraries only, aarch64". `ffmpeg -L` prints the LGPL text. |
| 2. `ffmpeg -decoders` of the bundled build | PASS | `h264`, `hevc` and `vp9` ("Google VP9", the FFmpeg-native decoder, no libvpx decoder) are listed. |
| 3. `ffmpeg -encoders` of the bundled build | PASS | `libvpx-vp9`, `h264_vaapi`, `hevc_vaapi` listed; no `libx264`. |
| 4. `cargo test -p cm-media --test vp9_export` with `CM_FFMPEG=<bundled>` | PASS | Reads the VP9 MP4, exports MP4 through the compositor: encoder `libvpx-vp9`, output codec `vp9`, 27,565 bytes, duration within 0.3 s of 3 s, `settings.video` = `{codec: vp9, encoder: libvpx-vp9, software: true, lgpl_safe: true, crf: 32}`. |
| 5. Same test with the distribution's GPL ffmpeg (has libx264 and an unusable h264_vaapi) | PASS | Encoder `libvpx-vp9`, not `libx264`. `h264_vaapi` is skipped because there is no render node. |
| 6. Same test with `CM_EXPORT_ALLOW_GPL=1` (developer override) | PASS | Encoder `libx264`, `lgpl_safe: false`, as before this change. |
| 7. Whole `cm-media` and `cm-core` suites in the VM with `CM_EXPORT_ALLOW_GPL=1` | PASS | cm-core 117, cm-media lib 124, quick_export 45 (3 ignored), other test files 5+4+4+5, vp9_export 1. Without the override about a dozen `quick_export` tests fail on Linux because they assert H.264 output; expected under the new Linux policy. |
| 8. Same suites on macOS (no override) | PASS | Unchanged behaviour: VideoToolbox / libx264 choice. |

## Acceptance criteria

No spec acceptance criterion is claimed. Related decisions: D-R2 (software VP9 in MP4 as the Linux fallback), D-R4 (VP9 decoder in the bundled FFmpeg), NFR-12 (no GPL in shipped binaries).

## Problems found

- `h264_vaapi` was never run: the VM has no render node. The argument list (`-vaapi_device`, `format=nv12,hwupload`, no `-pix_fmt`) is only checked as a string in a unit test. It must be run on an Intel or AMD machine.
- x86_64 build of `build-linux.sh` not run.
- The quick-export tests that assert H.264 need `CM_EXPORT_ALLOW_GPL=1` on Linux.
