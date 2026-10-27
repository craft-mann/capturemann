//! AI-7 against a scripted OpenAI-compatible server on loopback (no external network, no model).
#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cm_ai::text_model::{build, test_model, ErrorKind, JsonRequest, Provider, TextModelConfig};
use serde_json::{json, Value};

#[derive(Clone, Debug)]
struct Seen {
    path: String,
    headers: String,
    body: Value,
}

struct Scripted {
    port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
}

/// `script(n, request)` returns `(status, body)` for the n-th request (from 0).
fn serve(script: impl Fn(usize, &Seen) -> (u16, String) + Send + Sync + 'static) -> Scripted {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let log = seen.clone();
    let script = Arc::new(script);
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().enumerate() {
            let Ok(stream) = stream else { break };
            let (log, script) = (log.clone(), script.clone());
            std::thread::spawn(move || handle(stream, n, &log, &*script));
        }
    });
    Scripted { port, seen }
}

fn handle(
    mut s: TcpStream,
    n: usize,
    log: &Mutex<Vec<Seen>>,
    script: &(dyn Fn(usize, &Seen) -> (u16, String) + Send + Sync),
) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let Ok(k) = s.read(&mut chunk) else { return };
        if k == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..k]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let len: usize = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap_or(0);
    while buf.len() < head_end + len {
        let Ok(k) = s.read(&mut chunk) else { return };
        if k == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..k]);
    }
    let body = serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null);
    let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
    let seen = Seen {
        path,
        headers: head.to_ascii_lowercase(),
        body,
    };
    let (status, reply) = script(n, &seen);
    log.lock().unwrap().push(seen);
    let _ = write!(
        s,
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
        reply.len()
    );
}

fn chat(content: &str) -> String {
    json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] }).to_string()
}

fn config(port: u16, provider: Provider) -> TextModelConfig {
    TextModelConfig {
        provider,
        base_url: format!("http://127.0.0.1:{port}/v1"),
        model: "test-model".into(),
        embedded_model: String::new(),
        key_name: None,
        context_tokens: 8192,
        timeout_s: 5,
    }
}

fn request() -> JsonRequest {
    JsonRequest::new(
        "Summarise",
        json!({ "type": "object", "properties": { "title": { "type": "string" } },
                "required": ["title"], "additionalProperties": false }),
    )
}

#[test]
fn schema_success_first_try_with_json_schema_and_key() {
    let s = serve(|_, _| (200, chat("{\"title\": \"Hello\"}")));
    let m = build(
        &config(s.port, Provider::OpenAiCompatible),
        Some("sk-secret".into()),
    );
    let r = m.complete_json(&request()).unwrap();
    assert_eq!((r.value, r.attempts), (json!({"title": "Hello"}), 1));
    let seen = s.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].path, "/v1/chat/completions");
    assert_eq!(seen[0].body["response_format"]["type"], "json_schema");
    assert_eq!(
        seen[0].body["response_format"]["json_schema"]["schema"]["required"][0],
        "title"
    );
    assert_eq!(seen[0].body["model"], "test-model");
    assert!(seen[0].headers.contains("authorization: bearer sk-secret"));
}

#[test]
fn one_retry_carries_the_validation_error() {
    let s = serve(|n, _| {
        if n == 0 {
            (200, chat("{\"name\": \"wrong\"}"))
        } else {
            (200, chat("```json\n{\"title\": \"Fixed\"}\n```"))
        }
    });
    let m = build(&config(s.port, Provider::Local), None);
    let r = m.complete_json(&request()).unwrap();
    assert_eq!((r.value, r.attempts), (json!({"title": "Fixed"}), 2));
    let seen = s.seen.lock().unwrap();
    let msgs = seen[1].body["messages"].as_array().unwrap();
    let last = msgs.last().unwrap()["content"].as_str().unwrap();
    assert!(last.contains("missing required property `title`"), "{last}");
    assert_eq!(msgs[msgs.len() - 2]["role"], "assistant");
}

#[test]
fn schema_failure_twice_is_invalid_model_output() {
    let s = serve(|_, _| (200, chat("{\"nope\": 1}")));
    let m = build(&config(s.port, Provider::Local), None);
    let e = m.complete_json(&request()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidOutput);
    assert_eq!(e.reason(), "invalid_model_output");
    assert_eq!(s.seen.lock().unwrap().len(), 2, "exactly one retry");
    let cm = e.into_cm_error("job_1");
    assert_eq!(cm.code().as_str(), "job_failed");
}

