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
                               packaging/ffmpeg/build-windows.sh (in an MSYS2 MINGW64 shell). Refused when its
                               build line says GPL. When unset, packaging/ffmpeg/out-windows-x64 is used if it exists.
    CM_REQUIRE_FFMPEG=1        fail unless an LGPL FFmpeg with its licence files is bundled
    CM_LLAMA_BUNDLE            folder with llama-server.exe (+ BUILD-INFO.txt, LICENSES\) from
                               packaging/llama/build-windows.sh (in an MSYS2 MINGW64 shell). When unset,
                               packaging/llama/out-windows-x64 is used if it exists; otherwise the build has no
                               built-in text engine (the app says so; nothing else changes).
    CM_REQUIRE_LLAMA=1         fail unless llama-server.exe with its build record and licence files is bundled
    CM_REQUIRE_SHERPA=1        fail unless the speaker engine (sherpa-onnx libraries) is built in
    CM_SPEAKER_MODELS_BUNDLE   folder with the two speaker-label models (+ BUILD-INFO.txt, LICENSES\) from
                               packaging/sherpa/fetch-models.ps1 (default: packaging\sherpa\out\models). They go to
                               Resources\Models\speaker\ next to the programs (cm_ai::model::bundled_speaker_dir).
    CM_REQUIRE_SPEAKER_MODELS=1  fail unless both models and their licence files are bundled
    CM_NO_SPEAKER_MODELS=1     do not bundle the speaker models (testing only)
    CM_WINDOWS_OUT             output folder
    CM_WINDOWS_CRT_STATIC=0    link the Microsoft C runtime dynamically (default: static, so the
                               user needs no "Visual C++ Redistributable")
    CM_WINDOWS_CONSOLE=1       keep the console subsystem on cm-desktop and cm-overlay (debugging)
    CM_SKIP_LICENSES=1         use the checked-in THIRD_PARTY_LICENSES.md (macOS crate set)
    CM_SKIP_LIBCHECK=1         skip the dumpbin dependency policy check (testing only)
    CM_WINDOWS_ALLOW_MISSING   comma-separated program names that may be absent (testing only)
    CM_CARGO_JOBS              cargo -j value

  The speaker engine (sherpa-onnx, cargo feature `sherpa`) is built in when packaging\sherpa\fetch-windows.ps1 has
  run with the C runtime this build uses (/MT, its default, for +crt-static; -Crt md for CM_WINDOWS_CRT_STATIC=0)
  or SHERPA_ONNX_LIB_DIR is set. Without the libraries this builds with --no-default-features and "Find speakers"
  answers that the build has no speaker engine. CM_REQUIRE_SHERPA=1 makes that an error.

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

