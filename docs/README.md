# Documentation

Where things live. Files in the repository root (README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY,
SUPPORT, CHANGELOG, LICENSE, NOTICE) are the standard project files; everything else is below.

## Using CaptureMann

| Document | What it is |
|---|---|
| [User guide](user/user-guide.md) | Screenshots, recording, editing, captions, publishing, agents |
| [Auto-redact](user/redaction.md) | Find personal data and secrets in a screenshot, kinds, limits |
| [Connect Google Drive and YouTube](user/google-setup.md) | Create your own Google client and connect |
| [macOS permissions](user/macos-permissions.md) | Screen Recording, signing identity, keychain prompts |

## Developing CaptureMann

| Document | What it is |
|---|---|
| [Contributing](../CONTRIBUTING.md) | Ground rules and what a good PR contains |
| [Architecture overview](development/architecture.md) | The crates, how the clients reach the action bus, where to look |
| [Building from source](development/building.md) | Toolchain, tests, the macOS bundle, FFmpeg |
| [Auto-redact rules and benchmark](development/redaction.md) | Add a rule or a recognizer, run the benchmark, the generated tables |
| [Releasing](development/releasing.md) | Cut a release: tag, workflow, signing secrets, notarization, rollback |
| [Design surfaces](development/design-surfaces.md) | Product surfaces mapped to the design kit in [`design/`](../design/CLAUDE.md) (cited in specs as "DESIGN §n") |
| [Writing documentation](development/documentation.md) | Where a new document goes, naming, links |
| [Translations](../locales/README.md) | How to add a string or a language |
| [`CLAUDE.md`](../CLAUDE.md) | Architecture, commands and constraints, for AI coding tools and people |

## Specifications and tests

| Location | What it is |
|---|---|
| [Specs](../specs/README.md) | Product spec, technical design, agent interface, integrations, per-feature behavior and acceptance criteria, and the plans behind larger features |
| [Manual tests](../tests/manual/README.md) | Hands-on scripts for what automation cannot check |
| [Verification reports](../verification/README.md) | Stored reports of manual runs (the only basis for the word "verified") |
| [Quality reports](../reports/README.md) | Test results, dependency checks, security scan and SBOM for a commit |

## Project history

| Document | What it is |
|---|---|
| [Status](project/status.md) | What is built and how each part was checked |
| [Plan](project/plan.md) | Milestones and what is open |
| [Spikes](project/spikes.md) | Feasibility spike results |
| [Demo script](project/demo-script.md) | Script and shot list for the 0.1.0 demo video |
