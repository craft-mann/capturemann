# GitHub release text for 0.3.2 (draft, not published)

Prepared the night of Nov 11 to 12, 2026 from `specs/plans/release-0.3.2-plan.md`, `specs/plans/release-0.3.2-status.md`,
`CHANGELOG.md` ([Unreleased]) and the 0.3.1 and 0.2.9 texts. **Nothing here is final until the regression on the final files
exists.** Every `<FILL: ...>` is a placeholder for a fact that only exists on the release day (report names, checksums, run ids,
notarization, winget). Never write "passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule).
Steps: `release-day-0.3.2.md`. Candidate files were built overnight from the commit the tag will point to; a report on them says
"candidate files of commit <sha>", and the table below is filled from the reports on the **final** files built from the tag, or
from the candidate reports plus the smoke check of the final files (see the release-day steps, section 4).

- **Tag:** `v0.3.2` (never reuse a tag). **Title:** `CaptureMann 0.3.2: copy text from a screenshot, a Claude plugin`.
- **Type:** draft, then **pre-release** (the version is 0.x). Covers Windows 11 (x64), macOS 13+ (Apple Silicon and Intel) and Linux
  (Ubuntu 24.04+, x86_64 and aarch64). It becomes the next "Latest".
- **Files to attach:** Windows `capturemann-0.3.2-windows-x64-setup.exe`, `...-windows-x64.zip`, `.sha256` files and `SHA256SUMS.txt`;
  macOS `CaptureMann-v0.3.2-macos-arm64.dmg` and `CaptureMann-v0.3.2-macos-x86_64.dmg` with `.sha256` (rename the
  `make release` output `CaptureMann-0.3.2-<arch>.dmg` as in the 0.2.9 text; the checksums do not change, write the `.sha256`
  files again with the new names); Linux `capturemann-0.3.2-linux-{x86_64,aarch64}.tar.gz`,
  `capturemann_0.3.2_{amd64,arm64}.deb`, `.sha256`, `.minisig`. APT repository: release `apt` (assets), run
  `make-apt-repo.sh` again with both `.deb` files.
- **Headline numbers, where they come from:** 81 MCP tools (`crates/cm-mcp/tests/stdio.rs` asserts `list.len() == 81`; the README
  and the `compare/` pages say 81 since commit `eb89c833`); 21 Auto-redact kinds (unchanged, `specs/features/17-redaction.md`); five plugin
  skills (`plugins/capturemann/skills/`).

---

## Release body

# CaptureMann 0.3.2: copy text from a screenshot, a Claude plugin

0.3.2 has files for **Windows 11, macOS and Linux**. Main changes:

## New

- **Copy text from a screenshot.** A **Copy text** button and a **Select text** tool (key G) in the editor, a "Copy text" item in
  the library menu and a button on the capture toast read the text of a screenshot and put it on the clipboard. It uses the same
  on-device recognizer as Auto-redact (Apple Vision on macOS, the recognizer built into Windows, the `tesseract` program on Linux)
  and reads the **rendered image**, so a blurred or pixelated region never comes back as text. Lines come out in reading order, with
  a blank line between paragraphs and columns read one after the other. Nothing is stored and nothing leaves the computer. For
  agents: the `recognize_text` MCP tool and `capturemann text` (same options: a region in pixels of the original, copy to the
  clipboard, text or lines with boxes and confidence). **What each system reads:** macOS reads the languages Apple Vision supports;
  Windows reads only the OCR languages you have installed (Settings, Time and language, Language and region); Linux reads English
  with the `eng.traineddata` that comes with `tesseract`, and other languages only when you install their language data
  (`sudo apt install tesseract-ocr-<lang>`; `CM_TESSDATA` points at another folder). Handwriting and heavily stylised text are not
  promised on any system. Accuracy: the recognizer read a synthetic English and a synthetic Vietnamese screenshot on one Mac
  (character error rate 0.000 and 0.078, `verification/benchmarks/ocr-copy-text-synthetic-macos-2026-11-11.md`); no accuracy is
  claimed beyond that, and Windows and Linux were not measured for this feature. The MCP server now lists **81 tools**.
