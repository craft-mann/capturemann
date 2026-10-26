# Export (EXP)

Exports render master tracks + edit list through the `cm-render` compositor (Technical design → Export and render) into new files under the bundle's `exports/` folder (and optionally a user folder). The master is never changed.

**Build status (Oct 21, 2026).** The compositor (M4) is not built. Until it ships, CaptureMann has an **interim exporter**, the preset `quick` (product-maintainer decision, Oct 21, 2026): after a recording stops, it renders **one combined MP4** (screen + round camera bubble with the 4 px `rec` ring + microphone and system audio) with the FFmpeg sidecar (`cm-media`), saves it as `exports/<title> - quick.mp4`, lists it in the library with Play and Reveal, and leaves the masters untouched. It is deliberately small, and it keeps the interface that the M4 exporter keeps, so nothing above it changes when the compositor replaces the renderer:

| Kept by M4 (stable) | Interim behavior |
| --- | --- |
| MCP `export_video { capture_id, preset?, loudness_lufs?, request_id? }` → `{ job_id }` (Free, mutating), CLI `capturemann export <id> [--preset quick] [--loudness LUFS] [--wait]` | Only `preset: "quick"` (the default). Every other preset (`youtube-1080p`, ...) and the custom `size`/`fps`/`codec`/`quality`/`format`/`captions`/`out_dir` form fail with `invalid_input` "not available yet". |
| Job type `export` (one at a time, held while recording, restarted from the beginning after a restart; EXP-4) | Same. Inputs `{ capture_id, preset, by, auto }`; result `{ capture_id, preset, file, path, size_bytes, duration_s, duration_ms, encoder }`. |
| Manifest `exports[]` entry `{ file, preset, created, duration_ms, size_bytes, encoder, settings }`, one per preset (an export replaces only its own preset's entry), file `{title} - {preset}.mp4` | `settings` records what was used: size, fps, encoder (`h264_videotoolbox` or `libx264`), bitrate or CRF, audio layout, the track offsets, the bubble layout, notes. |
| Events `export.queued`, `export.done`, `capture.revised`, `job.progress`, `job.done`, `job.failed` | Same. |
| Setting `export.auto_on_stop` (default true) | The daemon enqueues the job when a recording stops normally (not on discard or the 60 fps self-test), after the job holds are released, so CLI and agent recordings are exported too. |

**How `quick` renders.** One FFmpeg run. The video starts at the recording's `live_at` (the manifest value, the moment every track was delivering; the screen track's first frame when the manifest has none or it lies outside the screen track, so the front of the screen file is trimmed by `live_at` minus that first frame, usually ~190 ms, up to about 3 s on a slow machine); the camera, when it starts after that origin, is held on its first frame so the bubble is present from frame 1, and when it starts before it is trimmed; the microphone (usually ~290 ms before the screen) is trimmed to the origin and the system audio is delayed or trimmed the same way (offsets from the manifest `start` times with the pause gaps removed, so every track stays on the session clock). The camera is cover-cropped to a square (circle) or 4:3 (rounded), mirrored when `mirror` is true (the stored track is never flipped, CAM-2.AC3), masked, ringed in `#FF4F3A` (4 px of a 240 px medium bubble, as a fraction of the bubble width) and placed at the layout rect. Output: H.264 High, yuv420p, constant frame rate equal to the screen fps, the screen master's size (even, capped to 4096×2304 for H.264 level 5.2), AAC-LC 48 kHz (128 kbps mono, 192 kbps stereo), `+faststart`, written through a temporary name and renamed. Encoder: `h264_videotoolbox` where FFmpeg has it (about 12 Mbps at 1080p30, scaled by area and fps), else `libx264` (CRF 20, preset `veryfast`); a failed hardware encode retries once in software.

