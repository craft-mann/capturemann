# Long Shot overlay on the installed build with an isolated daemon (library, config and socket in C:\lab\rc1-long).
$inst = "$env:LOCALAPPDATA\Programs\CaptureMann"
$d = 'C:\lab\rc1-long'
Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue
New-Item -ItemType Directory "$d\lib", "$d\cfg" -Force | Out-Null
$env:CAPTUREMANN_LIBRARY = "$d\lib"; $env:CAPTUREMANN_CONFIG_DIR = "$d\cfg"; $env:CAPTUREMANN_SOCKET = '\\.\pipe\cm-rc1-long'
$dm = Start-Process "$inst\cm-daemon.exe" -PassThru
Start-Sleep 5
"daemon alive: $(-not $dm.HasExited)"
$p = Start-Process "$inst\cm-overlay.exe" -ArgumentList 'long --auto-scroll available' -PassThru -RedirectStandardOutput "$d\ov.out" -RedirectStandardError "$d\ov.err"
Start-Sleep 6
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$bmp = New-Object Drawing.Bitmap ([Windows.Forms.SystemInformation]::VirtualScreen.Width), ([Windows.Forms.SystemInformation]::VirtualScreen.Height)
$g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen(0, 0, 0, 0, $bmp.Size); $bmp.Save("$d\shot.png")
$sum = 0; for ($x = 0; $x -lt $bmp.Width; $x += 97) { for ($y = 0; $y -lt $bmp.Height; $y += 97) { $c = $bmp.GetPixel($x, $y); $sum += $c.R + $c.G + $c.B } }
"mean sampled brightness: $([math]::Round($sum / ((($bmp.Width / 97) * ($bmp.Height / 97)) * 3), 1))"
if ($p.HasExited) { "overlay exited early, code $($p.ExitCode)" } else { "overlay running after 6 s; stopping it" ; Stop-Process -Id $p.Id -Force }
"stdout: $(Get-Content "$d\ov.out" -Raw)"
"stderr: $(Get-Content "$d\ov.err" -Raw)"
Stop-Process -Id $dm.Id -Force -ErrorAction SilentlyContinue
Start-Sleep 1
Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue
