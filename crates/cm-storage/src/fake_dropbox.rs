//! An in-process Dropbox for tests: `upload_session/start`, `append_v2` and `finish` with offset
//! checks (`incorrect_offset` with `correct_offset`), `get_metadata`, `delete_v2`,
//! `create_shared_link_with_settings` (with "already exists") and `list_shared_links`, with
//! failures a test can inject. It records protocol breaches (a request over 150 MB, a
//! non-ASCII `Dropbox-API-Arg`). No network, no account.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::fake_drive::Fault;
use crate::http::{HttpReply, HttpRequest, StorageHttp, TransportError};

const MAX_REQUEST: u64 = 150_000_000;

/// Which call a fault applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Call {
    Start,
    Append,
    Finish,
    Meta,
    Delete,
    CreateLink,
    ListLinks,
    Account,
}

#[derive(Debug, Clone)]
struct Rule {
    call: Call,
    /// For `append_v2` and `finish`: only when the cursor offset is at or after this byte.
    from_offset: u64,
    fault: Fault,
    times: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbxFile {
    pub id: String,
    /// The path as committed (after autorename).
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Default)]
struct Session {
    received: Vec<u8>,
    closed: bool,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    files: Vec<DbxFile>,
    sessions: HashMap<String, Session>,
    rules: Vec<Rule>,
    valid_token: String,
    email: String,
    quota: Option<u64>,
    stored_total: u64,
    deny_public: bool,
    deny_all_links: bool,
    link_exists_for_new_files: bool,
    size_skew: i64,
    /// `(file id, visibility)` of every link made.
    links: Vec<(String, String)>,
    /// `(cursor offset, len)` of every request that carried bytes and was processed.
    chunks: Vec<(u64, u64)>,
    violations: Vec<String>,
    calls: HashMap<Call, usize>,
    log: Vec<String>,
}

pub struct FakeDropbox {
    state: Mutex<State>,
}

