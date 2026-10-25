## What and why

<!-- What does this change, and why? Link the issue and the spec IDs (for example PUB-2, EDT-3.AC2). -->

## How it was tested

<!-- "Tested with fakes", or "verified on hardware" with the report in verification/. Hardware runs are never claimed without one. -->

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `cargo deny check` (when dependencies changed)
- [ ] UI text goes through `tr!` / `tr_ctx!` / `trn!` and `cargo run -p cm-i18n -- check` passes
- [ ] New or changed user-facing action has an MCP tool and a CLI command with the same behavior
- [ ] The Status line of the touched feature in `specs/features/` is updated; IDs are not renumbered
- [ ] No secrets, no telemetry, no GPL/AGPL code
