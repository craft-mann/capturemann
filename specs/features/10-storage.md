# Storage (STO)

Destinations share one `StorageProvider` trait (Technical design → Crates and stack). Uploads run as persisted, resumable jobs (`cm-jobs`). Credentials and tokens live only in the OS keychain (NFR-10). Platform API details: Integrations tab → Storage.

### STO-1 Local folders, external drives and NAS
**Priority** P0 · **Status** In progress (Oct 23, 2026: the `cm-storage` crate has the `StorageProvider` trait and `LocalProvider` (temp name, size and SHA-256 check, atomic rename, ` (2)` naming, folder patterns that cannot leave the root, cancel without a partial file), tested with temp dirs, AC1 and the offline error of AC2; the daemon actions `list_destinations` and `upload` with the upload job (it waits while the destination is offline, AC2), the MCP tools and `capturemann destinations` / `upload`, wired in the app and the headless daemon, tested with fakes; Settings → Destinations has the local-folder flow (list with path, Default pill and online or offline state, add a folder with the system picker, edit name, folder pattern and what to copy, make default, remove with a question first; the rules are in `apps/desktop/src/destinations.rs`, tested with temp dirs; the screens are not seen by eye; the per-destination "Upload automatically" choice (off, after recording, after export) is real, see STO-3); not built: a UNC run (AC3), disk-full on a real full volume) · **Crates** cm-storage · **Depends on** LIB-2

**User story** — As a creator, I want copies of my exports saved to a folder, drive or NAS automatically, so that my work is backed up where I keep my media.

**Description**
- A **destination** has: name, kind (`local`), path, folder pattern (default `CaptureMann/{yyyy}/{MM}/{title}`), what to copy (export / original bundle / both), auto on stop (on/off).
- Copy writes to a temp name, verifies size and SHA-256, then renames atomically; never overwrites (adds ` (2)`).
- NAS: any mounted path (macOS `/Volumes/…`, Windows mapped drive or UNC `\\server\share`); CaptureMann doesn't mount shares itself in v0.1.

**Acceptance criteria**
- **AC1** Given a local destination with auto-copy of exports, when an export finishes, then the file exists at the pattern path and its hash matches.
- **AC2** Given the destination path is offline, then the job waits as "Destination offline" and completes when the path returns (checked every 30 s and on volume-mount events).
- **AC3** Given a UNC path, then copying works and partial files never appear under the final name.

**Edge cases and errors** — Destination full → `disk_full` on that job only; permission denied → job fails with the OS message.

**Agent interface** — MCP `list_destinations` (Free); `upload { capture_id | file, destination_id, folder? }` → `{ job_id }` (Free).

**Out of scope** — Mounting SMB/NFS shares, two-way sync.

**Test notes** — Integration tests with temp dirs and a simulated offline volume; manual script `tests/manual/STO-1.md` (external drive, NAS, UNC, disk full).

---

### STO-2 Google Drive, OneDrive and Dropbox
> Scope change (owner decision, Oct 23, 2026): v0.1 ships ONE cloud drive, Google Drive, and ONE publish platform, YouTube. OneDrive, Dropbox, TikTok and Facebook Pages are "Coming soon" in the app and move to a later release, where user feedback or community contributions (the project is open source; see CONTRIBUTING.md) decide which come next. For STO-2 that means: the acceptance criteria are checked for Google Drive in v0.1; the OneDrive and Dropbox providers and connect flows stay in the code, tested with fakes, and their rows say "Coming soon" (`cloud_connect::OFFERED`).