impl Default for FakeDropbox {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDropbox {
    pub fn new() -> Self {
        FakeDropbox {
            state: Mutex::new(State {
                valid_token: "tok-1".to_owned(),
                email: "teacher@example.org".to_owned(),
                ..State::default()
            }),
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

    /// The address `users/get_current_account` reports.
    pub fn set_account_email(&self, email: &str) {
        self.st().email = email.to_owned();
    }

    /// Fails the next `times` matching calls (`append_v2` and `finish`: only those whose cursor
    /// offset is `from_offset` or later).
    pub fn inject(&self, call: Call, from_offset: u64, fault: Fault, times: usize) {
        self.st().rules.push(Rule {
            call,
            from_offset,
            fault,
            times,
        });
    }

    /// Total bytes the account may store; `finish` of a bigger file answers `insufficient_space`.
    pub fn set_quota(&self, bytes: u64) {
        self.st().quota = Some(bytes);
    }

    /// Team policy: a `public` link answers `settings_error`.
    pub fn deny_public_links(&self) {
        self.st().deny_public = true;
    }

    /// Team policy: every link request answers 403.
    pub fn deny_all_links(&self) {
        self.st().deny_all_links = true;
    }

    /// Every file gets a shared link the moment it is committed (so asking for one finds it).
    pub fn link_exists_for_new_files(&self) {
        self.st().link_exists_for_new_files = true;
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

    pub fn files(&self) -> Vec<DbxFile> {
        self.st().files.clone()
    }

    /// `(cursor offset, len)` of every request that carried bytes.
    pub fn chunks(&self) -> Vec<(u64, u64)> {
        self.st().chunks.clone()
    }

    pub fn violations(&self) -> Vec<String> {
        self.st().violations.clone()
    }

    pub fn count(&self, call: Call) -> usize {
        self.st().calls.get(&call).copied().unwrap_or(0)
    }

    /// `(file id, visibility)` of every link made.
    pub fn links(&self) -> Vec<(String, String)> {
        self.st().links.clone()
    }

    pub fn sessions_started(&self) -> usize {
        self.count(Call::Start)
    }

    pub fn log(&self) -> Vec<String> {
        self.st().log.clone()
    }
}

fn error_reply(status: u16, summary: &str, error: Value) -> HttpReply {
    HttpReply::new(status)
        .header("Content-Type", "application/json")
        .body(json!({ "error_summary": format!("{summary}/.."), "error": error }).to_string())
}

fn ok(v: Value) -> HttpReply {
    HttpReply::new(200)
        .header("Content-Type", "application/json")
        .body(v.to_string())
}

fn file_json(f: &DbxFile, size: i64) -> Value {
    json!({
        ".tag": "file",
        "id": f.id,
        "name": f.path.rsplit('/').next().unwrap_or(""),
        "path_display": f.path,
        "size": size,
    })
}

impl State {
    fn link_for(&mut self, id: &str, visibility: &str) {
        self.links.push((id.to_owned(), visibility.to_owned()));
    }

    fn link_url(id: &str) -> String {
        format!("https://dbx.fake/s/{id}")
    }

    fn find_by_path_or_id(&self, p: &str) -> Option<&DbxFile> {
        self.files
            .iter()
            .find(|f| f.id == p || f.path.eq_ignore_ascii_case(p))
    }
}

#[async_trait]
impl StorageHttp for FakeDropbox {
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError> {
        let mut s = self.st();
        let route = req
            .url
            .strip_prefix("https://content.dropboxapi.com/2/")
            .or_else(|| req.url.strip_prefix("https://api.dropboxapi.com/2/"))
            .unwrap_or("")
            .to_owned();
        s.log.push(format!("{} {}", req.method, route));
        let call = match (req.method.as_str(), route.as_str()) {
            ("POST", "files/upload_session/start") => Some(Call::Start),
            ("POST", "files/upload_session/append_v2") => Some(Call::Append),
            ("POST", "files/upload_session/finish") => Some(Call::Finish),
            ("POST", "files/get_metadata") => Some(Call::Meta),
            ("POST", "files/delete_v2") => Some(Call::Delete),
            ("POST", "sharing/create_shared_link_with_settings") => Some(Call::CreateLink),
            ("POST", "sharing/list_shared_links") => Some(Call::ListLinks),
            ("POST", "users/get_current_account") => Some(Call::Account),
            _ => None,
        };
        let Some(call) = call else {
            return Ok(HttpReply::new(404).body("no such route"));
        };
        *s.calls.entry(call).or_default() += 1;

        if req.header_value("authorization") != Some(&format!("Bearer {}", s.valid_token)) {
            return Ok(error_reply(
                401,
                "expired_access_token",
                json!({ ".tag": "expired_access_token" }),
            ));
        }

        // The argument: a header for content calls, the JSON body for RPC calls.
        let content_call = matches!(call, Call::Start | Call::Append | Call::Finish);
        let arg: Value = if content_call {
            let h = req.header_value("dropbox-api-arg").unwrap_or("{}");
            if !h.is_ascii() {
                s.violations.push("Dropbox-API-Arg is not ASCII".to_owned());
            }
            serde_json::from_str(h).unwrap_or(Value::Null)
        } else {
            serde_json::from_slice(&req.body.read().unwrap_or_default()).unwrap_or(Value::Null)
        };
        let cursor_offset = arg["cursor"]["offset"].as_u64().unwrap_or(0);

        // Injected faults.
        let hit = s.rules.iter().position(|r| {
            r.times > 0
                && r.call == call
                && (!matches!(call, Call::Append | Call::Finish) || cursor_offset >= r.from_offset)
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
                    message: _,
                } => {
                    let mut error = json!({ ".tag": reason });
                    if let Some(n) = retry_after {
                        error = json!({ "reason": { ".tag": reason }, "retry_after": n });
                    }
                    let mut r = error_reply(status, reason, error);
                    if let Some(n) = retry_after {
                        r = r.header("Retry-After", n.to_string());
                    }
                    return Ok(r);
                }
            }
        }

        let reply = match call {
            Call::Start => {
                s.next += 1;
                let id = format!("sess-{}", s.next);
                s.sessions.insert(id.clone(), Session::default());
                ok(json!({ "session_id": id }))
            }
            Call::Append | Call::Finish => {
                let data = req.body.read().unwrap_or_default();
                if data.len() as u64 > MAX_REQUEST {
                    s.violations
                        .push(format!("request of {} bytes is over 150 MB", data.len()));
                }
                let sid = arg["cursor"]["session_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                let finishing = call == Call::Finish;
                let known = s
                    .sessions
                    .get(&sid)
                    .map(|x| (x.closed, x.received.len() as u64));
                match known {
                    None => error_reply(
                        409,
                        "lookup_failed/not_found",
                        json!({ ".tag": "lookup_failed", "lookup_failed": { ".tag": "not_found" } }),
                    ),
                    Some((true, _)) => error_reply(
                        409,
                        "lookup_failed/closed",
                        json!({ ".tag": "lookup_failed", "lookup_failed": { ".tag": "closed" } }),
                    ),
                    Some((_, correct)) if correct != cursor_offset => {
                        if finishing {
                            error_reply(
                                409,
                                "lookup_failed/incorrect_offset",
                                json!({ ".tag": "lookup_failed", "lookup_failed": {
                                    ".tag": "incorrect_offset", "correct_offset": correct } }),
                            )
                        } else {
                            error_reply(
                                409,
                                "incorrect_offset",
                                json!({ ".tag": "incorrect_offset", "correct_offset": correct }),
                            )
                        }
                    }
                    Some(_) => {
                        s.chunks.push((cursor_offset, data.len() as u64));
                        let total = {
                            let sess = s.sessions.get_mut(&sid).expect("checked above");
                            sess.received.extend_from_slice(&data);
                            sess.received.len() as u64
                        };
                        if !finishing {
                            ok(Value::Null)
                        } else if s.quota.is_some_and(|q| s.stored_total + total > q) {
                            error_reply(
                                409,
                                "path/insufficient_space",
                                json!({ ".tag": "path", "path": { ".tag": "insufficient_space" } }),
                            )
                        } else {
                            let sess = s.sessions.get_mut(&sid).expect("session");
                            sess.closed = true;
                            let bytes = std::mem::take(&mut sess.received);
                            let mut path = arg["commit"]["path"].as_str().unwrap_or("/").to_owned();
                            // autorename
                            let mut n = 0;
                            let orig = path.clone();
                            while s.files.iter().any(|f| f.path.eq_ignore_ascii_case(&path)) {
                                n += 1;
                                path = match orig.rsplit_once('.') {
                                    Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                                    None => format!("{orig} ({n})"),
                                };
                            }
                            s.next += 1;
                            s.stored_total += total;
                            let f = DbxFile {
                                id: format!("id:{}", s.next),
                                path,
                                data: bytes,
                            };
                            s.files.push(f.clone());
                            if s.link_exists_for_new_files {
                                s.link_for(&f.id, "public");
                            }
                            let skew = s.size_skew;
                            ok(file_json(&f, f.data.len() as i64 + skew))
                        }
                    }
                }
            }
            Call::Account => {
                if req.header_value("content-type").is_some() {
                    s.violations
                        .push("users/get_current_account sent with a Content-Type".to_owned());
                }
                ok(json!({ "email": s.email, "account_id": "dbid:fake" }))
            }
            Call::Meta => {
                let p = arg["path"].as_str().unwrap_or("");
                match s.find_by_path_or_id(p) {
                    Some(f) => ok(file_json(f, f.data.len() as i64)),
                    None => error_reply(
                        409,
                        "path/not_found",
                        json!({ ".tag": "path", "path": { ".tag": "not_found" } }),
                    ),
                }
            }
            Call::Delete => {
                let p = arg["path"].as_str().unwrap_or("").to_owned();
                let before = s.files.len();
                s.files
                    .retain(|f| !(f.id == p || f.path.eq_ignore_ascii_case(&p)));
                if s.files.len() == before {
                    error_reply(
                        409,
                        "path_lookup/not_found",
                        json!({ ".tag": "path_lookup", "path_lookup": { ".tag": "not_found" } }),
                    )
                } else {
                    ok(json!({ "metadata": {} }))
                }
            }
            Call::CreateLink => {
                let p = arg["path"].as_str().unwrap_or("").to_owned();
                let visibility = arg["settings"]["requested_visibility"]
                    .as_str()
                    .unwrap_or("public")
                    .to_owned();
                let id = s.find_by_path_or_id(&p).map(|f| f.id.clone());
                match id {
                    None => error_reply(
                        409,
                        "path/not_found",
                        json!({ ".tag": "path", "path": { ".tag": "not_found" } }),
                    ),
                    Some(_) if s.deny_all_links => {
                        error_reply(403, "access_denied", json!({ ".tag": "access_denied" }))
                    }
                    Some(_) if s.deny_public && visibility == "public" => error_reply(
                        409,
                        "settings_error/not_authorized",
                        json!({ ".tag": "settings_error", "settings_error": { ".tag": "not_authorized" } }),
                    ),
                    Some(id) if s.links.iter().any(|l| l.0 == id) => error_reply(
                        409,
                        "shared_link_already_exists",
                        json!({ ".tag": "shared_link_already_exists" }),
                    ),
                    Some(id) => {
                        s.link_for(&id, &visibility);
                        ok(json!({ ".tag": "file", "url": State::link_url(&id), "id": id }))
                    }
                }
            }
            Call::ListLinks => {
                let p = arg["path"].as_str().unwrap_or("").to_owned();
                let id = s.find_by_path_or_id(&p).map(|f| f.id.clone());
                let links: Vec<Value> = id
                    .into_iter()
                    .filter(|id| s.links.iter().any(|l| &l.0 == id))
                    .map(|id| json!({ "url": State::link_url(&id) }))
                    .collect();
                ok(json!({ "links": links, "has_more": false }))
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
