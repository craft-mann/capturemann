# Windows 0.3.1 regression: not run, blocked by Smart App Control (Nov 10, 2026)

- Packages: Windows package run 42648505921 (success) and Windows tests run 42648553860 (success), both on main at 97ea4b8a.
- `scripts/testlab/regress.sh windows --tier standard --run 42648553860 --package-run 42648505921 --commit 97ea4b8a` at 23:45 and 00:00 (Vietnam time):
  the silent install of the new `capturemann-0.3.1-windows-x64-setup.exe` ("An Application Control policy has blocked this file") and
  `C:\lab\tests\cm-regress.exe` ("blocked by your organization's Device Guard policy") were both refused. No case ran; no report was written by the script.
- Read-only check on the laptop: `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` `VerifiedAndReputablePolicyState` = 1
  (Smart App Control on), CodeIntegrity event 3077 for the PowerShell host. Both files are `NotSigned`. Earlier runs (0.2.9, Nov 8) were not blocked,
  so Smart App Control was switched on (or left evaluation mode) since then.
- Nothing was changed on the laptop (no policy change, no bypass). To continue: the maintainer turns Smart App Control off (Windows Security, App and browser control;
  it cannot be turned on again without a reset), or the files are signed (SignPath, planned), then the run above is repeated.
- Result: **no Windows regression result for 0.3.1.** Verdict: not run.
