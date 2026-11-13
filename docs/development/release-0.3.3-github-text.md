# GitHub release text for 0.3.3 (draft, not published)

Written from `specs/plans/release-0.3.3-plan.md` (sections 2 and 3), `specs/features/02-annotation.md` (ANN-4, ANN-6, ANN-7),
`specs/features/18-import.md` (IMP-6), `CHANGELOG.md` ([Unreleased]) and the 0.3.2 text. **Nothing here is final until the regression
on the final files exists, and a line about a feature that was not built or was cut must be deleted.** Every `<FILL: ...>` is a
placeholder for a fact that only exists on the release day (commit, report names, checksums, run ids, notarization, winget). Never
write "passed" or "verified" without a stored report in `verification/` (CLAUDE.md wording rule). Steps: `release-day-0.3.3.md`.
Lines marked **OPTIONAL** describe work that may be cut (stamps, clipboard paste); keep them only if the feature is in the tagged commit.

- **Tag:** `v0.3.3` (never reuse a tag). **Title:** `CaptureMann 0.3.3: callouts, spotlight and step labels, import pictures, clearer text fields`.
- **Type:** draft, then **pre-release** (the version is 0.x). Covers Windows 11 (x64), macOS 13+ (Apple Silicon and Intel) and Linux
  (Ubuntu 24.04+, x86_64 and aarch64). It becomes the next "Latest".
- **Files to attach:** as 0.3.2 with the version changed: Windows `capturemann-0.3.3-windows-x64-setup.exe`, `...-windows-x64.zip`,
  `.sha256` files and `SHA256SUMS.txt`; macOS `CaptureMann-v0.3.3-macos-arm64.dmg` and `CaptureMann-v0.3.3-macos-x86_64.dmg` with
  `.sha256` (rename the `make release` output `CaptureMann-0.3.3-<arch>.dmg` to `CaptureMann-v0.3.3-macos-<arch>.dmg`, write the `.sha256` files again with the new names); Linux
  `capturemann-0.3.3-linux-{x86_64,aarch64}.tar.gz`, `capturemann_0.3.3_{amd64,arm64}.deb`, `.sha256`, `.minisig`. APT repository:
  release `apt` (assets), run `make-apt-repo.sh` again with the `.deb` files of 0.3.0 to 0.3.3.
- **Headline numbers, where they come from:** 81 MCP tools (unchanged from 0.3.2: no new tool, the new ops and the image import use
  `annotate` and `import_media`; `crates/cm-mcp/tests/stdio.rs` asserts `list.len() == 81`); 21 Auto-redact kinds (unchanged); five
  plugin skills (unchanged, the screenshot skill documents the new ops).

---

## Release body

# CaptureMann 0.3.3: callouts, spotlight and step labels, import pictures, clearer text fields

0.3.3 has files for **Windows 11, macOS and Linux**. Main changes: the screenshot editor gets new annotation tools and a tidier
layout, and pictures can be imported.

## New

- **Callout (key K).** A text box with a pointer, as a rounded bubble or a plain box. Drag to place the box, then click again or drag
  the blue "Tail tip" dot to aim the pointer; moving the box keeps the tip where it is. The box grows to fit what you type, and what
  is saved is what is drawn. Vietnamese Telex and VNI typing work as in the Text tool.
- **Spotlight (key S).** While you drag, the dim and the outline show live. Dims everything except the parts you mark (rectangles, rounded rectangles, ellipses). One spotlight per
  image: drawing another shape adds a hole to it, up to 8, in one undo step. A new spotlight goes under your arrows and callouts, so
  they stay bright. "Outside the holes" can be Dim (default 60 %) or Blur; **Blur here is only a look and hides nothing**: use Blur (B),
  Pixelate (X) or Auto-redact to hide information. Copy text reads the picture as drawn, so text under a blurred spotlight is not copied.
- **Step badges with a label, a numbering style and a start number.** The Steps panel sets 1 2 3, A B C or a b c (after Z come AA, AB)
  and the start number for all steps, lists the steps so you can **drag them into a new order** (the numbers follow, in one undo step),
  and a step can carry a short label drawn in a pill beside the badge (it flips to the left near the image edge).
- **Stamps (key U).** A check, cross, exclamation or question mark drawn as shapes (the same on every
  system) and "Before" and "After" labels written in the app language when placed.
- **Import pictures.** Import (the button, Ctrl+I or Cmd+I, drag and drop, `capturemann import`, `import_media`) now accepts **PNG and
  JPEG** pictures. Each becomes a screenshot in the library with the Imported mark; annotate, redact, Copy text and export work as on
  a screenshot taken here. A phone photo is turned upright, transparency is kept, and picture information such as GPS and camera data is
  **not** copied. Limits: 16,384 px on a side and 50 megapixels. WebP, HEIC, GIF, TIFF, BMP and SVG are refused by their content, with
  the format named. 
