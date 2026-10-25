//! Minimal JSON-RPC 2.0 client of the daemon (one request per line), as used by `cm-cli`.
//!
//! The overlay is a client like any other: it only speaks the public methods (`list_targets`,
//! `capture_screenshot`) and never links an engine crate.

use cm_core::ErrorCode;
use cm_ipc::{connect, ClientStream, Endpoint, RpcClient, RpcError};
use serde_json::{json, Value};

use crate::error::OverlayError;

/// Name sent in `session.hello`.
pub const CLIENT_NAME: &str = "capturemann-overlay";

pub struct Client {
    rpc: RpcClient<ClientStream>,
}

fn lost(e: std::io::Error) -> OverlayError {
    OverlayError::new(
        ErrorCode::DaemonUnavailable,
        format!("lost the connection to the daemon: {e}"),
    )
}

fn rpc_error(e: RpcError) -> OverlayError {
    match e {
        RpcError::Rpc(err) => OverlayError::from_rpc(&err),
        RpcError::Io(Some(e)) => lost(e),
        RpcError::Io(None) => OverlayError::new(
            ErrorCode::DaemonUnavailable,
            "the daemon closed the connection",
        ),
        RpcError::InvalidJson(m) => {
            OverlayError::internal(format!("the daemon sent invalid JSON: {m}"))
        }
    }
}

impl Client {
    /// Connects and says hello. The overlay never starts a daemon: the desktop app embeds one,
    /// or run `capturemann daemon start`.
    pub async fn connect(endpoint: &Endpoint) -> Result<Client, OverlayError> {
        let stream = connect(endpoint).await.map_err(|e| {
            OverlayError::new(
                ErrorCode::DaemonUnavailable,
                format!(
                    "cannot reach the CaptureMann daemon at {}: {e}. \
                     Start the CaptureMann app or `capturemann daemon start`",
                    endpoint.describe()
                ),
            )
        })?;
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

    /// Calls a method and waits for its result; event notifications are skipped. A JSON-RPC
    /// error becomes an [`OverlayError`] whose exit code comes from `error.data.code`.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, OverlayError> {
        self.rpc.call(method, params).await.map_err(rpc_error)
    }
}