**Priority** P0 · **Status** In progress (First real-account run on Oct 23, 2026, verified on hardware (as reported, `verification/STO-2-PUB-2-gdrive-youtube-2026-10-23-owner.md`): connecting Google Drive and one upload after a recording work; AC2 to AC5 and the logged-out link were not run; Oct 23, 2026, all tested with fakes, none seen by eye or run against a real account: Google Drive, OneDrive and Dropbox providers against in-process fake servers (resumable chunks, resume from the last confirmed chunk (AC3), `auth_expired` after a refused token (AC5), quota as `platform_rejected`, 429 `Retry-After`, size check (AC2), test connection, share link; OneDrive chunks are multiples of 327,680 bytes (AC6), simple `PUT` up to 4 MB, `nextExpectedRanges` resume; Dropbox upload session with `incorrect_offset` recovery, full-Dropbox or app-folder root); the `upload` job picks the provider by destination kind (`google_drive`, `onedrive`, `dropbox`); the Google Drive connect flow in `apps/desktop/src/cloud_connect.rs` (guided BYO setup with the Google Cloud Console steps, client ID and secret into the keychain, system browser sign-in with PKCE, the account email from Drive `about`, the "CaptureMann" folder, the destination written; AC1 against the fake Drive), Test connection, Reconnect (AC5: the row shows it, and signing in again calls `JobQueue::resume_auth_expired` so paused uploads continue), Disconnect (deletes the keychain secrets, revokes nothing remotely), the manage panel; `KeychainTokens` in `cm-daemon` adapts `TokenManager` (with `force_refresh` after a 401) and is the token source in the app and the headless daemon. The OneDrive and Dropbox connect flows are built the same way (Oct 23, 2026, tested with fakes, not seen by eye, never run against a real account): OneDrive: guided Microsoft Entra setup (public client, loopback redirect `http://127.0.0.1`, `Files.ReadWrite` and `offline_access`, no secret), the account label from Graph `/me` (mail or user principal name; the drive owner when Graph refuses the profile), a personal or work account note from `driveType`, the "CaptureMann" folder, link modes anyone, organization and none; Dropbox: guided setup for a scoped app (PKCE, no secret, the permissions to tick), the choice of full Dropbox or App folder kept in `CloudAccount.app_folder`, the account email from `users/get_current_account`; both: client credentials in the keychain, Test connection, Reconnect (calls `resume_auth_expired` as for Drive), Disconnect (keychain secrets deleted, nothing revoked remotely), the manage panel, and no secret left behind by a connect that fails after the sign-in. Oct 23, 2026, tested with fakes: an original-bundle copy on Google Drive claims a fresh folder (`Title.cmrec`, else `Title (2).cmrec`, ...) the way the local provider does: it checks the siblings, creates the folder, checks again that it is the oldest folder of that name (the loser of a race deletes its own and takes the next name), and saves the folder id in the job checkpoint before the first file, so a restart or a retry goes on in the same folder; the upload record keeps the id (`folder_id`) and the path; `StorageProvider` gained `claim_bundle_folder`, `reuse_bundle_folder` and `delete` (additive, defaults: no claim, `not_implemented`), built for Google Drive (`files.delete`, permanent, a file that is gone counts as deleted) and the local folder (the recorded file only, never outside the destination root, symbolic links resolved; a bundle copy loses only the files of the bundle and then its empty folders); OneDrive and Dropbox keep the defaults; a bundle that is edited while it uploads is brought up to date (see STO-5), temp files of atomic writes are skipped; not built: the claim for OneDrive and Dropbox, a run against real Google, Microsoft and Dropbox accounts (whether Entra accepts the redirect `http://127.0.0.1:<port>/callback` against a registered `http://127.0.0.1` is unconfirmed) (the fakes encode the documented protocols, not observed behaviour), browsing a OneDrive or Dropbox folder in the app) · **Crates** cm-storage, cm-auth · **Depends on** STO-1, PUB-1 (shared BYO setup flow)

**User story** — As a teacher, I want to connect my Google Drive, OneDrive or Dropbox and pick a folder, so that recordings land where my school already shares files.

**Description**
- **Settings UI** (kit `Settings` → Destinations, DESIGN §5.8): the "Save to" list shows one row per destination: Local folder (shows its real path, a Default pill, Manage) and Google Drive, OneDrive, Dropbox ("Not connected" with Connect, or the account email with Manage). Additional local folders, external drives and NAS paths (STO-1) are added from Manage on the Local folder row; the kit does not show that flow. The list of three cloud providers is confirmed by STO-2, resolving the kit's "assumed" note.
- **Connect** (Settings → Destinations → Connect on a provider row): choose provider → guided BYO setup screen (paste client ID; secret only where required) → system browser OAuth with PKCE and loopback redirect → account email shown → choose folder.
- Scopes: Drive `drive.file` (app sees only its own files; default folder "CaptureMann" created at connect, sub-folders created by the app); OneDrive `Files.ReadWrite` + `offline_access`; Dropbox `files.content.write` + `sharing.write`, plus the read scopes the provider calls need: `account_info.read` (the account email at connect), `files.metadata.read` (`get_metadata` when a finish is retried) and `sharing.read` (`list_shared_links`). The guided setup lists them for the user to tick.
- Upload: provider resumable protocol (Drive resumable session, chunks multiples of 256 KiB, default 8 MiB; OneDrive upload session, chunks multiples of 320 KiB under 60 MiB, default 10 MiB; Dropbox upload session, chunks ≤ 150 MB, default 16 MiB). Resume state saved after every chunk.
- Multiple accounts per provider allowed; each destination is one account + folder.
- "Test connection" button uploads and deletes a 1 KB file.

