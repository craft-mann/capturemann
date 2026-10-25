//! Local Streamable HTTP transport (AGT-1.AC3).
//!
//! Binds a loopback address only (`127.0.0.1` / `::1`; anything else is refused), and every
//! request must carry `Authorization: Bearer <token>`: a missing or wrong token is `401`
//! before the MCP layer sees the request. `rmcp` additionally validates the `Host` header
//! against loopback names (DNS-rebinding protection). Each MCP session gets its own
//! [`McpServer`] and therefore its own daemon connection, named after that session's client.
//!
//! Resource limits (R2-6): at most [`MAX_HTTP_CONNECTIONS`] simultaneous connections (more are
//! closed at once) and a [`HEADER_READ_TIMEOUT`] to receive each request's headers, so idle or
//! trickling local connections cannot pile up. A client holding an SSE stream is not "idle": it
//! is a response in progress.
//!
//! Where the token is stored (the keychain, shown once in Settings -> Agents) is the caller's
//! business: this module takes it as a string. [`generate_token`] makes a random one.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http::{header, Request, Response, StatusCode};
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::conn::Connector;
use crate::server::McpServer;

/// Most simultaneous HTTP connections (a connection is cheap, an MCP session is not).
pub const MAX_HTTP_CONNECTIONS: usize = 64;
/// How long a client may take to send the headers of one request.
pub const HEADER_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Options of the HTTP transport.
#[derive(Debug, Clone)]
pub struct HttpOptions {
    /// Must be a loopback address; port 0 picks a random free port.
    pub addr: SocketAddr,
    /// The bearer token clients must send. Must not be empty.
    pub token: String,
}

impl HttpOptions {
    /// `127.0.0.1` on a random port.
    pub fn loopback(token: String) -> Self {
        HttpOptions {
            addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            token,
        }
    }
}

/// A running HTTP server. Dropping it does not stop it; call [`HttpServer::shutdown`].
pub struct HttpServer {
    local_addr: SocketAddr,
    task: JoinHandle<()>,
}

impl HttpServer {
    /// The bound address (with the real port when 0 was requested).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stops accepting connections.
    pub fn shutdown(self) {
        self.task.abort();
    }

    /// Waits until the accept loop ends (it only ends on shutdown).
    pub async fn wait(self) {
        let _ = self.task.await;
    }
}

/// A 256-bit random token, hex encoded.
pub fn generate_token() -> std::io::Result<String> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

fn authorized<B>(req: &Request<B>, token: &str) -> bool {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            let (scheme, rest) = v.split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then_some(rest.trim())
        })
        .is_some_and(|t| ct_eq(t.as_bytes(), token.as_bytes()))
}

fn unauthorized() -> Response<BoxBody<Bytes, Infallible>> {
    let body = Full::new(Bytes::from_static(b"missing or invalid bearer token"))
        .map_err(|never| match never {})
        .boxed();
    let mut resp = Response::new(body);
    *resp.status_mut() = StatusCode::UNAUTHORIZED;
    resp.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer"),
    );
    resp
}

/// Starts the local HTTP server (`POST/GET/DELETE` on any path; clients use `/mcp`).
pub async fn serve_http(connector: Connector, options: HttpOptions) -> std::io::Result<HttpServer> {
    if !options.addr.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the MCP HTTP server only binds loopback addresses",
        ));
    }
    if options.token.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a bearer token is required",
        ));
    }
    let listener = TcpListener::bind(options.addr).await?;
    let local_addr = listener.local_addr()?;
    let service = StreamableHttpService::new(
        move || Ok(McpServer::new(connector.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let token: Arc<str> = Arc::from(options.token);
    let slots = Arc::new(Semaphore::new(MAX_HTTP_CONNECTIONS));
    let task = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            // Over the cap: close the connection at once.
            let Ok(permit) = slots.clone().try_acquire_owned() else {
                drop(stream);
                continue;
            };
            let service = service.clone();
            let token = token.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                    let service = service.clone();
                    let token = token.clone();
                    async move {
                        if !authorized(&req, &token) {
                            return Ok::<_, Infallible>(unauthorized());
                        }
                        Ok(service.handle(req).await)
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .timer(hyper_util::rt::TokioTimer::new())
                    .header_read_timeout(HEADER_READ_TIMEOUT)
                    .serve_connection(TokioIo::new(stream), svc)
                    .await;
            });
        }
    });
    Ok(HttpServer { local_addr, task })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_compare() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(!ct_eq(b"", b"a"));
    }

    #[test]
    fn bearer_parsing() {
        let req = |v: &str| {
            Request::builder()
                .header("authorization", v)
                .body(())
                .unwrap()
        };
        assert!(authorized(&req("Bearer t0k"), "t0k"));
        assert!(authorized(&req("bearer t0k"), "t0k"));
        assert!(!authorized(&req("Bearer nope"), "t0k"));
        assert!(!authorized(&req("Basic t0k"), "t0k"));
        assert!(!authorized(&Request::builder().body(()).unwrap(), "t0k"));
    }

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (generate_token().unwrap(), generate_token().unwrap());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