#[test]
fn a_slow_server_times_out() {
    let s = serve(|_, _| {
        std::thread::sleep(Duration::from_secs(3));
        (200, chat("{\"title\": \"late\"}"))
    });
    let mut c = config(s.port, Provider::Local);
    c.timeout_s = 1;
    let e = build(&c, None).complete_json(&request()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Timeout);
    assert_eq!(e.reason(), "timeout");
}

#[test]
fn nothing_listening_is_unreachable_with_the_hint() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let e = build(&config(port, Provider::Local), None)
        .complete_json(&request())
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unreachable);
    assert_eq!(e.reason(), "text_model_unreachable");
    assert!(e
        .message
        .contains("Open the AI page and check the built-in text model"));
}

#[test]
fn the_server_message_is_passed_through_and_the_key_is_masked() {
    let s = serve(|_, _| {
        (
            500,
            json!({ "error": { "message": "model failed to load; key sk-secret rejected" } })
                .to_string(),
        )
    });
    let m = build(
        &config(s.port, Provider::OpenAiCompatible),
        Some("sk-secret".into()),
    );
    let e = m.complete_json(&request()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Server);
    assert!(e.message.contains("model failed to load"));
    assert!(!e.message.contains("sk-secret"), "{}", e.message);
}

#[test]
fn a_server_without_json_schema_support_gets_one_plain_retry() {
    let s = serve(|n, _| {
        if n == 0 {
            (
                400,
                json!({"error": {"message": "response_format.type must be json_object"}})
                    .to_string(),
            )
        } else {
            (200, chat("{\"title\": \"ok\"}"))
        }
    });
    let r = build(&config(s.port, Provider::Local), None)
        .complete_json(&request())
        .unwrap();
    assert_eq!(r.attempts, 1);
    let seen = s.seen.lock().unwrap();
    assert!(seen[1].body.get("response_format").is_none());
    // The schema is also in the system prompt.
    assert!(seen[1].body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("\"required\""));
}

