# Integrations

## Auth model

v0.1 uses the creator's own API apps (bring your own, BYO): it fully works for Google Drive, OneDrive, Dropbox, YouTube and Facebook Pages, but TikTok limits unaudited apps so heavily that CaptureMann defaults to TikTok's draft upload and makes a TikTok audit the first "official app" task after v0.1.

| Platform | What the user sets up (guided screen in the app) | Works fully with BYO? | Caveat |
| --- | --- | --- | --- |
| Google Drive + YouTube | Google Cloud project, enable Drive and YouTube Data APIs, OAuth consent screen, Desktop OAuth client | Yes | Set the consent screen to "In production": Google expires refresh tokens after 7 days for External apps left in "Testing" (verify in M6). The user sees an "unverified app" warning for their own app |
| OneDrive | Microsoft Entra app registration, public client, redirect `http://localhost` | Yes | Personal and work accounts both supported |
| Dropbox | Scoped app in the App Console, PKCE (no secret) | Yes | — |
| YouTube | Same Google project | Yes | Each project gets 100 upload calls per day, its own quota, so BYO users never share a limit |
| Facebook Pages | Meta app in Development mode, the user added as admin | Yes, for Pages the user manages | In Development mode, people with a role on the app can use any permission without App Review; personal profiles can't be posted to by API |
| TikTok | TikTok developer app with Content Posting API | Partly | Unaudited apps: direct posts are "Only me", only to private accounts, for at most 5 users per 24 h. v0.1 default: upload as a draft to the creator's TikTok inbox, finished in the TikTok app |

**Official apps (after v0.1)**, in order of user value:

1. **TikTok audit**: unlocks public direct posts; needs the full TikTok UX rules (see Publishing) and a privacy policy.
2. **Google verification** for `youtube.upload` (a sensitive scope) plus a YouTube API compliance review to raise the shared 100 uploads/day quota. `drive.file` is non-sensitive and needs no verification.
3. **Meta App Review** (and business verification) for `pages_manage_posts` in Live mode.
4. **Microsoft and Dropbox** production app registrations (light review).

Official apps need a public privacy policy and terms page, a support contact, and a project owner of record; plan them with the beta release.

**Desktop OAuth flow**: system browser → consent → redirect to `127.0.0.1:<random port>` → code exchange with PKCE → tokens to the keychain. Client secrets shipped in a desktop app are not secret by design (RFC 8252), so security rests on PKCE and the loopback redirect.

## Storage

All three cloud drives support resumable chunked uploads and share links, so one `StorageProvider` interface covers them; the differences are chunk sizes and how folders are chosen.

|  | Google Drive | OneDrive | Dropbox | Local / external / NAS |
| --- | --- | --- | --- | --- |
| API | Drive API v3, `files.create` with resumable upload | Microsoft Graph `createUploadSession` | `files/upload_session/start` → `append_v2` → `finish` | File copy |
| Scope | `drive.file` (non-sensitive) | `Files.ReadWrite` | `files.content.write`, `sharing.write`, `account_info.read`, `files.metadata.read`, `sharing.read` | — |
| Chunk rules | Multiples of 256 KiB (check in M6) | Multiples of 320 KiB, under 60 MiB per request; 5–10 MiB recommended; sequential | Up to 150 MB per request (check in M6) | Stream copy, then verify size and SHA-256 |
| Resume | Query the session URI for the received range | `GET` upload URL → `nextExpectedRanges`; 404 means restart | Re-append from the recorded offset | Restart the copy |
| Share link | Permission "anyone with the link" → `webViewLink` | `createLink` (view; anonymous or organization) | `create_shared_link_with_settings` | Path copied |
| Folder choice | With `drive.file` the app sees only files it created: default folder "CaptureMann", sub-folders created by the app | Any folder, browsed in the app through the API | Any folder | Any mounted path or SMB share |

**Behavior (STO-1 to STO-5)**

- Each destination has: auto-upload on stop (on/off), what to upload (original bundle, export, or both), folder pattern (for example `CaptureMann/{yyyy}/{mm}/{title}`), and share-link mode.
- Uploads start after the recording is safely closed and never compete with an active recording for disk or network priority.
- An unplugged drive or NAS doesn't fail the job: it waits as "destination offline" and resumes when the path returns.
- A cloud upload of the original bundle uploads the folder's files individually and keeps the structure, so the bundle opens again after download.
- The library records each upload's remote id and link, so "Copy link" works later and `delete_capture` can offer to remove remote copies.

