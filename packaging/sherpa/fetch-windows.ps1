<#
.SYNOPSIS
  Fetches the prebuilt static libraries of the speaker engine (AI-8): sherpa-onnx v2.0.1 with its ONNX Runtime,
  for Windows x64 (MSVC). cm-ai links them statically (feature `sherpa`).

.DESCRIPTION
  Run on Windows from any directory:   packaging\sherpa\fetch-windows.ps1 [-Crt mt|md]
  Output: packaging\sherpa\out\{lib\*.lib,BUILD-INFO.txt,LICENSES\}
          (git-ignored; `.cargo/config.toml` points SHERPA_ONNX_LIB_DIR at out\lib, so a plain `cargo build`
          finds the libraries once this has run).

  Why this script exists: the Cargo build never downloads anything. The vendored sherpa-onnx-sys/build.rs only
  links what is here. This script is the one place that downloads, and it checks the SHA-256 before unpacking.

  -Crt picks the archive by the C runtime the Rust build uses. A mismatch is a link error (LNK2038):
    mt  (default)  /MT, static runtime: what packaging\windows\build-windows.ps1 builds (it sets +crt-static)
    md             /MD, dynamic runtime: a plain `cargo clippy` / `cargo test` (the CI jobs)

  Environment:
    CM_SHERPA_SRC_DIR  where the archive is cached (default packaging\sherpa\.downloads, git-ignored). A missing
                       archive is downloaded and checked against the pinned SHA-256; a mismatch deletes it and stops.
    CM_SHERPA_OUT      output directory (default packaging\sherpa\out)
    CM_SOURCE_REPO     the project URL written into BUILD-INFO.txt

  Needs: PowerShell 5.1 or later and tar.exe (bsdtar, in Windows 10 1803 and later; it unpacks .tar.bz2).

  Licence: sherpa-onnx is Apache-2.0 and ONNX Runtime is MIT. The archive holds only lib\*.lib and no licence
  files, so the notes written to LICENSES\ say where the texts are (as in fetch-macos.sh).

  NOT RUN ON WINDOWS YET (written on a Mac; the pinned hashes were checked there by downloading both archives).
