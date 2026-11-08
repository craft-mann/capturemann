# REC-8 manual script: crash-safe recording, kill -9 recovery (Linux, X11 and Wayland)

What automation cannot do on a real desktop: kill the process that owns a live screen and microphone
recording, and look at what the next start recovers. The logic (lock life cycle, truncation to the
last fragment, gap mapping, the disk guard) is covered by `cargo test -p cm-library -p cm-encode -p cm-daemon`;
the cm-regress case `rec.kill9_recovery` runs the same steps without a person on the lab laptop.

Privacy: this records the screen and, with `--mic`, the microphone. Use a throw-away library, delete it
afterwards, and keep only the numbers in the report (`verification/`, template in its README). Never say
"verified" without that report.

## Setup (a Linux desktop session; GNOME needs no dialog, other desktops show the portal's share dialog once)

```
export CAPTUREMANN_LIBRARY=$(mktemp -d) CAPTUREMANN_CONFIG_DIR=$(mktemp -d) CAPTUREMANN_SOCKET=$CAPTUREMANN_LIBRARY/sock
cm-daemon &                      # or the built target/debug/cm-daemon
```

An agent-started recording asks for confirmation unless `agents.tool_policy.start_recording` is `allow` in
`$CAPTUREMANN_CONFIG_DIR/settings.json`; for a lab run write `{"agents.tool_policy":{"start_recording":"allow"}}` there before starting the daemon.

## 1. AC1: kill -9 mid-recording loses less than 3 s

1. `capturemann record start --size 720p --countdown 0 --mic default` and note the time.
2. Let it record 20 s (move the mouse; play a tone for the microphone if you want to listen to it later).
3. `kill -9 $(pgrep -x cm-daemon)`. Check the bundle in the library: `recording.lock` is there, `screen.mp4` and `mic.wav` exist.
4. Start `cm-daemon` again. Within a few seconds the log says it recovered the recording; the bundle has a `manifest.json`, no `recording.lock`, `revisions[0].action` is `recover`.
5. Pass: every track is at least (20 s minus 3 s) long: `ffprobe -show_entries format=duration screen.mp4` and the same for the wav files; `recovered.lost_ms_max` is 2000 or less; the video plays to the end.

## 2. AC1 again after a pause

Same as 1, but `capturemann record pause`, wait 5 s, `record resume`, 10 s more, then kill. The recovered duration excludes the pause.

## 3. A stale lock next to a finished manifest

Copy `recording.lock` back into a finished bundle and start the daemon: the lock is removed, the bundle is untouched.

## 4. Where Linux differs

- GNOME shows its own recording indicator (the orange dot) while the session runs; it disappears after the kill (the compositor ends the session when the owner leaves the bus). If it stays, note it.
- With the portal (not GNOME) a kill leaves the share session to be closed by the portal; the next start asks nothing (restore token).
- Record which session type (`echo $XDG_SESSION_TYPE`), GPU and encoder (`capturemann diagnostics` names VA-API H.264 or software VP9) the run used.
