//! The OpenAI-compatible client of AI-7, used over loopback for the built-in engine's helper.
//! Blocking, on `ureq` like the rest of the workspace.
//!
//! Safety rules enforced here, not by the callers:
//! * every request starts with [`check_loopback`]: only plain `http://` to this computer;
//! * redirects are never followed (the helper cannot bounce text to another host).

use std::io::{BufRead as _, BufReader, Read as _};
use std::time::Duration;

use serde_json::{json, Value};

use super::schema::validate;
use super::{
    check_loopback, url_host, ErrorKind, JsonReply, JsonRequest, Message, TextModel,
    TextModelError, UNREACHABLE_HINT,
};

/// Longest reply body read (a text reply, not a download).
const MAX_BODY: u64 = 16 * 1024 * 1024;

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_connect(Some(Duration::from_secs(10).min(timeout)))
        .timeout_global(Some(timeout))
        .build()
        .into()
}

struct Reply {
    status: u16,
    body: String,
}

enum Fail {
    Timeout,
    Unreachable,
    /// The progress hook asked to stop.
    Cancelled,
    /// The server sent an error event inside a stream.
    Server(String),
}

/// The caller's progress hook of a streaming call, and the pieces counted so far (kept over the
/// validation retry, so the count only grows).
struct Progress<'a> {
    f: &'a mut dyn FnMut(usize) -> bool,
    pieces: usize,
}

impl Progress<'_> {
    /// One more piece arrived; `false` when the caller wants to stop.
    fn tick(&mut self) -> bool {
        self.pieces += 1;
        (self.f)(self.pieces)
    }
}

fn classify(e: &ureq::Error) -> Fail {
    match e {
        ureq::Error::Timeout(t) => {
            if matches!(t, ureq::Timeout::Resolve | ureq::Timeout::Connect) {
                Fail::Unreachable
            } else {
                Fail::Timeout
            }
        }
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => Fail::Timeout,
        _ => Fail::Unreachable,
    }
}

fn io_fail(e: &std::io::Error) -> Fail {
    if e.kind() == std::io::ErrorKind::TimedOut || e.to_string().contains("timeout") {
        Fail::Timeout
    } else {
        Fail::Unreachable
    }
}

fn read_body(mut resp: ureq::http::Response<ureq::Body>) -> Result<Reply, Fail> {
    let status = resp.status().as_u16();
    let mut body = String::new();
    resp.body_mut()
        .as_reader()
        .take(MAX_BODY)
        .read_to_string(&mut body)
        .map_err(|e| io_fail(&e))?;
    Ok(Reply { status, body })
}

/// The model's raw text for one request, and whether it stopped at the reply limit.
struct Chat {
    text: String,
    hit_limit: bool,
}

/// What a chat request came back with.
enum Outcome {
    /// A whole HTTP body (an error status, or a server that ignored `stream`).
    Reply(Reply),
    /// The assembled text of a streamed answer, and whether the model stopped at the reply limit
    /// (`finish_reason: "length"`) instead of finishing.
    Streamed(String, bool),
}

fn is_event_stream(resp: &ureq::http::Response<ureq::Body>) -> bool {
    resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("text/event-stream"))
}

/// Reads an OpenAI server-sent-events answer to the end, ticking `progress` for every piece of
/// text. The global timeout of the agent still bounds a stalled stream (the read fails with a
/// timeout), and a `false` from the hook stops between two events.
fn read_stream(
    resp: &mut ureq::http::Response<ureq::Body>,
    progress: &mut Progress<'_>,
) -> Result<(String, bool), Fail> {
    let mut reader = BufReader::new(resp.body_mut().as_reader().take(MAX_BODY));
    let mut text = String::new();
    let mut hit_limit = false;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|e| io_fail(&e))? == 0 {
            break;
        }
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(err) = v.get("error") {
            let msg = err["message"]
                .as_str()
                .or_else(|| err.as_str())
                .unwrap_or("the stream ended with an error");
            return Err(Fail::Server(msg.to_owned()));
        }
        if v["choices"][0]["finish_reason"].as_str() == Some("length") {
            hit_limit = true;
        }
        let piece = v["choices"][0]["delta"]["content"]
            .as_str()
            .or_else(|| v["choices"][0]["message"]["content"].as_str())
            .unwrap_or_default();
        if !piece.is_empty() {
            text.push_str(piece);
            if !progress.tick() {
                return Err(Fail::Cancelled);
            }
        }
    }
    Ok((text, hit_limit))
}

