//! An in-process Microsoft Graph (OneDrive) for tests: folder lookup and creation by item, the
//! upload session (`createUploadSession`, chunks with `Content-Range`, status query), the simple
//! `PUT .../content`, `createLink`, metadata and delete, with failures a test can inject. It
//! records protocol breaches (a chunk that is not a multiple of 320 KiB, a chunk over 60 MiB, an
//! `Authorization` header on the session URL). No network, no account.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use serde_json::{json, Value};

use crate::fake_drive::{FakeFile, Fault};
use crate::http::{HttpReply, HttpRequest, StorageHttp, TransportError};

pub const CHUNK_UNIT: u64 = 320 * 1024;
const MAX_CHUNK: u64 = 60 * 1024 * 1024;

/// Which call a fault applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Call {
    FolderGet,
    FolderCreate,
    Init,
    Chunk,
    StatusQuery,
    SimplePut,
    CreateLink,
    Meta,
    Delete,
    Me,
    DriveInfo,
}

#[derive(Debug, Clone)]
struct Rule {
    call: Call,
    /// For chunks: only when the chunk starts at or after this byte.
    from_offset: u64,
    fault: Fault,
    times: usize,
}

#[derive(Debug)]
struct Session {
    name: String,
    parent: String,
    received: Vec<u8>,
    total: u64,
    finished: bool,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    files: Vec<FakeFile>,
    sessions: HashMap<String, Session>,
    rules: Vec<Rule>,
    valid_token: String,
    mail: Option<String>,
    upn: String,
    drive_type: String,
    deny_profile: bool,
    quota: Option<u64>,
    stored_total: u64,
    deny_anonymous: bool,
    deny_all_links: bool,
    size_skew: i64,
    /// `(item id, scope)` of every link made.
    links: Vec<(String, String)>,
    /// `(start, len, total)` of every chunk that was processed.
    chunks: Vec<(u64, u64, u64)>,
    violations: Vec<String>,
    calls: HashMap<Call, usize>,
    log: Vec<String>,
}

pub struct FakeOneDrive {
    state: Mutex<State>,
}

impl Default for FakeOneDrive {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeOneDrive {
    pub fn new() -> Self {
        FakeOneDrive {
            state: Mutex::new(State {
                valid_token: "tok-1".to_owned(),
                mail: Some("teacher@example.org".to_owned()),
                upn: "teacher@example.org".to_owned(),
                drive_type: "business".to_owned(),
                ..State::default()
            }),
        }
    }

    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- scripting ---------------------------------------------------------------------------

    /// The only bearer token the API accepts; others get 401.
    pub fn set_valid_token(&self, t: &str) {
        self.st().valid_token = t.to_owned();
    }

    /// What `/me` reports: the mailbox address (none for an account without a mailbox) and the
    /// user principal name.
    pub fn set_account(&self, mail: Option<&str>, upn: &str) {
        let mut s = self.st();
        s.mail = mail.map(str::to_owned);
        s.upn = upn.to_owned();
    }

    /// `/me` answers 403 (the token has no permission to read the profile).
    pub fn deny_profile(&self) {
        self.st().deny_profile = true;
    }

    /// `personal`, `business` or `documentLibrary`.
    pub fn set_drive_type(&self, t: &str) {
        self.st().drive_type = t.to_owned();
    }

    /// Fails the next `times` matching calls (chunks: only those starting at `from_offset` or later).
    pub fn inject(&self, call: Call, from_offset: u64, fault: Fault, times: usize) {
        self.st().rules.push(Rule {
            call,
            from_offset,
            fault,
            times,
        });
    }

    /// Total bytes the account may store; a session that would exceed it gets 507.
    pub fn set_quota(&self, bytes: u64) {
        self.st().quota = Some(bytes);
    }

    /// Tenant policy: `createLink` with scope `anonymous` answers 403.
    pub fn deny_anonymous_links(&self) {
        self.st().deny_anonymous = true;
    }

    /// Tenant policy: every `createLink` answers 403.
    pub fn deny_all_links(&self) {
        self.st().deny_all_links = true;
    }

    /// The final answer reports `size + skew` bytes.
    pub fn skew_final_size(&self, skew: i64) {
        self.st().size_skew = skew;
    }

    /// Forgets every upload session (they expire).
    pub fn expire_sessions(&self) {
        self.st().sessions.clear();
    }

    // ---- inspection --------------------------------------------------------------------------

    pub fn files(&self) -> Vec<FakeFile> {
        self.st()
            .files
            .iter()
            .filter(|f| !f.is_folder)
            .cloned()
            .collect()
    }

