//! JSON-RPC 2.0 client over the daemon's IPC endpoint (one request per line).

use std::time::Duration;

use cm_ipc::{connect, ClientStream, Endpoint, RpcClient, RpcError};
use serde_json::{json, Value};

use cm_core::ErrorCode;

use crate::error::CliError;
use crate::launch::Launcher;

/// Name sent in `session.hello`; it becomes the `agent:capturemann-cli` actor in revisions.
pub const CLIENT_NAME: &str = "capturemann-cli";

/// How long to wait for a freshly started daemon to accept connections.
pub const START_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Client {
    rpc: RpcClient<ClientStream>,
}

fn unavailable(endpoint: &Endpoint, e: &std::io::Error) -> CliError {
    CliError::new(
        ErrorCode::DaemonUnavailable,
        format!(
            "cannot reach the CaptureMann daemon at {}: {e}. \
             Run `capturemann daemon start` or open the CaptureMann app",
            describe(endpoint)
        ),
    )
}

pub fn describe(endpoint: &Endpoint) -> String {
    endpoint.describe()
}

impl Client {
    /// Connects without starting anything.
    pub async fn connect_existing(endpoint: &Endpoint) -> Result<Client, CliError> {
        let stream = connect(endpoint)
            .await
            .map_err(|e| unavailable(endpoint, &e))?;
        let mut c = Client {
            rpc: RpcClient::new(stream),
        };
        c.call(
            "session.hello",
            json!({ "client": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") }),
        )
        .await?;
        Ok(c)
    }

    /// Connects; when nothing listens and `launcher` is given, starts a daemon and waits for
    /// it (AGT-1.AC2).
    pub async fn connect_or_start(
        endpoint: &Endpoint,
        launcher: Option<&Launcher>,
    ) -> Result<Client, CliError> {
        let first = match Client::connect_existing(endpoint).await {
            Ok(c) => return Ok(c),
            Err(e) if e.code == "daemon_unavailable" => e,
            Err(e) => return Err(e),
        };
        let Some(launcher) = launcher else {
            return Err(first);
        };
        launcher(endpoint).map_err(|e| {
            CliError::new(
                ErrorCode::DaemonUnavailable,
                format!("no daemon is running and starting one failed: {e}"),
            )
        })?;
        let deadline = tokio::time::Instant::now() + START_TIMEOUT;
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            match Client::connect_existing(endpoint).await {
                Ok(c) => return Ok(c),
                Err(e) if e.code == "daemon_unavailable" => {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(CliError::new(
                            ErrorCode::DaemonUnavailable,
                            format!(
                                "started a daemon but it did not accept connections at {} \
                                 within {} s",
                                describe(endpoint),
                                START_TIMEOUT.as_secs()
                            ),
                        ));
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Calls a method and waits for its result. A JSON-RPC error becomes a [`CliError`] whose
    /// exit code comes from `error.data.code`. Event notifications are kept aside and
    /// returned by [`Client::next_event`].
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, CliError> {
        self.rpc.call(method, params).await.map_err(rpc_error)
    }

    /// Next event notification (those that arrived during a call first).
    pub async fn next_event(&mut self) -> Result<Value, CliError> {
        self.rpc.next_event().await.map_err(rpc_error)
    }
}

fn rpc_error(e: RpcError) -> CliError {
    match e {
        RpcError::Rpc(err) => CliError::from_rpc(&err),
        RpcError::Io(Some(e)) => lost(e),
        RpcError::Io(None) => CliError::new(
            ErrorCode::DaemonUnavailable,
            "the daemon closed the connection",
        ),
        RpcError::InvalidJson(m) => CliError::new(
            ErrorCode::Internal,
            format!("the daemon sent invalid JSON: {m}"),
        ),
    }
}

fn lost(e: std::io::Error) -> CliError {
    CliError::new(
        ErrorCode::DaemonUnavailable,
        format!("lost the connection to the daemon: {e}"),
    )
}