fn outcome(
    mut resp: ureq::http::Response<ureq::Body>,
    progress: Option<&mut Progress<'_>>,
) -> Result<Outcome, Fail> {
    let ok = (200..300).contains(&resp.status().as_u16());
    match progress {
        Some(p) if ok && is_event_stream(&resp) => {
            read_stream(&mut resp, p).map(|(t, limit)| Outcome::Streamed(t, limit))
        }
        _ => read_body(resp).map(Outcome::Reply),
    }
}

fn get(url: &str, headers: &[(String, String)], timeout: Duration) -> Result<Reply, Fail> {
    let mut req = agent(timeout).get(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    read_body(req.call().map_err(|e| classify(&e))?)
}

fn post_outcome(
    url: &str,
    headers: &[(String, String)],
    body: &Value,
    timeout: Duration,
    progress: Option<&mut Progress<'_>>,
) -> Result<Outcome, Fail> {
    let bytes = serde_json::to_vec(body).map_err(|_| Fail::Unreachable)?;
    let mut req = agent(timeout)
        .post(url)
        .header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    outcome(req.send(&bytes[..]).map_err(|e| classify(&e))?, progress)
}

/// `GET http://127.0.0.1:<port>/health` of the built-in helper: 200 is ready, 503 is still
/// loading the model, anything else (or no answer) is down. Loopback only by construction.
pub(super) fn probe_health(port: u16) -> super::engine::Health {
    use super::engine::Health;
    match get(
        &format!("http://127.0.0.1:{port}/health"),
        &[],
        Duration::from_secs(2),
    ) {
        Ok(r) if r.status == 200 => Health::Ready,
        Ok(r) if r.status == 503 => Health::Loading,
        _ => Health::Down,
    }
}

/// The OpenAI-compatible client of the built-in engine: `POST {base_url}/chat/completions` on
/// this computer.
pub struct HttpTextModel {
    base_url: String,
    model: String,
    timeout_s: u64,
}

impl HttpTextModel {
    /// `base_url` is the helper's `http://127.0.0.1:<port>/v1`; any other host is refused when a
    /// request is made ([`check_loopback`]).
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, timeout_s: u64) -> Self {
        HttpTextModel {
            base_url: base_url.into().trim().trim_end_matches('/').to_owned(),
            model: model.into(),
            timeout_s,
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_s.max(1))
    }

    fn host(&self) -> String {
        url_host(&self.base_url).unwrap_or_default()
    }

    fn unreachable(&self) -> TextModelError {
        TextModelError {
            kind: ErrorKind::Unreachable,
            message: format!(
                "The text model server at {} did not answer. {UNREACHABLE_HINT}.",
                self.host()
            ),
        }
    }

    fn fail(&self, f: Fail) -> TextModelError {
        match f {
            Fail::Timeout => TextModelError {
                kind: ErrorKind::Timeout,
                message: format!(
                    "The text model did not answer within {} seconds",
                    self.timeout_s
                ),
            },
            Fail::Unreachable => self.unreachable(),
            Fail::Cancelled => TextModelError {
                kind: ErrorKind::Cancelled,
                message: "The request was cancelled".to_owned(),
            },
            Fail::Server(d) => TextModelError {
                kind: ErrorKind::Server,
                message: format!(
                    "{} sent an error: {}",
                    self.host(),
                    d.chars().take(400).collect::<String>()
                ),
            },
        }
    }

    /// The server's own words for a failed request.
    fn server_error(&self, r: &Reply) -> TextModelError {
        let parsed: Option<Value> = serde_json::from_str(&r.body).ok();
        let detail = parsed
            .as_ref()
            .and_then(|v| {
                v["error"]["message"]
                    .as_str()
                    .or_else(|| v["error"].as_str())
                    .or_else(|| v["message"].as_str())
            })
            .map_or_else(|| r.body.trim().to_owned(), str::to_owned);
        let detail: String = detail.chars().take(400).collect();
        let host = self.host();
        let message = match r.status {
            404 => format!("The text model was not found on {host} (HTTP 404). {detail}"),
            _ => format!("{host} answered HTTP {}: {detail}", r.status),
        };
        TextModelError {
            kind: ErrorKind::Server,
            message: message.trim().to_owned(),
        }
    }

    /// One round trip: the model's raw text for `messages`.
    fn chat_once(
        &self,
        req: &JsonRequest,
        messages: &[Message],
        progress: Option<&mut Progress<'_>>,
    ) -> Result<Chat, TextModelError> {
        check_loopback(&self.base_url)?;
        if self.model.is_empty() {
            return Err(TextModelError::invalid_input(
                "Choose a text model on the AI page first",
            ));
        }
        self.openai_once(req, messages, progress)
    }

    fn system_text(req: &JsonRequest) -> String {
        let mut s = req.system.clone().unwrap_or_default();
        if !s.is_empty() {
            s.push_str("\n\n");
        }
        s.push_str(
            "Reply with a single JSON value that matches this JSON schema, and nothing else:\n",
        );
        s.push_str(&req.schema.to_string());
        s
    }

    fn openai_once(
        &self,
        req: &JsonRequest,
        messages: &[Message],
        mut progress: Option<&mut Progress<'_>>,
    ) -> Result<Chat, TextModelError> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut msgs = vec![json!({ "role": "system", "content": Self::system_text(req) })];
        msgs.extend(
            messages
                .iter()
                .map(|m| json!({ "role": m.role, "content": m.content })),
        );
        let mut body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": progress.is_some(),
            "temperature": 0,
            "max_tokens": req.max_tokens,
            "response_format": { "type": "json_schema", "json_schema": {
                "name": req.schema_name, "strict": true, "schema": req.schema } },
        });
        let headers: [(String, String); 0] = [];
        let mut out = post_outcome(
            &url,
            &headers,
            &body,
            self.timeout(),
            progress.as_deref_mut(),
        )
        .map_err(|f| self.fail(f))?;
        // A server that knows only `json_object` (or nothing) refuses `json_schema`: the schema
        // is already in the system prompt, so ask again without the field.
        if let Outcome::Reply(r) = &out {
            if matches!(r.status, 400 | 422)
                && (r.body.contains("response_format") || r.body.contains("json_schema"))
            {
                if let Some(o) = body.as_object_mut() {
                    o.remove("response_format");
                }
                out = post_outcome(&url, &headers, &body, self.timeout(), progress)
                    .map_err(|f| self.fail(f))?;
            }
        }
        let reply = match out {
            Outcome::Streamed(text, hit_limit) if !text.is_empty() => {
                return Ok(Chat { text, hit_limit })
            }
            Outcome::Streamed(..) => {
                return Err(TextModelError {
                    kind: ErrorKind::Server,
                    message: "The text model server's reply has no message".to_owned(),
                })
            }
            Outcome::Reply(r) => r,
        };
        if !(200..300).contains(&reply.status) {
            return Err(self.server_error(&reply));
        }
        let v: Value = serde_json::from_str(&reply.body).map_err(|_| TextModelError {
            kind: ErrorKind::Server,
            message: "The text model server sent a reply that is not JSON. Is it an OpenAI-compatible /v1 server?"
                .to_owned(),
        })?;
        let hit_limit = v["choices"][0]["finish_reason"].as_str() == Some("length");
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(|text| Chat {
                text: text.to_owned(),
                hit_limit,
            })
            .ok_or_else(|| TextModelError {
                kind: ErrorKind::Server,
                message: "The text model server's reply has no message".to_owned(),
            })
    }
}

