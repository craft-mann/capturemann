$ErrorActionPreference = 'Stop'
# feedA: only spike1 + spike2 (single step). feed: all three (skip step). feedBad*: faults built later.
function Make-Feed($name, $versions) {
  $d = "C:\lab\velospike\$name"
  Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory $d | Out-Null
  $src = Get-Content C:\lab\velospike\feed\releases.win.json -Raw | ConvertFrom-Json
  $keep = @($src.Assets | Where-Object { $versions -contains $_.Version })
  foreach ($a in $keep) { New-Item -ItemType HardLink -Path "$d\$($a.FileName)" -Target "C:\lab\velospike\feed\$($a.FileName)" | Out-Null }
  [IO.File]::WriteAllText("$d\releases.win.json", (@{ Assets = $keep } | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding($false)))
  "$name : $($keep.Count) assets"
}
Make-Feed 'feedA' @('0.3.2-spike1', '0.3.2-spike2')
Get-ChildItem C:\lab\velospike\feedA | Select-Object Name, Length | Format-Table -Auto | Out-String
