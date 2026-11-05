//! A small MCP client over stdio (newline-delimited JSON-RPC 2.0) for black-box tests of `cm-mcp`.
//! It keeps a log of everything sent and received so a failing case can print the exchange.

use crate::ctx::Ctx;
use crate::model::{Stop, R};
use crate::sys;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// The answer to one request.
#[derive(Debug, Clone)]
pub struct Response {
    pub result: Option<Value>,
    /// JSON-RPC error: (code, message).
    pub error: Option<(i64, String)>,
}

/// The answer to `tools/call`.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub is_error: bool,
    pub text: String,
    pub structured: Option<Value>,
}

impl ToolResult {
    /// The result as JSON: `structuredContent`, else the text parsed.
    pub fn json(&self) -> Option<Value> {
        self.structured
            .clone()
            .or_else(|| serde_json::from_str(&self.text).ok())
    }
    /// The shared error code of a failed call (`structuredContent.code`).
    pub fn error_code(&self) -> Option<String> {
        if !self.is_error {
            return None;
        }
        self.structured
            .as_ref()?
            .get("code")?
            .as_str()
            .map(str::to_owned)
    }
}

pub struct McpClient {
    writer: Box<dyn Write + Send>,
    rx: Receiver<Result<Value, String>>,
    next_id: i64,
    /// Messages without an id (notifications) seen so far.
    pub notifications: Vec<Value>,
    /// The exchange, one line per message, for failure reports.
    pub log: Vec<String>,
    child: Option<Child>,
    tracker: Option<(crate::proc::Tracker, u32)>,
    pub default_timeout: Duration,
}

