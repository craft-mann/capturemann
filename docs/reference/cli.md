# CaptureMann command-line reference

`capturemann` is the command-line client of CaptureMann (105 commands and subcommands in version 0.3.4, listed below in the order of `capturemann --help`). It was written from the built binary (`--help` of every command) and the source of the 0.3.4 build. The same actions are available to AI agents as MCP tools; see [mcp.md](mcp.md) and the machine-readable [tools.json](tools.json). How these files were made: [README.md](README.md).

Wording: nothing here is "verified" in the sense of the repository's `verification/` reports. Outputs shown under a command were produced by running that command against a headless `cm-daemon` with an empty throw-away library (one small picture imported); commands that record, capture the screen, need a speech or text model, a recording, or an account were not run, and their examples are marked as such. **Long Shot** (`capture screenshot --long`) and **Click Guide** (`guide ...`) are Preview features: they may still change.

## Where the command lives

The program is called `capturemann` (`capturemann.exe` on Windows). It ships inside every CaptureMann package, next to `cm-mcp` (the MCP server):

| System | Location (from the packaging scripts and the README) |
| --- | --- |
| macOS | Inside the app: `/Applications/CaptureMann.app/Contents/MacOS/capturemann`. It is not on `PATH`; link it once with `sudo ln -s /Applications/CaptureMann.app/Contents/MacOS/capturemann /usr/local/bin/capturemann`. The Homebrew cask (`brew install --cask craft-mann/tap/capturemann`) links it into Homebrew's `bin`. The bundle contains `cm-desktop`, `cm-overlay`, `capturemann` and `cm-mcp`, but no `cm-daemon`. |
| Windows | The per-user installer puts the files in `%LOCALAPPDATA%\Programs\CaptureMann` (no administrator rights) and has an optional "add `capturemann.exe` and `cm-mcp.exe` to my PATH" step. In the portable zip, unpack anywhere; `capturemann.exe` is next to `cm-desktop.exe`. `cm-daemon.exe` is in the same folder. |
| Linux, `.deb` | The programs are in `/opt/capturemann/lib/capturemann/`; `/usr/bin/capturemann`, `/usr/bin/cm-mcp`, `/usr/bin/cm-daemon`, `/usr/bin/cm-desktop` and `/usr/bin/cm-overlay` are symlinks to them. |
| Linux, tarball | `install.sh` installs under a prefix (default `~/.local`, or `sudo ./install.sh --prefix /usr/local`): programs in `<prefix>/lib/capturemann/`, symlinks in `<prefix>/bin/`. |

`capturemann --version` prints the version; `--help` and `-h` print help on any command.

## How the CLI talks to the app

The CLI is a thin JSON-RPC client of the CaptureMann daemon. It never opens the library itself: every command is one daemon action (two for `capture screenshot --annotate`), the same action the MCP tool of the same name runs, so the app, the CLI and agents share state, undo and confirmation gates. A command opens a local socket (a Unix socket on macOS and Linux, a named pipe on Windows), sends `session.hello` with the client name `capturemann-cli`, and then one request. Revisions made by the CLI are recorded as `agent:capturemann-cli`.

- **Default endpoint.** macOS `~/Library/Application Support/CaptureMann/daemon.sock`; Linux `$XDG_RUNTIME_DIR/capturemann/daemon.sock` (or `~/.local/share/capturemann/daemon.sock`); Windows `\\.\pipe\capturemann`. The running CaptureMann app serves it.
- **If nothing listens,** the CLI starts a headless `cm-daemon` (detached, found through `CAPTUREMANN_DAEMON`, then next to the `capturemann` executable, then on `PATH`) and waits up to 15 seconds for it, unless `--no-start` is given. With `--no-start`, or when `cm-daemon` cannot be found, the error is `daemon_unavailable` (exit 5). Because the macOS bundle has no `cm-daemon`, on macOS the app has to be running. A headless daemon has no window to ask for approval: a command that needs confirmation is answered `user_declined` with reason `no_confirmer` (exit 3; run, see `library rm`).
- **Which library.** A daemon the CLI starts opens `--library <dir>` of `daemon start`, else `CAPTUREMANN_LIBRARY`, else `library.path` in the app's `settings.json`, else `~/Movies/CaptureMann`, the same rule as the app.

Environment variables the CLI reads:

| Variable | Meaning |
| --- | --- |
| `CAPTUREMANN_SOCKET` | Daemon socket path or pipe name; the same as `--socket`. Point a second instance at its own data with this, `CAPTUREMANN_LIBRARY` and `CAPTUREMANN_CONFIG_DIR`. |
| `CAPTUREMANN_LIBRARY` | Library folder for a daemon the CLI starts (also the `--library` of `daemon start`). |
| `CAPTUREMANN_CONFIG_DIR` | Config and settings folder (the CLI reads `settings.json` there to find `library.path`; the daemon keeps its settings there). |
| `CAPTUREMANN_DAEMON` | Path of the `cm-daemon` binary to start. |
| `CAPTUREMANN_MCP` | Path of the `cm-mcp` binary that `capturemann mcp` runs. |

`CAPTUREMANN_SECRETS=file` (keep secrets in an owner-only unencrypted file instead of the system keyring) and `CAPTUREMANN_MCP_TOKEN` (the HTTP bearer token of `cm-mcp --http`) belong to the daemon and the MCP server. Other `CM_*` variables (FFmpeg, text engine, OCR helper paths) are described in the repository's `CLAUDE.md`.

## Global flags

These four flags are accepted by every command, before or after the subcommand. They are left out of the per-command tables.

