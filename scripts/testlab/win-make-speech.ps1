# Makes English speech test audio on the Windows lab laptop with the voices Windows already has
# (System.Speech, the old SAPI voices). Nothing is downloaded and nothing is installed.
#
#   C:\lab\win-make-speech.ps1 [-OutDir C:\lab\speech] [-Repeat 1] [-AllowOneVoice]
#
# Writes into OutDir (16 kHz, mono, 16-bit PCM WAV, the format cm-ai's real_model and real_diarize read directly):
#   speech2.wav / speech2.txt   two speakers taking turns (two different voices) and the words they say
#   speech1.wav / speech1.txt   the same words in one voice (one speaker)
#   VOICES.txt                  which voices were used
# Use them as CM_WHISPER_AUDIO (+ CM_WHISPER_REFERENCE) and CM_DIARIZE_AUDIO. Synthetic speech: quote it as such.
# -Repeat N says the whole script N times (a longer clip for a speed measurement).
# -AllowOneVoice: when the machine has only one English voice, vary its rate for the second speaker (a weak test).
param(
  [string]$OutDir = 'C:\lab\speech',
  [int]$Repeat = 1,
  [switch]$AllowOneVoice
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech

$turns = @(
  'Good morning everyone, and thank you for joining the call today.',
  'Thanks for having me. I would like to start with the results of the last quarter.',
  'Please go ahead. We have about twenty minutes before the next meeting begins.',
  'The new release was shipped on Tuesday, and the number of crashes dropped by half.',
  'That is good news. Did the team finish the work on the export feature as well?',
  'Yes, the export feature is done, and the recordings now play back without any gaps.',
  'Excellent. Then the last item is the plan for next month, which I will send after the meeting.',
  'Sounds good to me. Thank you all, and have a nice afternoon.'
)

$probe = New-Object System.Speech.Synthesis.SpeechSynthesizer
$voices = @($probe.GetInstalledVoices() | Where-Object { $_.Enabled } | ForEach-Object { $_.VoiceInfo } |
  Where-Object { $_.Culture.Name -like 'en-*' })
$probe.Dispose()
if ($voices.Count -eq 0) { throw 'no English System.Speech voice is installed' }
$first = $voices[0]
# A different voice for speaker B; prefer another gender, then any other voice.
$second = $voices | Where-Object { $_.Name -ne $first.Name -and $_.Gender -ne $first.Gender } | Select-Object -First 1
if (-not $second) { $second = $voices | Where-Object { $_.Name -ne $first.Name } | Select-Object -First 1 }
$rateB = 0
if (-not $second) {
  if (-not $AllowOneVoice) { throw "only one English voice ($($first.Name)) is installed; use -AllowOneVoice for a weak two-speaker clip" }
  $second = $first; $rateB = 2
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
"A: $($first.Name) ($($first.Gender), $($first.Culture.Name))`r`nB: $($second.Name) ($($second.Gender), $($second.Culture.Name)), rate $rateB" |
  Out-File (Join-Path $OutDir 'VOICES.txt') -Encoding ascii

# The PCM bytes of the 'data' chunk of a WAV file.
function Get-WavData([string]$path) {
  $b = [System.IO.File]::ReadAllBytes($path)
  $i = 12
  while ($i + 8 -le $b.Length) {
    $id = [System.Text.Encoding]::ASCII.GetString($b, $i, 4)
    $len = [BitConverter]::ToUInt32($b, $i + 4)
    if ($id -eq 'data') {
      $n = [Math]::Min([int64]$len, [int64]($b.Length - $i - 8))
      $out = New-Object byte[] $n
      [Array]::Copy($b, $i + 8, $out, 0, $n)
      return , $out
    }
    $i += 8 + $len + ($len % 2)
  }
  throw "no data chunk in $path"
}

function Say([object]$voice, [int]$rate, [string]$text, [string]$file) {
  $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
  try {
    $s.SelectVoice($voice.Name)
    $s.Rate = $rate
    $fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
    $s.SetOutputToWaveFile($file, $fmt)
    $s.Speak($text)
  } finally { $s.Dispose() }
}

function Write-Wav([string]$path, [System.Collections.Generic.List[byte]]$pcm) {
  $fs = [System.IO.File]::Create($path)
  $w = New-Object System.IO.BinaryWriter($fs)
  try {
    $w.Write([System.Text.Encoding]::ASCII.GetBytes('RIFF')); $w.Write([int](36 + $pcm.Count))
    $w.Write([System.Text.Encoding]::ASCII.GetBytes('WAVEfmt ')); $w.Write([int]16)
    $w.Write([int16]1); $w.Write([int16]1); $w.Write([int]16000); $w.Write([int]32000); $w.Write([int16]2); $w.Write([int16]16)
    $w.Write([System.Text.Encoding]::ASCII.GetBytes('data')); $w.Write([int]$pcm.Count)
    $w.Write($pcm.ToArray())
  } finally { $w.Dispose(); $fs.Dispose() }
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("cm-speech-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $silence = New-Object byte[] (16000 * 2 * 6 / 10)   # 0.6 s
  $two = New-Object 'System.Collections.Generic.List[byte]'
  $one = New-Object 'System.Collections.Generic.List[byte]'
  $text = New-Object 'System.Collections.Generic.List[string]'
  for ($r = 0; $r -lt $Repeat; $r++) {
    for ($t = 0; $t -lt $turns.Count; $t++) {
      $f2 = Join-Path $tmp "b-$r-$t.wav"; $f1 = Join-Path $tmp "a-$r-$t.wav"
      if ($t % 2 -eq 0) { Say $first 0 $turns[$t] $f2 } else { Say $second $rateB $turns[$t] $f2 }
      Say $first 0 $turns[$t] $f1
      $two.AddRange([byte[]](Get-WavData $f2)); $two.AddRange($silence)
      $one.AddRange([byte[]](Get-WavData $f1)); $one.AddRange($silence)
      $text.Add($turns[$t])
    }
  }
  Write-Wav (Join-Path $OutDir 'speech2.wav') $two
  Write-Wav (Join-Path $OutDir 'speech1.wav') $one
  $joined = ($text -join ' ')
  $joined | Out-File (Join-Path $OutDir 'speech2.txt') -Encoding ascii
  $joined | Out-File (Join-Path $OutDir 'speech1.txt') -Encoding ascii
  "speech2.wav: {0:N1} s, two voices; speech1.wav: {1:N1} s, one voice (in $OutDir)" -f ($two.Count / 32000.0), ($one.Count / 32000.0)
} finally { Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue }
