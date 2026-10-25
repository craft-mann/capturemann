//! Dropbox provider (STO-2, STO-4): the HTTP API v2 upload session with the scopes
//! `files.content.write` and `sharing.write`. OAuth uses PKCE, so no client secret is involved.
//!
//! * One upload is a session: `files/upload_session/start` (empty), then `append_v2` for every
//!   chunk but the last (at most 150 MB, 16 MiB by default), then `finish` with the last piece and
//!   the commit (`mode: add`, `autorename`). Dropbox creates missing folders on commit.
//! * The client tracks the offset. After every confirmed chunk the [`ResumeState`] (session id,
//!   offset, size, file fingerprint, path) goes to [`Progress::save_resume`]. Dropbox has no status
//!   query: after a dropped connection or a restart the chunk is sent again at the recorded offset,
//!   and `incorrect_offset` answers with the `correct_offset` to continue from (the chunk had
//!   landed). A session that expired (`not_found`) starts over.
//! * A `finish` whose answer was lost and is sent again finds the session closed: the file is then
//!   read with `get_metadata` and accepted when its size is right.
//! * 429 (including `too_many_write_operations`) waits for `Retry-After` (header or body); when
//!   the wait is long or the retries run out the result is `network`. 401 asks the [`TokenSource`]
//!   once more, then `auth_expired`. `insufficient_space` and other refusals are
//!   `platform_rejected` with Dropbox's message and tag.
//! * Paths: with full Dropbox access files go under `/<root folder>/...` (`CaptureMann` by
//!   default); with an app-folder app the app folder is the root and the root folder name is not
//!   added. A leading `Target::folder` segment equal to the root folder name is skipped.
//! * The link (STO-4) is `create_shared_link_with_settings` (public viewer link, or team-only for
//!   [`LinkMode::Organization`]). If a link exists already, `list_shared_links` returns it. If the
//!   team forbids public links the team-only link is made and a note says so.
//! * The remote size must equal the local size when the upload ends (STO-2.AC2).

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use cm_core::CmError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::cloud::{folder_segments, Attempt, Client, Retry};
use crate::gdrive::{bearer, fingerprint as file_fingerprint};
use crate::http::{Body, HttpReply, HttpRequest, StorageHttp};
use crate::{LinkMode, Progress, ResumeState, StorageProvider, Target, TokenSource, Uploaded};

const API: &str = "https://api.dropboxapi.com/2";
const CONTENT: &str = "https://content.dropboxapi.com/2";
pub const DEFAULT_CHUNK: u64 = 16 * 1024 * 1024;
/// Dropbox takes at most 150 MB in one request.
pub const MAX_CHUNK: u64 = 150_000_000;

#[derive(Debug, Clone)]
pub struct DropboxConfig {
    /// Name of the folder everything goes below (full Dropbox access only).
    pub root_folder_name: String,
    /// The app was registered with "App folder" access: the app folder is the root.
    pub app_folder: bool,
    pub link_mode: LinkMode,
    /// Clamped to 1 byte..=150 MB. Multiples of 4 MiB are what Dropbox recommends (the default is).
    pub chunk_size: u64,
    pub retry: Retry,
}

impl Default for DropboxConfig {
    fn default() -> Self {
        DropboxConfig {
            root_folder_name: "CaptureMann".to_owned(),
            app_folder: false,
            link_mode: LinkMode::Anyone,
            chunk_size: DEFAULT_CHUNK,
            retry: Retry::default(),
        }
    }
}

/// What is saved between attempts (the `ResumeState` JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DropboxResume {
    session_id: String,
    /// Bytes Dropbox has confirmed.
    offset: u64,
    size: u64,
    fingerprint: String,
    path: String,
}

impl DropboxResume {
    fn to_state(&self) -> ResumeState {
        ResumeState(serde_json::to_value(self).unwrap_or(Value::Null))
    }
}

struct Remote {
    id: String,
    size: Option<u64>,
}

pub struct DropboxProvider {
    client: Client,
    cfg: DropboxConfig,
}

impl DropboxProvider {
    pub fn new(
        http: Arc<dyn StorageHttp>,
        tokens: Arc<dyn TokenSource>,
        mut cfg: DropboxConfig,
    ) -> Self {
        cfg.chunk_size = cfg.chunk_size.clamp(1, MAX_CHUNK);
        DropboxProvider {
            client: Client {
                http,
                tokens,
                retry: cfg.retry.clone(),
                service: "Dropbox",
            },
            cfg,
        }
    }

