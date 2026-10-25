//! The HTTP seam of the cloud providers: a generic request (method, url, headers, a body that is
//! empty, bytes or a range of a file) and reply, so tests use an in-process fake server and the
//! product uses `ureq` with the system TLS stack (as cm-auth and cm-ai do).

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// What goes in the request body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    /// `len` bytes of `path` starting at `offset`, read when the request is sent.
    FileRange {
        path: PathBuf,
        offset: u64,
        len: u64,
    },
}

impl Body {
    pub fn len(&self) -> u64 {
        match self {
            Body::Empty => 0,
            Body::Bytes(b) => b.len() as u64,
            Body::FileRange { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The body bytes (reads the file range).
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        match self {
            Body::Empty => Ok(Vec::new()),
            Body::Bytes(b) => Ok(b.clone()),
            Body::FileRange { path, offset, len } => {
                let mut f = std::fs::File::open(path)?;
                f.seek(SeekFrom::Start(*offset))?;
                let mut buf = Vec::new();
                f.take(*len).read_to_end(&mut buf)?;
                if buf.len() as u64 != *len {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "the file is shorter than it was",
                    ));
                }
                Ok(buf)
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl HttpRequest {
    pub fn new(method: &str, url: impl Into<String>) -> Self {
        HttpRequest {
            method: method.to_owned(),
            url: url.into(),
            headers: Vec::new(),
            body: Body::Empty,
        }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    pub fn body(mut self, body: Body) -> Self {
        self.body = body;
        self
    }

    /// The first header called `name` (case-insensitive).
    pub fn header_value(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Headers carry the bearer token.
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field(
                "headers",
                &format_args!("[{} redacted]", self.headers.len()),
            )
            .field("body_len", &self.body.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpReply {
    pub fn new(status: u16) -> Self {
        HttpReply {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// A transport failure (no reply): DNS, connect, TLS, timeout, a dropped connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

#[async_trait]
pub trait StorageHttp: Send + Sync {
    /// Any HTTP status is a reply, not an error (including 308).
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError>;
}

/// `ureq` with the system TLS stack, run on `spawn_blocking`.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // 308 is Drive's "resume incomplete", not a redirect.
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            // One chunk of 8 MiB on a slow line.
            .timeout_global(Some(Duration::from_secs(15 * 60)))
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .into();
        UreqHttp { agent }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

fn scrub(s: &str) -> String {
    cm_jobs::scrub_secrets(s).chars().take(200).collect()
}

#[async_trait]
impl StorageHttp for UreqHttp {
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError> {
        let agent = self.agent.clone();
        tokio::task::spawn_blocking(move || {
            let bytes = req
                .body
                .read()
                .map_err(|e| TransportError(format!("cannot read the file: {e}")))?;
            let method = ureq::http::Method::from_bytes(req.method.as_bytes())
                .map_err(|e| TransportError(e.to_string()))?;
            let mut builder = ureq::http::Request::builder()
                .method(method)
                .uri(req.url.as_str());
            for (k, v) in &req.headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
            let request = builder
                .body(bytes)
                .map_err(|e| TransportError(scrub(&e.to_string())))?;
            let mut resp = agent
                .run(request)
                .map_err(|e| TransportError(scrub(&e.to_string())))?;
            let status = resp.status().as_u16();
            let headers = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned())))
                .collect();
            let body = resp
                .body_mut()
                .read_to_vec()
                .map_err(|e| TransportError(scrub(&e.to_string())))?;
            Ok(HttpReply {
                status,
                headers,
                body,
            })
        })
        .await
        .map_err(|e| TransportError(format!("http task: {e}")))?
    }
}