/// The JSON in a model's text: the whole text, or the first fenced block, or the outermost
/// braces.
fn extract_json(text: &str) -> Result<Value, String> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Ok(v);
    }
    if let Some(start) = t.find("```") {
        let inner = &t[start + 3..];
        let inner = inner.strip_prefix("json").unwrap_or(inner);
        if let Some(end) = inner.find("```") {
            if let Ok(v) = serde_json::from_str::<Value>(inner[..end].trim()) {
                return Ok(v);
            }
        }
    }
    if let (Some(a), Some(b)) = (t.find(['{', '[']), t.rfind(['}', ']'])) {
        if a < b {
            if let Ok(v) = serde_json::from_str::<Value>(&t[a..=b]) {
                return Ok(v);
            }
        }
    }
    Err("the reply is not valid JSON".to_owned())
}

impl TextModel for HttpTextModel {
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        self.run(req, None)
    }

    fn complete_json_streaming(
        &self,
        req: &JsonRequest,
        on_progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<JsonReply, TextModelError> {
        self.run(req, Some(on_progress))
    }
}

impl HttpTextModel {
    /// The reply stopped at the limit: its JSON is cut off, and a retry that carries the cut-off
    /// reply would have even less room, so the caller gets a kind of its own.
    fn too_long(req: &JsonRequest) -> TextModelError {
        TextModelError {
            kind: ErrorKind::OutputTooLong,
            message: format!(
                "The text model's answer was longer than the {} tokens allowed for one reply and was cut off",
                req.max_tokens
            ),
        }
    }

