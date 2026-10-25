//! An in-process YouTube Data API for tests: `videos.insert` (resumable init, chunk, status
//! query), `thumbnails.set`, `captions.insert` and `playlistItems.insert`, with failures a test
//! can inject. No network, no account. Any other route (above all a `videos.update`, which would
//! change a video's privacy) is recorded in [`FakeYouTube::unexpected`] and answered with 404.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use cm_storage::http::{HttpReply, HttpRequest, StorageHttp, TransportError};
use percent_encoding::percent_decode_str;
use serde_json::{json, Value};

pub use cm_storage::fake_drive::FakeTokens;

/// Which call a fault applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Call {
    Init,
    Chunk,
    StatusQuery,
    Thumbnail,
    Playlist,
    Captions,
    Delete,
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
    /// For chunks: only when the chunk starts at or after this byte.
    from_offset: u64,
    fault: Fault,
    times: usize,
}

/// A video as the server stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct FakeVideo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category: String,
    pub privacy: String,
    /// What the client asked for before any lock.
    pub requested_privacy: String,
    pub made_for_kids: Option<bool>,
    pub publish_at: Option<String>,
    pub data: Vec<u8>,
    pub thumbnail: Option<Vec<u8>>,
    /// `(language, srt bytes)`
    pub captions: Vec<(String, Vec<u8>)>,
    pub playlists: Vec<String>,
}

#[derive(Debug)]
struct Session {
    meta: Value,
    received: Vec<u8>,
    finished: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    videos: Vec<FakeVideo>,
    sessions: HashMap<String, Session>,
    rules: Vec<Rule>,
    valid_token: String,
    lock_private: bool,
    thumbnails_need_verified: bool,
    chunks: Vec<(u64, u64, u64)>,
    violations: Vec<String>,
    unexpected: Vec<String>,
    calls: HashMap<Call, usize>,
    log: Vec<String>,
    init_params: Vec<String>,
}

pub struct FakeYouTube {
    state: Mutex<State>,
}

