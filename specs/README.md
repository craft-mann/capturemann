# CaptureMann specs

Implementation-ready specs for CaptureMann v0.1 and the next release: 70 requirements and 330 acceptance criteria in 15 feature files (recounted Oct 26, 2026, after adding the voice and meetings specs; recount when you add or remove criteria). The top-level documents hold the why and the system design; `features/` holds the exact behavior a coding agent builds and tests against. When two documents disagree, raise it; don't guess. These files are exported from the claude.ai doc "CaptureMann — Product & Technical Spec v0.1" (Oct 19, 2026); "tab" in the text means the matching file below.

## Documents

| File | Tab in the doc | Contents |
| --- | --- | --- |
| `00-product-spec.md` | Product spec | Decisions, goals, personas, FR summary table, recording quality, key flow, NFRs |
| `01-technical-design.md` | Technical design | Architecture, recording pipeline, data model, export/render, jobs/secrets/OAuth, crates |
| `02-agent-interface.md` | Agent interface | Permission tiers, MCP tools, resources, prompts, events, CLI, JSON-RPC |
| `03-integrations.md` | Integrations | BYO auth model, storage and publishing API details, sources |
| `04-delivery-plan.md` | Delivery plan | Milestones M0–M7 with build status, v0.1 acceptance checklist, risks, open questions |
| `plans/design-implementation.md` | — | Traceability from design-kit screens and components to spec IDs and build status; decided design questions (OD-1 to OD-11) and the open ones (OD-12, OD-13) |
| `platform-acceptance.md` | — | Per requirement: what changes on Windows and Linux, the plan stage that builds it, how it is checked (IDs and ACs unchanged) |
| `../docs/development/design-surfaces.md` | — | Product-side design contract (surfaces mapped to the kit in `../design/`), referenced as "DESIGN §n" |

## Plans

Plans for larger features and the design-kit traceability table live in `plans/`. They record the approach and decisions; the behavior and acceptance criteria stay in the feature files.

| File | Contents |
| --- | --- |
| `plans/design-implementation.md` | Traceability from design-kit screens to spec IDs and build status; design decisions (OD-n) |
| `plans/i18n-plan.md` | UI languages: approach, phases, decisions, what was built |
| `plans/caption-editor-plan.md` | The caption editor (kit board R3c) |
| `plans/webcam-audio-check-plan.md` | Settings → Webcam & audio check (kit board R13b) |
| `plans/voice-and-meetings-plan.md` | Audio-only recording, speaker labels, text model and AI reports: release split, model choices, spikes S7 and S8 |

## Feature files

| File | Area | IDs |
| --- | --- | --- |
| `features/01-screenshot.md` | Screenshot | CAP-1 … CAP-6 |
| `features/02-annotation.md` | Annotation | ANN-1 … ANN-4 |
| `features/03-recording.md` | Recording | REC-1 … REC-9 |
| `features/04-webcam.md` | Webcam | CAM-1 … CAM-5 |
| `features/05-audio.md` | Audio | AUD-1 … AUD-4 |
| `features/06-editing.md` | Editing | EDT-1 … EDT-5 |
| `features/07-ai.md` | AI | AI-1 … AI-9 |
| `features/08-export.md` | Export | EXP-1 … EXP-4 |
| `features/09-library.md` | Library | LIB-1 … LIB-3 |
| `features/10-storage.md` | Storage | STO-1 … STO-5 |
| `features/11-publishing.md` | Publishing | PUB-1 … PUB-7 |
| `features/12-agent.md` | Agent | AGT-1 … AGT-3 |
| `features/13-launcher.md` | Launcher | LNC-1 |
| `features/14-permissions.md` | Permissions | PRM-1 |
| `features/15-voice.md` | Voice and meetings | VOC-1 … VOC-3 |
| `features/17-redaction.md` | Redaction (Auto-redact kinds, benchmark, generated docs) | RED-1 … RED-3 |

Related: `../design/` (brand and UI kit, source of truth for visuals; start at `../design/CLAUDE.md`), `../docs/development/design-surfaces.md` (surfaces mapped to the kit), `plans/design-implementation.md` (kit traceability and build status), `../docs/project/spikes.md` (feasibility results).

## Conventions

- **IDs** are stable: `AREA-n`. Never renumber; retire an ID by marking it *Withdrawn*. Acceptance criteria are `AREA-n.ACm` (e.g. `REC-2.AC3`) so tests and commits can cite them; they are append-only (a new criterion gets the next number, an old one is revised in place with a dated note, never renumbered or reordered).
- **Priority**: **P0** blocks v0.1; **P1** ships in v0.1 unless cut at the week-20 scope review; **P2** is planned for v0.2+ and specified as a *Draft* (story, summary, key criteria) to be completed before work starts.
- **Status**: `Ready` (meets the Definition of Ready and **not started**), `Draft`, `In progress`, `Done` (only when every AC is verified), `Withdrawn`. A partly built feature is `In progress (...)`, and the parentheses name what is built, on which platform, and what is missing or unverified; never leave a built feature as `Ready`. A result that only ran with scripted fakes is "tested with fakes", not "verified"; "verified on hardware (as reported)" is for a run whose output was not stored; do not claim hardware verification without a stored artifact or report.
- **Words**: *must* = required for the criterion to pass; *should* = expected unless there's a recorded reason; *may* = optional.
- **Units**: sizes in logical px unless marked "physical px"; times in ms or `MediaTime` ticks (120,000 per second); data sizes in MB = 10^6 bytes.
- **Platforms**: every criterion applies to macOS 13+ and Windows 10 22H2+ unless it names one.
- **Agent parity**: every user-facing action lists its MCP tool and CLI command. The tool and CLI behave identically to the GUI, including undo and confirmations.

