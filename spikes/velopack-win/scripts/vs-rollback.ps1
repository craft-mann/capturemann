$root = "$env:LOCALAPPDATA\CaptureMannSpike"
function Show($tag, $feed) {
  & C:\lab\velospike\vs-upd.ps1 -Tag $tag -WithStand 0 -Feed $feed -WaitSec 25 2>&1 |
    Where-Object { $_ -match 'main-enter|HOOK|check:|download:|update:|after:|exited at|Hook|hook|ERROR|rollback|Rollback|restor|Restor|revert|Backing|Replacing|Unable|fail|Fail|launch' -and $_ -notmatch 'About to launch|Environment block' } |
    ForEach-Object { if ($_.Length -gt 300) { $_.Substring(0, 300) } else { $_ } }
  "current version now: " + ((Get-Content "$root\current\sq.version" -ErrorAction SilentlyContinue | Select-String '<version>') -join '')
  "current has velospike.exe: $(Test-Path "$root\current\velospike.exe") size=$((Get-Item "$root\current\velospike.exe" -ErrorAction SilentlyContinue).Length)"
  "root: " + ((Get-ChildItem $root -Force | ForEach-Object Name) -join ', ')
}
Write-Output "######## R1: new version's after-update hook exits 5"
& C:\lab\velospike\vs-reinstall.ps1 -Flags fail_after_update | Out-Null
Show 'R1' 'C:\lab\velospike\feedA'
Write-Output "######## R2: new version's velospike.exe is not a valid executable"
& C:\lab\velospike\vs-reinstall.ps1 | Out-Null
Show 'R2' 'C:\lab\velospike\laptop4'
