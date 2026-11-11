$env:DOTNET_ROOT = 'C:\lab\velospike\dotnet'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
& C:\lab\velospike\tools\vpk.exe pack -H 2>&1 | Where-Object { $_ -match '^\s+--(sign|azure|msi|shortcuts|framework|noInst|noPortable|delta|aumid|packAuthors|runtime|splash|instLocation|instWelcome|instConclusion|icon|skipVelo)' -or $_ -match '^\s{30,}' } | ForEach-Object { $_.TrimEnd() } | Select-Object -First 80
