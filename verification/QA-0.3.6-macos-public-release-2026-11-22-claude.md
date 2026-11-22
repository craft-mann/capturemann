# QA 0.3.6 macOS, files from the PUBLIC release v0.3.6, 2026-11-22, Claude

Verdict: PARTIAL (checks and regression runs, not a person's verification). No product issue found.

## What was downloaded
`gh release download v0.3.6 -R craft-mann/capturemann` (macOS files and SHA256SUMS.txt) into a fresh temp dir; the arm64 DMG was also fetched with `curl -L` from the public asset URL.
- CaptureMann-v0.3.6-macos-arm64.dmg, 92894698 bytes, sha256 7cdc5fdefedf60321d6b3b2fde7fb141bb5607296910d1092576d79566220586
- CaptureMann-v0.3.6-macos-x86_64.dmg, 97954617 bytes, sha256 c52e6256b7d81031bbfac2c7e21d53aa7ce006b240002007eb197ca8df2942ce
Both equal SHA256SUMS.txt, the two .sha256 files, the Homebrew cask values, and the pre-release DMGs of the 96b85253 pass (the same files). The curl download equals the gh download.

## Gatekeeper and signing
- `bash packaging/macos/verify-release.sh --arch arm64 <dmg>` and `--arch x86_64 <dmg>`: both "result: passed, 0 warning(s)" (Gatekeeper accepted, Notarized Developer ID, ticket stapled to the DMG, LGPL FFmpeg, llama-server, both speaker-model hashes, licences, version 0.3.6, LSMinimumSystemVersion 13.0).
- DMGs: Developer ID Application: TRUNG TRAN (DJ2ALRFN66), timestamps Nov 21, 2026 19:28 (arm64) and 19:57 (x86_64); `stapler validate` on the arm64 DMG worked.
- Quarantine test on COPIES (`xattr -w com.apple.quarantine "0081;00000000;Safari;"`): `spctl -a -t open --context context:primary-signature` on the DMG copy: accepted, Notarized Developer ID; `spctl -a -vv` on the quarantined app copy (ditto from the mounted DMG): accepted, Notarized Developer ID; `codesign --verify --deep --strict` ok on both apps (arm64 copy and Intel app in the mounted DMG).
- Both apps: version 0.3.6, bundle id dev.capturemann.app; the Intel app's cm-desktop is x86_64 only. The Intel app was NOT run.

## Homebrew (nothing installed, upgraded or removed)
- Read-only state first: `brew list --cask` has `capturemann` (tap craft-mann/tap, 0.3.6, 212.9 MB, installed on request; the maintainer already updated it); /Applications has CaptureMann.app (0.3.6 by Info.plist) and CaptureMann-dev.app; ~/Applications has no CaptureMann.
- `brew info --cask craft-mann/tap/capturemann`: 0.3.6, macOS >= 13. `brew fetch --cask craft-mann/tap/capturemann`: success (✔︎ Cask capturemann 0.3.6); the download was already in Homebrew's cache (07:21 today, from the maintainer's upgrade), so Homebrew re-checked that cached file against the cask sha256 (arm64 7cdc5fde... as above, intel c52e6256...), size 92894698 bytes equal to the public asset. The cask URL pattern points at the public asset names.
- `brew audit --cask --online craft-mann/tap/capturemann`: exit 0, no complaints (it printed only gem housekeeping lines).

## cm-regress on the arm64 app copy from the public DMG
App copied with `ditto` from the read-only mounted DMG. cm-daemon and cm-regress: release build from main 57b5b030 into CARGO_TARGET_DIR=/private/tmp/cm-pub-target (the earlier binaries no longer existed). `git diff --stat 96b85253 v0.3.6 -- crates apps packaging` touches only 3 desktop files (platform_win_monitors.rs, platform_window.rs, ui/quick_window.rs), and `git diff v0.3.6 HEAD -- crates apps packaging Cargo.lock` is empty, so the daemon and suite equal the tag's. Options: `--allow screen --timeout-scale 3 --installer <public arm64 dmg> --commit 11061ddf`.

| Tier | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard (app copy carried the Safari quarantine flag) | 52 | 0 | 12 | `L1-macos-0.3.6-public-regression-standard-2026-11-22-claude.md` |
| hardware (quarantine removed from the copy) | 59 | 0 | 29 | `L1-macos-0.3.6-public-regression-hardware-2026-11-22-claude.md` |
| hardware, first run (quarantined copy) | 58 | 1 | 29 | `...-hardware-quarantined-first-run-...md` (kept) |

The same counts as the 96b85253 pass (standard 52/0, hardware 59/0).

Failure in the first hardware run: `desktop.second_launch`, "1 other cm-desktop process(es) are still running". Cause is the lab, not the product: I had set the quarantine flag on the app copy, which the first instance (started directly) ignores, but the second launch goes through LaunchServices, which Gatekeeper-translocated to `/private/var/folders/.../AppTranslocation/.../CaptureMann.app` and kept as a separate process (parent 1) that the case counts as an extra instance. Reproduced twice with the flag; with the flag removed the case passed alone and in the full hardware rerun. I killed that stray translocated process (my own). Possible note for the suite: a quarantined copy makes this case fail; a real user's installed app is not translocated once moved to /Applications, so no product issue.

Skips are the usual ones (not implemented on macOS: shot.window, redact.window_text, desktop.hud_*, recording_banner, recording_placement, no_console_flash, settings_applied, guide.real_clicks, rec.each_monitor, installer.silent, linux.*; flags not given: mic, camera, sound, second monitor, 4K, soak; models for ai.transcribe/speakers).

## Launch of the downloaded app under isolation
Binary run directly (`Contents/MacOS/cm-desktop`) with CAPTUREMANN_LIBRARY, CAPTUREMANN_CONFIG_DIR, CAPTUREMANN_SOCKET=/tmp/cmpub.sock, CAPTUREMANN_SECRETS=file all in temp. Alive after 15 s and 31 s; the log shows the studio window opened (1 live); no panic or crash line in its desktop.log; no crash report. Through the CLI (`--no-start`): `diagnostics` reports CaptureMann 0.3.6 from an app bundle, macOS 26.6.2, Apple M4 Pro, permissions authorized, both speaker models present; `settings get` works; `capture screenshot --region 0,0,64,64` produced a 64x64 .cmshot in the temp library (listed by `library list`). Stopped with SIGTERM to my own pid; it exited. The "bar" itself was not opened or inspected (no CLI command for it; only the studio window was seen in the log).

## Issues found
None in the product. (Lab note only: the quarantine and `second_launch` interaction above.)

## Not run
Intel Mac run (the Intel DMG got signature, notarization, architecture and version checks only), microphone, camera, system audio, second monitor, soak, real-model transcription and speaker cases, window-based screenshot and Auto-redact (not implemented on macOS), Retype, the full tier and manual scripts, a real browser download with Safari's own quarantine, `brew install` (not allowed; the maintainer's install is already 0.3.6).

## Isolation
Library `~/Movies/CaptureMann`: 377 bundle folders (find, depth 3) before and 377 after. The maintainer's /Applications and ~/Applications were not touched; the dev app and the cm-mcp processes of target/bundle were running and not touched (no cm-desktop of the maintainer was running during the runs). All DMGs detached, temp dirs and /tmp socket removed, processes I started ended.
