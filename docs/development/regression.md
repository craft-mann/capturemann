# Release regression suite (`cm-regress`)

One program that checks the fundamentals of a **shipped** CaptureMann package on a real machine, so the maintainer
does not have to repeat them by hand on three systems before every release. It is run when preparing a release, not
on every commit. Written Nov 5, 2026 for Windows; the structure is shared so macOS and Linux drivers can reuse it.

## What it is

- `crates/cm-regress` (bin `cm-regress`): depends on `serde` and `serde_json` only, never on a product crate, so a
  product bug cannot be hidden by linking the code under test. It starts the programs of an installed folder or an
  unpacked zip (`capturemann`, `cm-mcp`, `cm-daemon`, `cm-desktop`, `cm-overlay`, the bundled `ffmpeg`, `ffprobe`
  and `llama-server`) as child processes and looks at what they print, write and show.
- Every case has its **own** temporary library, config folder, secrets file and endpoint (named pipe or socket), so
  it never touches the real library. Nothing is left running: the runner kills the case's processes, compares the
  process list before and after, and fails a case that leaves a program behind.
- Free checks for every case: no new stray product process; no `ERROR` or panic line in the case's own logs (a case
  can allow a line it expects); no new crash event of a CaptureMann program in the Windows Application log.
- Output: `results.json` (one record per case: id, status, duration, message, evidence) and `report-<tier>.md` in the
  format of `verification/README.md`: header (date, machine, GPU, commit, SHA-256 of the programs under test,
  flags), a table of cases, spec items and bug classes with their worst result, failures with evidence, and what was
  not run and why. The verdict line is **PARTIAL**; the suite never writes "verified" (that needs a person).

## Tiers

| Tier | Time | Adds |
| --- | --- | --- |
| `smoke` | under 3 min | package files, every program starts (no crash code), diagnostics, daemon start/stop, settings round trip, targets, one display screenshot, library manifest, MCP handshake and tool list against the spec, llama-server `--version` |
| `standard` | 15 to 20 min | everything that needs no sound, camera or long run: second instance, CLI auto-start, locked settings, region/window screenshots, library rename/annotate/undo/delete refusal, Auto-redact (a fixture picture and a real Notepad window), import (mp4/mov/mkv/rotated/VFR/audio/errors), export quick/audio/errors, the MCP policy, idempotency, errors, events, CLI parity and HTTP tests, the no-model messages, the FFmpeg-missing wording, the desktop app (stays up, second launch forwards a file, hotkey opens one overlay per monitor, no console flash) |
| `full` | 30 to 60 min | screen+cursor recording with pause/resume (H.264, constant 30 fps, 2 s key frames, length, no warnings), a region recording, kill -9 recovery, the HUD click-through check, export presets (YouTube 1080p, vertical, square, WebM, GIF), a transcription with `--whisper-model`, the installer case with `--installer` |
| `hardware` | flag-gated | microphone, system audio (a 1 kHz tone played by another process), camera frame rate, a second monitor, a long soak (`--soak-minutes N`) |

`cm-regress --list [--tier T]` prints the registry with the reason a case would be skipped on this machine.

## Flags

```
cm-regress --tier smoke|standard|full|hardware --package-dir <dir> --out <dir>
           [--only id,area,...] [--allow screen,sound,mic,camera,second_monitor,models,ffmpeg_full,network]
           [--keep] [--list] [--dev] [--installer <setup.exe>] [--zip <file>] [--commit <sha>]
           [--whisper-model <ggml file>] [--whisper-model-id small] [--soak-minutes N]
           [--speech <audio file>] [--speaker-models <dir>]
           [--timeout-scale F] [--verdict PARTIAL] [--who NAME]
```

- `--package-dir` defaults to `%LOCALAPPDATA%\Programs\CaptureMann`, then the folder of `cm-regress`.
- `--allow` is the permission list: `screen` lets cases capture the screen (screenshots, recordings), `sound` plays
  tones, `mic`, `camera`, `second_monitor`, `models` (also implied by `--whisper-model`). Cases that need something not
  allowed report SKIP with the reason. A logged-in desktop session is detected (Windows: not session 0).
- `--dev` means the folder is a build tree (`target/debug`): the bundled FFmpeg, licences and helper are not required
  and the FFmpeg on `PATH` is used. This is how the suite was checked on the Mac.
- `--keep` keeps each case's sandbox for inspection. Pictures of the screen are always deleted.

## Run it for a release (Windows lab laptop)

The laptop has no Rust toolchain: `cm-regress.exe` comes from the manual workflow **Windows tests**
(`cm-regress.exe` is in `capturemann-windows-tests.zip`). The package under test is installed per user
(`windows-package` artifact, `/S`), so the suite sees what a user gets.

