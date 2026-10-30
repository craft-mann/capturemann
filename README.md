<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="design/brand/logo/svg/lockup-horizontal-on-dark.svg">
    <img alt="CaptureMann" src="design/brand/logo/svg/lockup-horizontal-on-light.svg" height="56">
  </picture>
</h1>

<p align="center">
  <a href="https://www.craftmann.app/"><img alt="a CraftMann app" src="apps/desktop/assets/brand/endorsement-craftmann.svg" height="22"></a>
</p>

<p align="center">
Free, open-source screenshots, screen recording and audio recording, with local AI captions,<br>
transcripts, meeting minutes and speaker labels, and every feature available to AI agents<br>
through MCP and a CLI.
</p>

<p align="center">
<a href="https://github.com/craft-mann/capturemann/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/craft-mann/capturemann/actions/workflows/ci.yml/badge.svg"></a>
<a href="LICENSE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue"></a>
<img alt="Platform: macOS 13+" src="https://img.shields.io/badge/platform-macOS%2013%2B-lightgrey"> <img alt="Chips: Apple Silicon and Intel" src="https://img.shields.io/badge/chip-Apple%20Silicon%20%7C%20Intel-lightgrey">
</p>

<p align="center">
  <img alt="The CaptureMann library: recordings, audio and screenshots in one place, with a record button, region, window and display capture, and the AI and agent pages in the sidebar" src="docs/images/library.webp" width="900">
</p>

> **Status: 0.2.5 (macOS 13+, Apple Silicon and Intel).** Windows, then Linux follow. Most results are
> tested with scripted fakes, plus the maintainer's own use on one Mac; there are few stored hardware
> reports. **Speaker labels** are measured on public English conversation clips only (diarization error
> 17 % on clips held back, [report](verification/benchmarks/speaker-detection-2026-10-29.md)); Vietnamese
> is not measured yet. **Intel:** the build installs, opens and asks for permissions on a real Intel Mac, but
> starting a recording crashed one (issue 16); 0.2.3 and later have a fix that is not confirmed yet. What is still
> unverified is tracked in [`specs/04-delivery-plan.md`](specs/04-delivery-plan.md) under "Verification
> debt" and in [`docs/project/status.md`](docs/project/status.md).

## Features

- **Screenshots** of a display, a window or a region, with an annotation editor and Auto-redact for
  personal data and secrets (see below).
- **Screen recording** up to the display's native resolution (4K/5K) at 30 or 60 fps, with a webcam
  bubble, microphone (noise suppression), system audio and cursor telemetry. Pause, resume, and
  crash recovery that loses at most two seconds.
- **Editor**: trim, cut, split, speed, backgrounds, zoom regions, restylable cursor, 9:16 and 1:1
  reframe. Export to MP4, platform presets, GIF or WebM.
- **Audio recording**: meetings, interviews, lectures and voice notes from the microphone and,
  optionally, system audio. A compact recording bar with a level meter, markers (press `M`), a consent
  reminder, and a WAV master that survives a crash. An audio editor with a waveform, a transcript,
  trim and cut, and export to M4A or WAV with loudness levelling.
- **Local AI**, all on your Mac: Whisper transcripts and captions (SRT, VTT, TXT, or burned in) with
  Large v3 Turbo as the recommended model, a caption editor, translation to English with Whisper and to
  other languages with the built-in text model, and zoom and silence-cut suggestions. Models download
  only when you press Download.
- **Speaker labels**: who spoke when, from the voices, with rename and merge and a progress bar on long
  recordings. The two small models ship inside the app; nothing is sent anywhere. Tell the app which
  languages you speak (for example `vi, en`) so a short recording is not heard as a related language.
- **Meeting minutes and reports**: summary, minutes, one-on-one notes, interview notes, product
  recap and lecture notes written by a built-in text model (Gemma 4 E2B by default). Every item links
  to the moment it was said; export as Markdown or text. The model runs through a bundled
  `llama-server`; no cloud and no account. The picture below shows a report with sample data.
- **Library** of self-contained capture bundles, stored in a folder you choose.
- **Share**: save to local folders or Google Drive, publish to YouTube (private by default).
- **For agents**: 74 MCP tools plus resources and prompts, and an identical `capturemann` CLI. Risky
  actions (start recording, publish, delete) need an on-screen confirmation.
- **Private by design**: no telemetry, and no network traffic unless you connect a destination or
  download a model. You bring your own Google credentials; secrets live in the macOS Keychain.
- English and Vietnamese interface.

<p align="center">
  <img alt="A meeting-minutes report beside the transcript: summary, attendees, decisions and action items, each with a button that plays the moment it was said, and a waveform that marks where the items come from (sample data)" src="docs/images/meeting-report.webp" width="900">
