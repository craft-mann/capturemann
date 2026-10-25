//! An in-process Google Drive for tests: implements the folder search and create calls, the
//! resumable session (init, chunk, status query), permissions and delete, with failures that a
//! test can inject. No network, no account.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use cm_core::CmError;
use percent_encoding::percent_decode_str;
use serde_json::{json, Value};

use crate::http::{HttpReply, HttpRequest, StorageHttp, TransportError};
use crate::TokenSource;

/// Which call a fault applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Call {
    FolderSearch,
    FolderCreate,
    Init,
    Chunk,
    StatusQuery,
    Permission,
    Meta,
    Delete,
    About,
}

#[derive(Debug, Clone)]
pub enum Fault {
    /// The request never arrives.
    DropBefore,
    /// The request is processed, the answer is lost.
    DropAfter,
    /// An error status instead of processing.
    Status {
        status: u16,
        retry_after: Option<u64>,
        reason: &'static str,
        message: &'static str,
    },
}

#[derive(Debug, Clone)]
struct Rule {
    call: Call,
    /// Matching calls to let through before the fault starts.
    skip: usize,
    /// For chunks: only when the chunk starts at or after this byte.
    from_offset: u64,
    fault: Fault,
    times: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeFile {
    pub id: String,
    pub name: String,
    pub parent: String,
    pub is_folder: bool,
    pub data: Vec<u8>,
}

#[derive(Debug)]
struct Session {
    /// The file a PATCH session replaces.
    replace: Option<String>,
    name: String,
    parent: String,
    received: Vec<u8>,
    finished: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    files: Vec<FakeFile>,
    sessions: HashMap<String, Session>,
    rules: Vec<Rule>,
    valid_token: String,
    email: String,
    quota: Option<u64>,
    stored_total: u64,
    deny_public: bool,
    size_skew: i64,
    permissions: Vec<(String, String, String)>,
    /// Folder names for which a rival "client" creates the same name just before ours.
    race_names: Vec<String>,
    trashed: Vec<String>,
    /// `(start, len, total)` of every chunk that was processed.
    chunks: Vec<(u64, u64, u64)>,
    violations: Vec<String>,
    calls: HashMap<Option<Call>, usize>,
    log: Vec<String>,
}

type Hook = (Call, usize, Option<Box<dyn FnOnce() + Send>>);

pub struct FakeDrive {
    state: Mutex<State>,
    /// `(call, nth, action)`: runs `action` (once) when the `nth` call of that kind arrives,
    /// before it is processed. The action must not call back into this fake.
    hooks: Mutex<Vec<Hook>>,
}

impl Default for FakeDrive {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDrive {
    pub fn new() -> Self {
        FakeDrive {
            state: Mutex::new(State {
                valid_token: "tok-1".to_owned(),
                email: "teacher@example.org".to_owned(),
                ..State::default()
            }),
            hooks: Mutex::new(Vec::new()),
        }
    }

    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- scripting ---------------------------------------------------------------------------

    /// The only bearer token the server accepts; others get 401.
    pub fn set_valid_token(&self, t: &str) {
        self.st().valid_token = t.to_owned();
    }

    /// The address `about` reports for the signed-in user.
    pub fn set_account_email(&self, email: &str) {
        self.st().email = email.to_owned();
    }

    /// Like [`inject`](Self::inject) but lets the first `skip` matching calls through.
    pub fn inject_after(&self, call: Call, skip: usize, fault: Fault, times: usize) {
        self.st().rules.push(Rule {
            call,
            skip,
            from_offset: 0,
            fault,
            times,
        });
    }

    /// Runs `action` once when the `nth` (1-based) call of kind `call` arrives, before the fake
    /// processes it: a test uses it to change a file on disk at an exact point of an upload.
    pub fn on_nth_call(&self, call: Call, nth: usize, action: impl FnOnce() + Send + 'static) {
        self.hooks.lock().unwrap_or_else(|e| e.into_inner()).push((
            call,
            nth,
            Some(Box::new(action)),
        ));
    }

