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
| `standard` | 15 to 20 min | everything that needs no sound, camera or long run: second instance, CLI auto-start, locked settings, region/window screenshots, library rename/annotate/undo/delete refusal, the 0.3.3 annotation ops (`annotate.pack`: callout, spotlight, stamps and a labelled step through the CLI with pixel statistics of the render; SKIPPED on a package older than 0.3.3), import of a PNG and a JPEG (`import.images`, same skip rule), Auto-redact (a fixture picture and a real Notepad window), import (mp4/mov/mkv/rotated/VFR/audio/errors), export quick/audio/errors, the MCP policy, idempotency, errors, events, CLI parity and HTTP tests, the no-model messages, the FFmpeg-missing wording, the desktop app (stays up, second launch forwards a file, hotkey opens one overlay per monitor, no console flash) |
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

Driver: `crates/cm-regress/src/sys/macos.rs` (written Nov 6, 2026 for 0.2.9, tested with a fake command runner only;
no hardware tier has been run yet, so there is no stored macOS report). Every probe goes through one `Runner`
trait, so each function has a unit test with scripted output (`cargo test -p cm-regress`, which also runs on
Linux and Windows).

```sh
make bundle-dev                       # target/bundle/CaptureMann-dev.app (bundle id dev.capturemann.app.dev)
scripts/testlab/regress.sh macos --tier smoke
scripts/testlab/regress.sh macos --tier standard --allow screen
scripts/testlab/regress.sh macos --tier full --allow screen --installer target/bundle/CaptureMann.dmg
scripts/testlab/regress.sh macos --tier hardware --allow screen,sound,mic,camera   # only when asked
```

The wrapper builds `cm-regress` and `cm-daemon` (release), takes `${TMPDIR}/cm-regress-mac.lock`, keeps the Mac
awake with `caffeinate`, with `--allow sound` sets the output volume to 30 % unmuted and puts the level and mute
state back, runs the suite, and copies the report to `verification/L1-macos-<version>-regression-<tier>-<date>-claude.md`
(+ `.json`). It refuses a package folder under `/Applications`, and never changes the display settings or the output
device. The app has no `cm-daemon` (it is embedded in `cm-desktop`), so the headless cases use a build of the same
commit; by hand: `cm-regress --tier standard --package-dir <App>.app/Contents/MacOS --daemon target/release/cm-daemon --out <dir>`.
Licences are looked up in `Contents/Resources`.

What the driver provides:

| Part | How | Needs |
| --- | --- | --- |
| Processes | `ps`, `pgrep -P`, `kill -9` (children first) | nothing |
| Window and display listing | JavaScript for Automation reading CoreGraphics (`CGWindowListCopyWindowInfo`, `NSScreen`); window layer above 0 counts as "topmost", `window_from_point` takes the frontmost visible window at the point | nothing (window titles are empty without Screen Recording) |
| Desktop session | `ioreg` screen-lock flag, and at least one display listed | nothing |
| `desktop.stays_up`, `desktop.second_launch`, `desktop.settings_applied` | start `Contents/MacOS/cm-desktop` on the case's own endpoint; a window or a menu-bar icon counts as showing something | `desktop_session` (the last also `screen`) |
| `desktop.hotkey_overlay` | Option+Shift+1 sent through System Events, one overlay window per display on its exact rectangle, Esc ends it | `screen`, Accessibility (SKIP when the key cannot be injected) |
| `desktop.tray_icon` | counts the app's menu-bar items with System Events (`menu bar 2`) | Accessibility (SKIP without it) |
| Recording: screen, mic, system audio (a 1 kHz tone played with `afplay`), camera | the shared `rec.*` cases and their picture, length and frame-rate checks | `screen`, `sound`, `mic`, `camera` |
| Crash reports | `~/Library/Logs/DiagnosticReports` `.ips` / `.crash` of `cm-*`, `capturemann`, `llama-server` written since the case started (a free check of every case) | nothing |
| `installer.dmg` | `hdiutil attach -nobrowse -readonly -mountpoint <sandbox>/mnt`, `ditto` the app into the sandbox, eject, `codesign --verify --deep --strict`, bundle id and version through `plutil`, the four programs present, `capturemann --version`, remove the copy and check nothing is left; a `.app` given instead of a `.dmg` skips the mount. The app is never opened and `/Applications` is never touched | `--installer <file>` |

Not implemented on macOS (they report SKIP, "not implemented for macos"): `desktop.hud_click_through` (click-through
is not readable from CoreGraphics), `desktop.recording_placement`, `desktop.no_console_flash` (a Windows
problem), `shot.window` / `redact.window_text` (they open Notepad) and `rec.each_monitor`. A locked screen or a
display that is off lists no display: the cases that need a desktop session report SKIP with that reason. Screen
Recording, Microphone, Camera and Accessibility are granted to the program that runs the suite (Terminal) and to the
dev bundle; see [`building.md`](building.md).

## Run it on Linux (the Ubuntu 24.04 lab laptop)

