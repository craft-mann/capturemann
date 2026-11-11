param([string]$List = 'feedT,feedH,feedF,feedD,feedM,feedG')
foreach ($n in ($List -split ',')) {
  Write-Output "######## $n"
  & C:\lab\velospike\vs-reinstall.ps1 | Out-Null
  & C:\lab\velospike\vs-upd.ps1 -Tag $n -WithStand 0 -Feed "C:\lab\velospike\$n" -WaitSec 20 2>&1 |
    Where-Object { $_ -match 'check:|download:|update:|after:|nupkg|partial|exited at|\.log' -and $_ -notmatch 'INFO' } |
    ForEach-Object { if ($_.Length -gt 330) { $_.Substring(0, 330) } else { $_ } }
}
