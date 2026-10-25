//! YouTube provider (PUB-2): `videos.insert` with the resumable protocol, then `thumbnails.set`,
//! `playlistItems.insert` and `captions.insert`.
//!
//! * One upload is a session (`uploadType=resumable&part=snippet,status`) that takes the file in
//!   chunks (multiples of 256 KiB, 8 MiB by default). After every chunk the [`ResumeState`]
//!   (session URI, confirmed offset, file fingerprint, metadata hash) goes to
//!   [`Progress::save_resume`]. After a dropped connection the next step is a status query
//!   (`bytes */total`), never a blind resend, so the retry continues from the last confirmed byte
//!   (PUB-2.AC3). A changed file or changed metadata starts a new session.
//! * Once the video exists its id is part of the state, so a retry never creates a second video
//!   and only repeats the steps that did not finish.
//! * 429, 5xx and `rateLimitExceeded` back off per `Retry-After`; when the wait is long or the
//!   retries run out the result is `network` (the job retries later). 401 asks the
//!   [`TokenSource`] once for a different token, then `auth_expired`. `quotaExceeded`,
//!   `uploadLimitExceeded` and `dailyLimitExceeded` are `platform_rejected` with the reason as
//!   `platform_code`. Other 4xx are `platform_rejected` with YouTube's message.
//! * The privacy requested is the one in the metadata. The provider has no call that changes the
//!   privacy of an existing video. If YouTube answers with `private` where something else was
//!   requested (unverified API project), the publish succeeds with
//!   [`PublishStatus::PrivacyLocked`] and a warning.
//! * Thumbnail, playlist and caption refusals are warnings (PUB-2.AC2).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_core::CmError;
use cm_storage::http::{Body, HttpReply, HttpRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::meta::{rfc3339_utc, validate, PlatformRules, Privacy, PublishMeta};
use crate::{Progress, PublishProvider, PublishStatus, Published, ResumeState};
use crate::{StorageHttp, TokenSource};

const UPLOAD: &str = "https://www.googleapis.com/upload/youtube/v3";
const API: &str = "https://www.googleapis.com/youtube/v3";
/// YouTube takes chunks in multiples of this.
pub const CHUNK_UNIT: u64 = 256 * 1024;
pub const DEFAULT_CHUNK: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct YouTubeConfig {
    /// Rounded down to a multiple of 256 KiB (at least one unit).
    pub chunk_size: u64,
    /// Retries of one request (and of consecutive failed chunks) before giving up as `network`.
    pub max_retries: u32,
    pub backoff_base: Duration,
    pub max_backoff: Duration,
    /// A `Retry-After` longer than this is not waited for in place; the error is `network` and
    /// the job queue retries later.
    pub max_inline_wait: Duration,
    /// Language of the caption track when the metadata names none.
    pub default_caption_language: String,
}

impl Default for YouTubeConfig {
    fn default() -> Self {
        YouTubeConfig {
            chunk_size: DEFAULT_CHUNK,
            max_retries: 5,
            backoff_base: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
            max_inline_wait: Duration::from_secs(60),
            default_caption_language: "en".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Created {
    id: String,
    privacy: String,
    upload_status: String,
}

/// What is saved between attempts (the `ResumeState` JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct YtResume {
    session_uri: String,
    /// Bytes YouTube has confirmed.
    offset: u64,
    size: u64,
    fingerprint: String,
    meta_hash: String,
    /// Set when the video exists.
    video: Option<Created>,
    thumbnail_done: bool,
    playlist_done: bool,
    captions_done: bool,
    warnings: Vec<String>,
}

impl YtResume {
    fn to_state(&self) -> ResumeState {
        ResumeState(serde_json::to_value(self).unwrap_or(Value::Null))
    }
}

enum Attempt {
    Reply(HttpReply),
    Transient {
        message: String,
        wait: Option<Duration>,
    },
}

enum Status {
    Complete(HttpReply),
    Offset(u64),
    Gone,
}

pub struct YouTubeProvider {
    http: Arc<dyn StorageHttp>,
    tokens: Arc<dyn TokenSource>,
    cfg: YouTubeConfig,
    rules: PlatformRules,
}

impl YouTubeProvider {
    pub fn new(
        http: Arc<dyn StorageHttp>,
        tokens: Arc<dyn TokenSource>,
        mut cfg: YouTubeConfig,
    ) -> Self {
        cfg.chunk_size = (cfg.chunk_size / CHUNK_UNIT).max(1) * CHUNK_UNIT;
        YouTubeProvider {
            http,
            tokens,
            cfg,
            rules: PlatformRules::youtube(),
        }
    }

    /// Replaces the rules (the user's `platform_rules.toml` override).
    pub fn with_rules(mut self, rules: PlatformRules) -> Self {
        self.rules = rules;
        self
    }

    // ---- requests ----------------------------------------------------------------------------

    /// One try of a request. 401 asks the token source once for another token and sends again.
    async fn attempt(
        &self,
        tok: &mut String,
        make: &(dyn Fn(&str) -> HttpRequest + Sync),
    ) -> Result<Attempt, CmError> {
        let mut auth_retried = false;
        loop {
            let reply = match self.http.send(make(tok)).await {
                Ok(r) => r,
                Err(e) => {
                    return Ok(Attempt::Transient {
                        message: format!("Cannot reach YouTube: {}", e.0),
                        wait: None,
                    })
                }
            };
            match reply.status {
                401 => {
                    if auth_retried {
                        return Err(reconnect());
                    }
                    auth_retried = true;
                    let fresh = self.tokens.access_token(Some(tok)).await?;
                    if fresh == *tok {
                        return Err(reconnect());
                    }
                    *tok = fresh;
                }
                408 | 429 | 500..=599 => {
                    return Ok(Attempt::Transient {
                        message: format!(
                            "YouTube is busy or not responding (HTTP {})",
                            reply.status
                        ),
                        wait: retry_after(&reply),
                    });
                }
                403 if is_rate_limit(&reply) => {
                    return Ok(Attempt::Transient {
                        message: "YouTube is rate limiting requests (HTTP 403)".to_owned(),
                        wait: retry_after(&reply),
                    });
                }
                _ => return Ok(Attempt::Reply(reply)),
            }
        }
    }

    /// A request that retries transient failures with back-off (non-chunk requests).
    async fn call(
        &self,
        tok: &mut String,
        make: &(dyn Fn(&str) -> HttpRequest + Sync),
    ) -> Result<HttpReply, CmError> {
        let mut failures = 0u32;
        loop {
            match self.attempt(tok, make).await? {
                Attempt::Reply(r) => return Ok(r),
                Attempt::Transient { message, wait } => {
                    failures += 1;
                    self.back_off(failures, wait, message).await?;
                }
            }
        }
    }

    async fn back_off(
        &self,
        failures: u32,
        wait: Option<Duration>,
        message: String,
    ) -> Result<(), CmError> {
        if failures > self.cfg.max_retries {
            return Err(CmError::Network { message });
        }
        let delay = match wait {
            Some(w) if w > self.cfg.max_inline_wait => {
                return Err(CmError::Network {
                    message: format!("{message}; try again in {} s", w.as_secs()),
                })
            }
            Some(w) => w,
            None => self
                .cfg
                .backoff_base
                .saturating_mul(1u32 << (failures - 1).min(10))
                .min(self.cfg.max_backoff),
        };
        tokio::time::sleep(delay).await;
        Ok(())
    }

    // ---- the video ---------------------------------------------------------------------------

    /// The `videos.insert` body: snippet and status, nothing else.
    fn insert_body(&self, meta: &PublishMeta) -> Vec<u8> {
        let privacy = meta.effective_privacy(&self.rules);
        let mut status = json!({
            "privacyStatus": privacy.as_str(),
            "selfDeclaredMadeForKids": meta.made_for_kids.unwrap_or(false),
        });
        if let Some(at) = meta.scheduled_at_unix {
            status["publishAt"] = json!(rfc3339_utc(at));
        }
        json!({
            "snippet": {
                "title": meta.title.trim(),
                "description": meta.description,
                "tags": meta.tags,
                "categoryId": meta.category.clone().unwrap_or_else(|| self.rules.default_category.clone()),
            },
            "status": status,
        })
        .to_string()
        .into_bytes()
    }

    async fn new_session(
        &self,
        tok: &mut String,
        file: &Path,
        size: u64,
        fingerprint: &str,
        body: &[u8],
        meta_hash: &str,
    ) -> Result<YtResume, CmError> {
        let url = format!("{UPLOAD}/videos?uploadType=resumable&part=snippet,status");
        let mime = video_mime(file);
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new("POST", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", "application/json; charset=UTF-8")
                    .header("X-Upload-Content-Type", mime)
                    .header("X-Upload-Content-Length", size.to_string())
                    .body(Body::Bytes(body.to_vec()))
            })
            .await?;
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        let session_uri = reply
            .header_value("location")
            .ok_or_else(|| CmError::Internal {
                message: "YouTube did not return an upload address".to_owned(),
            })?
            .to_owned();
        Ok(YtResume {
            session_uri,
            offset: 0,
            size,
            fingerprint: fingerprint.to_owned(),
            meta_hash: meta_hash.to_owned(),
            video: None,
            thumbnail_done: false,
            playlist_done: false,
            captions_done: false,
            warnings: Vec::new(),
        })
    }

    async fn query_status(&self, tok: &mut String, st: &YtResume) -> Result<Status, CmError> {
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new("PUT", st.session_uri.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Range", format!("bytes */{}", st.size))
            })
            .await?;
        match reply.status {
            200 | 201 => Ok(Status::Complete(reply)),
            308 => Ok(Status::Offset(confirmed(&reply))),
            404 | 410 => Ok(Status::Gone),
            _ => Err(api_error(&reply)),
        }
    }

    /// Sends the file; returns the video resource YouTube answers with.
    async fn upload_video(
        &self,
        tok: &mut String,
        file: &Path,
        st: &mut YtResume,
        body: &[u8],
        mut need_sync: bool,
        progress: &Progress,
    ) -> Result<HttpReply, CmError> {
        let size = st.size;
        let mut failures = 0u32;
        let mut restarts = 0u32;
        loop {
            if progress.is_cancelled() {
                return Err(CmError::Internal {
                    message: "Upload cancelled".to_owned(),
                });
            }
            if need_sync {
                match self.query_status(tok, st).await? {
                    Status::Complete(r) => return Ok(r),
                    Status::Offset(n) if n < size => st.offset = n,
                    Status::Offset(_) | Status::Gone => {
                        restarts += 1;
                        if restarts > 2 {
                            return Err(CmError::Network {
                                message: "YouTube keeps losing the upload session".to_owned(),
                            });
                        }
                        let (fp, mh) = (st.fingerprint.clone(), st.meta_hash.clone());
                        *st = self.new_session(tok, file, size, &fp, body, &mh).await?;
                        progress.save_resume(&st.to_state());
                    }
                }
                progress.report(st.offset, size);
                need_sync = false;
            }
            if st.offset >= size {
                // Everything is confirmed but the final answer was not seen: ask for it.
                need_sync = true;
                continue;
            }
            let len = self.cfg.chunk_size.min(size - st.offset);
            let range = format!("bytes {}-{}/{size}", st.offset, st.offset + len - 1);
            let chunk = Body::FileRange {
                path: file.to_path_buf(),
                offset: st.offset,
                len,
            };
            let outcome = self
                .attempt(tok, &|t| {
                    HttpRequest::new("PUT", st.session_uri.clone())
                        .header("Authorization", bearer(t))
                        .header("Content-Range", range.clone())
                        .body(chunk.clone())
                })
                .await?;
            let reply = match outcome {
                Attempt::Transient { message, wait } => {
                    failures += 1;
                    self.back_off(failures, wait, message).await?;
                    // The chunk may have arrived: ask before sending anything again.
                    need_sync = true;
                    continue;
                }
                Attempt::Reply(r) => r,
            };
            match reply.status {
                200 | 201 => return Ok(reply),
                308 => {
                    let n = confirmed(&reply);
                    if n > st.offset {
                        failures = 0;
                    }
                    st.offset = n.min(size);
                    progress.save_resume(&st.to_state());
                    progress.report(st.offset, size);
                }
                404 | 410 => need_sync = true,
                _ => return Err(api_error(&reply)),
            }
        }
    }

    // ---- the extras --------------------------------------------------------------------------

    /// Runs a request of an optional step. A transient failure that outlasts the retries is an
    /// error (the job retries; the video is already in the resume state); a refusal is `Err(text)`
    /// for a warning.
    async fn side_call(
        &self,
        tok: &mut String,
        make: &(dyn Fn(&str) -> HttpRequest + Sync),
    ) -> Result<Result<HttpReply, HttpReply>, CmError> {
        let reply = self.call(tok, make).await?;
        if (200..300).contains(&reply.status) {
            Ok(Ok(reply))
        } else {
            Ok(Err(reply))
        }
    }

    async fn set_thumbnail(
        &self,
        tok: &mut String,
        id: &str,
        path: &Path,
    ) -> Result<Option<String>, CmError> {
        let bytes = read_file("thumbnail", path)?;
        let mime = image_mime(path);
        let url = format!("{UPLOAD}/thumbnails/set?videoId={id}&uploadType=media");
        Ok(
            match self
                .side_call(tok, &|t| {
                    HttpRequest::new("POST", url.clone())
                        .header("Authorization", bearer(t))
                        .header("Content-Type", mime)
                        .body(Body::Bytes(bytes.clone()))
                })
                .await?
            {
                Ok(_) => None,
                Err(r) if r.status == 403 => {
                    Some("Thumbnail needs a verified YouTube account".to_owned())
                }
                Err(r) => Some(format!("The thumbnail was not set: {}", reason_text(&r))),
            },
        )
    }

    async fn add_to_playlist(
        &self,
        tok: &mut String,
        id: &str,
        playlist: &str,
    ) -> Result<Option<String>, CmError> {
        let url = format!("{API}/playlistItems?part=snippet");
        let body = json!({
            "snippet": {
                "playlistId": playlist,
                "resourceId": { "kind": "youtube#video", "videoId": id },
            }
        })
        .to_string()
        .into_bytes();
        Ok(
            match self
                .side_call(tok, &|t| {
                    HttpRequest::new("POST", url.clone())
                        .header("Authorization", bearer(t))
                        .header("Content-Type", "application/json; charset=UTF-8")
                        .body(Body::Bytes(body.clone()))
                })
                .await?
            {
                Ok(_) => None,
                Err(r) => Some(format!(
                    "The video was not added to the playlist: {}",
                    reason_text(&r)
                )),
            },
        )
    }

    async fn add_captions(
        &self,
        tok: &mut String,
        id: &str,
        path: &Path,
        language: &str,
    ) -> Result<Option<String>, CmError> {
        let srt = read_file("captions", path)?;
        let boundary = format!("cm-{}", &hex_hash(&srt)[..16]);
        let meta = json!({
            "snippet": { "videoId": id, "language": language, "name": "", "isDraft": false }
        })
        .to_string();
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(&srt);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let url = format!("{UPLOAD}/captions?uploadType=multipart&part=snippet");
        Ok(
            match self
                .side_call(tok, &|t| {
                    HttpRequest::new("POST", url.clone())
                        .header("Authorization", bearer(t))
                        .header(
                            "Content-Type",
                            format!("multipart/related; boundary={boundary}"),
                        )
                        .body(Body::Bytes(body.clone()))
                })
                .await?
            {
                Ok(_) => None,
                Err(r) => Some(format!("The captions were not added: {}", reason_text(&r))),
            },
        )
    }
}

#[async_trait]
impl PublishProvider for YouTubeProvider {
    fn id(&self) -> &'static str {
        "youtube"
    }

    fn requirements(&self) -> PlatformRules {
        self.rules.clone()
    }

    async fn delete_video(&self, remote_id: &str) -> Result<(), CmError> {
        if remote_id.is_empty() {
            return Err(CmError::invalid_input("the video has no id"));
        }
        let mut tok = self.tokens.access_token(None).await?;
        let url = format!(
            "{API}/videos?id={}",
            percent_encoding::utf8_percent_encode(remote_id, percent_encoding::NON_ALPHANUMERIC)
        );
        let reply = self
            .call(&mut tok, &|t| {
                HttpRequest::new("DELETE", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        match reply.status {
            200..=299 => Ok(()),
            // YouTube answers 404 `videoNotFound` for a video that is gone: nothing left to do.
            404 => Ok(()),
            _ => Err(api_error(&reply)),
        }
    }

    async fn publish(
        &self,
        video: &Path,
        meta: &PublishMeta,
        resume: Option<ResumeState>,
        progress: Progress,
    ) -> Result<Published, CmError> {
        // Nothing leaves the machine before this passes.
        validate(&self.rules, video, meta)?;

        let (size, fingerprint) = fingerprint(video).await?;
        let body = self.insert_body(meta);
        let meta_hash = hex_hash(&body);
        let mut tok = self.tokens.access_token(None).await?;

        let saved = resume
            .and_then(|r| serde_json::from_value::<YtResume>(r.0).ok())
            .filter(|s| s.size == size && s.fingerprint == fingerprint && s.meta_hash == meta_hash);
        let resumed = saved.is_some();
        let mut st = match saved {
            Some(s) => s,
            None => {
                let s = self
                    .new_session(&mut tok, video, size, &fingerprint, &body, &meta_hash)
                    .await?;
                progress.save_resume(&s.to_state());
                s
            }
        };

        // 1. The video.
        if st.video.is_none() {
            // A restored state has an unknown chunk fate: ask first.
            let need_sync = resumed;
            let reply = self
                .upload_video(&mut tok, video, &mut st, &body, need_sync, &progress)
                .await?;
            st.video = Some(parse_created(&reply)?);
            st.offset = size;
            progress.save_resume(&st.to_state());
            progress.report(size, size);
        }
        let created = st.video.clone().ok_or_else(|| CmError::Internal {
            message: "YouTube video state lost".to_owned(),
        })?;

        // 2. The extras. None of them changes the video's privacy.
        if !st.thumbnail_done {
            if let Some(t) = &meta.thumbnail {
                if progress.is_cancelled() {
                    return Err(cancelled());
                }
                if let Some(w) = self.set_thumbnail(&mut tok, &created.id, t).await? {
                    st.warnings.push(w);
                }
            }
            st.thumbnail_done = true;
            progress.save_resume(&st.to_state());
        }
        if !st.playlist_done {
            if let Some(p) = &meta.playlist_id {
                if progress.is_cancelled() {
                    return Err(cancelled());
                }
                if let Some(w) = self.add_to_playlist(&mut tok, &created.id, p).await? {
                    st.warnings.push(w);
                }
            }
            st.playlist_done = true;
            progress.save_resume(&st.to_state());
        }
        if !st.captions_done {
            if let Some(c) = &meta.captions {
                if progress.is_cancelled() {
                    return Err(cancelled());
                }
                let lang = meta
                    .captions_language
                    .clone()
                    .unwrap_or_else(|| self.cfg.default_caption_language.clone());
                if let Some(w) = self.add_captions(&mut tok, &created.id, c, &lang).await? {
                    st.warnings.push(w);
                }
            }
            st.captions_done = true;
            progress.save_resume(&st.to_state());
        }

        // 3. What YouTube says about the result.
        let requested = meta.effective_privacy(&self.rules);
        let actual = Privacy::parse(&created.privacy).unwrap_or(Privacy::Private);
        let mut warnings = st.warnings.clone();
        let scheduled = meta.scheduled_at_unix.is_some();
        let status = if actual != requested && !scheduled {
            warnings.push(format!(
                "YouTube kept this video {} although {} was requested. API projects that are not verified can only publish private videos; change the visibility in YouTube Studio or verify the project.",
                actual.as_str(),
                requested.as_str()
            ));
            PublishStatus::PrivacyLocked
        } else if created.upload_status == "processed" {
            PublishStatus::Processed
        } else {
            PublishStatus::Processing
        };
        Ok(Published {
            url: format!("https://www.youtube.com/watch?v={}", created.id),
            remote_id: created.id,
            status,
            privacy: actual,
            warnings,
        })
    }
}

// ---- helpers ---------------------------------------------------------------------------------

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn cancelled() -> CmError {
    CmError::Internal {
        message: "Publish cancelled".to_owned(),
    }
}

fn reconnect() -> CmError {
    CmError::AuthExpired {
        message: "YouTube no longer accepts this account. Reconnect it in Settings → Destinations."
            .to_owned(),
    }
}

fn read_file(field: &str, path: &Path) -> Result<Vec<u8>, CmError> {
    std::fs::read(path)
        .map_err(|e| CmError::invalid_field(field, format!("cannot read {}: {e}", path.display())))
}

fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn video_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        _ => "video/mp4",
    }
}

fn image_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        _ => "image/jpeg",
    }
}

