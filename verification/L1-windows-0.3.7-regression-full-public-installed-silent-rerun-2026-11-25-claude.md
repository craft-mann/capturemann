# L1 windows 0.3.7 regression report (full)

- **Date:** 2026-11-25T07:49:46Z to 2026-11-25T07:50:02Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit cc1f281c, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\20261125T074945Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen --only installer.silent --installer C:\lab\pub037\setup.exe --commit cc1f281c`, tier `full`
- **Result:** 0 passed, 1 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `47777613ae5b14c1eb0b71dcf4c274d46eb5d288d286e2d41569072260fa1e94`
  - `cm-mcp.exe` `4086522d1f51368dc2733804bc1fb0b83ca6b2faf8ea756408cea60d3d2179ba`
  - `cm-daemon.exe` `ca053fb997db50803ddc14c0df74c3af48d41ad3db30b207da4846a494739e2b`
  - `cm-desktop.exe` `e351ac4946927764be2aab17cd9a3a9ce1988f62b2069e76f55a9ec2c1041717`
  - `cm-overlay.exe` `fb717076bd6d64555746761f9063a171af9fdda24e132e3eaecd26147e19efa9`
  - `ffmpeg.exe` `75933d80ff595ac4bc8d8cdaa39f45ca8e732400899b24c1d99a21e3dc471205`
  - `ffprobe.exe` `37daffbdc19995ce7e5ec9542102ed4e77ca25415b8a7f98dce2005854f58aea`
  - `llama-server.exe` `323afb63b5879e0bc375b9c5de2aa44b471094d5bf211039f90436acbc816732`
  - `setup.exe` `5e158e64d543e565ffc704d6a6d558e8459a2e0bdef4bc197547c8e310724b63`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `installer.silent` | FAIL | silent install into a temp folder, the programs run, silent uninstall leaves nothing: the uninstaller left Start menu entries behind: ["CaptureMann (dev).lnk"] (before the run: ["CaptureMann", "CaptureMann (dev).lnk"]) |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| O7 installer | FAIL | `installer.silent` |
| windows-package.yml silent install | FAIL | `installer.silent` |

## Problems found

### `installer.silent`: silent install into a temp folder, the programs run, silent uninstall leaves nothing

- the uninstaller left Start menu entries behind: ["CaptureMann (dev).lnk"] (before the run: ["CaptureMann", "CaptureMann (dev).lnk"])

Evidence:

```text
1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]
saved before: 12 shared key(s), Start menu folder yes, user PATH yes
installer: exit 0 (0x00000000) in 9374 ms
installed: capturemann 0.3.7
daemon pid 13384 answered after 318 ms
uninstaller: exit 0 (0x00000000) in 234 ms
```

## Not run

- Every selected case ran.

## Evidence of passing cases


## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
