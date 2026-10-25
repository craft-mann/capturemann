//! Google Drive provider (STO-2, STO-4): the Drive v3 resumable upload protocol with the
//! `drive.file` scope.
//!
//! * One upload is a session (`uploadType=resumable`) that takes the file in chunks (multiples of
//!   256 KiB, 8 MiB by default). After every chunk the [`ResumeState`] (session URI, confirmed
//!   offset, size and a file fingerprint) goes to [`Progress::save_resume`], so a retry or a
//!   restart continues from the last confirmed byte; a changed file starts over.
//! * After a dropped connection the next step is always a status query (`bytes */total`), never a
//!   blind resend: the chunk may have landed.
//! * 429 and 5xx back off per `Retry-After`; when the wait is long or the retries run out the
//!   result is `network` (the job retries later). 401 asks the [`TokenSource`] once more, then
//!   `auth_expired`. Other 4xx (quota, policy) are `platform_rejected` with Drive's message.
//! * `drive.file` only sees files the app created, so the root folder ("CaptureMann") and the
//!   sub-folders of [`Target::folder`] are found or created by name and their ids cached.
//! * The share link (STO-4) is a `reader`/`anyone` permission and the file's `webViewLink`. If
//!   the organisation forbids it the upload still succeeds, without a link and with a note.
//! * The remote size must equal the local size when the upload ends (STO-2.AC2).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cm_core::CmError;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::http::{Body, HttpReply, HttpRequest, StorageHttp};
use crate::pattern::numbered_names;
use crate::{FolderClaim, Progress, ResumeState, StorageProvider, Target, TokenSource, Uploaded};

const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
/// Drive takes chunks in multiples of this.
pub const CHUNK_UNIT: u64 = 256 * 1024;
pub const DEFAULT_CHUNK: u64 = 8 * 1024 * 1024;
const FILE_FIELDS: &str = "id,size,webViewLink";
/// Drive ids are letters, digits, `-` and `_`; anything else in an id is escaped.
const ID_SET: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_');

/// STO-4: the link made after an upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkMode {
    /// Anyone with the link can view.
    #[default]
    Anyone,
    /// Only people in the account's organisation (OneDrive work accounts, Dropbox teams). Google
    /// Drive has no such mode here: the file gets no link and a note says so.
    Organization,
    /// No link; no permission is changed.
    None,
}

#[derive(Debug, Clone)]
pub struct DriveConfig {
    /// Name of the folder everything goes below, created in "My Drive" when missing.
    pub root_folder_name: String,
    /// The id of that folder when it is already known.
    pub root_folder_id: Option<String>,
    pub link_mode: LinkMode,
    /// Rounded down to a multiple of 256 KiB (at least one unit).
    pub chunk_size: u64,
    /// Retries of one request (and of consecutive failed chunks) before giving up as `network`.
    pub max_retries: u32,
    pub backoff_base: Duration,
    pub max_backoff: Duration,
    /// A `Retry-After` longer than this is not waited for in place; the error is `network` and
    /// the job queue retries later.
    pub max_inline_wait: Duration,
}

impl Default for DriveConfig {
    fn default() -> Self {
        DriveConfig {
            root_folder_name: "CaptureMann".to_owned(),
            root_folder_id: None,
            link_mode: LinkMode::Anyone,
            chunk_size: DEFAULT_CHUNK,
            max_retries: 5,
            backoff_base: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
            max_inline_wait: Duration::from_secs(60),
        }
    }
}

/// What is saved between attempts (the `ResumeState` JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DriveResume {
    session_uri: String,
    /// Bytes Drive has confirmed.
    offset: u64,
    size: u64,
    fingerprint: String,
    parent: String,
    name: String,
    /// The file whose content this session replaces (STO-5); `None` creates a new file.
    #[serde(default)]
    file_id: Option<String>,
}

