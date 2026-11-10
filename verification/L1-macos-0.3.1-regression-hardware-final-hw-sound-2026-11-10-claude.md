# L1 macos 0.3.1 regression report (hardware)

- **Date:** 2026-11-10T15:14:47Z to 2026-11-10T15:14:57Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit c92a0ac8, package folder `/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --package-dir /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261110T151446Z --who cm-regress via regress.sh --verdict PARTIAL --allow sound,screen,mic,camera --only rec.system_audio --commit c92a0ac8`, tier `hardware`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `9e185276fa4bb1ba113af2a8cf49282be7c98ff218cc4a7f446ffc8054ee05f1`
  - `cm-mcp` `4531bce6e33c3837a17585bb576b4f8754ea6f59e456f83f195209e8c8f3ea36`
  - `cm-daemon` `20de0d11f6c192e73cd91f81d67509e4248a3161dfa5049fc58648af24e1d6c8`
  - `cm-desktop` `508edf58d416e5b08b85767c271eb5f1a82146a472275f69cd0f746651075f7e`
  - `cm-overlay` `672043066e9f348362d7ca1abe36b01fe516c6e55c7a298c4bb6709d3a601a7a`
  - `ffmpeg` `d35782a34284e705231733cb6f28808e0adb9fb6b6f94768a700cd1768c318d4`
  - `ffprobe` `a011e763189cf87e43429f1085b292c21d1e3c36d0fa494b229cd291a02081e8`
  - `llama-server` `b80772d44a3913c507e5f9acb70a67ae16a9a5d55c780330381c2418e4053d32`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `rec.system_audio` | PASS | system audio contains a tone played by another process |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AUD-1 | PASS | `rec.system_audio` |
| bug class: process loopback | PASS | `rec.system_audio` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `rec.system_audio` (10222 ms): daemon pid 18449 answered after 1133 ms; started rec_01a1c606ae0b_0001; export job job_a3eb668dbea9dc99: done; system audio level: mean -10.5 dB, peak -4.4 dB (the tone is 1 kHz, 2 s, played by another process); log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
