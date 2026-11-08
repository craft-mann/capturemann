# winget wording for the 0.2.9 release day (Nov 9, 2026)

Decision of Nov 8, 2026: 0.2.9 ships without winget. The package `CaptureMann.CaptureMann` is submitted for **0.2.8**
(<https://github.com/microsoft/winget-pkgs/pull/472327>, state on Nov 8: open, validation passed, waiting for a moderator).
Replace the bracketed part if the pull request is merged before you paste.

## 0.2.8 GitHub release notes (add under the install section)

> **winget (Windows):** a winget package for this version, `CaptureMann.CaptureMann` 0.2.8, is submitted and waiting for review
> by the winget-pkgs moderators (<https://github.com/microsoft/winget-pkgs/pull/472327>). Until it is accepted, `winget
> install CaptureMann.CaptureMann` does not work: use the installer or the zip on this page. When it is accepted, winget
> will install **0.2.8**; newer versions (0.2.9 and later) reach winget in a later update, so check the version number
> (`winget show CaptureMann.CaptureMann`) against the latest release.

## Website and README (install section for Windows)

> **Windows 11 (x64):** download `capturemann-<version>-windows-x64-setup.exe` (or the zip) from the latest release.
> **winget:** version **0.2.8** only, and only after the winget-pkgs review is finished (pending on Nov 8, 2026). The latest
> release is **0.2.9**; for it use the installer or the zip.

README: the paragraph "A winget package (`winget install CaptureMann.CaptureMann`) is submitted ..." (Windows section, line about
the pull request) and the status block's "a winget package is submitted, not yet accepted" take the same wording on the
release day (the release-day steps already list the README status rewrite).

## Later

When 472327 is merged: update the lines above to "winget has 0.2.8", then prepare the 0.2.9 update (`release-day-0.2.9.md`
step 7: `winget validate`, copy the rendered manifest to a fork of `microsoft/winget-pkgs`, pull request "New version:
CaptureMann.CaptureMann version 0.2.9").