</p>

## Find personal data before you share a screenshot

**Auto-redact** reads the text in a screenshot with the recognizer built into your Mac (Apple Vision) and
**proposes** blur rectangles over the personal data and secrets it finds. You review the proposals
(masked previews, dashed boxes) and accept them one by one or all at once; **nothing is applied
automatically**. It runs on your computer and uses no network. The same function is available to agents
(MCP `suggest_redactions`, which only proposes) and to the command line
(`capturemann annotate <id> --suggest-redactions`). The user guide has the details:
[Auto-redact](docs/user/redaction.md).

It finds **formats and checksums**, not meaning: a payment card number (Luhn check and issuer prefix), a
Social Security number in an issuable range, a routing number with a valid checksum, a private-key block, a
`PASSWORD=...` line. It does not find names, addresses or dates. The kinds, generated from the rule data
(a test fails when this table and the code differ):

<!-- redaction-kinds:start -->
| Kind | Group | What it finds | Standards that name it | On by default | Check used |
|---|---|---|---|---|---|
| `email` | Contact | Email addresses (`local@domain.tld`). | HIPAA 6 | yes | Shape only |
| `token` | Credentials and secrets | API keys, JWTs and other long random-looking strings: known provider shapes, or a mix of 24 or more letters and digits (lower confidence). | GLBA / state breach laws (credentials) | yes | Known prefix and length, or letter-and-digit mix |
| `private_key` | Credentials and secrets | PEM private-key blocks, every line from the BEGIN line to the END line. | GLBA / state breach laws (credentials) | yes | BEGIN and END lines |
| `secret_assignment` | Credentials and secrets | The value in `NAME=value` or `"name": "value"` pairs whose name looks secret (password, token, key and similar). | GLBA / state breach laws (credentials) | yes | Secret-looking name, value is not a placeholder |
| `url_secret` | Credentials and secrets | A password inside a URL, or a secret value in a URL query string. | HIPAA 14; GLBA / state breach laws (credentials) | yes | Userinfo password or secret-looking parameter name |
| `card` | Payment cards | Payment card numbers of 13 to 19 digits; masked numbers are skipped. | PCI DSS; HIPAA 10; GLBA | yes | Luhn + issuer prefix + length |
| `card_extra` | Payment cards | An expiry date or security code beside a detected card number (found by position, no label needed). | PCI DSS (sensitive authentication data) | yes | Valid month, or 3 or 4 digits, beside a detected card |
| `ssn` | Identity and tax numbers | US Social Security numbers written `NNN-NN-NNNN` or with spaces; masked forms and bare nine-digit numbers are skipped. | NIST SP 800-122; HIPAA 7; GLBA | yes | Issuable ranges (no 000, 666 or 9xx area, no 00 group, no 0000 serial) |
| `itin` | Identity and tax numbers | US Individual Taxpayer Identification Numbers (`9NN-NN-NNNN` with an ITIN group). | NIST SP 800-122; HIPAA 7; GLBA | yes | ITIN group ranges |
| `ein` | Identity and tax numbers | US Employer Identification Numbers (`NN-NNNNNNN`). | NIST SP 800-122 | yes | Valid prefix |
| `aba` | Bank accounts | US bank routing numbers (nine digits). | HIPAA 10; GLBA | yes | Checksum + prefix range |
| `iban` | Bank accounts | International bank account numbers, solid or in groups of four. | HIPAA 10; GLBA | yes | Mod 97 |
| `npi` | Health identifiers | US National Provider Identifiers (ten digits). | HIPAA 11 | no | Luhn with the 80840 prefix |
| `dea` | Health identifiers | US DEA registration numbers (two letters, seven digits). | HIPAA 11 | yes | Check digit |
| `mbi` | Health identifiers | US Medicare Beneficiary Identifiers (11 characters). | HIPAA 9 | yes | Position rules for each character |
| `passport` | Identity and tax numbers | US passport book numbers written as a letter and eight digits. | HIPAA 11; GLBA | yes | Letter + eight digits |
| `vin` | Devices and networks | Vehicle identification numbers (17 characters). | HIPAA 12 | yes | Check digit at position 9 |
| `imei` | Devices and networks | Mobile device IMEI numbers (15 digits). | HIPAA 13 | yes | Luhn |
| `ip_address` | Devices and networks | IPv4 and IPv6 addresses; loopback and unspecified addresses are skipped. | HIPAA 15 | yes | Valid address syntax |
| `mac_address` | Devices and networks | MAC addresses (six hex pairs). | HIPAA 13 | yes | Six hex pairs, one separator style |
| `phone` | Contact | Phone and fax numbers of 10 to 15 digits with separators or a leading `+`, extensions included. | HIPAA 4, 5 | yes | NANP rules for US numbers; separators or `+` required |
<!-- redaction-kinds:end -->

