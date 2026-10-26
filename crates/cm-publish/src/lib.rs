//! Publishing to video platforms (PUB). One `PublishProvider` trait; this first slice has the
//! YouTube provider (PUB-2). TikTok (PUB-3) and Facebook Pages (PUB-4) plug in behind the same
//! trait later.
//!
//! The HTTP seam (`StorageHttp`), `TokenSource`, `Progress` and `ResumeState` are cm-storage's,
//! reused as they are: the cloud providers have the same needs (a fake server in tests, `ureq`
//! with the system TLS stack in the product, a token that can be refreshed once on 401, a
//! resume state saved after every chunk), and a copy would drift.
//!
//! # Rules the provider keeps
//!
//! * **Validation first.** [`validate`] checks the metadata and files against
//!   [`PlatformRules`] and returns `invalid_input` naming the field, before a token is fetched
//!   or a byte is sent.
//! * **Privacy is only what the caller passes.** The provider requests the privacy in
//!   [`PublishMeta`] (or the platform default) at creation and has no method that changes the
//!   privacy of an existing remote video (the "Never" tier of the agent interface). If the
//!   platform keeps a video private because the API project is unverified, that is reported in
//!   [`Published::status`] and a warning, not raised as a failure.
//! * **Side steps never fail the publish.** A thumbnail, playlist or caption step the platform
//!   refuses becomes a warning in [`Published::warnings`]; a transient failure of such a step
//!   returns `network` with the resume state saved, and the retry skips the finished upload.
//!
//! # Design note: how the daemon's `publish` job will call the provider
//!
//! Built in `cm-daemon` (`publish_ops.rs`): the action, the job, the accounts store
//! ([`accounts`]) and the daily counter. The numbered plan below is what it does.
//!
//! 1. `publish_video` is Confirm tier. Its dialog must show exactly what will run: the
//!    **account** (channel name and id), the **privacy** that will be requested (and, when the
//!    platform rules say unverified projects are locked, the line that YouTube may keep it
//!    private), the **title**, and the **file** (path or capture name, size). The dialog is built
//!    from the same [`PublishMeta`] that the job runs; nothing is sent before the user approves.
//! 2. On approval the daemon runs [`validate`] again (the request may have changed), exports with
//!    the platform preset if the source is a capture (`youtube-4k` or `youtube-1080p`), and
//!    persists a `publish` job (account, file, meta, request id for idempotency).
//! 3. The job builds a [`YouTubeProvider`] with a `UreqHttp` and a `TokenSource` adapter over
//!    cm-auth's `TokenManager` for `ProviderConfig::google_youtube()`, then calls
//!    [`PublishProvider::publish`] with the stored [`ResumeState`] and a [`Progress`] whose
//!    resume saver writes the job row. A `network` error is retried by the queue with the saved
//!    state; `auth_expired` marks the account as needing reconnect; `platform_rejected` is final.
//! 4. On success the job result is `{ url, video_id, warnings, status }` from [`Published`].
//!    Quota ("x of 100 uploads used today", reset at midnight Pacific) is tracked by the daemon,
//!    not here.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod accounts;
pub mod channel;
pub mod meta;
pub mod youtube;

#[cfg(any(test, feature = "testing"))]
pub mod fake_youtube;

use std::path::Path;

use async_trait::async_trait;
use cm_core::CmError;

pub use accounts::{
    next_pacific_midnight, next_pacific_midnight_rfc3339, pacific_day, project_key, PublishAccount,
    PublishAccountStore, YOUTUBE_UPLOADS_PER_DAY,
};
pub use cm_storage::{Progress, ResumeState, StorageHttp, TokenSource, UreqHttp};
pub use meta::{
    rfc3339_utc, validate, validate_meta, validate_video, PlatformRules, Privacy, PublishMeta,
};
pub use youtube::{YouTubeConfig, YouTubeProvider};

/// Where the video is on the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishStatus {
    /// Uploaded; the platform may still be processing it (links work, quality options come later).
    /// Recorded as `processing`; the app does not poll the platform again, so it shows this as
    /// published.
    Processing,
    /// Uploaded and processed.
    Processed,
    /// The platform kept the video private although another privacy was requested (for YouTube:
    /// an unverified API project). Nothing was changed afterwards; the user can make it public
    /// in the platform's own studio.
    PrivacyLocked,
}

/// The result of a publish.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    pub remote_id: String,
    pub url: String,
    pub status: PublishStatus,
    /// The privacy the platform reports (not necessarily the one asked for).
    pub privacy: Privacy,
    /// Things the user should know that are not failures (thumbnail refused, captions failed,
    /// privacy locked).
    pub warnings: Vec<String>,
}

#[async_trait]
pub trait PublishProvider: Send + Sync {
    /// `"youtube"`, later `"tiktok"` and `"facebook"`.
    fn id(&self) -> &'static str;

    fn requirements(&self) -> PlatformRules;

    /// Validates, uploads and sets the extras. `resume` is the state the previous attempt saved
    /// through [`Progress::save_resume`]; a state that no longer matches the file or the
    /// metadata is ignored and the upload starts over.
    async fn publish(
        &self,
        video: &Path,
        meta: &PublishMeta,
        resume: Option<ResumeState>,
        progress: Progress,
    ) -> Result<Published, CmError>;

    /// Permanently deletes a video this provider published (`remote_id` is the one in
    /// [`Published::remote_id`]). A video that is already gone is success. This is the only call
    /// that changes an existing remote video, and it cannot change its privacy: there is no
    /// method for that (the "Never" tier). The default is `not_implemented`.
    async fn delete_video(&self, _remote_id: &str) -> Result<(), CmError> {
        Err(CmError::not_implemented(format!(
            "deleting a video is not available for {} yet",
            self.id()
        )))
    }
}
