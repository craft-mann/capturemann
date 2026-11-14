# rc1 extra checks on the installed build: Open With registration, Long Shot overlay start, Click Guide strings.
$inst = "$env:LOCALAPPDATA\Programs\CaptureMann"
"== Open With registry"
foreach ($e in '.png', '.jpg', '.jpeg') {
  $k = "HKCU:\Software\Classes\$e\OpenWithList\cm-desktop.exe"
  "$e OpenWithList: $(Test-Path $k)"
}
$app = Get-ItemProperty "HKCU:\Software\Classes\Applications\cm-desktop.exe\shell\open\command" -ErrorAction SilentlyContinue
"command: $($app.'(default)')"
$st = (Get-Item "HKCU:\Software\Classes\Applications\cm-desktop.exe\SupportedTypes" -ErrorAction SilentlyContinue).Property -join ','
"SupportedTypes: $st"
"== Click Guide strings in cm-desktop.exe"
$b = [IO.File]::ReadAllBytes("$inst\cm-desktop.exe")
$s = [Text.Encoding]::ASCII.GetString($b)
"New Click Guide present: $($s.Contains('New Click Guide'))"
"== Long Shot overlay start"
$env:CAPTUREMANN_SOCKET = 'cm-rc1-overlay-test'
$p = Start-Process "$inst\cm-overlay.exe" -ArgumentList 'long --auto-scroll unavailable' -PassThru -RedirectStandardOutput C:\lab\rc1-ov.out -RedirectStandardError C:\lab\rc1-ov.err
Start-Sleep 6
if ($p.HasExited) { "overlay exited early, code $($p.ExitCode)" } else { "overlay running after 6 s (pid $($p.Id)); stopping"; Stop-Process -Id $p.Id -Force }
"stdout: $(Get-Content C:\lab\rc1-ov.out -Raw)"
"stderr: $(Get-Content C:\lab\rc1-ov.err -Raw)"
Remove-Item C:\lab\rc1-ov.out, C:\lab\rc1-ov.err -ErrorAction SilentlyContinue