impl Default for FakeYouTube {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeYouTube {
    pub fn new() -> Self {
        FakeYouTube {
            state: Mutex::new(State {
                valid_token: "tok-1".to_owned(),
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

    /// Fails the next `times` matching calls (chunks: only those starting at `from_offset` or
    /// later).
    pub fn inject(&self, call: Call, from_offset: u64, fault: Fault, times: usize) {
        self.st().rules.push(Rule {
            call,
            from_offset,
            fault,
            times,
        });
    }

    /// An unverified API project: whatever privacy is asked for, the video is created private.
    pub fn lock_to_private(&self) {
        self.st().lock_private = true;
    }

    /// The channel has no verified phone: `thumbnails.set` answers 403 `forbidden`.
    pub fn thumbnails_need_verified_account(&self) {
        self.st().thumbnails_need_verified = true;
    }

    /// Forgets every upload session (they expire).
    pub fn expire_sessions(&self) {
        self.st().sessions.clear();
    }

    // ---- inspection --------------------------------------------------------------------------

    pub fn videos(&self) -> Vec<FakeVideo> {
        self.st().videos.clone()
    }

    pub fn chunks(&self) -> Vec<(u64, u64, u64)> {
        self.st().chunks.clone()
    }

    /// Protocol breaches seen (a non-final chunk that is not a multiple of 256 KiB, ...).
    pub fn violations(&self) -> Vec<String> {
        self.st().violations.clone()
    }

    /// Routes the fake does not implement that a client called (for example `PUT /videos`).
    pub fn unexpected(&self) -> Vec<String> {
        self.st().unexpected.clone()
    }

    pub fn count(&self, call: Call) -> usize {
        self.st().calls.get(&call).copied().unwrap_or(0)
    }

    pub fn sessions_started(&self) -> usize {
        self.count(Call::Init)
    }

    /// The query strings of the init calls (`part=...&uploadType=...`).
    pub fn init_params(&self) -> Vec<String> {
        self.st().init_params.clone()
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

fn split_url(url: &str) -> (String, HashMap<String, String>, String) {
    let url = url.split('#').next().unwrap_or(url);
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let path = path
        .strip_prefix("https://www.googleapis.com")
        .or_else(|| path.strip_prefix("https://fake.youtube"))
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
    (path.to_owned(), q, query.to_owned())
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

/// The server's own check of the snippet (what `invalidTitle`, `invalidTags` mean).
fn snippet_problem(meta: &Value) -> Option<(&'static str, &'static str)> {
    let sn = &meta["snippet"];
    let title = sn["title"].as_str().unwrap_or("");
    if title.trim().is_empty() || title.chars().count() > 100 {
        return Some(("invalidTitle", "The video title is empty or too long."));
    }
    if sn["description"].as_str().unwrap_or("").len() > 5000 {
        return Some(("invalidDescription", "The video description is too long."));
    }
    let tags: usize = sn["tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| t.as_str().unwrap_or("").chars().count())
                .sum()
        })
        .unwrap_or(0);
    if tags > 500 {
        return Some(("invalidTags", "The video keywords are too long."));
    }
    None
}

/// Splits a `multipart/related` body into its parts' bodies.
fn multipart_parts(content_type: &str, body: &[u8]) -> Vec<Vec<u8>> {
    let Some(boundary) = content_type.split("boundary=").nth(1) else {
        return Vec::new();
    };
    let delim = format!("--{boundary}");
    let text = body;
    let mut parts = Vec::new();
    let mut i = 0usize;
    let find = |hay: &[u8], needle: &[u8], from: usize| -> Option<usize> {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
    };
    while let Some(start) = find(text, delim.as_bytes(), i) {
        let after = start + delim.len();
        if text[after..].starts_with(b"--") {
            break;
        }
        let Some(hdr_end) = find(text, b"\r\n\r\n", after) else {
            break;
        };
        let body_start = hdr_end + 4;
        let Some(next) = find(text, delim.as_bytes(), body_start) else {
            break;
        };
        // The part ends before the CRLF that precedes the next delimiter.
        parts.push(text[body_start..next.saturating_sub(2)].to_vec());
        i = next;
    }
    parts
}

#[async_trait]
impl StorageHttp for FakeYouTube {
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError> {
        let mut s = self.st();
        let (path, q, raw_query) = split_url(&req.url);
        s.log.push(format!("{} {}", req.method, path));
        let call = match (req.method.as_str(), path.as_str()) {
            ("POST", "/upload/youtube/v3/videos") => Some(Call::Init),
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
            ("POST", "/upload/youtube/v3/thumbnails/set") => Some(Call::Thumbnail),
            ("POST", "/upload/youtube/v3/captions") => Some(Call::Captions),
            ("POST", "/youtube/v3/playlistItems") => Some(Call::Playlist),
            ("DELETE", "/youtube/v3/videos") => Some(Call::Delete),
            _ => None,
        };
        let Some(call) = call else {
            s.unexpected.push(format!("{} {}", req.method, path));
            return Ok(error_body(404, "notFound", "no such route"));
        };
        *s.calls.entry(call).or_default() += 1;

        if req.header_value("authorization") != Some(&format!("Bearer {}", s.valid_token)) {
            return Ok(error_body(401, "authError", "Invalid Credentials"));
        }

        // Injected faults.
        let start = req
            .header_value("content-range")
            .and_then(parse_range)
            .and_then(|(r, _)| r.map(|(a, _)| a))
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
            Call::Init => {
                let meta: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                    .unwrap_or(Value::Null);
                s.init_params.push(raw_query);
                if let Some((reason, message)) = snippet_problem(&meta) {
                    error_body(400, reason, message)
                } else if q.get("part").map(String::as_str) != Some("snippet,status")
                    || q.get("uploadType").map(String::as_str) != Some("resumable")
                {
                    error_body(400, "badRequest", "part or uploadType missing")
                } else {
                    s.next += 1;
                    let uri = format!("https://fake.youtube/session/{}", s.next);
                    s.sessions.insert(
                        uri.clone(),
                        Session {
                            meta,
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
                        let mut violation = None;
                        let mut chunk = None;
                        {
                            let sess = s.sessions.get_mut(&key).expect("checked above");
                            if sess.finished.is_none() {
                                if let Some((a, b)) = span {
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
                                    } else {
                                        violation = Some(format!(
                                            "chunk {a}-{b} does not continue at {}",
                                            sess.received.len()
                                        ));
                                    }
                                }
                            }
                        }
                        if let Some(v) = violation {
                            s.violations.push(v);
                        }
                        if let Some(c) = chunk {
                            s.chunks.push(c);
                        }
                        let got = s.sessions[&key].received.len() as u64;
                        let mut finished = s.sessions[&key].finished.clone();
                        if finished.is_none() && got == total && span.is_some() {
                            s.next += 1;
                            let id = format!("vid{}", s.next);
                            let meta = s.sessions[&key].meta.clone();
                            let data = s.sessions[&key].received.clone();
                            let requested = meta["status"]["privacyStatus"]
                                .as_str()
                                .unwrap_or("private")
                                .to_owned();
                            let privacy = if s.lock_private {
                                "private".to_owned()
                            } else {
                                requested.clone()
                            };
                            s.videos.push(FakeVideo {
                                id: id.clone(),
                                title: meta["snippet"]["title"].as_str().unwrap_or("").to_owned(),
                                description: meta["snippet"]["description"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_owned(),
                                tags: meta["snippet"]["tags"]
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|t| t.as_str().map(str::to_owned))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                category: meta["snippet"]["categoryId"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_owned(),
                                privacy,
                                requested_privacy: requested,
                                made_for_kids: meta["status"]["selfDeclaredMadeForKids"].as_bool(),
                                publish_at: meta["status"]["publishAt"].as_str().map(str::to_owned),
                                data,
                                thumbnail: None,
                                captions: Vec::new(),
                                playlists: Vec::new(),
                            });
                            s.sessions.get_mut(&key).expect("session").finished = Some(id.clone());
                            finished = Some(id);
                        }
                        if let Some(id) = finished {
                            let v = s.videos.iter().find(|v| v.id == id).cloned();
                            let privacy = v.as_ref().map(|v| v.privacy.clone()).unwrap_or_default();
                            json_reply(json!({
                                "kind": "youtube#video",
                                "id": id,
                                "snippet": { "title": v.map(|v| v.title).unwrap_or_default() },
                                "status": { "uploadStatus": "uploaded", "privacyStatus": privacy },
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
            Call::Thumbnail => {
                let id = q.get("videoId").cloned().unwrap_or_default();
                let data = req.body.read().unwrap_or_default();
                if s.thumbnails_need_verified {
                    error_body(
                        403,
                        "forbidden",
                        "The authenticated user doesn't have permissions to upload and set custom video thumbnails.",
                    )
                } else if let Some(v) = s.videos.iter_mut().find(|v| v.id == id) {
                    v.thumbnail = Some(data);
                    json_reply(json!({ "items": [] }))
                } else {
                    error_body(404, "videoNotFound", "The video could not be found.")
                }
            }
            Call::Captions => {
                let ct = req.header_value("content-type").unwrap_or("").to_owned();
                let parts = multipart_parts(&ct, &req.body.read().unwrap_or_default());
                let meta: Value = parts
                    .first()
                    .and_then(|p| serde_json::from_slice(p).ok())
                    .unwrap_or(Value::Null);
                let id = meta["snippet"]["videoId"].as_str().unwrap_or("").to_owned();
                let lang = meta["snippet"]["language"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                match (s.videos.iter_mut().find(|v| v.id == id), parts.get(1)) {
                    (Some(v), Some(srt)) => {
                        v.captions.push((lang, srt.clone()));
                        json_reply(json!({ "id": "cap1" }))
                    }
                    _ => error_body(400, "badRequest", "Bad captions request"),
                }
            }
            Call::Delete => {
                let id = q.get("id").cloned().unwrap_or_default();
                let before = s.videos.len();
                s.videos.retain(|v| v.id != id);
                if s.videos.len() == before {
                    error_body(404, "videoNotFound", "The video could not be found.")
                } else {
                    HttpReply::new(204)
                }
            }
            Call::Playlist => {
                let body: Value = serde_json::from_slice(&req.body.read().unwrap_or_default())
                    .unwrap_or(Value::Null);
                let vid = body["snippet"]["resourceId"]["videoId"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                let pl = body["snippet"]["playlistId"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                match s.videos.iter_mut().find(|v| v.id == vid) {
                    Some(v) => {
                        v.playlists.push(pl);
                        json_reply(json!({ "id": "pli1" }))
                    }
                    None => error_body(404, "videoNotFound", "The video could not be found."),
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
