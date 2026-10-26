//! `list_publish_accounts`, `publish_video` and the `publish` job (PUB-1, PUB-2).
//!
//! Accounts come from `publish_accounts.json` ([`cm_publish::PublishAccountStore`]); the app
//! writes it after the connect flow, agents only read it, and what they read never includes the
//! keychain key or a token.
//!
//! # `publish_video` (Confirm)
//!
//! `publish_video { capture_id, platform: "youtube", account_id?, title, description?, tags?,
//! privacy?, made_for_kids, thumbnail?, export_preset?, captions?, category?, playlist_id?,
//! metadata?, request_id? }`. Everything that can be checked is checked **before** the dialog
//! opens ([`Inner::plan_publish`]): the capture exists and is a recording, an export exists (or
//! `export_preset` is valid for it), the account exists, the daily upload limit is not used up,
//! and [`cm_publish::validate`] passes (a field-named `invalid_input`). The dialog then shows
//! exactly what will run: the platform, the account label, the title, the privacy that will be
//! requested, made for kids, the file name and size, that the video is **uploaded over the
//! internet**, and the day's upload count. A missing `privacy` is the account's default, which
//! is never `public`. What the user approved is what runs: [`Inner::run_publish`] enqueues the
//! `publish` job from the same plan, and a declined or timed-out dialog never gets that far.
//!
//! There is no action and no parameter that changes the privacy of a video that already exists
//! on the platform (the "Never" tier of the agent interface); the provider has no such method
//! either.
//!
//! # The `publish` job
//!
//! One job at a time (the queue's default limit of 1 for a type it has no entry for), held while
//! a recording runs like `upload`. It builds a [`YouTubeProvider`] over the same HTTP client and
//! token sources as the cloud destinations, saves the provider's resume state after every chunk,
//! and is retried with the queue's backoff on `network`. `auth_expired` pauses it until the
//! account signs in again. On success it adds a record to the capture's `publishes[]` (no undo
//! step), counts the upload for the Pacific day, and publishes `publish.done`. A thumbnail or
//! captions step the platform refuses, and a video the platform kept private, are warnings in
//! the result, not failures.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use cm_core::{Actor, CmError};
use cm_jobs::{JobContext, JobFilter, JobHandler, JobState, NewJob, JOB_PUBLISH};
use cm_library::manifest::CaptureKind;
use cm_publish::{
    next_pacific_midnight_rfc3339, PlatformRules, Privacy, PublishAccount, PublishAccountStore,
    PublishMeta, PublishProvider, YouTubeConfig, YouTubeProvider, YOUTUBE_UPLOADS_PER_DAY,
};
use cm_storage::Progress;
use serde_json::{json, Map, Value};
use time::OffsetDateTime;

use crate::action::{ExportVideoParams, ExternalParams, Fields};
use crate::ai::{recording_bundle, CaptionFormat};
use crate::daemon::{blocking, lock_library, Daemon, Inner};
use crate::error::DaemonError;
use crate::events::{self, EventBus};
use crate::library_ops::human_size;
use crate::publish_records;
use crate::storage_cloud::{AccountTokens, CloudBackend};

/// The action names of this module that are not Confirm tier.
pub const PUBLISH_METHODS: &[&str] = &["list_publish_accounts"];

const PARAMS: &[&str] = &[
    "capture_id",
    "platform",
    "account_id",
    "title",
    "description",
    "tags",
    "privacy",
    "made_for_kids",
    "thumbnail",
    "export_preset",
    "export_path",
    "captions",
    "category",
    "playlist_id",
    "metadata",
    "request_id",
];

/// The fields `metadata` may carry (the spec's nested form of the same flat fields).
const METADATA_FIELDS: &[&str] = &[
    "title",
    "description",
    "tags",
    "privacy",
    "made_for_kids",
    "thumbnail",
    "captions",
    "category",
    "playlist_id",
];

/// A validated `publish_video` request.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishVideoParams {
    pub capture_id: String,
    pub platform: String,
    pub account_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub privacy: Option<Privacy>,
    pub made_for_kids: Option<bool>,
    pub thumbnail: Option<String>,
    pub export_preset: Option<String>,
    /// Absolute path of an export of this capture (the publish sheet sends the one it shows).
    pub export_path: Option<String>,
    pub captions: bool,
    pub category: Option<String>,
    pub playlist_id: Option<String>,
    pub request_id: Option<String>,
}