The Linux driver is `crates/cm-regress/src/sys/linux.rs` (probes and parsers, unit-tested with a fake command runner on
any machine) and `cases/linux.rs` (the `linux.*` cases). It was written on a Mac and **cross-checked only**
(`cargo check` and `clippy` for `x86_64-unknown-linux-gnu`); no case has run on the laptop yet, so the first run may
show wrong assumptions in the suite (fix the suite, not the evidence).

```sh
cargo build --release -p cm-regress            # on a Linux machine or CI: the laptop has no Rust toolchain
scripts/testlab/regress.sh linux --tier smoke    --binary <path to cm-regress> --session x11
scripts/testlab/regress.sh linux --tier standard --allow screen --session wayland
scripts/testlab/regress.sh linux --tier full     --allow screen --installer capturemann_<v>_amd64.deb      # linux.deb_install
scripts/testlab/regress.sh linux --tier full     --allow screen --installer capturemann-<v>-linux-x86_64.tar.gz   # linux.tarball_install
scripts/testlab/regress.sh linux --tier hardware --allow screen,sound,mic    # on request only
```

- The wrapper (`scripts/testlab/regress-linux.sh`, sourced by `regress.sh`) takes `~/lab/lab.lock` on the laptop, reads
  the desktop session's environment (`DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, `DBUS_SESSION_BUS_ADDRESS`,
  `XDG_RUNTIME_DIR`) from the running `gnome-shell`, runs `cm-regress` in it, and copies `results.json` and the report to
  `verification/L1-linux-<version>-regression-<tier>-<x11|wayland>-<date>-claude.md` (+ `.json`). It sets GNOME's idle
  delay and AC sleep to never for the run and puts them back; with `--allow sound` it saves, sets (0.3, unmuted) and
  restores the **volume and mute** of the default output with `wpctl` (the default device is never changed). The
  monitor layout is only read (`xrandr --listmonitors`); `WANT_DISPLAYS=2` waits for an external monitor. It never
  switches between X11 and Wayland: `--session` refuses to run on the wrong one, the person at the laptop chooses at the
  login screen. Run each tier once per session type.
- Every case has its own library and config folder, as on Windows: the real library is never touched.
- Window and monitor probes use `xprop`, `xwininfo` and `xrandr` (`apt install x11-utils`, installed by
  `linux-setup.sh`). On **Wayland** GNOME offers no way to list native windows from outside, so window-based checks say
  so and the app is checked by "stays up and its daemon answers"; the global hotkey and overlay case (`linux.hotkey_overlay`)
  reports SKIP always (no portal path on GNOME 46, no key injection). Screenshots of a window on Wayland need the
  desktop's own chooser and are a manual step (`tests/manual/LINUX-1.md`).
- `linux.vaapi_encoder` runs `vainfo` and compares it with the VAAPI line of `capturemann diagnostics`: H.264 encode
  listed but no render device reported (or the software-VP9 note) fails; no H.264 encode is reported as "software VP9
  encoder", the expected state on a machine without a VA-API GPU.
- `linux.deb_install` needs `--installer <file>.deb`, passwordless `sudo` and no capturemann package installed
  (otherwise SKIP, so a real install is never replaced). It runs `apt-get install`, checks `dpkg`, the files, that every
  program's libraries resolve (`ldd`, which tests the `Depends:` list) and `capturemann --version`, then `remove` and
  `purge`, and purges again whatever happened. `linux.tarball_install` extracts the tarball, installs with
  `install.sh --prefix <sandbox>` and `HOME` in the sandbox, runs the same checks and `--uninstall`.
- **Recording** (`rec.*`) runs by default since 0.3.0 (recording is on in the Linux build); `--no-recording` sets `CM_LINUX_RECORDING=0`
  for the suite and each recording case then reports SKIP with that reason. `--recording` is still accepted and does nothing new.
- Crashes: `systemd-coredump` records are read with `coredumpctl list --since @<time>` by the same hygiene check that reads the Windows event log (empty when
  journald keeps no dumps); `kill -9`/signal exits are still caught by the exit-code checks.
- Not covered on Linux (manual): tray menu entries as the shell draws them, KDE, NVIDIA/AMD encoders, Wayland window
  capture, the Record sheet look, aarch64.

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
- Stubs: clipboard copy (touches the user's clipboard). The macOS and Linux drivers are described above; cases list the
  platforms they support.
- The Windows package workflow does not run `cm-regress`: the suite has not run on a hosted runner yet and a red
  step caused by an unproven assumption would hide real failures. Add `cm-regress.exe --tier smoke --allow screen
  --package-dir <unpacked zip>` after the existing smoke step once a lab run has shown it stable.

## Known limits of the checks

Window cases open Notepad and close its process (on Windows 11 that can close other Notepad windows). The
installer case installs into a temp folder but the installer shares the per-user uninstall key with a real install;
the case exports the key first and imports it back. The Windows `cm-daemon` has no graceful stop signal for a hidden
process, so its stop is a kill there.