impl DriveResume {
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

struct Remote {
    id: String,
    size: Option<u64>,
    link: Option<String>,
}

pub struct GoogleDriveProvider {
    http: Arc<dyn StorageHttp>,
    tokens: Arc<dyn TokenSource>,
    cfg: DriveConfig,
    folders: Mutex<HashMap<String, String>>,
    folder_lock: tokio::sync::Mutex<()>,
}

impl GoogleDriveProvider {
    pub fn new(
        http: Arc<dyn StorageHttp>,
        tokens: Arc<dyn TokenSource>,
        mut cfg: DriveConfig,
    ) -> Self {
        cfg.chunk_size = (cfg.chunk_size / CHUNK_UNIT).max(1) * CHUNK_UNIT;
        GoogleDriveProvider {
            http,
            tokens,
            cfg,
            folders: Mutex::new(HashMap::new()),
            folder_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// The id of the root folder, once it is known (the daemon may store it in the destination).
    pub fn root_folder_id(&self) -> Option<String> {
        self.cfg.root_folder_id.clone().or_else(|| {
            self.folders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&cache_key("root", &self.cfg.root_folder_name))
                .cloned()
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
            .upload_with(&path, &target, None, Progress::none(), LinkMode::None, None)
            .await;
        let _ = std::fs::remove_file(&path);
        let up = result?;
        let mut tok = self.tokens.access_token(None).await?;
        let url = format!("{API}/files/{}", up.remote_id);
        let reply = self
            .call(&mut tok, &|t| {
                HttpRequest::new("DELETE", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        Ok(())
    }

    /// The address of the signed-in Google account (Drive `about`, which the `drive.file` scope
    /// allows). Shown in Settings after connecting.
    pub async fn account_email(&self) -> Result<String, CmError> {
        let mut tok = self.tokens.access_token(None).await?;
        let url = format!("{API}/about?fields=user(emailAddress)");
        let reply = self
            .call(&mut tok, &|t| {
                HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        reply
            .json()
            .and_then(|v| v["user"]["emailAddress"].as_str().map(str::to_owned))
            .filter(|e| !e.is_empty())
            .ok_or_else(|| CmError::Internal {
                message: "Google Drive did not say which account this is".to_owned(),
            })
    }

    /// Finds or creates the root folder (`root_folder_name` in "My Drive") and returns its id, so
    /// the connect flow can create "CaptureMann" and keep the id in the destination.
    pub async fn ensure_root_folder(&self) -> Result<String, CmError> {
        let mut tok = self.tokens.access_token(None).await?;
        self.root_id(&mut tok).await
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
                        message: format!("Cannot reach Google Drive: {}", e.0),
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
                    let wait = retry_after(&reply);
                    return Ok(Attempt::Transient {
                        message: format!(
                            "Google Drive is busy or not responding (HTTP {})",
                            reply.status
                        ),
                        wait,
                    });
                }
                403 if is_rate_limit(&reply) => {
                    return Ok(Attempt::Transient {
                        message: "Google Drive is rate limiting requests (HTTP 403)".to_owned(),
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

    /// Sleeps before a retry, or gives up as `network` when the retries are used up or the
    /// server asked for a long wait.
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

    // ---- folders -----------------------------------------------------------------------------

    async fn root_id(&self, tok: &mut String) -> Result<String, CmError> {
        if let Some(id) = &self.cfg.root_folder_id {
            return Ok(id.clone());
        }
        self.find_or_create_folder(tok, "root", &self.cfg.root_folder_name.clone())
            .await
    }

    /// Folder id for `Target::folder`, below the root. A leading segment equal to the root
    /// folder's name is skipped (the default pattern starts with `CaptureMann`).
    async fn ensure_folder_path(&self, tok: &mut String, folder: &str) -> Result<String, CmError> {
        let mut parent = self.root_id(tok).await?;
        let mut segments: Vec<&str> = folder
            .split(['/', '\\'])
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "." && *s != "..")
            .collect();
        if segments.first().copied() == Some(self.cfg.root_folder_name.as_str()) {
            segments.remove(0);
        }
        for seg in segments {
            parent = self.find_or_create_folder(tok, &parent, seg).await?;
        }
        Ok(parent)
    }

    async fn find_or_create_folder(
        &self,
        tok: &mut String,
        parent: &str,
        name: &str,
    ) -> Result<String, CmError> {
        let key = cache_key(parent, name);
        let cached = |s: &Self| {
            s.folders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&key)
                .cloned()
        };
        if let Some(id) = cached(self) {
            return Ok(id);
        }
        // Two uploads must not both create the same folder.
        let _guard = self.folder_lock.lock().await;
        if let Some(id) = cached(self) {
            return Ok(id);
        }
        let id = match self
            .list_folders(tok, parent, name)
            .await?
            .into_iter()
            .next()
        {
            Some(id) => id,
            None => self.create_folder(tok, parent, name).await?,
        };
        self.folders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, id.clone());
        Ok(id)
    }

    /// The ids of the live folders called `name` in `parent`, oldest first.
    async fn list_folders(
        &self,
        tok: &mut String,
        parent: &str,
        name: &str,
    ) -> Result<Vec<String>, CmError> {
        let q = format!(
            "name='{}' and '{}' in parents and mimeType='{FOLDER_MIME}' and trashed=false",
            escape_q(name),
            escape_q(parent)
        );
        let url = format!(
            "{API}/files?q={}&fields=files(id)&pageSize=10&orderBy=createdTime&spaces=drive",
            utf8_percent_encode(&q, NON_ALPHANUMERIC)
        );
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        Ok(reply
            .json()
            .and_then(|v| {
                v["files"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|f| f["id"].as_str().map(str::to_owned))
                        .collect()
                })
            })
            .unwrap_or_default())
    }

    async fn create_folder(
        &self,
        tok: &mut String,
        parent: &str,
        name: &str,
    ) -> Result<String, CmError> {
        let body = json!({
            "name": name,
            "mimeType": FOLDER_MIME,
            "parents": [parent],
        })
        .to_string()
        .into_bytes();
        let url = format!("{API}/files?fields=id");
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new("POST", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", "application/json; charset=UTF-8")
                    .body(Body::Bytes(body.clone()))
            })
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        reply
            .json()
            .and_then(|v| v["id"].as_str().map(str::to_owned))
            .ok_or_else(|| CmError::Internal {
                message: "Google Drive created a folder but sent no id".to_owned(),
            })
    }

    /// Claims a fresh folder `name` (or `name (2)`, ...) below `base`.
    async fn claim_in(
        &self,
        tok: &mut String,
        base: &str,
        name: &str,
    ) -> Result<FolderClaim, CmError> {
        let parent = self.ensure_folder_path(tok, base).await?;
        // The check and the create happen under the folder lock, so two uploads of this process
        // cannot take the same name.
        let _guard = self.folder_lock.lock().await;
        for candidate in numbered_names(name) {
            if !self
                .list_folders(tok, &parent, &candidate)
                .await?
                .is_empty()
            {
                continue;
            }
            let id = self.create_folder(tok, &parent, &candidate).await?;
            // Drive cannot create a folder only if the name is free. Another client may have
            // created the same name between the check and the create; the older folder wins and
            // the loser removes its own and tries the next name.
            let same = self.list_folders(tok, &parent, &candidate).await?;
            if same.first().map(String::as_str) != Some(id.as_str()) {
                let _ = self.delete_id(tok, &id).await;
                continue;
            }
            self.folders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(cache_key(&parent, &candidate), id.clone());
            return Ok(FolderClaim {
                name: candidate,
                id: Some(id),
            });
        }
        Err(CmError::Internal {
            message: format!("No free folder name for {name} in Google Drive"),
        })
    }

    /// Is `id` a live file in `parent`?
    async fn file_is_in(&self, tok: &mut String, id: &str, parent: &str) -> Result<bool, CmError> {
        let url = format!(
            "{API}/files/{}?fields=id,trashed,parents",
            utf8_percent_encode(id, ID_SET)
        );
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        match reply.status {
            200 => Ok(reply.json().is_some_and(|v| {
                v["trashed"].as_bool() != Some(true)
                    && v["parents"]
                        .as_array()
                        .is_none_or(|p| p.iter().any(|x| x.as_str() == Some(parent)))
            })),
            404 => Ok(false),
            _ => Err(api_error(&reply)),
        }
    }

    /// The oldest live file (not a folder) called `name` in `parent`.
    async fn find_file(
        &self,
        tok: &mut String,
        parent: &str,
        name: &str,
    ) -> Result<Option<String>, CmError> {
        let q = format!(
            "name='{}' and '{}' in parents and mimeType!='{FOLDER_MIME}' and trashed=false",
            escape_q(name),
            escape_q(parent)
        );
        let url = format!(
            "{API}/files?q={}&fields=files(id)&pageSize=10&orderBy=createdTime&spaces=drive",
            utf8_percent_encode(&q, NON_ALPHANUMERIC)
        );
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        Ok(reply
            .json()
            .and_then(|v| v["files"][0]["id"].as_str().map(str::to_owned)))
    }

    /// `files.delete`: permanent, no Trash. A file that is already gone counts as deleted.
    async fn delete_id(&self, tok: &mut String, id: &str) -> Result<(), CmError> {
        let url = format!("{API}/files/{}", utf8_percent_encode(id, ID_SET));
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new("DELETE", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        match reply.status {
            200..=299 | 404 => Ok(()),
            _ => Err(api_error(&reply)),
        }
    }

    // ---- the upload --------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    async fn new_session(
        &self,
        tok: &mut String,
        file: &Path,
        parent: &str,
        name: &str,
        size: u64,
        fingerprint: &str,
        replace_id: Option<&str>,
    ) -> Result<DriveResume, CmError> {
        // A replacement keeps the file's name and place: only the content is new.
        let meta = match replace_id {
            Some(_) => json!({}),
            None => json!({ "name": name, "parents": [parent] }),
        }
        .to_string()
        .into_bytes();
        let (method, url) = match replace_id {
            Some(id) => (
                "PATCH",
                format!(
                    "{UPLOAD}/{}?uploadType=resumable&fields={FILE_FIELDS}",
                    utf8_percent_encode(id, ID_SET)
                ),
            ),
            None => (
                "POST",
                format!("{UPLOAD}?uploadType=resumable&fields={FILE_FIELDS}"),
            ),
        };
        let mime = mime_for(file);
        let reply = self
            .call(tok, &|t| {
                HttpRequest::new(method, url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", "application/json; charset=UTF-8")
                    .header("X-Upload-Content-Type", mime)
                    .header("X-Upload-Content-Length", size.to_string())
                    .body(Body::Bytes(meta.clone()))
            })
            .await?;
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        let session_uri = reply
            .header_value("location")
            .ok_or_else(|| CmError::Internal {
                message: "Google Drive did not return an upload address".to_owned(),
            })?
            .to_owned();
        Ok(DriveResume {
            session_uri,
            offset: 0,
            size,
            fingerprint: fingerprint.to_owned(),
            parent: parent.to_owned(),
            name: name.to_owned(),
            file_id: replace_id.map(str::to_owned),
        })
    }

    async fn query_status(&self, tok: &mut String, st: &DriveResume) -> Result<Status, CmError> {
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

    async fn upload_with(
        &self,
        file: &Path,
        target: &Target,
        resume: Option<ResumeState>,
        progress: Progress,
        link_mode: LinkMode,
        replace_id: Option<&str>,
    ) -> Result<Uploaded, CmError> {
        let (size, fingerprint) = fingerprint(file).await?;
        let name = target.file_name.trim().to_owned();
        if name.is_empty() {
            return Err(CmError::InvalidInput {
                message: "The file has no name".to_owned(),
            });
        }
        let mut tok = self.tokens.access_token(None).await?;
        let parent = self.ensure_folder_path(&mut tok, &target.folder).await?;

        let mut state = resume
            .and_then(|r| serde_json::from_value::<DriveResume>(r.0).ok())
            .filter(|s| {
                s.size == size
                    && s.fingerprint == fingerprint
                    && s.parent == parent
                    && s.name == name
                    && s.file_id.as_deref() == replace_id
            });
        let mut need_sync = state.is_some();
        let mut st = match state.take() {
            Some(s) => s,
            None => {
                let s = self
                    .new_session(
                        &mut tok,
                        file,
                        &parent,
                        &name,
                        size,
                        &fingerprint,
                        replace_id,
                    )
                    .await?;
                progress.save_resume(&s.to_state());
                s
            }
        };
        let mut failures = 0u32;
        let mut restarts = 0u32;

        let done: HttpReply = loop {
            if progress.is_cancelled() {
                return Err(CmError::Internal {
                    message: "Upload cancelled".to_owned(),
                });
            }
            if need_sync {
                match self.query_status(&mut tok, &st).await? {
                    Status::Complete(r) => break r,
                    Status::Offset(n) if n <= size => st.offset = n,
                    Status::Offset(_) | Status::Gone => {
                        restarts += 1;
                        if restarts > 2 {
                            return Err(CmError::Network {
                                message: "Google Drive keeps losing the upload session".to_owned(),
                            });
                        }
                        st = self
                            .new_session(
                                &mut tok,
                                file,
                                &parent,
                                &name,
                                size,
                                &fingerprint,
                                replace_id,
                            )
                            .await?;
                        progress.save_resume(&st.to_state());
                    }
                }
                progress.report(st.offset, size);
                need_sync = false;
            }
            let len = self.cfg.chunk_size.min(size - st.offset);
            let range = if size == 0 {
                "bytes */0".to_owned()
            } else {
                format!("bytes {}-{}/{size}", st.offset, st.offset + len - 1)
            };
            let body = if len == 0 {
                Body::Empty
            } else {
                Body::FileRange {
                    path: file.to_path_buf(),
                    offset: st.offset,
                    len,
                }
            };
            let outcome = self
                .attempt(&mut tok, &|t| {
                    HttpRequest::new("PUT", st.session_uri.clone())
                        .header("Authorization", bearer(t))
                        .header("Content-Range", range.clone())
                        .body(body.clone())
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
                200 | 201 => break reply,
                308 => {
                    let n = confirmed(&reply);
                    if n > st.offset {
                        failures = 0;
                    }
                    st.offset = n.min(size);
                    progress.save_resume(&st.to_state());
                    progress.report(st.offset, size);
                }
                // The session is gone: the status query finds out and starts a new one.
                404 | 410 => need_sync = true,
                _ => return Err(api_error(&reply)),
            }
        };

        let mut remote = parse_remote(&done)?;
        if remote.size.is_none() || remote.link.is_none() {
            // Fall back to a metadata read when the response lacked the fields.
            let url = format!("{API}/files/{}?fields={FILE_FIELDS}", remote.id);
            let meta = self
                .call(&mut tok, &|t| {
                    HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
                })
                .await?;
            if meta.status == 200 {
                let again = parse_remote(&meta)?;
                remote.size = remote.size.or(again.size);
                remote.link = remote.link.or(again.link);
            }
        }
        let remote_size = remote.size.unwrap_or(0);
        if remote_size != size {
            return Err(CmError::Internal {
                message: format!(
                    "Google Drive stored {remote_size} bytes but the file has {size}; the upload is not complete"
                ),
            });
        }
        progress.report(size, size);

        let (share_link, note) = match link_mode {
            LinkMode::None => (None, None),
            LinkMode::Organization => (
                None,
                Some("Google Drive has no organisation-only link here, so the file has no share link.".to_owned()),
            ),
            LinkMode::Anyone => self.share(&mut tok, &remote).await,
        };
        Ok(Uploaded {
            remote_id: remote.id,
            share_link,
            bytes: size,
            note,
        })
    }

    /// STO-4: "anyone with the link can view". A refusal is a note, never a failed upload.
    async fn share(&self, tok: &mut String, remote: &Remote) -> (Option<String>, Option<String>) {
        let url = format!("{API}/files/{}/permissions?fields=id", remote.id);
        let body = json!({ "role": "reader", "type": "anyone" })
            .to_string()
            .into_bytes();
        let result = self
            .call(tok, &|t| {
                HttpRequest::new("POST", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", "application/json; charset=UTF-8")
                    .body(Body::Bytes(body.clone()))
            })
            .await;
        match result {
            Ok(r) if (200..300).contains(&r.status) => match &remote.link {
                Some(link) => (Some(link.clone()), None),
                None => (
                    None,
                    Some("The file was uploaded but Google Drive sent no link.".to_owned()),
                ),
            },
            Ok(r) if r.status == 403 => (
                None,
                Some(
                    "Your organisation does not allow public links, so the file has no share link."
                        .to_owned(),
                ),
            ),
            Ok(r) => (
                None,
                Some(format!(
                    "The file was uploaded but the share link failed: {}",
                    api_error(&r)
                )),
            ),
            Err(e) => (
                None,
                Some(format!(
                    "The file was uploaded but the share link failed: {e}"
                )),
            ),
        }
    }
}

#[async_trait]
impl StorageProvider for GoogleDriveProvider {
    fn id(&self) -> &'static str {
        "gdrive"
    }

    async fn upload(
        &self,
        file: &Path,
        target: &Target,
        resume: Option<ResumeState>,
        progress: Progress,
    ) -> Result<Uploaded, CmError> {
        self.upload_with(file, target, resume, progress, self.cfg.link_mode, None)
            .await
    }

    async fn claim_bundle_folder(&self, base: &str, name: &str) -> Result<FolderClaim, CmError> {
        let mut tok = self.tokens.access_token(None).await?;
        self.claim_in(&mut tok, base, name).await
    }

    async fn reuse_bundle_folder(
        &self,
        base: &str,
        claim: &FolderClaim,
    ) -> Result<FolderClaim, CmError> {
        let mut tok = self.tokens.access_token(None).await?;
        let parent = self.ensure_folder_path(&mut tok, base).await?;
        let Some(id) = &claim.id else {
            return Ok(claim.clone());
        };
        // Is the remembered folder still there, and still in this place?
        let url = format!(
            "{API}/files/{}?fields=id,trashed,parents",
            utf8_percent_encode(id, ID_SET)
        );
        let reply = self
            .call(&mut tok, &|t| {
                HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        let alive = reply.status == 200
            && reply.json().is_some_and(|v| {
                v["trashed"].as_bool() != Some(true)
                    && v["parents"]
                        .as_array()
                        .is_none_or(|p| p.iter().any(|x| x.as_str() == Some(parent.as_str())))
            });
        if alive {
            self.folders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(cache_key(&parent, &claim.name), id.clone());
            return Ok(claim.clone());
        }
        if reply.status != 200 && reply.status != 404 {
            return Err(api_error(&reply));
        }
        // Gone (deleted or moved): find the folder by name, or make it again, so the job can
        // go on instead of failing for good. The record then holds the new id.
        let id = self
            .find_or_create_folder(&mut tok, &parent, &claim.name)
            .await?;
        Ok(FolderClaim {
            name: claim.name.clone(),
            id: Some(id),
        })
    }

    async fn replace(
        &self,
        file: &Path,
        target: &Target,
        existing_remote_id: Option<&str>,
        progress: Progress,
    ) -> Result<Uploaded, CmError> {
        let name = target.file_name.trim().to_owned();
        let mut tok = self.tokens.access_token(None).await?;
        let parent = self.ensure_folder_path(&mut tok, &target.folder).await?;
        // The id from the first upload, if that file is still alive and still in this folder;
        // otherwise the file of that name in the folder (the oldest one); otherwise none.
        let mut id = None;
        if let Some(known) = existing_remote_id {
            if self.file_is_in(&mut tok, known, &parent).await? {
                id = Some(known.to_owned());
            }
        }
        if id.is_none() {
            id = self.find_file(&mut tok, &parent, &name).await?;
        }
        // The share link and permissions of an existing file stay as they are.
        self.upload_with(file, target, None, progress, LinkMode::None, id.as_deref())
            .await
    }

    async fn delete(&self, remote_id: &str) -> Result<(), CmError> {
        let mut tok = self.tokens.access_token(None).await?;
        self.delete_id(&mut tok, remote_id).await
    }
}

// ---- helpers ---------------------------------------------------------------------------------

pub(crate) fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn reconnect() -> CmError {
    CmError::AuthExpired {
        message:
            "Google Drive no longer accepts this account. Reconnect it in Settings → Destinations."
                .to_owned(),
    }
}

fn cache_key(parent: &str, name: &str) -> String {
    format!("{parent}\u{0}{name}")
}

fn escape_q(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

pub(crate) fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("gif") => "image/gif",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("json") => "application/json",
        Some("srt" | "txt" | "vtt") => "text/plain",
        _ => "application/octet-stream",
    }
}

/// `(size, fingerprint)`: size, modification time and a hash of the first MiB, so a file that was
/// edited or replaced starts a new session.
pub(crate) async fn fingerprint(path: &Path) -> Result<(u64, String), CmError> {
    let path: PathBuf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let fail = |e: std::io::Error| crate::io_error("Cannot read", &path, &e);
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
        let hash = format!("{:x}", Sha256::digest(&head));
        Ok((meta.len(), format!("{}:{mtime}:{hash}", meta.len())))
    })
    .await
    .map_err(|e| CmError::Internal {
        message: format!("worker failed: {e}"),
    })?
}

/// Bytes Drive has confirmed: `Range: bytes=0-N` means N+1; no header means none.
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
        Some("rateLimitExceeded" | "userRateLimitExceeded" | "sharingRateLimitExceeded")
    )
}

/// A non-retryable error status as the shared error: `platform_rejected` with Drive's own
/// message and reason (for example `storageQuotaExceeded`).
fn api_error(reply: &HttpReply) -> CmError {
    let (message, reason) = error_parts(reply);
    let message = message.unwrap_or_else(|| format!("HTTP {}", reply.status));
    CmError::PlatformRejected {
        message: format!("Google Drive: {}", cm_jobs::scrub_secrets(&message)),
        platform_code: Some(reason.unwrap_or_else(|| reply.status.to_string())),
    }
}

fn parse_remote(reply: &HttpReply) -> Result<Remote, CmError> {
    let v = reply.json().ok_or_else(|| CmError::Internal {
        message: "Google Drive sent an unreadable answer".to_owned(),
    })?;
    let id = v["id"].as_str().ok_or_else(|| CmError::Internal {
        message: "Google Drive sent no file id".to_owned(),
    })?;
    Ok(Remote {
        id: id.to_owned(),
        size: v["size"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| v["size"].as_u64()),
        link: v["webViewLink"].as_str().map(str::to_owned),
    })
}

#[cfg(test)]
mod tests;