#[test]
fn provider_none_makes_no_call_and_a_lan_local_provider_is_refused() {
    let s = serve(|_, _| (200, chat("{\"title\": \"x\"}")));
    let mut none = config(s.port, Provider::None);
    none.provider = Provider::None;
    let e = build(&none, None).complete_json(&request()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::NoTextModel);
    assert_eq!(e.error_data()["reason"], "no_text_model");
    assert_eq!(e.into_cm_error("j").code().as_str(), "invalid_input");

    let mut lan = config(s.port, Provider::Local);
    lan.base_url = format!("http://192.168.1.50:{}/v1", s.port);
    assert!(!lan.is_local());
    let e = build(&lan, None).complete_json(&request()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    assert!(
        s.seen.lock().unwrap().is_empty(),
        "nothing reached the server"
    );
}

#[test]
fn anthropic_uses_tool_input_and_needs_a_key() {
    let s = serve(|_, _| {
        (
            200,
            json!({ "content": [{ "type": "tool_use", "name": "reply", "input": { "title": "Hi" } }] })
                .to_string(),
        )
    });
    let mut c = config(s.port, Provider::Anthropic);
    c.base_url = format!("http://127.0.0.1:{}", s.port);
    let e = build(&c, None).complete_json(&request()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    let r = build(&c, Some("ak-1".into()))
        .complete_json(&request())
        .unwrap();
    assert_eq!(r.value, json!({"title": "Hi"}));
    let seen = s.seen.lock().unwrap();
    assert_eq!(seen[0].path, "/v1/messages");
    assert!(seen[0].headers.contains("x-api-key: ak-1"));
    assert_eq!(seen[0].body["tool_choice"]["name"], "reply");
    assert_eq!(
        seen[0].body["tools"][0]["input_schema"]["required"][0],
        "title"
    );
}

#[test]
fn the_test_button_reports_latency_model_and_structured_output() {
    let s = serve(|_, _| (200, chat("{\"ok\": true}")));
    let m = build(&config(s.port, Provider::Local), None);
    let t = test_model(m.as_ref(), "test-model");
    assert!(t.ok && t.structured_output);
    assert_eq!(t.model, "test-model");

    let bad = serve(|_, _| (200, chat("not json")));
    let m = build(&config(bad.port, Provider::Local), None);
    let t = test_model(m.as_ref(), "test-model");
    assert!(!t.ok);
    assert_eq!(t.reason.as_deref(), Some("invalid_model_output"));
}

// ---- the built-in provider (AI-7, `embedded`) ----------------------------------------------

use cm_ai::text_model::engine::{EmbeddedTextModel, EngineSpec, EngineStatus, TextEngine};
use cm_ai::text_model::TextModel;

/// An engine that hands out fixed loopback addresses, counting acquire and release.
struct FakeEngine {
    urls: Mutex<Vec<String>>,
    acquired: Mutex<u32>,
    released: Mutex<u32>,
}

impl FakeEngine {
    fn new(urls: Vec<String>) -> Arc<FakeEngine> {
        Arc::new(FakeEngine {
            urls: Mutex::new(urls),
            acquired: Mutex::new(0),
            released: Mutex::new(0),
        })
    }

    fn counts(&self) -> (u32, u32) {
        (
            *self.acquired.lock().unwrap(),
            *self.released.lock().unwrap(),
        )
    }
}

impl TextEngine for FakeEngine {
    fn acquire(&self, _: &EngineSpec) -> Result<String, cm_ai::text_model::TextModelError> {
        *self.acquired.lock().unwrap() += 1;
        let mut urls = self.urls.lock().unwrap();
        Ok(if urls.len() > 1 {
            urls.remove(0)
        } else {
            urls[0].clone()
        })
    }
    fn release(&self) {
        *self.released.lock().unwrap() += 1;
    }
    fn status(&self) -> EngineStatus {
        unreachable!()
    }
    fn stop(&self) {}
    fn reap_idle(&self) -> bool {
        false
    }
}

fn embedded(engine: Arc<FakeEngine>) -> EmbeddedTextModel {
    EmbeddedTextModel::new(
        engine,
        EngineSpec {
            model_id: "built-in".into(),
            model_path: "/models/built-in.gguf".into(),
            ram_mb: 0,
            context_tokens: 8192,
        },
        &TextModelConfig {
            timeout_s: 5,
            ..TextModelConfig::default()
        },
    )
}

#[test]
fn embedded_asks_the_engine_for_the_address_and_talks_openai() {
    let s = serve(|_, _| (200, chat("{\"title\": \"Hi\"}")));
    let e = FakeEngine::new(vec![format!("http://127.0.0.1:{}/v1", s.port)]);
    let r = embedded(e.clone()).complete_json(&request()).unwrap();
    assert_eq!(r.value["title"], "Hi");
    let seen = s.seen.lock().unwrap();
    assert_eq!(seen[0].path, "/v1/chat/completions");
    assert_eq!(seen[0].body["model"], "built-in");
    assert_eq!(e.counts(), (1, 1));
}

#[test]
fn embedded_retries_once_when_the_helper_died_and_gives_up_clearly() {
    let s = serve(|_, _| (200, chat("{\"title\": \"Back\"}")));
    // The first address is dead; the engine hands out a live one on the second acquire.
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = dead.local_addr().unwrap().port();
    drop(dead);
    let e = FakeEngine::new(vec![
        format!("http://127.0.0.1:{dead_port}/v1"),
        format!("http://127.0.0.1:{}/v1", s.port),
    ]);
    let r = embedded(e.clone()).complete_json(&request()).unwrap();
    assert_eq!(r.value["title"], "Back");
    assert_eq!(e.counts(), (2, 2));

    // Dead both times: engine_failed, not the unreachable hint.
    let e = FakeEngine::new(vec![format!("http://127.0.0.1:{dead_port}/v1")]);
    let err = embedded(e).complete_json(&request()).unwrap_err();
    assert_eq!(err.reason(), "engine_failed");
    assert!(!err.message.contains("AI page and check"), "{err}");
}

#[test]
fn embedded_is_local_with_no_remote_plan_and_settings_can_choose_it() {
    let mut m = serde_json::Map::new();
    m.insert("ai.text.provider".into(), json!("embedded"));
    m.insert("ai.text.embedded_model".into(), json!("qwen3.5-4b"));
    m.insert("ai.text.base_url".into(), json!("https://evil.example/v1"));
    let c = TextModelConfig::from_settings(&m);
    assert_eq!(c.provider, Provider::Embedded);
    assert_eq!(c.embedded_model, "qwen3.5-4b");
    assert!(c.is_local() && !c.is_remote());
    assert!(c.send_plan(100).is_none());
    assert!(c.check_endpoint().is_ok());
    assert_eq!(c.host().as_deref(), Some("127.0.0.1"));
    // Without an engine wired in, `build` cannot answer and says why.
    let err = build(&c, None).complete_json(&request()).unwrap_err();
    assert_eq!(err.reason(), "engine_missing");
}

// ---- streaming (AI-9 progress): the answer arrives as server-sent events ---------------------

/// What a scripted streaming server does after the request head and body are read.
#[derive(Clone)]
struct Sse {
    /// The `delta.content` pieces, one event each.
    pieces: Vec<String>,
    /// Pause between two events.
    gap: Duration,
    /// Before this event, stay silent for the duration.
    stall_at: Option<(usize, Duration)>,
}

impl Sse {
    fn new(pieces: &[&str]) -> Sse {
        Sse {
            pieces: pieces.iter().map(|p| (*p).to_owned()).collect(),
            gap: Duration::from_millis(5),
            stall_at: None,
        }
    }
}

fn send_events(s: &mut TcpStream, sse: &Sse) {
    let _ = write!(
        s,
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n"
    );
    let _ = write!(
        s,
        "data: {}\n\n",
        json!({"choices":[{"delta":{"role":"assistant"}}]})
    );
    for (i, p) in sse.pieces.iter().enumerate() {
        if let Some((n, d)) = sse.stall_at {
            if i == n {
                std::thread::sleep(d);
            }
        }
        let ev = json!({"choices":[{"delta":{"content": p}}]});
        if write!(s, "data: {ev}\n\n").is_err() || s.flush().is_err() {
            return;
        }
        std::thread::sleep(sse.gap);
    }
    let _ = write!(s, "data: [DONE]\n\n");
}

/// A server that answers a request with `stream: true` with `script(n)` as `text/event-stream`
/// and any other request with the whole text as plain JSON.
fn serve_sse(script: impl Fn(usize) -> Sse + Send + Sync + 'static) -> Scripted {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let log = seen.clone();
    let script = Arc::new(script);
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().enumerate() {
            let Ok(mut s) = stream else { break };
            let (log, script) = (log.clone(), script.clone());
            std::thread::spawn(move || {
                let Some(req) = read_request(&mut s) else {
                    return;
                };
                let streaming = req.body["stream"] == json!(true);
                log.lock().unwrap().push(req);
                let sse = script(n);
                if streaming {
                    send_events(&mut s, &sse);
                } else {
                    let body = chat(&sse.pieces.concat());
                    let _ = write!(
                        s,
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            });
        }
    });
    Scripted { port, seen }
}

fn read_request(s: &mut TcpStream) -> Option<Seen> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let k = s.read(&mut chunk).ok()?;
        if k == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..k]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let len: usize = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap_or(0);
    while buf.len() < head_end + len {
        let k = s.read(&mut chunk).ok()?;
        if k == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..k]);
    }
    Some(Seen {
        path: head.split_whitespace().nth(1).unwrap_or("").to_owned(),
        headers: head.to_ascii_lowercase(),
        body: serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null),
    })
}

