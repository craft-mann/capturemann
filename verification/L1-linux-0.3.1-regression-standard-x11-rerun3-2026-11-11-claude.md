# L1 linux 0.3.1 regression report (standard)

- **Date:** 2026-11-11T00:55:03Z to 2026-11-11T00:55:09Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 5e1a0d9d, package folder `/home/trung/lab/pkg-0.3.1/capturemann-0.3.1-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out /home/trung/lab/regress-out/20261111T005503Z --who cm-regress via regress.sh --verdict PARTIAL --only export.estimate,export.estimate_mcp --package-dir /home/trung/lab/pkg-0.3.1/capturemann-0.3.1-linux-x86_64/lib/capturemann --commit 5e1a0d9d`, tier `standard`
- **Result:** 2 passed, 0 failed, 0 skipped of 2 cases
- **SHA-256 of the files under test:**
  - `capturemann` `13f8eef07abddbdbeced18d24d89935f03dfd08d0a61da60846fd5b88c67929b`
  - `cm-mcp` `3d326b4d7d5c4a390d50f0bf1bbfacdf95778b9b520e01683d428b1c1e8f4cd2`
  - `cm-daemon` `e571a30b686af90148cec0c072fa56116c33e7596d672436e5cfeddfcaf8b298`
  - `cm-desktop` `3ece2fd78d2e709d87bd4d6508580e50a384d5a2a125e7fa3c5b3b75d37344cc`
  - `cm-overlay` `567bf63e28a789b1187e8fbe24a835b37a9e6063390be9115a3573c91c4bf86e`
  - `ffmpeg` `938493953ab257b6e930a3da39c5c813111a447f1eff5c162378e3b774429ba6`
  - `ffprobe` `cac1da599bda11125bb6b86baa4ae6aa2f8221fbeff519f68ae6c9fb0d6d7760`
  - `llama-server` `6b75848a8a8e358a2e0421f8de74b4b111b93acc6d327d8e0450ce6131c64727`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `export.estimate` | PASS | export --estimate prints About N MB, up to M MB with codec and pixels; a GIF has none; the refusals are the shared ones |
| `export.estimate_mcp` | PASS | the MCP tool estimate_export answers the same bound and typical size as the CLI |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT-1 | PASS | `export.estimate_mcp` |
| EXP-5.AC3 | PASS | `export.estimate`, `export.estimate_mcp` |
| EXP-5.AC4 | PASS | `export.estimate` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `export.estimate` (1146 ms): daemon pid 42508 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1203 ms): daemon pid 42753 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
