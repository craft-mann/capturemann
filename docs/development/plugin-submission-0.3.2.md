# Submission pack: the Claude plugin, release 0.3.2

What the maintainer needs at hand to submit `plugins/capturemann` to Anthropic's directory from the
developer portal (https://claude.ai/directory/manage), prepared Nov 11, 2026 from the pages
`claude.com/docs/directory/publish`, `claude.com/docs/plugins/submit`,
`claude.com/docs/plugins/pre-submission-checklist`, `claude.com/docs/plugins/platform-support`,
`code.claude.com/docs/en/plugins/{overview,create,components,manifest-reference,publish,marketplace-reference,security}`
and the Anthropic Software Directory Policy (support.claude.com article 13145358). Plan item:
`specs/plans/release-0.3.2-plan.md`, section 4. The plugin's own README is `plugins/capturemann/README.md`;
the user page is `docs/user/claude-code-plugin.md`; the manual check is `tests/manual/PLUGIN-1.md`.

Nothing here has been submitted. The portal was not opened while preparing this; every field below is
what the docs say the portal asks, with the answer to give.

## How to submit, step by step (for the maintainer)

Nothing here has been done, and the portal pages were read in the docs, not opened. If the portal differs from this, trust the portal and fix this page.

1. **Get the plugin on public `main`** (after 0.3.2 is merged and pushed by you): `git push public main`. The portal reads GitHub; the repository must be public.
2. **Bump the version first** (the 0.3.2 bump of `Cargo.toml`), then `python3 scripts/check_plugin_version.py --fix` so `plugin.json` says 0.3.2, commit, push.
3. **Test the plugin yourself once** (30 minutes, `tests/manual/PLUGIN-1.md`): in a terminal `claude --plugin-dir plugins/capturemann`, run `/mcp` and see `plugin:capturemann:capturemann` connected, ask Claude to run `list_library`, and look at Settings → Agents & CLI. Save a short report in `verification/`.
4. **Check the validators locally:** `claude plugin validate --strict plugins/capturemann` and `claude plugin validate .` (both passed on Nov 11).
5. **Sign in at https://claude.ai/directory/manage** with the claude.ai account that has a paid plan (Pro, Max, Ultra, or a Team or Enterprise Owner). Connect your **GitHub account** there (it asks for it) and make sure that account can push to `craft-mann/capturemann`. The first organization that submits the folder owns the listing, so pick the right account now.
6. **Submit new → Plugin bundle.** Repository `craft-mann/capturemann`, plugin path `plugins/capturemann`, branch empty (follows `main`). Click **Validate**.
   - Fix every **Blocking** finding, push, click **Re-validate**.
   - Expected: a **hold** for a reviewer because the MCP server is the app's own binary (not a rejection). Possible block: `${user_config.capturemann_path}` in the command. If the portal says "Command path can't be followed", switch to Variant B below (one line in `.mcp.json`, delete `userConfig`), push, re-validate.
7. **Listing details:** read from `plugin.json` and the README; check them, do not retype. Decide the author name ("CraftMann" or "The CaptureMann contributors").
8. **Data handling:** four questions; use the answers in the table below.
9. **Compliance:** give a contact email, tick the acknowledgements after reading them (Directory Terms and Policy).
10. **Review and submit:** keep "GitHub push webhook" if you are admin of the repository; choose auto-publish or manual. Submit. Limit: 10 submissions per organization per 24 hours.
11. **Wait.** Status is on the Review tab (`claude.com/docs/directory/submission-status`). A **hold** is not a rejection. If the security scan rejects, use **Resubmit for review**. Do not hold the 0.3.2 release for it: the plugin installs through our own marketplace as soon as the files are pushed (`claude plugin marketplace add craft-mann/capturemann`).
12. **Each later release:** raise the plugin `version` with the app version (`check_plugin_version.py`), push; the directory picks up the tracked branch.

## Before opening the portal

| Step | State | Note |
| --- | --- | --- |
| Plugin files exist and `claude plugin validate --strict plugins/capturemann` passes | done | output in the 0.3.2 plugin work report; re-run at the release commit |
| `claude plugin validate .` (the marketplace) passes | done | same |
| `python3 scripts/check_plugin_version.py` passes after the version bump to 0.3.2 | needs the maintainer | run it after `Cargo.toml` is bumped; `--fix` writes the number |
| The plugin is on the **public** `main` of `craft-mann/capturemann` | needs the maintainer | the portal reads GitHub; a private repo can be validated but not published |
| The maintainer's GitHub account is connected on claude.ai, in the organization that will own the listing, and can push to the repository | needs the maintainer | the portal checks push access at create, submit and publish |
| A paid claude.ai plan (Pro, Max, Ultra, or Team/Enterprise Owner or a role with the Directory permission) | needs the maintainer | |
| Decide which Claude organization owns the listing | needs the maintainer | the first organization to submit a repository folder holds it; a second one is refused |
| `PLUGIN-1.md` run once against the release build and a report stored in `verification/` | needs the maintainer | the pre-submission page asks to test on each surface before submitting |
| Optional: `claude plugin eval` suite | not done | no `evals/` in the plugin; not required by the checklist |

## Portal fields, field by field

**Submit new → Plugin bundle.**

Source step:

| Field | Answer |
| --- | --- |
| Repository | `craft-mann/capturemann` (or `https://github.com/craft-mann/capturemann`) |
| Plugin path | `plugins/capturemann` |
| Branch or tag | leave empty to follow `main`, or `v0.3.2` to follow the release tag (a tag stays on its commit until the maintainer moves it; the existing release process already moves tags once, see `docs/development/release-day-0.3.1.md`). **Recommendation:** follow `main`, so a hotfix to a skill reaches users without a release; the version pin in `plugin.json` still has to be raised for users to receive it. |
| Validate | click; fix every **Blocking** finding, push, **Re-validate** |

Listing details step (read from `plugin.json` and the README; change them in the repository, not in the portal):

| Listing item | Source | Value |
| --- | --- | --- |
| Name | `plugin.json` `name` | `capturemann` |
| Display name | `displayName` | CaptureMann |
| Short description | `description` | "Screenshots, screen and audio recording, local transcripts, speaker labels and meeting notes, export and publishing, through the CaptureMann app installed on this computer." |
| Long description | `plugins/capturemann/README.md` | the README (at least 40 words outside code blocks: it has far more) |
| Icon | `icon` | `./icon.png`, 512 by 512 PNG, 24 KB, the full-bleed app icon (`design/brand/app-icon/png/windows/app-icon-512.png`). The docs give no size or shape rule; if the portal asks for another size, export it from `design/brand/app-icon/svg/app-icon.svg`. |
| Author / publisher | `author.name` | CraftMann (`author.url` https://www.craftmann.app/). **Decision needed:** "CraftMann" or "The CaptureMann contributors" (the Cargo workspace author). The portal holds a publisher name that could be confused with another; CraftMann is the maker named on the website and in the privacy policy. |
| Homepage | `homepage` | https://www.craftmann.app/capturemann/ |
| Repository | `repository` | https://github.com/craft-mann/capturemann |
| Licence | `license` and `plugins/capturemann/LICENSE` | Apache-2.0 |
| Documentation | `documentationUrl` | https://github.com/craft-mann/capturemann/blob/main/docs/user/claude-code-plugin.md (a docs page on craftmann.app does not exist yet; replace the URL in `plugin.json` if one is made) |
| Support | `supportUrl` | https://github.com/craft-mann/capturemann/issues |
| Privacy policy | `privacyPolicyUrl` | https://www.craftmann.app/capturemann/privacy/ (exists, last updated Nov 3, 2026) |
| Terms of service | `termsOfServiceUrl` | **not set: no terms of service exists** for CaptureMann (the website has none). The listing field is optional in the manifest reference; the policy requires agreeing to the Directory Terms, not publishing your own. Leave it out unless the portal marks it required, in which case a page has to be written first. |
| Keywords | `keywords` | screenshot, screen-recording, audio-recording, transcription, meeting-notes, redaction, local-ai, mcp |
| Supported surfaces | derived by the portal | expect: Claude Code and Cowork (on the user's computer) for the MCP server; chat loads the skills only. Check the portal shows this and nothing more. |

Data handling step (the portal asks four questions; answer from the facts below, in your own words):

| Question | Answer |
| --- | --- |
| Does the plugin read or store personal data? | The plugin itself stores nothing. The CaptureMann app, which the user installs separately, stores screenshots, recordings, transcripts and reports in the user's own library folder on their computer, under the user's control. A tool result (a screenshot image, a transcript) the user asks for becomes part of the Claude conversation. No account, no server, no analytics. |
| Does it send data to services other than its declared connectors? | No. The MCP server is local (`capturemann mcp`). The app sends data only to a destination the user connected inside the app (Google Drive, YouTube) and only through the `upload` and `publish_video` tools; `publish_video` requires the user's on-screen confirmation. The only other network use is a speech or text model download the user asks for (a Confirm tool). No telemetry. |
| How long is data kept? | On the user's computer until the user deletes it (the `delete_capture` tool needs the user's confirmation). The plugin keeps no data of its own. |
| Intended for people under 18? | No. |

Compliance step: the contact email (an address Anthropic can reach the maintainer at; **needs the
maintainer**: `trungtlt@outlook.com` or a project address), and four acknowledgements to tick (the
Directory Terms, the Directory Policy, and two more the portal shows; read them there).

Review and submit step: **How new versions reach the directory**: keep "GitHub push webhook" (needs
admin access to the repository to install the webhook; "Scheduled check only" works without it).
**Auto-publish passing versions**: the maintainer's choice; a reviewer publishes each version by
default until Anthropic changes the setting.

## The pre-submission checklist, item by item

From `claude.com/docs/plugins/pre-submission-checklist`. "done" means the plugin as written meets it;
"needs the maintainer" means a step only he can take; "unknown" means the docs do not say enough.

Repository and folder layout:

- [done] `plugins/capturemann/.claude-plugin/plugin.json` exists; one plugin per submission (the
  marketplace at the root lists one plugin; validate and submit the folder, not the root).
- [done] Every path stays inside the plugin folder (`./icon.png`); no `..`.
- [done] Regular files only: no symbolic links, submodules or LFS pointers in the folder.
- [needs the maintainer] No `.DS_Store` committed under `plugins/capturemann/` (check with
  `git ls-files plugins | grep -i ds_store` before each push; the repository `.gitignore` already
  ignores it, verify).
- [done] File names valid on Windows and macOS; folders on the path (`plugins`, `capturemann`) use
  letters only.
- [done] `.gitattributes` has no `export-ignore`, `export-subst` or `filter` attribute (checked Nov 11,
  2026; keep it so, or validation stops with "Couldn't validate that repository").
- [done, re-measure at the release commit] Repository under 50 MiB as GitHub archives it, under
  256 MiB unpacked, fewer than 10,000 files and folders. Measured Nov 11, 2026 on `main`:
  `git archive --format=tar.gz HEAD | wc -c` gives 29 MB, the uncompressed tar 67 MB,
  `git ls-files | wc -l` gives 3,102. The vendored whisper.cpp and sherpa sources and the design
  assets are what take the room; a release that adds large files could cross the line, and then
  validation stops ("Repository too large to validate") and the fix is a dedicated plugin repository
  (the marketplace file and the install commands change).

Manifest and plugin name:

- [done] `name` is `capturemann`: lowercase, 11 characters, not a reserved word, not an Anthropic
  marketplace name, no "claude" or "anthropic".
- [unknown] "Name is taken" if another organization already listed `capturemann` (or a look-alike):
  only the portal's Validate tells.
- [unknown] "Name matches a known brand": CaptureMann is the maker's own product, but a hold for a
  reviewer is possible for any brand-like name; expect it, it is not a rejection.
- [done] `displayName` and `author.name` in one writing system, no look-alike letters.
- [done] `description`, `author`, `version` set; `version` equals the workspace version (script).

README and licence:

- [done] README over 40 words outside code blocks, named `README.md`, in the plugin folder.
- [done] `LICENSE` in the plugin folder and `license` in `plugin.json`.

Files in the plugin folder:

- [done] Every non-image file under 256 KiB (largest is `LICENSE`, 11 KB); 10 files, under 512.
- [done] Only text, PNG and the licence; no `.ico`, `.pdf`, `.zip` or executables.
- [done] The README refers to no bundled image from a command or code block.
- [done] The MCP server is declared with `command` and `args`, not a `.mcpb` bundle.

What the plugin runs and connects to (**the important part**):

- [done] No package launcher (`npx`, `uvx`, ...), no package-manager config, no credentials, no
  `$GITHUB_TOKEN`-style environment secrets, no hooks, no scripts.
- [done] `.mcp.json` is valid JSON with one stdio entry.
- [**expect a hold or a block; decision needed**] The checklist says a local MCP server should start
  "a file in the plugin with plain arguments, such as `node ${CLAUDE_PLUGIN_ROOT}/server.js`"
  (otherwise **held for a reviewer**, "MCP server command wasn't read") and that the command must
  contain "no other variable" than `${CLAUDE_PLUGIN_ROOT}` (otherwise **Blocks**, "Command path can't
  be followed"). CaptureMann's server is the app's own binary, installed separately, so no variant
  can be "a file in the plugin": a reviewer hold is unavoidable and the README explains the design
  (the app installs the server; the plugin downloads and runs nothing of its own). The question is
  whether the portal reads `${user_config.capturemann_path}` in `command` as "another variable" and
  blocks:
  - **Variant A (what is committed):** `"command": "${user_config.capturemann_path}", "args": ["mcp"]`.
    Allowed by Claude Code (the manifest reference lists `${user_config.KEY}` as substituted in MCP
    `command`, `args` and `env`; `claude plugin validate` 2.1.311 passes it), best for macOS users
    (the binary is inside the app bundle and not on the PATH), and the plan's design.
  - **Variant B (fallback if Validate reports "Command path can't be followed"):**
    `"command": "capturemann", "args": ["mcp"]`, with no `userConfig`. Then macOS users must link the
    command (`sudo ln -s ... /usr/local/bin/capturemann`, documented in the README and the user guide)
    or add `Contents/MacOS` to their PATH. One line in `.mcp.json` and the removal of `userConfig`
    from `plugin.json`; update the README's Requirements section and `docs/user/claude-code-plugin.md`.
    Expect the "MCP server command wasn't read" hold instead of a block.
  - A third form (a shell script inside the plugin that `exec`s the binary, with the path in `env`)
    is also a hold ("Scripts the validator couldn't follow": a shell variable other than
    `${CLAUDE_PLUGIN_ROOT}`) and does not run on Windows without a POSIX shell; not recommended.
  Try A first; the portal's Validate is free and applies to one commit.
- [done] No remote server, so the `https://` URL rule does not apply.

Hooks, skills, commands, agents:

- [done] No `hooks/`. Each `SKILL.md` has YAML front matter with a single-text `description`
  (all five under 200 characters; the skill listing cap is 1,536). Folder names `skills/` and
  `SKILL.md` spelled as Claude Code expects.

Security scan (runs after submit, on every commit the directory picks up):

- [done] The README describes everything the plugin runs (the app's `capturemann mcp`), sends
  (nothing by itself; the app's destinations, with confirmation) and fetches (nothing). Readable
  source only; no compiled, packed or minified code in the folder.
- [unknown] Whether the scanner flags a plugin whose server binary it cannot inspect as "code the
  security scan can't read" (held for a reviewer). If so, the answer is the README's Privacy section
  and the open-source repository the binary is built from.

Directory Policy (support.claude.com article 13145358), the rules that touch this plugin:

- [done] Tool descriptions match the tools (they come from the server; the skills name only tools
  that exist, checked against `crates/cm-daemon/src/action/registry.rs`).
- [done] No hidden or encoded instructions; no runtime fetching of instructions; nothing reads
  Claude's memory or chat history; no advertising; no AI image, video or audio **generation** (the
  app records and transcribes; it generates text reports from the user's own recording).
- [done] A privacy policy link is given (3A). The policy says the app collects no data.
- [needs the maintainer] 3B to 3E: verified contact information, documentation, "a standard testing
  account with sample data" and "at least three working example prompts". For a local app there is
  no account; offer the reviewer the release download, the synthetic fixture
  (`cargo run -p cm-daemon --example make_fixture -- --library <dir>`) and these prompts:
  1. "Take a screenshot of my browser window with CaptureMann and blur any email addresses before
     saving it."
  2. "Record a 2-minute audio note with CaptureMann, then transcribe it and give me the summary."
  3. "Export my last CaptureMann recording as a 1080p MP4 and tell me how big the file is."
  (All three need the app and, for 2, the user's Allow click and a speech model.)
- [needs the maintainer] 3G and 3H: maintain the listing and tell Anthropic about material changes
  (a new version is picked up from `main` automatically; a change in what the plugin runs or sends
  is a material change).

## Platform support to state in the listing

From `claude.com/docs/plugins/platform-support`:

| Component | Chat (claude.ai, desktop, mobile) | Cowork | Claude Code |
| --- | --- | --- | --- |
| The five skills | load | load | load |
| The local MCP server (`capturemann mcp`) | **ignored** | loads when the session runs on the user's computer; `${user_config}` with a default is accepted (Cowork does not prompt) | loads; Claude Code prompts for the command path |

So the listing reaches chat users with skills that describe tools they do not have. The README and
the skills say so (`capturemann-basics`: "No capturemann tools in this session"). **Ask the reviewer**,
in the submission notes if there is a field for them, whether a listing whose server needs a
separately installed desktop app is accepted; the docs say local servers belong in plugin bundles
(desktop extensions are deprecated) but do not address a server the plugin does not contain.

## Screenshots and text the portal may ask for

The docs list no screenshot upload for a plugin bundle (the listing is built from `plugin.json` and
the README). Have ready, in case a field appears:

- The short description above (one sentence) and a two-sentence version: "CaptureMann is a free,
  open-source desktop app for screenshots and screen or audio recording with AI that runs on your
  computer. This plugin gives Claude its tools: capture and redact screenshots, record after you
  approve on screen, transcribe meetings and write the minutes, export and publish."
- The three example prompts above.
- Screenshots, if asked: the app's Record sheet and the editor with a transcript already exist for
  the README (`docs/images/windows/windows-record-sheet.webp`,
  `docs/images/windows/windows-editor-transcript-speakers.webp`); a screenshot of the Confirm dialog
  with the agent chip and of Settings → Agents & CLI with the plugin listed would need to be taken on
  a Mac with the 0.3.2 build (sample content only, as the README's images). None were made for this
  pack.

## After submitting

- Every commit on the tracked branch is scanned; the version in `plugin.json` must rise with each
  release or users keep the old copy (`scripts/check_plugin_version.py`).
- Statuses and who acts next: `claude.com/docs/directory/submission-status`. A **held** version is
  not a rejection. A failed security scan on a never-published plugin is a rejection with a
  **Resubmit for review** button on the Review tab.
- Up to 10 submissions per organization per 24 hours, drafts and withdrawals included.
- To list the MCP server as its own connector is not possible: that path is for **remote** servers.
- The repository stays the single source: the marketplace at the repository root keeps working for
  Claude Code users who never touch the directory.

## Risks

1. **Validation blocks on `${user_config}` in `command`** (above). Mitigation: Variant B, one small
   change, at the cost of the macOS path option.
2. **Reviewer hold on an external binary.** Unavoidable; the README and the open-source repository are
   the answer. Review time is not fixed; do not make the 0.3.2 release wait for it. The marketplace
   install works on the day the files are pushed.
3. **Repository too large to validate** (50 MiB archived, 10,000 entries). Measure first. Fallback: a
   small `craft-mann/capturemann-plugins` repository that holds `plugins/capturemann` and the
   marketplace file, with CI copying it from this repository at each release.
4. **The name is taken or held as a brand.** Expect a hold, not a block; `capturemann` is the product.
5. **Chat users install a plugin whose tools do not run in chat.** Stated in the README, the user page
   and the basics skill; a one-star review risk, not a policy one.
6. **A client-list bug in the app** (plan item 4a): before the 0.3.2 fix, a Claude Code client that
   made only Free calls may be missing from Settings → Agents & CLI, so a user cannot block it there.
   Ship the fix in the same release.
7. **Settings written by the plugin.** The plugin writes nothing to the user's settings itself; Claude
   Code stores the one option (`pluginConfigs`). `set_settings` from Claude is limited by the daemon
   to the non-app-only keys. No risk beyond the existing agent interface.