## Publishing

YouTube and Facebook Pages publish fully with BYO credentials; TikTok publishes as a draft until CaptureMann's app passes TikTok's audit. All three go through one "Publish" sheet with per-platform fields, and each platform's rules come from `get_platform_rules` so the UI and agents check limits before uploading.

|  | YouTube | TikTok | Facebook Pages |
| --- | --- | --- | --- |
| API | Data API v3 `videos.insert`, resumable | Content Posting API: inbox upload (`video.upload`) and direct post (`video.publish`) | Video API, resumable upload on `graph.facebook.com`, then publish to the Page |
| Scopes / permissions | `youtube.upload`; `youtube` for thumbnails and playlists | `video.upload`, `video.publish` | `pages_show_list`, `pages_read_engagement`, `pages_manage_posts` |
| Limits | 256 GB per file; 100 upload calls/day per API project | MP4/MOV/WebM, H.264/H.265, 23–60 fps, 360–4096 px per side, ≤ 4 GB, ≤ 10 min via API; 6 requests/min per user | Check in M6 (not confirmed in this review) |
| Upload mechanics | Resumable session URI, any chunk size in 256 KiB multiples | Chunks 5–64 MB (final up to 128 MB), sequential, 1–1,000 chunks; upload URL valid 1 hour | Resumable Upload API, then publish by video id |
| Unaudited / BYO behavior | Current reference: unverified projects are not restricted to private. Observed Oct 23, 2026 (as reported by the maintainer, one upload, not stored: a video published as Unlisted from the maintainer's own unverified Google Cloud project showed Unlisted in YouTube Studio; Public was not tried, and Google's policy can differ by project and date, so the code still reports a video that comes back private as a warning and never changes it; `tests/manual/PUB-2.md` step 3 has the full record) | Direct post: "Only me", private accounts only, ≤ 5 users/24 h; inbox: ≤ 5 pending drafts/24 h | Development mode works for people with a role on the app |
| Fields in the Publish sheet | Title, description (with chapters), tags, category, privacy, made-for-kids, thumbnail, playlist | Caption (≤ 2,200 chars), privacy chosen by the user (no default), comment/duet/stitch toggles off by default, commercial content disclosure | Title, description, privacy |
| v0.1 default | Upload → private or unlisted as chosen → set thumbnail → add to playlist | Draft to the creator's inbox; finish in the TikTok app | Upload and publish to the chosen Page |

**TikTok UX rules (required for the audit, so built in from v0.1)**: fetch creator info when the sheet opens and show the account nickname; build privacy options from what TikTok returns, with none preselected; disable interactions the creator turned off; commercial content toggle with "Your brand" / "Branded content" choices; show the Music Usage Confirmation declaration before the Publish button; show a preview; never add watermarks; tell the user processing can take a few minutes and poll the publish status. Whether the inbox upload has extra limits for unaudited apps isn't documented; confirm it in M6.

**Agent rules for publishing**: `publish_video` is always a Confirm-tier action. The dialog shows platform, account, title, privacy and file; nothing is sent before the user clicks Publish (this is also a TikTok requirement).

## Sources

Checked Oct 19, 2026. Platform rules change often; re-check each before M6.

- [YouTube: Videos.insert reference](https://developers.google.com/youtube/v3/docs/videos/insert)
- [YouTube: recommended upload encoding settings](https://support.google.com/youtube/answer/1722171)
- [YouTube Data API changes, third-party summary (Aug 2026)](https://singhamandeep.com/?p=1570)
- [TikTok: Content Posting API, direct post](https://developers.tiktok.com/docs/en/content-posting-api-reference-direct-post)
- [TikTok: upload to inbox](https://developers.tiktok.com/doc/content-posting-api-reference-upload-video)
- [TikTok: media transfer guide](https://developers.tiktok.com/doc/content-posting-api-media-transfer-guide)
- [TikTok: content sharing guidelines](https://developers.tiktok.com/doc/content-sharing-guidelines)
- [Meta: Video API overview](https://developers.facebook.com/docs/video-api/overview)
- [Meta: build and test (development mode, app roles)](https://developers.facebook.com/docs/development/build-and-test)
- [Google Drive: choose API scopes](https://developers.google.com/workspace/drive/api/guides/api-specific-auth)
- [Microsoft Graph: createUploadSession](https://learn.microsoft.com/en-us/graph/api/driveitem-createuploadsession)
