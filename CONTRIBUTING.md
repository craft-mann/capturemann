# Contributing to CaptureMann

CaptureMann is Apache-2.0 and open source. v0.1 ships **Google Drive** as the one cloud drive and
**YouTube** as the one publish platform. Everything else is "Coming soon", and **pull requests that add
a provider or a platform are welcome**. This page says where the code goes and what a good PR
contains. Read `CLAUDE.md` (architecture, rules and commands) and `specs/README.md` (conventions)
first. For a map of the crates, see [`docs/development/architecture.md`](docs/development/architecture.md).

## Ground rules

- **Licence.** Apache-2.0, in and out: by submitting a contribution you agree it is licensed under
  Apache-2.0 (section 5 of the licence). There is no CLA and no sign-off line to add. No GPL or AGPL code in anything we ship; `cargo deny check` must pass and a
  new dependency needs a licence on the `deny.toml` allow list.
- **No telemetry**, and no network traffic when destinations and AI keys are off.
- **Bring your own credentials.** Users create their own OAuth client (client ID, and a secret only
  where the platform needs one). Nothing secret is committed, logged or put in a config file; secrets
  live in the OS keychain through `cm-auth`'s `SecretStore`, and config files hold only key names.
- **Tests never touch the network, a real account or the real keychain.** Every provider is tested
  against an in-process fake server. Say "tested with fakes" in specs and commits, and write
  "verified" only with a stored report from a real run (see `verification/`).
- **Every user-facing action has an MCP tool and a CLI command with identical behaviour.** Connecting
  an account is the exception: it is human-only (OAuth in a browser) and never an agent action.
- **Errors use the shared codes** in `specs/README.md` (`auth_expired`, `network`,
  `platform_rejected`, ...). A revoked sign-in must become `auth_expired`, not a silent failure.

Before you open a PR:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
```

Also update the Status line of the feature you touched in `specs/features/` and the M6 row of
`docs/project/plan.md`. Keep IDs stable (never renumber `STO-n`, `PUB-n` or acceptance criteria).

## Already built, not offered yet: OneDrive and Dropbox

Both providers, their fake servers, tests and connect flows are in the repository. They are hidden in
the UI only. To offer one, add its name to `OFFERED` in `apps/desktop/src/cloud_connect.rs`. The most
useful contributions here are a **real-account run** (record what you observed in `verification/`,
using the template there) and fixes where the real service differs from the documented protocol that
the fakes encode.

## Add a cloud drive (storage provider)

1. **Provider.** In `crates/cm-storage/src/`, add `<name>.rs` implementing `StorageProvider`
   (`upload(file, target, resume, progress)`), modelled on `gdrive.rs`, `onedrive.rs` or
   `dropbox.rs`. Use the shared seams so the crate stays testable: `StorageHttp` for requests (the
   `ureq` client is `UreqHttp`), `TokenSource` for the access token, `Progress` for progress and
   cancel, `ResumeState` saved after every confirmed chunk so an interrupted upload continues from
   the last confirmed byte. Honour the provider's chunk-size rules, `Retry-After`, a 401 (ask the
   token source once more, then `auth_expired`), quota errors (`platform_rejected` with the
   provider's message), and check that the remote size equals the local size.
2. **Fake server.** Add `fake_<name>.rs` behind the `testing` feature, with fault injection (a drop
   at 40% then resume, 5xx, 429, a refused token, quota, a wrong size), as `fake_drive.rs` does, and
   tests for STO-2 AC2, AC3 and AC5.
3. **Kind and factory.** Add a variant to `Kind` in `destination.rs` (serde snake_case; old files
   must still load) and an arm in the provider factory in `crates/cm-daemon/src/storage_cloud.rs`.
4. **Sign-in.** Add a `ProviderConfig` preset in `crates/cm-auth/src/provider.rs` (authorization and
   token URLs, the smallest scopes that work) with a test.
5. **Connect flow.** Add a `ProviderInfo` row to `apps/desktop/src/cloud_connect.rs` (the guided
   setup steps, the client ID check, how the account label is fetched), a fake-based test, and list
   the name in `OFFERED`. Flow logic stays in that testable module; the UI is thin.
6. **Specs.** STO-2 Status line and any facts that changed in `specs/03-integrations.md`.

## Add a publish platform

1. **Provider.** In `crates/cm-publish/src/`, add `<name>.rs` implementing `PublishProvider`
   (`id`, `requirements() -> PlatformRules`, `publish(video, meta, resume, progress)`), modelled on
   `youtube.rs`. Validate against the platform's rules **before any network call**
   (`invalid_input` naming the field). The provider only sets the privacy the caller passes and
   never changes the privacy of an existing remote post.
2. **Fake server and tests.** `fake_<name>.rs` behind the `testing` feature, as `fake_youtube.rs`,
   covering resume, quota and rate limits, a revoked token and the platform's own restrictions
   (for example posts that stay private until the app is reviewed). Surface those as warnings in the
   result, not as failures.
3. **Daemon.** Teach `crates/cm-daemon/src/publish_ops.rs` the platform id (account lookup, the
   Confirm dialog rows, the job). The dialog must show exactly what will run: platform, account,
   title, privacy, file and that it uploads over the internet.
4. **Accounts and sign-in.** `PublishAccountStore` (`crates/cm-publish/src/accounts.rs`) and a
   `ProviderConfig` preset in `cm-auth`.
5. **UI.** The connect card in Settings → Destinations (`ui/settings/publish_card.rs`,
   `apps/desktop/src/publish_connect.rs`) and the platform's fields in the publish sheet
   (`apps/desktop/src/publish/`). See `specs/features/11-publishing.md` (PUB-3 TikTok, PUB-4 Facebook
   Pages) for what each platform needs, and `specs/03-integrations.md` for the platform rules.
6. **Specs.** The PUB-n Status line, and the MCP and CLI parity tables and tool counts if you add an
   action.

## Translate the app (UI languages)

The UI is English by default and can be shown in other languages (Settings -> General -> Language). Only the UI
is translated: not recordings, captions, titles, the CLI, MCP, logs or the macOS permission prompts. Catalogues
are plain gettext files in `locales/`; `locales/README.md` has the details.

1. Copy `locales/capturemann.pot` to `locales/<code>.po`, set the `Language` and `Plural-Forms` header lines,
   and translate with Poedit, Weblate or a text editor. Keep every `{placeholder}`; you may move it. Leave a
   message empty when unsure (the English shows) and mark a guess `#, fuzzy`.
2. Ask a maintainer to add the language to the code (`cm_i18n::Lang`, the date and number table, a font for
   scripts Geist does not cover), or do it yourself following `locales/README.md`.
3. Run `cargo run -p cm-i18n -- check` and `cargo test -p cm-i18n`: they catch a stale msgid, a changed or
   missing placeholder and an empty plural form. `-- coverage` prints your percentage.
4. Open a pull request. A native speaker reviews it before the language ships.

Developers: wrap every UI string in `tr!`, `tr_ctx!` or `trn!` (one sentence per message, placeholders instead of
concatenation, a context for short words) and run `cargo run -p cm-i18n -- extract`; CI fails on a stale
template and on new unwrapped UI strings.

## Other ways to help

- **Real-hardware and real-account runs**: the plan's *Verification debt* section lists what has only
  been tested with fakes. A stored report (`verification/`) is worth more than a patch.
- **Windows**: the code compiles there, but nothing has run on a Windows machine yet.
- **Docs and setup guides** for each provider's BYO credentials.
