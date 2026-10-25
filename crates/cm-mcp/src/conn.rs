//! The client side of the daemon's JSON-RPC protocol (one request per line).
//!
//! A [`DaemonConn`] multiplexes concurrent requests over one stream by `id`, names itself with
//! `session.hello`, and reports a dead stream as [`ConnError::Closed`] so the caller can
//! reconnect (AGT-1 edge case: the next call after a daemon crash reconnects).
//!
//! # Cancellation (R2-14)
//!
//! If the future of [`DaemonConn::call`] is dropped before the response arrives (the MCP client
//! sent `notifications/cancelled`, or its connection went away), the connection sends the
//! daemon `rpc.cancel { id }` so the request stops there too: a confirmation dialog the
//! request was waiting on closes instead of lingering for five minutes.
//!
//! # Events
//!
//! The daemon's event notifications (`method: "event"`) are delivered to the receiver returned
//! by [`DaemonConn::subscribe_events`] (the MCP server turns them into MCP notifications, see
//! `server`). The queue is bounded ([`EVENT_QUEUE`]); when the consumer is slow the reader drops
//! new events, counts them, and inserts one `events.dropped` event with the count as soon as
//! there is room again. Without a subscription, events are discarded.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

/// Events buffered between the daemon reader and the MCP forwarder.
pub const EVENT_QUEUE: usize = 256;

/// A boxed, type-erased pair of daemon stream halves.
pub type BoxedRead = Box<dyn AsyncRead + Send + Unpin>;
/// See [`BoxedRead`].
pub type BoxedWrite = Box<dyn AsyncWrite + Send + Unpin>;

/// Opens a fresh stream to the daemon. Called again after the connection dies.
pub type Connector = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = std::io::Result<(BoxedRead, BoxedWrite)>> + Send>>
        + Send
        + Sync,
>;

/// A connector for a daemon endpoint (Unix socket or Windows named pipe).
pub fn endpoint_connector(endpoint: cm_ipc::Endpoint) -> Connector {
    Arc::new(move || {
        let endpoint = endpoint.clone();
        Box::pin(async move {
            let stream = cm_ipc::connect(&endpoint).await?;
            let (r, w) = tokio::io::split(stream);
            Ok((Box::new(r) as BoxedRead, Box::new(w) as BoxedWrite))
        })
    })
}

/// Why a request produced no JSON-RPC response.
#[derive(Debug)]
pub enum ConnError {
    /// The stream is gone (daemon stopped or crashed).
    Closed,
    /// Could not write the request or read the response.
    Io(std::io::Error),
    /// The daemon answered with something that is not a valid response.
    Protocol(String),
}

impl std::fmt::Display for ConnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnError::Closed => write!(f, "the CaptureMann daemon closed the connection"),
            ConnError::Io(e) => write!(f, "cannot talk to the CaptureMann daemon: {e}"),
            ConnError::Protocol(m) => write!(f, "unexpected reply from the daemon: {m}"),
        }
    }
}

impl std::error::Error for ConnError {}

/// The `error` object of a JSON-RPC error response.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    /// The shared payload `{ code, message, data }` (`error.data`); `Null` if absent.
    pub data: Value,
}

