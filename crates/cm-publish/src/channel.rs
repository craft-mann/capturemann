//! "Which channel is this?": `channels.list?mine=true`, used by the connect flow (PUB-1.AC1) and
//! by "Test connection". Read-only; it needs the `youtube` scope the sign-in already asked for.

use cm_core::CmError;
use cm_storage::http::{HttpReply, HttpRequest};

use crate::{StorageHttp, TokenSource};

const CHANNELS: &str = "https://www.googleapis.com/youtube/v3/channels?part=snippet&mine=true";

/// The channel a Google sign-in publishes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub title: String,
}

/// Asks YouTube for the signed-in user's channel. A 401 gets one fresh token from `tokens`, then
/// `auth_expired`. A 403 (API not enabled, scope refused) and an account with no channel are
/// `platform_rejected` with words the setup screen can show; a dropped connection or a 5xx is
/// `network`.
pub async fn my_channel(
    http: &dyn StorageHttp,
    tokens: &dyn TokenSource,
) -> Result<Channel, CmError> {
    let mut token = tokens.access_token(None).await?;
    let mut retried = false;
    loop {
        let req = HttpRequest::new("GET", CHANNELS)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/json");
        let reply = http.send(req).await.map_err(|e| CmError::Network {
            message: format!("Cannot reach YouTube: {}", e.0),
        })?;
        match reply.status {
            200 => return parse(&reply),
            401 => {
                if retried {
                    return Err(reconnect());
                }
                retried = true;
                let fresh = tokens.access_token(Some(&token)).await?;
                if fresh == token {
                    return Err(reconnect());
                }
                token = fresh;
            }
            408 | 429 | 500..=599 => {
                return Err(CmError::Network {
                    message: format!("YouTube is busy or not responding (HTTP {}).", reply.status),
                })
            }
            _ => return Err(refused(&reply)),
        }
    }
}

fn reconnect() -> CmError {
    CmError::AuthExpired {
        message: "YouTube no longer accepts this account. Sign in again.".to_owned(),
    }
}

fn parse(reply: &HttpReply) -> Result<Channel, CmError> {
    let v = reply.json().ok_or_else(|| CmError::Internal {
        message: "YouTube sent an unreadable answer".to_owned(),
    })?;
    let item = v["items"].as_array().and_then(|a| a.first());
    let Some(item) = item else {
        return Err(CmError::PlatformRejected {
            message: "This Google account has no YouTube channel. Create one at youtube.com, or sign in with another account.".to_owned(),
            platform_code: Some("noChannel".to_owned()),
        });
    };
    let id = item["id"].as_str().unwrap_or_default();
    if id.is_empty() {
        return Err(CmError::Internal {
            message: "YouTube sent no channel id".to_owned(),
        });
    }
    let title = item["snippet"]["title"].as_str().unwrap_or(id);
    Ok(Channel {
        id: id.to_owned(),
        title: title.to_owned(),
    })
}

fn refused(reply: &HttpReply) -> CmError {
    let v = reply.json().unwrap_or_default();
    let reason = v["error"]["errors"][0]["reason"]
        .as_str()
        .map(str::to_owned);
    let message = v["error"]["message"]
        .as_str()
        .map(cm_jobs::scrub_secrets)
        .unwrap_or_else(|| format!("HTTP {}", reply.status));
    let message = match reason.as_deref() {
        Some("accessNotConfigured") => {
            "YouTube: the YouTube Data API v3 is not enabled in your Google Cloud project."
                .to_owned()
        }
        Some("quotaExceeded") | Some("dailyLimitExceeded") => {
            "YouTube: the daily limit of your project is used up; try again after midnight Pacific time.".to_owned()
        }
        _ => format!("YouTube: {message}"),
    };
    CmError::PlatformRejected {
        message,
        platform_code: reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use cm_storage::http::TransportError;
    use std::sync::Mutex;

    type Reply = Result<HttpReply, TransportError>;

    struct Script(Mutex<Vec<Reply>>, Mutex<Vec<String>>);
    #[async_trait]
    impl StorageHttp for Script {
        async fn send(&self, req: HttpRequest) -> Reply {
            self.1
                .lock()
                .unwrap()
                .push(req.header_value("authorization").unwrap_or("").to_owned());
            self.0.lock().unwrap().remove(0)
        }
    }
    struct Tokens(Mutex<Vec<&'static str>>);
    #[async_trait]
    impl TokenSource for Tokens {
        async fn access_token(&self, _rejected: Option<&str>) -> Result<String, CmError> {
            Ok(self.0.lock().unwrap().remove(0).to_owned())
        }
    }
    fn ok(body: &str) -> Reply {
        Ok(HttpReply::new(200).body(body))
    }

    #[tokio::test]
    async fn reads_the_channel() {
        let http = Script(
            Mutex::new(vec![ok(
                r#"{"items":[{"id":"UC1","snippet":{"title":"My Channel"}}]}"#,
            )]),
            Mutex::default(),
        );
        let c = my_channel(&http, &Tokens(Mutex::new(vec!["t1"])))
            .await
            .unwrap();
        assert_eq!(c.id, "UC1");
        assert_eq!(c.title, "My Channel");
        assert_eq!(http.1.lock().unwrap()[0], "Bearer t1");
    }

    #[tokio::test]
    async fn refreshes_once_on_401_then_gives_up() {
        let http = Script(
            Mutex::new(vec![
                Ok(HttpReply::new(401)),
                ok(r#"{"items":[{"id":"UC1","snippet":{"title":"A"}}]}"#),
            ]),
            Mutex::default(),
        );
        let c = my_channel(&http, &Tokens(Mutex::new(vec!["t1", "t2"])))
            .await
            .unwrap();
        assert_eq!(c.title, "A");
        assert_eq!(http.1.lock().unwrap()[1], "Bearer t2");

        let http = Script(
            Mutex::new(vec![Ok(HttpReply::new(401)), Ok(HttpReply::new(401))]),
            Mutex::default(),
        );
        let e = my_channel(&http, &Tokens(Mutex::new(vec!["t1", "t2"])))
            .await
            .unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::AuthExpired);
    }

    #[tokio::test]
    async fn names_the_failures() {
        let none = Script(Mutex::new(vec![ok(r#"{"items":[]}"#)]), Mutex::default());
        let e = my_channel(&none, &Tokens(Mutex::new(vec!["t"])))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("no YouTube channel"));

        let off = Script(
            Mutex::new(vec![Ok(HttpReply::new(403).body(
                r#"{"error":{"message":"x","errors":[{"reason":"accessNotConfigured"}]}}"#,
            ))]),
            Mutex::default(),
        );
        let e = my_channel(&off, &Tokens(Mutex::new(vec!["t"])))
            .await
            .unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::PlatformRejected);
        assert!(e.to_string().contains("not enabled"));

        let down = Script(Mutex::new(vec![Ok(HttpReply::new(503))]), Mutex::default());
        let e = my_channel(&down, &Tokens(Mutex::new(vec!["t"])))
            .await
            .unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::Network);
    }
}
