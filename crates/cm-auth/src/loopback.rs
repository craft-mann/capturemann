//! The one-shot loopback listener for the OAuth redirect (RFC 8252 section 7.3).
//!
//! Binds `127.0.0.1` on a random port. [`LoopbackListener::wait_for_code`] accepts connections until
//! the first request for `/callback`, which is the callback: its `state` must match, then the page
//! "You can close this tab" is shown and the code is returned. Other paths (a browser asking for
//! `/favicon.ico`) get a 404 and are ignored. The listener is consumed, so a second callback finds
//! the port closed and cannot change the result.

use std::time::Duration;

use cm_core::CmError;
use percent_encoding::percent_decode_str;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_HEAD: usize = 8 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct LoopbackListener {
    listener: TcpListener,
    port: u16,
}

#[derive(Debug)]
enum Reaction {
    /// Not the callback: answer 404 and keep waiting.
    NotCallback,
    /// The callback: answer with this page and finish with this result.
    Callback {
        page: &'static str,
        status: u16,
        result: Result<String, CmError>,
    },
}

impl LoopbackListener {
    pub async fn bind() -> Result<Self, CmError> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| CmError::internal(format!("could not open the sign-in listener: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| CmError::internal(format!("sign-in listener: {e}")))?
            .port();
        Ok(Self { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    /// Waits for the callback and returns the authorization code.
    ///
    /// Errors: a `state` mismatch is `invalid_input` (the request did not come from our flow); the
    /// provider's `error=` (for example `access_denied`) is `platform_rejected` with that
    /// `platform_code`; no callback within `timeout` is `user_declined`.
    pub async fn wait_for_code(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, CmError> {
        match tokio::time::timeout(timeout, self.run(expected_state)).await {
            Ok(r) => r,
            Err(_) => Err(CmError::user_declined(
                "Sign-in was not completed in time. Try connecting again.",
            )),
        }
    }

    async fn run(self, expected_state: &str) -> Result<String, CmError> {
        loop {
            let (mut stream, _) = match self.listener.accept().await {
                Ok(c) => c,
                Err(_) => continue,
            };
            let Some(line) = read_request_line(&mut stream).await else {
                continue;
            };
            match react(&line, expected_state) {
                Reaction::NotCallback => {
                    respond(&mut stream, 404, "Not found").await;
                }
                Reaction::Callback {
                    page,
                    status,
                    result,
                } => {
                    respond(&mut stream, status, page).await;
                    return result;
                }
            }
        }
    }
}

async fn read_request_line(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let read = async {
        loop {
            let n = stream.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(2).any(|w| w == b"\r\n") || buf.len() >= MAX_HEAD {
                return Some(());
            }
        }
    };
    tokio::time::timeout(READ_TIMEOUT, read).await.ok()??;
    let end = buf
        .windows(2)
        .position(|w| w == b"\r\n")
        .unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end.min(MAX_HEAD)]).into_owned())
}

async fn respond(stream: &mut TcpStream, status: u16, page: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>CaptureMann</title></head>\
         <body style=\"font-family:system-ui,sans-serif;text-align:center;margin-top:4em\">\
         <p>{page}</p></body></html>"
    );
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn react(request_line: &str, expected_state: &str) -> Reaction {
    let mut parts = request_line.split(' ');
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if method != "GET" || path != "/callback" {
        return Reaction::NotCallback;
    }
    let params = parse_query(query);
    let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    if get("state") != Some(expected_state) {
        return Reaction::Callback {
            page: "This sign-in request does not belong to CaptureMann. You can close this tab.",
            status: 400,
            result: Err(CmError::invalid_input(
                "The sign-in response did not match the request (state mismatch); it was rejected.",
            )),
        };
    }
    if let Some(err) = get("error") {
        let code: String = err.chars().take(64).collect();
        return Reaction::Callback {
            page: "Sign-in was cancelled. You can close this tab.",
            status: 200,
            result: Err(CmError::platform_rejected(
                format!("The provider refused the sign-in: {code}"),
                Some(code),
            )),
        };
    }
    match get("code") {
        Some(code) if !code.is_empty() => Reaction::Callback {
            page: "You are signed in. You can close this tab and go back to CaptureMann.",
            status: 200,
            result: Ok(code.to_string()),
        },
        _ => Reaction::Callback {
            page: "The sign-in response had no code. You can close this tab.",
            status: 400,
            result: Err(CmError::invalid_input(
                "The sign-in response had no authorization code.",
            )),
        },
    }
}

/// Splits and decodes a query string (`+` is a space).
pub(crate) fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

fn decode(s: &str) -> String {
    percent_decode_str(&s.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::ErrorCode;

    async fn get(port: u16, target: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out).await;
        out
    }

    #[tokio::test]
    async fn returns_the_code_and_shows_the_close_tab_page() {
        let l = LoopbackListener::bind().await.unwrap();
        let port = l.port();
        assert_eq!(
            l.redirect_uri(),
            format!("http://127.0.0.1:{port}/callback")
        );
        let waiter = tokio::spawn(l.wait_for_code("st", Duration::from_secs(5)));
        // A favicon request first is ignored.
        assert!(get(port, "/favicon.ico").await.starts_with("HTTP/1.1 404"));
        let page = get(port, "/callback?code=a%2Fb+c&state=st").await;
        assert!(page.starts_with("HTTP/1.1 200"));
        assert!(page.contains("You can close this tab"));
        assert_eq!(waiter.await.unwrap().unwrap(), "a/b c");
    }

    #[tokio::test]
    async fn state_mismatch_is_rejected() {
        let l = LoopbackListener::bind().await.unwrap();
        let port = l.port();
        let waiter = tokio::spawn(l.wait_for_code("good", Duration::from_secs(5)));
        let page = get(port, "/callback?code=stolen&state=evil").await;
        assert!(page.starts_with("HTTP/1.1 400"));
        let err = waiter.await.unwrap().unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(!err.to_string().contains("stolen"));
    }

    #[tokio::test]
    async fn missing_state_is_rejected() {
        let l = LoopbackListener::bind().await.unwrap();
        let port = l.port();
        let waiter = tokio::spawn(l.wait_for_code("good", Duration::from_secs(5)));
        get(port, "/callback?code=c").await;
        assert_eq!(
            waiter.await.unwrap().unwrap_err().code(),
            ErrorCode::InvalidInput
        );
    }

    #[tokio::test]
    async fn provider_error_carries_its_code() {
        let l = LoopbackListener::bind().await.unwrap();
        let port = l.port();
        let waiter = tokio::spawn(l.wait_for_code("s", Duration::from_secs(5)));
        get(port, "/callback?error=access_denied&state=s").await;
        let err = waiter.await.unwrap().unwrap_err();
        assert_eq!(err.code(), ErrorCode::PlatformRejected);
        assert_eq!(
            err.data().unwrap()["platform_code"].as_str(),
            Some("access_denied")
        );
    }

    #[tokio::test]
    async fn times_out_without_a_callback() {
        let l = LoopbackListener::bind().await.unwrap();
        let err = l
            .wait_for_code("s", Duration::from_millis(100))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::UserDeclined);
    }

    #[tokio::test]
    async fn second_callback_is_ignored() {
        let l = LoopbackListener::bind().await.unwrap();
        let port = l.port();
        let waiter = tokio::spawn(l.wait_for_code("s", Duration::from_secs(5)));
        get(port, "/callback?code=first&state=s").await;
        // The listener is gone: a second callback (right or wrong) cannot connect or change anything.
        let second = TcpStream::connect(("127.0.0.1", port)).await;
        if let Ok(mut s) = second {
            let _ = s
                .write_all(b"GET /callback?code=second&state=s HTTP/1.1\r\n\r\n")
                .await;
            let mut out = String::new();
            let _ = s.read_to_string(&mut out).await;
            assert!(!out.starts_with("HTTP/1.1 200"));
        }
        assert_eq!(waiter.await.unwrap().unwrap(), "first");
    }
}