    fn run(
        &self,
        req: &JsonRequest,
        on_progress: Option<&mut dyn FnMut(usize) -> bool>,
    ) -> Result<JsonReply, TextModelError> {
        let check = |text: &str| -> Result<Value, String> {
            let v = extract_json(text)?;
            validate(&req.schema, &v)?;
            Ok(v)
        };
        let mut progress = on_progress.map(|f| Progress { f, pieces: 0 });
        let first = self.chat_once(req, &req.messages, progress.as_mut())?;
        if first.hit_limit {
            return Err(Self::too_long(req));
        }
        let first = first.text;
        let err = match check(&first) {
            Ok(value) => return Ok(JsonReply { value, attempts: 1 }),
            Err(e) => e,
        };
        // One retry, with the validation error.
        let mut again = req.messages.clone();
        again.push(Message {
            role: "assistant",
            content: first,
        });
        again.push(Message::user(format!(
            "That reply does not match the schema: {err}. Reply again with only the corrected JSON."
        )));
        let second = self.chat_once(req, &again, progress.as_mut())?;
        if second.hit_limit {
            return Err(Self::too_long(req));
        }
        match check(&second.text) {
            Ok(value) => Ok(JsonReply { value, attempts: 2 }),
            Err(e) => Err(TextModelError {
                kind: ErrorKind::InvalidOutput,
                message: format!(
                    "The text model's answer did not match the expected format twice ({e})"
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_chatty_replies() {
        assert_eq!(extract_json("{\"a\":1}").unwrap(), json!({"a": 1}));
        assert_eq!(
            extract_json("Sure!\n```json\n{\"a\": 2}\n```\nDone").unwrap(),
            json!({"a": 2})
        );
        assert_eq!(
            extract_json("Here: {\"a\": 3} ok").unwrap(),
            json!({"a": 3})
        );
        assert!(extract_json("no json").is_err());
    }
}