**Acceptance criteria**
- **AC1** Given a fresh install, when the user connects Google Drive with their client ID, then the account email shows and a "CaptureMann" folder exists in their Drive.
- **AC2** Given a 2 GB export, when uploading, then progress, speed and ETA show and the remote file's size equals the local size.
- **AC3** Given the network drops at 40%, when it returns, then the upload resumes from the last confirmed chunk (not from 0).
- **AC4** Given the app restarts mid-upload, then the job resumes after launch.
- **AC5** Given a revoked token, then the destination shows "Reconnect" and the job waits as `auth_expired` instead of failing silently.
- **AC6** Given OneDrive, then every chunk except the last is a multiple of 327,680 bytes.

**Edge cases and errors** — Quota exceeded on the cloud → `platform_rejected` with the provider's message; provider rate limits (429) → back off per `Retry-After`.

**Agent interface** — Connecting accounts is human-only (OAuth in a browser); agents use `list_destinations` and `upload`. CLI `capturemann upload <id> --to gdrive:/Lessons`.

**Out of scope** — iCloud Drive, Box, S3-compatible storage (P2; the trait allows them).

**Test notes** — Mock servers implementing each resumable protocol (including failures) for CI; weekly live test with sandbox accounts; manual script `tests/manual/STO-2.md` (Google Drive).

---

### STO-3 Auto-upload on stop, resumable, survives restart
**Priority** P0 · **Status** In progress (Oct 23, 2026: the provider-agnostic part is built and tested with fakes, not seen by eye: the per-destination mode `auto_upload` (`off`, `on_stop`, `on_export`; old files with `auto_on_stop` still load, `true` as `on_stop`); the daemon queues the `upload` job at once when a recording stops normally (AC1) and, for `on_export`, behind every `export` job it queues (AC2); idempotent per capture, destination and `what`; held while recording like any `upload` job (AC3); the queue policy of the `upload` type (AC4, plus `auth_expired`); Settings → Destinations has the three-way choice. `retry_job` is built (action, MCP tool, `capturemann jobs retry`; AC4 tested with fakes: a failed job is retried and completes). The notifications are built (tested with fakes, not seen by eye): a tracker in the app (`recording/uploads.rs`) follows `upload.queued`, `job.progress`, `job.done`, `job.failed` and the new `job.paused` (published when an upload job waits with `pause_reason: "auth_expired"`), and shows "Uploaded to <destination>" with "Copy link" and "Open" (the link, or the file in the file manager for a local folder), "Upload to <destination> failed" with the reason and "Retry" (`retry_job`), and one "Reconnect needed for <destination>" toast per destination that opens Settings → Destinations; the recording details page lists running uploads next to Exports. Oct 23, 2026: the details page also lists the finished copies (destination, what, size, date) next to Exports. Not built: a sleep/wake run on a machine; manual script `tests/manual/STO-3.md` written, not run) · **Crates** cm-jobs, cm-storage · **Depends on** STO-1, STO-2, EXP-4

**User story** — As a teacher, I want my lesson to upload by itself after I stop recording, so that I don't have to remember.

