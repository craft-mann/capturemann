# RELEASE-0.2.0 report: signing, notarization and first install

The signing and notarization results come from the terminal output the maintainer pasted
(`make release`, `verify-release`) and from read-only checks run on the DMG afterwards. The hands-on lines
are **as reported**: nothing else is stored, no screenshots, no recordings.

- **Date:** 2026-10-27
- **Who:** the maintainer
- **Machine:** MacBook Pro, Apple M4 Pro, macOS 26 (not re-read for this run)
- **Build:** git `f4497b3` (the commit the tag `v0.2.0` is meant to point at), release build, made with
  `make release` (bundle-release, dmg, notarize, verify-release)
- **FFmpeg:** the bundled LGPL build (`make ffmpeg`); `verify-release` found no gpl, nonfree, libx264, libx265
  or libfdk-aac
- **Other bundled pieces:** `llama-server` (llama.cpp v0.7.0, `make llama`), the sherpa-onnx libraries and the
  two speaker models (`make sherpa`)
- **Artifacts:** `CaptureMann-0.2.0-arm64.dmg`, 86,563,107 bytes, copied as
  `CaptureMann-v0.2.0-macos-arm64.dmg`, SHA-256
  `8344dbf22eab44c95e375acb2c2ecc3d3711d5a058876ac5cf37f062b100e150`; the app inside is about 181 MB
- **Published as:** GitHub release `v0.2.0`, published 2026-10-27 13:52 UTC (read with `gh release view`), tag
  on `f4497b3`, assets `CaptureMann-v0.2.0-macos-arm64.dmg` (86,563,107 bytes) and its `.sha256` file

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| `make release` | PASS | `verify-release`: "result: passed, 0 warning(s)"; Info.plist version 0.2.0 equals the workspace version; LSMinimumSystemVersion 13.0; deployment targets at most 13.0 (pasted by the maintainer) |
| Notarization ticket | PASS | `xcrun stapler validate` on the DMG: "The validate action worked!" (read-only check run after the build) |
| Gatekeeper on the DMG | PASS | `spctl --assess --type open --context context:primary-signature`: accepted, source=Notarized Developer ID |
| DMG signature | PASS | `codesign --verify`: satisfies its Designated Requirement |
| Checksum file | PASS | `shasum -a 256 -c` printed OK on the copy named for the release |
| Install of 0.2.0 | PASS (as reported) | the maintainer installed the new version |
| Recording the screen | PASS (as reported) | works as expected |
| Taking a screenshot | PASS (as reported) | works as expected |
| New audio module: audio recording | PASS (as reported) | works as expected |
| Generating a report | PASS (as reported) | works as expected with the built-in text engine |
| A second Mac, or a second macOS user (first install with no Keychain items) | not run | |
| Opening a real 0.1.0 library with 0.2.0 | not reported | the synthetic 0.1.0 library migrated (index 2 to 3, all bundle files unchanged) on Oct 27, 2026 |
| Homebrew cask update and `brew install` | PASS (as reported) | the maintainer reported the Homebrew cask is OK; nothing stored |

## Acceptance criteria

NFR-15 (Distribution: signed and notarized macOS build): PASS for the DMG above (macOS, Apple Silicon only).
Updates stay manual.

## Problems found

- Pushing the tag also started `release.yml` on GitHub (run 40512349260). Its build job passed (20 minutes, an
  unsigned build, because no signing secrets exist in the repository) and its last job refused to create a
  release because the hand-made one existed. The release was not changed. For manual releases, disable the
  workflow first (`gh workflow disable release.yml`) or limit it to manual runs.

## Verdict

PASS for signing, notarization, Gatekeeper on the DMG and the first install on the build machine; the
hands-on checks are as reported. A first install on a second Mac or user is not done. The release workflow ran once on GitHub (unsigned build, passed; see Problems found).
