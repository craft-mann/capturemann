<#
.SYNOPSIS
  Builds the Windows "early build": a portable zip and a per-user NSIS installer.

.DESCRIPTION
  Output (packaging\windows\out, git-ignored, or $env:CM_WINDOWS_OUT):
    capturemann-<version>-windows-x64.zip
    capturemann-<version>-windows-x64-setup.exe      (unless -NoInstaller)
    <each file>.sha256
  The tree holds cm-desktop.exe, cm-overlay.exe, capturemann.exe, cm-mcp.exe, cm-daemon.exe, app.ico, the licence
  files and, when given, an LGPL FFmpeg (ffmpeg.exe and ffprobe.exe NEXT TO the programs: cm-media
  looks beside the running executable first).

  Run on Windows (x86_64) from any directory:   packaging\windows\build-windows.ps1
  Needs: Rust (MSVC toolchain), Visual Studio Build Tools (link.exe, dumpbin.exe), cmake, python3 and
  bash (Git for Windows) for THIRD_PARTY_LICENSES, NSIS (makensis.exe) unless -NoInstaller.
  The windows-latest runner has all of these except NSIS (choco install nsis).

  Environment:
    CM_SKIP_BUILD=1            reuse the binaries in <target>\release
    CM_FFMPEG_BUNDLE           folder with ffmpeg.exe, ffprobe.exe (+ SOURCE-OFFER.txt, LICENSES\) from
                               packaging/ffmpeg/build-windows.sh. Refused when its build line says GPL.
    CM_REQUIRE_FFMPEG=1        fail unless an LGPL FFmpeg with its licence files is bundled
    CM_WINDOWS_OUT             output folder
    CM_WINDOWS_CRT_STATIC=0    link the Microsoft C runtime dynamically (default: static, so the
                               user needs no "Visual C++ Redistributable")
    CM_WINDOWS_CONSOLE=1       keep the console subsystem on cm-desktop and cm-overlay (debugging)
    CM_SKIP_LICENSES=1         use the checked-in THIRD_PARTY_LICENSES.md (macOS crate set)
    CM_SKIP_LIBCHECK=1         skip the dumpbin dependency policy check (testing only)
    CM_WINDOWS_ALLOW_MISSING   comma-separated program names that may be absent (testing only)
    CM_CARGO_JOBS              cargo -j value

  The speaker engine (sherpa-onnx) has only macOS libraries, so this builds with
  --no-default-features: "Find speakers" answers that the build has no speaker engine.

  NOT RUN ON WINDOWS YET: see packaging/windows/README.md ("Status").
