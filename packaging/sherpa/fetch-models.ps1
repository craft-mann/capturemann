<#
.SYNOPSIS
  Windows twin of fetch-models.sh: fetches the two speaker-label models (AI-8) that ship INSIDE the app.

.DESCRIPTION
  pyannote segmentation-3.0 (MIT) and the 3D-Speaker CAM++ Chinese-English embedding model (Apache-2.0).
  Same pins as fetch-models.sh (keep the two files in step), same output:
    packaging\sherpa\out\models\{pyannote-segmentation-3-0.onnx, campplus-sv-zh-en-16k-advanced.onnx,
                                 BUILD-INFO.txt, LICENSES\}
  packaging\windows\build-windows.ps1 copies that folder to <install dir>\Resources\Models\speaker\.
  A download is checked against the pinned SHA-256 and size; a mismatch stops the script and deletes the file.
  The tar that comes with Windows 10+ (bsdtar) unpacks the .tar.bz2 here at BUILD time; nothing in the app unpacks archives.

  Environment: CM_SHERPA_SRC_DIR (download cache, default packaging\sherpa\.downloads), CM_SHERPA_OUT
  (default packaging\sherpa\out). Unlike fetch-windows.ps1 this does not replace the whole out folder: only out\models.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail([string]$msg) { Write-Error "fetch-models.ps1: $msg"; exit 1 }

# ---- pins (read and hashed on Oct 27, 2026; identical to fetch-models.sh) ----
$SegArchive = 'sherpa-onnx-pyannote-segmentation-3-0.tar.bz2'
$SegUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/$SegArchive"
$SegArchiveSha = '24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488'
$SegArchiveBytes = 6958444
$SegDir = 'sherpa-onnx-pyannote-segmentation-3-0'
$SegFile = 'pyannote-segmentation-3-0.onnx'
$SegSha = '220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079'
$SegBytes = 5992913

$EmbDownload = '3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx'
$EmbUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/$EmbDownload"
$EmbFile = 'campplus-sv-zh-en-16k-advanced.onnx'
$EmbSha = 'aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2'
$EmbBytes = 28281164