The "Standards that name it" column maps each kind to the public lists that mention that kind of data
(NIST SP 800-122, HIPAA Safe Harbor identifiers, PCI DSS, GLBA and state breach-notification laws). **This
is a mapping of kinds, not a compliance claim: CaptureMann is not a compliance tool, and using it does not
make a screenshot compliant with any of them.**

### How well it works (measured, synthetic data)

All numbers below come from a stored report and are measured on **synthetic screenshots made by this project**
(300 images, fixed seed 20261030, 13 screen layouts, light and dark, 1x and 2x, some degraded): 211 images from
9 layouts (`dev`, where the rules were tuned) and 89 images from 4 other layouts (`held_out`, read once after
everything was fixed). The images contain made-up data whose formats the rules know, so these figures are an
upper bound for real screenshots, not a measure of real-world accuracy. Recognizer: Apple Vision, on an Apple
M4 Pro, macOS 26.6.2. Date: 2026-10-30.

| Apple Vision, synthetic | dev: 0.2.3 (4 kinds) | dev: 0.2.5 | held-out: 0.2.3 | held-out: 0.2.5 |
|---|---|---|---|---|
| Items scored | 1,367 | 2,275 | 436 | 1,410 |
| Found (any kind) | 92.2 % | 95.3 % | 94.5 % | 98.6 % |
| Precision (proposals that were real items) | 68.8 % | 92.0 % | 69.2 % | 94.4 % |
| False alarms per image | 2.725 | 0.417 | 2.056 | 0.831 |
| False alarms per image with nothing to find | 2.000 | 0.746 | 2.074 | 0.889 |
| Leak rate (still readable after the blur) | 4.2 % (58 of 1,367) | 2.1 % (47 of 2,275) | 3.4 % (15 of 436) | 0.5 % (7 of 1,410) |

The old and new columns score different sets of items (the old detector knew 4 of the 20 default kinds), so the
"found" rows are not like for like. With perfect text instead of Apple Vision (this separates rule errors from
recognizer errors) the 0.2.5 rules find 99.2 % at 98.5 % precision on `dev` and 100 % at 95.9 % on `held_out`.
Report: [0.2.5 final run](verification/benchmarks/redaction-0.2.5-final-2026-10-30.md) (it includes the 0.2.3
detector on the same held-out layouts), with the earlier
[0.2.3 baseline](verification/benchmarks/redaction-0.2.3-baseline-2026-10-30.md) and
[interim 0.2.5 run](verification/benchmarks/redaction-0.2.5-dev-2026-10-30.md); per-kind, per-layout,
per-font-size and per-language tables and the lookalikes that raised false alarms are in them.

