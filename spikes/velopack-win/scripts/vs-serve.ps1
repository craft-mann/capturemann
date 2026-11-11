param([string]$Root = 'C:\lab\velospike\feed', [int]$Port = 8099, [string]$Log = 'C:\lab\velospike\serve.log')
# Scratch static server, loopback only. Stops when C:\lab\velospike\flags\stop_server exists.
$l = New-Object System.Net.HttpListener
$l.Prefixes.Add("http://127.0.0.1:$Port/")
$l.Start()
"started $Root $(Get-Date -Format o)" | Out-File $Log -Append
$task = $null
while ($l.IsListening) {
  if (Test-Path C:\lab\velospike\flags\stop_server) { break }
  $ar = $l.BeginGetContext($null, $null)
  while (-not $ar.AsyncWaitHandle.WaitOne(500)) { if (Test-Path C:\lab\velospike\flags\stop_server) { break } }
  if (-not $ar.IsCompleted) { break }
  $c = $l.EndGetContext($ar)
  $req = $c.Request; $res = $c.Response
  $rel = [Uri]::UnescapeDataString($req.Url.AbsolutePath.TrimStart('/'))
  $path = Join-Path $Root $rel
  $line = "$(Get-Date -Format o) $($req.HttpMethod) $($req.Url.PathAndQuery) UA='$($req.UserAgent)' from=$($req.RemoteEndPoint)"
  try {
    if (Test-Path $path -PathType Leaf) {
      $fi = Get-Item $path
      $res.ContentLength64 = $fi.Length
      $res.StatusCode = 200
      $fs = [IO.File]::OpenRead($path)
      try { $fs.CopyTo($res.OutputStream) } finally { $fs.Close() }
      $line += " -> 200 $($fi.Length)"
    } else { $res.StatusCode = 404; $line += ' -> 404' }
  } catch { $line += " -> ERR $($_.Exception.Message)" }
  try { $res.Close() } catch {}
  $line | Out-File $Log -Append
}
$l.Stop()
"stopped $(Get-Date -Format o)" | Out-File $Log -Append