#>
[CmdletBinding()]
param(
  [ValidateSet('mt', 'md')][string]$Crt = 'mt'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail([string]$msg) { Write-Error "fetch-windows.ps1: $msg"; exit 1 }

if ([Environment]::OSVersion.Platform -ne 'Win32NT') { Fail 'this script only runs on Windows' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail "unsupported CPU $($env:PROCESSOR_ARCHITECTURE) (x64 only)" }

# ---- pinned archives (downloaded and hashed on Nov 4, 2026; the sizes and hashes match GitHub's asset digests) ----
$version = '2.0.1'
$pins = @{
  'mt' = @{ Sha256 = '23e7b986b197eda0bbc47ff3d7d5c87afd6ef2b9077214ba26b0f1ec52ef0e66'; Bytes = 122863596 }
  'md' = @{ Sha256 = '7c1f3ef15487f2a06625ef06685a9f96f03c0a1f0b3ad52de9e3bf6af6c1be39'; Bytes = 96089581 }
}
$crtName = $Crt.ToUpperInvariant()
$sha256 = $pins[$Crt].Sha256
$bytes = $pins[$Crt].Bytes
$archiveName = "sherpa-onnx-v$version-win-x64-static-$crtName-Release-lib.tar.bz2"
$url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$version/$archiveName"
$dirName = "sherpa-onnx-v$version-win-x64-static-$crtName-Release-lib"

# The libraries vendor/sherpa-onnx-sys/build.rs links (STATIC_LIBS there must match). Windows names have no "lib" prefix.
$libs = @('sherpa-onnx-c-api', 'sherpa-onnx-core', 'kaldi-decoder-core', 'sherpa-onnx-kaldifst-core',
  'sherpa-onnx-fstfar', 'sherpa-onnx-fst', 'kaldi-native-fbank-core', 'kissfft-float', 'onnxruntime',
  'ssentencepiece_core', 'utf8proc')

$sourceRepo = if ($env:CM_SOURCE_REPO) { $env:CM_SOURCE_REPO } else { 'https://github.com/craft-mann/capturemann' }

if ($sha256 -notmatch '^[0-9a-f]{64}$') { Fail 'the pinned SHA-256 is not 64 lowercase hex digits' }
$tar = Get-Command tar.exe -ErrorAction SilentlyContinue
if (-not $tar) { Fail 'tar.exe not found (Windows 10 1803 or later has it in System32)' }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$cache = if ($env:CM_SHERPA_SRC_DIR) { $env:CM_SHERPA_SRC_DIR } else { Join-Path $root 'packaging\sherpa\.downloads' }
$out = if ($env:CM_SHERPA_OUT) { $env:CM_SHERPA_OUT } else { Join-Path $root 'packaging\sherpa\out' }
$archive = Join-Path $cache $archiveName

function Get-Sha([string]$path) { (Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant() }

New-Item -ItemType Directory -Force -Path $cache | Out-Null
if ((Test-Path $archive) -and ((Get-Sha $archive) -ne $sha256)) {
  Write-Warning "the cached $archiveName does not match the pinned SHA-256; deleting it"
  Remove-Item -Force $archive
}
if (-not (Test-Path $archive)) {
  Write-Host "Downloading $url"
  $part = "$archive.part"
  Remove-Item -Force -ErrorAction SilentlyContinue $part
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
  try {
    $ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is very slow with the progress bar on
    Invoke-WebRequest -Uri $url -OutFile $part -UseBasicParsing
  } catch { Remove-Item -Force -ErrorAction SilentlyContinue $part; Fail "download failed: $_" }
  $size = (Get-Item $part).Length
  if ($size -ne $bytes) { Remove-Item -Force $part; Fail "the download is $size bytes, expected $bytes" }
  if ((Get-Sha $part) -ne $sha256) { Remove-Item -Force $part; Fail 'SHA-256 mismatch: the download is not the pinned archive; deleted' }
  Move-Item -Force $part $archive
}
Write-Host "Archive OK: $archiveName (sha256 $sha256)"

# The archive must hold only files under its own folder (no absolute paths, no "..").
$members = & $tar.Source -tjf $archive
if ($LASTEXITCODE -ne 0) { Fail 'tar could not read the archive' }
foreach ($m in $members) {
  $n = $m -replace '\\', '/'
  if (($n -ne "$dirName/" -and -not $n.StartsWith("$dirName/")) -or $n -match '(^|/)\.\.(/|$)') { Fail "unexpected archive member: $m" }
}

$work = Join-Path ([IO.Path]::GetTempPath()) ('cm-sherpa-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $work | Out-Null
try {
  & $tar.Source -xjf $archive -C $work
  if ($LASTEXITCODE -ne 0) { Fail 'tar failed to unpack the archive' }
  $libSrc = Join-Path $work "$dirName\lib"
  if (-not (Test-Path $libSrc)) { Fail "the archive has no $dirName\lib" }
  foreach ($l in $libs) {
    if (-not (Test-Path (Join-Path $libSrc "$l.lib"))) { Fail "the archive has no $l.lib" }
  }

  # Build the new tree next to the old one and swap, so a failure never leaves half a folder.
  $new = "$out.new.$PID"
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $new
  New-Item -ItemType Directory -Path (Join-Path $new 'lib'), (Join-Path $new 'LICENSES') | Out-Null
  foreach ($l in $libs) { Copy-Item (Join-Path $libSrc "$l.lib") (Join-Path $new 'lib') }

  $notice = @"
sherpa-onnx static libraries (speaker engine of CaptureMann, AI-8)

The release archive $archiveName contains only lib\*.lib and no licence files. What is in them:

  sherpa-onnx (sherpa-onnx-*, kaldi-*, kissfft, utf8proc, ssentencepiece)
      Apache License 2.0. Source and licence: https://github.com/k2-fsa/sherpa-onnx (tag v$version).
      The Apache-2.0 text is also in vendor/sherpa-onnx/LICENSE of the CaptureMann repository.
      The small libraries it builds in keep their own permissive licences in its source tree
      (kaldi-native-fbank and kaldi-decoder: Apache-2.0; kissfft: BSD-3-Clause; utf8proc: MIT).
  ONNX Runtime (onnxruntime.lib)
      MIT License, Copyright (c) Microsoft Corporation. https://github.com/microsoft/onnxruntime
      (the MIT text is packaging/sherpa/licenses/onnxruntime-MIT.txt of the repository, and the
      ThirdPartyNotices.txt of ONNX Runtime 1.28.2, the version inside sherpa-onnx v$version,
      is packaging/sherpa/licenses/ThirdPartyNotices-onnxruntime.txt; both are in THIRD_PARTY_LICENSES.md).

The libraries are linked statically into cm-desktop and cm-daemon; nothing here is shipped as a separate file.
The speaker MODEL files (pyannote segmentation 3.0, MIT; CAM++ English, Apache-2.0) are not part of
this archive: they are downloaded or imported by the user from the AI page.
"@
  Set-Content -Path (Join-Path $new 'LICENSES\NOTICE.txt') -Value $notice -Encoding UTF8

  $libLines = $libs | ForEach-Object { "  $_.lib  sha256 $(Get-Sha (Join-Path $new "lib\$_.lib"))" }
  $info = @(
    'CaptureMann sherpa-onnx libraries: fetch record'
    "Fetched by packaging/sherpa/fetch-windows.ps1 on $([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'))"
    "C runtime: $crtName (Rust build must match: mt = +crt-static, md = default)"
    "Host: $([Environment]::OSVersion.VersionString)"
    "Project: $sourceRepo"
    ''
    'Archive (pinned, SHA-256 checked before unpacking):'
    "  $url"
    "  sha256 $sha256"
    "  size   $bytes bytes"
    ''
    'Libraries copied to lib\ (the ones vendor/sherpa-onnx-sys links):'
  ) + $libLines + @('', 'Not copied: sherpa-onnx-cxx-api.lib (not used).')
  Set-Content -Path (Join-Path $new 'BUILD-INFO.txt') -Value $info -Encoding UTF8

  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $out
  Move-Item $new $out
  Write-Host "Done: $out\lib ($($libs.Count) libraries, $crtName runtime)."
  Write-Host 'cargo finds it through .cargo/config.toml (SHERPA_ONNX_LIB_DIR=packaging/sherpa/out/lib).'
} finally {
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $work
}