**Description**
- Per destination: **Auto upload** off / on stop / on export. "On stop" uploads the chosen item (STO-5) after the recording closes; if the item is an export, the default export preset is rendered first (job dependency).
- Uploads never start while recording; a running upload pauses automatically when a recording starts if bandwidth is limited (setting "Pause uploads while recording", key `storage.pause_uploads_while_recording`, default `true`). The recording engine holds the `upload` job type just before the tracks start and releases the hold on stop, discard, a failed or cancelled start or a crash of the engine; held jobs resume from their checkpoint. With the setting `false`, uploads are not held. The `export` and `transcribe` types are always held during a recording (EXP-4.AC2), whatever this setting says. Restarting a take keeps the holds across the gap. Built and tested with fakes in `cm-daemon` and `cm-jobs`.
- **Built behavior (Oct 23, 2026).** `on_stop`: at a normal stop (never a discard, a cancelled countdown or the 60 fps self-test) each destination with `on_stop` gets an `upload` job of its `what` (inputs `auto: true`, `trigger`), published as `upload.queued`. When `what` includes the export the job depends on the export job of that stop; if the automatic export is off, the default `quick` export is queued anyway for it; with no exporter or no FFmpeg nothing is queued and `upload.skipped { reason }` is published. `on_export`: every `export` job queued by `export_video` or by the automatic export gets an upload job that depends on it. If the export fails, the queue fails the dependent upload with the reason `dependency <job id> failed` (the "skipped with a note" of AC2; `retry` on the export brings it back). Never two active uploads for the same capture, destination and `what`.
- Concurrency: 2 uploads at once; per-job retry with exponential backoff (1 s → 5 min, max 20 attempts) for network/5xx errors; 4xx stops with a readable reason. An `auth_expired` error does not fail or retry the job: it pauses it (`pause_reason: auth_expired`, the reason in `error`, no attempt used, checkpoint kept, still paused after a restart) until `JobQueue::resume_auth_expired` is called when the account signs in again (the app does that after Connect or Reconnect). The `retry_job` action (Free, idempotent by `request_id`; MCP tool and `capturemann jobs retry <id>`) re-queues a failed job (attempts reset, checkpoint kept) or a cancelled one (from the start) and continues one paused as `auth_expired`; a done job or a job the user paused is `invalid_input`.
- Notifications: toast on completion with "Copy link" and "Open"; failure toast with "Retry".

**Acceptance criteria**
- **AC1** Given auto-upload on stop to Drive, when a recording stops, then an upload job is queued within 2 s without user action.
- **AC2** Given "on export" with an export dependency, then the upload starts only after the export succeeds, and is skipped (with a note) if it fails.
- **AC3** Given recording starts during an upload with "Pause uploads while recording" on, then the upload pauses within 2 s and resumes after stop.
- **AC4** Given 20 failed attempts, then the job is marked failed and stays in the jobs list with Retry.

**Edge cases and errors** — Laptop sleep during upload → resume after wake.

**Agent interface** — `get_job`, `list_jobs { type: "upload" }`, `cancel_job`; events `job.progress`, `job.done`.

**Out of scope** — Bandwidth scheduling (upload only at night).

**Test notes** — Job engine unit tests; sleep/wake manual test (`tests/manual/STO-3.md`).

---

### STO-4 Share link after upload
**Priority** P0 · **Status** In progress (Oct 23, 2026: the Google Drive `anyone` reader link and `none` are built, with a note instead of a failure when the organisation forbids public links, tested with a fake; AC3 built and tested with fakes: a finished `upload` job of a capture adds `uploads[]` records to its manifest (`destination_id`, `destination`, `what`, `remote_id`, `bytes`, `files`, `share_link`, `at`; one for the export and one for the original bundle) without a revision, so there is nothing to undo, refreshes the index, and `list_library` items and `get_capture` return them as `uploads`; `share_link` stays empty for a local folder. OneDrive (`createLink` view, `anonymous` or `organization`, with the fall back to the organisation link and a note when anonymous links are forbidden) and Dropbox (`create_shared_link_with_settings`, the existing link through `list_shared_links`, team-only fall back with a note) links and the `organization` link mode are built the same way, tested with fakes. The manage panel of OneDrive and Dropbox offers the organization link mode (Google Drive keeps anyone and none); the upload toast has "Copy link" (the system clipboard, tested with fakes for the decision, not run). Oct 23, 2026, tested with fakes, not seen by eye: AC1 copy when done: the setting `storage.copy_link_when_done` (default on, app-only like the rest of `storage.*`; a switch in Settings → Destinations) makes the app put the share link of a finished upload on the clipboard (a `TextClipboard` fake in the tests; with the setting off, no link, or a clipboard that refuses, nothing is copied and the toast says nothing false), and the toast says "Link copied to the clipboard"; AC3 Copy link: an entry in the library card menu for captures with an upload that has a share link (the newest copy of the video, else of the original), and Copy link and Open on each finished copy on the recording details page. `delete_capture { also_delete_remote: true }` (Confirm tier) deletes the recorded Drive files and bundle folders, local copies and YouTube videos after a dialog that lists each one and says it is permanent; a failed remote step keeps the local capture and reports what was and was not deleted, and the outcome is in the manifest (`remote_deletions`); a Google Drive bundle copy claims a fresh `Title (2).cmrec` folder and a restart continues in it; `delete_capture` and `rename_capture` answer `busy` while an upload or publish job names the capture; upload records are written under the capture lock so a concurrent annotation cannot lose them (all tested with fakes, never run against a real account; OneDrive and Dropbox copies are left in place). Not built: a run against real accounts (manual script `tests/manual/STO-4.md` written, not run)) · **Crates** cm-storage · **Depends on** STO-2

