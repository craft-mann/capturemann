# Drag probe for the recording HUD / bubble on Windows (REC-5.AC18). Runs in the logged-in session.
# -Inst: folder with capturemann.exe (and cm-desktop.exe unless -Desktop is given). -Mode screen|audio|control.
# Uses a private library, config folder and pipe; the maintainer's data is never touched.
param([string]$Inst="$env:LOCALAPPDATA\Programs\CaptureMann",[string]$Desktop='',[string]$Mode='screen',[string]$Tag='x')
$ErrorActionPreference='Continue'
if(-not $Desktop){ $Desktop="$Inst\cm-desktop.exe" }
$root="C:\lab\trackF-run-$Tag"; Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue; New-Item -ItemType Directory "$root\cfg" -Force | Out-Null
$env:CAPTUREMANN_LIBRARY="$root\lib"; $env:CAPTUREMANN_CONFIG_DIR="$root\cfg"; $env:CAPTUREMANN_SOCKET="\\.\pipe\cm-trackf-$Tag"
'{"agents.tool_policy":{"start_recording":"allow"}}' | Set-Content "$root\cfg\settings.json" -Encoding ascii
Add-Type @"
using System;using System.Runtime.InteropServices;using System.Collections.Generic;using System.Text;
public class W{
 public delegate bool P(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] public static extern bool EnumWindows(P p,IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h,int i);
 [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
 [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 public static long Fg(){return GetForegroundWindow().ToInt64();}
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,int dx,int dy,uint d,UIntPtr e);
 [StructLayout(LayoutKind.Sequential)] public struct R{public int l,t,r,b;}
 [StructLayout(LayoutKind.Sequential)] public struct POINT{public int x,y;}
 public static List<string> Find(uint pid){var o=new List<string>();EnumWindows((h,l)=>{uint p;GetWindowThreadProcessId(h,out p);if(p==pid&&IsWindowVisible(h)){R r;GetWindowRect(h,out r);int ex=GetWindowLong(h,-20);o.Add(h.ToInt64()+","+r.l+","+r.t+","+r.r+","+r.b+","+ex);}return true;},IntPtr.Zero);return o;}
 public static int Menus(){int n=0;EnumWindows((h,l)=>{var s=new StringBuilder(64);GetClassName(h,s,64);if(s.ToString()=="#32768"&&IsWindowVisible(h))n++;return true;},IntPtr.Zero);return n;}
 public static long Hit(int x,int y){POINT p;p.x=x;p.y=y;return WindowFromPoint(p).ToInt64();}
 public static void Abs(int x,int y){int sw=GetSystemMetrics(0),sh=GetSystemMetrics(1);mouse_event(0x8001,(int)(x*65535L/(sw-1)),(int)(y*65535L/(sh-1)),0,UIntPtr.Zero);}
 public static void Settle(int x,int y){Abs(x,y);System.Threading.Thread.Sleep(300);Abs(x+1,y);System.Threading.Thread.Sleep(100);Abs(x,y);System.Threading.Thread.Sleep(150);}
 public static void Drag(int x,int y,int dx,int dy){Settle(x,y);mouse_event(2,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(300);for(int i=1;i<=15;i++){Abs(x+dx*i/15,y+dy*i/15);System.Threading.Thread.Sleep(40);}System.Threading.Thread.Sleep(300);mouse_event(4,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(600);}
 public static void Click(int x,int y){Settle(x,y);mouse_event(2,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(60);mouse_event(4,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(500);}
 public static void DoubleClick(int x,int y){Settle(x,y);mouse_event(2,0,0,0,UIntPtr.Zero);mouse_event(4,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(80);mouse_event(2,0,0,0,UIntPtr.Zero);mouse_event(4,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(700);}
 public static void RightClick(int x,int y){Settle(x,y);mouse_event(8,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(60);mouse_event(16,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(700);}
}
"@
[void][W]::SetProcessDPIAware()
if($Mode -eq 'control'){
 # Control: the probe can move a window that has a real title bar (a WinForms form in its own process).
 $cmd='Add-Type -AssemblyName System.Windows.Forms; $f=New-Object System.Windows.Forms.Form; $f.Text="ctl"; $f.StartPosition="Manual"; $f.Location=New-Object System.Drawing.Point(300,300); $f.Size=New-Object System.Drawing.Size(500,300); $f.TopMost=$true; $f.Add_Shown({ $t=New-Object System.Windows.Forms.Timer; $t.Interval=25000; $t.Add_Tick({ $f.Close() }); $t.Start() }); [void]$f.ShowDialog()'
 $np=Start-Process powershell -ArgumentList '-NoProfile','-Command',$cmd -PassThru; Start-Sleep 4
 $w=@([W]::Find($np.Id))
 $f=$w[0].Split(','); $l=[int]$f[1];$t=[int]$f[2]
 [W]::Drag($l+250,$t+15,100,60)
 "control before=$($w[0]) after=$([W]::Find($np.Id)[0])"
 Stop-Process -Id $np.Id -Force -ErrorAction SilentlyContinue
 exit
}
$app=Start-Process $Desktop -PassThru
Start-Sleep 8
$cli="$Inst\capturemann.exe"
if($Mode -eq 'audio'){ (& $cli record start --audio 2>&1) | Out-String | Write-Output }
elseif($Mode -eq 'bubble'){
 $dev=(& $cli devices 2>&1) | Out-String
 $camId=[regex]::Match($dev,'(?m)^\s+(\S+)\s{2}.*\(default\)').Groups[1].Value
 "camera id: $camId"
 (& $cli record start --countdown 0 --size 720p --camera $camId 2>&1) | Out-String | Write-Output
} else { (& $cli record start --countdown 0 --size 720p 2>&1) | Out-String | Write-Output }
Start-Sleep 5
function Rects{ $o=@(); foreach($s in [W]::Find($app.Id)){ $f=$s.Split(','); $o+=[pscustomobject]@{h=[int64]$f[0];l=[int]$f[1];t=[int]$f[2];r=[int]$f[3];b=[int]$f[4];ex=[int]$f[5]} }; return $o }
$all=Rects
$all | ForEach-Object { "win $($_.h) at $($_.l),$($_.t) size $($_.r-$_.l)x$($_.b-$_.t) ex=0x$('{0:X}' -f $_.ex)" }
# The HUD is the wide, short window (the full bar is about 1200 x 152 px); the other small windows are toasts and cards.
# The webcam bubble (-Mode bubble) is the roughly square window of a few hundred px.
if($Mode -eq 'bubble'){
 $huds=@($all | Where-Object { ($_.r-$_.l) -le 700 -and ($_.b-$_.t) -le 700 -and ($_.r-$_.l) -ge 150 -and -not (($_.r-$_.l) -eq 682 -and ($_.b-$_.t) -eq 218) } | Sort-Object { -($_.r-$_.l)*($_.b-$_.t) })
 $fys=@(0.5,0.65,0.8); $fxs=@(0.3,0.5,0.7)
} else {
 $huds=@($all | Where-Object { ($_.b-$_.t) -le 170 -and ($_.r-$_.l) -ge 300 -and ($_.r-$_.l) -le 1500 } | Sort-Object { -($_.r-$_.l) })
 $fys=@(0.6,0.75,0.9); $fxs=@(0.02,0.15,0.3,0.45,0.6,0.75,0.9,0.98)
}
if($huds.Count -eq 0){ "NO HUD WINDOW" } else {
 $hud=$huds[0]; $hh=$hud.h
 "HUD window $hh"
 $moved=0; $tried=0; $grabFx=$null; $grabFy=$null; $still=@()
 foreach($fy in $fys){
  foreach($fx in $fxs){
   $cur=Rects | Where-Object { $_.h -eq $hh } | Select-Object -First 1
   if(-not $cur){ "HUD window gone"; break }
   $x=$cur.l+[int](($cur.r-$cur.l)*$fx); $y=$cur.t+[int](($cur.b-$cur.t)*$fy)
   $hit=[W]::Hit($x,$y)
   $fg0=[W]::Fg()
   [W]::Drag($x,$y,-90,-50)
   $fg1=[W]::Fg()
   if($fg0 -ne $fg1 -or $fg1 -eq $hh){ "  foreground window changed: before $fg0, after $fg1 (HUD is $hh)" }
   $after=Rects | Where-Object { $_.h -eq $hh } | Select-Object -First 1
   if(-not $after){ "HUD window gone after the drag from ($x,$y): the press ended the recording"; break }
   $dl=$after.l-$cur.l; $dt=$after.t-$cur.t; $tried++
   # A drag moves the window by about (-90,-50); the HUD also changes height by itself when a hover hint opens or closes, so only a sideways move counts.
   $isDrag = [Math]::Abs($dl) -ge 40
   if($isDrag){ $moved++; if($grabFx -eq $null){ $grabFx=$fx; $grabFy=$fy } } else { $still += "($fx,$fy)" }
   "drag from ($x,$y) fx=$fx fy=$fy hitHud=$($hit -eq $hh) moved by ($dl,$dt)"
   if($isDrag){ [W]::Drag($x+$dl,$y+$dt,-$dl,-$dt) }
  }
 }
 "SUMMARY tried=$tried moved=$moved not-moved(buttons or text)=$($still -join ' ')"
 if($grabFx -ne $null){
  $cur=Rects | Where-Object { $_.h -eq $hh } | Select-Object -First 1
  $x=$cur.l+[int](($cur.r-$cur.l)*$grabFx); $y=$cur.t+[int](($cur.b-$cur.t)*$grabFy)
  $size0="$($cur.r-$cur.l)x$($cur.b-$cur.t)"
  [W]::DoubleClick($x,$y)
  $c2=Rects | Where-Object { $_.h -eq $hh } | Select-Object -First 1
  "double-click on the grip: size before $size0, after $($c2.r-$c2.l)x$($c2.b-$c2.t) (a change would mean it maximised)"
  $m0=[W]::Menus(); [W]::RightClick($x,$y); $m1=[W]::Menus()
  "right-click on the grip: open menus before $m0, after $m1 (a system menu would mean the OS treats the grip as a title bar)"
  [W]::Click($x,$y) ; # close a menu, if any
  (& $cli record status 2>&1) | Out-String | Write-Output
 }
 # A click on a button must still reach it: press every not-moving point once and print the recording status after each.
 foreach($p in $still){
  $parts=$p.Trim('()').Split(','); $fx=[double]$parts[0]; $fy=[double]$parts[1]
  $cur=Rects | Where-Object { $_.h -eq $hh } | Select-Object -First 1
  if(-not $cur){ "HUD window gone"; break }
  $x=$cur.l+[int](($cur.r-$cur.l)*$fx); $y=$cur.t+[int](($cur.b-$cur.t)*$fy)
  [W]::Click($x,$y)
  $st=(& $cli record status 2>&1) | Out-String
  "click at fx=$fx fy=$fy -> " + ($st -replace "\s+"," ")
  if($st -match 'No recording|not recording|idle'){ break }
 }
}
(& $cli record stop 2>&1) | Out-String | Write-Output
Start-Sleep 3
Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Get-Process cm-daemon,cm-desktop,cm-overlay -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$Inst*" -or $_.Path -like "C:\wt\*" } | Stop-Process -Force
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
