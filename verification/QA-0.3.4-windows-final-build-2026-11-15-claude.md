# QA 0.3.4 Windows final build and install (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11.
- **Source:** local main `7fb80d70` (`git archive HEAD`, unpacked to `C:\wt\main`).
- **Build:** `packaging\windows\build-windows.ps1` through a scheduled task (CM_NO_SPEAKER_MODELS=1, bundles in `C:\wt\bundles`, FFmpeg and llama required, release profile with LTO off, 16 codegen units, 3 jobs). Build time 20 min 29 s (18:11:05 to 18:31:34). Packaging printed one warning: the Windows third-party licence list could not be generated (needs bash and python3), so the checked-in macOS list ships. No speaker engine in this build.
- **Files** (`C:\wt\out`, kept, with `C:\wt\main`, `bundles`, `target`):
  - `capturemann-0.3.4-windows-x64.zip`, 73,357,002 bytes, SHA-256 `da7f0622c4dd54892d2952b8b35fa4379d3a0fe3b066758880e4e02274b0d7a5`
  - `capturemann-0.3.4-windows-x64-setup.exe`, 51,429,123 bytes, SHA-256 `1b3f07eb4417274e2dbcaeb000252a8f457256b934802bdc3f49f81c85660415`
- **Install:** NSIS setup run silently (`/S`, exit 0) over `C:\Users\trung\AppData\Local\Programs\CaptureMann`. `capturemann --version` prints `capturemann 0.3.4`. `cm-desktop.exe` exists, SHA-256 `993CEF34CC8A369327D3328F641CA2C48F04FF63D57E1E5F4130AE0DDECBD126`.
- **Start check:** `cm-desktop.exe` started once through a scheduled task in the logged-in session; after 15 s the process was running with Responding True. It was then stopped; no CaptureMann process is running.
- **Harness:** `HKCU\Console\%%Startup` delegation keys set for the build and removed afterwards; scheduled tasks `fbbuild` and `fbrun` removed; temporary scripts and the source archive in `C:\lab` removed.
- **Not run:** everything except the install and the one start (no QA, no `cm-regress`, no feature checks, no reboot, no window inspection beyond Responding). Files are unsigned.