const PIECES: [&str; 6] = ["{\"ti", "tle\"", ": \"He", "llo ", "wor", "ld\"}"];

#[test]
fn a_streamed_answer_reports_growing_progress_and_assembles_the_same_json() {
    let s = serve_sse(|_| Sse::new(&PIECES));
    let m = build(&config(s.port, Provider::OpenAiCompatible), None);
    let mut ticks = Vec::new();
    let streamed = m
        .complete_json_streaming(&request(), &mut |n| {
            ticks.push(n);
            true
        })
        .unwrap();
    assert_eq!(ticks, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(streamed.value, json!({"title": "Hello world"}));
    assert_eq!(streamed.attempts, 1);
    // The plain call to the same server asks without `stream` and gets the same value.
    let plain = m.complete_json(&request()).unwrap();
    assert_eq!(plain.value, streamed.value);
    let seen = s.seen.lock().unwrap();
    assert_eq!(seen[0].body["stream"], json!(true));
    assert_eq!(seen[0].body["response_format"]["type"], "json_schema");
    assert_eq!(seen[1].body["stream"], json!(false));
}

#[test]
fn the_progress_count_keeps_growing_over_the_validation_retry() {
    // The first answer breaks the schema (two pieces), the retry is good (six pieces).
    let s = serve_sse(|n| {
        if n == 0 {
            Sse::new(&["{\"na", "me\": 1}"])
        } else {
            Sse::new(&PIECES)
        }
    });
    let m = build(&config(s.port, Provider::Local), None);
    let mut ticks = Vec::new();
    let r = m
        .complete_json_streaming(&request(), &mut |n| {
            ticks.push(n);
            true
        })
        .unwrap();
    assert_eq!((r.value, r.attempts), (json!({"title": "Hello world"}), 2));
    assert_eq!(ticks, (1..=8).collect::<Vec<_>>());
}

#[test]
fn a_server_that_ignores_stream_still_works_without_progress() {
    // Answers a streaming request with a plain JSON body.
    let s = serve(|_, _| (200, chat("{\"title\": \"Plain\"}")));
    let m = build(&config(s.port, Provider::Local), None);
    let mut ticks = 0;
    let r = m
        .complete_json_streaming(&request(), &mut |_| {
            ticks += 1;
            true
        })
        .unwrap();
    assert_eq!(r.value, json!({"title": "Plain"}));
    assert_eq!(ticks, 0);
    assert_eq!(s.seen.lock().unwrap()[0].body["stream"], json!(true));
}

#[test]
fn a_streaming_request_refused_for_response_format_is_asked_again_without_it() {
    let s = serve(|n, _| {
        if n == 0 {
            (
                400,
                json!({"error": {"message": "bad response_format"}}).to_string(),
            )
        } else {
            (200, chat("{\"title\": \"Second\"}"))
        }
    });
    let m = build(&config(s.port, Provider::Local), None);
    let r = m
        .complete_json_streaming(&request(), &mut |_| true)
        .unwrap();
    assert_eq!(r.value["title"], "Second");
    let seen = s.seen.lock().unwrap();
    assert!(seen[1].body.get("response_format").is_none());
}

#[test]
fn an_error_status_to_a_streaming_request_is_mapped_like_before() {
    let s = serve(|_, _| {
        (
            500,
            json!({"error": {"message": "model exploded"}}).to_string(),
        )
    });
    let m = build(&config(s.port, Provider::Local), None);
    let e = m
        .complete_json_streaming(&request(), &mut |_| true)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Server);
    assert!(e.message.contains("model exploded"), "{e}");
}