- **A tidier editor.** The 16 tool buttons are now **8 grouped buttons** (Select, Shapes, Text, Highlighter, Markers, Emphasis, Redact,
  Crop); a button shows the tool you used last in its group and every tool keeps its key. The bottom **view bar** holds Undo, Redo, Copy
  text, Auto-redact, zoom, Images, Keyboard and a three-dot menu: the zoom value opens a menu (Fit to window, Actual size, 200%, custom)
  and replaces the Fit and 100% buttons, and **Revert to original** moved into the three-dot menu (it still asks first). An **image
  strip** under the picture jumps to another image of the list you opened it from (same list and order as the Library); switching never
  asks to save and each image keeps its own undo; the Images button hides it, and it hides below 640 px of window height.
- **Text fields that are easy to see.** Everywhere you type (rename, search, the inspector, settings, captions and transcript lines, text on a
  screenshot) a focused field now has a **blue frame and ring** (for mouse and keyboard alike), a 2 px **blinking caret** and a **strong blue
  selection** that turns grey when the window is not active; there are error, disabled and read-only looks. Double-click selects a word,
  triple-click selects all, Shift+arrows extend the selection. Search fields get a clear button and **Esc clears first, then leaves the field**.
  **Rename** has a Name label, shows `Enter rename, Esc cancel`, selects the whole name on opening, is disabled while the name is empty or
  unchanged and shows a refusal under the field. **Text on a screenshot** shows an outline that reads on light and dark pictures and a "Type
  text" hint; a selected box shows handles and a "Double-click or Enter to edit" hint, a hovered box a dashed outline, and a text box can be
  resized from all four corners. **Step rows** in the Steps panel: the label is plain text until you edit it; click it or press Enter to edit
  (the whole label is selected), Enter saves and opens the next step, Esc restores, Alt+Up and Alt+Down move the step, Delete removes it.
  **Editing a caption line:** Esc now **cancels** and restores the text (it used to keep the edit).
- **For agents.** `annotate` takes the ops `callout`, `spotlight` and `stamp`, and `step` takes `style`, `start` and `text`; `import_media`
  takes pictures (rows gain `width` and `height`). A second `spotlight` in one call is `invalid_input` naming it (`ops[1]`). No new
  MCP tool: the server still lists 81. The plugin skill `screenshot-and-redact` documents the new ops.

## Not in 0.3.3

- **Magnify** (a loupe over a detail) is on the backlog (ANN-8 and the op name `magnify` are reserved).
- **An in-app update** is not in this release (Velopack spikes are done, `specs/plans/release-0.3.3-plan.md`); updates stay manual.
- **Windowed audio** and **PhoWhisper** have no release assigned.
- **A headless daemon:** `capturemann mcp` still starts no daemon; the app must be open and tools answer `daemon_unavailable` otherwise.
- WebP and HEIC pictures, and keeping the source file of an imported picture.

## Fixed

- **Saving captions while a Google Drive upload waits for you to sign in again.** Editing captions, the transcript or markers of a recording was
  refused ("is being uploaded (job ...); wait for the job") while its upload was paused for the Google sign-in, and the pause never ends by
  itself. A paused upload no longer blocks these edits; an upload that is running still does, and renaming or deleting still waits for any
  unfinished job.

## Compatibility with 0.3.2

- A capture saved by 0.3.3 with a callout, spotlight or stamp **opens in 0.3.2**: the new annotations are kept in the file but **not drawn**,
  and 0.3.2 shows its existing warning "Kept an annotation this version cannot show". If 0.3.2 then saves an edit, the **step label,
  numbering style and start number are lost** (the badges go back to plain numbers); the other fields and ops stay.
- 0.3.3 keeps a field it does not know inside a known annotation and writes it back unchanged, so later versions can add fields without
  this loss. An unknown operation is still kept whole.
- Old captures and the 0.3.2 steps draw exactly as before.

## Changed

- Revert to original, Fit and 100% moved as described above; no setting or shortcut was removed.

## Checked, and what was not

Reports in `verification/` (all PARTIAL; one computer each). The files below were built from commit `85df0235` (the commit the tag
`v0.3.3` points to) and are the files that were regressed (the same files: no rebuild after the tests, except the macOS DMGs, which were
made from a later commit that differs only in documents and stored reports). **This is a pre-release because the Windows regression has not
been run on these files yet** (the lab computer could not boot Windows on release day); it follows and this page is updated.

