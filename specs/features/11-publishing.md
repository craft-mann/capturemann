# Publishing (PUB)

Publishing uses the creator's own API apps (bring your own, BYO) in v0.1; official audited apps come later (Integrations tab → Auth model). Every publish is a **Confirm-tier** action (AGT-2): nothing is sent to a platform until the user clicks Publish in the sheet or approves the agent's confirmation dialog. Platform rules (limits, fields, privacy options) come from `platform_rules.toml` plus live queries (TikTok creator info). Platform API details and sources: Integrations tab → Publishing.

### PUB-1 Connect YouTube, TikTok and Facebook Pages with BYO credentials
> Scope change (owner decision, Oct 23, 2026): v0.1 ships ONE cloud drive, Google Drive, and ONE publish platform, YouTube. OneDrive, Dropbox, TikTok and Facebook Pages are "Coming soon" in the app and move to a later release, where user feedback or community contributions (the project is open source; see CONTRIBUTING.md) decide which come next. For PUB-1 that means YouTube only in v0.1; the TikTok and Facebook cards say "Coming soon".

**Priority** P0 · **Status** In progress (First real-account run on Oct 23, 2026, verified on hardware (as reported, `verification/STO-2-PUB-2-gdrive-youtube-2026-10-23-owner.md`): connecting YouTube with the owner's own client works; YouTube only, Oct 23, 2026, tested with fakes, not seen by eye and never run against a real Google account, keychain or browser: `cm-auth` token and OAuth plumbing with `ProviderConfig::google_youtube`; the guided setup in Settings → Destinations → Publish to (`apps/desktop/src/publish_connect.rs`, `ui/settings/publish_card.rs`): Google Cloud steps for the YouTube Data API v3 with the unverified-app, test-user (100) and 7-day Testing notes, client ID and secret into the keychain, system-browser PKCE sign-in, the channel name from `channels.list?mine=true` (`cm_publish::channel`), the account written to `publish_accounts.json` (`cm_publish::accounts::PublishAccountStore`, no secret in it), AC1 against fakes; Test connection (token renewal plus `channels.list`), Reconnect (also offered when the keychain entries are gone or the test says `auth_expired`), the default visibility, Disconnect (Oct 23, 2026, tested with fakes: asks Google to revoke the refresh token first through `cm_auth::revoke_account`, best effort, then deletes the keychain secrets and the entry; offline or refused it still signs out here and says "signed out here; Google could not be reached to revoke access, you can remove it at myaccount.google.com/permissions", so AC5 is met against fakes and not yet seen against Google); every connected channel is listed on the card, each with Manage or Reconnect, Add another channel, and the default one (the first in `publish_accounts.json`, which the publish sheet preselects) is chosen in Manage (tested with fakes, not seen by eye); manual script `tests/manual/PUB-1.md` (from-scratch setup in 15 minutes or less, unverified-app screen, test users, Reconnect after a revoke, Disconnect online and offline), not run yet; not built: TikTok and Meta provider configs and setup, the channel avatar) (`list_publish_accounts` is built in the daemon) · **Crates** cm-auth, cm-publish, apps/desktop · **Depends on** NFR-10

**User story** — As a creator, I want to connect my YouTube channel, TikTok account and Facebook Page, so that I can publish from CaptureMann without third-party services.

**Description**
- Settings → Destinations → "Publish to" (kit `Settings`, DESIGN §5.8; this replaces the earlier separate "Publishing" page): one card per platform (YouTube, TikTok, Facebook) with a status pill (Connected / Not connected), one detail line (for YouTube the channel name and default visibility, e.g. "default visibility: Unlisted") and a button, Manage when connected, Connect account otherwise. Connect account starts the **guided setup**: numbered steps with screenshots and deep links into the platform console (Google Cloud Console; TikTok for Developers; Meta for Developers), fields for client ID / key (and secret where required), and the exact redirect URI to register (`http://127.0.0.1` loopback, port chosen at runtime where the platform allows wildcards; otherwise a fixed port 53682 documented in the guide).
- Card copy: the kit's Facebook line reads "Post to a profile or a Page you manage". v0.1 publishes to **Pages only** (PUB-4, Meta API), so the card must say "Post to a Page you manage." (decision OD-7, Oct 21, 2026). The TikTok line ("Vertical 9:16 export with auto-reframed bubble") matches EXP-2/EDT-5 and PUB-3 (draft to inbox by default).
- OAuth: system browser, PKCE, loopback redirect; tokens to the keychain; refresh on demand.
- Scopes: YouTube `youtube.upload` + `youtube` (thumbnails, playlists) + `youtube.readonly` (channel list); TikTok `user.info.basic`, `video.upload`, `video.publish`; Facebook `pages_show_list`, `pages_read_engagement`, `pages_manage_posts`.
- After connecting, show the account: YouTube channel name and avatar; TikTok nickname; Facebook Pages the user manages (choose one or more).
- Platform-specific notes shown in setup: Google "set the consent screen to In production" (to avoid short-lived tokens in Testing mode; see open questions); TikTok "unaudited apps post as drafts or 'Only me'"; Meta "keep the app in Development mode and add yourself as admin".
- "Test connection" calls a read-only endpoint (channel list, creator info, Page list).

**Acceptance criteria**
- **AC1** Given a valid Google client ID, when the user completes OAuth, then the channel name appears and the token is in the keychain (not in any file).
- **AC2** Given a Meta app in Development mode with the user as admin, then the user's Pages are listed and selectable.
- **AC3** Given a TikTok developer app, then the account nickname shows and the sheet states the unaudited-app limits.
- **AC4** Given a first-time user following the guide, then setup takes ≤ 15 minutes per platform (usability test with 3 people).
- **AC5** Given "Disconnect", then tokens are deleted from the keychain and revoked at the platform where an endpoint exists.

**Edge cases and errors** — Wrong redirect URI → show the platform's error with the exact URI to register; user denies consent → back to setup with "Permission not granted".

**Agent interface** — Human-only (browser consent). Agents use `list_publish_accounts` (Free) → `{ platform, account_id, name, status }`.

**Open questions (verify in M6)** — Whether TikTok and Meta accept a plain `http://127.0.0.1` loopback redirect for BYO apps or require an `https` URI (fallback: a static redirect page that forwards the code to the loopback port); Google's 7-day refresh-token expiry for apps left in "Testing" (wording in the setup guide depends on it).

**Out of scope** — Official CaptureMann apps (after v0.1), Instagram/LinkedIn/X (PUB-7).

**Test notes** — Mock OAuth servers in CI; manual setup walkthrough per platform recorded with CaptureMann itself for the docs.

---

### PUB-2 Publish to YouTube
**Priority** P0 · **Status** In progress (First real-account run on Oct 23, 2026, verified on hardware (as reported, `verification/STO-2-PUB-2-gdrive-youtube-2026-10-23-owner.md`): one recording was published after connecting and "worked OK"; an Unlisted request came back Unlisted (not locked to private for the owner's unverified project; Public not tried), and resume and quota were not run; provider only, tested with a fake YouTube server: validation, resumable upload and resume, thumbnail, playlist and captions as warnings, quota and unverified-project handling, AC1 to AC4 as far as a provider can meet them; the publish sheet (Oct 23, 2026, form logic, validation mapping, job events and the daemon request tested with fakes, the page not seen by eye): title, description, tags, visibility with the account default and the extra confirmation for Public, made for kids with no default and Publish disabled with the reason (AC4), a thumbnail from a time slider over the export (drag or arrow keys, the frame follows when it rests) or an image file, a playlist (an ID or a playlist address is typed; the channel's playlists are not listed), an "Upload captions (SRT) with the video" switch shown only when the recording has captions, field errors from `cm_publish::validate`, the export to upload with an Export button when there is none, the quota line when the daemon reports one, the note about the confirmation dialog, progress from `job.*` events, the link with Open and Copy link, plain-word warnings (AC2 wording, privacy locked), Retry through `retry_job`; the description is a multi-line field (Oct 23, 2026: Enter inserts a line break, arrows move across lines, paste keeps line breaks, it wraps to the width, grows to a few rows and then scrolls; the text buffer logic is unit tested, the drawing is not seen by eye); the engine (Oct 23, 2026, tested with fakes, nothing run against a real YouTube account): `publish_video` in `cm-daemon` is validated before its dialog (field-named `invalid_input`), the Confirm dialog shows platform, account, title, privacy, made for kids, file and size, the playlist (the id as given, after the tags, only when one is given; its title would need a YouTube call before the user approves, so the dialog never shows it), that it uploads over the internet and the day's count, a refused or timed-out dialog never uploads, it is idempotent by `request_id`, `export_preset` queues an export the publish job waits for, `export_path` and SRT captions are accepted; the `publish` job (progress, cancel, resume after every chunk, retry with backoff, `auth_expired` pauses it until the account signs in again, held while recording, a privacy the platform locked and warnings are results, not failures); the `publishes[]` record in the manifest (no undo step) shown by `list_library`; `list_publish_accounts` with the Pacific-day counter (`uploads_today`, `quota { used, limit }`); MCP tools and `capturemann publish`; no action or parameter changes the privacy of an existing remote video; the daily count is kept per Google Cloud project, not per channel (Oct 23, 2026, tested with fakes): the key is the project number at the start of the client ID the account was connected with (read from the keychain entry, only the key is stored in `publish_quota.json`), so two channels that use one client share the 100 uploads a day, an account whose client is not known is counted on its own, and a file written by an earlier version (counters per account) is read as it is and folded into the project on the next upload; `PublishProvider::delete_video` (additive, default `not_implemented`) is built for YouTube (`videos.delete`, permanent, a video that is gone counts as deleted) for `delete_capture { also_delete_remote: true }`, tested with fakes; it is the only call that changes an existing video and no call changes its privacy; manual script `tests/manual/PUB-2.md` (every field, the privacy chosen against the privacy YouTube applied, which settles the open question against `specs/03-integrations.md`, resume, quota text, the Confirm dialog), not run yet; not built: chapters from AI-2 (no UI), a list of the channel's playlists to choose from, the Published page list) · **Crates** cm-publish · **Depends on** PUB-1, EXP-2, AI-2

**User story** — As a creator, I want to publish to YouTube with title, description, chapters, tags, privacy, thumbnail and playlist, so that my video is complete without opening YouTube Studio.

**Description**
- Fields: title (≤ 100 chars, required), description (≤ 5,000 chars; chapters inserted as `00:00 Intro` lines when present), tags (total ≤ 500 chars), category (default Science & Technology, remembered), privacy (Private / Unlisted / Public; default Unlisted, remembered), made for kids (required yes/no, no default), playlist (optional, from the channel's playlists), thumbnail (frame picker from the video or an image file; JPG/PNG ≤ 2 MB, 1280×720 recommended), captions upload (SRT from AI-1, optional).
- Flow: export (preset `youtube-4k` or `youtube-1080p` by master size) → resumable upload (`videos.insert`) → set thumbnail → add to playlist → upload captions → done with the video URL.
- Quota awareness: each project has 100 upload calls/day (own bucket); show "x of 100 uploads used today" (tracked locally per Google Cloud project, so channels that share one client share the count; reset at midnight Pacific).
- Shorts: a vertical video ≤ 3 minutes is published normally and YouTube classifies it; the sheet notes this.

**Acceptance criteria**
- **AC1** Given a 1 GB export, when published as Unlisted, then the job reports the YouTube URL, and the video's title, description, tags and privacy match the sheet.
- **AC2** Given a custom thumbnail on a channel without verified phone, then the video still publishes and the job reports "Thumbnail needs a verified YouTube account" as a warning, not a failure.
- **AC3** Given the network drops during upload, then it resumes from the last confirmed byte.
- **AC4** Given "made for kids" is not answered, then Publish is disabled.

**Edge cases and errors** — Quota exhausted → `platform_rejected` with "Upload limit reached for today; try after `<time>`". Duplicate publish of the same request id → returns the first result (idempotency).

**Agent interface** — MCP `publish_video { capture_id | export_path, platform: "youtube", account_id, metadata: { title, description, tags, category, privacy, made_for_kids, playlist_id?, thumbnail?, captions? }, request_id? }` → `{ job_id }` → `{ url, video_id, warnings[] }` (**Confirm**).

**Out of scope** — Scheduled publishing (PUB-6), premieres, monetization settings.

**Test notes** — Mock YouTube API for CI; weekly live test to a sandbox channel as Private.

---

### PUB-3 Publish to TikTok
**Priority** P2 (was P0; deferred Oct 23, 2026) · **Status** Deferred to a later release or a community pull request (see CONTRIBUTING.md) · **Crates** cm-publish · **Depends on** PUB-1, EXP-2, EDT-5

**User story** — As a creator, I want to send my vertical cut to TikTok with a caption, so that I can post from the same place I edit.

**Description**
- Default mode: **Upload as draft** to the creator's TikTok inbox (`video.upload`); the creator finishes in the TikTok app. **Direct post** (`video.publish`) is available but shows the unaudited limits ("Only me", private accounts only) until CaptureMann's official app is audited.
- Before showing the form, query creator info: show nickname/avatar, allowed privacy options, interaction settings, max post duration; stop with a message if the creator can't post now.
- TikTok UX rules (required, built in from v0.1): privacy dropdown with no default; Comment/Duet/Stitch unchecked by default and disabled when the creator turned them off; commercial content toggle off by default with "Your brand" / "Branded content" (at least one required when on; branded content can't be private); Music Usage Confirmation declaration above the button; preview of the video; no watermarks; caption ≤ 2,200 characters.
- Upload: export `vertical-1080`; chunked upload (5–64 MB chunks, final ≤ 128 MB, sequential, URL valid 1 hour); poll publish status until processed.
- Limits checked before upload: ≤ 10 min, ≤ 4 GB, 23–60 fps, 360–4,096 px per side.

**Acceptance criteria**
- **AC1** Given a 60 s vertical export, when sent as a draft, then the creator receives the TikTok inbox notification and the job reports success.
- **AC2** Given the form, then no privacy option is preselected and Publish is disabled until one is chosen.
- **AC3** Given Branded content checked, then "Only me" is disabled (or switched to public with a notice).
- **AC4** Given a 12-minute video, then the sheet blocks TikTok with the 10-minute API limit before uploading.
- **AC5** Given 5 pending drafts in 24 h, then the next attempt fails with a clear message (`spam_risk_too_many_pending_share`).

**Edge cases and errors** — Upload URL expires (> 1 hour) → restart the upload with a new init; creator banned from posting → readable error.

**Agent interface** — `publish_video { platform: "tiktok", mode: "draft"|"direct", metadata: { caption, privacy_level, allow_comment, allow_duet, allow_stitch, brand_content?, brand_organic? } }` (**Confirm**; the confirmation dialog shows the same TikTok-required fields and declarations — an agent can't skip them).

**Out of scope** — Photo posts, TikTok sounds, scheduling.

**Test notes** — Mock API; live test with a sandbox/test account; UX checklist review against TikTok's guidelines before applying for the audit.

---

### PUB-4 Publish to a Facebook Page
**Priority** P2 (was P0; deferred Oct 23, 2026) · **Status** Deferred to a later release or a community pull request (see CONTRIBUTING.md) · **Crates** cm-publish · **Depends on** PUB-1, EXP-2

**User story** — As a creator who runs a Facebook Page, I want to post my video there with a title and description, so that my audience sees it without manual uploads.

**Description**
- Choose a connected Page; fields: title, description, privacy where the API allows it (Page videos default to public).
- Flow: Resumable Upload API on `graph.facebook.com` → publish to the Page with the video id → poll processing status → return the post URL.
- Video limits are taken from `platform_rules.toml`; values must be confirmed against Meta's docs in M6 (not confirmed in the Oct 19 review).

**Acceptance criteria**
- **AC1** Given a Page the user manages and a 1080p export, then the video appears on the Page and the job returns its URL.
- **AC2** Given a token without `pages_manage_posts`, then the sheet shows "Reconnect and grant 'Manage posts'".
- **AC3** Given the network drops mid-upload, then the upload resumes.

**Edge cases and errors** — Personal profiles aren't supported by Meta's API: the account picker lists Pages only, with a note.

**Agent interface** — `publish_video { platform: "facebook", account_id: <page_id>, metadata: { title, description } }` (**Confirm**).

**Out of scope** — Reels (P2 with PUB-7), Groups, personal profiles, Stories.

**Test notes** — Mock Graph API; live test to a test Page.

---

### PUB-5 One publish sheet for several platforms
**Priority** P1 · **Status** Ready · **Crates** apps/desktop, cm-publish · **Depends on** PUB-2, PUB-3, PUB-4, AI-2

**User story** — As a creator, I want to publish one recording to YouTube, TikTok and Facebook in one go, with the right format for each, so that I save time.

**Description**
- Sheet (DESIGN §5.6): platform checkboxes at the top; a tab per selected platform with its preview (16:9 or 9:16) and fields; "Generate with AI" fills all tabs (AI-2, which is after v0.1: the button is not in v0.1) per platform rules; a shared field toggle copies title/description to all tabs.
- Validation per tab before Publish (limits, required fields, TikTok rules); the Publish button lists what will happen ("Publish to 3 platforms").
- Creates jobs: one export per distinct preset, then one publish per platform depending on its export; status per platform with links when done.

**Acceptance criteria**
- **AC1** Given YouTube and TikTok selected, then one `youtube-*` export and one `vertical-1080` export are made, and two publish jobs run.
- **AC2** Given TikTok fails, then YouTube still completes and the sheet shows per-platform results.
- **AC3** Given "Generate with AI", then each tab gets platform-appropriate text within its limits.

**Edge cases and errors** — A platform account needs reconnect → that tab shows Reconnect; others can proceed.

**Agent interface** — Agents call `publish_video` per platform; prompt `publish-everywhere` sequences it. One confirmation dialog may cover several platforms when called within 10 s with the same capture (dialog lists each).

**Out of scope** — Cross-posting text-only posts.

**Test notes** — Mock all three APIs; partial-failure tests.

---

### PUB-6 Scheduled publishing
**Priority** P2 · **Status** Draft (detail before v0.2) · **Crates** cm-publish

**User story** — As a creator, I want to schedule a release time, so that videos go live when my audience is online.

**Summary** — Use platform-native scheduling where available (YouTube `publishAt` with private status); otherwise not offered. No CaptureMann background scheduler needs to stay running.

**Key criteria (to refine)** — Time zone shown explicitly; schedule editable until the time.

---

### PUB-7 Instagram Reels, LinkedIn and X
**Priority** P2 · **Status** Draft (detail before v0.2) · **Crates** cm-publish

**User story** — As a creator, I want to post to Instagram Reels, LinkedIn and X too, so that one tool covers all my channels.

**Summary** — New `PublishProvider` implementations; each needs its own API review (Instagram Graph API for professional accounts, LinkedIn Community Management API, X API v2 media upload).

**Key criteria (to refine)** — Same Confirm tier, BYO-first auth, platform rules file entries, mock-API CI tests.