# ---- the speaker engine: the libraries must match the C runtime of this build (a mismatch is a link error) ----
$crtName = if ($crtStatic) { 'MT' } else { 'MD' }
$sherpaOut = Join-Path $root 'packaging\sherpa\out'
$useSherpa = $false
if ($env:SHERPA_ONNX_LIB_DIR) {
  $useSherpa = $true
  Write-Warning "SHERPA_ONNX_LIB_DIR is set ($($env:SHERPA_ONNX_LIB_DIR)): the speaker engine is linked from there; its C runtime is not checked"
} elseif (Test-Path (Join-Path $sherpaOut 'lib\onnxruntime.lib')) {
  $info = Join-Path $sherpaOut 'BUILD-INFO.txt'
  if ((Test-Path $info) -and (Select-String -Path $info -Pattern "C runtime: $crtName" -Quiet)) { $useSherpa = $true }
  else { Write-Warning "packaging\sherpa\out holds sherpa-onnx libraries for the other C runtime (this build needs $crtName); run packaging\sherpa\fetch-windows.ps1 -Crt $($crtName.ToLowerInvariant())" }
}
if (-not $useSherpa -and $env:CM_REQUIRE_SHERPA -eq '1') {
  Fail 'CM_REQUIRE_SHERPA=1 but the sherpa-onnx libraries for this C runtime are missing (packaging\sherpa\fetch-windows.ps1)'
}
$features = if ($useSherpa) { @() } else { @('--no-default-features') }
Write-Host "speaker engine: $(if ($useSherpa) { "built in ($crtName runtime)" } else { 'not built in (no libraries)' })"
if ($env:CM_SKIP_BUILD -ne '1') {
  if ($crtStatic) {
    # /MT everywhere (Rust crates and, through cc-rs and cmake, the C and C++ ones): no vcruntime140.dll.
    $env:RUSTFLAGS = (($env:RUSTFLAGS, '-C target-feature=+crt-static') -join ' ').Trim()
  }
  # whisper.cpp (cm-ai, built by whisper-rs-sys) defaults to -march=native: it was compiled with AVX-512 on the CI
  # runner and cm-desktop.exe died with an illegal instruction (0xc000001d, a `vmovups zmm`) when Transcribe loaded
  # the model on a ThinkPad T490 (AVX2 only), Nov 5, 2026. Same fixed baseline as bundle.sh and build-linux.sh:
  # the result must not depend on the build machine. whisper-rs-sys passes GGML_* variables to CMake but does not
  # re-run when they change, so its cached output (rust-cache) is dropped first.
  $env:GGML_NATIVE = 'OFF'; $env:GGML_AVX = 'ON'; $env:GGML_AVX2 = 'ON'; $env:GGML_FMA = 'ON'
  $env:GGML_F16C = 'ON'; $env:GGML_BMI2 = 'ON'; $env:GGML_AVX512 = 'OFF'
  # Optimisation flags of the whisper.cpp build. cmake-rs (used by whisper-rs-sys) builds with the Visual Studio
  # generator and then REPLACES CMAKE_C_FLAGS_<BUILD TYPE> / CMAKE_CXX_FLAGS_<BUILD TYPE> with the cc-rs flags minus
  # every -O flag, for the build type named by CMAKE_BUILD_TYPE. whisper-rs-sys sets that to "Release" in a release
  # build (the config actually built is Release too), so /O2 /Ob2 /DNDEBUG were dropped and ggml compiled with no
  # optimisation: 11x slower than the CI test build, whose debug_assertions make CMAKE_BUILD_TYPE "RelWithDebInfo",
  # a configuration that is never built, so its Release flags kept /O2 (Nov 5, 2026). cmake-rs leaves a variable
  # alone when it is already defined, and whisper-rs-sys forwards every CMAKE_* environment variable as a define.
  $crtFlag = if ($crtStatic) { '/MT' } else { '/MD' }
  $env:CMAKE_C_FLAGS_RELEASE = "/O2 /Ob2 /DNDEBUG $crtFlag"
  $env:CMAKE_CXX_FLAGS_RELEASE = "/O2 /Ob2 /DNDEBUG $crtFlag"
  Push-Location $root
  try {
    Write-Host '==> cargo clean -p whisper-rs-sys (portable CPU baseline: AVX2, no AVX-512)'
    Invoke-Native cargo @('clean', '--release', '-p', 'whisper-rs-sys')
    Write-Host '==> cargo build --release (capturemann, cm-mcp)'
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
      Invoke-Native cargo (@('rustc', '--release', '--locked') + $features + $jobs + @('-p', $p[0], '--bin', $p[1], '--') + $sub)
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
$ffDefault = Join-Path $root 'packaging\ffmpeg\out-windows-x64'
if (-not $env:CM_FFMPEG_BUNDLE -and (Test-Path (Join-Path $ffDefault 'ffmpeg.exe'))) {
  Write-Host "==> using the FFmpeg built by packaging/ffmpeg/build-windows.sh: $ffDefault"
  $env:CM_FFMPEG_BUNDLE = $ffDefault
}
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

# ---- the text engine (llama-server, AI-7) ----
$llDir = $null; $llLic = $false
$llDefault = Join-Path $root 'packaging\llama\out-windows-x64'
if (-not $env:CM_LLAMA_BUNDLE -and (Test-Path (Join-Path $llDefault 'llama-server.exe'))) {
  Write-Host "==> using the llama-server built by packaging/llama/build-windows.sh: $llDefault"
  $env:CM_LLAMA_BUNDLE = $llDefault
}
if ($env:CM_LLAMA_BUNDLE) {
  $llDir = $env:CM_LLAMA_BUNDLE
  if (-not (Test-Path (Join-Path $llDir 'llama-server.exe'))) { Fail "no llama-server.exe in CM_LLAMA_BUNDLE=$llDir" }
  $llLic = (Test-Path (Join-Path $llDir 'BUILD-INFO.txt')) -and (Test-Path (Join-Path $llDir 'LICENSES'))
  if (-not $llLic) {
    if ($env:CM_REQUIRE_LLAMA -eq '1') { Fail "$llDir has no BUILD-INFO.txt and LICENSES\" }
    Write-Warning 'no BUILD-INFO.txt / LICENSES next to llama-server.exe; this package must not be distributed'
  }
} elseif ($env:CM_REQUIRE_LLAMA -eq '1') {
  Fail 'CM_REQUIRE_LLAMA=1 but CM_LLAMA_BUNDLE is not set (build one with packaging/llama/build-windows.sh)'
}

# ---- the speaker-label models (AI-8): pyannote segmentation-3.0 (MIT) and CAM++ (Apache-2.0), ~35 MB ----
$spkDir = $null
$spkFiles = 'pyannote-segmentation-3-0.onnx', 'campplus-sv-zh-en-16k-advanced.onnx'
$spkLicenses = 'pyannote-segmentation-3.0-LICENSE.txt', 'campplus-3D-Speaker-NOTICE.txt'
if ($env:CM_NO_SPEAKER_MODELS -eq '1') {
  if ($env:CM_REQUIRE_SPEAKER_MODELS -eq '1') { Fail 'CM_NO_SPEAKER_MODELS=1 and CM_REQUIRE_SPEAKER_MODELS=1 contradict each other' }
  Write-Warning 'CM_NO_SPEAKER_MODELS=1: the speaker models are not bundled (a test package, not for release)'
} else {
  $spkCandidate = if ($env:CM_SPEAKER_MODELS_BUNDLE) { $env:CM_SPEAKER_MODELS_BUNDLE } else { Join-Path $sherpaOut 'models' }
  $spkMissing = @($spkFiles | Where-Object { -not (Test-Path (Join-Path $spkCandidate $_)) }) +
    @($spkLicenses | Where-Object { -not (Test-Path (Join-Path $spkCandidate "LICENSES\$_")) }) +
    @('BUILD-INFO.txt' | Where-Object { -not (Test-Path (Join-Path $spkCandidate $_)) })
  if ($spkMissing.Count -eq 0) { $spkDir = $spkCandidate; Write-Host "==> bundling the speaker models from $spkDir" }
  elseif ($env:CM_REQUIRE_SPEAKER_MODELS -eq '1') { Fail "CM_REQUIRE_SPEAKER_MODELS=1 but $spkCandidate lacks: $($spkMissing -join ', ') (packaging\sherpa\fetch-models.ps1)" }
  elseif ($env:CM_SPEAKER_MODELS_BUNDLE) { Fail "CM_SPEAKER_MODELS_BUNDLE=$spkCandidate lacks: $($spkMissing -join ', ')" }
  else { Write-Warning "no speaker models in $spkCandidate (run packaging\sherpa\fetch-models.ps1); this build downloads none and has no speaker labels until they are imported" }
}

# ---- licences ----
$tmp =Join-Path ([IO.Path]::GetTempPath()) ("cm-win-pack-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
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
      # Python on Windows reads files in the ANSI code page by default; the crate licence texts are UTF-8.
      $env:PYTHONUTF8 = '1'
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
  if ($llDir) {
    # Next to the programs: cm_ai::text_model::engine looks beside the running executable first.
    Copy-Item (Join-Path $llDir 'llama-server.exe') $stage
    if ($llLic) {
      $lll = Join-Path $stage 'Llama'; New-Item -ItemType Directory -Path $lll | Out-Null
      Copy-Item (Join-Path $llDir 'BUILD-INFO.txt') $lll
      Copy-Item (Join-Path $llDir 'LICENSES') (Join-Path $lll 'LICENSES') -Recurse
    }
  }
  if ($spkDir) {
    # <install dir>\Resources\Models\speaker: cm_ai::model::bundled_speaker_dir looks there on Windows.
    $spk = Join-Path $stage 'Resources\Models\speaker'; New-Item -ItemType Directory -Path $spk -Force | Out-Null
    foreach ($f in $spkFiles) { Copy-Item (Join-Path $spkDir $f) $spk }
    Copy-Item (Join-Path $spkDir 'BUILD-INFO.txt') $spk
    Copy-Item (Join-Path $spkDir 'LICENSES') (Join-Path $spk 'LICENSES') -Recurse
  }
  $doc = Join-Path $stage 'doc'; New-Item -ItemType Directory -Path $doc | Out-Null
  Copy-Item (Join-Path $root 'LICENSE'), (Join-Path $root 'NOTICE') $doc
  Copy-Item $licenses (Join-Path $doc 'THIRD_PARTY_LICENSES.md')
  $sherpaNotice = Join-Path $sherpaOut 'LICENSES\NOTICE.txt'
  if ($useSherpa -and (Test-Path $sherpaNotice)) { Copy-Item $sherpaNotice (Join-Path $doc 'sherpa-onnx-NOTICE.txt') }
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
  $speakerLine = if ($useSherpa -and $spkDir) { 'The speaker engine ("Find speakers") is built in and its two models ship in Resources\Models\speaker (MIT and Apache-2.0, licences there).' } elseif ($useSherpa) { 'The speaker engine ("Find speakers") is built in; this package has no speaker models (import them on the AI page).' } else { 'This build has no speaker engine ("Find speakers" is off).' }
  $readme = @"
CaptureMann $fullVersion for Windows (x64), early build.

Portable:  unzip anywhere and run cm-desktop.exe. Nothing is written outside this folder and your
           settings and library (%APPDATA%\CaptureMann and your Videos folder).
Command line:  capturemann.exe --help      MCP server for agents:  cm-mcp.exe
               (both start cm-daemon.exe, next to them, when the app is not running)
Add this folder to your PATH:  powershell -ExecutionPolicy Bypass -File path.ps1 -Action add -Dir "<this folder>"

Not available yet on Windows: screen, camera and audio RECORDING (screenshots, annotation, the library, local AI,
the CLI and MCP work). $speakerLine
Windows may show "Windows protected your PC" (SmartScreen): the early build is not code-signed yet.
Choose "More info" and "Run anyway", or compare the SHA-256 file first.
Licences: doc\ . ffmpeg.exe (if present) is FFmpeg under the LGPL, see FFmpeg\ .
llama-server.exe (if present) is llama.cpp (MIT), the built-in text engine, see Llama\ .
Resources\Models\speaker (if present): the two speaker-label models, pyannote segmentation-3.0 (MIT) and CAM++ (Apache-2.0), see its LICENSES\ .
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
    # LF line ending and no BOM: `sha256sum -c` on Linux and macOS reads it as written (Set-Content adds CRLF).
    [IO.File]::WriteAllText("$f.sha256", ("{0} *{1}`n" -f $h, (Split-Path $f -Leaf)), (New-Object Text.UTF8Encoding($false)))
    Get-Item $f | ForEach-Object { Write-Host ("==> {0}  ({1:N1} MB)" -f $_.FullName, ($_.Length / 1MB)) }
    Get-Content "$f.sha256"
  }
} finally {
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
}