| System | Files and computer | Tiers run (passed/failed) | Reports |
| --- | --- | --- | --- |
| Windows 11 x64 | installer and zip from the **Windows package** workflow (run 43376683644 on `85df0235`) | **not run**; the workflow's own Windows tests passed (run 43376687740) | none yet |
| macOS arm64 (the maintainer's Mac, Apple M4 Pro) | `CaptureMann-v0.3.3-macos-arm64.dmg`, signed with a Developer ID, notarized and stapled | full 53/0 (22 skipped), on the dev bundle built from the same code (not on the app copied out of the DMG); earlier full runs 54/0 and 46/0 | `L1-macos-0.3.3-regression-full-release-candidate-2026-11-13-claude`, `...-candidate-screen-...`, `...-candidate3-...`, `...-candidate-textfields-...` |
| macOS x86_64 (Intel) | `CaptureMann-v0.3.3-macos-x86_64.dmg`, same code, signed, notarized and stapled; **checked on Apple Silicon only** | not run on Intel hardware | none |
| Linux x86_64 (ThinkPad T490, Ubuntu 24.04, X11) | tarball and `.deb` from `release-build.sh`; both pass `check-package.sh` | full 61/0 (14 skipped), tarball install case included | `L1-linux-0.3.3-regression-full-x11-release-candidate-2026-11-13-claude` |
| Linux aarch64 (Ubuntu 24.04 VM, headless) | tarball and `.deb` from `release-build.sh`; both pass `check-package.sh` | full 40/0 (35 skipped: no desktop session) | `L1-linux-0.3.3-aarch64-vm-release-candidate-85df0235-full-2026-11-13-claude` |

Skipped cases are the ones that need a logged-in desktop session, a model, another system or hardware; no run had a failure on the files
above. Earlier in the day one stale case was fixed (`import.errors` still expected a PNG to be refused) and one macOS CI check and one Windows
CI check failed for reasons that were fixed or turned out to be a flake; the CI run on `85df0235` is the one the release rests on.

The new features were checked like this: the new operations, the step labels, the spotlight rules, the pass-through of unknown fields and the
image import have unit tests and golden images (ANN-4, ANN-6, ANN-7, IMP-6; tested with fakes). The `cm-regress` cases `annotate.pack` (the
new ops through the CLI, pixel statistics of the render, a second spotlight refused) and `import.images` (a PNG and a JPEG imported,
annotated and exported) are part of the macOS and Linux reports above. The editor (palette, view bar, image strip,
callout, spotlight, steps), the text fields, the spotlight drag preview, caption saving and importing pictures were **tried by the maintainer
on macOS** (as reported; no report is stored in `verification/`, and the scripts `ANN-6.md`, `IMP-6.md` and `TEXT-FIELDS.md` were not run as
written).

Not run: **Intel Mac hardware**, Windows 10, SmartScreen after "Keep", mixed DPI, NVIDIA and AMD encoders, KDE and other Linux
distributions, a second monitor, a 16 GB Mac for long recordings, the **Windows regression**, the editor and the text fields on Windows and Linux, a phone photo with an orientation tag from a real phone on Windows and Linux, the Linux `.deb` through APT on a clean
computer, the Linux Wayland session.

## Known limits

- **Pictures:** only PNG and JPEG. A 16-bit, palette or wide-colour picture is converted to 8-bit sRGB; an animated PNG imports its first
  frame; picture information is dropped; the source file is not kept (leave it in place, or remove it at import, which asks first).
- **Spotlight Blur is only a look.** Text under a blurred spotlight is still in the file; hide information with Blur or Pixelate.
- **Opening a 0.3.3 capture in 0.3.2** does not draw the new annotations and loses step labels on save (see Compatibility).
- **The image strip uses the Library thumbnails**, loaded as you scroll; a very large library was not timed.
- **The Windows files are not code-signed yet** (free signing for open source is being arranged; the options and the recommendation are
  in `docs/development/windows-signing-options.md`). Edge may say the installer "isn't commonly downloaded": open the download's menu,
  choose Keep, then Show more and Keep anyway. When you run it, Windows may show "Windows protected your PC": choose More info, then Run
  anyway. Before you do, check the file's SHA-256 against `SHA256SUMS.txt` on this page (`certutil -hashfile <file> SHA256`). On a PC with
  Smart App Control turned on the installer cannot run; use the portable zip or wait for a signed release.
- **Updates are manual in 0.3.3**: download the new file, or `brew upgrade --cask` and `apt upgrade` where you installed that way; the app
  makes no network call of its own.
- Carried over from 0.3.2, unchanged: Copy text reads what the system's recognizer can read (Windows: only the installed OCR languages;
  Linux: English unless `tesseract` language data is installed); the plugin needs the desktop app running and a local session; the typical
  export size is a calibrated guess (one Mac); long recordings with transcript and speakers at the same time peaked at 11.7 GB on a 48 GB Mac
  and were not tried on a 16 GB Mac.

## Install and checks

