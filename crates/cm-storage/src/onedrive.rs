//! OneDrive provider (STO-2, STO-4): Microsoft Graph with the `Files.ReadWrite` scope.
//!
//! * A file up to [`OneDriveConfig::simple_put_max`] (4 MB, Graph's limit) goes in one
//!   `PUT .../content`; anything larger uses an upload session (`createUploadSession` on the
//!   target path) and is sent in chunks that are multiples of 320 KiB (327,680 bytes), under
//!   60 MiB, 10 MiB by default (STO-2.AC6), with `Content-Range`. Chunks go one after the other.
//! * The session URL is pre-authenticated: it never gets an `Authorization` header.
//! * After every confirmed chunk the [`ResumeState`] (session URL, confirmed offset, size, file
//!   fingerprint) goes to [`Progress::save_resume`]. After a dropped connection, a 416 or a
//!   restart the next step is a status query (`GET` the session URL, `nextExpectedRanges`),
//!   never a blind resend; 404 means the session expired and a new one starts.
//! * 429 and 5xx back off per `Retry-After`; when the wait is long or the retries run out the
//!   result is `network` (the job retries later). 401 asks the [`TokenSource`] once more, then
//!   `auth_expired`. 507 (the account is full), 403 and other 4xx are `platform_rejected` with
//!   Graph's message and code.
//! * Folders are found or created by name below the root folder ("CaptureMann") and their ids
//!   cached.
//! * The link (STO-4) is `createLink` type `view`: scope `anonymous` for [`LinkMode::Anyone`],
//!   `organization` for [`LinkMode::Organization`]. When the tenant forbids anonymous links the
//!   organisation link is made instead and a note says so; if that fails too the upload still
//!   succeeds, without a link and with a note.
//! * The remote size must equal the local size when the upload ends (STO-2.AC2).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cm_core::CmError;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::cloud::{folder_segments, Attempt, Client, Retry};
use crate::gdrive::{bearer, fingerprint as file_fingerprint, mime_for};
use crate::http::{Body, HttpReply, HttpRequest, StorageHttp};
use crate::{LinkMode, Progress, ResumeState, StorageProvider, Target, TokenSource, Uploaded};

const GRAPH: &str = "https://graph.microsoft.com/v1.0/me/drive";
const ME_URL: &str =
    "https://graph.microsoft.com/v1.0/me?$select=userPrincipalName,mail,displayName";
const DRIVE_INFO_URL: &str = "https://graph.microsoft.com/v1.0/me/drive?$select=driveType,owner";
/// Graph takes chunks in multiples of this.
pub const CHUNK_UNIT: u64 = 320 * 1024;
pub const DEFAULT_CHUNK: u64 = 10 * 1024 * 1024;
/// A chunk must stay under 60 MiB (60 MiB is exactly 192 units).
const MAX_CHUNK: u64 = 191 * CHUNK_UNIT;
/// The largest file Graph takes in one `PUT .../content`.
pub const SIMPLE_PUT_MAX: u64 = 4_000_000;

/// Path segments and ids: everything but the unreserved characters and `!` (OneDrive personal
/// ids contain it).
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'!');

fn seg(s: &str) -> String {
    utf8_percent_encode(s, SEGMENT).to_string()
}

/// Who signed in to OneDrive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneDriveAccount {
    pub label: String,
    /// A work or school account (`Some(true)`), a personal Microsoft account (`Some(false)`), or
    /// not known.
    pub work: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct OneDriveConfig {
    /// Name of the folder everything goes below, created in the drive root when missing.
    pub root_folder_name: String,
    /// The id of that folder when it is already known.
    pub root_folder_id: Option<String>,
    pub link_mode: LinkMode,
    /// Rounded down to a multiple of 320 KiB, at least one unit and under 60 MiB.
    pub chunk_size: u64,
    /// Files up to this size use the simple `PUT` (at most [`SIMPLE_PUT_MAX`]).
    pub simple_put_max: u64,
    pub retry: Retry,
}

impl Default for OneDriveConfig {
    fn default() -> Self {
        OneDriveConfig {
            root_folder_name: "CaptureMann".to_owned(),
            root_folder_id: None,
            link_mode: LinkMode::Anyone,
            chunk_size: DEFAULT_CHUNK,
            simple_put_max: SIMPLE_PUT_MAX,
            retry: Retry::default(),
        }
    }
}

/// What is saved between attempts (the `ResumeState` JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OneDriveResume {
    upload_url: String,
    /// Bytes Graph has confirmed.
    offset: u64,
    size: u64,
    fingerprint: String,
    parent: String,
    name: String,
}

