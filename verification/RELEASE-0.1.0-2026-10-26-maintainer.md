# RELEASE-0.1.0 report: signing, notarization and first install

The notarization, `verify-release` and Gatekeeper results below are the output the maintainer pasted
from the terminal (quoted in part). The hands-on lines are **as reported**: nothing else is stored,
no screenshots, no recordings.

- **Date:** 2026-10-26
- **Who:** the maintainer
- **Machine:** MacBook Pro, Apple M4 Pro, macOS 26 (as in the other reports; not re-read for this run)
- **Build:** git `7fe4c97` (the commit the tag `v0.1.0` points at), release build, bundle made with
  `CM_HARDENED=1 make bundle-release`, then `make dmg`, `make notarize`, `make verify-release`
- **FFmpeg:** the bundled LGPL build (`make ffmpeg`); `verify-release` found no gpl, nonfree, libx264,
  libx265 or libfdk-aac
- **Fixture:** none; a real recording made in the installed app
- **Published as:** GitHub release `v0.1.0` (pre-release), assets `CaptureMann-v0.1.0-macos-arm64.dmg`
  and its `.sha256` file

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Bundle signed with the Developer ID Application certificate | PASS | `codesign --verify --deep --strict` valid; `Authority=Developer ID Application: TRUNG TRAN (DJ2ALRFN66)`, `TeamIdentifier=DJ2ALRFN66`, `flags=0x10000(runtime)`, secure timestamp |
| `make dmg` | PASS | `CaptureMann-0.1.0-arm64.dmg`, copied as `CaptureMann-v0.1.0-macos-arm64.dmg` for the release |
| `make notarize` (App Store Connect API key) | PASS | submission `70a3df49-d81f-4ba2-8458-530c63871ca3`, status **Accepted**; stapled; `staple and validate` worked |
| `make verify-release` | PASS | "result: passed, 0 warning(s)": Developer ID identity; hardened runtime and secure timestamp on the bundle and on all 6 Mach-O files (`capturemann`, `cm-desktop`, `cm-mcp`, `cm-overlay`, `ffmpeg`, `ffprobe`); entitlements equal `CaptureMann.entitlements` and no `get-task-allow`; Gatekeeper "accepted, source=Notarized Developer ID"; ticket stapled to the DMG; bundled FFmpeg is LGPL, source offer and licences present; `Info.plist` version 0.1.0; `LSMinimumSystemVersion` 13.0; deployment targets all <= 13.0 |
| Checksum file | PASS | `shasum -a 256 -c CaptureMann-v0.1.0-macos-arm64.dmg.sha256` printed OK before the release was created (as reported) |
| Download the DMG from the release in a browser, open it, drag the app to `/Applications` | PASS (as reported) | the normal first-launch dialog "downloaded from the Internet, are you sure you want to open it?" with Open; no other warning |
| Installed copy checked | PASS | `/Applications/CaptureMann.app`: `codesign --verify --deep --strict` OK, `spctl --assess` "accepted, source=Notarized Developer ID", version 0.1.0, `cm-desktop` byte-identical to the one in the DMG |
| Permissions on the installed copy: microphone, camera, input monitoring, Screen Recording | PASS (as reported) | Screen Recording needed the old dev-signed copy removed first (see Problems) |
| Short recording in the installed app | PASS (as reported) | nothing stored |
| Publish to YouTube from the same build line | PASS (as reported) | reported working on 2026-10-26 |
| A second Mac or a second macOS user (first install with no Keychain items) | not run | |

## Acceptance criteria

NFR-15 (Distribution: signed and notarized macOS build): PASS for the DMG above (macOS, Apple Silicon
only, Windows after 0.1.0). The "auto-update" part of the NFR is not in 0.1.0 (updates are manual).

## Problems found

- **Keychain prompt on the build machine.** The notarized app asked for the login password to use the
  Keychain item `dev.capturemann.app`. That item was created by earlier builds signed with the local
  "CaptureMann Local Dev" identity, so the Developer ID app was not on its access list. Expected when
  the signing identity changes; choose Always Allow. A Mac with no earlier items does not see it
  (second-user check not run).
- **Screen Recording stayed "not granted".** The old dev-signed `~/Applications/CaptureMann.app`
  (same bundle id, different signing requirement) was still the copy that opened; the log said
  `Failed to match existing code requirement for subject dev.capturemann.app and service
  kTCCServiceScreenCapture`. Fixed by removing that copy and opening `/Applications/CaptureMann.app`.
  Documented in `docs/development/building.md`; local builds should use `make bundle-dev`.
- The `release.yml` workflow was not used for this release (it has never run on GitHub; the DMG was
  built, notarized and uploaded by hand).

## Verdict

PASS for signing, notarization, Gatekeeper and the first install on the build machine. The
hands-on checks are as reported. A first install on a second Mac or a second user, and a run of the
release workflow, are not done.