    pub fn folders(&self) -> Vec<FakeFile> {
        self.st()
            .files
            .iter()
            .filter(|f| f.is_folder)
            .cloned()
            .collect()
    }

    /// `CaptureMann/2026/10/Lesson` style path of an item id.
    pub fn path_of(&self, id: &str) -> String {
        let s = self.st();
        let mut parts = Vec::new();
        let mut cur = id.to_owned();
        while let Some(f) = s.files.iter().find(|f| f.id == cur) {
            parts.push(f.name.clone());
            cur = f.parent.clone();
        }
        parts.reverse();
        parts.join("/")
    }

    pub fn chunks(&self) -> Vec<(u64, u64, u64)> {
        self.st().chunks.clone()
    }

    pub fn violations(&self) -> Vec<String> {
        self.st().violations.clone()
    }

    pub fn count(&self, call: Call) -> usize {
        self.st().calls.get(&call).copied().unwrap_or(0)
    }

    /// `(item id, scope)` of every link made.
    pub fn links(&self) -> Vec<(String, String)> {
        self.st().links.clone()
    }

    pub fn sessions_started(&self) -> usize {
        self.count(Call::Init)
    }

    pub fn log(&self) -> Vec<String> {
        self.st().log.clone()
    }
}

fn error_body(status: u16, code: &str, message: &str) -> HttpReply {
    HttpReply::new(status)
        .body(json!({ "error": { "code": code, "message": message } }).to_string())
}

fn json_reply(status: u16, v: Value) -> HttpReply {
    HttpReply::new(status)
        .header("Content-Type", "application/json")
        .body(v.to_string())
}

fn decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// `Content-Range: bytes a-b/total`.
fn parse_range(h: &str) -> Option<(u64, u64, u64)> {
    let r = h.strip_prefix("bytes ")?;
    let (span, total) = r.split_once('/')?;
    let (a, b) = span.split_once('-')?;
    Some((a.parse().ok()?, b.parse().ok()?, total.parse().ok()?))
}

/// `items/{parent}:/{name}[:/{op}]` -> (parent, name, op).
fn parse_item_path(rest: &str) -> Option<(String, String, String)> {
    let (parent, tail) = rest.split_once(":/")?;
    let (name, op) = match tail.split_once(":/") {
        Some((n, op)) => (n, op),
        None => (tail.trim_end_matches(':'), ""),
    };
    Some((decode(parent), decode(name), op.to_owned()))
}

fn item_json(f: &FakeFile, size: i64) -> Value {
    json!({
        "id": f.id,
        "name": f.name,
        "size": size,
        "webUrl": format!("https://onedrive.fake/item/{}", f.id),
        "folder": if f.is_folder { json!({}) } else { Value::Null },
    })
}

impl State {
    fn add_file(&mut self, name: &str, parent: &str, data: Vec<u8>, is_folder: bool) -> FakeFile {
        // conflictBehavior=rename
        let mut final_name = name.to_owned();
        let mut n = 0;
        while self
            .files
            .iter()
            .any(|f| f.parent == parent && f.name == final_name)
        {
            n += 1;
            final_name = match name.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem} {n}.{ext}"),
                None => format!("{name} {n}"),
            };
        }
        self.next += 1;
        let f = FakeFile {
            id: format!(
                "{}-{}",
                if is_folder { "folder" } else { "file" },
                self.next
            ),
            name: final_name,
            parent: parent.to_owned(),
            is_folder,
            data,
        };
        self.files.push(f.clone());
        f
    }
}

