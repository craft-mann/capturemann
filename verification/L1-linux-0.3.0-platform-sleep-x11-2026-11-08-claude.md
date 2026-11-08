# L1 Linux 0.3.0 platform: idle-sleep guard (NEW-A, VOC-1, REC-1)

Result: PARTIAL (the lock is taken and released on one machine; no 10-minute screen-blank run).

- Machine: ThinkPad T490, Ubuntu 24.04, x86_64, GNOME 46 on X11 (auto-login desktop), reached over ssh.
- Build: branch `track/linux-platform` from 63baf9bc, `cargo test -p cm-platform` (debug).
- Date: 2026-11-08. Run by the Claude agent.

## What was run

`cargo test -p cm-platform -- --ignored` (tests `the_real_layers_on_this_desktop`, `the_guard_shows_in_systemd_inhibit_and_is_released`) with the desktop session's D-Bus environment.

## Findings

1. logind `Inhibit("sleep:idle", block)` from an ssh session: `AccessDenied` (also with `systemd-inhibit` itself). `idle` alone in block mode works. The code tries `sleep:idle`, then `idle`. A blocking `sleep` lock from the logged-in desktop session itself was not tried.
2. The portal `org.freedesktop.portal.Inhibit.Inhibit` answers with a request handle, but GNOME's session manager listed no inhibitor (`GetInhibitors` returned `[]`). So on GNOME 46 the portal alone does not hold. The direct `org.gnome.SessionManager.Inhibit` is therefore taken as well.
3. With all layers held: `systemd-inhibit --list` shows the "CaptureMann test lock" row; `org.gnome.SessionManager.IsInhibited(8)` is `true`, `GetInhibitors` lists one inhibitor. After the guard is dropped both are gone (checked).
4. KDE: not run. The portal layer is the expected path there (KDE's portal backend implements Inhibit); untested.
5. Crash release (kill -9 of the holder): relies on the bus dropping the connection / the fd closing; not run.

## Not done

A 10-minute recording with screen blanking off.