**Known gaps of the interim exporter** (each goes away with the compositor, EXP-1/EXP-2):
- **No loudness normalization** (−14 LUFS is for the platform presets, EXP-2); the mic and system audio are summed (`amix`, `normalize=0`) with a peak limiter.
- **The bubble layout is the one at stop** (`edits[0].layout.camera`, `{ visible, shape, rect, mirror, size, ring, radius }`, rect as fractions of the recorded area). The engine does not record bubble moves *during* the recording (the bubble is a window of the desktop app), so a bubble dragged halfway appears at its final place for the whole video; a bubble hidden with the HUD button is not recorded either. Changes made through the `camera.*` settings during the recording are picked up; the request's own values are kept otherwise.
- **The edit list is ignored** (no trims, cuts, speed, zoom, cursor redraw, captions or background), and there is no stream-copy fast path.
- **The FFmpeg sidecar ships in the app** (the LGPL build of `make ffmpeg`, `Contents/MacOS/ffmpeg`; NFR-12) and is found in this order: `CM_FFMPEG`, next to the executable (`Contents/MacOS/ffmpeg`), `PATH`, then `/opt/homebrew/bin` and `/usr/local/bin`. Without it the job fails once (no retry) with `not_found` "FFmpeg was not found: install it with Homebrew (brew install ffmpeg) ..." in a development build, or, in a packaged app whose own copy is missing or cannot start, "FFmpeg was not found: the copy that ships inside CaptureMann is missing ... Reinstall CaptureMann"; the recording is unaffected and the Export button retries. **The automatic export is skipped when no FFmpeg is found**: instead of queuing a job that is sure to fail, the daemon publishes `export.skipped { capture_id, title, reason: "ffmpeg_not_found", message }` once per run, which the app shows as a hint. **Typed code (Oct 24, 2026):** a missing FFmpeg has stable ids in `cm_core::message`, all with code `not_found` and unchanged English: `ffmpeg_not_found` (no copy on hand), `ffmpeg_bundle_missing` (the copy in the app is gone), `ffmpeg_bundle_cannot_run {path, error}`, `ffmpeg_env_invalid {path}` (`CM_FFMPEG` is not a file) and `ffmpeg_permission_denied {path}`. The job queue still carries the failure as `job_failed` with the English reason, and the daemon adds `error.data.reason_code` (the id) to `job.failed`, so MCP, the CLI and JSON-RPC keep their shape and gain one field; the app words the failure from the id (the damaged-app case says to reinstall or set `CM_FFMPEG`) and keeps matching the old prefix for an older daemon.
- **Licensing (NFR-12):** the FFmpeg we **bundle** is an **LGPL build without `libx264`** (`packaging/ffmpeg/build-macos.sh`) (VideoToolbox on macOS, Media Foundation on Windows; no software H.264 encoder ships in v0.1). **Encoder order** (first one the FFmpeg offers wins): `h264_videotoolbox`, `h264_mf`, `libopenh264` (only if a developer's own FFmpeg has it; the app ships none), `h264_nvenc`, `h264_qsv`, `h264_amf`, and `libx264` **last**. `libx264` is GPL: it is only used when a developer's own GPL build is on `PATH`, it is flagged `lgpl_safe: false` in the export entry (`settings.video.lgpl_safe`), and a shipping build can refuse it. Each encoder gets its own arguments (only `libx264` uses a CRF). The output is capped at 4096 px per side and 4096×2304 area (H.264 level 5.2); a larger master is scaled down proportionally. The job **retries with the next encoder only when the encoder fails to initialize**; any other FFmpeg failure fails the job once. A re-export after a title rename (the file name is `{title} - quick.mp4`) deletes the previous quick file, only inside `exports/`.
- **Bundling (NFR-12, NFR-15): built, Oct 22, 2026.** `make ffmpeg` builds `ffmpeg` and `ffprobe` (FFmpeg 9.0.2 with libvpx 1.17.0 and libopus 1.6.1 linked statically; LGPL v2.1 or later, no GPL or nonfree component; about 8.7 MB each; arm64, macOS 13 or later) from pinned source tarballs, and `make bundle` copies them into `Contents/MacOS`, signs both with the app's identity and puts the licence texts, `SOURCE-OFFER.txt` and `BUILD-INFO.txt` in `Contents/Resources/FFmpeg/`; `make bundle-release` fails without them. Every preset, GIF, WebM, the loudness option and the edit-list export were run with this build on the developer's Mac (CLI and daemon end to end, plus the `cm-media`, `cm-daemon`, `cm-ai` and parity tests; a stored record is in `verification/`). **Not done:** a signature check of the FFmpeg tarball with `gpg`, a run on a clean Mac without Homebrew, an x86_64 or universal build, and the Windows build.
- macOS only (VideoToolbox); Windows uses `libx264` only if the user has installed FFmpeg.

### EXP-1 Export MP4 at source resolution or a preset
**Priority** P0 · **Status** In progress (interim `quick` preset built on macOS: screen + camera bubble + audio; since Oct 22, 2026 it honours the edit list (trim, cuts, speed, background) through the compositor, tested with FFmpeg and a GPU on this Mac; the other presets, the Source fast path, loudness and AUD-4 gains are M4) · **Crates** cm-render, cm-encode, cm-media · **Depends on** EDT-1, AUD-4

*Captions (AI-1, Oct 22, 2026):* `export_video` takes `captions: "burn" | "none"` (omitted: the recording's `burn_in` switch). Burning uses the compositor path whatever the preset (GIF and WebM included): the caption lines on the output timeline are drawn over the finished frame, inside the preset's safe area. The choice is part of the job's identity, and the job result says `captions: "burned" | "off" | "skipped_no_transcript"`. Tested with FFmpeg and a GPU on this Mac.

**User story** — As a creator, I want to export my edited recording as an MP4 at full quality, so that I can upload it anywhere.

**Description**
- Export sheet: preset (EXP-2) or custom (size: source/2160p/1440p/1080p/720p; fps: source/30/60; codec: H.264/HEVC; quality: High/Balanced/Small), captions (none/SRT file/burn in), audio loudness normalization (on for platform presets), destination (bundle only, or also a folder).
- Container: MP4, fast start (moov first), AAC-LC 48 kHz (mono 128 kbps or stereo 384 kbps, per YouTube's recommendation).
- Video: hardware encoder; H.264 High profile, closed GOP of half the frame rate, 2 B-frames (YouTube recommendation); HEVC Main / Main10 when the master is 10-bit.
- **Fast path**: when the edit list has only trims and no layout, zoom, cursor, speed or caption changes, and the preset matches the master codec, re-mux without re-encoding ("Source" preset).
- Output file name: `{title} - {preset}.mp4`; repeated exports overwrite only the same preset's previous file after confirmation.

**Acceptance criteria**
- **AC1** Given a 4K30 HEVC master with trims only and the Source preset, then export takes ≤ 10% of the video's duration and the output is bit-identical in the kept range (stream copy).
- **AC2** Given a 10-minute 4K30 recording with a bubble and zooms and the YouTube 4K preset, then export finishes at real time or faster on Apple Silicon M-series / NVENC (≤ 10 min).
- **AC3** Given any export, then the output passes `ffprobe` checks: fast start, AAC 48 kHz, expected size, fps and duration (± 1 frame).
- **AC4** Given the export, then a frame-by-frame comparison against the editor preview at 5 timestamps differs by ≤ 1% (PSNR ≥ 40 dB).
- **AC5** (interim `quick`) Given a recording stops normally and `export.auto_on_stop` is on (the default), then one `export` job (`preset: "quick"`, `auto: true`) is queued after the job holds are released and renders `exports/<title> - quick.mp4`; the manifest `exports[]` lists it with the encoder used; a discard, a cancelled countdown, the 60 fps self-test or the setting off enqueue nothing.
- **AC6** (interim `quick`) Given a fixture with the screen starting at 390 ms, the camera 190 ms later and the microphone 290 ms earlier, then in the output a screen event and an audio event that coincide on the session clock are within 50 ms of each other (measured: 5 ms), and the camera changes at the screen-relative time (± 2 frames). With a manifest `live_at` the same holds relative to `live_at` (the export is `live_at` minus the screen start shorter; tested with 0.19 s and a 2.5 s pre-roll).
- **AC7** (interim `quick`) Given `layout.camera` (circle, mirror on), then the output shows the camera inside a circle at the layout rect with a 4 px `#FF4F3A` ring, the screen shows through outside the circle, mirror flips the camera, and a recording without a camera (or with `visible: false`) exports screen and audio only.
- **AC8** (interim `quick`) Given FFmpeg is missing, then the job fails once (no retry) with `not_found` and the install hint, nothing is written to `exports/`, and the recording is unaffected.

**Edge cases and errors** — Disk full during export → stop, delete the partial file, `disk_full`. Encoder session limit reached (consumer NVIDIA GPUs) → queue the export until a session frees.

**Agent interface** — MCP `export_video { capture_id, preset? | { size, fps, codec, quality }, captions?: "none"|"srt"|"burn", loudness_lufs?, out_dir? }` → `{ job_id }` → `{ path, size_bytes, duration_s }` (Free). CLI `capturemann export <id> --preset youtube-4k [--out dir]`. Built: `export_video { capture_id, preset?, format?, width?, fps?, max_mb?, loudness_lufs?, request_id? }` and `capturemann export <id> [--preset ID] [--format gif|webm] [--width PX] [--fps N] [--max-mb MB] [--loudness LUFS] [--wait]` (`preset` is `quick` by default or a row of the preset table, EXP-2 and EXP-3 below; `loudness_lufs` is -40 to -5, defaults to the preset's, and with the preset and the GIF/WebM options is part of the job: a different value is a different job; `--wait` blocks and exits 6 if the job fails, 2 for a missing FFmpeg). Not built, refused with `invalid_input`: the custom `size`, `codec`, `quality`, `captions` and `out_dir`.

**Out of scope** — ProRes/DNxHR exports (P2), image-sequence export.

**Test notes** — ffprobe validation in CI; PSNR preview-vs-export test; performance test on reference machines. Interim `quick`: `cm-media` unit tests (filter graph, offset math, bubble geometry) and FFmpeg-gated integration tests on `lavfi` fixtures (`crates/cm-media/tests/quick_export.rs`, `crates/cm-daemon/tests/export_real.rs`: ffprobe checks, A/V alignment, ring and circle pixels, mirror, camera-less, cancel, missing FFmpeg), daemon tests with a stub exporter (`crates/cm-daemon/tests/export.rs`: auto-export, hold order, settings, idempotency). CI installs FFmpeg on both runners (`brew install ffmpeg`, `choco install ffmpeg`), so the FFmpeg-gated tests (AC3 and AC6 to AC8) run in CI instead of printing SKIPPED; before this change they passed vacuously there.

---

### EXP-2 Platform presets
**Priority** P0 · **Status** In progress (built on macOS, tested with FFmpeg and a GPU on this Mac, not on CI or Windows: the six presets of the table with their frames, bitrates and -14 LUFS audio, the upscale refusal, the bubble placed on the picture and out of the vertical safe area, the preset table as data with a replacement file; not built: the EDT-5 reframe (the screen is fitted and centered), the Source stream-copy fast path, `get_platform_rules` and the publish sheet's warnings, which need `cm-publish`) · **Crates** cm-media, cm-render, cm-daemon (the rules file will move to cm-publish) · **Depends on** EXP-1, EDT-5

**User story** — As a creator, I want one-click presets for YouTube, Shorts, TikTok and Reels, so that I don't need to know codecs and bitrates.

**Description**

| Preset id | Frame | fps | Video | Notes |
| --- | --- | --- | --- | --- |
| `youtube-4k` | 3840×2160 | source (30/60) | H.264 High, 35–45 Mbps (30) / 53–68 Mbps (60) | Upscaling from < 2160p is refused with a hint to use `youtube-1440p` or `youtube-1080p` |
| `youtube-1440p` | 2560×1440 | source | H.264, 16 / 24 Mbps | |
| `youtube-1080p` | 1920×1080 | source | H.264, 8 / 12 Mbps | |
| `vertical-1080` | 1080×1920 | source | H.264, 12 Mbps | Shorts, TikTok, Reels; uses EDT-5 reframe; safe area: keep captions and bubble out of the bottom 20% and right 12% (platform UI) |
| `square-1080` | 1080×1080 | source | H.264, 8 Mbps | Feeds |
| `source` | master size | master | stream copy when possible | EXP-1 fast path |

- Presets live in a versioned data file (`cm-publish/platform_rules.toml`) with each platform's limits (TikTok: ≤ 4,096 px per side, 23–60 fps, ≤ 4 GB, ≤ 10 min via API), so the app can update rules without code changes.
- Selecting a platform in the publish sheet (PUB-5) chooses its preset automatically.

**Acceptance criteria**
- **AC1** Given `vertical-1080`, then the output is 1080×1920 with the EDT-5 reframe applied.
- **AC2** Given a 12-minute video and the TikTok target, then the sheet warns "TikTok accepts up to 10 minutes through its API" before exporting.
- **AC3** Given a 1080p master and `youtube-4k`, then the preset is disabled with the hint.
- **AC4** Given an updated `platform_rules.toml`, then new limits apply without rebuilding the app.

**Edge cases and errors** — Odd-sized masters → pad to even dimensions with the background color.

**Agent interface** — `export_video { preset: "<id>" }`; MCP `get_platform_rules { platform }` returns limits and recommended preset.

*As built (Oct 22, 2026).* `export_video { preset }` takes any id of the table; `quick` stays the default and is not in the table. The table is `crates/cm-media/data/presets.toml` (version 1, embedded in the binary; the spec's `cm-publish/platform_rules.toml` moves there when `cm-publish` exists). A file named by the environment variable `CAPTUREMANN_PLATFORM_RULES` replaces it the next time an export starts, without rebuilding (AC4); a file that does not parse is ignored with a warning, never an error. `get_platform_rules` and the per-platform limits (TikTok's 10 minutes, 4 GB) are not built: they belong to `cm-publish`.

| Preset | Frame | fps | Video | Audio |
| --- | --- | --- | --- | --- |
| `youtube-4k` | 3840×2160 | source | 40 Mbps (30) / 60 Mbps (60) | AAC, -14 LUFS |
| `youtube-1440p` | 2560×1440 | source | 16 / 24 Mbps | AAC, -14 LUFS |
| `youtube-1080p` | 1920×1080 | source | 8 / 12 Mbps | AAC, -14 LUFS |
| `vertical-1080` | 1080×1920 | source | 12 Mbps | AAC, -14 LUFS |
| `square-1080` | 1080×1080 | source | 8 Mbps | AAC, -14 LUFS |
| `source` | master size (capped for H.264) | source | the encoder's size-based default | untouched level |

The bitrates are inside YouTube's recommended ranges (35–45 and 53–68 Mbps at 4K). The recording is fitted into the frame by width (or height) and centered on the edit list's background (the frame is the compositor's output size; padding from EDT-3 applies inside it: a fraction of the width at the sides and of the height above and below): that is the hook for EDT-5 (`preset::Reframe::Crop` takes priority when set). `loudness_lufs` in the request overrides the preset's. **AC1** holds for the size (1080×1920, tested) but not yet for "the EDT-5 reframe applied". **AC3** is checked when the request arrives (`export_video` fails with `invalid_input` before a job exists): `youtube-4k` for a recording that would be enlarged (any master below 3840×2160 once fitted) says which preset fits, for example "`youtube-4k` would enlarge this 1920x1080 recording to 3840x2160; use `youtube-1080p` instead"; the other presets do not refuse an enlargement. In `vertical-1080` the camera bubble is placed on the picture the way the layout places it on the recording, then moved out of the bottom 20% and the right 12% of the frame (the safe area is in the table; tested on pixels). **AC2** (the sheet's TikTok warning) and the odd-size padding (every preset frame is even; the master is cropped by a pixel when odd, as for `quick`) are not built.

**Out of scope** — LinkedIn and X presets (P2 with PUB-7).

**Test notes** — ffprobe checks per preset; rules-file loading unit tests.

---

### EXP-3 GIF and WebM export
**Priority** P1 · **Status** In progress (built on macOS, tested with FFmpeg and a GPU on this Mac, not on CI or Windows: GIF and WebM through the compositor, the size limit, the 30 s limit; not built: the estimated size, the optional `gifski` component, a browser playback check of AC3) · **Crates** cm-media, cm-render · **Depends on** EXP-1

**User story** — As a developer, I want a short GIF or WebM of a UI interaction, so that I can drop it into a pull request or docs page.

**Description**
- GIF: width 480/640/800/1024 px, fps 10/15/24, max 30 s (warning above 15 s); palette quantization in our own code or the optional `gifski` component (AGPL, not bundled — downloaded on request with a license notice). Loop forever by default.
- WebM: VP9 at the chosen size via LGPL FFmpeg (`libvpx` is BSD), Opus audio optional.
- Target size hint: shows estimated file size; "Fit under 10 MB" adjusts width/fps automatically.

**Acceptance criteria**
- **AC1** Given a 10 s clip at 800 px 15 fps, then a looping GIF of 150 ± 1 frames is produced.
- **AC2** Given "Fit under 10 MB", then the result is ≤ 10 MB.
- **AC3** Given WebM, then the file plays in Chrome and Firefox.

**Edge cases and errors** — GIF longer than 30 s → refused with "Use MP4 or WebM for clips longer than 30 s".

**Agent interface** — `export_video { format: "gif"|"webm", width?, fps?, max_mb? }`.

*As built (Oct 22, 2026).* `format: "gif"` and `"webm"` are the presets of those names (`preset` and `format` must agree); the same goes for `capturemann export --format gif --width 640 --fps 10 --max-mb 10`. The edit list, the background and the bubble apply, as for MP4.
- **GIF** (`gif`): the frame has the recording's aspect; width 480, 640, 800 (default) or 1024, fps 10, 15 (default) or 24; no audio. Two steps so a size retry does not compose again: the compositor writes a lossless FFV1 file, then FFmpeg makes one palette for the whole clip (`palettegen stats_mode=diff`) and applies it (`paletteuse`, Bayer dither, rectangle diffing) into a GIF that loops forever (`NETSCAPE2.0`, loop 0). The palette is our own use of FFmpeg's native GIF encoder; `gifski` (AGPL) is not used. A clip over 15 s gets a note in the export settings, over 30 s is refused at once with "Use MP4 or WebM for clips longer than 30 s".
- **WebM** (`webm`): VP9 (`libvpx-vp9`, BSD) and Opus (`libopus`, BSD) in WebM; any even width (default 1280, never above the recording's width), fps default 30 (never above the recording's), constant quality (CRF 32) without a limit; Opus 96 kbit/s stereo or 64 mono; the audio mix is the same as for MP4. Both encoders are built into the FFmpeg that ships in the app (`libvpx-vp9` and `libopus`, static, BSD); an FFmpeg without them (for example a Homebrew build lacking them) fails the export with `platform_rejected` and a hint (install an FFmpeg with libvpx and libopus, or export MP4); GIF needs only native encoders.
- **"Fit under N MB"** (`max_mb`, 10^6 bytes, 0.01 to 1000): a GIF steps the width down the allowed list and then the frame rate (24, 15, 10) until the file fits, and fails with "The smallest GIF of this clip is X MB, over the Y MB limit; use WebM or a shorter clip" when it cannot; a WebM is re-encoded at a bitrate computed from the limit and lowered (up to three times, from the size it came out at) and, if it is still over, is kept with a note. Tested on a 4 s fixture; long clips are not measured.
- **AC1** (150 ± 1 frames, looping) and **AC2** (≤ the limit) are tested for GIF; **AC2** for WebM too. **AC3** (plays in Chrome and Firefox) is checked with ffprobe only (VP9 + Opus in a `webm` container); nobody has played the file in a browser.

**Out of scope** — APNG, animated WebP.

**Test notes** — Frame counts and size limits in CI.

---

### EXP-4 Background export with progress
**Priority** P0 · **Status** In progress (built on macOS for the `quick` export: persisted `cm-jobs` queue, hold while recording, restart recovery, cancel and delete of the partial file, progress events; AC1 and AC2 UI timing and the editor-driven parts are not built or measured; Windows not run) · **Crates** cm-jobs · **Depends on** EXP-1, AGT-1

**User story** — As a creator, I want exports to run in the background, so that I can keep editing or start recording again.

**Description**
- Exports are persisted jobs (`cm-jobs`): queued → running → done/failed/cancelled; progress %, elapsed, ETA, output size so far.
- One export runs at a time (GPU-bound); others queue. Starting a recording pauses a running export and resumes it after the recording stops. Transcription is held the same way (it is as heavy as an export), and uploads are held unless `storage.pause_uploads_while_recording` is `false` (STO-3). The hold is placed just before the recording's tracks start and released when the recording ends or is discarded, or the start is cancelled; a restart keeps it. Built in `cm-jobs` and the recording engine, tested with fakes.
- A job survives app restart: on launch, interrupted exports restart from the beginning (or resume if the encoder supports segmenting).
- Jobs panel (DESIGN toasts/progress) lists jobs; finished exports show Reveal, Copy path, Upload, Publish.

**Acceptance criteria**
- **AC1** Given an export running, then the editor stays responsive (UI frame time < 16 ms at the 95th percentile).
- **AC2** Given an export running, when the user starts recording, then the export pauses within 1 s and resumes after the recording stops.
- **AC3** Given the app is quit during export, when relaunched, then the export restarts automatically and the user is told. (Built: the job restarts from the beginning and `export.queued` is **re-published** after the restart, with `recovered: true` and `was_interrupted`, so the UI resumes showing it.)
- **AC4** Given cancel, then the partial file is deleted. (A cancel that races a handler that finishes successfully ends the job as `done`, because the output exists; `cancel_job` answers "cancelling" and the transition records "finished before the cancel took effect".)

**Edge cases and errors** — Source bundle moved or deleted while queued → job fails with `not_found`.

**Agent interface** — `export_video` returns `job_id`; `get_job`, `list_jobs`, `cancel_job`; events `job.progress`, `job.done`, `job.failed`; `get_job` reports `held` (true while the job waits for a recording to end), and `export.queued` carries `recovered` and `was_interrupted` after a restart. Built for `quick`: the job type `export` runs through `cm-jobs` (held while recording, AC2 tested with a stub; cancel kills FFmpeg and deletes the partial file, AC4 tested with a real FFmpeg), plus `export.queued` / `export.done` events; the app shows "Recording saved — exporting…" with progress, then "Exported <title>" with Play and Reveal.

**Out of scope** — Distributed or cloud rendering.

**Test notes** — Job queue unit tests; restart integration test.
