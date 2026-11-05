# L1 windows 0.2.7 regression report (full)

- **Date:** 2026-11-05T14:30:29Z to 2026-11-05T14:30:47Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\20261105T143027Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen --only installer.silent --installer C:\lab\capturemann-0.2.7-windows-x64-setup.exe`, tier `full`
- **Result:** 0 passed, 1 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `6de115d30da861aaece5b914eedad1ee44641fe6aaa1d3bb15e1a32d0100b943`
  - `cm-mcp.exe` `05b31fe08cdfaf21716c331d2102452e6ae660c2d4a4c4ec6436d9a17023d911`
  - `cm-daemon.exe` `5b53e1d9494b0c909c44def735591be81c359d536b74b090d4b77d311944bf0b`
  - `cm-desktop.exe` `1e8aff037be0bddc916c79242b9c6b2b7bfe443391a2164c0dc158e889595604`
  - `cm-overlay.exe` `f1ad57a6808e160b334c3bceb668cf0b0b4600046227c3f95bc67719c24ce978`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`
  - `capturemann-0.2.7-windows-x64-setup.exe` `4763088b56cd9cb3e9ec789b2060ac14ba432e5f787ec8b007b3ef9086012621`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `installer.silent` | FAIL | silent install into a temp folder, the programs run, silent uninstall leaves nothing: the Start menu entries changed: before ["CaptureMann"], after [] |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| O7 installer | FAIL | `installer.silent` |
| windows-package.yml silent install | FAIL | `installer.silent` |

## Problems found

### `installer.silent`: silent install into a temp folder, the programs run, silent uninstall leaves nothing

- the Start menu entries changed: before ["CaptureMann"], after []

Evidence:

```text
1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]
installer: exit 0 (0x00000000) in 10201 ms
installed: capturemann 0.2.7
daemon pid 14456 answered after 319 ms
uninstaller: exit 0 (0x00000000) in 245 ms
```

## Not run

- Every selected case ran.

## Evidence of passing cases


## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
