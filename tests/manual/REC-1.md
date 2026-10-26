# REC-1 / REC-2 / REC-3 / REC-4 / REC-6 / REC-8 manual script: the screen track (`cm-capture` + `cm-encode`)

Covers the screen recording engine (M3 slice 1). HUD, countdown, camera and audio are other slices.
The ACs that need a person (REC-1.AC4 bubble exclusion, REC-6.AC1/AC2) are at the bottom.

## Setup
- macOS 13+: grant Screen Recording to the app that hosts `cargo` (Terminal, iTerm, VS Code ...) in
  System Settings > Privacy & Security > Screen & System Audio Recording, then quit and reopen it.
- `ffprobe` on the PATH (Homebrew `ffmpeg`) enables the decode checks; without it they are skipped.
- Recordings go to a temp folder and are deleted when each test ends (they contain your screen).
- Windows is not implemented yet: `cm_encode::HardwareEncoder::create` returns the shared
  `not_implemented` error.

## Automated, no display needed (CI)
```
cargo test -p cm-encode -p cm-capture -p cm-library
```
Settings math (REC-2, REC-3), the fMP4 parser and truncation (REC-8), pause-gap math (REC-4),
late-frame accounting (NFR-2), disk guard (NFR-7) and the recorder with a fake source.

## Automated, real hardware, no permission (encoder only)
```
cargo test -p cm-encode --test mac_encode -- --ignored --nocapture --test-threads=1
```
Drives VideoToolbox with synthetic frames. Pass: HEVC and H.264 files have `moov` first, 4 fragments
for 7 s (2, 2, 2, 1 s), every fragment starts on a sync sample, ffprobe counts every frame, and a
file cut in the middle of the last fragment is repaired to the last complete fragment and decodes.

## Automated, real display
```
cargo test -p cm-capture --test record_real -- --ignored --nocapture --test-threads=1
```
Without permission the tests print `SKIPPED: Screen Recording permission ...` and pass.

| Test | AC | Pass when |
| --- | --- | --- |
| `rec1_ac1_records_three_seconds_of_the_real_display` | REC-1.AC1, REC-2.AC1 | file is the display's native size, about 3 s, `is_clean`, ffprobe frame count equals the parser's; prints start and stop latency |
| `rec1_ac3_region_1280x720` | REC-1.AC3 | file is exactly 1280x720 |
| `rec2_1080p_output_has_even_width_and_exact_height` | REC-2.AC3 | height 1080, even width (1662 on a 3024x1964 display) |
| `rec4_pause_resume_real_display` | REC-4.AC1 | 2 s + 3 s paused + 2 s is a 4 s file (+- 0.3 s), one gap in the details |
| `rec6_exclusions_build_a_valid_filter` | REC-6 | a filter with pid, bundle id and an unknown window id starts and records |
| `nfr2_twenty_seconds_at_60fps` | NFR-2, NFR-5, REC-3.AC3, REC-8 | late <= 1%, 1200 +- 2 frames, fragments about 2 s, RSS < 600 MB; prints CPU |
| `rec8_ac1_kill9_then_recover` | REC-8.AC1 | a child process killed with SIGKILL during recording is recovered on the next launch with >= elapsed - 2 s of media and decodes |

NFR-2 is only meaningful with something moving on screen. Static content makes SCK deliver few
frames and the recorder repeats the last one (counted as `repeated`, not late). For a real
measurement play a full-screen video or an animation while the 60 fps test runs and compare with
S6 (0.26 to 0.33% at 3024x1964 and 3840x2160, docs/project/spikes.md). Record the machine, display and content
next to the number.

## Manual checks
1. REC-1.AC4 / REC-6: show the camera bubble or HUD (exclusion by window id and pid; the maintainer also
   sets `sharingType = none`), record 5 s, step through `screen.mp4` (QuickTime, frame by frame):
   neither appears. Repeat with 1Password or Passwords open and its bundle id in the exclusion list:
   its windows are transparent. Add Slack to the list mid-way (new recording): its windows are
   absent, including ones opened after the recording started (REC-6.AC2, app-level exclusion).
   1a. REC-6.AC8, the library window is in the video. Put the CaptureMann library window on top of
   another app (a full-screen terminal), record the whole display for 5 s (HUD, countdown and bubble
   on), stop and step through `screen.mp4`: the library window is in the video; the HUD, the
   countdown, the webcam bubble, the start sheet, any toast and any confirmation dialog are not.
   Then minimize the library window mid-recording: from about 1 to 2 s later it is gone from the
   video; reopen it: it returns within about 2 s. Repeat for a Region that covers part of it.
1b. R3-3, an excluded app that is not running when the recording starts: put a bundle id of an
   app you can launch (for example `com.apple.Passwords`) in `privacy.excluded_apps`, make sure that
   app is quit, start a Display recording, then launch the app after about 3 s, show its window for
   3 s, quit it, stop. The watcher (`cm-sck-exclusions`) updates the filter within about 1 s of the
   launch: step through `screen.mp4`: the window may show for at most the first second after
   launch, never longer. (The logic is unit-tested in `record/exclude.rs`; only this step needs a
   real app.) A *window* recording ignores exclusions by design (it captures that one window).
1c. R3-7, static screens are not stalls: record a Display for 30 s with a still desktop (no
   moving content, no clock visible), then stop. In the track details of `manifest.json`,
   `tracks.screen.stalls` must be 0 and `stall_ms` 0. If it is not, ScreenCaptureKit stays silent
   for more than 1 s on a static screen on this machine: raise `STALL_TICKS` in
   `crates/cm-capture/src/record/pipeline.rs` (the watchdog only runs once the source has sent an
   idle status sample, see the module docs there) and note the macOS version here. A display put
   to sleep or a stuck capture for more than 1 s gives `stalls >= 1`, a `capture_stalled` warning
   and a failed 60 fps self-test when the stalls add up to more than 0.5 s.
2. REC-1.AC2: record a window, drag it across the screen during recording: the video keeps showing
   the whole window. Resize it: the content is letterboxed, never cropped.
3. REC-1 edge: close the recorded window: the recorder halts (`stopped_early` in the track details),
   the file is valid and keeps what was captured.
4. REC-8.AC3 / NFR-7: record to a small RAM disk (`hdiutil attach -nomount ram://4194304` is 2 GB)
   until free space drops to 1 GB: the recorder halts with `disk_full` and the file is valid.
5. REC-8.AC4: with free space under 10 minutes of recording at the chosen settings, the recorder
   emits `DiskLow` (the setup panel uses `cm_capture::disk::assess_disk` for the same warning).
6. Display disconnect during Display or Region recording: same as closing the window.
7. 5K display: Source size gives HEVC 5120x2880; forcing H.264 is refused with the 4096 px message.
