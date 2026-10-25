//! Destinations for exports and bundles (STO-1). One `StorageProvider` trait; the local
//! provider copies to a folder, external drive or mounted NAS. Cloud providers (STO-2) plug in
//! behind the same trait.

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod cloud;
pub mod destination;
pub mod dropbox;
pub mod gdrive;
pub mod http;
pub mod local;
pub mod onedrive;
pub mod pattern;

#[cfg(any(test, feature = "testing"))]
pub mod fake_drive;
#[cfg(any(test, feature = "testing"))]
pub mod fake_dropbox;
#[cfg(any(test, feature = "testing"))]
pub mod fake_onedrive;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use cm_core::CmError;
use serde::{Deserialize, Serialize};

pub use cloud::Retry;
pub use destination::{
    AutoUpload, CloudAccount, Destination, DestinationStore, Kind, What, DEFAULT_FOLDER_PATTERN,
};
pub use dropbox::{DropboxConfig, DropboxProvider};
pub use gdrive::{DriveConfig, GoogleDriveProvider, LinkMode};
pub use http::{StorageHttp, UreqHttp};
pub use local::LocalProvider;
pub use onedrive::{OneDriveAccount, OneDriveConfig, OneDriveProvider};
pub use pattern::{expand_pattern, numbered_names};

/// Where a file goes: a folder path below the destination root and the file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub folder: String,
    pub file_name: String,
}

/// Provider state that lets an interrupted upload continue (cloud providers; opaque JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumeState(pub serde_json::Value);

/// The result of a finished upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uploaded {
    /// Provider-specific id; the final path for the local provider.
    pub remote_id: String,
    pub share_link: Option<String>,
    pub bytes: u64,
    /// Something the user should know that is not a failure (for example: the organisation
    /// forbids public links, so the file has no link).
    pub note: Option<String>,
}

type ResumeSaver = Arc<dyn Fn(&ResumeState) + Send + Sync>;

/// Progress sink and cancel flag handed to one upload.
#[derive(Clone)]
pub struct Progress {
    report: Arc<dyn Fn(u64, u64) + Send + Sync>,
    cancelled: Arc<AtomicBool>,
    save_resume: Option<ResumeSaver>,
}

impl Progress {
    pub fn new(
        report: impl Fn(u64, u64) + Send + Sync + 'static,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Progress {
            report: Arc::new(report),
            cancelled,
            save_resume: None,
        }
    }

    /// Sets the callback that receives the provider's [`ResumeState`] after every chunk.
    pub fn with_resume_saver(mut self, f: impl Fn(&ResumeState) + Send + Sync + 'static) -> Self {
        self.save_resume = Some(Arc::new(f));
        self
    }

    pub fn save_resume(&self, state: &ResumeState) {
        if let Some(f) = &self.save_resume {
            f(state);
        }
    }

    /// Ignores progress and is never cancelled.
    pub fn none() -> Self {
        Progress::new(|_, _| {}, Arc::new(AtomicBool::new(false)))
    }

    pub fn report(&self, done: u64, total: u64) {
        (self.report)(done, total)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Where a cloud provider gets its access token. cm-auth's `TokenManager` adapts to it in the
/// daemon; tests use a fake. Kept here so this crate does not depend on cm-auth.
#[async_trait]
pub trait TokenSource: Send + Sync {
    /// A valid access token. `rejected` is the token the server just answered 401 to, if any;
    /// a source that cannot produce a different one returns the same token or `auth_expired`.
    async fn access_token(&self, rejected: Option<&str>) -> Result<String, CmError>;
}

/// The folder an original-bundle upload claimed for itself (`Title.cmrec`, `Title (2).cmrec`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderClaim {
    /// The name taken below the base folder.
    pub name: String,
    /// The provider's id of that folder, when it has ids (Google Drive). A restarted job passes
    /// it back to [`StorageProvider::reuse_bundle_folder`] so it continues in the same folder.
    pub id: Option<String>,
}

#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// `"local"`, `"gdrive"`, `"onedrive"` or `"dropbox"`.
    fn id(&self) -> &'static str;

    async fn upload(
        &self,
        file: &Path,
        target: &Target,
        resume: Option<ResumeState>,
        progress: Progress,
    ) -> Result<Uploaded, CmError>;

    /// Claims a fresh folder for a bundle copy below `base`: `name`, else `name (2)`, ... so two
    /// copies never mix their files (STO-1, STO-5). The default (OneDrive and Dropbox, which are
    /// "Coming soon") takes `name` as it is and does not look at what exists.
    async fn claim_bundle_folder(&self, _base: &str, name: &str) -> Result<FolderClaim, CmError> {
        Ok(FolderClaim {
            name: name.to_owned(),
            id: None,
        })
    }

    /// Continues in a folder that [`claim_bundle_folder`](Self::claim_bundle_folder) returned
    /// before a restart. Returns the claim to use from now on (the same one unless the provider
    /// had to look the folder up again). The default does nothing.
    async fn reuse_bundle_folder(
        &self,
        _base: &str,
        claim: &FolderClaim,
    ) -> Result<FolderClaim, CmError> {
        Ok(claim.clone())
    }

    /// Puts a new version of `file` where an earlier upload of the same file went, inside the
    /// folder `target.folder` (a bundle copy that was edited while it uploaded, STO-5): the
    /// remote file is replaced, or created when there is none. `existing_remote_id` is the id the
    /// first upload returned; a provider finds the file by name in the folder when it is absent
    /// or no longer valid. Never used for a plain upload, which never overwrites. The default is
    /// `not_implemented` (OneDrive and Dropbox).
    async fn replace(
        &self,
        _file: &Path,
        _target: &Target,
        _existing_remote_id: Option<&str>,
        _progress: Progress,
    ) -> Result<Uploaded, CmError> {
        Err(CmError::not_implemented(format!(
            "replacing uploaded files is not available for {} yet",
            self.id()
        )))
    }

    /// Permanently deletes what an upload created: a file by the `remote_id` the upload returned,
    /// or (Google Drive) a folder by its id, with everything in it. A thing that is already gone
    /// is success. The default is `not_implemented` (OneDrive and Dropbox).
    async fn delete(&self, _remote_id: &str) -> Result<(), CmError> {
        Err(CmError::not_implemented(format!(
            "deleting uploaded files is not available for {} yet",
            self.id()
        )))
    }
}

pub(crate) fn io_error(context: &str, path: &Path, e: &std::io::Error) -> CmError {
    let message = format!("{context} {}: {e}", path.display());
    match e.kind() {
        std::io::ErrorKind::StorageFull => CmError::DiskFull { message },
        std::io::ErrorKind::NotFound => CmError::NotFound { message },
        _ => CmError::Internal { message },
    }
}