#[async_trait]
impl StorageHttp for FakeOneDrive {
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError> {
        let mut s = self.st();
        let url = req.url.split('#').next().unwrap_or("").to_owned();
        let (path, _query) = url.split_once('?').unwrap_or((url.as_str(), ""));
        let is_session = path.starts_with("https://fake.onedrive/session/");
        let api_path = path
            .strip_prefix("https://graph.microsoft.com/v1.0/me/drive/")
            .map(str::to_owned);
        s.log.push(format!("{} {}", req.method, path));

        let call = if path == "https://graph.microsoft.com/v1.0/me" && req.method == "GET" {
            Some(Call::Me)
        } else if path == "https://graph.microsoft.com/v1.0/me/drive" && req.method == "GET" {
            Some(Call::DriveInfo)
        } else if is_session {
            match req.method.as_str() {
                "PUT" => Some(Call::Chunk),
                "GET" => Some(Call::StatusQuery),
                _ => None,
            }
        } else if let Some(p) = &api_path {
            match (req.method.as_str(), p.as_str()) {
                ("GET", p) if p.starts_with("items/") && p.contains(":/") => Some(Call::FolderGet),
                ("POST", p) if p.ends_with("/children") => Some(Call::FolderCreate),
                ("POST", p) if p.ends_with(":/createUploadSession") => Some(Call::Init),
                ("PUT", p) if p.ends_with(":/content") => Some(Call::SimplePut),
                ("POST", p) if p.ends_with("/createLink") => Some(Call::CreateLink),
                ("GET", p) if p.starts_with("items/") => Some(Call::Meta),
                ("DELETE", p) if p.starts_with("items/") => Some(Call::Delete),
                _ => None,
            }
        } else {
            None
        };
        let Some(call) = call else {
            return Ok(error_body(404, "itemNotFound", "no such route"));
        };
        *s.calls.entry(call).or_default() += 1;

        if is_session {
            // The pre-authenticated upload URL must not get a bearer token.
            if req.header_value("authorization").is_some() {
                s.violations
                    .push("Authorization header sent to the upload URL".to_owned());
            }
        } else if req.header_value("authorization") != Some(&format!("Bearer {}", s.valid_token)) {
            return Ok(error_body(
                401,
                "InvalidAuthenticationToken",
                "Access token invalid",
            ));
        }

        // Injected faults.
        let start = req
            .header_value("content-range")
            .and_then(parse_range)
            .map(|r| r.0)
            .unwrap_or(0);
        let hit = s.rules.iter().position(|r| {
            r.times > 0 && r.call == call && (call != Call::Chunk || start >= r.from_offset)
        });
        let mut drop_after = false;
        if let Some(i) = hit {
            s.rules[i].times -= 1;
            match s.rules[i].fault.clone() {
                Fault::DropBefore => return Err(TransportError("connection reset".into())),
                Fault::DropAfter => drop_after = true,
                Fault::Status {
                    status,
                    retry_after,
                    reason,
                    message,
                } => {
                    let mut r = error_body(status, reason, message);
                    if let Some(n) = retry_after {
                        r = r.header("Retry-After", n.to_string());
                    }
                    return Ok(r);
                }
            }
        }

        let reply = match call {
            Call::Me if s.deny_profile => error_body(403, "accessDenied", "Access denied"),
            Call::Me => json_reply(
                200,
                json!({ "userPrincipalName": s.upn, "mail": s.mail, "displayName": "Fake User" }),
            ),
            Call::DriveInfo => json_reply(
                200,
                json!({
                    "driveType": s.drive_type,
                    "owner": { "user": { "displayName": "Fake User", "email": s.mail } },
                }),
            ),
            Call::FolderGet => {
                let rest = api_path
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches("items/");
                let (parent, name, _) = parse_item_path(rest).unwrap_or_default();
                let hit = s
                    .files
                    .iter()
                    .find(|f| f.is_folder && f.name == name && f.parent == parent);
                match hit {
                    Some(f) => json_reply(200, item_json(f, 0)),
                    None => error_body(404, "itemNotFound", "The resource could not be found."),
                }
            }
            Call::FolderCreate => {
                let parent = api_path
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches("items/")
                    .trim_end_matches("/children")
                    .to_owned();
                let body: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                    .unwrap_or(Value::Null);
                let name = body["name"].as_str().unwrap_or("").to_owned();
                if s.files
                    .iter()
                    .any(|f| f.parent == parent && f.name == name && f.is_folder)
                {
                    error_body(409, "nameAlreadyExists", "The name already exists")
                } else {
                    let f = s.add_file(&name, &parent, Vec::new(), true);
                    json_reply(201, item_json(&f, 0))
                }
            }
            Call::Init => {
                let rest = api_path
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches("items/");
                let (parent, name, _) = parse_item_path(rest).unwrap_or_default();
                let body: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                    .unwrap_or(Value::Null);
                let total = body["fileSize"].as_u64().unwrap_or(0);
                if s.quota.is_some_and(|q| s.stored_total + total > q) {
                    error_body(
                        507,
                        "quotaLimitReached",
                        "Insufficient Storage: the quota is exceeded.",
                    )
                } else {
                    s.next += 1;
                    let uri = format!("https://fake.onedrive/session/{}", s.next);
                    s.sessions.insert(
                        uri.clone(),
                        Session {
                            name,
                            parent,
                            received: Vec::new(),
                            total,
                            finished: false,
                        },
                    );
                    json_reply(
                        200,
                        json!({ "uploadUrl": uri, "expirationDateTime": "2026-11-01T00:00:00Z" }),
                    )
                }
            }
            Call::Chunk | Call::StatusQuery => {
                let key = url.clone();
                let ranges = |got: u64, total: u64| {
                    let next = if got >= total {
                        json!([])
                    } else {
                        json!([format!("{got}-")])
                    };
                    json!({ "expirationDateTime": "2026-11-01T00:00:00Z", "nextExpectedRanges": next })
                };
                match s.sessions.get(&key) {
                    None => error_body(404, "itemNotFound", "The upload session was not found"),
                    Some(sess) if sess.finished => {
                        error_body(404, "itemNotFound", "The upload session was not found")
                    }
                    Some(sess) if call == Call::StatusQuery => {
                        json_reply(200, ranges(sess.received.len() as u64, sess.total))
                    }
                    Some(_) => {
                        let data = req.body.read().unwrap_or_default();
                        match req.header_value("content-range").and_then(parse_range) {
                            None => error_body(400, "invalidRange", "Bad Content-Range"),
                            Some((a, b, total)) => {
                                let sess = s.sessions.get_mut(&key).expect("checked above");
                                let got = sess.received.len() as u64;
                                if a != got || b + 1 - a != data.len() as u64 || total != sess.total
                                {
                                    let r = ranges(got, sess.total);
                                    json_reply(416, r)
                                } else {
                                    let last = b + 1 == total;
                                    sess.received.extend_from_slice(&data);
                                    let (name, parent, received) = (
                                        sess.name.clone(),
                                        sess.parent.clone(),
                                        sess.received.clone(),
                                    );
                                    let len = data.len() as u64;
                                    if !last && !len.is_multiple_of(CHUNK_UNIT) {
                                        s.violations.push(format!(
                                            "non-final chunk of {len} bytes is not a multiple of 320 KiB"
                                        ));
                                    }
                                    if len >= MAX_CHUNK {
                                        s.violations.push(format!(
                                            "chunk of {len} bytes is not under 60 MiB"
                                        ));
                                    }
                                    s.chunks.push((a, len, total));
                                    if last {
                                        s.stored_total += total;
                                        let f = s.add_file(&name, &parent, received, false);
                                        s.sessions.get_mut(&key).expect("session").finished = true;
                                        let skew = s.size_skew;
                                        json_reply(201, item_json(&f, f.data.len() as i64 + skew))
                                    } else {
                                        let sess = &s.sessions[&key];
                                        json_reply(
                                            202,
                                            ranges(sess.received.len() as u64, sess.total),
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Call::SimplePut => {
                let rest = api_path
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches("items/");
                let (parent, name, _) = parse_item_path(rest).unwrap_or_default();
                let data = req.body.read().unwrap_or_default();
                if data.len() > 4_000_000 {
                    s.violations.push("simple PUT over 4 MB".to_owned());
                }
                if s.quota
                    .is_some_and(|q| s.stored_total + data.len() as u64 > q)
                {
                    error_body(
                        507,
                        "quotaLimitReached",
                        "Insufficient Storage: the quota is exceeded.",
                    )
                } else {
                    s.stored_total += data.len() as u64;
                    let skew = s.size_skew;
                    let f = s.add_file(&name, &parent, data, false);
                    json_reply(201, item_json(&f, f.data.len() as i64 + skew))
                }
            }
            Call::CreateLink => {
                let id = api_path
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches("items/")
                    .trim_end_matches("/createLink")
                    .to_owned();
                let body: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                    .unwrap_or(Value::Null);
                let scope = body["scope"].as_str().unwrap_or("").to_owned();
                let kind = body["type"].as_str().unwrap_or("");
                if kind != "view" {
                    error_body(400, "invalidRequest", "unsupported link type")
                } else if s.deny_all_links || (s.deny_anonymous && scope == "anonymous") {
                    error_body(
                        403,
                        "accessDenied",
                        "Creating anonymous links is disabled by your administrator.",
                    )
                } else {
                    s.links.push((id.clone(), scope.clone()));
                    json_reply(
                        201,
                        json!({ "id": "link", "link": { "type": kind, "scope": scope,
                            "webUrl": format!("https://1drv.fake/{scope}/{id}") } }),
                    )
                }
            }
            Call::Meta => {
                let id = api_path
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches("items/");
                match s.files.iter().find(|f| f.id == id) {
                    Some(f) => json_reply(200, item_json(f, f.data.len() as i64)),
                    None => error_body(404, "itemNotFound", "The resource could not be found."),
                }
            }
            Call::Delete => {
                let id = api_path
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches("items/")
                    .to_owned();
                let before = s.files.len();
                s.files.retain(|f| f.id != id);
                if s.files.len() == before {
                    error_body(404, "itemNotFound", "The resource could not be found.")
                } else {
                    HttpReply::new(204)
                }
            }
        };
        if drop_after {
            return Err(TransportError(
                "connection reset while reading the answer".into(),
            ));
        }
        Ok(reply)
    }
}
