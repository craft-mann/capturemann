# Releasing CaptureMann

A release is a signed, notarized and stapled DMG plus a SHA-256 file, attached to a GitHub release.
The release is Apple Silicon (arm64) and macOS 13+ only. Updates are manual: users download the new
DMG, the app has no update code.

The workflow is [`.github/workflows/release.yml`](../../.github/workflows/release.yml). It was written
without being run on GitHub; test it on a fork first (see [Testing the workflow](#testing-the-workflow-on-a-fork)).

## What the workflow does

On a pushed tag `v*` (or a manual run with a tag that exists) on a `macos-14` (Apple Silicon) runner:

1. Checks the tag has the form `vX.Y.Z[-suffix]` and equals the workspace version in `Cargo.toml`.
2. Extracts the `## [X.Y.Z]` section of `CHANGELOG.md` for the release notes. It fails if the
   section is missing or empty.
3. Builds the LGPL FFmpeg with `make ffmpeg` (cached by the hash of `packaging/ffmpeg/build-macos.sh`)
   and runs `make licenses`.
4. If all signing secrets exist, it builds a signed release: `ci-keychain.sh setup`,
   `CM_HARDENED=1 make bundle-release`, `dmg.sh`, `notarize.sh`, `verify-release.sh`, and always
   `ci-keychain.sh teardown`. If none exist, it builds ad hoc, makes the DMG and runs
   `verify-release.sh --allow-unsigned`; the release notes then say the build is ad hoc signed and
   not notarized. If only some secrets exist, the workflow fails on purpose.
5. Creates a **draft** release with the DMG, the `.sha256` file and the notes. The release is also
   marked as a pre-release while the version is `0.x` or has a suffix. Nothing is public until you
   press Publish.

Only the last job has `contents: write`.

## Cutting a release

1. **Bump the version.** Set `version` in the root `Cargo.toml` (`[workspace.package]`), then run
   `cargo update -w` so `Cargo.lock` follows.
2. **Update `CHANGELOG.md`.** Under `## [Unreleased]` add `## [X.Y.Z] - YYYY-MM-DD` (the workflow
   matches `## [X.Y.Z]` and takes everything up to the next `## [` heading). Keep `## [Unreleased]`
   empty above it. Update the "Known limitations" list.
3. **Regenerate the licences:** `make licenses`, and commit `THIRD_PARTY_LICENSES.md` if it changed.
4. **Run the full checks:**
   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo test --workspace --locked
   cargo deny check
   ```
5. **Commit and merge** to `main`, and wait for CI to pass.
6. **Tag and push:**
   ```sh
   git tag -a vX.Y.Z -m "CaptureMann X.Y.Z"
   git push origin vX.Y.Z
   ```
7. **Watch the workflow** (Actions, "Release"). A cold run builds FFmpeg and whisper.cpp from
   source and takes a long time; later runs reuse the FFmpeg cache.
8. **Review the draft** under Releases: the notes, the two assets, and the line saying whether the
   build is notarized. Download the DMG and run the checks below on a Mac that is not your build
   machine.
9. **Publish** the draft. Then update the website download link (manual, see below) and the
   Homebrew cask: in a checkout of `craft-mann/homebrew-tap` run
   `scripts/update-homebrew-cask.sh X.Y.Z <path to the tap checkout>`, then commit and push the tap.
   The script reads the checksum from the release's `.sha256` asset, so the release must be public
   first. Test with `brew install --cask --appdir=/tmp/cm-brew-test craft-mann/tap/capturemann`.

## GitHub secrets

Settings, Secrets and variables, Actions, "New repository secret". All are optional together: with
none of them the workflow builds ad hoc.

| Secret | Value |
|---|---|
| `APPLE_CERT_P12_BASE64` | The Developer ID Application certificate and its private key, as a base64 `.p12` |
| `APPLE_CERT_PASSWORD` | The password you set when exporting the `.p12` |

Plus **one** notarization credential set:

| Secret | Value |
|---|---|
| `APPLE_API_KEY_ID` | App Store Connect API key id (for example `ABC123DEFG`) |
| `APPLE_API_ISSUER` | The issuer id shown on the API keys page (a UUID) |
| `APPLE_API_KEY_P8` | The full text of the downloaded `AuthKey_<id>.p8` file |

or, instead:

| Secret | Value |
|---|---|
| `APPLE_ID` | The Apple ID email of the developer account |
| `APPLE_TEAM_ID` | The 10-character team id |
| `APPLE_APP_PASSWORD` | An app-specific password for that Apple ID |

The API key set is preferred: it does not depend on a personal Apple ID. If both sets exist the
workflow uses the API key.

### Creating each one

**Developer ID Application certificate (`.p12`).**
1. Join the Apple Developer Program (a paid account, enrolment can take days).
2. In Keychain Access, Certificate Assistant, "Request a Certificate From a Certificate Authority",
   save the request to disk.
3. At developer.apple.com, Certificates, create a **Developer ID Application** certificate from that
   request, download it and double-click it to install it in the login keychain. (Only the Account
   Holder can create Developer ID certificates.)
4. In Keychain Access, "My Certificates", expand "Developer ID Application: ...", select the
   certificate (with its key), File, Export Items, format `.p12`, and set a password. That password
   is `APPLE_CERT_PASSWORD`.
5. Encode and copy it, then delete the loose files:
   ```sh
   base64 -i DeveloperID.p12 | pbcopy      # paste into APPLE_CERT_P12_BASE64
   rm DeveloperID.p12
   ```

**App Store Connect API key.** At appstoreconnect.apple.com, Users and Access, Integrations, App
Store Connect API, generate a key (access "Developer" is enough for notarization). Note the key id and
the issuer id, and download the `.p8` once (it cannot be downloaded again). Paste the whole file,
including the `BEGIN` and `END` lines, into `APPLE_API_KEY_P8`.

**App-specific password (alternative).** At account.apple.com, Sign-In and Security, App-Specific
Passwords, create one named for CI. The team id is on developer.apple.com, Membership details.

Rotate by replacing the secret. If a secret leaks, revoke the certificate or key at Apple first.

## Local release

For a release from your own Mac (same scripts as the workflow). You need the Developer ID identity in
your keychain and the notarization credentials in the environment.

```sh
make ffmpeg                       # once; builds packaging/ffmpeg/out
make licenses
export CM_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_API_KEY_ID=...  APPLE_API_ISSUER=...  APPLE_API_KEY_PATH=/path/to/AuthKey_ID.p8
make release                      # bundle-release, dmg, notarize, verify-release
```

`make release` is the chain `bundle-release`, `dmg`, `notarize`, `verify-release`; run the steps
one by one (`make dmg`, `make notarize`, ...) to debug. Use `bash packaging/macos/notarize.sh <dmg> --dry-run`
to check the setup without submitting. Do not run any of this over `target/bundle` while the app is
running from it. Upload by hand with `gh release create vX.Y.Z --draft ...` if you skip the workflow.

## Testing the workflow on a fork

1. Fork the repository and enable Actions on the fork.
2. On a branch of the fork, set the version to a throwaway one, for example `0.0.1-test.1` in
   `Cargo.toml` (`cargo update -w`) and add a `## [0.0.1-test.1]` section to `CHANGELOG.md`.
3. Without secrets this exercises the ad hoc path. Push `git tag v0.0.1-test.1` to the fork. To test
   the signed path, add the secrets to the fork (a real certificate is needed; notarization of a
   throwaway build is fine).
4. Check the draft release, download the assets, run `verify-release.sh` and the checks below.
5. Delete the draft, the tag (`git push origin :refs/tags/v0.0.1-test.1`) and, if you added them, the
   fork's secrets. The workflow refuses to create a release when one already exists for the tag.

## How users verify a download

In the folder with both files:

```sh
shasum -a 256 -c CaptureMann-vX.Y.Z-macos-arm64.dmg.sha256     # prints: ...dmg: OK
```

After copying the app to Applications (for a notarized build):

```sh
spctl --assess --type execute --verbose /Applications/CaptureMann.app   # accepted, source=Notarized Developer ID
codesign --verify --deep --strict --verbose=2 /Applications/CaptureMann.app
xcrun stapler validate /Applications/CaptureMann.app
```

For the DMG itself: `spctl --assess --type open --context context:primary-signature --verbose CaptureMann-vX.Y.Z-macos-arm64.dmg`.
An ad hoc build is rejected by `spctl`; that is expected, see the release notes for the first launch.
The checksum only proves the download matches what was uploaded; it does not replace the signature.

## Rollback and yank

- **Draft not published yet:** delete the draft and the tag, fix, tag again.
- **Published and broken:** edit the release and mark it as a pre-release or turn it back into a
  draft so it is no longer "latest"; add a note at the top of the release body saying why. Do not
  delete the asset others may have checksums for unless it is dangerous.
- **Dangerous (data loss, leaked secret, malware concern):** delete the release assets, say so in the
  release body, publish a fixed patch version (`X.Y.Z+1`, never reuse a tag) and add an entry to
  `CHANGELOG.md`. Move the tag only if the release was never public.
- **Signing problem:** a Developer ID certificate or API key that leaked is revoked at Apple. Already
  notarized builds keep working only until Apple revokes their tickets; ship a new build.
- Update the website download link to the previous good version meanwhile.

## What is not automated

- **0.1.0 was built, notarized and uploaded by hand** (`make release` steps, then `gh release create`
  with the workflow disabled); see `verification/RELEASE-0.1.0-2026-10-26-maintainer.md`. The
  workflow itself needs the five repository secrets above and a first run on a fork.
- **Gatekeeper check on a clean Mac** (no Xcode, no Homebrew, a user account that never ran the
  build): download the DMG from the draft in a browser (so it carries the quarantine flag), open
  it, drag the app out, launch it, and grant the permissions. This is the only way to see what a
  user sees; the workflow cannot do it. Record the run in `verification/`.
- Checking that the bundled FFmpeg runs on a Mac without Homebrew.
- The website download link and version on <https://www.craftmann.app/capturemann/>.
- The Homebrew cask in `craft-mann/homebrew-tap` (a script updates it, see step 9).
- Announcing the release.
- A changelog entry and the "Known limitations" list: written by hand.
- No update mechanism, no appcast and no automatic deletion of old releases.
- The workflow has never run on GitHub as of this writing; expect to fix details on the first run.
