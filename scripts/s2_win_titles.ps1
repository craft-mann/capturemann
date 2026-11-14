Add-Type @'
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class S2T {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static List<string> Wins() { var l = new List<string>();
    EnumWindows((h, p) => { if (IsWindowVisible(h)) { var s = new StringBuilder(256); GetWindowText(h, s, 256); if (s.Length > 0) l.Add(s.ToString()); } return true; }, IntPtr.Zero); return l; }
}
'@
'before: ' + ([S2T]::Wins() -join ' | ')
Start-Process 'ms-settings:display'; Start-Sleep 5
Start-Process explorer.exe 'C:\Windows'; Start-Sleep 4
'after: ' + ([S2T]::Wins() -join ' | ')