impl McpClient {
    /// Over any pair of streams (a spawned child's pipes, or a socket in the unit tests).
    pub fn from_streams(
        writer: Box<dyn Write + Send>,
        reader: Box<dyn Read + Send>,
        child: Option<Child>,
    ) -> McpClient {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut lines = BufReader::new(reader).lines();
            loop {
                match lines.next() {
                    Some(Ok(l)) if l.trim().is_empty() => {}
                    Some(Ok(l)) => {
                        let v = serde_json::from_str::<Value>(&l)
                            .map_err(|e| format!("not JSON ({e}): {l}"));
                        if tx.send(v).is_err() {
                            return;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = tx.send(Err(format!("read error: {e}")));
                        return;
                    }
                    None => {
                        let _ = tx.send(Err("the server closed its output".to_owned()));
                        return;
                    }
                }
            }
        });
        McpClient {
            writer,
            rx,
            next_id: 1,
            notifications: Vec::new(),
            log: Vec::new(),
            child,
            tracker: None,
            default_timeout: Duration::from_secs(30),
        }
    }

    /// Starts `cm-mcp` of the package against the sandbox daemon.
    pub fn spawn(ctx: &mut Ctx, extra: &[&str]) -> R<McpClient> {
        let ep = ctx.endpoint.clone();
        let err = std::fs::File::create(ctx.root.join(format!(
            "mcp-{}.err.txt",
            ctx.tracker.lock().map_or(0, |t| t.len())
        )))?;
        let mut c = ctx.product_cmd("cm-mcp");
        c.args(["--socket", &ep]).args(extra);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(err));
        let mut child = c
            .spawn()
            .map_err(|e| Stop::Fail(format!("cm-mcp did not start: {e}")))?;
        if let Ok(mut t) = ctx.tracker.lock() {
            t.push(child.id());
        }
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(Stop::Fail("cm-mcp has no pipes".to_owned()));
        };
        let mut cl = McpClient::from_streams(Box::new(stdin), Box::new(stdout), Some(child));
        cl.default_timeout = ctx.secs(30);
        cl.tracker = cl.child.as_ref().map(|c| (ctx.tracker.clone(), c.id()));
        Ok(cl)
    }

    fn send(&mut self, v: &Value) -> Result<(), String> {
        self.log.push(format!("-> {v}"));
        let mut line = v.to_string();
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .and_then(|()| self.writer.flush())
            .map_err(|e| format!("write to the server failed: {e}"))
    }

    /// Sends a notification (no answer).
    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    /// Sends a request and waits for the answer with the same id; notifications on the way are kept.
    pub fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Response, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(Ok(v)) => {
                    self.log
                        .push(format!("<- {}", crate::util::cut(&v.to_string(), 600)));
                    if v.get("id") == Some(&json!(id)) {
                        let error = v.get("error").map(|e| {
                            (
                                e.get("code").and_then(Value::as_i64).unwrap_or(0),
                                e.get("message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_owned(),
                            )
                        });
                        return Ok(Response {
                            result: v.get("result").cloned(),
                            error,
                        });
                    }
                    if v.get("id").is_none() {
                        self.notifications.push(v);
                    }
                    // A request from the server (sampling, roots): not expected; ignored.
                }
                Ok(Err(e)) => return Err(e),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "no answer to `{method}` within {} s",
                        timeout.as_secs()
                    ))
                }
                Err(RecvTimeoutError::Disconnected) => return Err("the reader stopped".to_owned()),
            }
        }
    }

    /// `initialize` and `notifications/initialized`; returns the initialize result.
    pub fn initialize(&mut self) -> Result<Value, String> {
        let t = self.default_timeout;
        let r = self.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "cm-regress", "version": "0"}}),
            t,
        )?;
        let result = r
            .result
            .ok_or_else(|| format!("initialize failed: {:?}", r.error))?;
        self.notify("notifications/initialized", json!({}))?;
        Ok(result)
    }

    /// The tools of `tools/list` (all pages).
    pub fn list_tools(&mut self) -> Result<Vec<Value>, String> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let t = self.default_timeout;
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let r = self.request("tools/list", params, t)?;
            let res = r
                .result
                .ok_or_else(|| format!("tools/list failed: {:?}", r.error))?;
            if let Some(a) = res.get("tools").and_then(Value::as_array) {
                all.extend(a.iter().cloned());
            }
            match res.get("nextCursor").and_then(Value::as_str) {
                Some(c) => cursor = Some(c.to_owned()),
                None => return Ok(all),
            }
        }
    }

    /// `tools/call`. A JSON-RPC level error (unknown tool, bad params) is an `Err((code, message))`.
    pub fn call(
        &mut self,
        name: &str,
        args: Value,
    ) -> Result<Result<ToolResult, (i64, String)>, String> {
        let t = self.default_timeout;
        self.call_with_timeout(name, args, t)
    }

    pub fn call_with_timeout(
        &mut self,
        name: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<Result<ToolResult, (i64, String)>, String> {
        let r = self.request(
            "tools/call",
            json!({"name": name, "arguments": args}),
            timeout,
        )?;
        if let Some(e) = r.error {
            return Ok(Err(e));
        }
        let res = r
            .result
            .ok_or("tools/call returned neither result nor error")?;
        let text = res
            .get("content")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(Ok(ToolResult {
            is_error: res.get("isError").and_then(Value::as_bool).unwrap_or(false),
            text,
            structured: res.get("structuredContent").cloned(),
        }))
    }

    /// Waits up to `timeout` for more notifications and returns all collected so far.
    pub fn drain_notifications(&mut self, timeout: Duration) -> &[Value] {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(Ok(v)) => {
                    self.log
                        .push(format!("<- {}", crate::util::cut(&v.to_string(), 600)));
                    if v.get("id").is_none() {
                        self.notifications.push(v);
                    }
                }
                _ => break,
            }
        }
        &self.notifications
    }

    /// The exchange as one text block (the last lines), for a failure message.
    pub fn transcript(&self) -> String {
        let from = self.log.len().saturating_sub(12);
        self.log[from..].join("\n")
    }

    /// Closes stdin and kills the server if it does not end.
    pub fn close(mut self) {
        drop(std::mem::replace(
            &mut self.writer,
            Box::new(std::io::sink()),
        ));
        if let Some(mut c) = self.child.take() {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                if matches!(c.try_wait(), Ok(Some(_))) {
                    self.untrack();
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            sys::kill_tree(c.id());
            let _ = c.kill();
            let _ = c.wait();
        }
        self.untrack();
    }

    fn untrack(&mut self) {
        if let Some((t, pid)) = self.tracker.take() {
            if let Ok(mut v) = t.lock() {
                v.retain(|p| *p != pid);
            }
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            sys::kill_tree(c.id());
            let _ = c.kill();
            let _ = c.wait();
        }
        self.untrack();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    /// A scripted server on a loopback socket: answers initialize, tools/list (two pages), one tool call
    /// and sends a notification before the answer to `tools/call`.
    fn fake_server() -> McpClient {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (s, _) = listener.accept().unwrap();
            let mut w = s.try_clone().unwrap();
            for line in BufReader::new(s).lines() {
                let Ok(line) = line else { return };
                let v: Value = serde_json::from_str(&line).unwrap();
                let Some(id) = v.get("id").cloned() else {
                    continue;
                };
                let method = v["method"].as_str().unwrap_or("");
                let reply = match method {
                    "initialize" => {
                        json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"fake","version":"1"}}})
                    }
                    "tools/list" if v["params"].get("cursor").is_none() => {
                        json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{"name":"a"}],"nextCursor":"p2"}})
                    }
                    "tools/list" => {
                        json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{"name":"b"}]}})
                    }
                    "tools/call" => {
                        let note = json!({"jsonrpc":"2.0","method":"notifications/message","params":{"logger":"capturemann.events","data":{"topic":"job.done"}}});
                        writeln!(w, "{note}").unwrap();
                        match v["params"]["name"].as_str() {
                            Some("fail") => {
                                json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":"not_found: x"}],"structuredContent":{"code":"not_found"},"isError":true}})
                            }
                            Some("ok") => {
                                json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":"{\"a\":1}"}]}})
                            }
                            _ => {
                                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"unknown tool"}})
                            }
                        }
                    }
                    _ => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"nope"}}),
                };
                writeln!(w, "{reply}").unwrap();
            }
        });
        let s = TcpStream::connect(addr).unwrap();
        let r = s.try_clone().unwrap();
        let mut c = McpClient::from_streams(Box::new(s), Box::new(r), None);
        c.default_timeout = Duration::from_secs(5);
        c
    }

    #[test]
    fn handshake_paging_calls_and_notifications() {
        let mut c = fake_server();
        let init = c.initialize().unwrap();
        assert_eq!(init["serverInfo"]["name"], "fake");
        let tools = c.list_tools().unwrap();
        assert_eq!(tools.len(), 2);
        let ok = c.call("ok", json!({})).unwrap().unwrap();
        assert!(!ok.is_error);
        assert_eq!(ok.json().unwrap()["a"], 1);
        // The notification that arrived before the answer was kept.
        assert_eq!(c.notifications.len(), 1);
        let bad = c.call("fail", json!({})).unwrap().unwrap();
        assert!(bad.is_error);
        assert_eq!(bad.error_code().as_deref(), Some("not_found"));
        let rpc = c.call("zzz", json!({})).unwrap().unwrap_err();
        assert_eq!(rpc.0, -32602);
        assert!(c.transcript().contains("tools/call"));
        let r = c
            .request("nosuch", json!({}), Duration::from_secs(2))
            .unwrap();
        assert_eq!(r.error.unwrap().0, -32601);
    }

    #[test]
    fn a_silent_server_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (_s, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(3));
        });
        let s = TcpStream::connect(addr).unwrap();
        let r = s.try_clone().unwrap();
        let mut c = McpClient::from_streams(Box::new(s), Box::new(r), None);
        let e = c
            .request("initialize", json!({}), Duration::from_millis(300))
            .unwrap_err();
        assert!(e.contains("no answer"));
    }
}
