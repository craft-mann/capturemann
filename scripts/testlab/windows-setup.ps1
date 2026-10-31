<#
Prepares a DEDICATED Windows 10/11 test machine (the ThinkPad T490) so that CaptureMann can be tested and
driven over ssh: OpenSSH server with key login, a power plan that never sleeps or locks, Fast Startup off
(it breaks dual boot), the clock in UTC like Linux, no BitLocker, and a helper to boot into Ubuntu next.
Run once in an ADMINISTRATOR PowerShell:

    Set-ExecutionPolicy -Scope Process Bypass
    .\windows-setup.ps1 -PubKeyFile C:\Users\<you>\id_ed25519.pub [-HostName cm-t490] [-DryRun]

Do not run it on a personal computer: it turns off the lock screen and sleep, and BitLocker.
Safe to run again. -DryRun prints what it would do and changes nothing.
UNTESTED on Windows when written (Oct 31, 2026): the first run on the ThinkPad is the test; read the output.
#>
param(
  [string]$PubKeyFile = "",
  [string]$HostName = "",
  [switch]$DryRun
)
$ErrorActionPreference = "Stop"

function Say($m) { Write-Host "`n== $m" }
function Act($desc, [scriptblock]$act) {
  if ($DryRun) { Write-Host "   [dry-run] $desc" } else { Write-Host "   $desc"; & $act }
}

$id = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not ([Security.Principal.WindowsPrincipal]$id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw "Run this in an Administrator PowerShell."
}

Say "OpenSSH server"
$cap = Get-WindowsCapability -Online -Name "OpenSSH.Server*" | Select-Object -First 1
if ($cap.State -ne "Installed") { Act "install $($cap.Name)" { Add-WindowsCapability -Online -Name $cap.Name | Out-Null } }
else { Write-Host "   already installed" }
Act "start sshd and start it at boot" { Set-Service -Name sshd -StartupType Automatic; Start-Service sshd }
Act "default shell: PowerShell 5 (change to pwsh if installed)" {
  New-ItemProperty -Path "HKLM:\SOFTWARE\OpenSSH" -Name DefaultShell `
    -Value "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -PropertyType String -Force | Out-Null
}
if (-not (Get-NetFirewallRule -Name "OpenSSH-Server-In-TCP" -ErrorAction SilentlyContinue)) {
  Act "firewall rule for port 22 (private networks only)" {
    New-NetFirewallRule -Name "OpenSSH-Server-In-TCP" -DisplayName "OpenSSH Server (sshd)" -Enabled True `
      -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 -Profile Private | Out-Null
  }
}

Say "ssh key login (administrators use a shared file)"
if ($PubKeyFile -ne "") {
  if (-not (Test-Path $PubKeyFile)) { throw "no such file: $PubKeyFile" }
  $auth = "$env:ProgramData\ssh\administrators_authorized_keys"
  $key = (Get-Content $PubKeyFile -Raw).Trim()
  $have = (Test-Path $auth) -and ((Get-Content $auth -Raw) -match [regex]::Escape($key.Split(" ")[1]))
  if (-not $have) {
    Act "add the key to $auth and fix its permissions" {
      Add-Content -Path $auth -Value $key
      icacls $auth /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F" | Out-Null
    }
  } else { Write-Host "   key already authorised" }
} else { Write-Host "   no -PubKeyFile given: ssh keys not touched" }

Say "Power: never sleep, never lock, no Fast Startup, no hibernation"
Act "power plan: High performance; monitor, disk and sleep timeouts off" {
  powercfg /setactive SCHEME_MIN 2>$null
  foreach ($t in "monitor-timeout-ac", "monitor-timeout-dc", "standby-timeout-ac", "standby-timeout-dc",
                 "disk-timeout-ac", "disk-timeout-dc", "hibernate-timeout-ac", "hibernate-timeout-dc") {
    powercfg /change $t 0
  }
}
Act "turn hibernation (and so Fast Startup) off" { powercfg /hibernate off }
Act "no lock screen, no screen saver password" {
  New-Item -Path "HKLM:\SOFTWARE\Policies\Microsoft\Windows\Personalization" -Force | Out-Null
  Set-ItemProperty -Path "HKLM:\SOFTWARE\Policies\Microsoft\Windows\Personalization" -Name NoLockScreen -Value 1 -Type DWord
  Set-ItemProperty -Path "HKCU:\Control Panel\Desktop" -Name ScreenSaveActive -Value "0"
}
Act "lid closed does nothing" { powercfg /setacvalueindex SCHEME_CURRENT SUB_BUTTONS LIDACTION 0; powercfg /setactive SCHEME_CURRENT }

Say "Clock in UTC (Linux keeps the hardware clock in UTC; avoids a wrong time after switching)"
Act "RealTimeIsUniversal = 1" {
  New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\TimeZoneInformation" -Name RealTimeIsUniversal `
    -Value 1 -PropertyType DWord -Force | Out-Null
}

Say "BitLocker / device encryption off (dual boot and image restores)"
try {
  $bl = Get-BitLockerVolume -MountPoint "C:" -ErrorAction Stop
  if ($bl.ProtectionStatus -ne "Off" -or $bl.VolumeStatus -ne "FullyDecrypted") {
    Act "decrypt C: (this takes a while and runs in the background)" { Disable-BitLocker -MountPoint "C:" | Out-Null }
  } else { Write-Host "   already off" }
} catch { Write-Host "   BitLocker cmdlets not available here: check Settings > Privacy & security > Device encryption" }

Say "Hostname"
if ($HostName -ne "") { Act "rename to $HostName (needs a reboot)" { Rename-Computer -NewName $HostName -Force } }
else { Write-Host "   kept: $env:COMPUTERNAME (use -HostName)" }

Say "Helper: boot Ubuntu next (lab-next-boot.ps1)"
$helper = @'
# lab-next-boot.ps1 [-Now]: boot Ubuntu once, then Windows again. Finds the Ubuntu firmware entry by name.
param([switch]$Now)
$fw = bcdedit /enum firmware | Out-String
$m = [regex]::Matches($fw, '(?ms)identifier\s+(\{[0-9a-f-]+\}).*?description\s+(ubuntu)', 'IgnoreCase')
if ($m.Count -eq 0) { throw "No Ubuntu entry in the firmware boot list (bcdedit /enum firmware). Install Ubuntu first." }
$guid = $m[0].Groups[1].Value
bcdedit /set "{fwbootmgr}" bootsequence $guid | Out-Null
Write-Host "next boot: Ubuntu ($guid)"
if ($Now) { Restart-Computer -Force }
'@
Act "write C:\lab\lab-next-boot.ps1" {
  New-Item -ItemType Directory -Path C:\lab -Force | Out-Null
  Set-Content -Path C:\lab\lab-next-boot.ps1 -Value $helper
}

Write-Host "`n$(if ($DryRun) { 'Dry run: nothing was changed.' } else { 'Done. Reboot once; then sign in and set auto-login (netplwiz) if you want it.' })"