    /// The Dropbox path of a file: `/CaptureMann/2026/10/Lesson/Lesson.mp4`.
    fn remote_path(&self, target: &Target) -> String {
        let mut path = String::new();
        if !self.cfg.app_folder {
            path.push('/');
            path.push_str(self.cfg.root_folder_name.trim_matches('/'));
        }
        for s in folder_segments(&target.folder, &self.cfg.root_folder_name) {
            path.push('/');
            path.push_str(s);
        }
        path.push('/');
        path.push_str(target.file_name.trim());
        path
    }

    /// Who signed in (`users/get_current_account`): the account's email address. The route takes
    /// no argument, so the request has no body and no content type (Dropbox refuses
    /// `application/json` with an empty body).
    pub async fn account_email(&self) -> Result<String, CmError> {
        let mut tok = self.client.tokens.access_token(None).await?;
        let url = format!("{API}/users/get_current_account");
        let reply = self
            .client
            .call(&mut tok, &|t| {
                HttpRequest::new("POST", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        reply
            .json()
            .and_then(|v| v["email"].as_str().map(str::to_owned))
            .filter(|e| !e.is_empty())
            .ok_or_else(|| CmError::Internal {
                message: "Dropbox did not say which account this is".to_owned(),
            })
    }

    /// Uploads a 1 KB file and deletes it again (the Settings "Test connection" button).
    pub async fn test_connection(&self) -> Result<(), CmError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "capturemann-connection-test-{}-{nanos}.txt",
            std::process::id()
        ));
        std::fs::write(&path, vec![b'x'; 1024]).map_err(|e| CmError::Internal {
            message: format!("Cannot write the test file: {e}"),
        })?;
        let target = Target {
            folder: String::new(),
            file_name: "capturemann-connection-test.txt".to_owned(),
        };
        let result = self
            .upload_with(&path, &target, None, Progress::none(), LinkMode::None)
            .await;
        let _ = std::fs::remove_file(&path);
        let up = result?;
        let mut tok = self.client.tokens.access_token(None).await?;
        let reply = self
            .api_json(
                &mut tok,
                "files/delete_v2",
                &json!({ "path": up.remote_id }),
            )
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        Ok(())
    }

    // ---- requests ----------------------------------------------------------------------------