- **A Claude plugin.** `claude plugin marketplace add craft-mann/capturemann` then
  `claude plugin install capturemann@capturemann-plugins` connects the CaptureMann MCP server to Claude Code and adds five skills
  (the basics, screenshots with redaction, screen recording, meeting notes, export and publishing). CaptureMann must be installed and
  running on the same computer: the plugin contains no binary and adds no network traffic; it runs the app's own `capturemann mcp`.
  On macOS the `capturemann` command is inside the app, so either link it once
  (`sudo ln -s /Applications/CaptureMann.app/Contents/MacOS/capturemann /usr/local/bin/capturemann`) or give the full path when Claude
  Code asks for **CaptureMann command** at install. The plugin version follows the app version (0.3.2); install the matching app
  release first. How it works and what to do when it does not: `docs/user/claude-code-plugin.md`. The plugin is also prepared for
  Anthropic's directory (`docs/development/plugin-submission-0.3.2.md`); <FILL: "it is submitted and under review" or "the submission
  follows the release", whichever the maintainer did>. The local MCP server does not run in Claude on the web or on a phone; there the
  skills load but the tools are missing, and the plugin says so.
- **Agent clients appear in Settings → Agents & CLI as soon as they connect**, with their last tool and call count, so you can allow,
  restrict or block a client before it asks for anything that needs confirmation. Before, a client that only called Free tools (most
  of them, including `generate_report`) never showed up in the list.

## Changed

- The "Add client" form preselects **Allowed** (the documented default) instead of Blocked.

## Fixed

- **The library thumbnail of a redacted screenshot kept showing the old picture** until the app restarted; the grid now drops its
  cached crop when a capture changes. The file on disk was already right, so existing libraries need no migration.
- **Change shortcuts:** a combination with Shift and a digit (Option+Shift+5) is accepted; the shortcuts are released while you press
  a new combination, so one that is already taken shows "Already used by ..." instead of starting that action; the reason a key is
  refused is shown under the row you are changing, not at the end of the page; Reset to defaults no longer loses a shortcut when two
  of them swap places. Checked with a keyboard on macOS by the maintainer (as reported); the keyboard check on Windows and Linux X11
  (`tests/manual/SHORTCUTS-1.md`) was not run.
- **The sidebar counts now follow a new screenshot or recording on every page** (they stayed behind on the Screenshots, Recordings and
  Audio pages and during a search), and the screenshot is announced before the clipboard copy, so the count moves as soon as the file
  exists.

## Checked, and what was not

Reports in `verification/` (all PARTIAL; one computer each). Candidate files were built overnight from `<FILL: release commit sha>`,
the commit the tag points to, and regressed; the final files built from the tag were <FILL: "byte-identical to the candidates
(same SHA-256)" or "re-checked with a `cm-regress` smoke run per system (report names below)">.

| System | Files and computer | Tiers run (passed/failed) | Reports |
| --- | --- | --- | --- |
| Windows 11 x64 (ThinkPad T490, Intel UHD 620) | installer and zip from the **Windows package** workflow, <FILL: run id, candidate or tag>; silent install | standard <FILL: n/0>, full <FILL: n/0>, transcription, speakers and the silent installer <FILL: n/n> | `<FILL: L1-windows-0.3.2-regression-*>` |
| macOS arm64 (the maintainer's Mac, Apple M4 Pro) | `CaptureMann-v0.3.2-macos-arm64.dmg`, <FILL: "signed with a Developer ID, notarized and stapled" or "not notarized">; the app copied out of the DMG | standard <FILL: n/0>, full <FILL: n/0>; hardware <FILL: "not run" or n/0, with the maintainer's consent> | `<FILL: L1-macos-0.3.2-regression-*>` |
| macOS x86_64 (Intel) | `CaptureMann-v0.3.2-macos-x86_64.dmg`, same build; **checked on Apple Silicon only** (DMG install check, the command-line tool under Rosetta) | not run on Intel hardware | `<FILL: the x86_64 DMG check report>` |
| Linux x86_64 (same ThinkPad, Ubuntu 24.04, X11) | tarball and `.deb` from `release-build.sh` | standard <FILL: n/0>, full <FILL: n/0>, `.deb` install <FILL: 1/0> | `<FILL: L1-linux-0.3.2-regression-*>` |
| Linux aarch64 (Ubuntu 24.04 VM, headless) | tarball and `.deb` from `release-build.sh` | full <FILL: n/0> | `<FILL: L1-linux-0.3.2-aarch64-*>` |

<FILL: delete any row or cell whose report does not exist; if a run had a failure, name it and say whether it was explained.>

The new features were checked like this: Copy text, the clients list, the redacted thumbnail, the shortcut fixes and the sidebar counts
have unit tests with fakes (ANN-5, AGT-2.AC15, LIB-1.AC6 and RED-2.AC5, CAP-2.AC2 and AC3, LIB-1.AC7) and were tried by the maintainer
on a macOS dev build on Nov 11, 2026 (as reported, `specs/plans/release-0.3.2-status.md`, "Dev bundle checks"; nothing stored in
`verification/`). The plugin passed `claude plugin validate --strict` (Claude Code 2.1.311) on Nov 11, 2026; `tests/manual/PLUGIN-1.md`
(a real session against the app) <FILL: "was run by the maintainer, report ..." or "was not run">.

Not run: **Intel Mac hardware**, Windows 10, SmartScreen after "Keep" (the Edge "isn't commonly downloaded" panel was seen on 0.3.1,
`verification/L1-windows-0.3.1-download-smartscreen-edge-2026-11-11-maintainer.md`), mixed DPI, NVIDIA and AMD encoders, KDE and
other Linux distributions, a second monitor, a 16 GB Mac for long recordings, the keyboard check of the shortcut fixes on Windows and
Linux X11, Copy text on real screenshots and on the Windows and Linux recognizers, the Linux `.deb` through APT on a clean computer,
the Linux Wayland session, the macOS 13 floor of the text recognizer.

## Known limits

- **Copy text reads what the system's recognizer can read.** Windows: only the OCR languages installed in Windows (Vietnamese and other
  languages need their language pack). Linux: English by default; other languages need `tesseract` language data. Without a recognizer
  (Linux without `tesseract`, Windows without a usable OCR language) the buttons are dimmed with the install hint, as Auto-redact's are.
  Accuracy is measured on two synthetic images on one Mac only (above).
