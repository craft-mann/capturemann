//! A minimal JSON-RPC 2.0 client: one request per line, one response per line, event
//! notifications set aside.

use std::io;

use serde_json::{json, Value};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf, WriteHalf,
};

use crate::endpoint::ClientStream;

/// Why a call produced no result.
#[derive(Debug)]
pub enum RpcError {
    /// The stream failed or ended (the daemon stopped); `None` when it closed cleanly.
    Io(Option<io::Error>),
    /// The daemon sent a line that is not JSON.
    InvalidJson(String),
    /// The daemon answered with a JSON-RPC `error` object (`error.data.code` is the shared
    /// error code, see `specs/README.md`).
    Rpc(Value),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::Io(Some(e)) => write!(f, "lost the connection to the daemon: {e}"),
            RpcError::Io(None) => write!(f, "the daemon closed the connection"),
            RpcError::InvalidJson(e) => write!(f, "the daemon sent invalid JSON: {e}"),
            RpcError::Rpc(v) => write!(f, "{v}"),
        }
    }
}

impl std::error::Error for RpcError {}

/// A sequential client over any byte stream (the daemon's [`ClientStream`] by default). Calls
/// run one at a time; open several connections for concurrency.
pub struct RpcClient<S = ClientStream> {
    reader: BufReader<ReadHalf<S>>,
    writer: WriteHalf<S>,
    next_id: u64,
    /// Event notifications (`params` of `{"method":"event"}`) that arrived while waiting.
    pub pending_events: Vec<Value>,
}

impl<S: AsyncRead + AsyncWrite> RpcClient<S> {
    pub fn new(stream: S) -> Self {
        let (r, w) = tokio::io::split(stream);
        RpcClient {
            reader: BufReader::new(r),
            writer: w,
            next_id: 1,
            pending_events: Vec::new(),
        }
    }

    async fn send(&mut self, method: &str, params: Value) -> Result<u64, RpcError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut line =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| RpcError::Io(Some(e)))?;
        self.writer
            .flush()
            .await
            .map_err(|e| RpcError::Io(Some(e)))?;
        Ok(id)
    }

    /// The next JSON message from the daemon (responses and notifications alike).
    pub async fn read_message(&mut self) -> Result<Value, RpcError> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| RpcError::Io(Some(e)))?;
            if n == 0 {
                return Err(RpcError::Io(None));
            }
            if line.trim().is_empty() {
                continue;
            }
            return serde_json::from_str(line.trim())
                .map_err(|e| RpcError::InvalidJson(e.to_string()));
        }
    }

    /// Calls `method` and waits for its result. Event notifications that arrive meanwhile are
    /// kept in [`RpcClient::pending_events`].
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.send(method, params).await?;
        loop {
            let msg = self.read_message().await?;
            if msg.get("method") == Some(&json!("event")) {
                self.pending_events.push(msg["params"].clone());
                continue;
            }
            if msg["id"] != json!(id) {
                continue;
            }
            if let Some(err) = msg.get("error") {
                return Err(RpcError::Rpc(err.clone()));
            }
            return Ok(msg["result"].clone());
        }
    }

    /// Next event notification (queued ones first).
    pub async fn next_event(&mut self) -> Result<Value, RpcError> {
        if !self.pending_events.is_empty() {
            return Ok(self.pending_events.remove(0));
        }
        loop {
            let msg = self.read_message().await?;
            if msg.get("method") == Some(&json!("event")) {
                return Ok(msg["params"].clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn call_skips_events_and_maps_errors() {
        let (a, b) = tokio::io::duplex(4096);
        let server = tokio::spawn(async move {
            let (r, mut w) = tokio::io::split(b);
            let mut r = BufReader::new(r);
            let mut line = String::new();
            for reply in [
                r#"{"jsonrpc":"2.0","method":"event","params":{"event":"x"}}
{"jsonrpc":"2.0","id":9,"result":"other"}
{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#,
                r#"{"jsonrpc":"2.0","id":2,"error":{"code":-1,"message":"m","data":{"code":"not_found"}}}"#,
            ] {
                line.clear();
                r.read_line(&mut line).await.unwrap();
                w.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
            }
        });
        let mut c = RpcClient::new(a);
        assert_eq!(c.call("m", json!({})).await.unwrap(), json!({"ok": true}));
        assert_eq!(c.pending_events, vec![json!({"event": "x"})]);
        match c.call("m", json!({})).await {
            Err(RpcError::Rpc(v)) => assert_eq!(v["data"]["code"], "not_found"),
            other => panic!("{other:?}"),
        }
        server.await.unwrap();
        assert!(matches!(c.call("m", json!({})).await, Err(RpcError::Io(_))));
    }
}