**User story** — As a teacher, I want a share link copied as soon as the upload finishes, so that I can paste it to my class immediately.

**Description**
- Per destination link mode: **Anyone with the link can view** (default), **People in my organization** (OneDrive work accounts), **No link** (private).
- On completion, CaptureMann creates the link (Drive permission + `webViewLink`, OneDrive `createLink`, Dropbox `create_shared_link_with_settings`), stores it in the library, and copies it to the clipboard if "Copy link when done" is on (default on).
- Link available later from the library context menu (Copy link) and on the recording details page.
- The setting is `storage.copy_link_when_done` (Settings → Destinations); it is app-only like the rest of `storage.*`: agents cannot change it.

**Acceptance criteria**
- **AC1** Given link mode "Anyone", when the upload finishes, then the clipboard holds a link that opens the video in a logged-out browser.
- **AC2** Given "No link", then no permission is changed on the remote file.
- **AC3** Given the link exists, then `list_library` returns it in the item's uploads.

**Edge cases and errors** — Organization policy forbids public links → fall back to organization link and tell the user.

**Agent interface** — `upload` result includes `share_link`; `get_capture` lists uploads with links.

**Out of scope** — Expiring links, password-protected links (P2).

**Test notes** — Live sandbox test per provider; logged-out browser check is manual (`tests/manual/STO-4.md`).

---

### STO-5 Choose what to upload: original, export, or both
**Priority** P1 · **Status** In progress (the choice per destination and the bundle copy work (see STO-1); the edge case below is built, Oct 23, 2026, tested with fakes: after the last file the job compares each sent file's source (size and modification time, taken just before it was sent and kept in the checkpoint) with the disk, and sends a changed or new file into the same claimed folder with `StorageProvider::replace` (additive, default `not_implemented`: the local folder writes a temp file in the bundle folder and renames it over the old file, refusing `..`, links and anything outside the folder and the root; Google Drive updates the content of the recorded file id with a resumable PATCH, or finds the file by name in the folder, or creates it, and changes no permission or link), at most three rounds and then a note that the bundle kept changing; `recording.lock`, temp files and new files below `exports/` are never sent, a removed file is noted and left at the destination, the job result lists `resent`, the upload record counts the final state, a restart matches the sent files by their place at the destination, and a loose file or an export is never re-sent; OneDrive and Dropbox keep the default and leave a note; AC1 on a second machine and the size estimate in the settings were not run) · **Crates** cm-storage · **Depends on** STO-1, STO-3

**User story** — As a creator, I want to back up the full original bundle to my NAS but send only the small export to Drive, so that I keep quality and save cloud space.

**Description**
- Per destination: Export only (default for cloud), Original bundle only (default for local/NAS), Both.
- Original bundle upload copies every file of the bundle preserving structure (`<title>.cmrec/...`), so it reopens after download.
- Shows estimated size per choice when configuring.

**Acceptance criteria**
- **AC1** Given "Original bundle" to a NAS, then the copied bundle opens in CaptureMann on another machine with all tracks and edits.
- **AC2** Given "Export only" to Drive, then only the MP4 is uploaded.

**Edge cases and errors** — Bundle changes during upload (user edits) → manifest re-uploaded at the end. Built as: every changed or new file of the bundle is sent again into the same folder, replacing the remote copy, at most three rounds (then a note); tests in `crates/cm-daemon/tests/bundle_resend.rs` (Drive fake with a hook that edits the bundle at an exact call, a local NAS with a real concurrent edit), `crates/cm-storage` (`replace` on the local folder and on the fake Drive).

**Agent interface** — `upload { what: "export"|"original"|"both", export_preset? }`.

**Deleting the remote copies (Oct 24, 2026):** `cm-daemon`'s remote-delete plan keeps, next to each English row, a structured `view` of the copy (`form`, `path`, `place`, and for a copy left in place a `reason` id); they reach the clients as `params.remote_items` and `remote.items` (ids in the agent interface spec) and the app words them in the chosen language (`apps/desktop/src/remote_text.rs`). Provider messages after a failed delete stay as the provider sent them.

**Out of scope** — Deduplicated/incremental backups.

**Test notes** — Round-trip test: upload bundle → download → open.
