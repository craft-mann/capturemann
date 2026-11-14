# s2_win_shots.ps1 -Theme light|dark: S2 screenshot set on the lab laptop (runs in the desktop session).
# Starts Settings, File Explorer, Edge (public page), Notepad, Calculator and some classic dialogs, captures
# each window to C:\lab\s2shots\<theme>_<name>.png, closes what it started. The theme is set for the run
# and restored at the end.
param([Parameter(Mandatory = $true)][string]$Theme)
$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class S2W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SendNotifyMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static List<KeyValuePair<IntPtr,string>> Wins() {
    var l = new List<KeyValuePair<IntPtr,string>>();
    EnumWindows((h, p) => { if (IsWindowVisible(h)) { var s = new StringBuilder(256); GetWindowText(h, s, 256);
      if (s.Length > 0) l.Add(new KeyValuePair<IntPtr,string>(h, s.ToString())); } return true; }, IntPtr.Zero);
    return l;
  }
}
'@
[void][S2W]::SetProcessDPIAware()
$out = 'C:\lab\s2shots'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$pers = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
$orig = @{ a = (Get-ItemProperty $pers).AppsUseLightTheme; s = (Get-ItemProperty $pers).SystemUsesLightTheme }
function Set-Theme([int]$apps, [int]$sys) {
  Set-ItemProperty $pers AppsUseLightTheme $apps
  Set-ItemProperty $pers SystemUsesLightTheme $sys
  [S2W]::SendNotifyMessage([IntPtr]0xffff, 0x1A, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  Start-Sleep 3
}
$v = if ($Theme -eq 'light') { 1 } else { 0 }
Set-Theme $v $v

foreach ($w in [S2W]::Wins()) { if ($w.Value -match 'System Properties|- File Explorer$|^Settings$') { [void][S2W]::PostMessage($w.Key, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) } }
Start-Sleep 2
$before = [S2W]::Wins() | ForEach-Object { $_.Key }
function Find-Win([string]$re, [bool]$any = $false) {
  for ($i = 0; $i -lt 20; $i++) {
    $w = [S2W]::Wins() | Where-Object { $_.Value -match $re -and ($any -or ($before -notcontains $_.Key)) } | Select-Object -First 1
    if ($w) { return $w.Key }
    Start-Sleep -Milliseconds 500
  }
  return [IntPtr]::Zero
}
function Shot([IntPtr]$h, [string]$name) {
  if ($h -eq [IntPtr]::Zero) { "MISSING $name"; return }
  [void][S2W]::ShowWindow($h, 9); [void][S2W]::SetForegroundWindow($h); Start-Sleep -Milliseconds 1500
  $r = New-Object S2W+RECT
  [void][S2W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16)
  $w = $r.R - $r.L; $hh = $r.B - $r.T
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
  $g.Dispose(); $bmp.Save("$out\${Theme}_$name.png", [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  "OK $name ${w}x$hh"
}
function Close-Win([IntPtr]$h) { [void][S2W]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep 1 }

# Settings pages (one window).
foreach ($p in @('ms-settings:display', 'ms-settings:personalization-colors', 'ms-settings:about', 'ms-settings:privacy-microphone', 'ms-settings:network-status')) {
  Start-Process $p; Start-Sleep 4
  $h = Find-Win "^Settings$" $true
  Shot $h ("settings_" + ($p -replace 'ms-settings:', ''))
}
$h = Find-Win "^Settings$" $true; if ($h -ne [IntPtr]::Zero) { Close-Win $h }
Get-Process SystemSettings -ErrorAction SilentlyContinue | Stop-Process -Force

# File Explorer: a system folder, a second one in the details view.
Start-Process explorer.exe 'C:\Windows'; $h = Find-Win "^Windows - File Explorer$" $true; Shot $h 'explorer_windows'; if ($h -ne [IntPtr]::Zero) { Close-Win $h }
Start-Process explorer.exe 'C:\Program Files'; $h = Find-Win "^Program Files - File Explorer$" $true; Shot $h 'explorer_programfiles'; if ($h -ne [IntPtr]::Zero) { Close-Win $h }

# Edge on a public page, in a private window with its own profile.
$edge = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
$ep = Start-Process $edge '--inprivate --user-data-dir=C:\lab\s2edge --no-first-run https://example.com' -PassThru
Start-Sleep 6; $h = Find-Win 'Example Domain'; Shot $h 'edge_example'
Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -like '*s2edge*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep 1; Remove-Item C:\lab\s2edge -Recurse -Force -ErrorAction SilentlyContinue

# Notepad with sample text.
Set-Content C:\lab\s2sample.txt "Quarterly report draft`r`n`r`nThe quick brown fox jumps over the lazy dog.`r`nRevenue grew 12 percent while costs stayed flat.`r`nNext steps: review, approve, publish.`r`n"
$np = Start-Process notepad.exe 'C:\lab\s2sample.txt' -PassThru; Start-Sleep 3
$h = Find-Win 's2sample'; Shot $h 'notepad'; if ($h -ne [IntPtr]::Zero) { Close-Win $h }
Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-Item C:\lab\s2sample.txt -Force -ErrorAction SilentlyContinue

# Calculator.
Start-Process calc.exe; Start-Sleep 3; $h = Find-Win 'Calculator'; Shot $h 'calculator'; if ($h -ne [IntPtr]::Zero) { Close-Win $h }
Get-Process CalculatorApp -ErrorAction SilentlyContinue | Stop-Process -Force

# Classic dialogs: Folder Options, System Properties, winver.
Start-Process rundll32.exe 'shell32.dll,Options_RunDLL 0'; $h = Find-Win 'File Explorer Options'; Shot $h 'dialog_folderoptions'; if ($h -ne [IntPtr]::Zero) { Close-Win $h }
Start-Process sysdm.cpl; $h = Find-Win 'System Properties'; Shot $h 'dialog_sysprops'; if ($h -ne [IntPtr]::Zero) { Close-Win $h }
Start-Process winver.exe; $h = Find-Win 'About Windows'; Shot $h 'dialog_winver'; if ($h -ne [IntPtr]::Zero) { Close-Win $h }

# An installer-style dialog (WinForms: checkbox, radio buttons, progress bar, coloured badge label).
$f = New-Object System.Windows.Forms.Form
$f.Text = 'S2 Setup'; $f.Width = 640; $f.Height = 420; $f.StartPosition = 'CenterScreen'; $f.TopMost = $true
$l = New-Object System.Windows.Forms.Label; $l.Text = 'Welcome to the Sample Setup Wizard'; $l.Font = New-Object System.Drawing.Font('Segoe UI', 14); $l.Left = 20; $l.Top = 20; $l.Width = 560; $l.Height = 36
$t = New-Object System.Windows.Forms.Label; $t.Text = 'This wizard installs the sample product on your computer. Click Next to continue.'; $t.Left = 20; $t.Top = 70; $t.Width = 580; $t.Height = 40
$c = New-Object System.Windows.Forms.CheckBox; $c.Text = 'Create a desktop shortcut'; $c.Left = 20; $c.Top = 120; $c.Width = 300; $c.Checked = $true
$r1 = New-Object System.Windows.Forms.RadioButton; $r1.Text = 'Install for all users'; $r1.Left = 20; $r1.Top = 150; $r1.Width = 300; $r1.Checked = $true
$r2 = New-Object System.Windows.Forms.RadioButton; $r2.Text = 'Install for me only'; $r2.Left = 20; $r2.Top = 180; $r2.Width = 300
$b = New-Object System.Windows.Forms.Label; $b.Text = ' NEW '; $b.BackColor = [System.Drawing.Color]::FromArgb(0, 120, 212); $b.ForeColor = [System.Drawing.Color]::White; $b.Left = 420; $b.Top = 124; $b.Width = 60; $b.Height = 22; $b.TextAlign = 'MiddleCenter'
$g = New-Object System.Windows.Forms.GroupBox; $g.Text = 'Options'; $g.Left = 400; $g.Top = 150; $g.Width = 200; $g.Height = 80
$gb = New-Object System.Windows.Forms.Label; $gb.Text = 'Disk space: 120 MB'; $gb.Left = 10; $gb.Top = 30; $gb.Width = 170; $g.Controls.Add($gb)
$p = New-Object System.Windows.Forms.ProgressBar; $p.Left = 20; $p.Top = 260; $p.Width = 580; $p.Height = 22; $p.Value = 55
$n = New-Object System.Windows.Forms.Button; $n.Text = 'Next >'; $n.Left = 420; $n.Top = 320; $n.Width = 90
$x = New-Object System.Windows.Forms.Button; $x.Text = 'Cancel'; $x.Left = 520; $x.Top = 320; $x.Width = 90
$f.Controls.AddRange(@($l, $t, $c, $r1, $r2, $b, $g, $p, $n, $x))
$f.Show(); for ($i = 0; $i -lt 10; $i++) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 200 }
$f.Activate()
$h = Find-Win 'S2 Setup'
$sw = [System.Diagnostics.Stopwatch]::StartNew()
[S2W]::SetForegroundWindow($h) | Out-Null
for ($i = 0; $i -lt 10; $i++) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 100 }
Shot $h 'dialog_setup'
$f.Close()

Set-Theme $orig.a $orig.s
Get-ChildItem $out | ForEach-Object { $_.Name + ' ' + $_.Length }
