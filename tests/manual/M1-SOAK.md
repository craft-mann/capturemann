# M1-SOAK: three hours of recording with the library open (release 0.2.7, track M1)

Status: planned, NOT run. It records the screen, the microphone and system audio, so it needs the
maintainer's consent and a time slot. Do not start it from an agent.

Goal: find growth that a short test never shows: memory, open files and threads of every CaptureMann
process, stuck windows, and `desktop.log` errors. Extends `VOC-1.md` section 11 (audio only, memory
limit 200 MB for the audio recording) to the screen recording with the library open.

## Setup

- A Mac on power, display allowed to sleep, the dev bundle (`make bundle-dev`, run by the
  maintainer; id `dev.capturemann.app.dev`) with Screen Recording and Microphone granted.
- About 40 GB free (3 h of screen at the default quality plus the mic and system tracks).
- A library with at least 200 items, opened in Studio (the Library page) the whole time, and the
  Settings -> About -> Diagnostics card open once at the start.
- Something that plays continuously (a lecture or a playlist) and a window that changes (a
  clock or a terminal running `top`), so the screen track has motion.
- Note the version, the build number and the chip (Apple Silicon or Intel).

## Run

1. In a terminal: `scripts/soak/sample.sh ~/soak-m1.csv 300 11400` (every 5 minutes for 3 h 10 min).
   It only reads process numbers (RSS, open files, threads) and counts `ERROR` lines in the log.
2. Start a screen recording with mic and system audio (the Record sheet, "Screen", camera off; run
   a second pass with the camera bubble on if time allows). Note the start time.
3. Every 30 minutes, without touching the recording: open the Diagnostics card, press Refresh and
   Copy, and paste the text under the hour in the report. The "Live app windows" list must show
   only what is on screen (the main window, the HUD, the indicator; plus the bubble when it is on).
4. At 1 h and 2 h: pause 10 s, resume (the pause path), and add a marker.
5. At 1 h 30: open and close the Library, the Settings page and one recording in the editor (window
   open/close churn), and trigger one toast (copy a path).
6. After exactly 3 hours press Stop. Wait for the post-processing, open the `.cmrec`, play the last
   minute, close it, and press Copy on the Diagnostics card once more.
7. Stop the sampler. Save `desktop.log` (the lines of the run) next to the CSV.

## Measured

| What | How | Pass |
|---|---|---|
| Memory (RSS) of `cm-desktop` | sampler column `rss_kb`, plot over time | no steady growth: the last hour's slope under 10 MB per hour, peak under 1 GB (the first run sets the baseline; write it down) |
| Open files | `open_files` | flat after the first 10 minutes, no more than 20 above the start |
| Threads | `threads` | flat, no more than 10 above the start |
| Library open | the Library stays responsive: scroll it at 0 h, 1 h, 2 h, 3 h and note the time to first scroll | no freeze, scrolling smooth |
| Windows | Diagnostics "Live app windows" at each check | only expected surfaces; no `STUCK` or `CLOSING` entry; no window that outlives its purpose |
| Log errors | `log_errors_since_start` and `grep ' ERROR ' desktop.log` | none; any `RefCell already borrowed` is listed with the user action that came just before it (the "activating" lines and the surface lines carry the module name) |
| Recording | durations of the tracks against the HUD timer; the drift check of VOC-1 section 11 | tracks within 50 ms of each other; no gap at the pause |
| Sleep | `pmset -g log` or the manifest | the Mac did not sleep |

## Report

Store it in `verification/` (template in its README): the CSV, a plot or table of RSS, files and
threads per hour, the five Diagnostics texts, the log lines, the verdict per row and the build id.
Until it exists, every text says "soak not run".
