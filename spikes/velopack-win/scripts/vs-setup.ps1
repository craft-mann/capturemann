$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
New-Item -ItemType Directory -Force C:\lab\velospike | Out-Null
Set-Location C:\lab\velospike
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest https://dot.net/v1/dotnet-install.ps1 -OutFile dotnet-install.ps1 -UseBasicParsing
(Get-Item dotnet-install.ps1).Length
powershell -NoProfile -ExecutionPolicy Bypass -File .\dotnet-install.ps1 -Channel 8.0 -InstallDir C:\lab\velospike\dotnet -NoPath 2>&1 | Select-Object -Last 5
& C:\lab\velospike\dotnet\dotnet.exe --version
$env:DOTNET_ROOT = 'C:\lab\velospike\dotnet'
$env:DOTNET_CLI_HOME = 'C:\lab\velospike\dotnethome'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:NUGET_PACKAGES = 'C:\lab\velospike\nuget'
& C:\lab\velospike\dotnet\dotnet.exe tool install --tool-path C:\lab\velospike\tools vpk 2>&1 | Select-Object -Last 4
& C:\lab\velospike\tools\vpk.exe --help 2>&1 | Select-Object -First 3
