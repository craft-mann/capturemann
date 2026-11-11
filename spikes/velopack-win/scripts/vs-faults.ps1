param([string]$Only = '')
$ErrorActionPreference = 'Stop'
$base = 'C:\lab\velospike\feed'
$src = Get-Content "$base\releases.win.json" -Raw | ConvertFrom-Json
function Asset($ver, $type) { $src.Assets | Where-Object { $_.Version -eq $ver -and $_.Type -eq $type } }
function Write-Feed($dir, $assets) {
  [IO.File]::WriteAllText("$dir\releases.win.json", (@{ Assets = @($assets) } | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding($false)))
}
function New-Dir($name) { $d = "C:\lab\velospike\$name"; Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $d | Out-Null; $d }
function Link($d, $a) { New-Item -ItemType HardLink -Path "$d\$($a.FileName)" -Target "$base\$($a.FileName)" | Out-Null }
$s1 = Asset '0.3.2-spike1' 'Full'; $s2 = Asset '0.3.2-spike2' 'Full'; $s2d = Asset '0.3.2-spike2' 'Delta'

# T: truncated full package (60 MB of 115 MB), feed unchanged -> Size mismatch
$d = New-Dir 'feedT'; Link $d $s1
$in = [IO.File]::OpenRead("$base\$($s2.FileName)"); $out = [IO.File]::Create("$d\$($s2.FileName)")
$buf = New-Object byte[] (60MB); [void]$in.Read($buf, 0, $buf.Length); $out.Write($buf, 0, $buf.Length); $in.Close(); $out.Close()
Write-Feed $d @($s1, $s2)

# H: package intact, feed SHA256 wrong (and SHA1 left as is)
$d = New-Dir 'feedH'; Link $d $s1; Link $d $s2
$bad = $s2.PSObject.Copy(); $c = $bad.SHA256.ToCharArray(); $c[0] = $(if ($c[0] -eq 'A') { 'B' } else { 'A' }); $bad.SHA256 = -join $c
Write-Feed $d @($s1, $bad)

# F: one byte flipped in the middle of the package, same size, feed correct -> SHA256 mismatch
$d = New-Dir 'feedF'; Link $d $s1
Copy-Item "$base\$($s2.FileName)" "$d\$($s2.FileName)"
$fs = [IO.File]::Open("$d\$($s2.FileName)", 'Open', 'ReadWrite'); $fs.Position = 50MB; $b = $fs.ReadByte(); $fs.Position = 50MB; $fs.WriteByte([byte]($b -bxor 0xFF)); $fs.Close()
Write-Feed $d @($s1, $s2)

# D: delta flipped (same size), full intact, deltas listed -> falls back to the full?
$d = New-Dir 'feedD'; Link $d $s1; Link $d $s2
Copy-Item "$base\$($s2d.FileName)" "$d\$($s2d.FileName)"
$fs = [IO.File]::Open("$d\$($s2d.FileName)", 'Open', 'ReadWrite'); $fs.Position = 100000; $b = $fs.ReadByte(); $fs.Position = 100000; $fs.WriteByte([byte]($b -bxor 0xFF)); $fs.Close()
Write-Feed $d @($s1, $s2, $s2d)

# M: feed lists a package the server does not have (404)
$d = New-Dir 'feedM'; Link $d $s1
Write-Feed $d @($s1, $s2)

# G: feed is not JSON
$d = New-Dir 'feedG'; Link $d $s1
[IO.File]::WriteAllText("$d\releases.win.json", '<html>404 not found</html>')
Get-ChildItem C:\lab\velospike\feed? -Directory | ForEach-Object { "$($_.Name): " + ((Get-ChildItem $_.FullName | ForEach-Object { "$($_.Name)=$($_.Length)" }) -join ' ') }