#>
[CmdletBinding()]
param(
  [switch]$NoInstaller,
  [switch]$NoZip
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail([string]$msg) { Write-Error "build-windows.ps1: $msg"; exit 1 }
function Invoke-Native {
  # Runs a native command and stops when it fails ($ErrorActionPreference does not cover exit codes).
  param([string]$File, [string[]]$Arguments)
  & $File @Arguments
  if ($LASTEXITCODE -ne 0) { Fail "$File failed with exit code $LASTEXITCODE" }
}

if ([Environment]::OSVersion.Platform -ne 'Win32NT') { Fail 'this script only runs on Windows' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail "unsupported CPU $($env:PROCESSOR_ARCHITECTURE) (x86_64 only for the early build)" }
foreach ($t in 'cargo', 'python') {
  if (-not (Get-Command $t -ErrorAction SilentlyContinue)) {
    if ($t -eq 'python' -and (Get-Command python3 -ErrorAction SilentlyContinue)) { continue }
    Fail "required tool not found: $t"
  }
}
$python = if (Get-Command python3 -ErrorAction SilentlyContinue) { 'python3' } else { 'python' }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out = if ($env:CM_WINDOWS_OUT) { $env:CM_WINDOWS_OUT } else { Join-Path $root 'packaging\windows\out' }
$triple = 'x86_64-pc-windows-msvc'

# ---- version and target directory ----
Push-Location $root
try {
  $metaJson = & cargo metadata --no-deps --format-version 1 --locked
  if ($LASTEXITCODE -ne 0) { Fail 'cargo metadata failed' }
} finally { Pop-Location }
$meta = $metaJson | ConvertFrom-Json
$version = ($meta.packages | ForEach-Object { [version]($_.version -replace '[-+].*$', '') } | Sort-Object | Select-Object -Last 1).ToString()
$fullVersion = ($meta.packages | Where-Object { ($_.version -replace '[-+].*$', '') -eq $version } | Select-Object -First 1).version
if (-not $fullVersion) { Fail 'cannot read the workspace version' }
$targetDir = $meta.target_directory
Write-Host "CaptureMann $fullVersion"

# ---- build ----
$jobs = @()
if ($env:CM_CARGO_JOBS) { $jobs = @('-j', $env:CM_CARGO_JOBS) }
$crtStatic = $env:CM_WINDOWS_CRT_STATIC -ne '0'
if ($env:CM_SKIP_BUILD -ne '1') {
  if ($crtStatic) {
    # /MT everywhere (Rust crates and, through cc-rs and cmake, the C and C++ ones): no vcruntime140.dll.
    $env:RUSTFLAGS = (($env:RUSTFLAGS, '-C target-feature=+crt-static') -join ' ').Trim()
  }
  Push-Location $root
  try {
    Write-Host '==> cargo build --release --no-default-features (capturemann, cm-mcp)'
    Invoke-Native cargo (@('build', '--release', '--locked', '--no-default-features') + $jobs + @('-p', 'cm-cli', '-p', 'cm-mcp'))
    # The window programs and the daemon get the GUI subsystem, so Explorer or the Start Menu opens no console
    # window with them. The crates have no `windows_subsystem` attribute (they are shared with the
    # other systems), so the linker is told here. `cargo rustc` passes the flags to the last crate only.
    $sub = @()
    if ($env:CM_WINDOWS_CONSOLE -ne '1') { $sub = @('-C', 'link-arg=/SUBSYSTEM:WINDOWS', '-C', 'link-arg=/ENTRY:mainCRTStartup') }
    # cm-daemon is started by the CLI and by cm-mcp when the app is not running; as a GUI-subsystem program it
    # opens no console window of its own.
    foreach ($p in @(@('cm-desktop', 'cm-desktop'), @('cm-overlay', 'cm-overlay'), @('cm-daemon', 'cm-daemon'))) {
      Write-Host "==> cargo rustc --release -p $($p[0])"
      Invoke-Native cargo (@('rustc', '--release', '--locked', '--no-default-features') + $jobs + @('-p', $p[0], '--bin', $p[1], '--') + $sub)
    }
  } finally { Pop-Location }
}
$release = Join-Path $targetDir 'release'
$allBins = 'cm-desktop', 'cm-overlay', 'capturemann', 'cm-mcp', 'cm-daemon'
$allowMissing = @(); if ($env:CM_WINDOWS_ALLOW_MISSING) { $allowMissing = $env:CM_WINDOWS_ALLOW_MISSING -split ',' }
$bins = @()
foreach ($b in $allBins) {
  if (Test-Path (Join-Path $release "$b.exe")) { $bins += $b }
  elseif ($allowMissing -contains $b) { Write-Warning "$b.exe is missing; allowed by CM_WINDOWS_ALLOW_MISSING (a test package, not for release)" }
  else { Fail "missing $release\$b.exe (build it, or unset CM_SKIP_BUILD)" }
}

# ---- dumpbin: the linked DLLs are a policy check (nothing GPL, no FFmpeg libraries) and a record ----
function Find-Dumpbin {
  $c = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
  if ($c) { return $c.Source }
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (Test-Path $vswhere) {
    $f = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
    if ($f) { return $f }
  }
  return $null
}
function Get-Dependents([string]$exe, [string]$dumpbin) {
  $o = & $dumpbin /nologo /dependents $exe
  if ($LASTEXITCODE -ne 0) { Fail "dumpbin failed on $exe" }
  # The DLL names are the indented lines between the two headings.
  $inList = $false
  foreach ($line in $o) {
    if ($line -match 'Image has the following dependencies') { $inList = $true; continue }
    if ($line -match '^\s*Summary') { break }
    if ($inList -and $line -match '^\s+(\S+\.dll)\s*$') { $Matches[1].ToLowerInvariant() }
  }
}
$needed = @()
if ($env:CM_SKIP_LIBCHECK -ne '1') {
  Write-Host '==> checking the linked libraries'
  $dumpbin = Find-Dumpbin
  if (-not $dumpbin) { Fail 'dumpbin.exe not found (install the Visual Studio Build Tools, or set CM_SKIP_LIBCHECK=1 for a test package)' }
  $needed = $bins | ForEach-Object { Get-Dependents (Join-Path $release "$_.exe") $dumpbin } | Sort-Object -Unique
  # Nothing GPL or nonfree may be linked in. cargo deny covers the Rust crates; this covers the DLLs the
  # loader will bring in. A DLL that is not a Windows component and is not on this list is also refused.
  $denied = $needed | Where-Object { $_ -match 'x264|x265|fdk|libgmp|readline|libav|swscale|swresample|postproc|ffmpeg|libwinpthread|libgcc|libstdc\+\+' }
  if ($denied) { Fail "a program links a DLL that is not allowed in a shipped build: $($denied -join ', ')" }
  $vcrt = $needed | Where-Object { $_ -match '^(vcruntime|msvcp|concrt|vcomp)\d*' }
  if ($vcrt -and $crtStatic) { Fail "CRT_STATIC was requested but the programs still need: $($vcrt -join ', ')" }
  if ($vcrt) { Write-Warning "the programs need the Visual C++ runtime ($($vcrt -join ', ')): users must install the Redistributable" }
  Write-Host '    DLLs:'; $needed | ForEach-Object { Write-Host "      $_" }
} else { Write-Warning 'CM_SKIP_LIBCHECK=1: the dependency policy check was skipped (a test package, not for release)' }

# ---- the FFmpeg ----
$ffdir = $null; $ffLic = $false
if ($env:CM_FFMPEG_BUNDLE) {
  $ffdir = $env:CM_FFMPEG_BUNDLE
  $ffexe = Join-Path $ffdir 'ffmpeg.exe'
  if (-not (Test-Path $ffexe)) { Fail "no ffmpeg.exe in CM_FFMPEG_BUNDLE=$ffdir" }
  $conf = (& $ffexe -hide_banner -buildconf 2>&1) -join "`n"
  if ($conf -match '--enable-(gpl|nonfree|version3|libx264|libx265|libfdk-aac)') {
    Fail "$ffexe was built with --enable-gpl/nonfree/libx264/libx265/libfdk-aac: not allowed in a shipped build (NFR-12)"
  }
  $ffLic = (Test-Path (Join-Path $ffdir 'SOURCE-OFFER.txt')) -and (Test-Path (Join-Path $ffdir 'LICENSES'))
  if (-not $ffLic) {
    if ($env:CM_REQUIRE_FFMPEG -eq '1') { Fail "$ffdir has no SOURCE-OFFER.txt and LICENSES\" }
    Write-Warning 'no SOURCE-OFFER.txt / LICENSES next to ffmpeg.exe; this package must not be distributed'
  }
} elseif ($env:CM_REQUIRE_FFMPEG -eq '1') {
  Fail 'CM_REQUIRE_FFMPEG=1 but CM_FFMPEG_BUNDLE is not set (build one with packaging/ffmpeg/build-windows.sh)'
}

# ---- licences ----
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("cm-win-pack-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $licenses = Join-Path $root 'THIRD_PARTY_LICENSES.md'
  if ($env:CM_SKIP_LICENSES -ne '1') {
    Write-Host '==> generating THIRD_PARTY_LICENSES.md for the Windows crate set'
    $bash = Get-Command bash.exe -ErrorAction SilentlyContinue
    $gen = Join-Path $tmp 'THIRD_PARTY_LICENSES.md'
    $ok = $false
    if ($bash) {
      $env:CM_LICENSE_TRIPLES = $triple
      $genScript = (Join-Path $root 'packaging/macos/gen-third-party.sh') -replace '\\', '/'
      $genOut = $gen -replace '\\', '/'
      & $bash.Source $genScript $genOut *> (Join-Path $tmp 'gen.log')
      $ok = ($LASTEXITCODE -eq 0) -and (Test-Path $gen)
    }
    if ($ok) { $licenses = $gen }
    else {
      if (Test-Path (Join-Path $tmp 'gen.log')) { Get-Content (Join-Path $tmp 'gen.log') -Tail 5 | Write-Host }
      Write-Warning 'could not generate the Windows list (needs bash, python3 and the crates fetched); shipping the checked-in macOS one'
    }
  }

  # ---- assemble the tree (the zip's content, and what the installer installs) ----
  $name = "capturemann-$version-windows-x64"
  $stage = Join-Path $tmp $name
  New-Item -ItemType Directory -Path $stage | Out-Null
  foreach ($b in $bins) { Copy-Item (Join-Path $release "$b.exe") $stage }
  if ($ffdir) {
    Copy-Item (Join-Path $ffdir 'ffmpeg.exe') $stage
    if (Test-Path (Join-Path $ffdir 'ffprobe.exe')) { Copy-Item (Join-Path $ffdir 'ffprobe.exe') $stage }
    if ($ffLic) {
      $ffl = Join-Path $stage 'FFmpeg'; New-Item -ItemType Directory -Path $ffl | Out-Null
      Copy-Item (Join-Path $ffdir 'SOURCE-OFFER.txt') $ffl
      if (Test-Path (Join-Path $ffdir 'BUILD-INFO.txt')) { Copy-Item (Join-Path $ffdir 'BUILD-INFO.txt') $ffl }
      Copy-Item (Join-Path $ffdir 'LICENSES') (Join-Path $ffl 'LICENSES') -Recurse
    }
  }
  $doc = Join-Path $stage 'doc'; New-Item -ItemType Directory -Path $doc | Out-Null
  Copy-Item (Join-Path $root 'LICENSE'), (Join-Path $root 'NOTICE') $doc
  Copy-Item $licenses (Join-Path $doc 'THIRD_PARTY_LICENSES.md')
  if (Test-Path (Join-Path $root 'CHANGELOG.md')) { Copy-Item (Join-Path $root 'CHANGELOG.md') $doc }
  $libs = @("Dynamic libraries (DLL imports) of the programs in this build, x64:") + ($needed | ForEach-Object { "  $_" })
  if (-not $needed) { $libs += '  (not recorded: the dependency check was skipped)' }
  Set-Content -Path (Join-Path $doc 'LIBRARIES.txt') -Value $libs -Encoding UTF8
  # Icon: the kit's Windows icon, copied as is (design/CLAUDE.md section 7).
  $ico = Join-Path $root 'design\brand\app-icon\app.ico'
  if (-not (Test-Path $ico)) { Fail "icon not found: $ico" }
  Copy-Item $ico (Join-Path $stage 'app.ico')
  # The path helper the installer's optional "Add to PATH" uses; the zip ships it too.
  Copy-Item (Join-Path $PSScriptRoot 'path.ps1') $stage
  $readme = @"
CaptureMann $fullVersion for Windows (x64), early build.

Portable:  unzip anywhere and run cm-desktop.exe. Nothing is written outside this folder and your
           settings and library (%APPDATA%\CaptureMann and your Videos folder).
Command line:  capturemann.exe --help      MCP server for agents:  cm-mcp.exe
               (both start cm-daemon.exe, next to them, when the app is not running)
Add this folder to your PATH:  powershell -ExecutionPolicy Bypass -File path.ps1 -Action add -Dir "<this folder>"

Not available yet on Windows: screen, camera and audio RECORDING (screenshots, annotation, the library, local AI,
the CLI and MCP work). This build has no speaker engine ("Find speakers" is off).
Windows may show "Windows protected your PC" (SmartScreen): the early build is not code-signed yet.
Choose "More info" and "Run anyway", or compare the SHA-256 file first.
Licences: doc\ . ffmpeg.exe (if present) is FFmpeg under the LGPL, see FFmpeg\ .
Source: https://github.com/craft-mann/capturemann
"@
  Set-Content -Path (Join-Path $stage 'README.txt') -Value $readme -Encoding UTF8

  New-Item -ItemType Directory -Force -Path $out | Out-Null
  $made = @()

  # ---- zip ----
  if (-not $NoZip) {
    $zip = Join-Path $out "$name.zip"
    Remove-Item -Force -ErrorAction SilentlyContinue $zip, "$zip.sha256"
    Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
    $made += $zip
  }

  # ---- installer ----
  if (-not $NoInstaller) {
    $mk = Get-Command makensis.exe -ErrorAction SilentlyContinue
    $makensis = if ($mk) { $mk.Source } else {
      @("${env:ProgramFiles(x86)}\NSIS\makensis.exe", "$env:ProgramFiles\NSIS\makensis.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $makensis) { Fail 'makensis.exe not found (choco install nsis, or pass -NoInstaller)' }
    $setup = Join-Path $out "$name-setup.exe"
    Remove-Item -Force -ErrorAction SilentlyContinue $setup, "$setup.sha256"
    $nsVersion = ($fullVersion -replace '[-+].*$', '') + '.0'   # VIProductVersion wants four numbers
    $nsiArgs = @('/V2', "/DVERSION=$fullVersion", "/DVI_VERSION=$nsVersion", "/DSTAGE=$stage", "/DOUTFILE=$setup", (Join-Path $PSScriptRoot 'installer.nsi'))
    Write-Host '==> makensis'
    Invoke-Native $makensis $nsiArgs
    $made += $setup
  }

  foreach ($f in $made) {
    $h = (Get-FileHash -Algorithm SHA256 $f).Hash.ToLowerInvariant()
    # "<hash> *<file>" is the format `sha256sum -c` reads on any system.
    Set-Content -Path "$f.sha256" -Value ("{0} *{1}" -f $h, (Split-Path $f -Leaf)) -Encoding ASCII -NoNewline:$false
    Get-Item $f | ForEach-Object { Write-Host ("==> {0}  ({1:N1} MB)" -f $_.FullName, ($_.Length / 1MB)) }
    Get-Content "$f.sha256"
  }
} finally {
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
}
