# Copies the installed 0.3.3 folder to C:\lab\trackF2-inst and puts the freshly built cm-desktop.exe in it.
$src = "$env:LOCALAPPDATA\Programs\CaptureMann"
$dst = 'C:\lab\trackF2-inst'
Remove-Item -Recurse -Force $dst -ErrorAction SilentlyContinue
Copy-Item -Recurse $src $dst
Copy-Item C:\wt\target\release\cm-desktop.exe "$dst\cm-desktop.exe" -Force
Get-ChildItem "$dst\cm-desktop.exe", "$src\cm-desktop.exe" | ForEach-Object { "$($_.FullName) $($_.Length) $($_.LastWriteTime)" }
(Get-FileHash "$dst\cm-desktop.exe").Hash