## Template

```markdown
### AREA-n Title
**Priority** P0 · **Status** Ready · **Crates** cm-… · **Depends on** AREA-m, NFR-k

**User story** — As a <persona>, I want <capability>, so that <outcome>.

**Description** — What the feature does, inputs, states, defaults, limits.

**Acceptance criteria**
- **AC1** Given <state>, when <action>, then <observable result>.
- **AC2** …

**Edge cases and errors** — Each case with the required behavior and user-facing message.

**Agent interface** — MCP tool(s), CLI command, events emitted, confirmation tier.

**Out of scope** — What this feature deliberately does not do.

**Test notes** — Unit / integration / golden / manual checks, fixtures, platforms.
```

## Definition of Ready (before a feature starts)

1. Story, description and every acceptance criterion are testable without asking the author.
2. Dependencies are Done or stubbed behind a trait.
3. Agent interface (tool name, inputs, outputs, tier) is fixed.
4. Any UI follows a named surface in `docs/development/design-surfaces.md` (which maps it to a kit screen in `design/`), or a sketch is attached.

## Definition of Done (before a feature is marked Done)

1. Every AC has an automated test, or a manual test script in `tests/manual/<ID>.md` when automation is impossible (e.g. OS permission prompts).
2. Works on macOS and Windows from a release build.
3. MCP tool and CLI command implemented, documented, and covered by an integration test.
4. Undo works for every mutating action; revisions record `user` or `agent:<name>`.
5. Errors use the shared error codes (below) and readable messages; no panics in UI callbacks.
6. `cargo clippy -D warnings`, `cargo fmt`, license check and tests pass in CI.
7. Docs updated (user docs page or CLI help).

## Shared error codes

Used by the GUI, MCP (`error.data.code`), JSON-RPC and CLI exit codes.

| Code | CLI exit | Meaning |
| --- | --- | --- |
| `invalid_input` | 2 | Bad or missing parameter; message names the field |
| `not_found` | 2 | Capture, target, device, job or account id doesn't exist |
| `user_declined` | 3 | User refused a confirmation |
| `confirmation_timeout` | 3 | No answer within 5 minutes |
| `permission_missing` | 4 | OS permission absent; `data.permission` = `screen` · `camera` · `microphone` · `accessibility` · `input_monitoring` |
| `device_busy` | 5 | Camera or mic in exclusive use by another app |
| `disk_full` | 5 | Below the free-space floor (NFR-7) |
| `network` | 5 | Transient network failure (retryable) |
| `platform_rejected` | 5 | Platform API refused; `data.platform_code` carries the platform's error |
| `auth_expired` | 5 | Token revoked or expired; connection needs reconnect |
| `job_failed` | 6 | Background job failed; `data.job_id`, `data.reason` |
| `busy` | 6 | Conflicting operation running (e.g. a second recording) |
| `internal` | 1 | Unexpected internal, I/O or database failure not covered by another code |
| `daemon_unavailable` | 5 | Client cannot reach the daemon (not running, socket refused); start it and retry |
| `not_implemented` | 1 | Feature or platform backend not built yet (an action with no engine in this build, or a Windows backend that is still a stub). JSON-RPC `-32099`. Exit 1 is shared with `internal` on purpose: both mean "not your input, not a retry"; read the code to tell them apart |

**Codes outside the table.** `method_not_found` and `invalid_request` are protocol errors of the JSON-RPC transport, not shared codes: the CLI maps any code outside the table to exit 1. `not_implemented` is a shared code (decided Oct 21, 2026, product maintainer): the daemon answers it (`-32099`, `error.data.code: "not_implemented"`) when an action is known but no engine implements it (for example `publish_video` and `delete_capture` today), and the platform stubs (the Windows encoder and capture, and the system-audio, camera and cursor recorders on unsupported systems) return it directly. Its payload is `{ "code": "not_implemented", "message": "<what is not built>", "data": null }`. `cm-core`'s `ErrorCode::NotImplemented` and `CmError::not_implemented(message)` carry it, so the table has 15 codes.


## Glossary

| Term | Meaning |
| --- | --- |
| Capture | One screenshot (`.cmshot`) or recording (`.cmrec`) bundle in the library |
| Bundle | Folder with the original media, manifest and sidecar files (see Technical design → Data model) |
| Master | The original recorded tracks; never modified after recording |
| Edit list | `manifest.edits`: trims, cuts, speed, layout, zooms applied at preview and export |
| Export | A rendered file made from master + edit list with a preset |
| Destination | Where files are stored: local folder, external drive, NAS, Google Drive, OneDrive, Dropbox |
| Platform | Where videos are published: YouTube, TikTok, Facebook Pages |
| Action | One user or agent intent on the action bus; one undo step |
| Job | Persisted background work (export, transcribe, upload, publish) |
| `MediaTime` | i64 ticks, 120,000 per second, on the recording's session clock |
| Confirm tier | Actions that require an on-screen confirmation even when an agent calls them |