- **The plugin needs the desktop app and a local session.** It runs in Claude Code and in Cowork on the user's computer; in Claude on the
  web the local server is ignored. A client that connected before 0.3.2 and only made Free calls was not in the Clients list; from 0.3.2 it
  is, from its next `session.hello`.
- **The Windows files are not code-signed yet** (free signing for open source is being arranged; the options and the recommendation are
  in `docs/development/windows-signing-options.md`). Edge may say the installer "isn't commonly downloaded": open the download's menu,
  choose Keep, then Show more and Keep anyway. When you run it, Windows may show "Windows protected your PC": choose More info, then Run
  anyway. Before you do, check the file's SHA-256 against `SHA256SUMS.txt` on this page (`certutil -hashfile <file> SHA256`). On a PC with
  Smart App Control turned on the installer cannot run; use the portable zip or wait for a signed release.
- **Updates are manual in 0.3.2**: download the new file, or `brew upgrade --cask` and `apt upgrade` where you installed that way. An
  in-app update check is planned for 0.3.3 (`specs/plans/release-0.3.3-plan.md`); the app makes no network call of its own.
- Carried over from 0.3.1, unchanged: the typical export size is a calibrated guess (one Mac, one user's screens; Windows and Linux
  encoders, 60 fps, 4K and Intel are not calibrated); on Linux GPUs that take no bit rate (VA-API constant quality, Intel UHD 620 with
  iHD) the MP4 "Up to" figure is a scaled worst case with no "About" figure; an MP3's length is an estimate in the file; the guard
  against sentences nobody said runs on Vietnamese transcripts only; a rare failure of one daemon socket test
  (`r2_7_stale_socket_is_replaced_live_one_is_not`) was seen once in CI and is not explained; long recordings with transcript and
  speakers at the same time peaked at 11.7 GB on a 48 GB Mac and were not tried on a 16 GB Mac (start speakers after the transcript
  there).

## Install and checks

| File | SHA-256 |
|---|---|
| `capturemann-0.3.2-windows-x64-setup.exe` | `<FILL>` |
| `capturemann-0.3.2-windows-x64.zip` | `<FILL>` |
| `CaptureMann-v0.3.2-macos-arm64.dmg` | `<FILL>` |
| `CaptureMann-v0.3.2-macos-x86_64.dmg` | `<FILL>` |
| `capturemann-0.3.2-linux-x86_64.tar.gz` | `<FILL>` |
| `capturemann_0.3.2_amd64.deb` | `<FILL>` |
| `capturemann-0.3.2-linux-aarch64.tar.gz` | `<FILL>` |
| `capturemann_0.3.2_arm64.deb` | `<FILL>` |

Minisign public key: `packaging/linux/minisign.pub`; public key `<FILL: the key line from packaging/linux/minisign.pub; 0.3.1 used
RWTeQv8ogB2CCuZ7j0rr3MovdHod4QATmVn+I329YC8pomOC6R9Rxc2k>` (verify: `minisign -Vm <file> -P <that key>`).