impl PublishVideoParams {
    /// Reads the pass-through payload of `publish_video`. Unknown fields are refused (a typo must
    /// not publish with the default). `metadata` is the spec's nested form: its fields are used
    /// unless the same field is also given at the top level.
    pub fn parse(p: &ExternalParams) -> Result<Self, CmError> {
        let mut merged: Map<String, Value> = p.rest.clone();
        if let Some(id) = &p.capture_id {
            merged.insert("capture_id".into(), json!(id));
        }
        let empty = Map::new();
        let top = Value::Object(merged.clone());
        let top_fields = Fields::new(&top, &empty)?;
        top_fields.only(PARAMS)?;
        if let Some(meta) = merged.remove("metadata") {
            let Value::Object(meta) = meta else {
                return Err(CmError::invalid_field("metadata", "must be an object"));
            };
            for (k, v) in meta {
                if !METADATA_FIELDS.contains(&k.as_str()) {
                    return Err(CmError::invalid_field(
                        &format!("metadata.{k}"),
                        "unknown parameter",
                    ));
                }
                merged.entry(k).or_insert(v);
            }
        }
        let all = Value::Object(merged);
        let f = Fields::new(&all, &empty)?;
        let platform = f.str_req("platform")?;
        let tags = match f.obj.get("tags") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .map(|t| {
                    t.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| CmError::invalid_field("tags", "must be a list of strings"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(CmError::invalid_field("tags", "must be a list of strings")),
        };
        let privacy = match f.str_opt("privacy")? {
            None => None,
            Some(s) => Some(Privacy::parse(&s).ok_or_else(|| {
                CmError::invalid_field("privacy", "must be `private`, `unlisted` or `public`")
            })?),
        };
        let made_for_kids = match f.obj.get("made_for_kids") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => {
                return Err(CmError::invalid_field(
                    "made_for_kids",
                    "must be true or false",
                ))
            }
        };
        let thumbnail = f.str_opt("thumbnail")?;
        let export_preset = f.str_opt("export_preset")?;
        if export_preset.as_deref().is_some_and(str::is_empty) {
            return Err(CmError::invalid_field("export_preset", "must not be empty"));
        }
        let export_path = f.str_opt("export_path")?;
        Ok(PublishVideoParams {
            capture_id: f.str_req("capture_id")?,
            platform,
            account_id: f.str_opt("account_id")?,
            title: f.str_opt("title")?.unwrap_or_default(),
            description: f.str_opt("description")?,
            tags,
            privacy,
            made_for_kids,
            thumbnail,
            export_preset,
            export_path,
            captions: f.bool_or("captions", false)?,
            category: f.str_opt("category")?,
            playlist_id: f.str_opt("playlist_id")?,
            request_id: p.request_id.clone(),
        })
    }
}

/// Which video a publish sends.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum VideoPick {
    /// An export that exists: path relative to the bundle, and its size now.
    File { rel: String, size: u64 },
    /// An export that will be made first, with this preset.
    Preset(String),
}

/// What the dialog showed and what runs after the user approved it.
#[derive(Debug, Clone)]
pub(crate) struct PreparedPublish {
    pub summary: String,
    pub details: Value,
    /// The same plan as typed parameters (P3).
    pub params: Value,
    capture_id: String,
    capture_title: String,
    account: PublishAccount,
    meta: PublishMeta,
    pick: VideoPick,
    captions: bool,
}

fn platform_name(p: &str) -> &'static str {
    match p {
        "youtube" => "YouTube",
        "tiktok" => "TikTok",
        "facebook" => "Facebook",
        _ => "the platform",
    }
}

fn capitalised(p: Privacy) -> &'static str {
    match p {
        Privacy::Private => "Private",
        Privacy::Unlisted => "Unlisted",
        Privacy::Public => "Public",
    }
}

fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}...", &s[..i]),
        None => s.to_owned(),
    }
}

fn now_unix(now: OffsetDateTime) -> i64 {
    now.unix_timestamp()
}

/// The key of the Google Cloud project an account's uploads are counted under: from the client
/// ID it was connected with (two accounts that share a client share YouTube's 100 uploads a day),
/// `None` when that is not known, and the count then stays per account.
pub(crate) fn project_of(tokens: &dyn AccountTokens, account_id: &str) -> Option<String> {
    tokens
        .client_id(account_id)
        .and_then(|c| cm_publish::project_key(&c))
}

