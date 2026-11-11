# Velopack spike for 0.3.2: Windows (Nov 11, 2026)

Scope: Phase 0 of the in-app update workstream (`specs/plans/release-0.3.2-plan.md`, section "In-app update with Velopack"), Windows side. The macOS side, the crate audit and the hosting check are in `docs/development/velopack-spike-0.3.2.md` (branch `track/velopack-spike-mac`); this report reuses their crate facts (velopack 1.2.161, `ureq` 3.5.1, `HttpSource` query, SHA-256 check, `deny.toml` findings) and does not repeat them. Nothing here is product code: the scratch code is `spikes/velopack-win/` (own workspace; helper scripts in `spikes/velopack-win/scripts/`). Nothing was published, tagged, uploaded or pushed. Wording rule: every result is "run on the ThinkPad T490 (Windows 11 Pro 10.0.26200, x64, user `trung`, session via ssh) on Nov 11, 2026" unless it says "not run" or "from the docs/source".

Versions: `vpk` **1.2.161** (installed two ways: `dotnet tool install --tool-path` with the .NET SDK **8.0.428** installed user-locally by `dotnet-install.ps1 -Channel 8.0 -InstallDir C:\lab\velospike\dotnet -NoPath` on the laptop; and on the Mac with the Homebrew .NET 10 SDK and `DOTNET_ROLL_FORWARD=Major`), `velopack` crate **1.2.161**, `ureq` 3.5.1. Payload for the installer tests: the **public 0.3.1 Windows zip** (`capturemann-0.3.1-windows-x64.zip`, 112,725,636 bytes, SHA-256 `44c3584b...cc78` checked against the published `.sha256`) and, for the NSIS comparison, the public `capturemann-0.3.1-windows-x64-setup.exe` (86,587,794 bytes, SHA-256 `d9ffd956...cc78`, the laptop's copy checked against the published `.sha256`).

## Recommendation for Windows: GO WITH CHANGES

Velopack works on Windows as documented and better than the NSIS installer on speed: a per-user, no-admin `Setup.exe` installs the real 0.3.1 payload in 2 s (NSIS 17 s), an update from a plain HTTP feed downloads, applies, restarts the app and finishes in about 1.4 s of down time, a skipped version chains deltas, a truncated, bit-flipped or wrong-hash package is refused before anything is applied, and `vpk` packs (and delta-packs) from the Mac in 8 s. It is a go only with these changes, none optional: (1) **`VelopackApp::build().set_auto_apply_on_startup(false)`**: the default is on and any started exe that calls `run()` applies a downloaded package by itself, which would bypass the Confirm tier and the busy refusal; (2) **there is no rollback and no health check**: a new version whose exe does not start leaves the user on a dead install (reproduced, no restart, no revert), and `Update.exe --uninstall` of such an install blocked until killed, so the release gate needs a "install the packed Setup silently and start it" smoke test before upload, and Phase 1 must treat a hook failure as non-fatal because Velopack does (exit code 5 from the update hook was only a warning); (3) **no default network timeout**: a server that accepts and never answers hung the check for more than 45 s, so the product must use `HttpSource::new_with_options` with a timeout (and the stock source sends a persistent random `stagingId`, see the Mac report: write a small custom `UpdateSource` if that is not acceptable); (4) **Windows is unsigned, so the integrity story is the feed's SHA-256 over HTTPS and nothing else**: Option (b) of the plan (detached minisign check between download and apply) works only if the check sits inside a custom `UpdateSource` that verifies each file as it is fetched, because the client deletes downloaded delta files after patching and the rebuilt full package is not byte-identical to the published one (118,205,960 vs 115,247,135 bytes here); (5) **one-time manual migration**: 0.3.1 NSIS and zip installs cannot self-update (`UpdateManager::new` returns `NotInstalled` in the NSIS folder), the Velopack `Setup.exe` does not detect or remove an NSIS install (two entries named "CaptureMann" in Apps), but an after-install hook can run the NSIS uninstaller silently (tested from outside the hook: 6 s, removes the PATH entry, leaves settings). Also needed: choose `packId` so that it does not equal `CaptureMann` (the Apps key name is the pack id, the NSIS key is `CaptureMann`), put `capturemann.exe` on PATH from a hook (works, 0.25 s) and write the winget manifest for the new installer type (not run). If the maintainer prefers fewer moving parts, the notify-only fallback in the plan stays valid; nothing below makes Velopack on Windows a no-go.

## Five most important findings

1. **Locks: Velopack kills every process whose exe lives in the install folder, with no prompt, but a process whose working directory is `current\` makes the update fail.** Stand-ins for `cm-daemon.exe`, `cm-overlay.exe` and `cm-mcp.exe` were killed (`Killing process: ...\current\cm-daemon.exe`) and the update succeeded; a `cmd.exe` sitting in `current\` (a user's terminal) blocked the folder swap: 10 retries at 1 s, then "Unable to start the update, because one or more running processes prevented it", and Update.exe restarted the old version. The product must stop its own helpers gracefully first (a recording or job in flight is lost by `TerminateProcess`; the existing busy refusal covers it) and must report the failure.
2. **`run()` is cheap (2 to 3 ms), the crate costs 2.8 MB, and `Update.exe` does not run unless something applies or uninstalls.** Starting the stub launches `current\<mainExe>` and the stub exits; nothing else stays resident. A hook (`--veloapp-install|updated|obsolete|uninstall`) is the same exe called with two arguments and must finish within about 15 s; `run()` calls `exit(0)` for them, so it has to come before any argument parsing in `main` (a GUI-subsystem exe works).
3. **Delta size on the real payload is 25 %, and the 33 MB speaker models are never in it.** 0.2.9 to 0.3.1: full 112,283,547 bytes, delta 28,661,865 bytes (17 of 36 files patched, 19 unchanged). Rebuilding the full package from a delta takes **7.1 to 7.8 s** of CPU on the T490 (the download itself is the small part); a full download over loopback takes 0.9 s.
4. **Install and uninstall footprint.** Velopack installs to `%LocalAppData%\<packId>` (`Update.exe`, stub `<title>.exe`, `current\`, `packages\`), uses 373 MB on disk against 246 MB for NSIS because the full package stays in `packages\` as the delta base, shows one Apps & features entry (name from `--packTitle`, publisher from `--packAuthors`, `DisplayVersion` loses the pre-release tag), uninstalls in 4 s and leaves `%APPDATA%` settings, the NSIS install and PATH alone. Setup.exe for the real payload is 119.9 MB (NSIS 86.6 MB).
5. **Cross-packing works and the 0.3.1 line is no longer a pre-release.** `vpk '[win]' pack` on the Mac produced Setup.exe, the full and delta packages and the feed in 8 s; `--msi`, `--signParams` and `--azureTrustedSignFile` exist only on a Windows host (MSI built on the laptop in 80 s, 105.9 MB, not installed). The GitHub API lists v0.3.1 with `prerelease: false` and `releases/latest/download/...` resolves to it, so the plan's remark that all 0.3.x releases are pre-releases is out of date (see section 5).

## 1. Installer

### 1.1 What `vpk pack` produces (real 0.3.1 payload)

Command (on the Mac, cross-packing; helper `scripts/pack-real.sh`):

```
vpk '[win]' pack -u CaptureMannReal -v 0.3.1 -p payload031/capturemann-0.3.1-windows-x64 -e cm-desktop.exe \
    -o out-real -r win-x64 --packTitle CaptureMann --packAuthors "CaptureMann contributors" -i .../app.ico --shortcuts StartMenuRoot
[15:28:09 INF] Directive enabled for cross-compiling from OSX (current os) to Windows.
[15:28:09 WRN] No signing parameters provided, 10 file(s) will not be signed.
[15:28:09 INF] Setup bundle created 'dev.capturemann.spike-win-Setup.exe'.   (pack seconds: 8)
```

| File | Bytes | Note |
| --- | ---: | --- |
| `<id>-<ver>-full.nupkg` | 112,314,598 | the whole payload, zip; the one the client downloads on a first update |
| `<id>-win-Setup.exe` | 119,939,814 | one click, embeds the full package plus `Update.exe`; NSIS `setup.exe` is 86,587,794 |
| `<id>-win-Portable.zip` | 112,298,298 | `.portable`, stub `CaptureMann.exe`, `Update.exe`, `current\` (the published 0.3.1 zip is 112,725,636, flat) |
| `releases.win.json` | 277 | `Assets[]`: PackageId, Version, Type Full or Delta, FileName, SHA1, **SHA256**, Size |
| `RELEASES`, `assets.win.json` | 92, 235 | legacy list (UTF-8 BOM) and the asset list for `vpk upload` |
| `<id>-<ver>-delta.nupkg` | see 2.2 | from the second pack on, needs the previous full package in `-o` |
| `<id>-win.msi` | 105,914,368 | **Windows host only** (`--msi`, WiX fetched by vpk, 80 s); machine-wide bootstrap, needs admin; **not installed** |

Facts that bit or will bite: without `--runtime win-x64` vpk warns "No architecture specified ... defaulting to x86"; `--shortcuts` defaults to `Desktop,StartMenuRoot` (a Desktop shortcut we do not create today; the NSIS installer puts a `CaptureMann` folder in the Start Menu); `--noPortable` and `--noInst` cannot be combined; **vpk does not check that a Rust exe calls `VelopackApp`** (it packed an exe containing the text "this is not an executable" without complaint, even without `--skipVeloAppCheck`), so CI needs its own check; the stub `CaptureMann.exe` and `Update.exe` carry no file description of ours (`Update.exe`: product "Velopack", company "Velopack", the same bytes in every Velopack app, SHA-256 `4E1263A1...A01F`; the stub has an empty version resource), while `Setup.exe` carries ours (description and product `CaptureMann`, company from `--packAuthors`): relevant to the SignPath rule "product name metadata must match the project name". Subsystems (PE header): `Update.exe`, stub and `Setup.exe` are GUI, `capturemann.exe` console.

### 1.2 Install, location, Apps & features, admin

Scratch app packaged with the real payload (`CaptureMannSpike`, `vpk` run from the Mac, `Setup.exe` pushed to the laptop):

```
> Setup-spike1.exe --silent                         # exit 0, 2.1 s  (NSIS silent setup over an existing install: 17.2 s)
Installation Directory: "C:\Users\trung\AppData\Local\CaptureMannSpike"
There is 174.81 GB free space available at destination, this package requires 423.25 MB.
%LOCALAPPDATA%\CaptureMannSpike\{current\, packages\, Update.exe (5,319,680), CaptureMann.exe (466,944)}
installed bytes: 373,142,298   (NSIS: 246,305,815)
```

- **Admin:** not needed (`Root directory ... writable: true`, no UAC at any step; HKCU only). Per-user, like our NSIS installer.
- **Apps & features** (registry `HKCU\...\Uninstall\CaptureMannSpike`, key name = pack id): `DisplayName CaptureMann`, `DisplayVersion 0.3.2` (installed `0.3.2-spike1`: the pre-release suffix is cut), `Publisher CaptureMann contributors`, `InstallLocation %LOCALAPPDATA%\CaptureMannSpike`, `UninstallString "...\Update.exe" --uninstall`, `QuietUninstallString ... --uninstall --silent`, `DisplayIcon ...\current\velospike.exe`, `EstimatedSize 246305`. **Uninstall** (`Update.exe --uninstall --silent`): exit 0 in 4.1 s, runs the `before_uninstall` hook, removes the folder (with a scheduled `rmdir`), the Start Menu shortcut and the registry key; a fake `%APPDATA%\CaptureMannSpike\settings.json` survived and the NSIS install was untouched. (Our NSIS uninstaller also leaves settings alone.)
- **Start Menu:** `Programs\CaptureMann.lnk` (with `--shortcuts StartMenuRoot`), target `current\<mainExe>`; Update.exe re-points it on every update ("Updating existing shortcut").
- **First run:** a silent Setup does not start the app and `first_run` does not fire; the hooks seen were only `after_install` (run by Setup in the new exe, args `--veloapp-install 0.3.2-spike1`). The non-silent Setup (splash, progress) was **not run**: it needs a person at the desktop.

### 1.3 SmartScreen, Edge, signing (what was and was not observed)

- **Not observed:** the SmartScreen "Windows protected your PC" page and Edge's download warning. They appear when a browser-downloaded file carries the Mark of the Web and are shown in the interactive desktop, not over ssh; I did not drive the desktop (a screenshot is a screen capture and the rules say to ask) and did not change any setting. The laptop's `SmartScreenEnabled` policy value was empty (read only). Expectation from our own signing research (`docs/development/windows-signing-options.md`, section 7): the same "unrecognized app" page as for the NSIS `setup.exe`, with no publisher name, for both installers while unsigned.
- Facts that do bear on it: the Setup.exe and `Update.exe` are `NotSigned` (`Get-AuthenticodeSignature`); `Update.exe` is byte-identical in every Velopack app, so any reputation or Defender verdict it has is shared with all of them (good or bad, **not observed**); files an installer writes and files `ureq` downloads carry no Mark of the Web (reasoning, not tested), so after the first Setup the update itself should not raise SmartScreen again.
- **`--signParams` / `--signTemplate` in CI** (read from `vpk pack -H` run on the laptop, nothing was signed): on a Windows host `--signParams, -n` runs `signtool.exe` with the given parameters, `--azureTrustedSignFile` uses Azure Trusted Signing, `--signTemplate` runs any command with `{{file}}` substituted, `--signExclude` skips files by regex, `--signParallel` is the number of files per command (default 10). On the Mac cross-pack none of the signing flags except the template ones are listed. SignPath (our research's pick) is a submit-and-wait service with a human approval per request, so `--signParams` does not apply; `--signTemplate "powershell sign.ps1 {{file}}"` with a high `--signParallel` could submit one request for all files (one approval), but whether `{{file}}` expands to several paths and how Update.exe, the stub and Setup.exe (generated by vpk during the pack) get signed inside that flow is **not run** and should be a Phase 1 spike item once the SignPath account exists. The files to sign are the 5 CaptureMann exes, `Update.exe`, the stub and `Setup.exe`: 13 files in the real pack ("13 file(s) will not be signed").

### 1.4 `capturemann.exe` on PATH, stable paths, MCP config

- `current\` is the stable path: `%LOCALAPPDATA%\<packId>\current\capturemann.exe` and `...\cm-mcp.exe` are the same path before and after every update (the folder is swapped, `Backing up current dir ... Replacing current dir`); the root stub is `<title>.exe` and only launches `current\<mainExe>` (it cannot be used as a stable `capturemann.exe`, it starts the main exe with the arguments it got).
- **A hook can add and remove the PATH entry.** In the scratch app (`path_hook`, the shipped `path.ps1` run from the hook): after install `PATH` = `...\WindowsApps;...\CaptureMannSpike\current` (hook took 228 to 246 ms); after a skip-two-versions update (spike1 to spike3) the entry was still there and `capturemann.exe --version` resolved to `...\current\capturemann.exe` (prints `capturemann 0.3.1`, the real exe); `cm-mcp.exe` resolved the same way; `before_uninstall` removed the entry (268 ms). One of my runs showed `program not found` for `powershell.exe` in the uninstall hook: that was my script (it had replaced its own `$env:Path` with the user PATH before starting `Update.exe`, so the hook inherited no System32); with a normal environment it worked twice. Use the absolute `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe` (as the NSIS installer does) or write the registry directly, so a hook never depends on the inherited PATH.
- Claude Desktop's MCP config points at a path; the app's own snippet (`apps/desktop/src/launcher/mcp_config.rs`, `locate()`) is computed from `current_exe`, so a Velopack install shows `...\current\cm-mcp.exe`. That path survives updates, but **the running `cm-mcp.exe` is killed during an update** (it is in `current\`; seen with the stand-in) and the client has to start it again. A user who configured the old NSIS path `%LOCALAPPDATA%\Programs\CaptureMann\cm-mcp.exe` must change it after migrating (and so must a PATH entry). Claude Desktop itself is not on the laptop: its reconnect behaviour is **not run**.

## 2. Update cycle

Setup: the scratch app `velospike.exe` (GUI subsystem, `VelopackApp` first in `main`, auto-apply off, logs to a file) packed **with the real 0.3.1 payload** as `0.3.2-spike1`, `-spike2` (20,000 scattered 4-byte edits in `cm-desktop.exe`, as a recompile stand-in), `-spike3` (the same plus `cm-daemon.exe`), served by a loopback-only (`127.0.0.1:8099`) PowerShell `HttpListener` on the laptop (`scripts/vs-serve.ps1`), request log included. Commands: `velospike.exe update http://127.0.0.1:8099` (check, `download_updates`, `apply_updates_and_restart`). All processes of a test were started inside one ssh session: **processes started in an ssh session die with it**, so each script starts its own server and stand-ins.

### 2.1 Cycle, down time, locks (single step spike1 to spike2)

With stand-ins started from `current\` (copies of a tiny exe renamed `cm-daemon.exe`, `cm-overlay.exe`, `cm-mcp.exe`, sleeping) and no terminal in the folder (`scripts/vs-upd.ps1 -WithStand 2`):

```
+175ms  old app exited
+1158ms HOOK before_update 0.3.2-spike1  (--veloapp-obsolete, run by Update.exe in the OLD exe)
+1327ms stand-ins all gone                (Killing process: ...\current\cm-daemon.exe / cm-overlay.exe / cm-mcp.exe)
+1412ms HOOK after_update 0.3.2-spike2    (--veloapp-updated, NEW exe)
+1562ms new instance running, HOOK restarted, UpdateManager says version 0.3.2-spike2
```

- **Down time** (old process exit to new process running): about **1.4 s**. No prompt, no dialog, no kept-open window was seen (an ssh session shows none; the interactive UI of a non-silent apply was **not observed**).
- **What is killed:** every running process whose image is inside the install folder (`Inspected 211 running processes ... 4 matched directory`, three `Killing process` lines, `Skipping killing self` for Update.exe). Not killed: processes outside the folder (a client that started `cm-mcp.exe` from there is killed; the client process itself is not).
- **The hole:** with an extra `cmd.exe` whose working directory was `current\` the same update **failed**: `Backing up current dir` retried 10 times at 1 s ("The process cannot access the file because it is being used by another process"), then `Apply error: ... Unable to start the update, because one or more running processes prevented it`. Update.exe then **restarted the old version** (about 11 s after the app exited), the new package stayed in `packages\`, the app was still 0.3.2-spike1. A terminal in the install folder is plausible for a CLI product.
- Hooks: `before_update` runs in the old exe, `after_update` in the new one, `restarted` in the relaunched app (and receives no arguments: `apply_updates_and_restart` passes none; `..._with_args` exists).

### 2.2 Full versus delta (real payload)

| Case | Full | Delta | Time on the T490 |
| --- | ---: | ---: | --- |
| real 0.2.9 to 0.3.1 (both public zips) | 112,283,547 | **28,661,865** (25 %), 17 of 36 files patched | pack 8 s each |
| scratch: 20,000 scattered 4-byte edits in `cm-desktop.exe` (94.7 MB) | 115,247,135 | 221,672 | client: 7.2 s to rebuild the full package from the delta |
| scratch: the same plus `cm-daemon.exe` (spike2 to spike3) | 115,343,493 | 557,735 | |
| skip: spike1 straight to spike3 | | spike2 delta + spike3 delta (779 KB) chained | 7.8 s |

- Delta files only carry changed files: the 33 MB of speaker models (`Resources\Models\speaker`), FFmpeg and `llama-server.exe` were "unchanged" in every pack and never travelled; the real 0.2.9 to 0.3.1 delta is large because the real Rust binaries change widely, so plan on **25 to 30 MB** per release, not the synthetic few hundred KB. The first update after a Setup install can already be a delta (the full package stays in `packages\`; the Windows install keeps it as the base, unlike a DMG-installed Mac app).
- **A full download is cheap locally (0.9 s for 115 MB over loopback), the delta patch is not (7 s)**: on a slow disk or a busy laptop the patch time is user-visible and runs in the app process, so it belongs on a worker thread with a progress callback (`download_updates(..., Some(Sender<i16>))`, steps 0 to 100).
- **Skipped versions:** spike1 to spike3 worked with the two deltas chained (request log: both delta files, no full). With a feed served from `releases/latest/download`, deltas of the intermediate versions live in older releases and would 404; the client falls back to the full package when a delta fails (shown with a corrupt delta, 2.3), so that costs bandwidth, not correctness.
- Observed side effect: after a delta the full package rebuilt on disk (`packages\...-spike2-full.nupkg`, 118,205,960 bytes) differs from the published one (115,247,135 bytes). Only the downloaded delta is hash-checked.

### 2.3 Faults (feed `releases.win.json` variants; helper `scripts/vs-faults.ps1`, `vs-faultrun.ps1`)

| Fault | Result (`download_updates` / apply) | State afterwards |
| --- | --- | --- |
| feed host refuses the connection | `check: ERR Network(Http(Io(ConnectionRefused)))` after 2.0 to 3.2 s | version unchanged |
| unresolvable host | `ERR ... No such host is known.`, 0.1 s | unchanged |
| server accepts and never answers | **no result after 45 s** (killed by me); the crate sets no timeout unless `HttpOptions.TimeoutMilliseconds` is given | hung call |
| feed is not JSON (also: JSON with a UTF-8 BOM) | `ERR Json(Error("expected value", line: 1, column: 1))`: PowerShell 5.1 `Set-Content -Encoding UTF8` writes a BOM and breaks the feed; vpk's own JSON has none | unchanged |
| feed lists a package that is 404 | `Network(Http(StatusCode(404)))` | unchanged |
| truncated full package (60 MB of 115 MB, feed unchanged) | `SizeInvalid(..., 115247135, 62914560)` | `.partial` left in `packages\`, not applied |
| one byte flipped, same size | `ChecksumInvalid(expected 36875B..., got 46D802...)` (SHA-256) | `.partial` left, not applied |
| feed SHA-256 wrong, package intact | `ChecksumInvalid` | same |
| **delta** flipped (full intact) | falls back to the full package, downloads it, applies it, restart OK | updated to spike2 |
| new version's `--veloapp-updated` hook exits with code 5 | `Hook exited with non-zero exit code: 5` is a **warning**; the update continues and the new version is kept and restarted | updated (no rollback) |
| new version's `velospike.exe` is not a valid exe (packed with `--skipVeloAppCheck`... and also without it) | package applied, hook cannot start, no restart | **install is dead** (`current\velospike.exe` is the 26-byte text file); no revert |

So: integrity failures are caught before apply (size, then SHA-256 if the feed has one, else SHA-1: the Mac report tested this order), and the client has **no signature check and no rollback**. Uninstalling the dead install (`Update.exe --uninstall --silent`, via the registry string) blocked at "Running --veloapp-uninstall hook" until I killed it (more than 5 minutes); this was in a non-interactive session where a "cannot run" dialog could not be seen, so whether a person sees a message box is **not observed**, but the user would be stuck either way, and re-running `Setup.exe` over it is presumably the repair path (section 3.1 shows Setup installs over a healthy install and even downgrades silently; over a dead one it was not run).

## 3. Coexistence and migration

### 3.1 NSIS 0.3.1 next to a Velopack install

The laptop already had the 0.3.1 NSIS install (per user, no PATH); the scratch Setup was installed next to it:

```
Apps & features (registry): key=CaptureMann        name=CaptureMann ver=0.3.1 loc=...\Programs\CaptureMann
                            key=CaptureMannSpike   name=CaptureMann ver=0.3.2 loc=...\CaptureMannSpike
Start Menu:  CaptureMann.lnk            CaptureMann\CaptureMann.lnk
winget list: CaptureMann  ARP\User\X64\CaptureMann       0.3.1
             CaptureMann  ARP\User\X64\CaptureMannSpike  0.3.2
```

- **Two copies, same name, two entries, two shortcuts.** Nothing in Velopack's Setup detects the NSIS install (docs and `Setup.exe --help`: only `--silent`, `--verbose`, `--log`, `--installto`, EXE_ARGS; no pre-install hook). Both run; they share `%APPDATA%\CaptureMann` and the library, so two daemons could fight over the socket.
- Setup over an installed Velopack app (silent): `Setup(spike3)` over `spike1` gave `spike3` in 2.1 s (the old package removed); `Setup(spike1)` over `spike3` gave `spike1`: **the installer silently downgrades**, relevant when winget or a user runs an older download.
- **Migration by a hook (tested from outside the hook):** NSIS silent install with `/ADDPATH` (17.2 s) then `"%LOCALAPPDATA%\Programs\CaptureMann\uninstall.exe" /S _?=%LOCALAPPDATA%\Programs\CaptureMann` (the `_?=` makes it wait; 6.2 s, exit 0), then the folder is removed by hand because `_?=` leaves it (as `installer.nsi` documents): the NSIS directory, its Apps key and its PATH entry were gone, the Velopack install was untouched, `%APPDATA%\CaptureMann` (settings, library index) survived. A hook doing this must read the NSIS state first (`HKCU\Software\CaptureMann\PathAdded`, `OpenWith`, `InstallDir`) and re-create what the user had (PATH for the new `current`, the Open-with entries that point at the old exe path, the Start Menu), and must not run while the NSIS copy is running (the NSIS uninstaller does not stop it). Not run inside a real hook (the hook budget is about 15 s, the uninstall took 6 s). Whether it is good manners to remove an install without asking is a product decision.
- **Do not name the pack id `CaptureMann`:** the Apps key is the pack id, so a same-named key would overwrite the NSIS uninstall entry and orphan its files. (By construction from the observed key name; not run.) The install folder is `%LOCALAPPDATA%\<packId>`, also an AppUserModelID source (`velopack.<packId>`); use a stable id once.

### 3.2 winget

Current manifest (`packaging/windows/winget/CaptureMann.CaptureMann.installer.yaml.template`): `InstallerType: nullsoft`, `Scope: user`, `Silent: /S`, `UpgradeBehavior: install`. **Not run** (a local manifest install needs the `LocalManifestFiles` winget setting, a machine-wide change I did not make; and PRs 472327 and 477195 are still open). What the evidence supports:
- The Velopack Setup takes `--silent` and `--installto <dir>`, so a Velopack manifest would use `InstallerType: exe`, `InstallerSwitches: Silent: --silent`, `SilentWithProgress: --silent`, and `InstallLocation: --installto "<INSTALLPATH>"` if `--location` is to be supported, plus `AppsAndFeaturesEntries` with `ProductCode` = the pack id so winget can match the Apps entry. `winget list` already shows the Velopack entry as `ARP\User\X64\<packId>` with the version 0.3.2.
- **No fight with the in-app update:** after each update Update.exe writes the Apps entry again ("Writing uninstall registry key"), so `DisplayVersion` follows the installed version; `winget upgrade` compares that to the manifest and offers an upgrade only when winget has a newer one, which runs the newer `Setup.exe` over the install (works, 3.1; the downgrade danger above applies only when the manifest is older than what is installed and winget decides to "install" it).
- A winget-installed Velopack copy is indistinguishable from a directly installed one (no marker), so "managed by winget" cannot be shown for it; the "managed by" states are only possible for installs that are not Velopack (NSIS, zip) and for the Mac brew case.

### 3.3 Which installs may self-update, and how the app tells

`UpdateManager::new(source, None, None)` is the check (Mac report: the Result is the signal). Run with a scratch exe in each layout:

```
Velopack Setup install (current\vcli.exe)   installed=true  portable=false  id=CaptureMannSpike  version=0.3.2-spike1
Velopack Portable.zip (portable\current\)   installed=true  portable=true   (has .portable at the root)
flat folder (the published 0.3.1 zip)       installed=false err=NotInstalled("Could not auto-locate app manifest")
inside the NSIS install folder              installed=false err=NotInstalled(...)
```

So: Ok and `get_is_portable() == false` is the Setup install (may self-update); Ok and portable is the new-style zip (updates in place inside its folder, an option if we ship the Velopack Portable.zip instead of the flat zip; **not run** to completion); Err is everything else: the NSIS install, the flat zip, a dev build. The detection runs without a feed (`NoneSource`).

## 4. Process model

- `VelopackApp::build().set_auto_apply_on_startup(false).run()` first in `main`: for a **GUI exe with `#![windows_subsystem = "windows"]`** this worked (`velospike.exe`, hooks ran, stdout is irrelevant). `run()` reads `std::env::args()`, calls `exit(0)` after a fast hook, sets the AppUserModelID of the process (`velopack.<packId>`; `set_app_user_model_id` overrides it: our overlay and toast code may care), cleans old packages, and with auto-apply on **applies a newer downloaded package and exits** if the process was not itself restarted by Velopack. Because of that every exe in the folder that might call it (`capturemann.exe`, `cm-mcp.exe`, `cm-daemon.exe`, `cm-overlay.exe`) must either not call `run()` or call it with auto-apply off; only `cm-desktop.exe` needs the hooks.
- **Console CLI start-up cost** (60 runs each, PowerShell call operator, includes process creation, ms per run on the T490): `vplain` (no crate) 5.9; `vcli` with the crate linked but `run()` skipped 10.7; `vcli` with `run()` installed 14.1, not installed 11.6; the real `capturemann.exe --version` (0.3.1, no Velopack) 7.4. The timestamps inside the process say `run()` itself takes **2 to 3 ms** installed, 1 to 2 ms when not installed (it builds an `UpdateManager` with `NoneSource` and lists `packages\`); the rest is the larger exe (2.97 MB scratch exe against 137 KB: the crate adds about **2.8 MB** at `opt-level = "s"`). Do not call `run()` in `capturemann.exe`; the check and apply there are separate calls.
- **Spawned processes:** `VelopackApp` consumes the env vars `VELOPACK_FIRSTRUN`/`VELOPACK_RESTART` (removed after reading), so children do not inherit "first run". Child processes of a killed app are not stopped unless their image is in the folder (the stand-ins were; a `cmd.exe` from elsewhere is not).
- **Does Update.exe start on launch? No.** Starting the stub `CaptureMann.exe sleep 12` (the process list 3 s later): one process, `velospike.exe` in `current\`, the stub already gone (`HasExited=True`), no `Update.exe`; after the app exited nothing from the install was left running. `Update.exe` runs only for apply, uninstall and Setup.
- Hook budget: Update.exe waits 15 s for each hook exe (`Waiting 15000ms for process handle to exit`).

## 5. Feed and hosting

- **URLs the Windows client would request** (`HttpSource`, channel `win` by default; `vpk pack --channel` changes it, and an Intel or ARM Windows build would need its own channel name as on the Mac): `GET <base>/releases.win.json?localVersion=<v>&id=<packId>&stagingId=<uuid>` with `user-agent: ureq/3.5.1`, then `GET <base>/<packId>-<ver>-delta.nupkg` or `-full.nupkg` (request log of the local server; the same as the Mac report). Everything under one base URL.
- **GitHub, read-only check on Nov 11, 2026:** `GET https://api.github.com/repos/craft-mann/capturemann/releases` lists `v0.3.1 prerelease=false`, `v0.3.0 true`, `apt true`, `v0.2.9 false`, `v0.2.8 true`, `v0.2.7 false`; `curl -I .../releases/latest/download/SHA256SUMS.txt` answers `302 -> .../releases/download/v0.3.1/SHA256SUMS.txt`. GitHub's documented rule (docs.github.com, "Get the latest release", read by the Mac agent) is that "latest" is the most recent non-prerelease, non-draft release; I did not create a pre-release to test it. Consequences: 0.3.1 is stable, so `latest/download` would already work for a stable 0.3.2; but a pre-release (as 0.3.0 was) is skipped and so would never be offered. The dedicated `updates` release proposed in the Mac report keeps one place, one cumulative feed and the old deltas, and avoids the 404-then-full fallback for skipped versions described in 2.2. Feed JSON must be UTF-8 without BOM.
- `HttpSource` follows the 302 to `release-assets.githubusercontent.com` (`ureq` follows redirects; only the redirect was seen with `curl`, not through `ureq`: **not run**).

## What the maintainer must do himself

1. **Look at the real UX once** (needs a person at the desktop on any Windows 11 machine): download the packed `Setup.exe` in Edge from a scratch location and record what Edge and SmartScreen show; run it without `--silent` and note the splash, the progress and what a locked update shows (a message box or nothing); do the same for the NSIS `setup.exe` for comparison.
2. **Decide** the `deny.toml` questions in the Mac report (CDLA-Permissive-2.0 for `webpki-roots`, the `derivative` ignore, `zip` >= 9) and whether to keep the stock `HttpSource` (identifier and no timeout) or a small custom source.
3. **Decide the integrity policy for Windows** (plan decision 4): (a) wait for SignPath, (b) minisign verification inside a custom `UpdateSource`, or (c) notify-only until signed. This spike shows (b) is feasible only in that form; it was not built.
4. **Decide the migration behaviour** for 0.3.1 NSIS users: manual install of the Velopack Setup with a release-text sentence, and whether the after-install hook may remove the NSIS copy (tested: it can, 6 s) or must only warn.
5. **Pick the pack id and the first-install format** (not `CaptureMann`; keep NSIS only if winget stays on it, but then NSIS users are never self-updating) and approve the winget manifest change (`InstallerType: exe`, `--silent`); winget behaviour was not run.
6. **SignPath and `vpk`:** if a SignPath application goes ahead, a Phase 1 spike must show that `vpk pack` on a Windows runner can sign all 13 files through `--signTemplate` and one approval (nothing was signed here).
7. Intel/ARM Windows, Windows 10 and mixed DPI were not run (same gaps as 0.3.1).

## Not verified

The non-silent Setup UI and any dialog Update.exe or Setup shows; Edge and SmartScreen pages; Defender's verdict on `Update.exe`/Setup; installing the MSI (admin); winget install and upgrade of a Velopack Setup; a real `cm-desktop.exe`, `cm-daemon.exe` or an MCP client holding files (stand-ins only: a renamed tiny exe in `current\`); a recording running during an update; Claude Desktop's reaction to a killed `cm-mcp.exe`; behaviour when the user's account is non-admin or the disk is full; HTTPS and the GitHub 302 through `ureq` on Windows; a custom `UpdateSource` with signature checks; `--signTemplate` with SignPath; the Velopack Portable.zip updating itself end to end; the 1.3.x pre-release client; Windows 10; long-term growth of `packages\`; a packet capture (I logged requests on my own server).

## Cleanup

Laptop: the scratch install and its registry key, shortcut and `%LOCALAPPDATA%\velopack` logs removed; `C:\lab\velospike` (the .NET SDK, vpk, feeds, packages, 4 GB) and `C:\lab\vs-setup.ps1` deleted; the local feed server stopped (loopback only); the user PATH is back to `...\WindowsApps` (a `dotnet tool install` had added `C:\lab\velospike\dotnethome\.dotnet\tools` once; removed with the product's own `path.ps1`, nothing system-wide was changed); the laptop's NSIS 0.3.1 install is present as it was found (reinstalled silently from the verified public setup during the coexistence test). Mac: `cargo clean` in `spikes/velopack-win`; scratch files in the session scratchpad only. `cargo-xwin` 0.23.1 was installed with `cargo install` (user scope) and `rustup component add llvm-tools` (cross build needed `AR_x86_64_pc_windows_msvc` pointing at `llvm-ar`).