- **Windows 11, x64:** installer `capturemann-0.3.2-windows-x64-setup.exe` (per user, no administrator rights; tick "Add
  capturemann.exe and cm-mcp.exe to my PATH" if you will use the CLI or the Claude plugin) or the zip `capturemann-0.3.2-windows-x64.zip`
  (unpack anywhere, run `cm-desktop.exe`; `capturemann.exe` is the command line). Check with `certutil -hashfile <file> SHA256` or
  `SHA256SUMS.txt`. **winget:** 0.3.2 is **not** in winget. <FILL: on the release day, one of: "Pull requests 472327 (0.2.8) and 477195
  (0.3.1) to microsoft/winget-pkgs are still open, so `winget install CaptureMann.CaptureMann` does not work yet; use the installer or
  the zip. A 0.3.2 update follows once the first package is accepted." or "winget has <version> (pull request <n> merged); a 0.3.2 update
  is submitted as pull request <n>.">
- **macOS 13+:** `CaptureMann-v0.3.2-macos-arm64.dmg` (Apple Silicon) or `CaptureMann-v0.3.2-macos-x86_64.dmg` (Intel);
  <FILL: "signed with a Developer ID, notarized and stapled" or "not notarized: right-click the app and choose Open the first time">.
  Or `brew upgrade --cask craft-mann/tap/capturemann` (`brew install --cask craft-mann/tap/capturemann` for a first install; the cask
  is at <FILL: 0.3.2, tap commit ...> ). Check with `shasum -a 256 -c <file>.sha256`.
- **Linux (Ubuntu 24.04 or newer, Debian 13, or glibc 2.39+; amd64 and arm64):** `sudo apt install ./capturemann_0.3.2_amd64.deb` (or
  `_arm64.deb`), or the tarball with `./check-runtime.sh` and `./install.sh`. Copy text and Auto-redact need
  `sudo apt install tesseract-ocr` (and `tesseract-ocr-<lang>` for other languages). Or the APT repository:

```sh
U=https://github.com/craft-mann/capturemann/releases/download/apt
sudo curl -fsSL --retry 5 --retry-all-errors $U/capturemann-archive-keyring.gpg -o /usr/share/keyrings/capturemann-archive-keyring.gpg
sudo curl -fsSL --retry 5 --retry-all-errors $U/capturemann.sources -o /etc/apt/sources.list.d/capturemann.sources
sudo apt -o Acquire::Retries=5 update && sudo apt -o Acquire::Retries=5 install capturemann
```

GitHub's release host sometimes answers the first download of a file with a `504`; repeating the command works (the `Retries` option does
it for you). The repository is hosted on GitHub release assets for now; it carries <FILL: "0.3.0, 0.3.1 and 0.3.2" once make-apt-repo.sh
has run>.

Repository key fingerprint: `516C CD4F C7B3 5167 8803  F71B DEB1 BB54 FAEF 557D` (RSA 4096, signing only, expires Nov 8, 2031). The
repository is the set of files of the release tagged [`apt`](https://github.com/craft-mann/capturemann/releases/tag/apt). The key and
source files must be the ones from that release; check the fingerprint with `gpg --show-keys /usr/share/keyrings/capturemann-archive-keyring.gpg`.

- **The Claude plugin** (any system, after the app is installed and running): `claude plugin marketplace add craft-mann/capturemann`,
  then `claude plugin install capturemann@capturemann-plugins`; inside a session,
  `/plugin install capturemann --marketplace craft-mann/capturemann`. Claude Code 2.1.281 or later.

Report problems at the issue tracker with the output of `capturemann diagnostics`.

## Licences

Apache-2.0. The packages carry `THIRD_PARTY_LICENSES.md`. FFmpeg is an LGPL build (no GPL code, no libx264); its source offer is in the
package. The Windows build and the Linux `.deb` and tarball use the recognizer of the system (Windows.Media.Ocr, the `tesseract` program);
nothing GPL is linked or bundled. <FILL: "The Rust dependency set did not change from 0.3.1 (`make licenses` left `THIRD_PARTY_LICENSES.md`
unchanged)" or what `git diff v0.3.1 -- THIRD_PARTY_LICENSES.md` shows.>

---

## CHANGELOG section (turn `## [Unreleased]` into this on the release day; `release.yml` reads it)

```
## [0.3.2] - 2026-11-<FILL: day>
```

The body under it is the current `[Unreleased]` text of `CHANGELOG.md` (Added: Copy text, the Claude plugin, agent clients in the list;
Fixed: the redacted thumbnail, Change shortcuts, the sidebar counts; Changed: "Add client" preselects Allowed). Do not rewrite it here;
the release body above is the long form.