fn account_json(
    a: &PublishAccount,
    store: &PublishAccountStore,
    project: Option<&str>,
    now: i64,
    needs_reconnect: bool,
) -> Value {
    let used = store.uploads_today_in(&a.id, project, now);
    // Label and defaults; the keychain key and any token stay private.
    json!({
        "account_id": a.id,
        "id": a.id,
        "platform": a.platform,
        "name": a.name,
        "status": if needs_reconnect { "needs_reconnect" } else { "connected" },
        "default_privacy": a.fallback_privacy().as_str(),
        "uploads_today": used,
        "uploads_limit": YOUTUBE_UPLOADS_PER_DAY,
        "quota": { "used": used, "limit": YOUTUBE_UPLOADS_PER_DAY },
        "resets_at": next_pacific_midnight_rfc3339(now),
    })
}

impl Inner {
    /// `list_publish_accounts`.
    pub(crate) async fn list_publish_accounts(&self) -> Result<Value, DaemonError> {
        let rules = PlatformRules::youtube();
        let rules_json = json!({
            "youtube": {
                "title_max_chars": rules.title_max_chars,
                "description_max_bytes": rules.description_max_bytes,
                "tags_max_total_chars": rules.tags_max_total_chars,
                "thumbnail_max_bytes": rules.thumbnail_max_bytes,
                "privacies": rules.privacies.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
                "default_privacy": rules.default_privacy.as_str(),
                "requires_made_for_kids": rules.requires_made_for_kids,
                "uploads_per_day": YOUTUBE_UPLOADS_PER_DAY,
            }
        });
        let Some(store) = self.publish_accounts.clone() else {
            return Ok(json!({ "accounts": [], "platforms": rules_json }));
        };
        let now = now_unix(self.clock.now());
        // A publish paused as `auth_expired` says which account must sign in again.
        let waiting: Vec<String> = match self.jobs.as_ref() {
            Some(rt) => rt
                .queue
                .list(&JobFilter {
                    job_type: Some(JOB_PUBLISH.to_owned()),
                    states: Some(vec![JobState::Paused]),
                    limit: Some(200),
                })?
                .into_iter()
                .filter(|j| j.pause_reason.as_deref() == Some(cm_jobs::PAUSE_AUTH_EXPIRED))
                .filter_map(|j| {
                    j.inputs
                        .get("account_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect(),
            None => Vec::new(),
        };
        let tokens = self.cloud.tokens.clone();
        let list = blocking(move || {
            Ok(store
                .list()?
                .iter()
                .map(|a| {
                    let project = project_of(tokens.as_ref(), &a.id);
                    account_json(a, &store, project.as_deref(), now, waiting.contains(&a.id))
                })
                .collect::<Vec<_>>())
        })
        .await?;
        Ok(json!({ "accounts": list, "platforms": rules_json }))
    }

    /// Everything about a `publish_video` that can be checked without side effects, and the plan
    /// for the dialog. Runs before the gate, so a bad request never opens a dialog.
    pub(crate) async fn plan_publish(
        &self,
        raw: &ExternalParams,
    ) -> Result<PreparedPublish, DaemonError> {
        let p = PublishVideoParams::parse(raw)?;
        match p.platform.as_str() {
            "youtube" => {}
            "tiktok" | "facebook" => {
                return Err(DaemonError::NotImplemented(format!(
                    "publishing to {} is not built yet; YouTube is",
                    platform_name(&p.platform)
                )))
            }
            _ => {
                return Err(CmError::invalid_field(
                    "platform",
                    "must be `youtube` (`tiktok` and `facebook` are not built yet)",
                )
                .into())
            }
        }
        let store = self.publish_accounts.clone().ok_or_else(|| {
            DaemonError::NotImplemented(
                "publishing is not available: this daemon has no publish accounts".to_owned(),
            )
        })?;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_PUBLISH) {
            return Err(DaemonError::NotImplemented(
                "publishing is not available: this daemon has no publisher registered".to_owned(),
            ));
        }
        let (library, now) = (self.library.clone(), now_unix(self.clock.now()));
        let exports_ok = self.job_queue()?.queue.has_handler(cm_jobs::JOB_EXPORT);
        let captions_ok = self.ai.captions.available();
        let tokens = self.cloud.tokens.clone();
        blocking(move || {
            plan_blocking(
                &library,
                &store,
                tokens.as_ref(),
                p,
                now,
                exports_ok,
                captions_ok,
            )
        })
        .await
    }

    /// Runs the approved plan: makes the export first when the plan said so, then queues the
    /// `publish` job. Nothing here asks again or reads live settings.
    pub(crate) async fn run_publish(
        &self,
        actor: &Actor,
        plan: PreparedPublish,
        request_id: Option<String>,
    ) -> Result<Value, DaemonError> {
        let queue = self.job_queue()?.queue.clone();
        let mut inputs = json!({
            "capture_id": plan.capture_id,
            "platform": plan.account.platform,
            "account_id": plan.account.id,
            "by": actor.to_string(),
            "meta": serde_json::to_value(&plan.meta)
                .map_err(|e| CmError::internal(e.to_string()))?,
            "request_id": request_id,
        });
        let mut export_job_id: Option<String> = None;
        let mut file_rel: Option<String> = None;
        let mut preset: Option<String> = None;
        match &plan.pick {
            VideoPick::File { rel, .. } => {
                file_rel = Some(rel.clone());
                inputs["export_file"] = json!(rel);
            }
            VideoPick::Preset(pr) => {
                preset = Some(pr.clone());
                inputs["export_preset"] = json!(pr);
                let started = self
                    .export_video(
                        actor,
                        ExportVideoParams {
                            capture_id: plan.capture_id.clone(),
                            preset: pr.clone(),
                            loudness_lufs: None,
                            width: None,
                            fps: None,
                            max_mb: None,
                            captions: None,
                            request_id: None,
                        },
                    )
                    .await?;
                export_job_id = started
                    .get("job_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
        }
        if plan.captions {
            // The SRT is made now, from the captions as they are at the moment of approval.
            let srt = self
                .export_captions(&plan.capture_id, CaptionFormat::Srt)
                .await?;
            inputs["captions_file"] = srt.get("path").cloned().unwrap_or(Value::Null);
        }
        let mut new = NewJob::new(JOB_PUBLISH, inputs);
        if let Some(id) = &export_job_id {
            new = new.depends_on(id.clone());
        }
        let job = blocking(move || queue.enqueue(new)).await?;
        let privacy = plan.meta.privacy.unwrap_or(Privacy::Unlisted);
        self.events.publish(
            events::PUBLISH_QUEUED,
            json!({
                "job_id": job.id,
                "capture_id": plan.capture_id,
                "title": plan.capture_title,
                "video_title": plan.meta.title,
                "platform": plan.account.platform,
                "account_id": plan.account.id,
                "account": plan.account.name,
                "privacy": privacy.as_str(),
                "export_job_id": export_job_id,
            }),
        );
        Ok(json!({
            "job_id": job.id,
            "capture_id": plan.capture_id,
            "platform": plan.account.platform,
            "account_id": plan.account.id,
            "account": plan.account.name,
            "title": plan.meta.title,
            "privacy": privacy.as_str(),
            "made_for_kids": plan.meta.made_for_kids,
            "file": file_rel,
            "export_preset": preset,
            "export_job_id": export_job_id,
            "captions": plan.captions,
            "state": job.state.as_str(),
        }))
    }
}

/// The blocking part of the plan: library and file checks, validation, the dialog rows.
fn plan_blocking(
    library: &Arc<std::sync::Mutex<cm_library::Library>>,
    store: &PublishAccountStore,
    tokens: &dyn AccountTokens,
    p: PublishVideoParams,
    now: i64,
    exports_ok: bool,
    captions_ok: bool,
) -> Result<PreparedPublish, CmError> {
    let rules = PlatformRules::youtube();
    let account = store.resolve(&p.platform, p.account_id.as_deref())?;

    // The capture: a recording that is not being recorded.
    let (rec, dir) = {
        let lib = lock_library(library);
        let r = lib.get(&p.capture_id)?;
        let d = lib.bundle_path(&r);
        (r, d)
    };
    if rec.kind == CaptureKind::Audio {
        return Err(crate::export::audio_has_no_video(&rec.id));
    }
    if rec.kind != CaptureKind::Recording {
        return Err(CmError::invalid_input(format!(
            "`{}` is a screenshot; only recordings can be published as video",
            rec.id
        )));
    }
    if !dir.is_dir() {
        return Err(CmError::not_found(format!(
            "the folder of `{}` was moved or deleted",
            rec.id
        )));
    }
    if dir.join(cm_library::rec::LOCK_FILE).exists() {
        return Err(CmError::busy(format!(
            "`{}` is still being recorded; stop the recording before you publish it",
            rec.id
        )));
    }

    // The daily limit.
    let project = project_of(tokens, &account.id);
    let used = store.uploads_today_in(&account.id, project.as_deref(), now);
    if used >= YOUTUBE_UPLOADS_PER_DAY {
        return Err(CmError::platform_rejected(
            format!(
                "Upload limit reached for today ({used} of {YOUTUBE_UPLOADS_PER_DAY}); try after {}",
                next_pacific_midnight_rfc3339(now)
            ),
            Some("uploadLimitExceeded".to_owned()),
        ));
    }

    // Paths must be absolute, and a path and a preset exclude each other.
    if let Some(t) = &p.thumbnail {
        if !Path::new(t).is_absolute() {
            return Err(CmError::invalid_field(
                "thumbnail",
                "must be an absolute path to a JPG or PNG file",
            ));
        }
    }
    if let Some(e) = &p.export_path {
        if !Path::new(e).is_absolute() {
            return Err(CmError::invalid_field(
                "export_path",
                "must be an absolute path of an export of this capture",
            ));
        }
        if p.export_preset.is_some() {
            return Err(CmError::invalid_field(
                "export_path",
                "give `export_path` or `export_preset`, not both",
            ));
        }
    }

    // The privacy: what was asked, or the account's default (never public).
    let defaulted = p.privacy.is_none();
    let privacy = p.privacy.unwrap_or_else(|| account.fallback_privacy());

    let mut meta = PublishMeta {
        title: p.title.clone(),
        description: p.description.clone().unwrap_or_default(),
        tags: p.tags.clone(),
        privacy: Some(privacy),
        category: p.category.clone(),
        made_for_kids: p.made_for_kids,
        thumbnail: p.thumbnail.as_ref().map(PathBuf::from),
        playlist_id: p.playlist_id.clone(),
        ..PublishMeta::default()
    };

    // Captions need something to make an SRT from.
    if p.captions {
        if !captions_ok {
            return Err(CmError::not_implemented(
                "captions are not available: local AI is not set up in this build",
            ));
        }
        let manifest = cm_library::read_manifest(&dir)?;
        let has = cm_library::rec_edit::captions_of(&manifest).is_some()
            || dir.join(crate::ai::TRANSCRIPT_FILE).is_file();
        if !has {
            return Err(CmError::invalid_field(
                "captions",
                "the recording has no captions yet; run `transcribe` or `set_captions` first",
            ));
        }
    }

    // The video: an export that exists, or the preset that will be made first.
    let pick = match (&p.export_preset, &p.export_path) {
        (None, Some(given)) => {
            let rel = Path::new(given)
                .strip_prefix(&dir)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .filter(|r| !r.split('/').any(|s| s == ".."))
                .ok_or_else(|| {
                    CmError::invalid_field("export_path", "is not a file of this capture")
                })?;
            let size = std::fs::metadata(dir.join(&rel))
                .map_err(|_| CmError::invalid_field("export_path", "the file does not exist"))?
                .len();
            VideoPick::File { rel, size }
        }
        (Some(preset), _) => {
            crate::export::check_preset(preset, &crate::export::ExportOpts::default())?;
            crate::export::check_preset_for_recording(
                library,
                &rec.id,
                preset,
                crate::export::ExportOpts::default(),
            )?;
            if !exports_ok {
                return Err(CmError::not_found(
                    "export is not available: this daemon has no exporter registered",
                ));
            }
            VideoPick::Preset(preset.clone())
        }
        (None, None) => {
            let path = crate::storage_ops::latest_export(&dir)?.ok_or_else(|| {
                CmError::not_found(format!(
                    "`{}` has no export yet; run `export_video`, or give `export_preset` (for \
                     example `youtube-1080p`)",
                    rec.id
                ))
            })?;
            let rel = path
                .strip_prefix(&dir)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            VideoPick::File { rel, size }
        }
    };

    // Validation by the platform's rules, field by field.
    match &pick {
        VideoPick::File { rel, .. } => {
            meta.duration_secs = export_duration_secs(&dir, rel);
            cm_publish::validate(&rules, &dir.join(rel), &meta)?;
        }
        VideoPick::Preset(_) => cm_publish::validate_meta(&rules, &meta)?,
    }

    // The dialog.
    let platform = platform_name(&account.platform);
    let (file_row, size_row) = match &pick {
        VideoPick::File { rel, size } => (
            rel.rsplit('/').next().unwrap_or(rel).to_owned(),
            Some(human_size(*size)),
        ),
        VideoPick::Preset(pr) => (format!("Exported first with the `{pr}` preset"), None),
    };
    let summary = format!(
        "Publish \"{}\" to {platform} as {}",
        clip(&meta.title, 80),
        capitalised(privacy)
    );
    let mut rows = Map::new();
    rows.insert("capture".into(), json!(rec.title));
    rows.insert("platform".into(), json!(account.platform));
    rows.insert("account".into(), json!(account.name));
    rows.insert("title".into(), json!(meta.title));
    rows.insert("privacy".into(), json!(privacy.as_str()));
    if defaulted {
        rows.insert(
            "privacy_source".into(),
            json!("not given: the account's default"),
        );
    }
    rows.insert("made_for_kids".into(), json!(meta.made_for_kids));
    rows.insert("file".into(), json!(file_row));
    if let Some(s) = size_row {
        rows.insert("size".into(), json!(s));
    }
    if !meta.description.is_empty() {
        rows.insert("description".into(), json!(clip(&meta.description, 200)));
    }
    if !meta.tags.is_empty() {
        rows.insert("tags".into(), json!(meta.tags.join(", ")));
    }
    // The id only: a title would need a playlists call (the `youtube` scope and the network)
    // before the user approves, and the dialog never talks to the platform.
    if let Some(id) = meta.playlist_id.as_deref().filter(|s| !s.trim().is_empty()) {
        rows.insert("playlist".into(), json!(id.trim()));
    }
    if let Some(t) = &meta.thumbnail {
        rows.insert(
            "thumbnail".into(),
            json!(t.file_name().map(|n| n.to_string_lossy().into_owned())),
        );
    }
    if p.captions {
        rows.insert(
            "captions".into(),
            json!("Uploads the captions as an SRT track"),
        );
    }
    rows.insert(
        "sends".into(),
        json!(format!(
            "Uploads this video over the internet to {platform}, channel \"{}\"",
            account.name
        )),
    );
    rows.insert(
        "uploads_today".into(),
        json!(format!("{used} of {YOUTUBE_UPLOADS_PER_DAY} used today")),
    );
    if privacy != Privacy::Private && rules.unverified_project_privacy == Some(Privacy::Private) {
        rows.insert(
            "note".into(),
            json!(format!(
                "{platform} keeps videos from an unverified API project private; the result says so if it happens"
            )),
        );
    }
    let params = json!({
        "title": clip(&meta.title, 80),
        "platform": account.platform,
        "platform_name": platform,
        "account": account.name,
        "privacy": privacy.as_str(),
        "privacy_defaulted": defaulted,
        "preset": match &pick { VideoPick::Preset(pr) => Some(pr.clone()), VideoPick::File { .. } => None },
        "size_bytes": match &pick { VideoPick::File { size, .. } => Some(*size), VideoPick::Preset(_) => None },
        "captions": p.captions,
        "uploads_used": used,
        "uploads_limit": YOUTUBE_UPLOADS_PER_DAY,
        "kept_private_note": privacy != Privacy::Private
            && rules.unverified_project_privacy == Some(Privacy::Private),
    });
    Ok(PreparedPublish {
        summary,
        details: Value::Object(rows),
        params,
        capture_id: rec.id,
        capture_title: rec.title,
        account,
        meta,
        pick,
        captions: p.captions,
    })
}

/// The duration in seconds the manifest records for an export file, if it does.
fn export_duration_secs(dir: &Path, rel: &str) -> Option<f64> {
    let manifest = cm_library::read_manifest(dir).ok()?;
    cm_library::exports_of(&manifest)
        .iter()
        .find(|e| e.get("file").and_then(Value::as_str) == Some(rel))
        .and_then(|e| e.get("duration_ms").and_then(Value::as_f64))
        .map(|ms| ms / 1000.0)
}

// ---- the job ---------------------------------------------------------------------------------

impl Daemon {
    /// Registers the executor of the job type `publish`. Call it next to
    /// [`Daemon::register_upload_handler`], before [`Daemon::start_jobs`], in every process that
    /// owns a daemon. Errors with `not_implemented` without a job queue.
    pub fn register_publish_handler(&self) -> Result<(), DaemonError> {
        self.register_job_handler(JOB_PUBLISH, self.publish_handler())
    }

    /// The handler of the `publish` job.
    pub fn publish_handler(&self) -> Arc<dyn JobHandler> {
        Arc::new(PublishHandler {
            locks: self.inner.capture_locks.clone(),
            library: self.inner.library.clone(),
            accounts: self.inner.publish_accounts.clone(),
            cloud: self.inner.cloud.clone(),
            youtube: self.inner.youtube.clone(),
            events: self.inner.events.clone(),
        })
    }
}

struct PublishHandler {
    locks: Arc<crate::daemon::LockMap>,
    library: Arc<std::sync::Mutex<cm_library::Library>>,
    accounts: Option<PublishAccountStore>,
    cloud: CloudBackend,
    youtube: YouTubeConfig,
    events: EventBus,
}

/// `auth_expired` says which account to reconnect; the job pauses with that message.
fn explain_auth(account: &PublishAccount, e: CmError) -> CmError {
    match e {
        CmError::AuthExpired { message } => CmError::AuthExpired {
            message: format!(
                "Reconnect needed for {} account \"{}\" (auth_expired): {message}",
                platform_name(&account.platform),
                account.name
            ),
        },
        other => other,
    }
}

struct Source {
    path: PathBuf,
    title: String,
    duration_secs: Option<f64>,
}

/// The video file the job sends: the export pinned at approval, or the newest export of the
/// preset that was made first.
fn resolve_source(
    library: &Arc<std::sync::Mutex<cm_library::Library>>,
    inputs: &Value,
) -> Result<Source, CmError> {
    let id = inputs
        .get("capture_id")
        .and_then(Value::as_str)
        .ok_or_else(|| CmError::internal("the publish job has no capture"))?;
    let (dir, manifest) = recording_bundle(library, id)?;
    let title = lock_library(library).get(id)?.title;
    let exports = cm_library::exports_of(&manifest);
    let rel = if let Some(rel) = inputs.get("export_file").and_then(Value::as_str) {
        rel.to_owned()
    } else if let Some(preset) = inputs.get("export_preset").and_then(Value::as_str) {
        exports
            .iter()
            .rev()
            .find(|e| e.get("preset").and_then(Value::as_str) == Some(preset))
            .and_then(|e| e.get("file").and_then(Value::as_str))
            .map(str::to_owned)
            .ok_or_else(|| {
                CmError::not_found(format!("`{id}` has no `{preset}` export to publish"))
            })?
    } else {
        return Err(CmError::internal("the publish job names no video"));
    };
    // An entry must stay inside the bundle.
    if rel.starts_with('/') || rel.split('/').any(|s| s == "..") {
        return Err(CmError::invalid_input("the export path leaves the capture"));
    }
    let path = dir.join(&rel);
    if !path.is_file() {
        return Err(CmError::not_found(format!(
            "the export `{rel}` of `{id}` no longer exists; export it again"
        )));
    }
    let duration_secs = exports
        .iter()
        .find(|e| e.get("file").and_then(Value::as_str) == Some(rel.as_str()))
        .and_then(|e| e.get("duration_ms").and_then(Value::as_f64))
        .map(|ms| ms / 1000.0);
    Ok(Source {
        path,
        title,
        duration_secs,
    })
}

#[async_trait]
impl JobHandler for PublishHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let store = self
            .accounts
            .clone()
            .ok_or_else(|| CmError::not_found("no publish account is set up"))?;
        let inputs = ctx.job.inputs.clone();
        let account_id = inputs
            .get("account_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let (s2, id2) = (store.clone(), account_id.clone());
        let account = tokio::task::spawn_blocking(move || s2.get(&id2))
            .await
            .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
        let mut meta: PublishMeta = serde_json::from_value(
            inputs.get("meta").cloned().unwrap_or(Value::Null),
        )
        .map_err(|e| CmError::internal(format!("the publish job has damaged metadata: {e}")))?;
        let (lib, inp) = (self.library.clone(), inputs.clone());
        let source = tokio::task::spawn_blocking(move || resolve_source(&lib, &inp))
            .await
            .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
        meta.duration_secs = source.duration_secs;
        meta.captions = inputs
            .get("captions_file")
            .and_then(Value::as_str)
            .map(PathBuf::from);
        let requested = meta
            .privacy
            .ok_or_else(|| CmError::internal("the publish job has no privacy"))?;

        let provider = YouTubeProvider::new(
            self.cloud.http.clone(),
            self.cloud.tokens.source(&account.id),
            self.youtube.clone(),
        );

        // Progress by bytes (at most one database write per percent); the provider's resume
        // state is saved after every chunk, so a restart continues from the last confirmed byte.
        let flag = Arc::new(AtomicBool::new(false));
        let last = Arc::new(AtomicU64::new(u64::MAX));
        let reporter = ctx.reporter.clone();
        let (flag2, last2) = (flag.clone(), last.clone());
        let saver = ctx.reporter.clone();
        let progress = Progress::new(
            move |done, total| {
                let total = total.max(1);
                let pct = done.min(total) * 100 / total;
                if last2.swap(pct, Ordering::Relaxed) != pct {
                    reporter.report(done.min(total) as f64 / total as f64, None);
                }
            },
            flag2,
        )
        .with_resume_saver(move |state| {
            let _ = saver.save_checkpoint(&json!({ "resume": state.0 }));
        });
        let resume = ctx
            .job
            .checkpoint
            .as_ref()
            .and_then(|c| c.get("resume").cloned())
            .map(cm_storage::ResumeState);

        let mut cancel = ctx.cancel.clone();
        let publish = provider.publish(&source.path, &meta, resume, progress);
        tokio::pin!(publish);
        let mut interrupted = false;
        let published = loop {
            // Biased: when the cancel and the upload become ready together, the flag is set
            // first, so the next chunk boundary sees it (a fast transport could otherwise finish
            // the remaining chunks inside one poll).
            tokio::select! {
                biased;
                _ = cancel.interrupted(), if !interrupted => {
                    interrupted = true;
                    flag.store(true, Ordering::Relaxed);
                }
                r = &mut publish => break r,
            }
        }
        .map_err(|e| explain_auth(&account, e))?;

        // The video exists now. Bookkeeping must not redo the upload: a failure here is a
        // warning in the result.
        let now = OffsetDateTime::now_utc();
        let at = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        let record = publish_records::build(&account, &meta.title, requested, &published, &at);
        let cid = inputs
            .get("capture_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let record_warning = publish_records::save(&self.library, &self.locks, &cid, record)
            .await
            .err()
            .map(|e| format!("The publish record was not saved: {e}"));
        let (s3, aid, ts) = (store.clone(), account.id.clone(), now.unix_timestamp());
        let tokens = self.cloud.tokens.clone();
        let uploads_today = tokio::task::spawn_blocking(move || {
            let project = project_of(tokens.as_ref(), &aid);
            s3.record_upload_in(&aid, project.as_deref(), ts).ok()
        })
        .await
        .unwrap_or(None);

        let status = publish_records::status_str(published.status);
        self.events.publish(
            events::PUBLISH_DONE,
            json!({
                "job_id": ctx.job.id,
                "capture_id": inputs.get("capture_id"),
                "title": source.title,
                "platform": account.platform,
                "account_id": account.id,
                "account": account.name,
                "video_id": published.remote_id,
                "url": published.url,
                "privacy": published.privacy.as_str(),
                "status": status,
                "warnings": published.warnings,
            }),
        );
        Ok(json!({
            "capture_id": inputs.get("capture_id"),
            "platform": account.platform,
            "account_id": account.id,
            "account": account.name,
            "video_id": published.remote_id,
            "url": published.url,
            "status": status,
            "privacy": published.privacy.as_str(),
            "privacy_requested": requested.as_str(),
            "warnings": published.warnings,
            "uploads_today": uploads_today,
            "record_warning": record_warning,
        }))
    }
}
