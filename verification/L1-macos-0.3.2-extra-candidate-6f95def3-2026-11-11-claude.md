# L1 macOS 0.3.2 DMG checks and extra checks, candidate files of commit 6f95def3 (Nov 11, 2026)

- **Verdict:** PARTIAL (scripted run by an agent, no person looked; no screen, sound, microphone, camera or model used)
- **Files:** `CaptureMann-0.3.2-arm64.dmg` and `CaptureMann-0.3.2-x86_64.dmg`, built from commit 6f95def3 (code identical to 77af6b04) with `make bundle-release` and `packaging/macos/dmg.sh`, Developer ID Application: TRUNG TRAN (DJ2ALRFN66), hardened runtime, secure timestamp. **Signed, NOT notarized, NOT stapled** (no notarization credentials in the session; `notarize.sh --check-env` says so).
- SHA-256: arm64 `862dd40dd419b05d2310aa40c1eb82d05a8e5f4a550b2a79b6da52d54a4f3a00`, x86_64 `f4dc8905b3d9036eb2a245fc44b08718ddebe7eec74098978d7b4aa2a7ab5c0c`.
- Apps were copied from the mounted DMGs into a scratch folder (never /Applications, never target/bundle).

| Check | arm64 | x86_64 |
| --- | --- | --- |
| `hdiutil verify` | valid | valid |
| `codesign --verify --deep --strict` | valid on disk, satisfies its Designated Requirement | same |
| Hardened runtime (`flags=0x10000(runtime)`) | app and all 7 Mach-O files | same |
| Entitlements | `device.audio-input` and `device.camera` only (as `CaptureMann.entitlements`) | same |
| Designated requirement | `identifier "dev.capturemann.app" and anchor apple generic and ... leaf[subject.OU] = DJ2ALRFN66` | same |
| `spctl --assess --type execute` (app) | rejected, `source=Unnotarized Developer ID`, origin TRUNG TRAN (DJ2ALRFN66) | same |
| `spctl --assess --type open --context context:primary-signature` (DMG) | rejected, `source=Unnotarized Developer ID` | same |
| Helpers signed (capturemann, cm-desktop, cm-mcp, cm-overlay, ffmpeg, ffprobe, llama-server) | all Developer ID, hardened | same |
| Run | `capturemann --version` = 0.3.2, `ffmpeg -version` 9.0.2, `llama-server --version` ok, `cm-mcp` usage | `file` = Mach-O x86_64; under Rosetta `capturemann --version` = 0.3.2, ffmpeg runs |
| App size | 187 MB | 195 MB |
| `packaging/macos/verify-release.sh --arch <a> <dmg>` | FAILED 4 errors, all notarization: Gatekeeper rejects DMG and app, `stapler validate` fails for DMG and app. All other checks ok (identity, hardened runtime, timestamps, single arch, LGPL FFmpeg, llama-server, speaker model hashes, licences, version 0.3.2, LSMinimumSystemVersion 13.0, vtool targets) | same |

## Extra 0.3.2 checks (arm64 app, fresh release `cm-daemon` of this commit, private library, config and socket under a scratch folder, removed afterwards; the bundle was hand-written around the committed `tests/data/regress/redact-text.png`)

| Check | Result |
| --- | --- |
| `capturemann text <id>` | exit 0; "Owner: jane.doe@example.com" and "Server address 203.0.113.45 is down" (exact) |
| `capturemann text <id> --lines --json` | `recognizer: "apple_vision"`, 7 words, 2 lines with boxes, confidence 1.0, no warnings, `truncated: false` |
| `capturemann diagnostics` | `text recognizer: apple_vision (available)` |
| `capturemann mcp` stdio | `initialize` answered (protocol 2025-06-18, serverInfo capturemann 0.3.2); `tools/list` returned 81 tools, `recognize_text` among them |

Not checked: Gatekeeper acceptance after notarization, a real Intel Mac, a screenshot from the real screen, a language other than the Mac's.
