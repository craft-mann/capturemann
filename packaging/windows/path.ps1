<#
.SYNOPSIS
  Adds a folder to, or removes it from, the PER-USER PATH (HKCU\Environment), without touching the system PATH.
  Used by the installer's optional "Add to PATH" and by the uninstaller; the portable zip ships it too.

  path.ps1 -Action add|remove -Dir <folder>

  The registry value is edited directly so that REG_EXPAND_SZ entries such as %USERPROFILE%\bin survive
  ([Environment]::SetEnvironmentVariable would turn them into plain text). The installer broadcasts
  WM_SETTINGCHANGE afterwards; from the zip, open a new terminal.
#>
param(
  [Parameter(Mandatory)][ValidateSet('add', 'remove')][string]$Action,
  [Parameter(Mandatory)][string]$Dir
)
$ErrorActionPreference = 'Stop'
$Dir = $Dir.TrimEnd('\')
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
try {
  $raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
  $parts = @($raw -split ';' | Where-Object { $_ -ne '' })
  $same = { param($p) ($p.TrimEnd('\')) -ieq $Dir }
  $has = [bool]($parts | Where-Object { & $same $_ })
  if ($Action -eq 'add' -and -not $has) { $parts += $Dir }
  elseif ($Action -eq 'remove' -and $has) { $parts = @($parts | Where-Object { -not (& $same $_) }) }
  else { exit 0 }
  $key.SetValue('Path', ($parts -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
} finally { $key.Close() }