impl OneDriveResume {
    fn to_state(&self) -> ResumeState {
        ResumeState(serde_json::to_value(self).unwrap_or(Value::Null))
    }
}

enum Status {
    Offset(u64),
    Gone,
}

struct Remote {
    id: String,
    size: Option<u64>,
}

pub struct OneDriveProvider {
    client: Client,
    cfg: OneDriveConfig,
    folders: Mutex<HashMap<String, String>>,
    folder_lock: tokio::sync::Mutex<()>,
}

impl OneDriveProvider {
    pub fn new(
        http: Arc<dyn StorageHttp>,
        tokens: Arc<dyn TokenSource>,
        mut cfg: OneDriveConfig,
    ) -> Self {
        cfg.chunk_size =
            (cfg.chunk_size / CHUNK_UNIT).clamp(1, MAX_CHUNK / CHUNK_UNIT) * CHUNK_UNIT;
        cfg.simple_put_max = cfg.simple_put_max.min(SIMPLE_PUT_MAX);
        OneDriveProvider {
            client: Client {
                http,
                tokens,
                retry: cfg.retry.clone(),
                service: "OneDrive",
            },
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

    /// Who signed in and what kind of account it is (the connect flow shows the label and keeps it
    /// as the destination's label). The label is the mailbox address or the user principal name
    /// from `/me`; `/me` needs a permission the `Files.ReadWrite` scope may not include, so when
    /// Graph refuses it the drive's owner is used, and "OneDrive account" when even that is
    /// silent. Only a lost sign-in or an unreachable server is an error.
    pub async fn account(&self) -> Result<OneDriveAccount, CmError> {
        let mut tok = self.client.tokens.access_token(None).await?;
        let me = self
            .client
            .call(&mut tok, &|t| {
                HttpRequest::new("GET", ME_URL).header("Authorization", bearer(t))
            })
            .await?;
        let first = |v: &Value, keys: &[&str]| {
            keys.iter()
                .filter_map(|k| v[*k].as_str())
                .map(str::trim)
                .find(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let mut label = (me.status == 200)
            .then(|| me.json())
            .flatten()
            .and_then(|v| first(&v, &["mail", "userPrincipalName"]));
        let drive = self
            .client
            .call(&mut tok, &|t| {
                HttpRequest::new("GET", DRIVE_INFO_URL).header("Authorization", bearer(t))
            })
            .await?;
        let v = (drive.status == 200)
            .then(|| drive.json())
            .flatten()
            .unwrap_or(Value::Null);
        if label.is_none() {
            let owner = &v["owner"]["user"];
            label = first(owner, &["email", "displayName"]);
        }
        Ok(OneDriveAccount {
            label: label.unwrap_or_else(|| "OneDrive account".to_owned()),
            work: v["driveType"]
                .as_str()
                .map(|t| matches!(t, "business" | "documentLibrary")),
        })
    }

    /// Finds or creates the root folder ("CaptureMann" in the drive root) and returns its id, so
    /// the connect flow can create it and keep the id in the destination.
    pub async fn ensure_root_folder(&self) -> Result<String, CmError> {
        let mut tok = self.client.tokens.access_token(None).await?;
        self.root_id(&mut tok).await
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
        let url = format!("{GRAPH}/items/{}", seg(&up.remote_id));
        let reply = self
            .client
            .call(&mut tok, &|t| {
                HttpRequest::new("DELETE", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        Ok(())
    }

    // ---- folders -----------------------------------------------------------------------------

    async fn root_id(&self, tok: &mut String) -> Result<String, CmError> {
        if let Some(id) = &self.cfg.root_folder_id {
            return Ok(id.clone());
        }
        let name = self.cfg.root_folder_name.clone();
        self.find_or_create_folder(tok, "root", &name).await
    }

    /// Folder id for `Target::folder`, below the root. A leading segment equal to the root
    /// folder's name is skipped (the default pattern starts with `CaptureMann`).
    async fn ensure_folder_path(&self, tok: &mut String, folder: &str) -> Result<String, CmError> {
        let mut parent = self.root_id(tok).await?;
        for s in folder_segments(folder, &self.cfg.root_folder_name) {
            parent = self.find_or_create_folder(tok, &parent, s).await?;
        }
        Ok(parent)
    }

    async fn get_folder(
        &self,
        tok: &mut String,
        parent: &str,
        name: &str,
    ) -> Result<Option<String>, CmError> {
        let url = format!("{GRAPH}/items/{}:/{}", seg(parent), seg(name));
        let reply = self
            .client
            .call(tok, &|t| {
                HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
            })
            .await?;
        match reply.status {
            200 => {
                let v = reply.json().unwrap_or(Value::Null);
                if v["folder"].is_null() {
                    return Err(CmError::PlatformRejected {
                        message: format!(
                            "OneDrive: `{name}` is a file, so a folder of that name cannot be made"
                        ),
                        platform_code: Some("nameAlreadyExists".to_owned()),
                    });
                }
                Ok(v["id"].as_str().map(str::to_owned))
            }
            404 => Ok(None),
            _ => Err(api_error(&reply)),
        }
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
        let id = match self.get_folder(tok, parent, name).await? {
            Some(id) => id,
            None => {
                let body = json!({
                    "name": name,
                    "folder": {},
                    "@microsoft.graph.conflictBehavior": "fail",
                })
                .to_string()
                .into_bytes();
                let url = format!("{GRAPH}/items/{}/children", seg(parent));
                let reply = self
                    .client
                    .call(tok, &|t| {
                        HttpRequest::new("POST", url.clone())
                            .header("Authorization", bearer(t))
                            .header("Content-Type", "application/json")
                            .body(Body::Bytes(body.clone()))
                    })
                    .await?;
                if reply.status == 409 {
                    // Someone made it between the lookup and the create.
                    self.get_folder(tok, parent, name)
                        .await?
                        .ok_or_else(|| api_error(&reply))?
                } else if (200..300).contains(&reply.status) {
                    reply
                        .json()
                        .and_then(|v| v["id"].as_str().map(str::to_owned))
                        .ok_or_else(|| CmError::Internal {
                            message: "OneDrive created a folder but sent no id".to_owned(),
                        })?
                } else {
                    return Err(api_error(&reply));
                }
            }
        };
        self.folders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, id.clone());
        Ok(id)
    }

    // ---- the upload --------------------------------------------------------------------------

    async fn new_session(
        &self,
        tok: &mut String,
        parent: &str,
        name: &str,
        size: u64,
        fingerprint: &str,
    ) -> Result<OneDriveResume, CmError> {
        let body = json!({
            "item": { "@microsoft.graph.conflictBehavior": "rename", "name": name },
            "fileSize": size,
        })
        .to_string()
        .into_bytes();
        let url = format!(
            "{GRAPH}/items/{}:/{}:/createUploadSession",
            seg(parent),
            seg(name)
        );
        let reply = self
            .client
            .call(tok, &|t| {
                HttpRequest::new("POST", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", "application/json")
                    .body(Body::Bytes(body.clone()))
            })
            .await?;
        if reply.status != 200 {
            return Err(api_error(&reply));
        }
        let upload_url = reply
            .json()
            .and_then(|v| v["uploadUrl"].as_str().map(str::to_owned))
            .ok_or_else(|| CmError::Internal {
                message: "OneDrive did not return an upload address".to_owned(),
            })?;
        Ok(OneDriveResume {
            upload_url,
            offset: 0,
            size,
            fingerprint: fingerprint.to_owned(),
            parent: parent.to_owned(),
            name: name.to_owned(),
        })
    }

    async fn query_status(&self, tok: &mut String, st: &OneDriveResume) -> Result<Status, CmError> {
        // No Authorization header on the session URL.
        let reply = self
            .client
            .call(tok, &|_| HttpRequest::new("GET", st.upload_url.clone()))
            .await?;
        match reply.status {
            200 => Ok(Status::Offset(next_expected(&reply).unwrap_or(st.size))),
            404 | 410 => Ok(Status::Gone),
            _ => Err(api_error(&reply)),
        }
    }

    async fn simple_put(
        &self,
        tok: &mut String,
        file: &Path,
        parent: &str,
        name: &str,
        size: u64,
    ) -> Result<HttpReply, CmError> {
        let url = format!(
            "{GRAPH}/items/{}:/{}:/content?@microsoft.graph.conflictBehavior=rename",
            seg(parent),
            seg(name)
        );
        let mime = mime_for(file);
        let body = if size == 0 {
            Body::Empty
        } else {
            Body::FileRange {
                path: file.to_path_buf(),
                offset: 0,
                len: size,
            }
        };
        let reply = self
            .client
            .call(tok, &|t| {
                HttpRequest::new("PUT", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", mime)
                    .body(body.clone())
            })
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        Ok(reply)
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
        let name = target.file_name.trim().to_owned();
        if name.is_empty() {
            return Err(CmError::InvalidInput {
                message: "The file has no name".to_owned(),
            });
        }
        let mut tok = self.client.tokens.access_token(None).await?;
        let parent = self.ensure_folder_path(&mut tok, &target.folder).await?;

        let done: HttpReply = if size <= self.cfg.simple_put_max {
            let r = self
                .simple_put(&mut tok, file, &parent, &name, size)
                .await?;
            progress.report(size, size);
            r
        } else {
            self.upload_session(
                &mut tok,
                file,
                &parent,
                &name,
                size,
                &fingerprint,
                resume,
                &progress,
            )
            .await?
        };

        let mut remote = parse_remote(&done)?;
        if remote.size.is_none() {
            // Fall back to a metadata read when the response lacked the field.
            let url = format!("{GRAPH}/items/{}?select=id,size", seg(&remote.id));
            let meta = self
                .client
                .call(&mut tok, &|t| {
                    HttpRequest::new("GET", url.clone()).header("Authorization", bearer(t))
                })
                .await?;
            if meta.status == 200 {
                remote.size = parse_remote(&meta)?.size;
            }
        }
        let remote_size = remote.size.unwrap_or(0);
        if remote_size != size {
            return Err(CmError::Internal {
                message: format!(
                    "OneDrive stored {remote_size} bytes but the file has {size}; the upload is not complete"
                ),
            });
        }
        progress.report(size, size);

        let (share_link, note) = match link_mode {
            LinkMode::None => (None, None),
            LinkMode::Anyone => self.share(&mut tok, &remote, "anonymous", true).await,
            LinkMode::Organization => self.share(&mut tok, &remote, "organization", false).await,
        };
        Ok(Uploaded {
            remote_id: remote.id,
            share_link,
            bytes: size,
            note,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload_session(
        &self,
        tok: &mut String,
        file: &Path,
        parent: &str,
        name: &str,
        size: u64,
        fingerprint: &str,
        resume: Option<ResumeState>,
        progress: &Progress,
    ) -> Result<HttpReply, CmError> {
        let saved = resume
            .and_then(|r| serde_json::from_value::<OneDriveResume>(r.0).ok())
            .filter(|s| {
                s.size == size
                    && s.fingerprint == fingerprint
                    && s.parent == parent
                    && s.name == name
            });
        let mut need_sync = saved.is_some();
        let mut st = match saved {
            Some(s) => s,
            None => {
                let s = self
                    .new_session(tok, parent, name, size, fingerprint)
                    .await?;
                progress.save_resume(&s.to_state());
                s
            }
        };
        let mut failures = 0u32;
        let mut restarts = 0u32;

        loop {
            if progress.is_cancelled() {
                return Err(CmError::Internal {
                    message: "Upload cancelled".to_owned(),
                });
            }
            if need_sync {
                match self.query_status(tok, &st).await? {
                    Status::Offset(n) if n < size => st.offset = n,
                    Status::Offset(_) | Status::Gone => {
                        restarts += 1;
                        if restarts > 2 {
                            return Err(CmError::Network {
                                message: "OneDrive keeps losing the upload session".to_owned(),
                            });
                        }
                        st = self
                            .new_session(tok, parent, name, size, fingerprint)
                            .await?;
                        progress.save_resume(&st.to_state());
                    }
                }
                progress.report(st.offset, size);
                need_sync = false;
            }
            let len = self.cfg.chunk_size.min(size - st.offset);
            let range = format!("bytes {}-{}/{size}", st.offset, st.offset + len - 1);
            let body = Body::FileRange {
                path: file.to_path_buf(),
                offset: st.offset,
                len,
            };
            // No Authorization header on the session URL.
            let outcome = self
                .client
                .attempt(tok, &|_| {
                    HttpRequest::new("PUT", st.upload_url.clone())
                        .header("Content-Type", "application/octet-stream")
                        .header("Content-Range", range.clone())
                        .body(body.clone())
                })
                .await?;
            let reply = match outcome {
                Attempt::Transient { message, wait } => {
                    failures += 1;
                    self.client.back_off(failures, wait, message).await?;
                    // The chunk may have arrived: ask before sending anything again.
                    need_sync = true;
                    continue;
                }
                Attempt::Reply(r) => r,
            };
            match reply.status {
                200 | 201 => return Ok(reply),
                202 => {
                    let n = next_expected(&reply).unwrap_or(st.offset + len);
                    if n > st.offset {
                        failures = 0;
                    } else {
                        // No progress: do not loop forever on a server that keeps saying so.
                        failures += 1;
                        self.client
                            .back_off(
                                failures,
                                None,
                                "OneDrive did not accept the chunk".to_owned(),
                            )
                            .await?;
                    }
                    st.offset = n.min(size);
                    progress.save_resume(&st.to_state());
                    progress.report(st.offset, size);
                }
                // Out of step, or the session is gone: the status query finds out.
                404 | 410 | 416 => need_sync = true,
                _ => return Err(api_error(&reply)),
            }
        }
    }

    /// STO-4: `createLink` type `view`. `fall_back`: when anonymous links are forbidden, make an
    /// organisation link instead and say so. A refusal is a note, never a failed upload.
    async fn share(
        &self,
        tok: &mut String,
        remote: &Remote,
        scope: &str,
        fall_back: bool,
    ) -> (Option<String>, Option<String>) {
        match self.create_link(tok, remote, scope).await {
            Ok(link) => (Some(link), None),
            Err(LinkError::Forbidden) if fall_back => {
                match self.create_link(tok, remote, "organization").await {
                    Ok(link) => (
                        Some(link),
                        Some(
                            "Your organisation does not allow public links, so the link works for people in your organisation only."
                                .to_owned(),
                        ),
                    ),
                    Err(_) => (
                        None,
                        Some(
                            "Your organisation does not allow public links, so the file has no share link."
                                .to_owned(),
                        ),
                    ),
                }
            }
            Err(LinkError::Forbidden) => (
                None,
                Some(
                    "Your account cannot make organisation links, so the file has no share link."
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
        scope: &str,
    ) -> Result<String, LinkError> {
        let url = format!("{GRAPH}/items/{}/createLink", seg(&remote.id));
        let body = json!({ "type": "view", "scope": scope })
            .to_string()
            .into_bytes();
        let reply = self
            .client
            .call(tok, &|t| {
                HttpRequest::new("POST", url.clone())
                    .header("Authorization", bearer(t))
                    .header("Content-Type", "application/json")
                    .body(Body::Bytes(body.clone()))
            })
            .await
            .map_err(LinkError::Other)?;
        match reply.status {
            200 | 201 => reply
                .json()
                .and_then(|v| v["link"]["webUrl"].as_str().map(str::to_owned))
                .ok_or_else(|| {
                    LinkError::Other(CmError::Internal {
                        message: "OneDrive sent no link".to_owned(),
                    })
                }),
            // Policy refusals: the tenant forbids this kind of link.
            403 | 400 => Err(LinkError::Forbidden),
            _ => Err(LinkError::Other(api_error(&reply))),
        }
    }
}

enum LinkError {
    Forbidden,
    Other(CmError),
}

#[async_trait]
impl StorageProvider for OneDriveProvider {
    fn id(&self) -> &'static str {
        "onedrive"
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

fn cache_key(parent: &str, name: &str) -> String {
    format!("{parent}\u{0}{name}")
}

/// The start of the first `nextExpectedRanges` entry (`"26-"` or `"26-99"`): the bytes Graph has
/// are everything before it. An empty list means it has all of them.
fn next_expected(reply: &HttpReply) -> Option<u64> {
    let v = reply.json()?;
    let ranges = v["nextExpectedRanges"].as_array()?;
    match ranges.first() {
        Some(r) => r.as_str()?.split('-').next()?.parse().ok(),
        None => None,
    }
}

/// A non-retryable error status as the shared error: `platform_rejected` with Graph's own message
/// and code (for example `quotaLimitReached`).
fn api_error(reply: &HttpReply) -> CmError {
    let v = reply.json().unwrap_or(Value::Null);
    let message = v["error"]["message"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("HTTP {}", reply.status));
    let code = v["error"]["code"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| reply.status.to_string());
    let code = if reply.status == 507 && v["error"]["code"].is_null() {
        "insufficientStorage".to_owned()
    } else {
        code
    };
    CmError::PlatformRejected {
        message: format!("OneDrive: {}", cm_jobs::scrub_secrets(&message)),
        platform_code: Some(code),
    }
}

fn parse_remote(reply: &HttpReply) -> Result<Remote, CmError> {
    let v = reply.json().ok_or_else(|| CmError::Internal {
        message: "OneDrive sent an unreadable answer".to_owned(),
    })?;
    let id = v["id"].as_str().ok_or_else(|| CmError::Internal {
        message: "OneDrive sent no file id".to_owned(),
    })?;
    Ok(Remote {
        id: id.to_owned(),
        size: v["size"].as_u64(),
    })
}

#[cfg(test)]
mod tests;