| File | SHA-256 |
|---|---|
| `capturemann-0.3.3-windows-x64-setup.exe` | `7791304199c484b7eb4dba5571d4a8c4b75c4b2bdf69bbcd0bd6d3868e7af7cd` |
| `capturemann-0.3.3-windows-x64.zip` | `185a49e5e7474c299a06360ed07239350777f0f8187454f43d7f2a47b47dd624` |
| `CaptureMann-v0.3.3-macos-arm64.dmg` | `09747c0cc9d9bd04215c88bf03501898b944c773777df0bf65edc5472d4b1ba8` |
| `CaptureMann-v0.3.3-macos-x86_64.dmg` | `6f493e15197ce1ab1d320c3018798ecd7d2c7cf5da53a5d0a626e54108da7d28` |
| `capturemann-0.3.3-linux-x86_64.tar.gz` | `4f392dc303fe4f938ac464022b19d81a5e1f62eb4b6fd33295dc2548c03ef2e0` |
| `capturemann_0.3.3_amd64.deb` | `3efc0d6594c47cee119e2ac9a809e98cbb6cd6c138f7f0065a6951a15a6b4e91` |
| `capturemann-0.3.3-linux-aarch64.tar.gz` | `e36e2a5f9056d347c1c692673e73877130862eed0753c19c1c400df2fc4bb808` |
| `capturemann_0.3.3_arm64.deb` | `6166840bb79e4214982a8fd8f60f26609c01b195a2c5effaf1baf2bbe8f563ef` |

Minisign public key: `packaging/linux/minisign.pub`; public key `RWTeQv8ogB2CCuZ7j0rr3MovdHod4QATmVn+I329YC8pomOC6R9Rxc2k` (verify: `minisign -Vm <file> -P <that key>`).

- **Windows 11, x64:** installer `capturemann-0.3.3-windows-x64-setup.exe` (per user, no administrator rights; tick "Add
  capturemann.exe and cm-mcp.exe to my PATH" if you will use the CLI or the Claude plugin) or the zip `capturemann-0.3.3-windows-x64.zip`
  (unpack anywhere, run `cm-desktop.exe`; `capturemann.exe` is the command line). Check with `certutil -hashfile <file> SHA256` or
  `SHA256SUMS.txt`. **winget:** 0.3.3 is **not** in winget. Pull requests 472327 (0.2.8) and 477195
  (0.3.1) to microsoft/winget-pkgs are still open, so `winget install CaptureMann.CaptureMann` does not work yet; use the installer or
  the zip. A 0.3.3 update follows once the first package is accepted.
- **macOS 13+:** `CaptureMann-v0.3.3-macos-arm64.dmg` (Apple Silicon) or `CaptureMann-v0.3.3-macos-x86_64.dmg` (Intel);
  signed with a Developer ID, notarized and stapled.
  Or `brew upgrade --cask craft-mann/tap/capturemann` (`brew install --cask craft-mann/tap/capturemann` for a first install; the cask
  moves to 0.3.3 shortly after this release is published). Check with `shasum -a 256 -c <file>.sha256`.
- **Linux (Ubuntu 24.04 or newer, Debian 13, or glibc 2.39+; amd64 and arm64):** `sudo apt install ./capturemann_0.3.3_amd64.deb` (or
  `_arm64.deb`), or the tarball with `./check-runtime.sh` and `./install.sh`. Copy text and Auto-redact need
  `sudo apt install tesseract-ocr` (and `tesseract-ocr-<lang>` for other languages). Or the APT repository:

```sh
U=https://github.com/craft-mann/capturemann/releases/download/apt
sudo curl -fsSL --retry 5 --retry-all-errors $U/capturemann-archive-keyring.gpg -o /usr/share/keyrings/capturemann-archive-keyring.gpg
sudo curl -fsSL --retry 5 --retry-all-errors $U/capturemann.sources -o /etc/apt/sources.list.d/capturemann.sources
sudo apt -o Acquire::Retries=5 update && sudo apt -o Acquire::Retries=5 install capturemann
```

GitHub's release host sometimes answers the first download of a file with a `504`; repeating the command works (the `Retries` option does
it for you). The repository is hosted on GitHub release assets for now; it carries 0.3.0 to 0.3.2 now, and 0.3.3 is added shortly after this release is published.

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
nothing GPL is linked or bundled. No dependency was added since 0.3.2: `Cargo.lock` differs only because the test tool `cm-regress` (not shipped) now uses the `png` crate that was already in the tree, and `THIRD_PARTY_LICENSES.md` is unchanged.

---

## CHANGELOG section (turn `## [Unreleased]` into this on the release day; `release.yml` reads it)

```
## [0.3.3] - 2026-11-13
```

The body under it is the current `[Unreleased]` text of `CHANGELOG.md` (Added: callout, spotlight, step label, style and start, reorder, stamps if
built, image import, the grouped palette, the view bar, the image strip; Changed: Revert to original, Fit and 100% moved). Do not rewrite it
here; the release body above is the long form.
