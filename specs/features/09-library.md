# Library (LIB)

### LIB-1 Local library with search
**Priority** P0 · **Status** In progress (built on macOS: bundles and SQLite index with FTS search and index rebuild, the library view with sidebar and context menu (Open, Reveal, Rename, Delete), `list_library` with transcript snippet hits; the daemon's `rename_capture` (Free, renames the bundle folder, event `capture.revised` with `renamed: true`) and `delete_capture` (Confirm, moves the bundle to the Trash, removes the index row, event `capture.deleted`), with Rename and Delete in the context menu wired to them and a daemon-side `rescan_library`; hands-on checks are in `tests/manual/LIB-1.md` and have not been run; the Uploaded and Published filter chips and the Delete question about the copies online are built (Oct 23, 2026, tested with fakes, not seen by eye, the manual steps 8 to 14 are not run); the live library watcher is built (polling, tested with fakes, `tests/manual/LIB-2.md` not run); the 1,000-capture performance test, the Duplicate/Copy/Export/Upload/Publish menu items and Repair are not built; Windows not run) · **Crates** cm-library, apps/desktop · **Depends on** CAP-1, REC-1, DESIGN §5.7 (kit screens `Editor`, `EditorLight` for the shell)

**User story** — As a creator, I want all my screenshots and recordings in one place with search, so that I find last week's demo in seconds.

**Description**
- Navigation (kit sidebar, DESIGN §5.7): **Recordings** (count), **Screenshots** (count), **Published**, then **Destinations**, **Agents & CLI**, **Settings**, which open the settings pages (STO, PUB, AGT) in the same window. The first three are the library filters: Recordings = `kind: recording`, Screenshots = `kind: screenshot`, Published = captures with at least one published platform. The earlier filters **All** and **Uploaded** have no sidebar item in the kit; keep them as a filter menu in the content header (All is the default when the library opens via "Open library"; the kit's `Editor` screen opens on Recordings). Decided (OD-6, Oct 21, 2026).
- Home view (DESIGN §5.7; the kit has no grid screen): the latest capture large, two recent ones beside it, then a grid; sort by date (default), title, duration, size.
- Search box matches title, transcript text (SQLite FTS5) and destination/platform names; results highlight the matched transcript snippet with its timestamp, and clicking opens the editor at that time.
- Each item shows: thumbnail (silent preview on hover for recordings), title, date, duration, size, status icons (uploaded, published, recovering, job running).
- Context menu: Open, Rename, Reveal in Finder/Explorer, Copy (image or file), Export, Upload to…, Publish…, Duplicate, Delete (Confirm tier, offers to delete remote uploads too). **Built:** Open, Reveal, Rename and Delete. Rename opens a rename box (it does not navigate to the detail view: the menu occludes mouse events and opening a card is ignored while the menu is up) and calls `rename_capture`; Delete asks, then calls `delete_capture`, which moves the bundle to the Trash (AC4). A rename is not an undo step (the daemon records it as `capture.revised` with `renamed: true`); both are refused with `busy` while the capture is being recorded or exported. With `also_delete_remote: true` the recorded remote copies are deleted first (Oct 23, 2026, tested with fakes, never run against a real account): the Confirm dialog lists each Google Drive file or folder (path and destination), each copy in a local destination folder, each YouTube video (title, URL, channel), each copy that is left in place and why, and that this is permanent; if a remote step fails (`auth_expired`, `network`, `platform_rejected`) the local capture is **kept** and the error says which copies were deleted and which were not; the manifest records the outcome and drops the records of the copies that are gone, so a second call only meets what is left. `busy` also covers a queued, running or paused upload or publish job of the capture (the message names the job). The app's own dialog can ask `Daemon::describe_delete` for the same rows. In the app (Oct 23, 2026, tested with fakes, not seen by eye): Delete asks the daemon for those rows (`Daemon::describe_delete`) and, when a capture has copies it can delete, the confirmation lists each as an "Online copy" row and offers "Also delete the copies on Google Drive / YouTube (permanent)", **off by default**: unticked, only the local capture goes to the Trash and the copies stay online; ticked, `delete_capture` runs with `also_delete_remote: true`. Copies the app cannot delete are listed as "Stays online". A capture with no online copy keeps the single plain confirmation. A partial failure keeps the capture and a toast (kept until dismissed) says what was and was not deleted and what to do (`auth_expired`: reconnect in Settings; `network`: check the connection). The logic is in `apps/desktop/src/delete_flow.rs`. Filter chips: All, Recordings, Screenshots, Uploaded and Published sit in the toolbar; Uploaded and Published are one-at-a-time toggles that combine with the kind and the search box and show the number of uploaded and published captures of the whole library (from `list_library { status }`; the index keeps `upload_count` and `publish_count` per capture, derived from the manifest when a bundle is indexed). The library rescans the folder at startup and when the window regains focus (bundles copied in appear, deleted ones go); a live watcher in the daemon (so the headless daemon has it too) notices bundles added, removed, renamed or edited from outside within about 3 s, indexes them incrementally and publishes `capture.created`, `capture.revised` (`renamed: true` for a folder rename) or `capture.deleted` with `by: "watcher"` and `external: true`; the library view reloads on those events and keeps its scroll position and selection. Mechanism, settling rules and own-write suppression: `crates/cm-daemon/src/watcher.rs`. "Rescan library" stays. **The remote-copy rows of the Delete dialog are worded in the app (Oct 24, 2026):** the daemon sends the copies as `remote_items` (see the agent interface spec) next to its English rows, and the app words each copy, the reason one stays online and the result toast in the chosen language; tested with fakes, English and Vietnamese.
- The index rebuilds from bundles on disk if missing or corrupted (bundles are the source of truth).
- Default title: `Recording {yyyy-MM-dd HH.mm}` / `Screenshot {…}`; renaming renames the bundle folder.

**Acceptance criteria**
- **AC1** Given 1,000 captures, then the library opens in ≤ 1 s and scrolling stays at 60 fps.
- **AC2** Given a recording whose transcript contains "readiness probe", when the user searches "readiness", then it appears with the snippet and clicking opens the editor at that word's time.
- **AC3** Given the SQLite index is deleted, when the app starts, then it rebuilds from the bundles and all captures reappear.
- **AC4** Given Delete, then a confirmation lists what will be removed (local bundle, and optionally remote uploads) and the bundle moves to the OS Trash/Recycle Bin, not permanent deletion.
- **AC5** Given rename to "K8s probes", then the folder becomes `K8s probes.cmrec` (with a suffix if taken) and the library entry updates.

**Edge cases and errors** — Bundles added by copying into the library folder appear within a few seconds through the library watcher (or at once after "Rescan library"); a bundle is indexed only after its folder has stopped changing for 1 s and its `manifest.json` parses (an unreadable one becomes a broken row after a further 10 s), and a folder with `recording.lock` is never indexed; unreadable bundles show a broken badge with "Repair" (runs REC-8 recovery).

**Agent interface** — MCP `list_library { query?, kind?, status?, limit?, cursor? }` (sidebar views map to `kind` and `status` values) → items (Free); hits of a transcript search carry `transcript { snippet, start_s }`; `status` is `uploaded` (at least one `uploads[]` record), `not_uploaded`, `published` (at least one `publishes[]` record) or `not_published`, any other value is `invalid_input`; it combines with `query` and `kind`, and paging stays correct because the filter runs in the index query (the index schema version is 2: an older index is rebuilt from the bundles on start, LIB-2.AC3); a `cursor` above 2,147,483,647 is `invalid_input`; resource `capturemann://library`; `rename_capture { capture_id, title, request_id? }` (Free, mutating); `delete_capture { capture_id, also_delete_remote?, request_id? }` (Confirm; strict input; `also_delete_remote: true` deletes the recorded remote copies first, see the context menu above). CLI `capturemann library list [--status uploaded|not_uploaded|published|not_published]|search|rename|rm [--also-remote]`. The internal action `rescan_library` (no MCP tool, no CLI command) is the app's "Rescan library".

**Out of scope** — Smart albums, face search.

**Test notes** — Performance test with 1,000 synthetic bundles; rebuild test.

---

### LIB-2 Configurable library location
**Priority** P0 · **Status** In progress (built on macOS: while the library folder is unavailable the watcher pauses, removes nothing from the index and resumes by diffing when it returns (tested with fakes; `tests/manual/LIB-2.md` covers the real unplug, AC2 and AC3 as far as the watcher goes); the `library.path` setting and its default resolution (`~/Movies/CaptureMann`), the fallback folder and the "Library unavailable" hint; Move existing captures with verification (AC1, AC4), the fallback-recording move (AC3) and the banner flow are not built; the setting is app-only: an agent cannot change it) · **Crates** cm-library, apps/desktop, cm-daemon · **Depends on** LIB-1, STO-1

**User story** — As a creator with big 4K files, I want my library on an external drive or NAS, so that my laptop disk doesn't fill up.

**Description**
- Settings → General → Library folder: default `~/Movies/CaptureMann` (macOS) or `%USERPROFILE%\Videos\CaptureMann` (Windows).
- Changing the folder offers: **Move existing captures** (copy, verify size + SHA-256, then delete originals) or **Start fresh** (old library stays where it is, can be re-added later).
- External drive or NAS offline → library shows "Library unavailable — connect `<name>`" and new recordings go to a fallback folder on the internal disk, moved later with the user's consent.
- Space check before recording uses the library folder's volume.

**Acceptance criteria**
- **AC1** Given the library is moved to an external drive, then all captures open from the new location and originals are removed only after every file verifies.
- **AC2** Given the drive is unplugged, when the user records, then the recording goes to the fallback folder and a banner explains it.
- **AC3** Given the drive is reconnected, then the user is offered to move fallback recordings to the library.
- **AC4** Given a move is interrupted (drive unplugged), then no capture is lost and the move can resume.

**Edge cases and errors** — Network drives (SMB) with high latency: recording always writes locally first to the fallback/temp folder and moves to the NAS after stop (recording directly to a NAS is unsupported in v0.1, noted in settings).

**Agent interface** — `get_settings` reads `library.path`. `set_settings` **refuses** it from agents (an app-only key: it redirects every later capture, and the spec requires moving files to be a Confirm-tier dialog); only the app's own UI writes it. A headless daemon started by the CLI resolves the same default and the same file-backed settings as the app, so both see one library.

**Out of scope** — Multiple libraries at once (P2).

**Test notes** — Move/verify tests with simulated interruption; manual SMB test.

---

### LIB-3 Tags and folders
**Priority** P2 · **Status** Draft (detail before v0.2) · **Crates** cm-library

**User story** — As a teacher, I want to tag recordings by course and lesson, so that I can organize a semester's worth of videos.

**Summary** — Tags stored in the manifest and indexed; folders as saved tag filters (virtual, not disk folders).

**Key criteria (to refine)** — Tag rename/merge; filter by multiple tags; agent tools `tag_capture`, `list_tags`.