```sh
gh workflow run windows-tests.yml --repo craft-mann/capturemann -r track/windows     # builds cm-regress.exe
scripts/testlab/regress.sh windows --tier smoke    --run latest --allow screen
scripts/testlab/regress.sh windows --tier standard --allow screen
scripts/testlab/regress.sh windows --tier full     --allow screen            # no sound
scripts/testlab/regress.sh windows --tier hardware --allow screen,sound,mic  # on request; volume 0.3 then back to 0 muted
# a new package first:  --package-run <Windows package run id>   (silent install, replaces the installed copy)
```

The wrapper takes `C:\lab\lab.lock` (refuses while another agent holds it, `FORCE=1` for a stale one), pushes
`win-run-in-session.ps1`, runs `cm-regress.exe` in the logged-in session, copies `results.json` and the report to
`verification/L1-windows-<version>-regression-<tier>-<date>-claude.md` (+ `.json`), restores the volume, removes its
files and the lock. Read the report, put real findings in the release checklist
([`windows-0.2.8-release-checklist.md`](windows-0.2.8-release-checklist.md)) and fix the suite, not the evidence.

## Run it on macOS

The app has no `cm-daemon` (it is embedded in `cm-desktop`), so give the headless cases a build of the same commit:
`cargo build --release -p cm-daemon`, then
`cm-regress --tier standard --package-dir <App>.app/Contents/MacOS --daemon target/release/cm-daemon --out <dir>`.
Licences are looked up in `Contents/Resources`. A locked screen or a display that is off lists no display: the
cases that need a desktop session report SKIP with that reason (unlock the Mac and use `--allow screen` for the
screenshot cases). Crashes are read from `~/Library/Logs/DiagnosticReports`. The desktop-app and window cases are
not implemented for macOS yet and report SKIP.

## Reading a result

- **FAIL** with the message and evidence lines: expected against actual. A failure in the suite itself (a wrong
  assumption) is fixed in `crates/cm-regress`; a failure of the product is a bug to report with the evidence.
- **SKIP** is not a pass: the report lists what was not run and why. A tier without `--allow screen` runs no capture.
- The table "Acceptance criteria and bug classes" shows, per spec id or named bug, the worst result of the cases that
  protect it.

## Add a case

1. Pick the area module in `crates/cm-regress/src/cases/` (or add one and list it in `cases/mod.rs`).
2. Add a `Case { id: "area.name", title, area, specs, tier, needs, platforms, timeout_s, run }` to its `cases()` and
   write `fn run(ctx: &mut Ctx) -> R`. Use `ctx.start_daemon()`, `ctx.cli_ok(&[...])`, `McpClient`, `media::probe`,
   `check!(cond, "message")`; return `skip("reason")` when the machine cannot do it, never a silent pass.
3. Use only the sandbox the context gives you (`ctx.library`, `ctx.config`, `ctx.root`). No real library, no
   network, no sound or capture unless the case declares `Need::Sound` / `Need::Screen`.
4. Unit-test pure parsing on any machine (`cargo test -p cm-regress`). OS-specific code goes in `src/sys/` only
   (`scripts/check_target_os.py`); `cargo check --target x86_64-pc-windows-msvc -p cm-regress` must pass.
5. Mention the spec id or the bug in `specs`, so the report links the case to what it protects.

## What it does not replace

- The manual scripts in [`tests/manual/`](../../tests/manual/README.md) and every check a person must do by eye or
  ear: how the picker, HUD, toasts and countdown look, A/V sync by ear, the tray menu, Windows 10, SmartScreen,
  mixed-DPI layouts, a monitor left of or above the primary.
- Sound, camera and second-monitor cases need the hardware, the room (a lit room for the camera) and
  `--allow`; the volume must be above 0 for the system-audio case. Process-loopback exclusion of CaptureMann's own
  sound cannot be driven from outside; it stays with the `cm-audio` hardware test.
- Stubs in the first version: clipboard copy (touches the user's clipboard), the macOS and Linux drivers (cases list the platforms they
  support; the CLI/MCP/library/import/export cases already run on macOS with `--dev`).
- The Windows package workflow does not run `cm-regress`: the suite has not run on a hosted runner yet and a red
  step caused by an unproven assumption would hide real failures. Add `cm-regress.exe --tier smoke --allow screen
  --package-dir <unpacked zip>` after the existing smoke step once a lab run has shown it stable.

## Known limits of the checks

Window cases open Notepad and close its process (on Windows 11 that can close other Notepad windows). The
installer case installs into a temp folder but the installer shares the per-user uninstall key with a real install;
the case exports the key first and imports it back. The Windows `cm-daemon` has no graceful stop signal for a hidden
process, so its stop is a kill there.