impl RpcError {
    /// The shared error code (`error.data.code`), e.g. `user_declined`.
    pub fn kind(&self) -> Option<&str> {
        self.data.get("code").and_then(Value::as_str)
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;
type EventSink = Arc<Mutex<Option<mpsc::Sender<Value>>>>;

/// Hands one event (`params` of a daemon `event` notification) to the subscriber without ever
/// blocking the reader; see the module docs for the drop policy.
fn deliver_event(tx: &mpsc::Sender<Value>, dropped: &mut u64, params: Value) {
    if *dropped > 0 {
        let note = json!({ "event": "events.dropped", "data": { "dropped": *dropped } });
        if tx.try_send(note).is_ok() {
            *dropped = 0;
        }
    }
    if tx.try_send(params).is_err() {
        *dropped += 1;
    }
}

/// One live connection to the daemon.
pub struct DaemonConn {
    writer: Arc<tokio::sync::Mutex<BoxedWrite>>,
    pending: Pending,
    next: AtomicU64,
    closed: Arc<AtomicBool>,
    events: EventSink,
}

impl DaemonConn {
    /// Wraps an open stream and starts the reader task.
    pub fn new(reader: BoxedRead, writer: BoxedWrite) -> Arc<DaemonConn> {
        let pending: Pending = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        let events: EventSink = Arc::default();
        let sink = events.clone();
        let conn = Arc::new(DaemonConn {
            writer: Arc::new(tokio::sync::Mutex::new(writer)),
            pending: pending.clone(),
            next: AtomicU64::new(1),
            closed: closed.clone(),
            events,
        });
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            let mut dropped = 0u64;
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if v.get("method").and_then(Value::as_str) == Some("event") {
                    let tx = sink.lock().unwrap_or_else(PoisonError::into_inner).clone();
                    if let (Some(tx), Some(params)) = (tx, v.get("params").cloned()) {
                        deliver_event(&tx, &mut dropped, params);
                    }
                    continue;
                }
                let Some(id) = v.get("id").and_then(Value::as_u64) else {
                    continue;
                };
                let tx = pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&id);
                if let Some(tx) = tx {
                    let _ = tx.send(v);
                }
            }
            closed.store(true, Ordering::SeqCst);
            // Ends the event stream.
            sink.lock().unwrap_or_else(PoisonError::into_inner).take();
            // Dropping the senders wakes every waiter with `Closed`.
            pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
        });
        conn
    }

    /// Asks the daemon for events on `topics` and returns the stream of their `params`
    /// (`{ seq, event, at, data }`). The stream ends when the connection does. The daemon's
    /// per-client policy applies: a blocked client gets the RPC error back and no stream.
    /// Calling it again replaces the previous stream.
    pub async fn subscribe_events(
        &self,
        topics: &[&str],
    ) -> Result<Result<mpsc::Receiver<Value>, RpcError>, ConnError> {
        let (tx, rx) = mpsc::channel(EVENT_QUEUE);
        *self.events.lock().unwrap_or_else(PoisonError::into_inner) = Some(tx);
        match self
            .call("events.subscribe", json!({ "topics": topics }))
            .await
        {
            Ok(Ok(_)) => Ok(Ok(rx)),
            Ok(Err(e)) => {
                self.events
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                Ok(Err(e))
            }
            Err(e) => Err(e),
        }
    }

    /// Whether the stream has ended.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Sends one request and waits for its response. The outer `Result` is transport
    /// failure; the inner one is a JSON-RPC error response.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Result<Value, RpcError>, ConnError> {
        if self.is_closed() {
            return Err(ConnError::Closed);
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        let mut line =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        {
            let mut w = self.writer.lock().await;
            let sent = match w.write_all(line.as_bytes()).await {
                Ok(()) => w.flush().await,
                Err(e) => Err(e),
            };
            if let Err(e) = sent {
                self.pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&id);
                self.closed.store(true, Ordering::SeqCst);
                return Err(ConnError::Io(e));
            }
        }
        // From here on, dropping this future cancels the request at the daemon.
        let mut cancel_guard = CancelOnDrop {
            id,
            writer: self.writer.clone(),
            pending: self.pending.clone(),
            armed: true,
        };
        let resp = rx.await.map_err(|_| ConnError::Closed)?;
        cancel_guard.armed = false;
        if let Some(err) = resp.get("error") {
            return Ok(Err(RpcError {
                code: err.get("code").and_then(Value::as_i64).unwrap_or(-32603),
                message: err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_owned(),
                data: err.get("data").cloned().unwrap_or(Value::Null),
            }));
        }
        match resp.get("result") {
            Some(r) => Ok(Ok(r.clone())),
            None => Err(ConnError::Protocol("response has no result".to_owned())),
        }
    }
}

/// Sends `rpc.cancel` for a request whose caller gave up.
struct CancelOnDrop {
    id: u64,
    writer: Arc<tokio::sync::Mutex<BoxedWrite>>,
    pending: Pending,
    armed: bool,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let writer = self.writer.clone();
        let mut line = json!({ "jsonrpc": "2.0", "method": "rpc.cancel",
                               "params": { "id": self.id } })
        .to_string();
        line.push('\n');
        handle.spawn(async move {
            let mut w = writer.lock().await;
            if w.write_all(line.as_bytes()).await.is_ok() {
                let _ = w.flush().await;
            }
        });
    }
}

impl Drop for DaemonConn {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    /// AGT-1: a consumer that falls behind loses events, never blocks the reader, and is told
    /// how many with one `events.dropped` as soon as there is room again.
    #[tokio::test]
    async fn agt_1_slow_event_consumer_gets_events_dropped() {
        let (client, server) = tokio::io::duplex(1 << 22);
        let (cr, cw) = tokio::io::split(client);
        let (sr, mut sw) = tokio::io::split(server);
        let conn = DaemonConn::new(Box::new(cr), Box::new(cw));
        // A scripted daemon: answer `events.subscribe` (request id 1), then flood.
        tokio::spawn(async move {
            let mut lines = BufReader::new(sr).lines();
            let _req = lines.next_line().await.unwrap();
            sw.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n")
                .await
                .unwrap();
            let total = EVENT_QUEUE + 50;
            for seq in 0..total {
                let line = json!({ "jsonrpc": "2.0", "method": "event",
                    "params": { "seq": seq, "event": "job.progress", "data": {} } });
                sw.write_all(format!("{line}\n").as_bytes()).await.unwrap();
            }
            // After the consumer drains, one more event.
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let line = json!({ "jsonrpc": "2.0", "method": "event",
                "params": { "seq": total, "event": "job.done", "data": {} } });
            sw.write_all(format!("{line}\n").as_bytes()).await.unwrap();
            sw.flush().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        });
        let mut rx = conn.subscribe_events(&["*"]).await.unwrap().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        for seq in 0..EVENT_QUEUE {
            let ev = rx.try_recv().unwrap();
            assert_eq!(ev["seq"], seq, "events keep their order");
        }
        assert!(rx.try_recv().is_err(), "the overflow was dropped");
        let note = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(note["event"], "events.dropped");
        assert_eq!(note["data"]["dropped"], 50);
        let next = rx.recv().await.unwrap();
        assert_eq!(next["event"], "job.done");
    }
}