    /// Fails the next `times` matching calls (chunks: only those starting at `from_offset` or later).
    pub fn inject(&self, call: Call, from_offset: u64, fault: Fault, times: usize) {
        self.st().rules.push(Rule {
            call,
            skip: 0,
            from_offset,
            fault,
            times,
        });
    }

    /// Total bytes the account may store; a session that would exceed it gets 403.
    pub fn set_quota(&self, bytes: u64) {
        self.st().quota = Some(bytes);
    }

    /// Organisation policy: creating an "anyone" permission answers 403.
    pub fn deny_public_links(&self) {
        self.st().deny_public = true;
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

    /// When a folder called `name` is created, another client created one with the same name a
    /// moment earlier (the check-then-create race of two machines).
    pub fn race_folder_create(&self, name: &str) {
        self.st().race_names.push(name.to_owned());
    }

    /// Puts a folder in the trash (it stays, but no search finds it and `trashed` is true).
    pub fn trash(&self, id: &str) {
        self.st().trashed.push(id.to_owned());
    }

    /// Moves a file or folder to another parent.
    pub fn move_to(&self, id: &str, parent: &str) {
        if let Some(f) = self.st().files.iter_mut().find(|f| f.id == id) {
            f.parent = parent.to_owned();
        }
    }

    /// `CaptureMann/2026/10/Lesson` style path of a file or folder id.
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

    pub fn folders(&self) -> Vec<FakeFile> {
        self.st()
            .files
            .iter()
            .filter(|f| f.is_folder)
            .cloned()
            .collect()
    }

    pub fn chunks(&self) -> Vec<(u64, u64, u64)> {
        self.st().chunks.clone()
    }

    /// Protocol breaches seen (a non-final chunk that is not a multiple of 256 KiB, ...).
    pub fn violations(&self) -> Vec<String> {
        self.st().violations.clone()
    }

    pub fn count(&self, call: Call) -> usize {
        self.st().calls.get(&Some(call)).copied().unwrap_or(0)
    }

    pub fn permissions(&self) -> Vec<(String, String, String)> {
        self.st().permissions.clone()
    }

    pub fn sessions_started(&self) -> usize {
        self.count(Call::Init)
    }

    pub fn log(&self) -> Vec<String> {
        self.st().log.clone()
    }
}

fn error_body(status: u16, reason: &str, message: &str) -> HttpReply {
    HttpReply::new(status).body(
        json!({
            "error": { "code": status, "message": message, "errors": [{ "reason": reason }] }
        })
        .to_string(),
    )
}

fn json_reply(v: Value) -> HttpReply {
    HttpReply::new(200)
        .header("Content-Type", "application/json")
        .body(v.to_string())
}

fn split_url(url: &str) -> (String, HashMap<String, String>) {
    let url = url.split('#').next().unwrap_or(url);
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let path = path
        .strip_prefix("https://www.googleapis.com")
        .or_else(|| path.strip_prefix("https://fake.drive"))
        .unwrap_or(path);
    let q = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (
                k.to_owned(),
                percent_decode_str(v).decode_utf8_lossy().into_owned(),
            )
        })
        .collect();
    (path.to_owned(), q)
}

/// `name='X' and 'P' in parents ...` -> (X, P), undoing the `\'` and `\\` escapes.
fn parse_folder_query(q: &str) -> Option<(String, String)> {
    fn quoted(s: &str) -> Option<(String, &str)> {
        let s = s.strip_prefix('\'')?;
        let mut out = String::new();
        let mut it = s.char_indices();
        while let Some((i, c)) = it.next() {
            match c {
                '\\' => out.push(it.next()?.1),
                '\'' => return Some((out, &s[i + 1..])),
                _ => out.push(c),
            }
        }
        None
    }
    let (name, rest) = quoted(q.strip_prefix("name=")?)?;
    let (parent, _) = quoted(rest.strip_prefix(" and ")?)?;
    Some((name, parent))
}

fn parse_range(h: &str) -> Option<(Option<(u64, u64)>, u64)> {
    let r = h.strip_prefix("bytes ")?;
    let (span, total) = r.split_once('/')?;
    let total = total.parse().ok()?;
    if span == "*" {
        return Some((None, total));
    }
    let (a, b) = span.split_once('-')?;
    Some((Some((a.parse().ok()?, b.parse().ok()?)), total))
}

#[async_trait]
impl StorageHttp for FakeDrive {
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError> {
        let mut s = self.st();
        let (path, q) = split_url(&req.url);
        s.log.push(format!("{} {}", req.method, path));
        let call = match (req.method.as_str(), path.as_str()) {
            ("GET", "/drive/v3/about") => Some(Call::About),
            ("GET", "/drive/v3/files") => Some(Call::FolderSearch),
            ("POST", "/drive/v3/files") => Some(Call::FolderCreate),
            ("POST", "/upload/drive/v3/files") => Some(Call::Init),
            ("PATCH", p) if p.starts_with("/upload/drive/v3/files/") => Some(Call::Init),
            ("PUT", p) if p.starts_with("/session/") => {
                if req
                    .header_value("content-range")
                    .is_some_and(|r| r.starts_with("bytes */"))
                {
                    Some(Call::StatusQuery)
                } else {
                    Some(Call::Chunk)
                }
            }
            ("POST", p) if p.ends_with("/permissions") => Some(Call::Permission),
            ("GET", p) if p.starts_with("/drive/v3/files/") => Some(Call::Meta),
            ("DELETE", p) if p.starts_with("/drive/v3/files/") => Some(Call::Delete),
            _ => None,
        };
        *s.calls.entry(call).or_default() += 1;
        let Some(call) = call else {
            return Ok(error_body(404, "notFound", "no such route"));
        };
        let nth = s.calls[&Some(call)];
        {
            let mut hooks = self.hooks.lock().unwrap_or_else(|e| e.into_inner());
            for (c, n, action) in hooks.iter_mut() {
                if *c == call && *n == nth {
                    if let Some(f) = action.take() {
                        f();
                    }
                }
            }
        }

        if req.header_value("authorization") != Some(&format!("Bearer {}", s.valid_token)) {
            return Ok(error_body(401, "authError", "Invalid Credentials"));
        }

        // Injected faults.
        let start = req
            .header_value("content-range")
            .and_then(parse_range)
            .and_then(|(r, _)| r.map(|(a, _)| a))
            .unwrap_or(0);
        let mut hit = None;
        for (i, r) in s.rules.iter_mut().enumerate() {
            if r.times > 0 && r.call == call && (call != Call::Chunk || start >= r.from_offset) {
                if r.skip > 0 {
                    r.skip -= 1;
                    continue;
                }
                hit = Some(i);
                break;
            }
        }
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
            Call::FolderSearch => {
                let (name, parent) =
                    parse_folder_query(q.get("q").map(String::as_str).unwrap_or(""))
                        .unwrap_or_default();
                // `mimeType!=` asks for the files that are not folders.
                let want_folders = !q.get("q").is_some_and(|q| q.contains("mimeType!="));
                let hits: Vec<Value> = s
                    .files
                    .iter()
                    .filter(|f| {
                        f.is_folder == want_folders
                            && f.name == name
                            && f.parent == parent
                            && !s.trashed.contains(&f.id)
                    })
                    .map(|f| json!({ "id": f.id }))
                    .collect();
                json_reply(json!({ "files": hits }))
            }
            Call::About => json_reply(json!({ "user": { "emailAddress": s.email } })),
            Call::FolderCreate => {
                let body: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                    .unwrap_or(Value::Null);
                let name = body["name"].as_str().unwrap_or("").to_owned();
                let parent = body["parents"][0].as_str().unwrap_or("root").to_owned();
                if let Some(i) = s.race_names.iter().position(|n| *n == name) {
                    s.race_names.remove(i);
                    s.next += 1;
                    let rival = format!("rival-{}", s.next);
                    s.files.push(FakeFile {
                        id: rival,
                        name: name.clone(),
                        parent: parent.clone(),
                        is_folder: true,
                        data: Vec::new(),
                    });
                }
                s.next += 1;
                let id = format!("folder-{}", s.next);
                s.files.push(FakeFile {
                    id: id.clone(),
                    name,
                    parent,
                    is_folder: true,
                    data: Vec::new(),
                });
                json_reply(json!({ "id": id }))
            }
            Call::Init => {
                let body: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                    .unwrap_or(Value::Null);
                let total: u64 = req
                    .header_value("x-upload-content-length")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if s.quota.is_some_and(|quota| s.stored_total + total > quota) {
                    error_body(
                        403,
                        "storageQuotaExceeded",
                        "The user's Drive storage quota has been exceeded.",
                    )
                } else {
                    s.next += 1;
                    let uri = format!("https://fake.drive/session/{}", s.next);
                    let replace = (req.method == "PATCH").then(|| {
                        path.trim_start_matches("/upload/drive/v3/files/")
                            .to_owned()
                    });
                    s.sessions.insert(
                        uri.clone(),
                        Session {
                            replace,
                            name: body["name"].as_str().unwrap_or("").to_owned(),
                            parent: body["parents"][0].as_str().unwrap_or("root").to_owned(),
                            received: Vec::new(),
                            finished: None,
                        },
                    );
                    HttpReply::new(200).header("Location", uri)
                }
            }
            Call::Chunk | Call::StatusQuery => {
                let key = req.url.split('#').next().unwrap_or("").to_owned();
                let range = req.header_value("content-range").and_then(parse_range);
                match (s.sessions.contains_key(&key), range) {
                    (false, _) => error_body(404, "notFound", "Upload session not found"),
                    (_, None) => error_body(400, "badRequest", "Bad Content-Range"),
                    (true, Some((span, total))) => {
                        let data = req.body.read().unwrap_or_default();
                        let mut finished = None;
                        let mut violation = None;
                        let mut chunk = None;
                        {
                            let sess = s.sessions.get_mut(&key).expect("checked above");
                            if let Some(id) = &sess.finished {
                                finished = Some(id.clone());
                            } else if let Some((a, b)) = span {
                                if a as usize == sess.received.len()
                                    && (b - a + 1) as usize == data.len()
                                {
                                    let last = b + 1 == total;
                                    if !last && !data.len().is_multiple_of(256 * 1024) {
                                        violation = Some(format!(
                                            "non-final chunk of {} bytes is not a multiple of 256 KiB",
                                            data.len()
                                        ));
                                    }
                                    sess.received.extend_from_slice(&data);
                                    chunk = Some((a, data.len() as u64, total));
                                }
                            } else if total == 0 && sess.received.is_empty() {
                                chunk = Some((0, 0, 0));
                            }
                        }
                        if let Some(v) = violation {
                            s.violations.push(v);
                        }
                        if let Some(c) = chunk {
                            s.chunks.push(c);
                        }
                        let (name, parent, got, replace) = {
                            let sess = &s.sessions[&key];
                            (
                                sess.name.clone(),
                                sess.parent.clone(),
                                sess.received.len() as u64,
                                sess.replace.clone(),
                            )
                        };
                        if finished.is_none() && got == total && (span.is_some() || total == 0) {
                            if let Some(id) = replace {
                                // A replacement: same id, same place, new content.
                                let data = s.sessions[&key].received.clone();
                                if let Some(f) = s.files.iter_mut().find(|f| f.id == id) {
                                    f.data = data;
                                }
                                s.sessions.get_mut(&key).expect("session").finished =
                                    Some(id.clone());
                                finished = Some(id);
                            }
                        }
                        if finished.is_none() && got == total && (span.is_some() || total == 0) {
                            s.next += 1;
                            let id = format!("file-{}", s.next);
                            let data = s.sessions[&key].received.clone();
                            s.stored_total += got;
                            s.files.push(FakeFile {
                                id: id.clone(),
                                name,
                                parent,
                                is_folder: false,
                                data,
                            });
                            s.sessions.get_mut(&key).expect("session").finished = Some(id.clone());
                            finished = Some(id);
                        }
                        if let Some(id) = finished {
                            let size = (got as i64 + s.size_skew).max(0);
                            json_reply(json!({
                                "id": id,
                                "size": size.to_string(),
                                "webViewLink": format!("https://drive.fake/file/{id}/view"),
                            }))
                        } else {
                            let mut r = HttpReply::new(308);
                            if got > 0 {
                                r = r.header("Range", format!("bytes=0-{}", got - 1));
                            }
                            r
                        }
                    }
                }
            }
            Call::Permission => {
                if s.deny_public {
                    error_body(
                        403,
                        "domainPolicy",
                        "The domain administrator does not allow sharing outside the organisation.",
                    )
                } else {
                    let id = path
                        .trim_start_matches("/drive/v3/files/")
                        .trim_end_matches("/permissions")
                        .to_owned();
                    let body: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                        .unwrap_or(Value::Null);
                    s.permissions.push((
                        id,
                        body["role"].as_str().unwrap_or("").to_owned(),
                        body["type"].as_str().unwrap_or("").to_owned(),
                    ));
                    json_reply(json!({ "id": "perm" }))
                }
            }
            Call::Meta => {
                let id = path.trim_start_matches("/drive/v3/files/");
                match s.files.iter().find(|f| f.id == id) {
                    Some(f) => json_reply(json!({
                        "id": f.id,
                        "size": f.data.len().to_string(),
                        "trashed": s.trashed.contains(&f.id),
                        "parents": [f.parent],
                        "webViewLink": format!("https://drive.fake/file/{}/view", f.id),
                    })),
                    None => error_body(404, "notFound", "File not found"),
                }
            }
            Call::Delete => {
                let id = path.trim_start_matches("/drive/v3/files/").to_owned();
                let before = s.files.len();
                // Deleting a folder deletes what is inside it.
                let mut gone = vec![id.clone()];
                let mut i = 0;
                while i < gone.len() {
                    let cur = gone[i].clone();
                    gone.extend(
                        s.files
                            .iter()
                            .filter(|f| f.parent == cur)
                            .map(|f| f.id.clone()),
                    );
                    i += 1;
                }
                s.files.retain(|f| !gone.contains(&f.id));
                if s.files.len() == before {
                    error_body(404, "notFound", "File not found")
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

/// A token source with a scripted answer, for tests.
pub struct FakeTokens {
    state: Mutex<TokenState>,
}

struct TokenState {
    current: Result<String, String>,
    /// What `access_token(Some(rejected))` returns next, in order; then `current`.
    on_rejected: VecDeque<Result<String, String>>,
    calls: usize,
}

impl FakeTokens {
    pub fn new(token: &str) -> Self {
        FakeTokens {
            state: Mutex::new(TokenState {
                current: Ok(token.to_owned()),
                on_rejected: VecDeque::new(),
                calls: 0,
            }),
        }
    }

    /// The account is revoked: every call is `auth_expired`.
    pub fn revoked() -> Self {
        let t = FakeTokens::new("");
        t.state.lock().unwrap_or_else(|e| e.into_inner()).current = Err("revoked".to_owned());
        t
    }

    /// After a 401, the source hands out `token`.
    pub fn refresh_to(&self, token: &str) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.on_rejected.push_back(Ok(token.to_owned()));
    }

    /// After a 401, the refresh fails: the grant was revoked.
    pub fn revoke_on_reject(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.on_rejected
            .push_back(Err("The grant was revoked".to_owned()));
    }

    pub fn calls(&self) -> usize {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).calls
    }
}

#[async_trait]
impl TokenSource for FakeTokens {
    async fn access_token(&self, rejected: Option<&str>) -> Result<String, CmError> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.calls += 1;
        let r = match rejected {
            Some(_) => s
                .on_rejected
                .pop_front()
                .unwrap_or_else(|| s.current.clone()),
            None => s.current.clone(),
        };
        r.map_err(|m| CmError::AuthExpired { message: m })
    }
}