#[test]
fn a_stalled_stream_hits_the_timeout() {
    let s = serve_sse(|_| {
        let mut sse = Sse::new(&PIECES);
        sse.stall_at = Some((2, Duration::from_secs(6)));
        sse
    });
    let mut c = config(s.port, Provider::Local);
    c.timeout_s = 1;
    let mut ticks = 0;
    let t0 = std::time::Instant::now();
    let e = build(&c, None)
        .complete_json_streaming(&request(), &mut |_| {
            ticks += 1;
            true
        })
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Timeout);
    assert_eq!(ticks, 2);
    assert!(t0.elapsed() < Duration::from_secs(4), "{:?}", t0.elapsed());
}

#[test]
fn a_cancel_in_the_middle_of_a_stream_stops_the_call() {
    let s = serve_sse(|_| {
        let mut sse = Sse::new(&PIECES);
        sse.gap = Duration::from_millis(50);
        sse
    });
    let mut ticks = 0;
    let t0 = std::time::Instant::now();
    let e = build(&config(s.port, Provider::Local), None)
        .complete_json_streaming(&request(), &mut |n| {
            ticks = n;
            n < 2
        })
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Cancelled);
    assert_eq!(ticks, 2, "stopped at the piece that was refused");
    assert!(t0.elapsed() < Duration::from_millis(250));
    // No retry after a cancel.
    assert_eq!(s.seen.lock().unwrap().len(), 1);
}

#[test]
fn an_error_event_inside_the_stream_is_a_server_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            std::thread::spawn(move || {
                let _ = read_request(&mut s);
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n"
                );
                let _ = write!(
                    s,
                    "data: {}\n\n",
                    json!({"choices":[{"delta":{"content":"{\"ti"}}]})
                );
                let _ = write!(
                    s,
                    "data: {}\n\n",
                    json!({"error":{"message":"out of memory"}})
                );
            });
        }
    });
    let e = build(&config(port, Provider::Local), None)
        .complete_json_streaming(&request(), &mut |_| true)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Server);
    assert!(e.message.contains("out of memory"), "{e}");
}