if (-not (Get-Command tar -ErrorAction SilentlyContinue)) { Fail 'required tool not found: tar' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$cache = if ($env:CM_SHERPA_SRC_DIR) { $env:CM_SHERPA_SRC_DIR } else { Join-Path $root 'packaging\sherpa\.downloads' }
$out = if ($env:CM_SHERPA_OUT) { $env:CM_SHERPA_OUT } else { Join-Path $root 'packaging\sherpa\out' }

function Test-Pin([string]$path, [string]$sha, [long]$bytes) {
  (Test-Path $path) -and ((Get-Item $path).Length -eq $bytes) -and ((Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant() -eq $sha)
}
function Get-Pinned([string]$name, [string]$url, [string]$sha, [long]$bytes) {
  New-Item -ItemType Directory -Force -Path $cache | Out-Null
  $f = Join-Path $cache $name
  if ((Test-Path $f) -and -not (Test-Pin $f $sha $bytes)) {
    Write-Warning "the cached $name does not match the pin; deleting it"
    Remove-Item -Force $f
  }
  if (-not (Test-Path $f)) {
    Write-Host "Downloading $url"
    $part = "$f.part"
    Remove-Item -Force -ErrorAction SilentlyContinue $part
    try { Invoke-WebRequest -Uri $url -OutFile $part -UseBasicParsing -MaximumRetryCount 3 -RetryIntervalSec 5 }
    catch { Remove-Item -Force -ErrorAction SilentlyContinue $part; Fail "download failed: $url ($_)" }
    if (-not (Test-Pin $part $sha $bytes)) { Remove-Item -Force $part; Fail "$name does not match the pinned SHA-256/size; deleted" }
    Move-Item $part $f
  }
  Write-Host "OK: $name (sha256 $sha)"
  return $f
}

$segArchivePath = Get-Pinned $SegArchive $SegUrl $SegArchiveSha $SegArchiveBytes
$embPath = Get-Pinned $EmbDownload $EmbUrl $EmbSha $EmbBytes

# Only these members are taken out of the archive, and only after the listing shows they are under its own folder.
foreach ($m in (& tar -tjf $segArchivePath)) {
  $m = "$m".TrimEnd('/', '\')
  if ($m -match '\.\.' -or ($m -ne $SegDir -and -not $m.StartsWith("$SegDir/"))) { Fail "unexpected archive member: $m" }
}
if ($LASTEXITCODE -ne 0) { Fail 'tar could not list the segmentation archive (bzip2 support?)' }
$work = Join-Path ([IO.Path]::GetTempPath()) ("cm-speaker-models-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $work | Out-Null
$new = Join-Path $out "models.new.$PID"
try {
  & tar -xjf $segArchivePath -C $work "$SegDir/model.onnx" "$SegDir/LICENSE"
  if ($LASTEXITCODE -ne 0) { Fail 'tar could not unpack the segmentation archive' }
  $segSrc = Join-Path $work "$SegDir\model.onnx"
  $segLic = Join-Path $work "$SegDir\LICENSE"
  foreach ($f in $segSrc, $segLic) {
    if (-not (Test-Path $f -PathType Leaf) -or ((Get-Item $f).Attributes -band [IO.FileAttributes]::ReparsePoint)) { Fail "the archive has no plain $(Split-Path $f -Leaf)" }
  }
  New-Item -ItemType Directory -Force -Path $out | Out-Null
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $new
  New-Item -ItemType Directory -Path (Join-Path $new 'LICENSES') | Out-Null
  Copy-Item $segSrc (Join-Path $new $SegFile)
  Copy-Item $embPath (Join-Path $new $EmbFile)
  $segLicOut = Join-Path $new 'LICENSES\pyannote-segmentation-3.0-LICENSE.txt'
  Copy-Item $segLic $segLicOut
  if (-not (Select-String -Path $segLicOut -Pattern 'MIT' -Quiet)) { Fail 'the segmentation LICENSE is not the MIT text' }
  if (-not (Test-Pin (Join-Path $new $SegFile) $SegSha $SegBytes)) { Fail "$SegFile does not match the pinned SHA-256/size" }
  if (-not (Test-Pin (Join-Path $new $EmbFile) $EmbSha $EmbBytes)) { Fail "$EmbFile does not match the pinned SHA-256/size" }

  $notice = @"
CAM++ speaker embedding model (CaptureMann, speaker labels, AI-8)

File: $EmbFile (published as $EmbDownload)
Model: CAM++ Chinese-English 16 kHz ("advanced", trained on large-scale Chinese and English speaker data;
       the model card does not list the data), from the 3D-Speaker project of Alibaba / ModelScope
       (https://github.com/modelscope/3D-Speaker), licensed under the Apache License, Version 2.0
       (https://www.apache.org/licenses/LICENSE-2.0; licence field of the ModelScope card
       iic/speech_campplus_sv_zh_en_16k-common_advanced: "Apache License 2.0").
Obtained from: $EmbUrl
Converted/hosted by the sherpa-onnx project (https://github.com/k2-fsa/sherpa-onnx).
The file is shipped unmodified. The download carries no licence file of its own; the Apache-2.0
text is in the LICENSE file of the 3D-Speaker repository and in vendor/sherpa-onnx/LICENSE.
"@
  [IO.File]::WriteAllText((Join-Path $new 'LICENSES\campplus-3D-Speaker-NOTICE.txt'), ($notice -replace "`r`n", "`n") + "`n", (New-Object Text.UTF8Encoding($false)))

  $info = @"
CaptureMann speaker models: fetch record
Fetched by packaging/sherpa/fetch-models.ps1 on $([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'))

$SegFile   (model.onnx from $SegArchive)
  archive $SegUrl
  archive sha256 $SegArchiveSha
  sha256 $SegSha   size $SegBytes bytes   licence MIT (Copyright 2022 CNRS)
$EmbFile
  source  $EmbUrl
  sha256 $EmbSha   size $EmbBytes bytes   licence Apache-2.0
"@
  [IO.File]::WriteAllText((Join-Path $new 'BUILD-INFO.txt'), ($info -replace "`r`n", "`n") + "`n", (New-Object Text.UTF8Encoding($false)))

  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue (Join-Path $out 'models')
  Move-Item $new (Join-Path $out 'models')
} finally {
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $work
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $new
}
Write-Host "Done: $(Join-Path $out 'models') ($SegFile, $EmbFile, LICENSES\)."
