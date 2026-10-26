//! The HTTP providers of AI-7: OpenAI-compatible servers (Ollama, LM Studio, remote) and
//! Anthropic. Blocking, on `ureq` with the system TLS stack like the rest of the workspace.
//!
//! Safety rules enforced here, not by the callers:
//! * every request starts with [`TextModelConfig::check_endpoint`] (a `local` provider only talks
//!   to loopback; plain `http://` only to loopback);
//! * redirects are never followed (a local server cannot bounce text to another host);
//! * the API key is only ever put in a request header, and is masked in any message that is
//!   returned or logged.

use std::io::Read as _;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use super::schema::validate;
use super::{
    ErrorKind, JsonReply, JsonRequest, Message, Provider, TextModel, TextModelConfig,
    TextModelError, LM_STUDIO_URL, OLLAMA_URL, UNREACHABLE_HINT,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Longest reply body read (a text reply, not a download).
const MAX_BODY: u64 = 16 * 1024 * 1024;

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_connect(Some(Duration::from_secs(10).min(timeout)))
        .timeout_global(Some(timeout))
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

struct Reply {
    status: u16,
    body: String,
}

enum Fail {
    Timeout,
    Unreachable(String),
}

fn classify(e: &ureq::Error) -> Fail {
    match e {
        ureq::Error::Timeout(t) => {
            if matches!(t, ureq::Timeout::Resolve | ureq::Timeout::Connect) {
                Fail::Unreachable(e.to_string())
            } else {
                Fail::Timeout
            }
        }
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => Fail::Timeout,
        other => Fail::Unreachable(other.to_string()),
    }
}

fn read_body(mut resp: ureq::http::Response<ureq::Body>) -> Result<Reply, Fail> {
    let status = resp.status().as_u16();
    let mut body = String::new();
    resp.body_mut()
        .as_reader()
        .take(MAX_BODY)
        .read_to_string(&mut body)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut || e.to_string().contains("timeout") {
                Fail::Timeout
            } else {
                Fail::Unreachable(e.to_string())
            }
        })?;
    Ok(Reply { status, body })
}

fn get(url: &str, headers: &[(String, String)], timeout: Duration) -> Result<Reply, Fail> {
    let mut req = agent(timeout).get(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    read_body(req.call().map_err(|e| classify(&e))?)
}

fn post(
    url: &str,
    headers: &[(String, String)],
    body: &Value,
    timeout: Duration,
) -> Result<Reply, Fail> {
    let bytes = serde_json::to_vec(body).map_err(|e| Fail::Unreachable(e.to_string()))?;
    let mut req = agent(timeout)
        .post(url)
        .header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    read_body(req.send(&bytes[..]).map_err(|e| classify(&e))?)
}

/// A model the server offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelEntry {
    pub id: String,
    /// The context window, when the server reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
}

/// The `ai.text` provider over HTTP.
pub struct HttpTextModel {
    config: TextModelConfig,
    key: Option<String>,
}

impl HttpTextModel {
    pub fn new(config: TextModelConfig, key: Option<String>) -> Self {
        HttpTextModel {
            config,
            key: key.filter(|k| !k.is_empty()),
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.config.timeout_s.max(1))
    }

    /// Hides the key if a server echoed it back.
    fn mask(&self, text: &str) -> String {
        match &self.key {
            Some(k) if text.contains(k.as_str()) => text.replace(k.as_str(), "***"),
            _ => text.to_owned(),
        }
    }

    fn unreachable(&self, detail: &str) -> TextModelError {
        let host = self.config.host().unwrap_or_default();
        let message = if self.config.is_local() {
            format!("The text model server at {host} did not answer. {UNREACHABLE_HINT}.")
        } else {
            format!("Cannot reach {host} ({}).", self.mask(detail))
        };
        TextModelError {
            kind: ErrorKind::Unreachable,
            message,
        }
    }

    fn fail(&self, f: Fail) -> TextModelError {
        match f {
            Fail::Timeout => TextModelError {
                kind: ErrorKind::Timeout,
                message: format!(
                    "The text model did not answer within {} seconds",
                    self.config.timeout_s
                ),
            },
            Fail::Unreachable(d) => self.unreachable(&d),
        }
    }