What the numbers say plainly. The **false-alarm target is not met**: the goal was below 0.2 false alarms per
image with nothing to find and 0.2.5 shows 0.746 (`dev`) and 0.889 (`held_out`). The generic long-token rule
(unchanged from 0.2.3 and kept on) is the main cause, for example by proposing full git hashes. `npi` is **off by
default** because only 30 % of its proposals were real. Private-key blocks are the weakest default kind with
Apple Vision (65.5 % of key lines covered, 15.5 % still readable after the blur, `dev` only: the held-out layouts
contain no keys or secret URLs). Nothing here has been checked on real screenshots yet (that is the
maintainer's manual cross-check, [`tests/manual/RED-1.md`](tests/manual/RED-1.md)).

To reproduce on your own Mac (builds the synthetic set from the seed, no network; `run` uses the system
recognizer):

```sh
cargo run -p cm-render --example redaction_bench --release --locked -- gen --out /tmp/redaction-set
cargo run -p cm-render --example redaction_bench --release --locked -- run --set /tmp/redaction-set --detector-only --split dev
cargo run -p cm-render --example redaction_bench --release --locked -- run --set /tmp/redaction-set --recognizer system --split dev
```

### What it does not do

- It finds **formats and checksums only**. Names, street addresses and dates are not found, and there are no
  label-based kinds: a CVV or an expiry date with no card number on the screen is not proposed, and a bare
  value under a "Password" or "SSN" label is not found by the label (a `NAME=value` pair with a
  secret-looking name is).
- **Text the recognizer cannot read stays.** Small, low-contrast or unusual text can be missed, which is what
  the leak rate measures. Private-key blocks are the weakest default kind with Apple Vision.
- Bare ten-digit Vietnamese mobile numbers (`0934851497`) are not found, and passwords made only of letters
  (`letmein`) are not proposed.
- Some lookalikes are proposed anyway (in the dev split: hash strings, timestamps, ten-digit ids, tracking
  numbers); that is why you review.
- There is **no Windows or Linux recognizer yet**; the Auto-redact button is hidden there.
- It is not a compliance tool. **A blurred screenshot is not proof that nothing sensitive remains**: look at
  the result before you share it.

## Install

Requires a Mac with macOS 13 or later, Apple Silicon or Intel (Apple menu > About This Mac shows the chip). There is one download per chip. **The Intel build (0.2.1 and later) is built and checked on Apple Silicon.** It installs and opens on a real Intel Mac, but starting a screen recording crashed a MacBook Pro 16" 2019 in 0.2.2 ([issue 16](https://github.com/craft-mann/capturemann/issues/16)); 0.2.3 catches the error and retries with simpler settings, which is not confirmed on that Mac yet. The speech and text models may be slow on older Intel Macs. Please report problems. The app is about 180 MB (Apple Silicon) or 190 MB (Intel).  The AI models are not in it: they download only when you press Download (about 4.4 GB for the recommended speech model, Large v3 Turbo, plus the recommended text model, Gemma 4 E2B). The two small speaker models are inside the app.

**With Homebrew:**

```sh
brew install --cask craft-mann/tap/capturemann
```

Homebrew picks the right build for your chip. This also links the `capturemann` command line into Homebrew's `bin`. Update with
`brew upgrade --cask capturemann`.

**From the release page:**

1. Download `CaptureMann-v<version>-macos-arm64.dmg` (Apple Silicon) or
   `CaptureMann-v<version>-macos-x86_64.dmg` (Intel) and its `.sha256` file from
   [Releases](https://github.com/craft-mann/capturemann/releases).
2. Check the download (in the folder with both files):
   `shasum -a 256 -c <the DMG's name>.sha256` must print `OK`.
3. Open the DMG and drag CaptureMann to Applications.

The release DMG is signed with a Developer ID certificate and notarized by Apple, so it opens
normally. A build you make yourself from source is signed locally or ad hoc and is **not
notarized**: Gatekeeper blocks it at first launch, so right-click the app and choose Open, or run
`xattr -dr com.apple.quarantine /Applications/CaptureMann.app`. Updates are manual: download the new
DMG. The `capturemann` command line is inside the app; to use it as `capturemann`, link it once:
`sudo ln -s /Applications/CaptureMann.app/Contents/MacOS/capturemann /usr/local/bin/capturemann`.
To build it yourself instead, see [building from source](docs/development/building.md). More on verifying a
download: [docs/development/releasing.md](docs/development/releasing.md#how-users-verify-a-download).

## Quick start

```sh
make run-bundle                 # builds, signs and opens target/bundle/CaptureMann.app
# the CLI and MCP server are inside the bundle, in Contents/MacOS
target/bundle/CaptureMann.app/Contents/MacOS/capturemann capture screenshot
```

Connect an AI agent over MCP (stdio):

```json
{ "mcpServers": { "capturemann": { "command": "/path/to/CaptureMann.app/Contents/MacOS/capturemann", "args": ["mcp"] } } }
```

See [`specs/02-agent-interface.md`](specs/02-agent-interface.md) for the tools, permission tiers and
the CLI.

## Build from source

Rust 1.95, Xcode command line tools and `cmake`:

```sh
cargo test --workspace --locked
make bundle
```

Full instructions, the bundled LGPL FFmpeg and macOS permissions: [`docs/`](docs/README.md).

## Documentation

- [`docs/`](docs/README.md): building, permissions, plan, design, spikes
- [`specs/`](specs/README.md): product spec, technical design, agent interface, integrations, features
- [`CONTRIBUTING.md`](CONTRIBUTING.md), [`SECURITY.md`](SECURITY.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md),
  [`CHANGELOG.md`](CHANGELOG.md)

## Contributing

Contributions are welcome, especially new storage providers, publishing platforms and translations.
Read [`CONTRIBUTING.md`](CONTRIBUTING.md) first. Security problems: please report them privately, see
[`SECURITY.md`](SECURITY.md).

## Links

CaptureMann is a [CraftMann](https://www.craftmann.app/) app.
[Contact](https://www.craftmann.app/capturemann/) ·
[Privacy policy](https://www.craftmann.app/capturemann/privacy/) ·
[Source](https://github.com/craft-mann/capturemann)

## License

Apache-2.0, see [LICENSE](LICENSE). Third-party notices: [NOTICE](NOTICE) and
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). The bundled FFmpeg is an unmodified LGPL build.

The Apache-2.0 licence covers the code. It does not grant rights to the CaptureMann or CraftMann names or logos (`design/brand`); forks should use their own name and marks.