| Flag | Value | Default | Description |
| --- | --- | --- | --- |
| `--json` | switch | off | Machine-readable JSON on stdout (the daemon's result object, pretty-printed); errors as JSON on stderr. |
| `--socket <PATH>` | path or pipe name | the platform endpoint | Daemon socket path (Unix) or pipe name (Windows). Env: `CAPTUREMANN_SOCKET`. |
| `--no-start` | switch | off | Do not start a daemon when none is running; fail with `daemon_unavailable` instead. |
| `--request-id <ID>` | string | none | Idempotency key for commands that change something. A retry with the same key replays the first result (the daemon keeps it for 24 hours). Commands that run several actions append a suffix to it. |

## Output and JSON mode

By default every command prints human-readable text on stdout and errors on stderr as `error[<code>]: <message>` with extra lines such as `reason:` or a hint. With `--json`, stdout carries the daemon's result object and a failure prints one line of JSON on stderr: `{"error":{"code":...,"message":...,"data":...,"exit_code":N}}`. Image bytes are never printed: `image.data` is replaced by `image.bytes`; use `--out FILE` (on the commands that have it) to write the PNG. Progress of a waiting command (`--wait`) goes to stderr, so stdout stays parseable.

```text
$ capturemann --json settings get recording.fps
{
  "settings": {
    "recording.fps": 30
  }
}

$ capturemann --json capture get missing_id
{"error":{"code":"not_found","message":"no capture `missing_id`","data":null,"exit_code":2}}   (stderr)
(exit 2)
```

Times in editing commands are seconds (`12.5`) or ticks (`t:1500000`, 120,000 ticks per second); ranges are `START-END`.

## Exit codes

The exit code follows the shared error code of the daemon's answer (`error.data.code`). The table is from `specs/README.md` and `crates/cm-cli/src/error.rs` (a test pins every row).

| Exit | Meaning | Error codes |
| --- | --- | --- |
| 0 | Success (also `--help` and `--version`) | |
| 1 | Other failure | `internal`, `not_implemented`; any code outside the table (`method_not_found`, `invalid_request`); CLI-local errors `mcp_unavailable` and `mcp_failed` |
| 2 | Bad input or not found | `invalid_input`, `not_found`; also command-line usage errors (clap) |
| 3 | The user declined, or did not answer in 5 minutes | `user_declined`, `confirmation_timeout` |
| 4 | An operating-system permission is missing (screen, camera, microphone, accessibility, input monitoring, files) | `permission_missing` |
| 5 | Platform, network, disk or daemon problem | `device_busy`, `disk_full`, `network`, `platform_rejected`, `auth_expired`, `daemon_unavailable` |
| 6 | A job failed, or a conflicting operation is running | `job_failed`, `busy` |

`--wait` commands exit 6 when the job fails or is cancelled; `--timeout SECS` with `--wait` gives up after that many seconds (exit 6, the job keeps running).

## Command index

| Command | What it does | MCP tool |
| --- | --- | --- |
| [`capturemann targets`](#capturemann-targets) | List displays and windows (`list_targets`) | `list_targets` |
| [`capturemann capture`](#capturemann-capture) | Screenshots (`capture_screenshot`, `get_capture`) |  |
| [`capturemann annotate`](#capturemann-annotate) | Add annotation ops to a screenshot (`annotate`) | `annotate`, `suggest_redactions`, `suggest_blueprint` |
| [`capturemann undo`](#capturemann-undo) | Undo the last revision of a capture (`undo`) | `undo` |
| [`capturemann redo`](#capturemann-redo) | Redo the last undone revision of a capture (`redo`) | `redo` |
| [`capturemann export`](#capturemann-export) | Render a recording to one combined MP4 with the FFmpeg sidecar (`export_video`). Starts a background job (`jobs get <id>`); `--wait` blocks until it... | `export_video`, `export_audio`, `estimate_export`, `export_captions` |
| [`capturemann import`](#capturemann-import) | Bring media files and pictures that CaptureMann did not record into the library as captures (`import_media`): `import clip.mov memo.m4a photo.jpg... | `import_media` |
| [`capturemann board`](#capturemann-board) | Put 2 to 12 screenshots into one new picture (`create_board`): `board <id> <id> --layout pair --before-after`. Layouts: pair, grid, strip, free... | `create_board` |
| [`capturemann export-images`](#capturemann-export-images) | Resize, convert, watermark and rename many screenshots at once (`export_images`, Bulk export): `export-images <id>... --dest ~/Pictures/site... | `export_images` |
| [`capturemann library`](#capturemann-library) | Browse the library (`list_library`) |  |
| [`capturemann settings`](#capturemann-settings) | Read and change settings (`get_settings`, `set_settings`) |  |
| [`capturemann record`](#capturemann-record) | Screen recording: start, pause, resume, stop, status (`start_recording`, ...) |  |
| [`capturemann guide`](#capturemann-guide) | Click Guide (`start_guide`, `stop_guide`, `get_guide`, `set_guide`, `export_guide`): click through a task once and get a numbered guide with a... |  |
| [`capturemann text`](#capturemann-text) | Print the text in a screenshot (`recognize_text`, read-only, on this computer only): plain text in reading order from the rendered image (annotations... | `recognize_text` |
| [`capturemann devices`](#capturemann-devices) | List cameras, microphones and system-audio support (`list_devices`) | `list_devices` |
| [`capturemann diagnostics`](#capturemann-diagnostics) | Print the diagnostics report to paste into an issue (`get_diagnostics`): versions, system, permissions, models, a few settings, library counts and... | `get_diagnostics` |
| [`capturemann cursor`](#capturemann-cursor) | Print the pointer path and clicks of a recording (`get_cursor_track`): moves, button down/up, scroll direction, in physical pixels of the screen... | `get_cursor_track` |
| [`capturemann platform-rules`](#capturemann-platform-rules) | Print the limits, required fields, privacy options and export presets of a publishing platform (`get_platform_rules`). Today: `youtube` | `get_platform_rules` |
| [`capturemann destinations`](#capturemann-destinations) | List the destinations set up in Settings (`list_destinations`) | `list_destinations` |
| [`capturemann upload`](#capturemann-upload) | Copy a capture or a file to a destination in the background (`upload`). Prints the job id | `upload` |
| [`capturemann publish`](#capturemann-publish) | Publish a recording to YouTube (`publish_video`), or list the connected accounts (`publish accounts`). The app asks you to approve every publish;... | `publish_video` |
| [`capturemann mcp`](#capturemann-mcp) | Run the MCP server (the `cm-mcp` binary next to this one): stdio by default, local HTTP with `--http`. Stdout is the MCP channel, so nothing else is... |  |
| [`capturemann daemon`](#capturemann-daemon) | Manage the background daemon |  |
| [`capturemann edit`](#capturemann-edit) | Edit a recording without touching its files: trim, cut, split, speed, background (`trim`, `cut_segments`, `restore_segments`, `split`, `set_speed`,... |  |
| [`capturemann ai`](#capturemann-ai) | Local AI: speech models, transcripts, captions and suggestions. Everything runs on this computer; a model is downloaded only when you run `ai models... |  |
| [`capturemann jobs`](#capturemann-jobs) | Background jobs (`get_job`, `list_jobs`, `cancel_job`, `job.*` events) |  |

`capturemann help` is clap's built-in: `capturemann help <command>` prints the same text as `capturemann <command> --help`.

## Commands

### `capturemann targets`

List displays and windows (`list_targets`)

- **MCP tool:** `list_targets`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann targets [OPTIONS] [COMMAND]
```

Subcommands:

- `capturemann targets list`: Same as `capturemann targets`

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--displays-only` | switch |  |  | Only list displays (skips the window enumeration, which is much faster) |

Examples:

```text
$ capturemann targets --displays-only
Displays
  1  Built-in Retina Display (built-in)  3024x1964 at (0, 0)  scale 2.0  (primary)
  2  External Display  3840x2160 at (-442, -2160)  scale 2.0
Windows
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann targets list`

Same as `capturemann targets`

- **MCP tool:** `list_targets`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann targets list [OPTIONS]
```

Examples:

Not run here (syntax only):

```sh
capturemann targets list
```

### `capturemann capture`

Screenshots (`capture_screenshot`, `get_capture`)

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann capture [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann capture screenshot`: Take a screenshot of a display, a window or a region
- `capturemann capture get`: Show a capture's metadata and optionally save its current image

#### `capturemann capture screenshot`

Take a screenshot of a display, a window or a region

- **MCP tool:** `capture_screenshot`, Free, can be set to Ask first
- **Asks on screen:** Normally no. `--long` with automatic scrolling (the default for Long Shot) is Confirm because the screen moves; `--hand` (you scroll) is refused for agents. Both are Preview.
- **Preview:** yes, this is a Preview feature (it may still change).

Synopsis:

```text
capturemann capture screenshot [OPTIONS] <--display <ID>|--window <ID|TITLE>|--region <X,Y,W,H>|--pick>
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--display` | `<ID>` |  |  | Display id (see `targets`) |
| `--window` | `<ID\|TITLE>` |  |  | Window id, or a case-insensitive substring of its title or app name. An exact title wins; several partial matches fail with `ambiguous_target` and list their ids |
| `--region` | `<X,Y,W,H>` |  |  | Region `x,y,w,h` in physical px, relative to the display (default: primary display) |
| `--pick` | switch |  |  | Let the desktop's own picker choose a window, a region or a screen (Wayland sessions, where windows have no ids). A person at the screen must choose; Esc cancels (exit 3) |
| `--delay` | `<SECONDS>` |  |  | Wait this many seconds (0 to 30) before capturing |
| `--copy` | switch |  |  | Also copy the image to the clipboard |
| `--annotate` | `<OPS>` |  |  | Annotation ops to apply right away: a JSON file, inline JSON, or `-` for stdin |
| `--out` | `<FILE>` |  |  | Write the resulting PNG to this file |
| `--title` | `<TITLE>` |  |  | Library title (default: `Screenshot yyyy-MM-dd HH.mm`) |
| `--long` | switch |  |  | Long Shot (CAP-6): scroll the window (or area) and stitch one tall image. CaptureMann scrolls for you (the user confirms it when an agent runs this) **(Preview)** |
| `--horizontal` | switch |  |  | Long Shot: scroll sideways instead of down **(Preview)** |
| `--max-px` | `<PX>` |  |  | Long Shot: stop at this length in px (at most 16384) **(Preview)** |
| `--hand` | switch |  |  | Long Shot: you scroll by hand (refused for agents) **(Preview)** |

Examples:

```text
$ capturemann capture screenshot
error: the following required arguments were not provided:
  <--display <ID>|--window <ID|TITLE>|--region <X,Y,W,H>|--pick>

Usage: capturemann capture screenshot <--display <ID>|--window <ID|TITLE>|--region <X,Y,W,H>|--pick>

For more information, try '--help'.   (stderr)
(exit 2)
```

The output above was run against a headless daemon with an empty throw-away library.

Not run here (syntax only):

```sh
capturemann capture screenshot --display 1 --out shot.png
capturemann capture screenshot --window "Safari" --annotate ops.json --copy
capturemann capture screenshot --window "Docs" --long --max-px 8000   # Long Shot (Preview)
```

#### `capturemann capture get`

Show a capture's metadata and optionally save its current image

- **MCP tool:** `get_capture`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann capture get [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--revision` | `<REVISION>` |  |  | Revision (only the current one is supported) |
| `--out` | `<FILE>` |  |  | Write the current image (annotations applied) to this PNG file |

Examples:

```text
$ capturemann capture get shot_06GYEY0TM4JZ7KGH
shot_06GYEY0TM4JZ7KGH  demo
  kind: screenshot
  created: 2026-11-17T03:06:10.480342Z
  revision: 0  (undo: false, redo: false)
  ops: 0
  path: /home/me/cm-demo/lib/demo.cmshot
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann annotate`

Add annotation ops to a screenshot (`annotate`)

- **MCP tool:** `annotate`, Free, can be set to Ask first; `suggest_redactions`, Free, read-only; `suggest_blueprint`, Free, read-only
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann annotate [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--ops` | `<JSON\|FILE\|->` |  |  | Ops as a JSON array (or `{"ops": [...]}`): inline JSON, a file path, or `-` for stdin |
| `--replace` | switch |  |  | Replace the existing ops instead of appending (still one undo step) |
| `--out` | `<FILE>` |  |  | Write the rendered PNG to this file |
| `--copy` | switch |  |  | Also copy the rendered image (annotations and redactions applied) to the clipboard |
| `--no-image` | switch |  |  | Do not ask the daemon to send the rendered image back (`return_image: false`) |
| `--suggest-redactions` | switch |  |  | Read-only (`suggest_redactions`): find emails, API keys and other tokens (and phone numbers and IPv4 addresses) with on-device text recognition and print blur rectangles. Nothing is applied; pass the ones you accept to `annotate --ops` as `blur` ops |
| `--redact-emails` | switch |  |  | Like `--suggest-redactions`, but email addresses only |
| `--kinds` | `<KIND[,KIND...]>` |  |  | With `--suggest-redactions`: which kinds to look for, comma separated or repeated (`--kinds email,token --kinds phone`). Default: every kind except `npi`, which is off unless named. The daemon rejects unknown names |
| `--suggest-blueprint` | switch |  |  | Read-only (`suggest_blueprint`): find the text lines with on-device text recognition and print them as boxes (with the measured fill and bar colour) for a `blueprint` op. Nothing is applied; pass the boxes you want replaced to `annotate --ops` (base edits first) |

Examples:

```text
$ capturemann annotate shot_06GYEY0TM4JZ7KGH --ops '<ops>' --no-image
Annotated shot_06GYEY0TM4JZ7KGH: revision 1 by agent:capturemann-cli
```

```text
$ capturemann annotate shot_06GYEY0TM4JZ7KGH --suggest-redactions
0 proposed redaction(s) for shot_06GYEY0TM4JZ7KGH (nothing applied)
```

```text
$ capturemann annotate shot_06GYEY0TM4JZ7KGH --suggest-blueprint
0 text line(s) in shot_06GYEY0TM4JZ7KGH (nothing applied)
```

An `arrow`, `line`, `rect` or `ellipse` op takes an optional `style` (`"solid"` by default, `"dashed"` or `"dotted"`; ANN-12), for example `--ops '[{"op":"rect","rect":[20,20,200,80],"style":"dashed"}]'`. Any other value is `invalid_input` naming the op (`ops[0]`).

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann undo`

Undo the last revision of a capture (`undo`)

- **MCP tool:** `undo`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann undo [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--out` | `<FILE>` |  |  | Write the resulting PNG to this file |

Examples:

```text
$ capturemann undo shot_06GYEY0TM4JZ7KGH
Undid shot_06GYEY0TM4JZ7KGH: now at revision 2 (undo: false, redo: true)
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann redo`

Redo the last undone revision of a capture (`redo`)

- **MCP tool:** `redo`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann redo [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--out` | `<FILE>` |  |  | Write the resulting PNG to this file |

Examples:

```text
$ capturemann redo shot_06GYEY0TM4JZ7KGH
Redid shot_06GYEY0TM4JZ7KGH: now at revision 3 (undo: true, redo: false)
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann export`

Render a recording to one combined MP4 with the FFmpeg sidecar (`export_video`). Starts a background job (`jobs get <id>`); `--wait` blocks until it ends

- **MCP tool:** `export_video`, Free, can be set to Ask first; `export_audio`, Free, can be set to Ask first; `estimate_export`, Free, read-only; `export_captions`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann export [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--preset` | `<PRESET>` |  | quick | Preset: `quick` (the default: screen + camera bubble + audio, H.264/AAC at the recording's size), a platform preset (`youtube-4k`, `youtube-1440p`, `youtube-1080p`, `vertical-1080`, `square-1080`, `source`; -14 LUFS audio) or `gif` / `webm` |
| `--format` | `<FORMAT>` | mp4, gif, webm, m4a, wav |  | `gif` or `webm`: the same as `--preset gif` / `--preset webm`. With `--audio`, `m4a` (the default) or `wav`; `m4a` and `wav` alone imply `--audio` |
| `--audio` | switch |  |  | Export the audio only (`export_audio`): M4A (AAC, 48 kHz) or, with `--format wav`, 48 kHz 24-bit WAV, with the recording's cuts and speed applied, in the capture's `exports/` folder. For an audio recording or a screen recording; no video is encoded. M4A is normalised to -16 LUFS unless `--loudness` says otherwise |
| `--tracks` | `<TRACKS>` | mix, mic, system |  | For the audio export (`--audio`, or `--format m4a\|wav`): which tracks, `mix` (the default), `mic` or `system` |
| `--out-dir` | `<DIR>` |  |  | For the audio export: also copy the file into this existing folder (never overwrites) |
| `--size` | `<SIZE>` |  |  | The frame of an MP4 or WebM: `source` (the recording's own size, the default) or a height step, `1440p`, `1080p`, `720p` or `480p` (`1440`, `1080`, `720` and `480` work too). The picture keeps its shape and is never enlarged: a step above the recording is refused, one equal to it is the recording's size. Not for GIF (use `--width`) |
| `--estimate` | switch |  |  | Print the size to expect ("About 14 MB, up to 35 MB", with the codec and the pixels) instead of exporting. Takes `--format`, `--size`, `--width` and `--max-mb`; a GIF has no estimate (the size is known after the export). Free and read-only (`estimate_export`) |
| `--width` | `<PX>` |  |  | GIF width in px (480, 640, 800 or 1024); for WebM an even width, kept for older scripts (`--size` is the way to choose the frame of an MP4 or WebM, and `--width` cannot be combined with it) |
| `--fps` | `<FPS>` |  |  | GIF and WebM frame rate (GIF: 10, 15 or 24) |
| `--max-mb` | `<MB>` |  |  | GIF and WebM: fit the file under this many MB (10^6 bytes) by lowering the width, frame rate or bitrate |
| `--loudness` | `<LUFS\|off>` |  |  | Normalise the integrated loudness to this level in LUFS (-40 to -5; -14 is the usual platform target), with a -1 dBTP true-peak limiter. Default: keep the recorded level. With `--audio`: `off` turns the normalisation off (the default for M4A is -16) |
| `--captions` | `<FORMAT>` | srt, vtt, txt, md |  | Write the captions as a file instead of a video: `srt`, `vtt`, `txt` (plain text, one caption line per row) or `md` (Markdown) in the capture's `exports/` folder (`export_captions`), with the recording's cuts and speed applied. Run `capturemann ai transcribe` first |
| `--timestamps` | switch |  |  | With `--captions txt`: start every line with `[mm:ss]` (or `[hh:mm:ss]` for long recordings) |
| `--language` | `<LANG>` |  |  | With `--captions`: write this translation (made by `ai translate`) instead of the original captions. The file is `<title> - captions.<LANG>.<format>` |
| `--speaker-names` | switch |  |  | With `--captions srt`: start every cue with `NAME:` once the recording has speakers (VTT, TXT and Markdown name them on their own) |
| `--burn-captions` | switch |  |  | Burn the captions into the video (white text on a dark box, inside the platform's safe area). The default is the recording's `ai captions options --burn` setting. Needs a transcript: run `capturemann ai transcribe` first |
| `--no-captions` | switch |  |  | Leave the captions out of the video even if the recording is set to burn them in |
| `--wait` | switch |  |  | Wait for the export to finish and print its file (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (exit 6; the export keeps running). Without it the command waits for as long as the job is queued, held or running |

Examples:

Not run here (syntax only):

```sh
capturemann export rec_06GYEVMCKZSRHR46 --preset youtube-1080p --wait
capturemann export rec_06GYEVMCKZSRHR46 --estimate --format webm --size 720p
capturemann export rec_06GYEVMCKZSRHR46 --captions srt
```

### `capturemann import`

Bring media files and pictures that CaptureMann did not record into the library as captures (`import_media`): `import clip.mov memo.m4a photo.jpg --transcribe --wait`. Files only. Starts a background job (`jobs get <id>`); `--wait` blocks until it ends. `--remove-originals` moves each original to the Trash after its copy is verified and asks on screen first

- **MCP tool:** `import_media`, Free, can be set to Ask first
- **Asks on screen:** Normally no. `--remove-originals` (without `--dry-run`) raises this command to Confirm: the app lists every file and asks first.

Synopsis:

```text
capturemann import [OPTIONS] <FILE>...
```

Arguments:

| Argument | Description |
| --- | --- |
| `<FILE>...` | The files to import (video: MP4, MOV, M4V, MKV with H.264 or HEVC; audio: M4A, WAV, MP3; pictures: PNG, JPEG, which become screenshots you can annotate). Relative paths and `~` are resolved here. Folders are refused |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--title` | `<TITLE>` |  |  | A title for the file in the same position (repeat the flag in order; the file name is the default) |
| `--transcribe` | switch |  |  | Transcribe each import afterwards (needs an installed speech model; skipped otherwise) |
| `--language` | `<CODE>` |  |  | With `--transcribe`: the language code (default: detect) |
| `--diarize` | switch |  |  | With `--transcribe`: label the speakers afterwards |
| `--report` | `<TEMPLATE>` |  |  | With `--transcribe`: write a report from this template afterwards (`ai report templates`) |
| `--remove-originals` | switch |  |  | Move each original to the Trash once its copy in the library is verified. The user must approve on screen (the dialog lists every file); without `--wait` the answer is awaited before the command returns the job |
| `--dry-run` | switch |  |  | Only check the files and print what would happen; import nothing |
| `--wait` | switch |  |  | Wait for the import to finish (the files in the library) and print one line per file. A transcript from `--transcribe` is a job of its own: its id is printed, it is not waited for. Exit 6 if no file was imported |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (exit 6; the job keeps running) |

Examples:

```text
$ capturemann import demo.png --wait
imported /home/me/cm-demo/demo.png -> shot_06GYEY0TM4JZ7KGH
importing... 0%   (stderr)
```

The output above was run against a headless daemon with an empty throw-away library.

Not run here (syntax only):

```sh
capturemann import interview.m4a clip.mov --transcribe --wait
```

### `capturemann board`

Put 2 to 12 screenshots into one new picture (`create_board`): `board <id> <id> --layout pair --before-after`. Layouts: pair, grid, strip, free (`--slot x,y,w,h` per image). The board is a new screenshot capture; the sources are copied in as they are now and not changed

- **MCP tool:** `create_board`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann board [OPTIONS] --layout <LAYOUT> <IDS> <IDS>...
```

Arguments:

| Argument | Description |
| --- | --- |
| `<IDS> <IDS>...` | Capture ids of 2 to 12 screenshots, in order |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--layout` | `<LAYOUT>` | pair, grid, strip, free |  | pair (exactly 2), grid, strip or free |
| `--stacked` | switch |  |  | Pair and strip: stack them in a column instead of a row |
| `--columns` | `<N>` |  |  | Grid: columns (1 to 6) |
| `--slot` | `<X,Y,W,H>` |  |  | Free: the rectangle of one image, `x,y,w,h` in board pixels; give one per image |
| `--gap` | `<GAP>` |  |  | Space between images in px (0 to 200) |
| `--margin` | `<MARGIN>` |  |  | Space around the board in px (0 to 400) |
| `--background` | `<BACKGROUND>` |  |  | transparent, white, light_grey, dark or #RRGGBB |
| `--radius` | `<RADIUS>` |  |  | Corner radius in px (0 to 100) |
| `--shadow` | switch |  |  | A soft shadow under each image |
| `--before-after` | switch |  |  | Pair: BEFORE and AFTER labels |
| `--caption` | `<TEXT>` |  |  | A caption under an image; repeat once per image, in order |
| `--title` | `<TEXT>` |  |  | A title drawn on the board |
| `--name` | `<TITLE>` |  |  | The new capture's title |

Examples:

```text
$ capturemann board shot_06GYEY0TM4JZ7KGH shot_06GYEY0VP3EECBAH --layout pair --before-after
created board shot_06GYEY100BZ51NSJ (1384 x 516, pair)
/home/me/cm-demo/lib/Board.cmshot
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann export-images`

Resize, convert, watermark and rename many screenshots at once (`export_images`, Bulk export): `export-images <id>... --dest ~/Pictures/site --longest 1600 --format jpeg --watermark-text DRAFT`. Writes new files, never overwrites, never changes the library. Starts a background job (`jobs get <id>`); `--wait` blocks until it ends

- **MCP tool:** `export_images`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann export-images [OPTIONS] --dest <DIR> <IDS>...
```

Arguments:

| Argument | Description |
| --- | --- |
| `<IDS>...` | Capture ids of the screenshots (at most 500) |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--dest` | `<DIR>` |  |  | The folder to write to (created when missing) |
| `--longest` | `<PX>` |  |  | Size: the longest side in px |
| `--width` | `<PX>` |  |  | Size: the width in px |
| `--height` | `<PX>` |  |  | Size: the height in px |
| `--percent` | `<P>` |  |  | Size: a percentage of the original (1 to 400) |
| `--allow-larger` | switch |  |  | Allow a picture to become larger than the original |
| `--format` | `<FORMAT>` | png, jpeg, webp |  | png (default), jpeg or webp (lossless) |
| `--quality` | `<QUALITY>` |  |  | JPEG quality, 60 to 100 |
| `--watermark-text` | `<TEXT>` |  |  | A text watermark |
| `--watermark-image` | `<FILE>` |  |  | A PNG or JPEG watermark |
| `--watermark-position` | `<WHERE>` |  |  | top_left, top, top_right, left, center, right, bottom_left, bottom or bottom_right |
| `--watermark-opacity` | `<O>` |  |  | 0.05 to 1 |
| `--watermark-size` | `<S>` |  |  | The mark's width as a fraction of the picture, 0.05 to 1 |
| `--blueprint` | switch |  |  | Replace every text line of each picture with a bar in the exported files (on-device text recognition; the screenshots are not changed) |
| `--name-pattern` | `<PATTERN>` |  |  | File names: {name}, {date}, {w} and {h}, and the running number as the letter n in braces (default {name}). Help text cannot print that token literally |
| `--wait` | switch |  |  | Wait for the job and print what was exported and skipped |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (exit 6; the job keeps running) |

Examples:

```text
$ capturemann export-images shot_06GYEY0TM4JZ7KGH --dest exported --longest 320 --format jpeg --quality 85 --wait
exported /home/me/cm-demo/exported/demo.jpg
1 exported
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann library`

Browse the library (`list_library`)

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann library [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann library list`: List captures, newest first
- `capturemann library search`: Full-text search (same as `list --query`)
- `capturemann library rename`: Rename a capture: sets its title and renames its folder (`rename_capture`)
- `capturemann library rm`: Move a capture to the Trash (`delete_capture`). The user must approve on screen. With `--also-remote` the dialog lists the remote copies that are deleted first
- `capturemann library move`: Change the folder the library lives in (`move_library`). The user must approve on screen (the dialog names the old and the new folder and the size); nothing is deleted from the old folder. The app...

#### `capturemann library list`

List captures, newest first

- **MCP tool:** `list_library`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann library list [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--query` | `<QUERY>` |  |  | Search text (title match) |
| `--kind` | `<KIND>` | screenshot, recording, audio, guide |  | Only `screenshot`, `recording` (screen), `audio` or `guide` (Click Guides) |
| `--status` | `<STATUS>` | uploaded, not_uploaded, published, not_published |  | Only captures with or without upload or publish records |
| `--limit` | `<LIMIT>` |  |  | Page size (1 to 200, default 50) |
| `--cursor` | `<CURSOR>` |  |  | `next_cursor` of the previous page |

Examples:

```text
$ capturemann library list
shot_06GYEY0TM4JZ7KGH  screenshot  640x400  2026-11-17T03:06:10.480342Z  demo
shot_06GYEY0VP3EECBAH  screenshot  640x400  2026-11-17T03:06:10.747516Z  demo
```

```text
$ capturemann --json library list --limit 1
{
  "captures": [
    {
      "id": "shot_06GYEY0TM4JZ7KGH",
      "title": "demo",
      "kind": "screenshot",
      "created": "2026-11-17T03:06:10.480342Z",
      "duration": null,
      "width": 640,
      "height": 400,
      "size_bytes": 18114,
      "broken": false,
      "uploads": [],
      "publishes": [],
      "source": {
        "path": "/home/me/cm-demo/demo.png",
        "imported_at": "2026-11-17T03:06:10.480342Z",
        "original_name": "demo.png"
      }
    }
  ],
  "next_cursor": "1"
}
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann library search`

Full-text search (same as `list --query`)

- **MCP tool:** `list_library`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann library search [OPTIONS] <TEXT>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<TEXT>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--kind` | `<KIND>` | screenshot, recording, audio, guide |  | Only `screenshot`, `recording` (screen), `audio` or `guide` (Click Guides) |
| `--status` | `<STATUS>` | uploaded, not_uploaded, published, not_published |  | Only captures with or without upload or publish records |
| `--limit` | `<LIMIT>` |  |  | Page size (1 to 200, default 50) |
| `--cursor` | `<CURSOR>` |  |  | `next_cursor` of the previous page |

Examples:

```text
$ capturemann library search demo
shot_06GYEY0TM4JZ7KGH  screenshot  640x400  2026-11-17T03:06:10.480342Z  demo
shot_06GYEY0VP3EECBAH  screenshot  640x400  2026-11-17T03:06:10.747516Z  demo
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann library rename`

Rename a capture: sets its title and renames its folder (`rename_capture`)

- **MCP tool:** `rename_capture`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann library rename [OPTIONS] <ID> <TITLE>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id |
| `<TITLE>` | The new title |

Examples:

```text
$ capturemann library rename shot_06GYEY0VP3EECBAH After
Renamed shot_06GYEY0VP3EECBAH to "After"
  library: /home/me/cm-demo/lib/After.cmshot
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann library rm`

Move a capture to the Trash (`delete_capture`). The user must approve on screen. With `--also-remote` the dialog lists the remote copies that are deleted first

- **MCP tool:** `delete_capture`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann library rm [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--also-remote` | switch |  |  | Also delete the remote copies the capture records, PERMANENTLY: Google Drive files and folders, copies in local destination folders, YouTube videos. If one step fails the capture is kept and the error says what was and was not deleted |

Examples:

```text
$ capturemann library rm shot_06GYEY0VP3EECBAH
error[user_declined]: No confirmation UI is attached to this daemon
  reason: no_confirmer   (stderr)
(exit 3)
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann library move`

Change the folder the library lives in (`move_library`). The user must approve on screen (the dialog names the old and the new folder and the size); nothing is deleted from the old folder. The app uses the new folder after it restarts. Prints the job id; `--wait` blocks until the copy ends

- **MCP tool:** `move_library`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann library move [OPTIONS] <PATH>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<PATH>` | The new library folder (relative paths and `~` are resolved here; the folder is created if it does not exist) |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--fresh` | switch |  |  | Start an empty library there instead of copying (the old files stay where they are) |
| `--use-existing` | switch |  |  | The folder already holds a CaptureMann library: switch to it without copying |
| `--wait` | switch |  |  | Wait for the copy to finish (exit 6 if it fails or is cancelled) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (exit 6; the job keeps running) |

Examples:

Not run here (syntax only):

```sh
capturemann library move /Volumes/Big/CaptureMann --wait
```

### `capturemann settings`

Read and change settings (`get_settings`, `set_settings`)

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann settings [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann settings get`: Print settings (all, or the given dotted keys)
- `capturemann settings set`: Set `KEY VALUE` pairs; a value is JSON when it parses (60, true), else a string

#### `capturemann settings get`

Print settings (all, or the given dotted keys)

- **MCP tool:** `get_settings`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann settings get [OPTIONS] [KEYS]...
```

Arguments:

| Argument | Description |
| --- | --- |
| `[KEYS]...` | See the synopsis. |

Examples:

```text
$ capturemann settings get
recording.fps = 30
```

```text
$ capturemann settings get ui.language
error[not_found]: setting `ui.language` is not set   (stderr)
(exit 2)
```

```text
$ capturemann --json settings get recording.fps
{
  "settings": {
    "recording.fps": 30
  }
}
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann settings set`

Set `KEY VALUE` pairs; a value is JSON when it parses (60, true), else a string

- **MCP tool:** `set_settings`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann settings set [OPTIONS] <KEY VALUE>...
```

Arguments:

| Argument | Description |
| --- | --- |
| `<KEY VALUE>...` | See the synopsis. |

Examples:

```text
$ capturemann settings set recording.fps 30
recording.fps = 30
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann record`

Screen recording: start, pause, resume, stop, status (`start_recording`, ...)

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann record [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann record start`: Start a recording (`start_recording`). Asks for confirmation in the CaptureMann window and waits (up to 5 minutes) for the answer and for the countdown, unless the user turned the confirmation off...
- `capturemann record pause`: Pause the running recording (`pause_recording`)
- `capturemann record resume`: Resume a paused recording (`resume_recording`)
- `capturemann record mic`: Mute or unmute the microphone of the running recording without stopping it (`set_mic_muted`). Muted time is digital silence in the mic files and is listed in the manifest. Works while recording or...
- `capturemann record marker`: Add a marker (a moment to come back to) to the running recording at the current time (`add_marker`), or to a finished recording with `--capture` and `--at`. Markers are kept in recorded time: a pause...
- `capturemann record stop`: Stop and save the recording (`stop_recording`). Also cancels a countdown and a running 60 fps self-test
- `capturemann record status`: Show the state of the recording: elapsed time, tracks, mic level, disk (`get_recording_status`)
- `capturemann record selftest`: Test whether this machine records 60 fps without late frames (`run_fps_selftest`): records about 10 s of the screen, measures late and dropped frames, deletes the video and keeps only the result. 60...
- `capturemann record watch`: Stream `recording.*` events until interrupted

#### `capturemann record start`

Start a recording (`start_recording`). Asks for confirmation in the CaptureMann window and waits (up to 5 minutes) for the answer and for the countdown, unless the user turned the confirmation off for AI agents in Settings

- **MCP tool:** `start_recording`, Confirm, asks on screen
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann record start [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--audio` | switch |  |  | Record audio only (microphone and/or system audio; no screen, camera or cursor): the result is an audio recording in the library. Use `--mic <ID>` for a specific microphone, `--system-audio` for system audio and `--preset` for a use case |
| `--preset` | `<PRESET>` | room, online, lecture, product, note |  | Use case of an audio recording: `room` (interview, 1:1, meeting around a laptop), `online` (call: adds system audio), `lecture`, `product` (demo call: adds system audio) or `note` (voice note) |
| `--people` | `<N>` |  |  | How many people speak (1 to 8), for an audio recording: stored with it and used by the automatic speaker step. Leave it out when you do not know |
| `--display` | `<ID>` |  |  | Display id to record (see `targets`; default: the primary display) |
| `--window` | `<ID\|TITLE>` |  |  | Record one window: its id, or a case-insensitive substring of its title or app name. An exact title wins; several partial matches fail with `ambiguous_target` and list their ids |
| `--region` | `<X,Y,W,H>` |  |  | Record a region `x,y,w,h` in physical px, relative to the display |
| `--size` | `<SIZE>` | source, 2160p, 1440p, 1080p, 720p |  | Output size |
| `--fps` | `<FPS>` | 30, 60 |  | Frame rate |
| `--codec` | `<CODEC>` | auto, hevc, h264 |  | Video codec (default: HEVC above 1920 px, else H.264) |
| `--quality` | `<QUALITY>` | high, balanced, small |  | Bitrate preset |
| `--camera` | `[<on\|off\|ID>]` |  |  | Record the webcam: `on` (the camera chosen in Settings > Webcam & audio if it is connected, else the system default), `off`, or a camera id (see `devices`) |
| `--camera-resolution` | `<CAMERA_RESOLUTION>` | 720p, 1080p |  | Camera resolution |
| `--camera-shape` | `<CAMERA_SHAPE>` | circle, rounded |  | Webcam bubble shape |
| `--camera-size` | `<CAMERA_SIZE>` | s, m, l |  | Webcam bubble size (small 160, medium 240, large 320 px) |
| `--camera-pos` | `<POSITION>` |  |  | Webcam bubble position (default `bottom-left`): `bottom-right`, `top-left`, ... or `X,Y` as fractions 0..1 |
| `--no-camera-mirror` | switch |  |  | Store the camera display mirror flag off (the recorded pixels are never flipped) |
| `--mic` | `[<default\|off\|ID>]` |  |  | Record the microphone: `default` (the microphone chosen in Settings > Webcam & audio if it is connected, else the system default input), `off`, or a microphone id (see `devices`) |
| `--no-denoise` | switch |  |  | Do not write the noise-suppressed microphone track |
| `--system-audio` | switch |  |  | Also record system audio |
| `--no-cursor` | switch |  |  | Do not record cursor telemetry (the pointer stays in the video) |
| `--cursor-separate` | switch |  |  | Record the cursor separately: it is left out of the video and drawn from the telemetry, so the editor can restyle it (`edit cursor`). Default: the `recording.cursor_separate` setting |
| `--countdown` | `<SECONDS>` |  |  | Countdown before recording, 0 to 10 seconds (default: the `recording.countdown_s` setting, 3) |
| `--max-duration` | `<SECONDS>` |  |  | Stop automatically after this many seconds of recorded time |
| `--title` | `<TITLE>` |  |  | Library title (default: `Recording yyyy-MM-dd HH.mm`) |
| `--exclude-app` | `<BUNDLE_ID>` |  |  | Keep this app (bundle id) out of the recording, in addition to the defaults; repeatable |
| `--exclude-window` | `<ID>` |  |  | Keep this window id out of the recording; repeatable |

Examples:

Not run here (syntax only):

```sh
capturemann record start --display 1 --size 1080p --fps 30 --mic default --countdown 3
capturemann record start --audio --preset online --countdown 0
```

#### `capturemann record pause`

Pause the running recording (`pause_recording`)

- **MCP tool:** `pause_recording`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann record pause [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--id` | `<RECORDING_ID>` |  |  | Recording id (default: the running recording) |

Examples:

Not run here (syntax only):

```sh
capturemann record pause
```

#### `capturemann record resume`

Resume a paused recording (`resume_recording`)

- **MCP tool:** `resume_recording`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann record resume [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--id` | `<RECORDING_ID>` |  |  | Recording id (default: the running recording) |

Examples:

Not run here (syntax only):

```sh
capturemann record resume
```

#### `capturemann record mic`

Mute or unmute the microphone of the running recording without stopping it (`set_mic_muted`). Muted time is digital silence in the mic files and is listed in the manifest. Works while recording or paused, on a recording that has a microphone track

- **MCP tool:** `set_mic_muted`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann record mic [OPTIONS] <--mute|--unmute>
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--mute` | switch |  |  | Mute the microphone |
| `--unmute` | switch |  |  | Unmute the microphone |
| `--id` | `<RECORDING_ID>` |  |  | Recording id (default: the running recording) |

Examples:

Not run here (syntax only):

```sh
capturemann record mic --mute
```

#### `capturemann record marker`

Add a marker (a moment to come back to) to the running recording at the current time (`add_marker`), or to a finished recording with `--capture` and `--at`. Markers are kept in recorded time: a pause does not move them

- **MCP tool:** `add_marker`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann record marker [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--label` | `<LABEL>` |  |  | A short label for the marker |
| `--id` | `<RECORDING_ID>` |  |  | Recording id (default: the running recording) |
| `--capture` | `<CAPTURE_ID>` |  |  | A finished recording to mark (needs `--at`) |
| `--at` | `<SECONDS>` |  |  | Seconds of recorded time for a finished recording (pauses removed) |

Examples:

Not run here (syntax only):

```sh
capturemann record marker --label "Decision"
```

#### `capturemann record stop`

Stop and save the recording (`stop_recording`). Also cancels a countdown and a running 60 fps self-test

- **MCP tool:** `stop_recording`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann record stop [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--id` | `<RECORDING_ID>` |  |  | Recording id (default: the running recording) |

Examples:

Not run here (syntax only):

```sh
capturemann record stop
```

#### `capturemann record status`

Show the state of the recording: elapsed time, tracks, mic level, disk (`get_recording_status`)

- **MCP tool:** `get_recording_status`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann record status [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--id` | `<RECORDING_ID>` |  |  | Recording id (default: the running recording) |

Examples:

```text
$ capturemann record status
No recording is running.
```

```text
$ capturemann --json record status
{
  "state": "idle",
  "recording_id": null,
  "selftest": null,
  "last": null
}
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann record selftest`

Test whether this machine records 60 fps without late frames (`run_fps_selftest`): records about 10 s of the screen, measures late and dropped frames, deletes the video and keeps only the result. 60 fps is accepted by `record start --fps 60` only after a pass. Asks for confirmation in the CaptureMann window when run by an agent

- **MCP tool:** `run_fps_selftest`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann record selftest [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--display` | `<ID>` |  |  | Display id to test (see `targets`; default: the primary display) |
| `--size` | `<SIZE>` | source, 2160p, 1440p, 1080p, 720p |  | Output size to test (default: the `recording.size` setting, else source) |
| `--codec` | `<CODEC>` | auto, hevc, h264 |  | Video codec to test (default: the `recording.codec` setting, else auto) |

Examples:

Not run here (syntax only):

```sh
capturemann record selftest --display 1 --size 1440p
```

#### `capturemann record watch`

Stream `recording.*` events until interrupted

- **MCP tool:** none; the MCP server forwards the same events as notifications (see [mcp.md](mcp.md#events)).

Synopsis:

```text
capturemann record watch [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--topic` | `<TOPIC>` |  |  | Event topics (default `recording.*`) |
| `--max-events` | `<N>` |  |  | Stop after this many events (for scripts) |

Examples:

Not run here (syntax only):

```sh
capturemann record watch --max-events 5
```

### `capturemann guide`

Click Guide (`start_guide`, `stop_guide`, `get_guide`, `set_guide`, `export_guide`): click through a task once and get a numbered guide with a screenshot of each click **(Preview)**

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann guide [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann guide start`: Start a Click Guide (`start_guide`): every mouse click in the target becomes a step with a screenshot. Asks for approval on screen when an agent calls it; never records keystrokes. Needs Input...
- `capturemann guide stop`: Stop the recording guide (`stop_guide`) and print it **(Preview)**
- `capturemann guide get`: Print a guide: its steps, titles, notes and click points (`get_guide`) **(Preview)**
- `capturemann guide set`: Change a guide (`set_guide`): title, intro, numbering, the order, titles and notes of the steps, deletions, duplicates and new note steps. One call is one undo step (`undo <id>`) **(Preview)**
- `capturemann guide export`: Export a guide (`export_guide`): a Markdown folder, one HTML file, or rich text on the clipboard **(Preview)**

#### `capturemann guide start`

Start a Click Guide (`start_guide`): every mouse click in the target becomes a step with a screenshot. Asks for approval on screen when an agent calls it; never records keystrokes. Needs Input Monitoring on macOS. Stop it with `guide stop` **(Preview)**

- **MCP tool:** `start_guide`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).
- **Preview:** yes, this is a Preview feature (it may still change).

Synopsis:

```text
capturemann guide start [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--display` | `<ID>` |  |  | Display id to follow (see `targets`; default: the primary display) |
| `--window` | `<ID\|TITLE>` |  |  | Follow one window: its id, or a case-insensitive substring of its title or app name. An exact title wins; several partial matches fail with `ambiguous_target` and list their ids |
| `--region` | `<X,Y,W,H>` |  |  | Follow a region `x,y,w,h` in physical px, relative to the display |
| `--title` | `<TITLE>` |  |  | The guide's title (default: `Click Guide <date time>`) |
| `--countdown` | `<SECONDS>` |  |  | Seconds before the first click is listened to (0 to 10, default 3) |

Examples:

Not run here (syntax only):

```sh
capturemann guide start --window "Settings" --title "Change the theme" --countdown 3
```

#### `capturemann guide stop`

Stop the recording guide (`stop_guide`) and print it **(Preview)**

- **MCP tool:** `stop_guide`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.
- **Preview:** yes, this is a Preview feature (it may still change).

Synopsis:

```text
capturemann guide stop [OPTIONS] [ID]
```

Arguments:

| Argument | Description |
| --- | --- |
| `[ID]` | The guide (default: the one recording) |

Examples:

Not run here (syntax only):

```sh
capturemann guide stop
```

#### `capturemann guide get`

Print a guide: its steps, titles, notes and click points (`get_guide`) **(Preview)**

- **MCP tool:** `get_guide`, Free, read-only
- **Asks on screen:** No (read-only).
- **Preview:** yes, this is a Preview feature (it may still change).

Synopsis:

```text
capturemann guide get [OPTIONS] [ID]
```

Arguments:

| Argument | Description |
| --- | --- |
| `[ID]` | The guide (default: the one recording) |

Examples:

Not run here (syntax only):

```sh
capturemann guide get guide_REGRESSFIXTURE01
```

#### `capturemann guide set`

Change a guide (`set_guide`): title, intro, numbering, the order, titles and notes of the steps, deletions, duplicates and new note steps. One call is one undo step (`undo <id>`) **(Preview)**

- **MCP tool:** `set_guide`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.
- **Preview:** yes, this is a Preview feature (it may still change).

Synopsis:

```text
capturemann guide set [OPTIONS] [ID]
```

Arguments:

| Argument | Description |
| --- | --- |
| `[ID]` | The guide (default: the one recording) |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--title` | `<TITLE>` |  |  | (no help text in `--help`) |
| `--intro` | `<INTRO>` |  |  | The intro paragraph |
| `--numbering` | `<NUMBERING>` | number, upper, lower |  | Badge numbering: `number` (1 2 3), `upper` (A B C) or `lower` (a b c) |
| `--steps` | `<JSON\|@FILE\|->` |  |  | The new step list as JSON, `@file` or `-` (stdin): `[{"id":"step_2"},{"id":"step_1","title":"Open Settings","note":"..."},{"id":"step_3","delete":true},{"title":"Type your password"}]`. An entry with no `id` is a new note step; steps not listed keep their order after the listed ones |
| `--pause` | switch |  |  | While recording: pause |
| `--resume` | switch |  |  | While recording: resume |
| `--note-step` | `<TITLE>` |  |  | Add a note step (a step with no image) with this title |
| `--note` | `<TEXT>` |  |  | With `--note-step`: the note under the title |
| `--remove-last` | switch |  |  | While recording: remove the newest step |
| `--focus-crop` | `<on\|off>` | on, off |  | Turn the focus crop `on` or `off` for the steps named with `--crop-step` |
| `--crop-step` | `<STEP>` |  |  | A step for `--focus-crop` (repeatable) |
| `--retake` | `<STEP>` |  |  | Take a fresh frame of the followed target for this step (repeatable) |

Examples:

Not run here (syntax only):

```sh
capturemann guide set guide_REGRESSFIXTURE01 --title "Change the theme" --numbering upper
```

#### `capturemann guide export`

Export a guide (`export_guide`): a Markdown folder, one HTML file, or rich text on the clipboard **(Preview)**

- **MCP tool:** `export_guide`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.
- **Preview:** yes, this is a Preview feature (it may still change).

Synopsis:

```text
capturemann guide export [OPTIONS] --format <FORMAT> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | The guide |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--format` | `<FORMAT>` | markdown, html, pdf, rich-text |  | `markdown` (a folder with guide.md and images/), `html` (one file), `pdf` (one file) or `rich-text` (clipboard) |
| `--dest` | `<PATH>` |  |  | Where to write: a folder for markdown, a file or folder for html and pdf. Not used by rich-text |
| `--headings` | `<HEADINGS>` | numbered, step, title-only |  | Step headings: `numbered` (`1. Title`), `step` (`Step 1 · Title`) or `title-only` |
| `--paper` | `<PAPER>` | a4, letter |  | PDF paper size (default `a4`) |
| `--image-scale` | `<IMAGE_SCALE>` | 1, 2 |  | Image size: 1 (at most 800 px wide) or 2 (1600 px, never larger than the frame) |
| `--no-intro` | switch |  |  | Leave out the intro |
| `--no-notes` | switch |  |  | Leave out the step notes |
| `--no-note-steps` | switch |  |  | Leave out the note steps that have no image |

Examples:

Not run here (syntax only):

```sh
capturemann guide export guide_REGRESSFIXTURE01 --format markdown --dest ~/Documents/guides
```

### `capturemann text`

Print the text in a screenshot (`recognize_text`, read-only, on this computer only): plain text in reading order from the rendered image (annotations and redactions applied, so blurred text is not read). `--region` limits it; `--copy` puts it on the clipboard

- **MCP tool:** `recognize_text`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann text [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a screenshot |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--region` | `<X,Y,W,H>` |  |  | Only this part of the image: `x,y,w,h` in pixels of the original image |
| `--copy` | switch |  |  | Also put the text on the clipboard (only when text was found) |
| `--lines` | switch |  |  | Print each line with its box and confidence instead of the plain text (`--json` always has the text; with this flag it has the lines too) |

Examples:

```text
$ capturemann text shot_06GYEY0TM4JZ7KGH
Step 1
```

```text
$ capturemann text shot_06GYEY0TM4JZ7KGH --lines
[56, 201, 89, 34]  1.00  Step 1
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann devices`

List cameras, microphones and system-audio support (`list_devices`)

- **MCP tool:** `list_devices`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann devices [OPTIONS]
```

Examples:

Not run here (syntax only):

```sh
capturemann devices
```

### `capturemann diagnostics`

Print the diagnostics report to paste into an issue (`get_diagnostics`): versions, system, permissions, models, a few settings, library counts and the last warnings. Nothing is sent

- **MCP tool:** `get_diagnostics`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann diagnostics [OPTIONS]
```

Examples:

```text
$ capturemann diagnostics
## App
- CaptureMann 0.3.4
- started from a plain binary
- config folder /home/me/cm-demo/cfg
- log folder /home/me/cm-demo/cfg/logs

## System   (lines left out here)
## Permissions   (lines left out here)
## Models and engines   (lines left out here)
## Settings   (lines left out here)
## Library
- 3 captures, 0 MB
- 0 unreadable
- free space on the library volume 42 GB

## Last recording
- no recording yet

## Last runs
- (none)

## Warnings and errors (newest last)
- 2026-11-17T03:06:10.437052Z daemon WARN cm_daemon: cm-daemon listening on /tmp/cmref/d.sock (library /home/me/cm-demo/lib)
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann cursor`

Print the pointer path and clicks of a recording (`get_cursor_track`): moves, button down/up, scroll direction, in physical pixels of the screen track. Times are seconds or `t:<ticks>` (session time); a long track is read in pages (`--limit`, `next_from`)

- **MCP tool:** `get_cursor_track`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann cursor [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--from` | `<FROM>` |  |  | Start of the range, inclusive (seconds or `t:<ticks>`) |
| `--to` | `<TO>` |  |  | End of the range, exclusive (seconds or `t:<ticks>`) |
| `--limit` | `<LIMIT>` |  |  | Most events to print (default 5000, at most 50000) |

Examples:

```text
$ capturemann cursor shot_06GYEY0TM4JZ7KGH
error[invalid_input]: `shot_06GYEY0TM4JZ7KGH` is a screenshot; this works on recordings   (stderr)
(exit 2)
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann platform-rules`

Print the limits, required fields, privacy options and export presets of a publishing platform (`get_platform_rules`). Today: `youtube`

- **MCP tool:** `get_platform_rules`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann platform-rules [OPTIONS] <PLATFORM>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<PLATFORM>` | The platform: `youtube` |

Examples:

```text
$ capturemann platform-rules youtube
youtube
  max file size: 274877906944 bytes
  max duration: 43200 s
  video files: mp4, m4v, mov, webm, mkv, avi
  title: up to 100 characters
  description: up to 5000 bytes
  tags: up to 500 characters in all
  thumbnail: up to 2097152 bytes (jpg, jpeg, png)
  uploads per day: 100
  required: title, made_for_kids
  privacy: private, unlisted, public (default unlisted)
  recommended preset: youtube-1080p
    youtube-4k  YouTube 4K
    youtube-1440p  YouTube 1440p
    youtube-1080p  YouTube 1080p
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann destinations`

List the destinations set up in Settings (`list_destinations`)

- **MCP tool:** `list_destinations`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann destinations [OPTIONS]
```

Examples:

```text
$ capturemann destinations
No destinations. Add one in Settings > Destinations.
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann upload`

Copy a capture or a file to a destination in the background (`upload`). Prints the job id

- **MCP tool:** `upload`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann upload [OPTIONS] [CAPTURE_ID]
```

Arguments:

| Argument | Description |
| --- | --- |
| `[CAPTURE_ID]` | Capture id (or use `--file`) |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--file` | `<FILE>` |  |  | Upload this file instead of a capture |
| `--to` | `<TO>` |  |  | Destination id (see `destinations`); the default destination when absent |
| `--folder` | `<FOLDER>` |  |  | Folder inside the destination; replaces its folder pattern |
| `--what` | `<WHAT>` | export, original, both |  | What to send of a capture: `export`, `original` or `both` |

Examples:

Not run here (syntax only):

```sh
capturemann upload rec_06GYEVMCKZSRHR46 --to gdrive_main --what export
capturemann upload --file ~/notes.pdf
```

### `capturemann publish`

Publish a recording to YouTube (`publish_video`), or list the connected accounts (`publish accounts`). The app asks you to approve every publish; nothing is sent before you do. Prints the job id

- **MCP tool:** `publish_video`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann publish [OPTIONS] [CAPTURE_ID]
capturemann publish <COMMAND>
```

Subcommands:

- `capturemann publish accounts`: List the connected publish accounts with today's upload count (`list_publish_accounts`)

Arguments:

| Argument | Description |
| --- | --- |
| `[CAPTURE_ID]` | Capture id of the recording to publish |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--platform` | `<PLATFORM>` |  | youtube | Platform; only `youtube` is built |
| `--account` | `<ACCOUNT>` |  |  | Account id from `publish accounts`; the default account when absent |
| `--title` | `<TITLE>` |  |  | Video title (required, at most 100 characters) |
| `--description` | `<DESCRIPTION>` |  |  | (no help text in `--help`) |
| `--tags` | `<TAGS>` |  |  | Tags, comma separated |
| `--privacy` | `<PRIVACY>` | private, unlisted, public |  | `private`, `unlisted` or `public`; the account's default (never public) when absent |
| `--made-for-kids` | `<MADE_FOR_KIDS>` | yes, no |  | Is the video made for kids? Required by YouTube; there is no default |
| `--thumbnail` | `<THUMBNAIL>` |  |  | Absolute path of a JPG or PNG thumbnail (up to 2 MB) |
| `--export-preset` | `<EXPORT_PRESET>` |  |  | Export with this preset first (for example `youtube-1080p`); the newest export otherwise |
| `--captions` | switch |  |  | Also upload the captions as an SRT track |
| `--category` | `<CATEGORY>` |  |  | YouTube category id (28, Science & Technology, when absent) |
| `--playlist` | `<PLAYLIST>` |  |  | (no help text in `--help`) |

Examples:

Not run here (syntax only):

```sh
capturemann publish rec_06GYEVMCKZSRHR46 --title "Kubernetes probes in 5 minutes" --made-for-kids no --privacy unlisted
```

#### `capturemann publish accounts`

List the connected publish accounts with today's upload count (`list_publish_accounts`)

- **MCP tool:** `list_publish_accounts`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann publish accounts [OPTIONS]
```

Examples:

```text
$ capturemann publish accounts
No publish accounts. Connect one in Settings > Destinations.
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann mcp`

Run the MCP server (the `cm-mcp` binary next to this one): stdio by default, local HTTP with `--http`. Stdout is the MCP channel, so nothing else is printed there

- **MCP tool:** none (a local command, not a daemon action).

Synopsis:

```text
capturemann mcp [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--http` | switch |  |  | Serve Streamable HTTP on 127.0.0.1 with a bearer token instead of stdio |
| `--port` | `<N>` |  |  | Port for `--http` (default: a random free port) |

Examples:

Not run here (syntax only):

```sh
capturemann mcp
capturemann mcp --http --port 8765
```

### `capturemann daemon`

Manage the background daemon

- **MCP tool:** none (a local command, not a daemon action).

Synopsis:

```text
capturemann daemon [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann daemon start`: Make sure a daemon is running (starts a headless one if needed)
- `capturemann daemon status`: Report whether a daemon is running (exit 5, `daemon_unavailable`, if not)

#### `capturemann daemon start`

Make sure a daemon is running (starts a headless one if needed)

- **MCP tool:** none (a local command, not a daemon action).

Synopsis:

```text
capturemann daemon start [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--library` | `<DIR>` |  |  | Library folder for a daemon started by this command Env: `CAPTUREMANN_LIBRARY`. |

Examples:

```text
$ capturemann daemon start
Daemon already running at /tmp/cmref/d.sock
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann daemon status`

Report whether a daemon is running (exit 5, `daemon_unavailable`, if not)

- **MCP tool:** none (a local command, not a daemon action).

Synopsis:

```text
capturemann daemon status [OPTIONS]
```

Examples:

```text
$ capturemann daemon status
Daemon running at /tmp/cmref/d.sock (97 actions)
```

The output above was run against a headless daemon with an empty throw-away library.

### `capturemann edit`

Edit a recording without touching its files: trim, cut, split, speed, background (`trim`, `cut_segments`, `restore_segments`, `split`, `set_speed`, `set_background`, `set_audio_mix`). Times are seconds (`12.5`) or ticks (`t:1500000`); each command is one undo step

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann edit [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann edit trim`: Keep only START..END (`trim`)
- `capturemann edit cut`: Cut ranges like `10-15 40-42` (`cut_segments`); all are one undo step
- `capturemann edit restore`: Bring cut ranges back (`restore_segments`)
- `capturemann edit split`: Add a segment boundary (`split`)
- `capturemann edit speed`: Play a range like `30-50` at `--x` times normal speed, 0.25 to 8 (`set_speed`)
- `capturemann edit background`: Frame the screen on a background (`set_background`). Options left out keep their value
- `capturemann edit audio`: Balance the mic and system audio (`set_audio_mix`). Options left out keep their value
- `capturemann edit camera`: Set how the screen and the camera share the frame over a range like `0-15` (`set_camera_layout`). A later range overwrites the layouts it overlaps
- `capturemann edit zoom`: Add a zoom region (`set_zooms`): starts at `--at`, lasts `--length`, keeps `--focus` in the middle. A new region shortens the ones it overlaps
- `capturemann edit cursor`: Style the telemetry cursor (`set_cursor_style`). Options left out keep their value
- `capturemann edit reframe`: Reframe for a vertical or square export (`set_reframe`)

#### `capturemann edit trim`

Keep only START..END (`trim`)

- **MCP tool:** `trim`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit trim [OPTIONS] --start <START> --end <END> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--start` | `<START>` |  |  | (no help text in `--help`) |
| `--end` | `<END>` |  |  | (no help text in `--help`) |

Examples:

```text
$ capturemann edit trim shot_06GYEY0TM4JZ7KGH --start 1 --end 2
error[invalid_input]: `shot_06GYEY0TM4JZ7KGH` is a screenshot: trim edits recordings (use `annotate` for screenshots)   (stderr)
(exit 2)
```

The output above was run against a headless daemon with an empty throw-away library.

Not run here (syntax only):

```sh
capturemann edit trim rec_06GYEVMCKZSRHR46 --start 2.5 --end 118
```

#### `capturemann edit cut`

Cut ranges like `10-15 40-42` (`cut_segments`); all are one undo step

- **MCP tool:** `cut_segments`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit cut [OPTIONS] <ID> <START-END>...
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |
| `<START-END>...` | See the synopsis. |

Examples:

Not run here (syntax only):

```sh
capturemann edit cut rec_06GYEVMCKZSRHR46 10-15 40-42
```

#### `capturemann edit restore`

Bring cut ranges back (`restore_segments`)

- **MCP tool:** `restore_segments`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit restore [OPTIONS] <ID> <START-END>...
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |
| `<START-END>...` | See the synopsis. |

Examples:

Not run here (syntax only):

```sh
capturemann edit restore rec_06GYEVMCKZSRHR46 10-15
```

#### `capturemann edit split`

Add a segment boundary (`split`)

- **MCP tool:** `split`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit split [OPTIONS] --at <AT> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--at` | `<AT>` |  |  | (no help text in `--help`) |

Examples:

Not run here (syntax only):

```sh
capturemann edit split rec_06GYEVMCKZSRHR46 --at 60
```

#### `capturemann edit speed`

Play a range like `30-50` at `--x` times normal speed, 0.25 to 8 (`set_speed`)

- **MCP tool:** `set_speed`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit speed [OPTIONS] --x <FACTOR> <ID> <START-END>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |
| `<START-END>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--x` | `<FACTOR>` |  |  | The speed factor |
| `--keep-audio` | switch |  |  | Keep the audio above 2x (it is muted by default) |

Examples:

Not run here (syntax only):

```sh
capturemann edit speed rec_06GYEVMCKZSRHR46 30-50 --x 2
```

#### `capturemann edit background`

Frame the screen on a background (`set_background`). Options left out keep their value

- **MCP tool:** `set_background`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit background [OPTIONS] --kind <KIND> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--kind` | `<KIND>` | none, color, gradient, image |  | (no help text in `--help`) |
| `--value` | `<VALUE>` |  |  | `#rrggbb`, two colors `#rrggbb,#rrggbb` for a gradient, or an image path |
| `--padding` | `<PADDING>` |  |  | Padding, percent of the frame (0 to 20) |
| `--radius` | `<RADIUS>` |  |  | Screen corner radius in px (0 to 24) |
| `--shadow` | `<SHADOW>` | true, false |  | Draw a shadow under the screen |

Examples:

Not run here (syntax only):

```sh
capturemann edit background rec_06GYEVMCKZSRHR46 --kind color --value "#1E1E2E" --padding 6 --radius 12 --shadow true
```

#### `capturemann edit audio`

Balance the mic and system audio (`set_audio_mix`). Options left out keep their value

- **MCP tool:** `set_audio_mix`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit audio [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--mic-gain` | `<DB>` |  |  | Mic level in dB, -96 to 12 |
| `--system-gain` | `<DB>` |  |  | System-audio level in dB, -96 to 12 |
| `--mic-mute` | `<MIC_MUTE>` | true, false |  | Mute the mic |
| `--system-mute` | `<SYSTEM_MUTE>` | true, false |  | Mute the system audio |
| `--denoise` | `<DENOISE>` | true, false |  | Use the denoised mic file (true) or the raw one (false) |

Examples:

Not run here (syntax only):

```sh
capturemann edit audio rec_06GYEVMCKZSRHR46 --mic-gain 3 --denoise true
```

#### `capturemann edit camera`

Set how the screen and the camera share the frame over a range like `0-15` (`set_camera_layout`). A later range overwrites the layouts it overlaps

- **MCP tool:** `set_camera_layout`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit camera [OPTIONS] --layout <LAYOUT> <ID> <START-END>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |
| `<START-END>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--layout` | `<LAYOUT>` | bubble, screen, camera, side_by_side |  | `bubble` (screen with the camera bubble), `screen` (no camera), `camera` (camera full frame) or `side_by_side` (screen 2/3, camera 1/3) |
| `--shape` | `<SHAPE>` | circle, rounded |  | Bubble shape (`bubble` only) |
| `--rect` | `<X,Y,W,H>` |  |  | Bubble `x,y,w,h` as fractions of the frame, w from 0.05 to 1 (`bubble` only) |
| `--ring` | `<RING>` |  |  | Ring width as a fraction of the bubble (0 to 0.5), or `none` (`bubble` only) |
| `--mirror` | `<MIRROR>` | true, false |  | Mirror the camera (true) or not (false) |
| `--smooth` | switch |  |  | Move smoothly from the layout before over 300 ms instead of switching at once |

Examples:

Not run here (syntax only):

```sh
capturemann edit camera rec_06GYEVMCKZSRHR46 0-15 --layout bubble --shape circle --smooth
```

#### `capturemann edit zoom`

Add a zoom region (`set_zooms`): starts at `--at`, lasts `--length`, keeps `--focus` in the middle. A new region shortens the ones it overlaps

- **MCP tool:** `set_zooms`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit zoom [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--at` | `<AT>` |  |  | When the zoom starts, in seconds or `t:<ticks>` (session time) |
| `--length` | `<LENGTH>` |  |  | How long it lasts, in seconds (at least 0.8) |
| `--focus` | `<X,Y>` |  |  | The point kept in the middle as `x,y`, fractions of the frame (default 0.5,0.5) |
| `--scale` | `<SCALE>` |  |  | Zoom factor, 1.2 to 3 (default 1.8) |
| `--follow-cursor` | switch |  |  | Keep the cursor in view while zoomed |
| `--replace` | switch |  |  | Replace every existing zoom with this one |
| `--clear` | switch |  |  | Remove every zoom |

Examples:

Not run here (syntax only):

```sh
capturemann edit zoom rec_06GYEVMCKZSRHR46 --at 12 --length 3 --focus 0.4,0.5 --scale 1.8
```

#### `capturemann edit cursor`

Style the telemetry cursor (`set_cursor_style`). Options left out keep their value

- **MCP tool:** `set_cursor_style`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit cursor [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--size` | `<SIZE>` |  |  | Size, 1 to 3 times |
| `--smoothing` | `<SMOOTHING>` | off, light, strong |  | (no help text in `--help`) |
| `--click-effect` | `<CLICK_EFFECT>` | none, ring, ripple |  | (no help text in `--help`) |
| `--hide-idle` | `<HIDE_IDLE>` | true, false |  | Fade the cursor out after 2 s without movement |

Examples:

Not run here (syntax only):

```sh
capturemann edit cursor rec_06GYEVMCKZSRHR46 --size 1.5 --click-effect ring
```

#### `capturemann edit reframe`

Reframe for a vertical or square export (`set_reframe`)

- **MCP tool:** `set_reframe`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann edit reframe [OPTIONS] --aspect <ASPECT> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--aspect` | `<ASPECT>` | 9:16, 1:1 |  | The output aspect the setting is for |
| `--mode` | `<MODE>` | fit, fixed, follow |  | (no help text in `--help`) |
| `--focus` | `<X,Y>` |  |  | The window centre as `x,y`, fractions of the frame |
| `--at` | `<AT>` |  |  | A time inside the segment the focus is for (default: the first segment) |

Examples:

Not run here (syntax only):

```sh
capturemann edit reframe rec_06GYEVMCKZSRHR46 --aspect 9:16 --mode follow
```

### `capturemann ai`

Local AI: speech models, transcripts, captions and suggestions. Everything runs on this computer; a model is downloaded only when you run `ai models download`

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann ai [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann ai models`: List the speech models, or manage them: `models download|import|use|rm` (`list_models`, `download_model`, `import_model`, `select_model`, `remove_model`)
- `capturemann ai text-models`: List the text models of the engine built into CaptureMann, or manage them: `text-models download|use|rm` (`list_text_models`, `download_text_model`, `select_text_model`, `remove_text_model`). `use`...
- `capturemann ai transcribe`: Transcribe a recording with an installed model (`transcribe`); without a model it tells you how to get one (exit 2, `no_model`)
- `capturemann ai transcript`: Print a recording's transcript and caption lines (`get_transcript`)
- `capturemann ai normalize`: Add punctuation, capital letters and paragraph breaks to a transcript with the local text model, without changing a word (`normalize_transcript`). Writes a separate file; the transcript itself is not...
- `capturemann ai normalized`: Print the normalised transcript (`get_normalized_transcript`)
- `capturemann ai captions`: Edit the caption lines (`set_captions`)
- `capturemann ai translate`: Translate the captions into another language (`translate_captions`): into English with Whisper's translate task (offline, needs the large-v3 or medium model; large-v3-turbo cannot translate), into...
- `capturemann ai suggest`: Suggestions only, nothing is applied (`suggest_zooms`, `suggest_cuts`)
- `capturemann ai text-model`: Show how the text model is set up (`get_text_model`), or test it: `text-model test` (`test_text_model`). The settings are changed in the app only
- `capturemann ai speakers`: Find who spoke when (`diarize`), or fix the speakers: `speakers rename|merge|assign|list` (`set_speakers`). Runs on this computer; without a speaker model it fails with exit 2 (`no_speaker_model`)
- `capturemann ai report`: Write a report from a recording's transcript with the local text model (`generate_report`): `report <id> --template minutes`; or `report templates|get|export| delete|edit` (`list_report_templates`,...

#### `capturemann ai models`

List the speech models, or manage them: `models download|import|use|rm` (`list_models`, `download_model`, `import_model`, `select_model`, `remove_model`)

- **MCP tool:** `list_models`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai models [OPTIONS] [COMMAND]
```

Subcommands:

- `capturemann ai models download`: Download a model from the internet (asks you to confirm in the app). Resumes if it was interrupted
- `capturemann ai models import`: Install a model from a file you already have (no network)
- `capturemann ai models use`: Use this installed model by default
- `capturemann ai models rm`: Delete an installed model (asks you to confirm in the app)

Examples:

```text
$ capturemann ai models
Speech models (folder: /home/me/cm-demo/cfg/ai/models)
  tiny                     78 MB  not installed  Fastest, lowest accuracy. Good for quick drafts on any machine.
  base                    148 MB  not installed  Fast, fine for clear speech in a quiet room.
  small                   488 MB  not installed  Balanced accuracy and speed. A good choice with 8 GB of memory.
  medium                 1534 MB  not installed  Most accurate, slow. Best for accents and noisy audio.
  ...
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann ai models download`

Download a model from the internet (asks you to confirm in the app). Resumes if it was interrupted

- **MCP tool:** `download_model`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann ai models download [OPTIONS] <MODEL>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<MODEL>` | Model id, for example `small` (see `capturemann ai models`) |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--wait` | switch |  |  | Wait for the download to finish (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (the download keeps running) |

Examples:

Not run here (syntax only):

```sh
capturemann ai models download small
```

#### `capturemann ai models import`

Install a model from a file you already have (no network)

- **MCP tool:** `import_model`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai models import [OPTIONS] <MODEL> <PATH>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<MODEL>` | See the synopsis. |
| `<PATH>` | The model file, for example `ggml-small.bin` |

Examples:

Not run here (syntax only):

```sh
capturemann ai models import small ~/Downloads/ggml-small.bin
```

#### `capturemann ai models use`

Use this installed model by default

- **MCP tool:** `select_model`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai models use [OPTIONS] <MODEL>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<MODEL>` | See the synopsis. |

Examples:

Not run here (syntax only):

```sh
capturemann ai models use small
```

#### `capturemann ai models rm`

Delete an installed model (asks you to confirm in the app)

- **MCP tool:** `remove_model`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann ai models rm [OPTIONS] <MODEL>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<MODEL>` | See the synopsis. |

Examples:

Not run here (syntax only):

```sh
capturemann ai models rm small
```

#### `capturemann ai text-models`

List the text models of the engine built into CaptureMann, or manage them: `text-models download|use|rm` (`list_text_models`, `download_text_model`, `select_text_model`, `remove_text_model`). `use` also turns the built-in engine on; turning it off ("Don't use a model") is done on the AI page of the app

- **MCP tool:** `list_text_models`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai text-models [OPTIONS] [COMMAND]
```

Subcommands:

- `capturemann ai text-models download`: Download a text model from the internet (asks you to confirm in the app). Resumes if it was interrupted; the file is checked against its pinned checksum
- `capturemann ai text-models use`: Use this downloaded text model for the built-in engine
- `capturemann ai text-models rm`: Delete a downloaded text model (asks you to confirm in the app)

Examples:

```text
$ capturemann ai text-models
Built-in text models (folder: /home/me/cm-demo/cfg/ai/text/models)
  gemma-4-e2b       2842 MB  needs 3700 MB, recommended  Apache-2.0  not downloaded
  gemma-4-26b-a4b  14619 MB  needs 16800 MB, for Macs with 32 GB or more  Apache-2.0  not downloaded
  qwen3.5-4b        2708 MB  needs 4100 MB  Apache-2.0  not downloaded
  qwen3.5-2b        1271 MB  needs 2100 MB  Apache-2.0  not downloaded
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann ai text-models download`

Download a text model from the internet (asks you to confirm in the app). Resumes if it was interrupted; the file is checked against its pinned checksum

- **MCP tool:** `download_text_model`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann ai text-models download [OPTIONS] <MODEL>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<MODEL>` | Model id (see `capturemann ai text-models`) |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--wait` | switch |  |  | Wait for the download to finish (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (the download keeps running) |

Examples:

Not run here (syntax only):

```sh
capturemann ai text-models download gemma-4-e2b
```

#### `capturemann ai text-models use`

Use this downloaded text model for the built-in engine

- **MCP tool:** `select_text_model`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai text-models use [OPTIONS] <MODEL>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<MODEL>` | See the synopsis. |

Examples:

Not run here (syntax only):

```sh
capturemann ai text-models use gemma-4-e2b
```

#### `capturemann ai text-models rm`

Delete a downloaded text model (asks you to confirm in the app)

- **MCP tool:** `remove_text_model`, Confirm, asks on screen (locked)
- **Asks on screen:** Yes. The app shows an on-screen confirmation and the command waits up to 5 minutes; the answer no or no answer is exit 3 (`user_declined` or `confirmation_timeout`).

Synopsis:

```text
capturemann ai text-models rm [OPTIONS] <MODEL>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<MODEL>` | See the synopsis. |

Examples:

Not run here (syntax only):

```sh
capturemann ai text-models rm gemma-4-e2b
```

#### `capturemann ai transcribe`

Transcribe a recording with an installed model (`transcribe`); without a model it tells you how to get one (exit 2, `no_model`)

- **MCP tool:** `transcribe`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai transcribe [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording with a microphone or system-audio track |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--language` | `<LANG>` |  |  | `auto` (detect), a language code such as `en`, `vi` or `es`, or codes separated by commas to choose among, such as `vi,en`. Default: the `ai.languages` setting, else detect among all |
| `--model` | `<MODEL>` |  |  | Installed model id (default: the active one) |
| `--original-audio` | switch |  |  | Read the original microphone, as it was recorded (the default; setting `ai.transcribe_raw_mic` off makes the noise-reduced copy the default instead). It makes no difference to a recording made without noise reduction |
| `--denoised-audio` | switch |  |  | Read the noise-reduced copy a recording stores instead of the original microphone |
| `--wait` | switch |  |  | Wait for the transcription to finish (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (the job keeps running) |

Examples:

```text
$ capturemann ai transcribe shot_06GYEY0TM4JZ7KGH
error[invalid_input]: No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing is downloaded automatically.
  reason: no_model
  capturemann ai models download small   (stderr)
(exit 2)
```

The output above was run against a headless daemon with an empty throw-away library.

Not run here (syntax only):

```sh
capturemann ai transcribe rec_06GYEVMCKZSRHR46 --language en --wait
```

#### `capturemann ai transcript`

Print a recording's transcript and caption lines (`get_transcript`)

- **MCP tool:** `get_transcript`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai transcript [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--words` | switch |  |  | Print the words with their times, not only the caption lines |

Examples:

Not run here (syntax only):

```sh
capturemann ai transcript rec_06GYEVMCKZSRHR46
```

#### `capturemann ai normalize`

Add punctuation, capital letters and paragraph breaks to a transcript with the local text model, without changing a word (`normalize_transcript`). Writes a separate file; the transcript itself is not changed. Needs a transcript and a downloaded text model (exit 2, `no_text_model`). Read the result with `ai normalized <id>`

- **MCP tool:** `normalize_transcript`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai normalize [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording that has a transcript |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--language` | `<LANG>` |  |  | The language of the transcript for the prompt (`vi`, `en`, ...); default the transcript's |
| `--wait` | switch |  |  | Wait for the job to finish (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (the job keeps running) |

Examples:

Not run here (syntax only):

```sh
capturemann ai normalize rec_06GYEVMCKZSRHR46 --wait
```

#### `capturemann ai normalized`

Print the normalised transcript (`get_normalized_transcript`)

- **MCP tool:** `get_normalized_transcript`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai normalized [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Examples:

Not run here (syntax only):

```sh
capturemann ai normalized rec_06GYEVMCKZSRHR46
```

#### `capturemann ai captions`

Edit the caption lines (`set_captions`)

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann ai captions [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann ai captions get`: Print the caption lines: the edited ones, else lines built from the transcript (`get_captions`)
- `capturemann ai captions set`: Replace the caption lines from a JSON file, `-` for stdin: an array of `{ "start": 1.5, "end": 3, "text": "..." }` (seconds or `t:<ticks>`; at most 2 rows of 42 characters and 7 s per line). One undo...
- `capturemann ai captions replace`: Find and replace text across all caption lines (`replace_in_captions`): the same matching as the editor's find (Unicode, Vietnamese diacritics exact, NFC and NFD equal). Give `--find` and...
- `capturemann ai captions options`: Show or hide the captions on the video, and choose whether exports burn them in by default (`set_caption_options`). Give at least one option. One undo step

#### `capturemann ai captions get`

Print the caption lines: the edited ones, else lines built from the transcript (`get_captions`)

- **MCP tool:** `get_captions`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai captions get [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--language` | `<LANG>` |  |  | Read a translation (made by `ai translate`) instead of the original captions |

Examples:

Not run here (syntax only):

```sh
capturemann ai captions get rec_06GYEVMCKZSRHR46
```

#### `capturemann ai captions set`

Replace the caption lines from a JSON file, `-` for stdin: an array of `{ "start": 1.5, "end": 3, "text": "..." }` (seconds or `t:<ticks>`; at most 2 rows of 42 characters and 7 s per line). One undo step

- **MCP tool:** `set_captions`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai captions set [OPTIONS] <--file <FILE>|--clear> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--file` | `<FILE>` |  |  | JSON file with the lines (`-` reads stdin) |
| `--clear` | switch |  |  | Remove the edited lines (the generated ones come back) |
| `--language` | `<LANG>` |  |  | Replace the lines of this translation instead of the original captions (written to its file; not an undo step) |

Examples:

Not run here (syntax only):

```sh
capturemann ai captions set rec_06GYEVMCKZSRHR46 --file lines.json
```

#### `capturemann ai captions replace`

Find and replace text across all caption lines (`replace_in_captions`): the same matching as the editor's find (Unicode, Vietnamese diacritics exact, NFC and NFD equal). Give `--find` and `--replace`, or a JSON file of pairs. One undo step; the original transcript is not changed

- **MCP tool:** `replace_in_captions`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai captions replace [OPTIONS] <--find <TEXT>|--file <FILE>> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--find` | `<TEXT>` |  |  | Text to find |
| `--replace` | `<TEXT>` |  |  | Replacement text (may be empty to delete the match) |
| `--match-case` | switch |  |  | Match upper and lower case exactly |
| `--whole-word` | switch |  |  | Match whole words only |
| `--file` | `<FILE>` |  |  | JSON file with the pairs (`-` reads stdin): an array of `{ "find": "...", "replace": "...", "match_case": false, "whole_word": false }` |
| `--language` | `<LANG>` |  |  | Work on this translation instead of the original captions |

Examples:

Not run here (syntax only):

```sh
capturemann ai captions replace rec_06GYEVMCKZSRHR46 --find "cube control" --replace kubectl
```

#### `capturemann ai captions options`

Show or hide the captions on the video, and choose whether exports burn them in by default (`set_caption_options`). Give at least one option. One undo step

- **MCP tool:** `set_caption_options`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai captions options [OPTIONS] <--show|--hide|--burn|--no-burn> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--show` | switch |  |  | Show the captions in the editor's preview |
| `--hide` | switch |  |  | Hide the captions in the editor's preview |
| `--burn` | switch |  |  | Burn the captions into exported videos by default |
| `--no-burn` | switch |  |  | Do not burn the captions into exported videos by default |

Examples:

Not run here (syntax only):

```sh
capturemann ai captions options rec_06GYEVMCKZSRHR46 --burn
```

#### `capturemann ai translate`

Translate the captions into another language (`translate_captions`): into English with Whisper's translate task (offline, needs the large-v3 or medium model; large-v3-turbo cannot translate), into any other language with a text model (not available yet: exit 2, `no_text_model`). Read the result with `ai captions get <id> --language <LANG>`

- **MCP tool:** `translate_captions`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai translate [OPTIONS] --to <LANG> <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording that has a transcript |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--to` | `<LANG>` |  |  | The language to translate into, a code such as `en`, `vi`, `es` or `pt-br` |
| `--engine` | `<ENGINE>` | auto, whisper, text-model, text_model | auto | `auto` (the default: Whisper for English, the text model for other languages), `whisper` or `text-model` |
| `--wait` | switch |  |  | Wait for the translation to finish (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (the job keeps running) |

Examples:

Not run here (syntax only):

```sh
capturemann ai translate rec_06GYEVMCKZSRHR46 --to vi --wait
```

#### `capturemann ai suggest`

Suggestions only, nothing is applied (`suggest_zooms`, `suggest_cuts`)

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann ai suggest [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann ai suggest zooms`: Zoom regions from clicks and cursor activity; apply with `edit zoom`
- `capturemann ai suggest cuts`: Silences and filler words to cut; apply with `edit cut`

#### `capturemann ai suggest zooms`

Zoom regions from clicks and cursor activity; apply with `edit zoom`

- **MCP tool:** `suggest_zooms`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai suggest zooms [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Examples:

Not run here (syntax only):

```sh
capturemann ai suggest zooms rec_06GYEVMCKZSRHR46
```

#### `capturemann ai suggest cuts`

Silences and filler words to cut; apply with `edit cut`

- **MCP tool:** `suggest_cuts`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai suggest cuts [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Examples:

Not run here (syntax only):

```sh
capturemann ai suggest cuts rec_06GYEVMCKZSRHR46
```

#### `capturemann ai text-model`

Show how the text model is set up (`get_text_model`), or test it: `text-model test` (`test_text_model`). The settings are changed in the app only

- **MCP tool:** `get_text_model`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai text-model [OPTIONS] [COMMAND]
```

Subcommands:

- `capturemann ai text-model test`: Send one short prompt to the configured text model and report the reply time (exit 2 with `no_text_model` when none is set up)

Examples:

Not run here (syntax only):

```sh
capturemann ai text-model
```

#### `capturemann ai text-model test`

Send one short prompt to the configured text model and report the reply time (exit 2 with `no_text_model` when none is set up)

- **MCP tool:** `test_text_model`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai text-model test [OPTIONS]
```

Examples:

Not run here (syntax only):

```sh
capturemann ai text-model test
```

#### `capturemann ai speakers`

Find who spoke when (`diarize`), or fix the speakers: `speakers rename|merge|assign|list` (`set_speakers`). Runs on this computer; without a speaker model it fails with exit 2 (`no_speaker_model`)

- **MCP tool:** `diarize`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai speakers [OPTIONS] <ID>
capturemann ai speakers <COMMAND>
```

Subcommands:

- `capturemann ai speakers rename`: Name a speaker: `speakers rename <id> 2 Linh`. One undo step
- `capturemann ai speakers merge`: Fold one speaker into another: `speakers merge <id> 3 1`. One undo step
- `capturemann ai speakers assign`: Give a time range to a speaker: `speakers assign <id> 2 10.5 18`. One undo step
- `capturemann ai speakers list`: List the speakers of a recording

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording with a microphone or system-audio track |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--num` | `<N>` |  |  | Exactly this many speakers (wins over `--min` and `--max`) |
| `--min` | `<N>` |  |  | At least this many speakers |
| `--max` | `<N>` |  |  | At most this many speakers |
| `--headset` | switch |  |  | My microphone only hears me: label the microphone with `profile.display_name` (default `Me`) and tell speakers apart on the system-audio track only |
| `--wait` | switch |  |  | Wait for the job to finish (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (the job keeps running) |

Examples:

Not run here (syntax only):

```sh
capturemann ai speakers rec_06GYEVMCKZSRHR46
```

#### `capturemann ai speakers rename`

Name a speaker: `speakers rename <id> 2 Linh`. One undo step

- **MCP tool:** `set_speakers`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai speakers rename [OPTIONS] <ID> <SPEAKER> <NAME>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `<SPEAKER>` | Speaker number (see `speakers list`) |
| `<NAME>` | The new name |

Examples:

Not run here (syntax only):

```sh
capturemann ai speakers rename rec_06GYEVMCKZSRHR46 1 Linh
```

#### `capturemann ai speakers merge`

Fold one speaker into another: `speakers merge <id> 3 1`. One undo step

- **MCP tool:** `set_speakers`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai speakers merge [OPTIONS] <ID> <FROM> <INTO>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `<FROM>` | The speaker that goes away |
| `<INTO>` | The speaker that keeps all the words |

Examples:

Not run here (syntax only):

```sh
capturemann ai speakers merge rec_06GYEVMCKZSRHR46 3 1
```

#### `capturemann ai speakers assign`

Give a time range to a speaker: `speakers assign <id> 2 10.5 18`. One undo step

- **MCP tool:** `set_speakers`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai speakers assign [OPTIONS] <ID> <SPEAKER> <START> <END>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `<SPEAKER>` | Speaker number |
| `<START>` | Start, in seconds or `t:<ticks>` |
| `<END>` | End, in seconds or `t:<ticks>` |

Examples:

Not run here (syntax only):

```sh
capturemann ai speakers assign rec_06GYEVMCKZSRHR46 2 10.5 18
```

#### `capturemann ai speakers list`

List the speakers of a recording

- **MCP tool:** `get_transcript`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai speakers list [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |

Examples:

Not run here (syntax only):

```sh
capturemann ai speakers list rec_06GYEVMCKZSRHR46
```

#### `capturemann ai report`

Write a report from a recording's transcript with the local text model (`generate_report`): `report <id> --template minutes`; or `report templates|get|export| delete|edit` (`list_report_templates`, `get_report`, `export_report`, `delete_report`, `edit_report`). Needs a transcript and a local text model (exit 2, `no_text_model`)

- **MCP tool:** `generate_report`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai report [OPTIONS] --template <ID> <ID>
capturemann ai report <COMMAND>
```

Subcommands:

- `capturemann ai report templates`: List the templates (`list_report_templates`)
- `capturemann ai report get`: Print a report as Markdown, or `--json`-style with the global flag; without a report id list the reports of the recording (`get_report`)
- `capturemann ai report export`: Write a report as a Markdown or text file (`export_report`)
- `capturemann ai report delete`: Delete a report. One undo step
- `capturemann ai report edit`: Replace a report's Markdown with the text of a file (`-` reads stdin). One undo step
- `capturemann ai report check`: Tick (or with `--uncheck`, untick) a task of a report (`check_report_item`). One undo step

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording that has a transcript |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--template` | `<ID>` |  |  | The template: `summary`, `minutes`, `one_on_one`, `interview`, `product_recap` or `lecture` (see `ai report templates`) |
| `--lang` | `<LANG>` |  |  | Write the note text in this language (`vi`, `es`, ...); default the transcript's |
| `--title` | `<TITLE>` |  |  | Title of the report |
| `--attendee` | `<NAME>` |  |  | An attendee (repeat for several); owners and names come from these and the speakers |
| `--agenda` | `<AGENDA>` |  |  | The agenda, as text |
| `--role` | `<ROLE>` |  |  | Interview notes: the role |
| `--competency` | `<TEXT>` |  |  | Interview notes: a competency to collect evidence for (repeat for several) |
| `--report` | `<REPORT>` |  |  | Regenerate this report (the old one is replaced only when the new one succeeds) |
| `--wait` | switch |  |  | Wait for the job to finish (exit 6 if it fails) |
| `--timeout` | `<SECS>` |  |  | With `--wait`: give up after this many seconds (the job keeps running) |

Examples:

Not run here (syntax only):

```sh
capturemann ai report rec_06GYEVMCKZSRHR46 --template minutes --wait
```

#### `capturemann ai report templates`

List the templates (`list_report_templates`)

- **MCP tool:** `list_report_templates`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai report templates [OPTIONS]
```

Examples:

Not run here (syntax only):

```sh
capturemann ai report templates
```

#### `capturemann ai report get`

Print a report as Markdown, or `--json`-style with the global flag; without a report id list the reports of the recording (`get_report`)

- **MCP tool:** `get_report`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann ai report get [OPTIONS] <ID> [REPORT]
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `[REPORT]` | The report (see `ai report get <id>` for the list) |

Examples:

Not run here (syntax only):

```sh
capturemann ai report get rec_06GYEVMCKZSRHR46
capturemann ai report get rec_06GYEVMCKZSRHR46 rpt_00000001
```

#### `capturemann ai report export`

Write a report as a Markdown or text file (`export_report`)

- **MCP tool:** `export_report`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai report export [OPTIONS] <ID> <REPORT>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `<REPORT>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--format` | `<FORMAT>` | md, txt | md | `md` or `txt` |
| `--out-dir` | `<DIR>` |  |  | An existing absolute folder; default the recording's `exports/` |
| `--times` | switch |  |  | Print the citation times (`[mm:ss]`) after each item (default: left out) |

Examples:

Not run here (syntax only):

```sh
capturemann ai report export rec_06GYEVMCKZSRHR46 rpt_00000001 --format md
```

#### `capturemann ai report delete`

Delete a report. One undo step

- **MCP tool:** `delete_report`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai report delete [OPTIONS] <ID> <REPORT>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `<REPORT>` | See the synopsis. |

Examples:

Not run here (syntax only):

```sh
capturemann ai report delete rec_06GYEVMCKZSRHR46 rpt_00000001
```

#### `capturemann ai report edit`

Replace a report's Markdown with the text of a file (`-` reads stdin). One undo step

- **MCP tool:** `edit_report`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai report edit [OPTIONS] --file <FILE> <ID> <REPORT>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `<REPORT>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--file` | `<FILE>` |  |  | The Markdown file |

Examples:

Not run here (syntax only):

```sh
capturemann ai report edit rec_06GYEVMCKZSRHR46 rpt_00000001 --file minutes.md
```

#### `capturemann ai report check`

Tick (or with `--uncheck`, untick) a task of a report (`check_report_item`). One undo step

- **MCP tool:** `check_report_item`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann ai report check [OPTIONS] --section <SECTION> --item <ITEM> <ID> <REPORT>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Capture id of a recording |
| `<REPORT>` | See the synopsis. |

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--section` | `<SECTION>` |  |  | The section: `action_items`, `agreed_actions`, `follow_ups` or `next_steps` |
| `--item` | `<ITEM>` |  |  | The item's place in the section, counting from 0 |
| `--uncheck` | switch |  |  | Untick instead of tick |

Examples:

Not run here (syntax only):

```sh
capturemann ai report check rec_06GYEVMCKZSRHR46 rpt_00000001 --section action_items --item 0
```

### `capturemann jobs`

Background jobs (`get_job`, `list_jobs`, `cancel_job`, `job.*` events)

- **MCP tool:** see the subcommands below.

Synopsis:

```text
capturemann jobs [OPTIONS] <COMMAND>
```

Subcommands:

- `capturemann jobs get`: Show one job: state, progress, result or error (`get_job`)
- `capturemann jobs list`: List jobs, newest first (`list_jobs`)
- `capturemann jobs cancel`: Cancel a job; cancelling a cancelled job does nothing (`cancel_job`)
- `capturemann jobs retry`: Retry a failed or cancelled job, or continue one that is waiting for a sign-in (`retry_job`)
- `capturemann jobs watch`: Print the active jobs, then stream `job.*` events until interrupted

#### `capturemann jobs get`

Show one job: state, progress, result or error (`get_job`)

- **MCP tool:** `get_job`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann jobs get [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Job id |

Examples:

```text
$ capturemann jobs get job_7c624d14233345d7
job_7c624d14233345d7  export  done  100%  Done
  created: 2026-11-17T03:06:12.925Z
  started: 2026-11-17T03:06:12.925Z
  finished: 2026-11-17T03:06:13.055Z
  elapsed: 0 s
  result: {"dest":"/home/me/cm-demo/exported","exported":1,"skipped":0,"summary":"1 exported","files":[{"capture_id":"shot_06GYEY0TM4JZ7KGH","status":"exported","path":"/home/me/cm-demo/exported/demo.jpg","width":320,"height":200,"bytes":7342}]}
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann jobs list`

List jobs, newest first (`list_jobs`)

- **MCP tool:** `list_jobs`, Free, read-only
- **Asks on screen:** No (read-only).

Synopsis:

```text
capturemann jobs list [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--state` | `<STATE>` | queued, running, paused, failed, done, cancelled |  | Only jobs in this state |
| `--type` | `<TYPE>` |  |  | Only jobs of this type (`export`, `upload`, ...) |
| `--limit` | `<LIMIT>` |  |  | Page size (1 to 200, default 50) |

Examples:

```text
$ capturemann jobs list
job_7c624d14233345d7  export  done  100%  Done
job_097b376c55827407  import  done  100%  Done
job_b4adb0dab88caeed  import  done  100%  Done
```

The output above was run against a headless daemon with an empty throw-away library.

#### `capturemann jobs cancel`

Cancel a job; cancelling a cancelled job does nothing (`cancel_job`)

- **MCP tool:** `cancel_job`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann jobs cancel [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Job id |

Examples:

Not run here (syntax only):

```sh
capturemann jobs cancel job_f7b27a8e508ae4a1
```

#### `capturemann jobs retry`

Retry a failed or cancelled job, or continue one that is waiting for a sign-in (`retry_job`)

- **MCP tool:** `retry_job`, Free, can be set to Ask first
- **Asks on screen:** No, unless the user set the tool to Ask first in Settings > Agents & CLI.

Synopsis:

```text
capturemann jobs retry [OPTIONS] <ID>
```

Arguments:

| Argument | Description |
| --- | --- |
| `<ID>` | Job id |

Examples:

Not run here (syntax only):

```sh
capturemann jobs retry job_f7b27a8e508ae4a1
```

#### `capturemann jobs watch`

Print the active jobs, then stream `job.*` events until interrupted

- **MCP tool:** none; the MCP server forwards the same events as notifications (see [mcp.md](mcp.md#events)).

Synopsis:

```text
capturemann jobs watch [OPTIONS]
```

Options (plus the [global flags](#global-flags)):

| Flag | Value | Allowed values | Default | Description |
| --- | --- | --- | --- | --- |
| `--topic` | `<TOPIC>` |  |  | Event topics (default `job.*`) |
| `--max-events` | `<N>` |  |  | Stop after this many events (for scripts); the initial snapshot does not count |

Examples:

Not run here (syntax only):

```sh
capturemann jobs watch --max-events 3
```

