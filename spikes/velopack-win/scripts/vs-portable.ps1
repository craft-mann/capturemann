$d = 'C:\lab\velospike\portable'
Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue
Expand-Archive C:\lab\velospike\Portable-spike1.zip $d
Get-ChildItem $d -Force | ForEach-Object { "$($_.Name) $($_.Length)" }
Write-Output "--- vcli info from the portable layout (installed-by-Velopack check)"
& "$d\current\vcli.exe" info
Write-Output "--- vcli info from the ORIGINAL 0.3.1 zip layout (flat folder, no Velopack files): copy of vcli next to real exes"
New-Item -ItemType Directory -Force C:\lab\velospike\flat | Out-Null
Copy-Item C:\lab\velospike\bin\vcli.exe C:\lab\velospike\flat\vcli.exe -Force
& C:\lab\velospike\flat\vcli.exe info
Write-Output "--- same copy placed inside the NSIS install folder (the real 0.3.1 NSIS layout)"
Copy-Item C:\lab\velospike\bin\vcli.exe "$env:LOCALAPPDATA\Programs\CaptureMann\vcli-probe.exe" -Force
& "$env:LOCALAPPDATA\Programs\CaptureMann\vcli-probe.exe" info
Remove-Item "$env:LOCALAPPDATA\Programs\CaptureMann\vcli-probe.exe" -Force
Write-Output "--- Update.exe / stub in the portable layout"
Get-FileHash "$d\Update.exe" -Algorithm SHA256 | ForEach-Object Hash
Get-Item "$d\CaptureMann.exe", "$d\Update.exe" | ForEach-Object { "$($_.Name) $($_.Length) version=$($_.VersionInfo.FileVersion) product=$($_.VersionInfo.ProductName) company=$($_.VersionInfo.CompanyName) desc=$($_.VersionInfo.FileDescription)" }
Get-AuthenticodeSignature "$d\Update.exe" | ForEach-Object { "Update.exe signature: $($_.Status)" }
"Setup.exe version info: " + (Get-Item C:\lab\velospike\Setup-spike1.exe).VersionInfo.FileDescription + " / " + (Get-Item C:\lab\velospike\Setup-spike1.exe).VersionInfo.ProductName + " / " + (Get-Item C:\lab\velospike\Setup-spike1.exe).VersionInfo.CompanyName
Write-Output "--- SmartScreen related facts (read-only)"
(Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer' -Name SmartScreenEnabled -ErrorAction SilentlyContinue).SmartScreenEnabled
Get-Item C:\lab\velospike\Setup-spike1.exe -Stream * | ForEach-Object { "stream $($_.Stream)" }