    fn headers(&self) -> Vec<(String, String)> {
        let mut h = Vec::new();
        match (self.config.provider, &self.key) {
            (Provider::Anthropic, Some(k)) => {
                h.push(("x-api-key".to_owned(), k.clone()));
                h.push(("anthropic-version".to_owned(), ANTHROPIC_VERSION.to_owned()));
            }
            (Provider::Anthropic, None) => {
                h.push(("anthropic-version".to_owned(), ANTHROPIC_VERSION.to_owned()));
            }
            (_, Some(k)) => h.push(("authorization".to_owned(), format!("Bearer {k}"))),
            _ => {}
        }
        h
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
        let detail: String = self.mask(&detail).chars().take(400).collect();
        let host = self.config.host().unwrap_or_default();
        let message = match r.status {
            401 | 403 => format!("{host} refused the API key (HTTP {}). {detail}", r.status),
            404 => format!("The text model was not found on {host} (HTTP 404). {detail}"),
            _ => format!("{host} answered HTTP {}: {detail}", r.status),
        };
        TextModelError {
            kind: ErrorKind::Server,
            message: message.trim().to_owned(),
        }
    }

    /// One round trip: the model's raw text for `messages`.
    fn chat_once(&self, req: &JsonRequest, messages: &[Message]) -> Result<String, TextModelError> {
        self.config.check_endpoint()?;
        if self.config.model.is_empty() {
            return Err(TextModelError::invalid_input(
                "Choose a text model on the AI page first",
            ));
        }
        match self.config.provider {
            Provider::Anthropic => self.anthropic_once(req, messages),
            _ => self.openai_once(req, messages),
        }
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
    ) -> Result<String, TextModelError> {
        let url = format!("{}/chat/completions", self.config.effective_base_url());
        let mut msgs = vec![json!({ "role": "system", "content": Self::system_text(req) })];
        msgs.extend(
            messages
                .iter()
                .map(|m| json!({ "role": m.role, "content": m.content })),
        );
        let mut body = json!({
            "model": self.config.model,
            "messages": msgs,
            "stream": false,
            "temperature": 0,
            "max_tokens": req.max_tokens,
            "response_format": { "type": "json_schema", "json_schema": {
                "name": req.schema_name, "strict": true, "schema": req.schema } },
        });
        // Ollama's own structured-output field (its OpenAI endpoint also reads response_format).
        if self.config.effective_base_url() == OLLAMA_URL
            || self.config.effective_base_url().contains(":11434")
        {
            body["format"] = req.schema.clone();
        }
        let headers = self.headers();
        let mut reply = post(&url, &headers, &body, self.timeout()).map_err(|f| self.fail(f))?;
        // A server that knows only `json_object` (or nothing) refuses `json_schema`: the schema
        // is already in the system prompt, so ask again without the field.
        if matches!(reply.status, 400 | 422)
            && (reply.body.contains("response_format") || reply.body.contains("json_schema"))
        {
            if let Some(o) = body.as_object_mut() {
                o.remove("response_format");
                o.remove("format");
            }
            reply = post(&url, &headers, &body, self.timeout()).map_err(|f| self.fail(f))?;
        }
        if !(200..300).contains(&reply.status) {
            return Err(self.server_error(&reply));
        }
        let v: Value = serde_json::from_str(&reply.body).map_err(|_| TextModelError {
            kind: ErrorKind::Server,
            message: "The text model server sent a reply that is not JSON. Is the address an OpenAI-compatible /v1 URL?"
                .to_owned(),
        })?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| TextModelError {
                kind: ErrorKind::Server,
                message: "The text model server's reply has no message".to_owned(),
            })
    }

    fn anthropic_once(
        &self,
        req: &JsonRequest,
        messages: &[Message],
    ) -> Result<String, TextModelError> {
        if self.key.is_none() {
            return Err(TextModelError::invalid_input(
                "Add the Anthropic API key in Settings > AI first",
            ));
        }
        let url = format!("{}/v1/messages", self.config.effective_base_url());
        let tool = req.schema_name.clone();
        let body = json!({
            "model": self.config.model,
            "max_tokens": req.max_tokens,
            "system": req.system.clone().unwrap_or_default(),
            "messages": messages.iter().map(|m| json!({ "role": m.role, "content": m.content }))
                .collect::<Vec<_>>(),
            "tools": [{ "name": tool, "description": "Return the answer as structured data.",
                        "input_schema": req.schema }],
            "tool_choice": { "type": "tool", "name": tool },
        });
        let reply = post(&url, &self.headers(), &body, self.timeout()).map_err(|f| self.fail(f))?;
        if !(200..300).contains(&reply.status) {
            return Err(self.server_error(&reply));
        }
        let v: Value = serde_json::from_str(&reply.body).map_err(|_| TextModelError {
            kind: ErrorKind::Server,
            message: "The Anthropic reply is not JSON".to_owned(),
        })?;
        let blocks = v["content"].as_array().cloned().unwrap_or_default();
        if let Some(input) = blocks
            .iter()
            .find(|b| b["type"] == "tool_use")
            .map(|b| &b["input"])
        {
            return Ok(input.to_string());
        }
        // No tool call: take the text and let the validation retry deal with it.
        Ok(blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join(""))
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
        let check = |text: &str| -> Result<Value, String> {
            let v = extract_json(text)?;
            validate(&req.schema, &v)?;
            Ok(v)
        };
        let first = self.chat_once(req, &req.messages)?;
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
        let second = self.chat_once(req, &again)?;
        match check(&second) {
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

fn parse_models(body: &str) -> Vec<ModelEntry> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let list = v["data"].as_array().or_else(|| v["models"].as_array());
    list.map(|items| {
        items
            .iter()
            .filter_map(|m| {
                let id = m["id"].as_str().or_else(|| m["name"].as_str())?.to_owned();
                let ctx = [
                    "context_length",
                    "max_context_length",
                    "context_window",
                    "n_ctx",
                ]
                .iter()
                .find_map(|k| m[*k].as_u64())
                .or_else(|| m["meta"]["n_ctx_train"].as_u64());
                Some(ModelEntry {
                    id,
                    context_tokens: ctx,
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// `GET {base_url}/models`: the models the server offers (and their context size when it says).
pub fn list_models(
    config: &TextModelConfig,
    key: Option<String>,
) -> Result<Vec<ModelEntry>, TextModelError> {
    config.check_endpoint()?;
    let m = HttpTextModel::new(config.clone(), key);
    let base = config.effective_base_url();
    let url = if config.provider == Provider::Anthropic {
        format!("{base}/v1/models")
    } else {
        format!("{base}/models")
    };
    let timeout = m.timeout().min(Duration::from_secs(10));
    let reply = get(&url, &m.headers(), timeout).map_err(|f| m.fail(f))?;
    if !(200..300).contains(&reply.status) {
        return Err(m.server_error(&reply));
    }
    Ok(parse_models(&reply.body))
}

/// A preset server and whether it answers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetStatus {
    /// `ollama` or `lm_studio`.
    pub preset: &'static str,
    pub base_url: &'static str,
    pub running: bool,
    pub models: Vec<ModelEntry>,
}

/// Looks for Ollama and LM Studio on this machine (loopback only, 2 s each). The AI page uses it
/// to list their models (AI-7.AC1).
pub fn detect_presets() -> Vec<PresetStatus> {
    [("ollama", OLLAMA_URL), ("lm_studio", LM_STUDIO_URL)]
        .into_iter()
        .map(|(preset, base_url)| {
            let cfg = TextModelConfig {
                provider: Provider::Local,
                base_url: base_url.to_owned(),
                timeout_s: 2,
                ..TextModelConfig::default()
            };
            let models = list_models(&cfg, None);
            PresetStatus {
                preset,
                base_url,
                running: models.is_ok(),
                models: models.unwrap_or_default(),
            }
        })
        .collect()
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

    #[test]
    fn parses_model_lists() {
        let m = parse_models(
            r#"{"data":[{"id":"qwen3:8b"},{"id":"llama","max_context_length":32768}]}"#,
        );
        assert_eq!(m[0].id, "qwen3:8b");
        assert_eq!(m[0].context_tokens, None);
        assert_eq!(m[1].context_tokens, Some(32768));
        assert_eq!(parse_models(r#"{"models":[{"name":"x"}]}"#)[0].id, "x");
        assert!(parse_models("garbage").is_empty());
    }
}
