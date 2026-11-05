<#
.SYNOPSIS
  A/V sync stimulus for the lab laptop (Windows): records the screen and system audio through the capturemann CLI while a
  full-screen black/white flash and a 100 ms 1 kHz beep are produced from ONE clock, so the stimulus itself adds no latency.
.DESCRIPTION
  The beep goes out through winmm waveOut (10 ms buffers, so the start is known to a buffer); the flash is drawn when
  waveOutGetPosition reaches the beep's first sample. Event times carry a small jitter so the 30 fps frame grid averages out.
  -Idle opens and closes the device around every beep (the device is idle before each one, like a notification);
  without it a silence stream keeps the device busy. Logs per event: utc, the QPC time (ns, the host clock of the session)
  at the trigger and after the flash, to C:\lab\w6\av-log.csv. Run it in the user's session
  (win-run-in-session.ps1), volume 0.3 (C:\lab\vol.ps1) and restore the volume afterwards. Then analyse with
  scripts/testlab/av-sync-analyze.py <bundle> <av-log.csv>. Needs `$env:CAPTUREMANN_*` of the instance to use and a
  `start_recording` policy of "allow" (C:\lab\w6env.ps1 sets them and defines $cm).
#>
param([int]$Events = 20, [switch]$Idle)
$ErrorActionPreference = 'Continue'
. C:\lab\w6env.ps1
Add-Type -AssemblyName System.Windows.Forms
Add-Type -ReferencedAssemblies System.Windows.Forms,System.Drawing -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Threading; using System.Windows.Forms; using System.Diagnostics;
public static class Dpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
public class Stim {
  [StructLayout(LayoutKind.Sequential)] struct WAVEFORMATEX { public ushort wFormatTag, nChannels; public uint nSamplesPerSec, nAvgBytesPerSec; public ushort nBlockAlign, wBitsPerSample, cbSize; }
  [StructLayout(LayoutKind.Sequential)] struct WAVEHDR { public IntPtr lpData; public uint dwBufferLength, dwBytesRecorded; public IntPtr dwUser; public uint dwFlags, dwLoops; public IntPtr lpNext, reserved; }
  [StructLayout(LayoutKind.Sequential)] struct MMTIME { public uint wType; public uint u; public uint pad; }
  [DllImport("winmm.dll")] static extern int waveOutOpen(out IntPtr h, int dev, ref WAVEFORMATEX f, IntPtr cb, IntPtr inst, int flags);
  [DllImport("winmm.dll")] static extern int waveOutPrepareHeader(IntPtr h, IntPtr hdr, int size);
  [DllImport("winmm.dll")] static extern int waveOutWrite(IntPtr h, IntPtr hdr, int size);
  [DllImport("winmm.dll")] static extern int waveOutGetPosition(IntPtr h, ref MMTIME t, int size);
  [DllImport("winmm.dll")] static extern int timeBeginPeriod(int ms);
  const int N = 480, NB = 4;       // 10 ms buffers, 4 queued (40 ms)
  IntPtr dev; IntPtr[] hdr = new IntPtr[NB]; IntPtr[] data = new IntPtr[NB];
  public long written = 0;          // samples handed to the device
  public volatile bool beepPending = false; public long beepStart = -1; public volatile bool run = true;
  short[] beep; int beepPos = -1;
  public List<string> log = new List<string>();
  public Form form; public long queueMs;
  public Stim() { timeBeginPeriod(1); Open(); }
  void Open() {
    beep = new short[4800];
    for (int i = 0; i < beep.Length; i++) { double env = Math.Min(1.0, Math.Min(i / 48.0, (beep.Length - i) / 48.0)); beep[i] = (short)(26000 * env * Math.Sin(2 * Math.PI * 1000 * i / 48000.0)); }
    WAVEFORMATEX f = new WAVEFORMATEX { wFormatTag = 1, nChannels = 1, nSamplesPerSec = 48000, nAvgBytesPerSec = 96000, nBlockAlign = 2, wBitsPerSample = 16, cbSize = 0 };
    if (waveOutOpen(out dev, -1, ref f, IntPtr.Zero, IntPtr.Zero, 0) != 0) throw new Exception("waveOutOpen failed");
    written = 0; beepPos = -1; run = true;
    for (int i = 0; i < NB; i++) {
      data[i] = Marshal.AllocHGlobal(N * 2); hdr[i] = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(WAVEHDR)));
      WAVEHDR h = new WAVEHDR { lpData = data[i], dwBufferLength = (uint)(N * 2) }; Marshal.StructureToPtr(h, hdr[i], false);
      waveOutPrepareHeader(dev, hdr[i], Marshal.SizeOf(typeof(WAVEHDR)));
    }
  }
  [DllImport("winmm.dll")] static extern int waveOutReset(IntPtr h);
  [DllImport("winmm.dll")] static extern int waveOutUnprepareHeader(IntPtr h, IntPtr hdr, int size);
  [DllImport("winmm.dll")] static extern int waveOutClose(IntPtr h);
  void CloseDev() { waveOutReset(dev); for (int i = 0; i < NB; i++) waveOutUnprepareHeader(dev, hdr[i], Marshal.SizeOf(typeof(WAVEHDR))); waveOutClose(dev); }
  public Thread GoIdle(int events) { Thread t = new Thread(delegate() { try { RunIdle(events); } catch (Exception e) { log.Add("ERR " + e); } }); t.Start(); return t; }
  public void RunIdle(int events) {
    CloseDev(); Thread.Sleep(1500);   // the device is idle before every beep, like a notification or a video starting
    Stopwatch sw = Stopwatch.StartNew();
    for (int k = 0; k < events; k++) {
      while (sw.ElapsedMilliseconds < k * 2500L + 1000 + (k * 37) % 34) Thread.Sleep(1);
      Open(); beepStart = 0; beepPending = true;
      Thread ft = new Thread(Feeder); ft.Priority = ThreadPriority.Highest;
      long q0 = (long)(Stopwatch.GetTimestamp() * (1e9 / Stopwatch.Frequency));
      ft.Start();
      while (Pos() < 1) Thread.Sleep(0);
      long q1 = (long)(Stopwatch.GetTimestamp() * (1e9 / Stopwatch.Frequency));
      form.Invoke((MethodInvoker)delegate { form.BackColor = System.Drawing.Color.White; form.Refresh(); });
      long q2 = (long)(Stopwatch.GetTimestamp() * (1e9 / Stopwatch.Frequency));
      log.Add(k + "," + DateTime.UtcNow.ToString("o") + ",0,0,0," + q1 + "," + q2 + "," + q0);
      Thread.Sleep(300);
      form.Invoke((MethodInvoker)delegate { form.BackColor = System.Drawing.Color.Black; form.Refresh(); });
      Thread.Sleep(200); run = false; ft.Join(); CloseDev();
    }
  }
  void Fill(int i) {
    short[] b = new short[N];
    if (beepPending) { beepPending = false; beepPos = 0; beepStart = written; }
    if (beepPos >= 0) { for (int k = 0; k < N && beepPos < beep.Length; k++, beepPos++) b[k] = beep[beepPos]; if (beepPos >= beep.Length) beepPos = -1; }
    Marshal.Copy(b, 0, data[i], N);
    waveOutWrite(dev, hdr[i], Marshal.SizeOf(typeof(WAVEHDR)));
    written += N;
  }
  public void Feeder() {
    for (int i = 0; i < NB; i++) Fill(i);
    int cur = 0;
    while (run) {
      WAVEHDR h = (WAVEHDR)Marshal.PtrToStructure(hdr[cur], typeof(WAVEHDR));
      if ((h.dwFlags & 1) != 0) { Fill(cur); cur = (cur + 1) % NB; } else Thread.Sleep(1);
    }
  }
  long Pos() { MMTIME t = new MMTIME { wType = 2 }; waveOutGetPosition(dev, ref t, Marshal.SizeOf(typeof(MMTIME))); return t.u; }  // TIME_SAMPLES (32-bit wrap ok for minutes)
  public Thread Go(int events) { Thread t = new Thread(delegate() { try { Run(events); } catch (Exception e) { log.Add("ERR " + e); } }); t.Start(); return t; }
  public void Run(int events) {
    Thread ft = new Thread(Feeder); ft.Priority = ThreadPriority.Highest; ft.Start();
    Thread.Sleep(500);
    Stopwatch sw = Stopwatch.StartNew();
    for (int k = 0; k < events; k++) {
      while (sw.ElapsedMilliseconds < k * 2000L + 1000 + (k * 37) % 34) Thread.Sleep(1);
      beepStart = -1; beepPending = true;
      while (beepStart < 0) Thread.Sleep(1);
      long target = beepStart; long t0 = sw.ElapsedMilliseconds;
      // flash when the device has played up to the beep's first sample
      while (Pos() < target) Thread.Sleep(0);
      DateTime utc = DateTime.UtcNow; long q1 = (long)(Stopwatch.GetTimestamp() * (1e9 / Stopwatch.Frequency)); long t1 = sw.ElapsedMilliseconds;
      form.Invoke((MethodInvoker)delegate { form.BackColor = System.Drawing.Color.White; form.Refresh(); });
      long t2 = sw.ElapsedMilliseconds; long q2 = (long)(Stopwatch.GetTimestamp() * (1e9 / Stopwatch.Frequency));
      log.Add(k + "," + utc.ToString("o") + "," + t0 + "," + t1 + "," + t2 + "," + q1 + "," + q2);
      Thread.Sleep(300);
      form.Invoke((MethodInvoker)delegate { form.BackColor = System.Drawing.Color.Black; form.Refresh(); });
    }
    Thread.Sleep(500); run = false; ft.Join();
  }
}
'@
[void][Dpi]::SetProcessDPIAware()
$f = New-Object Windows.Forms.Form
$f.FormBorderStyle='None'; $f.StartPosition='Manual'; $f.Bounds=[Windows.Forms.Screen]::PrimaryScreen.Bounds; $f.BackColor='Black'; $f.TopMost=$true; $f.ShowInTaskbar=$false
$f.Show(); [Windows.Forms.Application]::DoEvents()
$s = New-Object Stim; $s.form = $f
$r = & $cm record start --mic off --camera off --system-audio --size source --fps 30 --json 2>&1 | Out-String
"record start: $(($r -replace '\s+',' ').Substring(0,[Math]::Min(200,$r.Length)))"
Start-Sleep 2
$t = if ($Idle) { $s.GoIdle($Events) } else { $s.Go($Events) }
while ($t.IsAlive) { [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 2 }
$f.Close()
$stop = & $cm record stop --json 2>&1 | Out-String
"stop: " + ($stop -replace '\s+',' ').Substring(0,[Math]::Min(300,$stop.Length))
$s.log | Set-Content C:\lab\w6\av-log.csv
"events: $($s.log.Count)"