/// `(size, fingerprint)`: size, modification time and a hash of the first MiB, so a file that was
/// edited or replaced starts a new session.
async fn fingerprint(path: &Path) -> Result<(u64, String), CmError> {
    let path: PathBuf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let fail = |e: std::io::Error| CmError::Internal {
            message: format!("Cannot read {}: {e}", path.display()),
        };
        let mut f = std::fs::File::open(&path).map_err(fail)?;
        let meta = f.metadata().map_err(fail)?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let mut head = Vec::new();
        (&mut f)
            .take(1 << 20)
            .read_to_end(&mut head)
            .map_err(fail)?;
        Ok((
            meta.len(),
            format!("{}:{mtime}:{}", meta.len(), hex_hash(&head)),
        ))
    })
    .await
    .map_err(|e| CmError::Internal {
        message: format!("worker failed: {e}"),
    })?
}

/// Bytes YouTube has confirmed: `Range: bytes=0-N` means N+1; no header means none.
fn confirmed(reply: &HttpReply) -> u64 {
    reply
        .header_value("range")
        .and_then(|r| r.rsplit('-').next())
        .and_then(|n| n.trim().parse::<u64>().ok())
        .map(|n| n + 1)
        .unwrap_or(0)
}

fn retry_after(reply: &HttpReply) -> Option<Duration> {
    reply
        .header_value("retry-after")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn error_parts(reply: &HttpReply) -> (Option<String>, Option<String>) {
    let v = reply.json().unwrap_or(Value::Null);
    let message = v["error"]["message"].as_str().map(str::to_owned);
    let reason = v["error"]["errors"][0]["reason"]
        .as_str()
        .or_else(|| v["error"]["status"].as_str())
        .map(str::to_owned);
    (message, reason)
}

fn is_rate_limit(reply: &HttpReply) -> bool {
    matches!(
        error_parts(reply).1.as_deref(),
        Some("rateLimitExceeded" | "userRateLimitExceeded")
    )
}

fn reason_text(reply: &HttpReply) -> String {
    let (m, _) = error_parts(reply);
    cm_jobs::scrub_secrets(&m.unwrap_or_else(|| format!("HTTP {}", reply.status)))
}

/// A non-retryable error status as the shared error: `platform_rejected` with YouTube's reason as
/// `platform_code`. Quota reasons get the PUB-2 wording.
fn api_error(reply: &HttpReply) -> CmError {
    let (message, reason) = error_parts(reply);
    let code = reason.unwrap_or_else(|| reply.status.to_string());
    let message = match code.as_str() {
        "quotaExceeded" | "uploadLimitExceeded" | "dailyLimitExceeded" => {
            "YouTube: Upload limit reached for today; try again after midnight Pacific time."
                .to_owned()
        }
        _ => format!(
            "YouTube: {}",
            cm_jobs::scrub_secrets(&message.unwrap_or_else(|| format!("HTTP {}", reply.status)))
        ),
    };
    CmError::PlatformRejected {
        message,
        platform_code: Some(code),
    }
}

fn parse_created(reply: &HttpReply) -> Result<Created, CmError> {
    let v = reply.json().ok_or_else(|| CmError::Internal {
        message: "YouTube sent an unreadable answer".to_owned(),
    })?;
    let id = v["id"].as_str().ok_or_else(|| CmError::Internal {
        message: "YouTube sent no video id".to_owned(),
    })?;
    let upload_status = v["status"]["uploadStatus"].as_str().unwrap_or("uploaded");
    if matches!(upload_status, "failed" | "rejected" | "deleted") {
        let why = v["status"]["rejectionReason"]
            .as_str()
            .or_else(|| v["status"]["failureReason"].as_str())
            .unwrap_or(upload_status);
        return Err(CmError::PlatformRejected {
            message: format!("YouTube did not accept the video: {why}"),
            platform_code: Some(why.to_owned()),
        });
    }
    Ok(Created {
        id: id.to_owned(),
        privacy: v["status"]["privacyStatus"]
            .as_str()
            .unwrap_or("private")
            .to_owned(),
        upload_status: upload_status.to_owned(),
    })
}

#[cfg(test)]
mod tests;
