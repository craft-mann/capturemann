//! What the OneDrive and Dropbox providers share: one request with the 401 renewal, the
//! transient-failure classes and the back-off. (The Drive provider has its own copy of this.)

use std::sync::Arc;
use std::time::Duration;

use cm_core::CmError;
use serde_json::Value;

use crate::http::{HttpReply, HttpRequest, StorageHttp};
use crate::TokenSource;

/// Retry policy of one request (and of consecutive failed chunks).
#[derive(Debug, Clone)]
pub struct Retry {
    /// Retries before giving up as `network`.
    pub max_retries: u32,
    pub backoff_base: Duration,
    pub max_backoff: Duration,
    /// A `Retry-After` longer than this is not waited for in place; the error is `network` and
    /// the job queue retries later.
    pub max_inline_wait: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Retry {
            max_retries: 5,
            backoff_base: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
            max_inline_wait: Duration::from_secs(60),
        }
    }
}

pub(crate) enum Attempt {
    Reply(HttpReply),
    Transient {
        message: String,
        wait: Option<Duration>,
    },
}

pub(crate) struct Client {
    pub http: Arc<dyn StorageHttp>,
    pub tokens: Arc<dyn TokenSource>,
    pub retry: Retry,
    /// "OneDrive" or "Dropbox", for messages.
    pub service: &'static str,
}

impl Client {
    pub fn reconnect(&self) -> CmError {
        CmError::AuthExpired {
            message: format!(
                "{} no longer accepts this account. Reconnect it in Settings → Destinations.",
                self.service
            ),
        }
    }

    /// One try. 401 asks the token source once for another token and sends again. 408, 429 and
    /// 5xx (except 507, which is a full account) are transient.
    pub async fn attempt(
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
                        message: format!("Cannot reach {}: {}", self.service, e.0),
                        wait: None,
                    })
                }
            };
            match reply.status {
                401 => {
                    if auth_retried {
                        return Err(self.reconnect());
                    }
                    auth_retried = true;
                    let fresh = self.tokens.access_token(Some(tok)).await?;
                    if fresh == *tok {
                        return Err(self.reconnect());
                    }
                    *tok = fresh;
                }
                507 => return Ok(Attempt::Reply(reply)),
                408 | 429 | 500..=599 => {
                    return Ok(Attempt::Transient {
                        message: format!(
                            "{} is busy or not responding (HTTP {})",
                            self.service, reply.status
                        ),
                        wait: retry_after(&reply),
                    })
                }
                _ => return Ok(Attempt::Reply(reply)),
            }
        }
    }

    /// A request that retries transient failures with back-off.
    pub async fn call(
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
    pub async fn back_off(
        &self,
        failures: u32,
        wait: Option<Duration>,
        message: String,
    ) -> Result<(), CmError> {
        if failures > self.retry.max_retries {
            return Err(CmError::Network { message });
        }
        let delay = match wait {
            Some(w) if w > self.retry.max_inline_wait => {
                return Err(CmError::Network {
                    message: format!("{message}; try again in {} s", w.as_secs()),
                })
            }
            Some(w) => w,
            None => self
                .retry
                .backoff_base
                .saturating_mul(1u32 << (failures - 1).min(10))
                .min(self.retry.max_backoff),
        };
        tokio::time::sleep(delay).await;
        Ok(())
    }
}

/// `Retry-After` in seconds, from the header or (Dropbox) the JSON body's `error.retry_after`.
pub(crate) fn retry_after(reply: &HttpReply) -> Option<Duration> {
    let from_header = reply
        .header_value("retry-after")
        .and_then(|v| v.trim().parse::<u64>().ok());
    let secs = from_header.or_else(|| {
        let v: Value = reply.json()?;
        v["error"]["retry_after"].as_u64()
    });
    secs.map(Duration::from_secs)
}

/// Folder segments of a `Target::folder`: no empty, `.` or `..` parts, and a leading segment equal
/// to `root_name` dropped (the default pattern starts with `CaptureMann`).
pub(crate) fn folder_segments<'a>(folder: &'a str, root_name: &str) -> Vec<&'a str> {
    let mut segments: Vec<&str> = folder
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect();
    if segments.first().copied() == Some(root_name) {
        segments.remove(0);
    }
    segments
}
