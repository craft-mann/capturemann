//! MCP server (stdio and local HTTP): a thin bridge to `cm-daemon`.
//!
//! The server never calls engine crates. Each MCP tool maps to one daemon action over the
//! daemon's JSON-RPC IPC (AGT-1): the tool name is the JSON-RPC method, the arguments are the
//! params, and the daemon's `error.data` (`{ code, message, data }`) becomes the MCP tool
//! error's `structuredContent`, so agents read `code` (`user_declined`, `not_found`, ...).
//! The MCP client's name (`clientInfo.name`) is sent in `session.hello`, which makes revisions
//! `agent:<name>` and keys the daemon's per-client policy and confirmation dialog.
//!
//! * [`McpServer`]: the `rmcp` `ServerHandler`.
//! * [`events`]: daemon events forwarded as MCP notifications.
//! * [`tools`]: the tool table with JSON Schema for input and output.
//! * [`conn`]: the daemon JSON-RPC client ([`DaemonConn`], [`Connector`]).
//! * [`serve_stdio`], [`serve_duplex`]: transports. See [`http`] for local HTTP.
//!
//! * [`resources`] and [`prompts`]: read-only MCP resources (each read is one Free daemon
//!   action) and task prompt templates.
//!
//! Not yet available (nothing behind them in the daemon): the cursor resource and every tool
//! the daemon does not implement.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod conn;
pub mod events;
pub mod http;
pub mod prompts;
pub mod resources;
pub mod server;
pub mod tools;

pub use conn::{endpoint_connector, ConnError, Connector, DaemonConn, RpcError};
pub use http::{generate_token, serve_http, HttpOptions, HttpServer};
pub use server::{sanitize_client_name, success_result, McpServer};

use rmcp::service::ServiceExt;

/// Serves MCP over this process's stdin/stdout until the client disconnects.
pub async fn serve_stdio(
    connector: Connector,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let running = McpServer::new(connector)
        .serve(rmcp::transport::stdio())
        .await?;
    running.waiting().await?;
    Ok(())
}
