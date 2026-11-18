# QA 0.3.5, macOS notarized DMGs built from main 9dcfb5c3 (Nov 17 to 18, 2026, claude)

Verdict: **PARTIAL**. This is a check of candidate files of commit 9dcfb5c3, not of the published files.

## Files

| File | SHA-256 | Checks |
| --- | --- | --- |
| `CaptureMann-0.3.5-arm64.dmg` | `53fe3400e7f3940ad916f3401da1fe3a579e7fc266cef9fb86a44e41251dd67c` | matches the maintainer's sum; `stapler validate` worked; `spctl` accepted (Notarized Developer ID); app inside is 0.3.5, bundle id `dev.capturemann.app`, `codesign --verify --deep --strict` OK, team id DJ2ALRFN66 |
| `CaptureMann-0.3.5-x86_64.dmg` | `cc8ab66c39e48998b19184a55adea60058e523493e99ab29bba8800a186a903a` | matches; `stapler validate` worked; `spctl` accepted; app is 0.3.5, `lipo` says x86_64, signature OK, hardened runtime, same team id |

The app inside a DMG has no ticket of its own (`stapler` says so); the ticket is on the DMG, as for 0.3.4. No Intel hardware: the Intel DMG was checked for version, architecture, signature, notarization and checksum only.

## cm-regress on the arm64 app copy (`--allow screen` only; no microphone, camera or system audio)

Reports: `L1-macos-0.3.5-dmg-9dcfb5c3-screen-regression-{standard,full,hardware}-2026-11-17-claude.md` (+ .json). The app was copied with `ditto` from the read-only mounted DMG; headless cases used a release cm-daemon built from the 9dcfb5c3 tree.

| Tier | Passed | Failed | Skipped |
| --- | --- | --- | --- |
| standard | 52 | 0 | 12 of 64 |
| full | 59 | 0 | 21 of 80 |
| hardware | 59 | 0 | 29 of 88 |

Skipped (all tiers, same causes): the `linux.*` cases, `shot.window`, `redact.window_text`, `installer.silent` and the `desktop.*` cases not yet implemented for macOS; `regress.ffmpeg_missing_wording` (an FFmpeg in `/opt/homebrew` cannot be hidden). Skipped for lack of consent or input (hardware tier): `rec.mic`, `rec.system_audio`, `rec.camera`, `rec.soak`, `rec.external_4k`, `rec.each_monitor`, `shot.second_monitor`, `guide.real_clicks`, `ai.transcribe`, `ai.speakers` (no model flags).

## Re-checks of merged fixes (CLI on the same app copy, a private library)

- `capturemann --version`: 0.3.5. MCP `tools/list`: 89 tools; `create_board` is listed.
- ANN-12: dashed rectangle, dotted ellipse, dashed line and dotted arrow rendered through `capture screenshot --annotate` (picture `shots/QA-0.3.5-macos-dmg-9dcfb5c3-ann12-styles.png`, looked at: the dashes and dots are correct and the arrow head is solid); `style: wavy` is refused with `invalid_input` naming `ops[0]` and the three allowed values.
- BUG-9: `--window e` answers `invalid_input` listing 12 matches; an unknown title answers `not_found`. The list names other apps' window titles (finding G4, CLI only).
- BUG-12: tested with fakes only (cm-daemon `a_big_image_is_omitted_and_a_small_one_is_inline`, cm-cli `an_omitted_image_is_copied_from_the_stored_file`, both pass). **Not run** against a real large Long Shot on macOS.
- About title: tested with fakes only (cm-desktop `the_general_feedback_url_carries_the_version_and_a_title_only`, passes). The button itself was not clicked.

## Not run

BUG-13, 14, 17, 18 on screen (no pointer), the real Long Shot and Click Guide with clicks, Intel hardware, microphone, camera and system-audio cases, a second monitor.

## Slip to report

Before the coordinator's change of plan I had run `make bundle-dev`, which rebuilt `target/bundle/CaptureMann-dev.app` and refreshed `~/Applications/CaptureMann-dev.app` (dev bundle of 9dcfb5c3). The DMGs and the other files in `target/bundle` were not touched; their sums were re-checked.
