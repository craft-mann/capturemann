# Linux import and export: VA-API H.264 with constant quality, labelled software VP9 fallback (release 0.3.0, task 1, O10)

- Date: Nov 8, 2026. Machine: lab ThinkPad T490, Ubuntu 24.04.5 LTS (kernel 7.0.0-42), Intel Core i7-8665U, **Intel UHD 620, iHD VA-API driver 24.1.0** (`VAProfileH264High : VAEntrypointEncSliceLP`). The runs were started over ssh with no desktop session of their own (`XDG_SESSION_TYPE=tty`): import and export need none.
- Build: branch `track/linux-media` at `95511ca6` (media `da225fcb`, regress `95511ca6`), merged with main `6131cdd2`; debug build, `--no-default-features` (no speaker engine), cm-regress `--dev` (FFmpeg: Ubuntu's `/usr/bin/ffmpeg` 6.1.1, which has `h264_vaapi`, `libvpx-vp9`, `libx264`; the product never picks libx264 on Linux).
- Command (each situation): `cm-regress --dev --package-dir <target>/debug --tier standard --only import,export --keep`. Cases: `import.formats`, `import.rotated`, `import.vfr`, `import.audio`, `import.errors`, `import.dry_run`, `export.quick`, `export.presets`, `export.audio`, `export.errors`. Reports of the lab runs: `report-standard.md` in `~/wt/media-out-{normal3,novaapi,badva}` on the laptop (deleted at the end; the relevant lines are quoted below).
- Root cause fixed: iHD lists and runs `h264_vaapi` but offers constant quality only. With `-b:v` and no rate mode FFmpeg fails ("Driver does not support any RC mode compatible with selected options (supported modes: CQP)"), so every import copy and export fell back to VP9. The media code now asks the recorder's probe (`cm_encode::ffmpeg_vp9::choose_backend`: one real 256x256 encode, bit rate first, then `-rc_mode CQP -qp`, cached per FFmpeg) for the render node and mode.

## Results (10 cases each)

| Situation | How it was made | Result | Encoder of the editing copy / exports | Label in the result |
| --- | --- | --- | --- | --- |
| 1. Normal VA-API | nothing changed | **10 passed**, 0 failed | `import.vfr` stored track: h264; `quick`, `youtube-1080p`, `vertical-1080`, `square-1080`: `h264_vaapi` | none (hardware H.264) |
| 2. VA-API hidden | `ffmpeg` wrapper first on `PATH` that removes `vaapi` lines from `-encoders` (a build without VA-API) | **10 passed** | `import.vfr` stored track: vp9; exports `libvpx-vp9` (`webm` always VP9) | `codec_note`: "This export is VP9 video, not H.264: this FFmpeg has no VA-API H.264 encoder." |
| 3. VA-API device that cannot start | wrapper that rewrites `-vaapi_device` to `/dev/dri/renderD999` (listed, unusable) | **10 passed** | same as 2 | `codec_note`: "This export is VP9 video, not H.264: this computer has no VA-API render node (/dev/dri/renderD*), so it has no hardware H.264 encoder." |

Before the fix (the 0.2.9 report on the same laptop): `export.quick` and `export.presets` gave VP9, and the import copy failed with "no hardware video encoder".

In situations 2 and 3 `import.vfr` first failed with "the editing copy is vp9 not h264" because the test required H.264; the expectation was changed (commit `95511ca6`) to accept VP9 only when the import result labels it (`editing_copy` with `codec: vp9`, `software: true`, a note).

## Bundled FFmpeg (package build)

The freshly built x86_64 package FFmpeg (`wt/pkg/packaging/ffmpeg/out-linux-x86_64/ffmpeg`, FFmpeg 9.0.2, VAAPI on, built by the packaging agent) was run read-only:

- `-encoders` lists `h264_vaapi`, `hevc_vaapi`, `libvpx-vp9`; `-filters` lists `hwupload` and `scale_vaapi`; `ldd` shows `libva.so.2` and `libva-drm.so.2` from the system.
- One-frame probe (256x256 NV12 from `pipe:0`, `format=nv12,hwupload`, `h264_vaapi`) on `/dev/dri/renderD128`: with `-b:v 2000000` it **fails** ("Could not open encoder", exit 234); with `-rc_mode CQP -qp 26` it **works** (exit 0). So the bundled build behaves as Ubuntu's, and the CQP path is the one the product now uses.
- The full cm-regress run was **not** made against the bundled FFmpeg (only the probe).

## Also checked

- Mac (Apple Silicon, macOS): the same cases with `cm-regress --dev --tier standard --only import,export`, built from the same tree: **10 passed** (VideoToolbox H.264, no VP9 label). `cargo test -p cm-media -p cm-daemon -p cm-cli -p cm-regress`, `cargo clippy --workspace --all-targets --locked -D warnings` (apps/desktop `lib test` fails on the Mac with `futures_lite` unresolved at `platform_hotkeys.rs:436`, which comes from main, not from this branch) and `cargo fmt --check`.
- One flake: the first run of situation 1 (while other builds ran on the laptop) failed `import.formats` and the gif part of `export.presets` with `daemon_unavailable` ("the daemon closed the connection" / "Broken pipe"). It did not reproduce in five later full runs, in `--keep` runs, or in situations 2 and 3, and its logs were not kept. Not explained; **not** claimed fixed.

## Not run

- Export or import with the bundled FFmpeg through the whole product (only the probe above).
- AMD or NVIDIA GPUs, a second Intel generation, aarch64; Wayland (`XDG_SESSION_TYPE` was `tty`); the editor playing the VP9 copy by hand; durations over a few seconds (fixtures are 3 to 4 s clips).
- A driver that offers a bit-rate mode (the probe's first branch): tested with fakes only (argument strings).