    /// An RPC call: JSON body on `api.dropboxapi.com`.
    async fn api_json(
        &self,
        tok: &mut String,
        route: &str,
        body: &Value,
    ) -> Result<HttpReply, CmError> {
        let url = format!("{API}/{route}");
        let bytes = body.to_string().into_bytes();
        self.client
            .call(tok, &|t| {
                HttpRequest::new("POST", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", "application/json")
                    .body(Body::Bytes(bytes.clone()))
            })
            .await
    }

    /// One try of a content-upload call: argument in `Dropbox-API-Arg`, bytes in the body.
    async fn content_attempt(
        &self,
        tok: &mut String,
        route: &str,
        arg: &Value,
        body: Body,
    ) -> Result<Attempt, CmError> {
        let url = format!("{CONTENT}/{route}");
        let arg = ascii_json(arg);
        self.client
            .attempt(tok, &|t| {
                HttpRequest::new("POST", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Dropbox-API-Arg", arg.clone())
                    .header("Content-Type", "application/octet-stream")
                    .body(body.clone())
            })
            .await
    }

    async fn start_session(
        &self,
        tok: &mut String,
        size: u64,
        fingerprint: &str,
        path: &str,
    ) -> Result<DropboxResume, CmError> {
        let mut failures = 0u32;
        let reply = loop {
            match self
                .content_attempt(
                    tok,
                    "files/upload_session/start",
                    &json!({ "close": false }),
                    Body::Empty,
                )
                .await?
            {
                Attempt::Reply(r) => break r,
                Attempt::Transient { message, wait } => {
                    failures += 1;
                    self.client.back_off(failures, wait, message).await?;
                }
            }
        };
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        let session_id = reply
            .json()
            .and_then(|v| v["session_id"].as_str().map(str::to_owned))
            .ok_or_else(|| CmError::Internal {
                message: "Dropbox did not return an upload session".to_owned(),
            })?;
        Ok(DropboxResume {
            session_id,
            offset: 0,
            size,
            fingerprint: fingerprint.to_owned(),
            path: path.to_owned(),
        })
    }

    async fn upload_with(
        &self,
        file: &Path,
        target: &Target,
        resume: Option<ResumeState>,
        progress: Progress,
        link_mode: LinkMode,
    ) -> Result<Uploaded, CmError> {
        let (size, fingerprint) = file_fingerprint(file).await?;
        if target.file_name.trim().is_empty() {
            return Err(CmError::InvalidInput {
                message: "The file has no name".to_owned(),
            });
        }
        let path = self.remote_path(target);
        let mut tok = self.client.tokens.access_token(None).await?;

        let saved = resume
            .and_then(|r| serde_json::from_value::<DropboxResume>(r.0).ok())
            .filter(|s| s.size == size && s.fingerprint == fingerprint && s.path == path);
        let mut st = match saved {
            Some(s) => s,
            None => {
                let s = self
                    .start_session(&mut tok, size, &fingerprint, &path)
                    .await?;
                progress.save_resume(&s.to_state());
                s
            }
        };
        progress.report(st.offset, size);
        let mut failures = 0u32;
        let mut restarts = 0u32;
        let mut corrections = 0u32;

        let done: HttpReply = loop {
            if progress.is_cancelled() {
                return Err(CmError::Internal {
                    message: "Upload cancelled".to_owned(),
                });
            }
            let remaining = size.saturating_sub(st.offset);
            let finishing = remaining <= self.cfg.chunk_size;
            let len = remaining.min(self.cfg.chunk_size);
            let body = if len == 0 {
                Body::Empty
            } else {
                Body::FileRange {
                    path: file.to_path_buf(),
                    offset: st.offset,
                    len,
                }
            };
            let cursor = json!({ "session_id": st.session_id, "offset": st.offset });
            let (route, arg) = if finishing {
                (
                    "files/upload_session/finish",
                    json!({
                        "cursor": cursor,
                        "commit": { "path": st.path, "mode": "add", "autorename": true, "mute": true },
                    }),
                )
            } else {
                (
                    "files/upload_session/append_v2",
                    json!({ "cursor": cursor, "close": false }),
                )
            };
            let reply = match self.content_attempt(&mut tok, route, &arg, body).await? {
                Attempt::Transient { message, wait } => {
                    failures += 1;
                    self.client.back_off(failures, wait, message).await?;
                    // The chunk may have arrived: sending it again at the same offset is
                    // answered with `incorrect_offset` and the offset to continue from.
                    continue;
                }
                Attempt::Reply(r) => r,
            };
            match reply.status {
                200 if finishing => break reply,
                200 => {
                    failures = 0;
                    st.offset += len;
                    progress.save_resume(&st.to_state());
                    progress.report(st.offset, size);
                }
                409 => {
                    let summary = error_summary(&reply);
                    if summary.contains("incorrect_offset") {
                        corrections += 1;
                        let correct = correct_offset(&reply).filter(|n| *n <= size);
                        match correct {
                            Some(n) if corrections <= 8 => {
                                if n > st.offset {
                                    failures = 0;
                                }
                                st.offset = n;
                                progress.save_resume(&st.to_state());
                                progress.report(st.offset, size);
                            }
                            _ => {
                                return Err(CmError::Network {
                                    message: "Dropbox keeps asking for another offset".to_owned(),
                                })
                            }
                        }
                    } else if summary.contains("closed") && finishing {
                        // The finish landed and only its answer was lost.
                        match self.landed(&mut tok, &st.path, size).await? {
                            Some(r) => break r,
                            None => {
                                restarts += 1;
                                st = self
                                    .restart(
                                        &mut tok,
                                        size,
                                        &fingerprint,
                                        &path,
                                        restarts,
                                        &progress,
                                    )
                                    .await?;
                            }
                        }
                    } else if summary.contains("not_found") || summary.contains("closed") {
                        // The session expired.
                        restarts += 1;
                        st = self
                            .restart(&mut tok, size, &fingerprint, &path, restarts, &progress)
                            .await?;
                    } else {
                        return Err(api_error(&reply));
                    }
                }
                _ => return Err(api_error(&reply)),
            }
        };

        let mut remote = parse_remote(&done)?;
        if remote.size.is_none() {
            if let Some(r) = self.metadata(&mut tok, &st.path).await? {
                remote.size = r.size;
            }
        }
        let remote_size = remote.size.unwrap_or(0);
        if remote_size != size {
            return Err(CmError::Internal {
                message: format!(
                    "Dropbox stored {remote_size} bytes but the file has {size}; the upload is not complete"
                ),
            });
        }
        progress.report(size, size);

        let (share_link, note) = match link_mode {
            LinkMode::None => (None, None),
            LinkMode::Anyone => self.share(&mut tok, &remote, true).await,
            LinkMode::Organization => self.share(&mut tok, &remote, false).await,
        };
        Ok(Uploaded {
            remote_id: remote.id,
            share_link,
            bytes: size,
            note,
        })
    }

    async fn restart(
        &self,
        tok: &mut String,
        size: u64,
        fingerprint: &str,
        path: &str,
        restarts: u32,
        progress: &Progress,
    ) -> Result<DropboxResume, CmError> {
        if restarts > 2 {
            return Err(CmError::Network {
                message: "Dropbox keeps losing the upload session".to_owned(),
            });
        }
        let s = self.start_session(tok, size, fingerprint, path).await?;
        progress.save_resume(&s.to_state());
        progress.report(0, size);
        Ok(s)
    }

    /// The file at `path`, when it exists.
    async fn metadata(&self, tok: &mut String, path: &str) -> Result<Option<Remote>, CmError> {
        let reply = self
            .api_json(tok, "files/get_metadata", &json!({ "path": path }))
            .await?;
        match reply.status {
            200 => Ok(Some(parse_remote(&reply)?)),
            409 => Ok(None),
            _ => Err(api_error(&reply)),
        }
    }

    /// After "session closed" on a repeated `finish`: the file is there with the right size.
    async fn landed(
        &self,
        tok: &mut String,
        path: &str,
        size: u64,
    ) -> Result<Option<HttpReply>, CmError> {
        let reply = self
            .api_json(tok, "files/get_metadata", &json!({ "path": path }))
            .await?;
        if reply.status == 200 && parse_remote(&reply)?.size == Some(size) {
            Ok(Some(reply))
        } else {
            Ok(None)
        }
    }

    /// STO-4. `public`: a viewer link for anyone, falling back to a team-only link (with a note)
    /// when the team forbids public links. A refusal is a note, never a failed upload.
    async fn share(
        &self,
        tok: &mut String,
        remote: &Remote,
        public: bool,
    ) -> (Option<String>, Option<String>) {
        let first = if public { "public" } else { "team_only" };
        match self.create_link(tok, remote, first).await {
            Ok(link) => (Some(link), None),
            Err(LinkError::Forbidden) if public => {
                match self.create_link(tok, remote, "team_only").await {
                    Ok(link) => (
                        Some(link),
                        Some(
                            "Your team does not allow public links, so the link works for people in your team only."
                                .to_owned(),
                        ),
                    ),
                    Err(_) => (
                        None,
                        Some(
                            "Your team does not allow public links, so the file has no share link."
                                .to_owned(),
                        ),
                    ),
                }
            }
            Err(LinkError::Forbidden) => (
                None,
                Some(
                    "Your account cannot make team-only links, so the file has no share link."
                        .to_owned(),
                ),
            ),
            Err(LinkError::Other(e)) => (
                None,
                Some(format!(
                    "The file was uploaded but the share link failed: {e}"
                )),
            ),
        }
    }

    async fn create_link(
        &self,
        tok: &mut String,
        remote: &Remote,
        visibility: &str,
    ) -> Result<String, LinkError> {
        let settings = if visibility == "public" {
            json!({ "requested_visibility": "public", "audience": "public", "access": "viewer" })
        } else {
            json!({ "requested_visibility": "team_only", "audience": "team", "access": "viewer" })
        };
        let reply = self
            .api_json(
                tok,
                "sharing/create_shared_link_with_settings",
                &json!({ "path": remote.id, "settings": settings }),
            )
            .await
            .map_err(LinkError::Other)?;
        match reply.status {
            200 => reply
                .json()
                .and_then(|v| v["url"].as_str().map(str::to_owned))
                .ok_or_else(|| {
                    LinkError::Other(CmError::Internal {
                        message: "Dropbox sent no link".to_owned(),
                    })
                }),
            409 if error_summary(&reply).contains("shared_link_already_exists") => {
                self.existing_link(tok, remote).await
            }
            // Policy refusals: the team forbids this kind of link.
            403 => Err(LinkError::Forbidden),
            409 if error_summary(&reply).contains("settings_error")
                || error_summary(&reply).contains("not_authorized") =>
            {
                Err(LinkError::Forbidden)
            }
            _ => Err(LinkError::Other(api_error(&reply))),
        }
    }

    async fn existing_link(&self, tok: &mut String, remote: &Remote) -> Result<String, LinkError> {
        let reply = self
            .api_json(
                tok,
                "sharing/list_shared_links",
                &json!({ "path": remote.id, "direct_only": true }),
            )
            .await
            .map_err(LinkError::Other)?;
        if reply.status != 200 {
            return Err(LinkError::Other(api_error(&reply)));
        }
        reply
            .json()
            .and_then(|v| v["links"][0]["url"].as_str().map(str::to_owned))
            .ok_or_else(|| {
                LinkError::Other(CmError::Internal {
                    message: "Dropbox says a link exists but sent none".to_owned(),
                })
            })
    }
}

enum LinkError {
    Forbidden,
    Other(CmError),
}

#[async_trait]
impl StorageProvider for DropboxProvider {
    fn id(&self) -> &'static str {
        "dropbox"
    }

    async fn upload(
        &self,
        file: &Path,
        target: &Target,
        resume: Option<ResumeState>,
        progress: Progress,
    ) -> Result<Uploaded, CmError> {
        self.upload_with(file, target, resume, progress, self.cfg.link_mode)
            .await
    }
}

// ---- helpers ---------------------------------------------------------------------------------

/// JSON for the `Dropbox-API-Arg` header: compact, with every non-ASCII character as `\uXXXX`
/// (the header must be ASCII).
fn ascii_json(v: &Value) -> String {
    let mut out = String::new();
    for c in v.to_string().chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// `error_summary` of a Dropbox error (`path/insufficient_space/..`), or the body text when the
/// answer is not JSON.
fn error_summary(reply: &HttpReply) -> String {
    match reply.json() {
        Some(v) => v["error_summary"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| v["error"].to_string()),
        None => String::from_utf8_lossy(&reply.body).trim().to_owned(),
    }
}

/// `correct_offset` anywhere in the error (`append_v2` has it at the top, `finish` nests it).
fn correct_offset(reply: &HttpReply) -> Option<u64> {
    fn find(v: &Value) -> Option<u64> {
        match v {
            Value::Object(m) => m
                .get("correct_offset")
                .and_then(Value::as_u64)
                .or_else(|| m.values().find_map(find)),
            _ => None,
        }
    }
    find(&reply.json()?["error"])
}

/// A non-retryable error status as the shared error: `platform_rejected` with Dropbox's message
/// and tag (for example `path/insufficient_space`).
fn api_error(reply: &HttpReply) -> CmError {
    let summary = error_summary(reply);
    let tag: String = summary
        .trim_end_matches('.')
        .trim_end_matches('/')
        .chars()
        .take(120)
        .collect();
    let message = if summary.contains("insufficient_space") {
        "your Dropbox is full".to_owned()
    } else if summary.contains("missing_scope") {
        "the app is missing a permission; reconnect the account".to_owned()
    } else if tag.is_empty() {
        format!("HTTP {}", reply.status)
    } else {
        tag.clone()
    };
    CmError::PlatformRejected {
        message: format!("Dropbox: {}", cm_jobs::scrub_secrets(&message)),
        platform_code: Some(if tag.is_empty() {
            reply.status.to_string()
        } else {
            tag
        }),
    }
}

fn parse_remote(reply: &HttpReply) -> Result<Remote, CmError> {
    let v = reply.json().ok_or_else(|| CmError::Internal {
        message: "Dropbox sent an unreadable answer".to_owned(),
    })?;
    let id = v["id"].as_str().ok_or_else(|| CmError::Internal {
        message: "Dropbox sent no file id".to_owned(),
    })?;
    Ok(Remote {
        id: id.to_owned(),
        size: v["size"].as_u64(),
    })
}

#[cfg(test)]
mod tests;
