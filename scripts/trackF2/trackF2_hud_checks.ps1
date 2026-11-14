# Track F2: HUD checks Track F did not run (REC-5.AC4, AC16, AC18): focus during a drag, snap to an edge,
# drop half off the screen, drag at the edge, collapse after a drag, remembered place after a restart.
# Runs in the logged-in desktop session (scripts/trackF/trackF_insession.sh). Private library, config, pipe.
# -Inst: folder with capturemann.exe and cm-desktop.exe. -Mode hud|bubble. -Audio: audio recording instead of screen.
param([string]$Inst='C:\lab\trackF2-inst',[string]$Mode='hud',[switch]$Audio,[switch]$Remember,[string]$Tag='f2')
$ErrorActionPreference='Continue'
Start-Transcript -Path "C:\lab\trackF2-transcript-$Mode.txt" -Force | Out-Null
$root="C:\lab\trackF2-run-$Tag"; Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue; New-Item -ItemType Directory "$root\cfg" -Force | Out-Null
$env:CAPTUREMANN_LIBRARY="$root\lib"; $env:CAPTUREMANN_CONFIG_DIR="$root\cfg"; $env:CAPTUREMANN_SOCKET="\\.\pipe\cm-trackf2-$Tag"
$settings = @{ 'agents.tool_policy' = @{ start_recording = 'allow' } }
if($Mode -eq 'bubble' -and $Remember){ $settings['camera.remember_position'] = $true }
($settings | ConvertTo-Json -Depth 5) | Set-Content "$root\cfg\settings.json" -Encoding ascii
Add-Type @"
using System;using System.Runtime.InteropServices;using System.Collections.Generic;using System.Text;
public class W{
 public delegate bool P(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] public static extern bool EnumWindows(P p,IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h,out R r);
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h,ref PT p);
 [StructLayout(LayoutKind.Sequential)] public struct PT{public int x,y;}
 [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h,int i);
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern uint GetDpiForSystem();
 [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool SystemParametersInfo(uint a,uint b,out R r,uint c);
 [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint t,ref GTI g);
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,int dx,int dy,uint d,UIntPtr e);
 [StructLayout(LayoutKind.Sequential)] public struct R{public int l,t,r,b;}
 [StructLayout(LayoutKind.Sequential)] public struct GTI{public int cb;public int flags;public IntPtr active,focus,capture,menu,move,caret;public R rc;}
 public static long Fg(){return GetForegroundWindow().ToInt64();}
 public static long Focus(){GTI g=new GTI();g.cb=Marshal.SizeOf(g);GetGUIThreadInfo(0,ref g);return g.focus.ToInt64();}
 public static long Active(){GTI g=new GTI();g.cb=Marshal.SizeOf(g);GetGUIThreadInfo(0,ref g);return g.active.ToInt64();}
 public static string Work(){R r;SystemParametersInfo(0x30,0,out r,0);return r.l+","+r.t+","+r.r+","+r.b;}
 public static string Screen(){return GetSystemMetrics(0)+","+GetSystemMetrics(1);}
 public static void Focus(long h){SetForegroundWindow(new IntPtr(h));}
 public static List<string> Find(uint pid){var o=new List<string>();EnumWindows((h,l)=>{uint p;GetWindowThreadProcessId(h,out p);if(p==pid&&IsWindowVisible(h)){R r;GetWindowRect(h,out r);R c;GetClientRect(h,out c);PT q;q.x=0;q.y=0;ClientToScreen(h,ref q);int ex=GetWindowLong(h,-20);o.Add(h.ToInt64()+","+q.x+","+q.y+","+(q.x+c.r)+","+(q.y+c.b)+","+ex+","+r.l+","+r.t+","+r.r+","+r.b);}return true;},IntPtr.Zero);return o;}
 public static void Abs(int x,int y){int sw=GetSystemMetrics(0),sh=GetSystemMetrics(1);if(x<0)x=0;if(y<0)y=0;if(x>sw-1)x=sw-1;if(y>sh-1)y=sh-1;mouse_event(0x8001,(int)(x*65535L/(sw-1)),(int)(y*65535L/(sh-1)),0,UIntPtr.Zero);}
 public static void Settle(int x,int y){Abs(x,y);System.Threading.Thread.Sleep(300);Abs(x+1,y);System.Threading.Thread.Sleep(100);Abs(x,y);System.Threading.Thread.Sleep(150);}
 // Returns "fgAfterPress,focusAfterPress,fgDuring,fgAfterRelease".
 public static string Drag(int x,int y,int dx,int dy){Settle(x,y);mouse_event(2,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(300);long a=Fg(),f=Focus();for(int i=1;i<=15;i++){Abs(x+dx*i/15,y+dy*i/15);System.Threading.Thread.Sleep(40);}System.Threading.Thread.Sleep(300);long d=Fg();mouse_event(4,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(900);return a+","+f+","+d+","+Fg();}
 public static void Click(int x,int y){Settle(x,y);mouse_event(2,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(60);mouse_event(4,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(600);}
 public static void Hover(int x,int y){Settle(x,y);System.Threading.Thread.Sleep(700);}
}
"@
[void][W]::SetProcessDPIAware()
$cli="$Inst\capturemann.exe"; $desk="$Inst\cm-desktop.exe"
$wk=[W]::Work().Split(','); $work=@{l=[int]$wk[0];t=[int]$wk[1];r=[int]$wk[2];b=[int]$wk[3]}
$sc=[W]::GetDpiForSystem()/96.0; $m=[int][Math]::Round(16*$sc)
"screen $([W]::Screen()) work area $([W]::Work()) scale $sc (16 pt margin = $m px)"
$script:app=$null
$env:RUST_LOG='cm_desktop=debug,info'
function StartApp { $script:app=Start-Process $desk -PassThru -RedirectStandardError "$root\app-err-$((Get-Date).Ticks).txt" -RedirectStandardOutput "$root\app-out-$((Get-Date).Ticks).txt"; Start-Sleep 8 }
function StopApp { (& $cli record stop 2>&1) | Out-Null; Start-Sleep 3; Get-Process cm-desktop,cm-daemon,cm-overlay -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$Inst*" } | Stop-Process -Force; Start-Sleep 2 }
function StartRec { if($Audio){ (& $cli record start --audio 2>&1) | Out-String } else { (& $cli record start --countdown 0 --size 720p 2>&1) | Out-String }; Start-Sleep 5 }
function Rects{ $o=@(); foreach($s in [W]::Find($script:app.Id)){ $f=$s.Split(','); $o+=[pscustomobject]@{h=[int64]$f[0];l=[int]$f[1];t=[int]$f[2];r=[int]$f[3];b=[int]$f[4];ex=[int]$f[5];ol=[int]$f[6];ot=[int]$f[7];orr=[int]$f[8];ob=[int]$f[9]} }; return $o }
# The HUD: topmost, wide and short (bar about 1183 x 152, audio 1293 x 152, collapsed narrower); widest match.
function Hud { Rects | Where-Object { ($_.b-$_.t) -le 170 -and ($_.r-$_.l) -ge 300 -and ($_.r-$_.l) -le 1500 -and ($_.ex -band 8) } | Sort-Object { -($_.r-$_.l) } | Select-Object -First 1 }
function Bubble { Rects | Where-Object { ($_.r-$_.l) -ge 150 -and ($_.r-$_.l) -le 700 -and ($_.b-$_.t) -ge 150 -and ($_.b-$_.t) -le 800 -and (($_.b-$_.t) -ge 0.8*($_.r-$_.l)) -and (($_.ex -band 8) -ne 0) } | Sort-Object { -($_.r-$_.l)*($_.b-$_.t) } | Select-Object -First 1 }
function Desc($w){ if(-not $w){ return 'none' }; "x=$($w.l) y=$($w.t) w=$($w.r-$w.l) h=$($w.b-$w.t) right=$($w.r) bottom=$($w.b) ex=0x$('{0:X}' -f $w.ex) hwnd=$($w.h) [outer $($w.ol),$($w.ot),$($w.orr),$($w.ob)]" }
function Cfg($k){ (& $cli settings get $k 2>&1) | Out-String }
# Finds a point that drags the HUD (a press on a button never drags): tries the grid, moves the HUD back each time.
$fxs=@(0.45,0.3,0.6,0.15,0.75,0.9,0.02,0.98); $fys=@(0.75,0.6,0.9)
function Grab { # returns @{fx;fy} or $null; leaves the HUD where it was
  foreach($fy in $fys){ foreach($fx in $fxs){
    $h=Hud; if(-not $h){ return $null }
    $x=$h.l+[int](($h.r-$h.l)*$fx); $y=$h.t+[int](($h.b-$h.t)*$fy)
    $x=[Math]::Max(5,[Math]::Min($x,[int]($work.r-5)))
    $dir=$(if($h.l -lt 400){200}else{-200})
    [void][W]::Drag($x,$y,$dir,0)
    $a=Hud; if(-not $a){ return $null }
    $d=$a.l-$h.l
    if([Math]::Abs($d) -ge 30){ $x2=$x+$d; [void][W]::Drag($x2,$y,-$d,0); return @{fx=$fx;fy=$fy} }
  } }
  return $null
}
function Pt($h,$g){ @(($h.l+[int](($h.r-$h.l)*$g.fx)), ($h.t+[int](($h.b-$h.t)*$g.fy))) }
function Report($name,$ok,$text){ "RESULT $name : $(if($ok){'PASS'}else{'FAIL'}) - $text" }
function InScreen($h){ $sz=[W]::Screen().Split(','); $h.l -ge 0 -and $h.t -ge 0 -and $h.r -le [int]$sz[0] -and $h.b -le [int]$sz[1] }
function InWork($h){ $h.l -ge $work.l -and $h.t -ge $work.t -and $h.r -le $work.r -and $h.b -le $work.b }

if($Mode -eq 'hud'){
 StartApp
 # a stand-in for the recorded app: a form with a text box, kept in the foreground
 $form='Add-Type -AssemblyName System.Windows.Forms; $f=New-Object System.Windows.Forms.Form; $f.Text="f2-recorded-app"; $f.StartPosition="Manual"; $f.Location=New-Object System.Drawing.Point(300,200); $f.Size=New-Object System.Drawing.Size(500,300); $tb=New-Object System.Windows.Forms.TextBox; $tb.Dock="Fill"; $f.Controls.Add($tb); $f.Add_Shown({ $f.Activate(); $tb.Focus(); $t=New-Object System.Windows.Forms.Timer; $t.Interval=280000; $t.Add_Tick({ $f.Close() }); $t.Start() }); [void]$f.ShowDialog()'
 $np=Start-Process powershell.exe -ArgumentList '-NoProfile','-Command',$form -PassThru; Start-Sleep 4
 $formH=0; foreach($s in [W]::Find($np.Id)){ $formH=[int64]($s.Split(',')[0]) }
 $fr=@(); foreach($s2 in [W]::Find($np.Id)){ $fr=$s2.Split(',') }; [W]::Click([int]$fr[1]+100,[int]$fr[2]+60); Start-Sleep 1
 $fg0=[W]::Fg(); "recorded-app stand-in is foreground: $fg0"
 StartRec | Write-Output
 $fg1=[W]::Fg()
 Report 'hud-open-keeps-focus' ($fg0 -eq $fg1) "foreground before the recording $fg0, after the HUD opened $fg1"
 $hud=Hud; "HUD at start: $(Desc $hud)"
 if(-not $hud){ Report 'hud-found' $false 'no HUD window'; }
 else {
  $isNoAct = ($hud.ex -band 0x08000000) -ne 0
  "HUD WS_EX_NOACTIVATE: $isNoAct"
  # 1. focus during a drag on the bar
  $g=Grab; "grab point: $($g.fx),$($g.fy)"
  $fr=@(); foreach($s2 in [W]::Find($np.Id)){ $fr=$s2.Split(',') }; [W]::Click([int]$fr[1]+100,[int]$fr[2]+60); Start-Sleep 1
  $base=[W]::Fg(); "before drag: fg=$base hud=$($hud.h) (the recorded app's window must be the foreground one)"
  $hud=Hud; $p=Pt $hud $g
  $r=([W]::Drag($p[0],$p[1],-80,-40)).Split(',')
  "drag focus: fgAfterPress=$($r[0]) focusAfterPress=$($r[1]) fgDuring=$($r[2]) fgAfterRelease=$($r[3]) form=$formH"
  Report 'focus-kept-during-drag' (($r[0] -eq $base) -and ($r[2] -eq $base) -and ($r[3] -eq $base)) "foreground after press $($r[0]), during $($r[2]), after release $($r[3]), recorded app $base (HUD $($hud.h))"
  # 2. snap to the right edge: drop the HUD 25 px short of the right edge of the work area
  $hud=Hud; $p=Pt $hud $g; $dx=($work.r-25)-$hud.r
  [void][W]::Drag($p[0],$p[1],$dx,0); Start-Sleep 1
  $hud=Hud; "after drop 25 px from the right edge: $(Desc $hud)"
  Report 'snap-right' ([Math]::Abs($hud.r-($work.r-$m)) -le 2) "right edge $($hud.r), expected $($work.r-$m) (work right $($work.r), margin $m)"
  Report 'bottom-stays-in-work-area' ($hud.b -le $work.b) "bottom $($hud.b), work area bottom $($work.b) (taskbar not covered)"
  # 3. drag when at the edge: from the right-snapped place, up and left by 400 px
  $p=Pt $hud $g; $before=$hud
  [void][W]::Drag($p[0],$p[1],-400,-300); Start-Sleep 1
  $hud=Hud; "drag from the edge by (-400,-300): $(Desc $hud)"
  Report 'drag-from-edge' ([Math]::Abs($hud.l-$before.l) -ge 300) "moved by $($hud.l-$before.l) sideways"
  # 4. drop half off the screen: pull to the top-left corner of the screen (the pointer stops at 0,0)
  $p=Pt $hud $g
  [void][W]::Drag($p[0],$p[1],-($p[0]+200),-($p[1]+200)); Start-Sleep 1
  $hud=Hud; "after a drop half off the top-left: $(Desc $hud)"
  Report 'pulled-back-on-screen' ((InWork $hud) -and [Math]::Abs($hud.l-($work.l+$m)) -le 2 -and [Math]::Abs($hud.t-($work.t+$m)) -le 2) "at $($hud.l),$($hud.t), expected $($work.l+$m),$($work.t+$m)"
  # 5. drop half off the bottom
  $p=Pt $hud $g
  [void][W]::Drag($p[0],$p[1],0,([W]::Screen().Split(',')[1]-$p[1]+100)); Start-Sleep 1
  $hud=Hud; "after a drop half off the bottom: $(Desc $hud)"
  Report 'bottom-pulled-back' ((InWork $hud)) "bottom $($hud.b), work bottom $($work.b)"
  # 6. back to the top-left corner for the collapse test
  $p=Pt $hud $g
  [void][W]::Drag($p[0],$p[1],-($p[0]+200),-($p[1]+200)); Start-Sleep 1
  $hud=Hud; $topLeft=$hud; "top-left place: $(Desc $hud)"
  # 7. collapse: click along the right end until the width changes
  $w0=$hud.r-$hud.l; $cy=$hud.t+[int](($hud.b-$hud.t)*0.75); $collapsed=$null
  foreach($k in 36,48,60,72,24){
   $h=Hud; if(-not $h){ break }
   [W]::Click($h.r-$k,$cy); $a=Hud; "  click right-$k -> $(Desc $a) status: $((& $cli record status 2>&1 | Out-String) -replace '\s+',' ')"
   if($a -and ($a.r-$a.l) -lt $w0-100){ $collapsed=$a; "chevron clicked at right-$k"; break }
  }
  "after collapse: $(Desc $collapsed)"
  if($collapsed){
   Report 'collapse-keeps-left-after-drag' (([Math]::Abs($collapsed.l-$topLeft.l) -le 2) -and ([Math]::Abs($collapsed.t-$topLeft.t) -le 2) -and (InWork $collapsed)) "left $($collapsed.l) (was $($topLeft.l)), top $($collapsed.t) (was $($topLeft.t))"
   # drag the collapsed bar to the right edge
   $g2=Grab; "collapsed grab point: $($g2.fx),$($g2.fy)"
   if($g2){ $h=Hud; $p=Pt $h $g2; [void][W]::Drag($p[0],$p[1],($work.r-25)-$h.r,($work.b-25)-$h.b); Start-Sleep 1
    $h=Hud; "collapsed bar dropped at the bottom-right: $(Desc $h)"
    Report 'collapsed-snap-bottom-right' (([Math]::Abs($h.r-($work.r-$m)) -le 2) -and ([Math]::Abs($h.b-($work.b-$m)) -le 2)) "right $($h.r) bottom $($h.b), expected $($work.r-$m),$($work.b-$m)" }
   $h=Hud; $rightEdge=$h.r; $cw=$h.r-$h.l
   # expand again: click the chevron (the last control)
   $expanded=$null; $cy=$h.t+[int](($h.b-$h.t)*0.75)
   foreach($k in 36,48,60,72,24){ $h=Hud; if(-not $h){ break }; [W]::Click($h.r-$k,$cy); $a=Hud; "  click right-$k -> $(Desc $a) status: $((& $cli record status 2>&1 | Out-String) -replace '\s+',' ')"; if($a -and ($a.r-$a.l) -gt $cw+100){ $expanded=$a; break } }
   "after expand: $(Desc $expanded)"
   if($expanded){ Report 'expand-keeps-right-edge' (([Math]::Abs($expanded.r-$rightEdge) -le 2) -and (InWork $expanded)) "right $($expanded.r), was $rightEdge" }
   else { Report 'expand' $false 'the width did not grow' }
  } else { Report 'collapse' $false 'no click near the right end changed the width' }
  # 8. leave it at a known place for the restart test: top-left corner, then record where it is
  $h=Hud; $g3=Grab; if($g3){ $p=Pt $h $g3; [void][W]::Drag($p[0],$p[1],-($p[0]+200),-($p[1]+200)); Start-Sleep 1 }
  $h=Hud; $remembered=$h; "place to remember: $(Desc $h)"
  "hud_positions in settings: $(Cfg 'recording.hud_positions')"
  "hud_collapsed in settings: $(Cfg 'recording.hud_collapsed')"
  Stop-Process -Id $np.Id -Force -ErrorAction SilentlyContinue
  StopApp
  # 9. restart, record again: the HUD opens at the remembered place
  StartApp; StartRec | Out-Null
  $h=Hud; "HUD after restart: $(Desc $h)"
  if($h){ Report 'remembered-after-restart' (([Math]::Abs($h.l-$remembered.l) -le 3) -and ([Math]::Abs($h.t-$remembered.t) -le 3)) "opened at $($h.l),$($h.t), left at $($remembered.l),$($remembered.t)" }
  else { Report 'remembered-after-restart' $false 'no HUD' }
 }
 StopApp
 Get-Process powershell | Where-Object { $_.MainWindowTitle -eq 'f2-recorded-app' } | Stop-Process -Force -ErrorAction SilentlyContinue
}

if($Mode -eq 'bubble'){
 StartApp
 $dev=(& $cli devices 2>&1) | Out-String
 $camId=[regex]::Match($dev,'(?m)^\s+(\S+)\s{2}.*\(default\)').Groups[1].Value
 "camera id: $camId"
 (& $cli record start --display 65537 --countdown 0 --size 720p --camera $camId 2>&1) | Out-String | Write-Output
 Start-Sleep 12
 Rects | ForEach-Object { "win $(Desc $_)" }
 foreach($pr in (Get-Process cm-desktop,cm-overlay,cm-daemon -ErrorAction SilentlyContinue)){ "proc $($pr.Name) $($pr.Id) app=$($script:app.Id)"; foreach($s3 in [W]::Find([uint32]$pr.Id)){ "  $s3" } }
 (& $cli record status 2>&1) | Out-String | Write-Output
 Get-ChildItem $root -Filter app-err-* | ForEach-Object { "stderr $($_.Name):"; Get-Content $_.FullName -Tail 25 }
 $b=Bubble; "bubble window: $(Desc $b)"
 if($b){
  $cx=($b.l+$b.r)/2; $cy=$b.t+($b.b-$b.t)/2
  [W]::Hover([int]$cx,[int]$cy); $b=Bubble; "hovered: $(Desc $b)"
  # drag by the picture
  $r=([W]::Drag([int]$cx,[int]$cy,-250,-120)).Split(','); Start-Sleep 1
  $a=Bubble; "after drag by (-250,-120): $(Desc $a)"
  Report 'bubble-drag' (([Math]::Abs($a.t-$b.t) -ge 60) -and (InScreen $a)) "moved by $($a.l-$b.l),$($a.t-$b.t); window inside the screen: $(InScreen $a)"
  "camera.position: $(Cfg 'camera.position')"
  # toolbar: hover, then click the mirror button (index 5) and the size-L button (index 2) and read the settings
  $m0=Cfg 'camera.mirror'; $s0=Cfg 'camera.size'
  $a=Bubble; $cx=($a.l+$a.r)/2; [W]::Hover([int]$cx,[int]($a.t+($a.b-$a.t)/2))
  $a=Bubble; $ww=$a.r-$a.l; $tx=$a.l+($ww-235)/2
  # the toolbar is at the top of the window (or at the bottom when flipped): try both rows
  foreach($rowY in ($a.t+20),($a.b-20)){
   [W]::Hover([int]$cx,[int]($a.t+($a.b-$a.t)/2)); [W]::Click([int]($tx+183),[int]$rowY)
   if((Cfg 'camera.mirror') -ne $m0){ break }
  }
  $m1=Cfg 'camera.mirror'
  Report 'bubble-toolbar-mirror-click' ($m1 -ne $m0) "camera.mirror before '$($m0.Trim())' after '$($m1.Trim())'"
  foreach($rowY in ($a.t+20),($a.b-20)){
   [W]::Hover([int]$cx,[int]($a.t+($a.b-$a.t)/2)); [W]::Click([int]($tx+84),[int]$rowY)
   if((Cfg 'camera.size') -ne $s0){ break }
  }
  $s1=Cfg 'camera.size'; $a2=Bubble
  Report 'bubble-toolbar-size-click' ($s1 -ne $s0) "camera.size before '$($s0.Trim())' after '$($s1.Trim())'; window now $(Desc $a2)"
  # drag again after the resize (the window changed size)
  $cx=($a2.l+$a2.r)/2; $cy=$a2.t+($a2.b-$a2.t)/2
  [W]::Hover([int]$cx,[int]$cy)
  [void][W]::Drag([int]$cx,[int]$cy,200,100); Start-Sleep 1
  $a3=Bubble; Report 'bubble-drag-after-resize' ([Math]::Abs($a3.l-$a2.l) -ge 100) "moved by $($a3.l-$a2.l),$($a3.t-$a2.t); now $(Desc $a3)"
  # drag to the screen corner: the bubble must stay inside the recorded area
  $cx=($a3.l+$a3.r)/2; $cy=$a3.t+($a3.b-$a3.t)/2
  [void][W]::Drag([int]$cx,[int]$cy,3000,3000); Start-Sleep 1
  $a4=Bubble; "after a drop at the bottom-right corner: $(Desc $a4)"
  Report 'bubble-stays-on-screen' (InScreen $a4) "window $(Desc $a4), screen $([W]::Screen()) (the recorded area is the whole display)"
  $pos=Cfg 'camera.position'; "camera.position: $pos"
  Get-ChildItem $root -Filter app-err-* | Select-Object -First 1 | ForEach-Object { "stderr tail:"; Get-Content $_.FullName -Tail 40 }
  StopApp
  StartApp
  (& $cli record start --display 65537 --countdown 0 --size 720p --camera $camId 2>&1) | Out-Null; Start-Sleep 5
  $r2=Bubble; "bubble after restart: $(Desc $r2) (before the restart: $(Desc $a4))"
  Report 'bubble-position-remembered' ($r2 -and [Math]::Abs($r2.l-$a4.l) -le 5 -and [Math]::Abs($r2.t-$a4.t) -le 5) "now $($r2.l),$($r2.t) before $($a4.l),$($a4.t)"
 } else { Report 'bubble-found' $false 'no bubble window' }
 StopApp
}
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
"done"
Stop-Transcript | Out-Null
