//! Client side of the CaptureMann daemon's local IPC (specs/02-agent-interface.md).
//!
//! A deliberately small crate: it names the daemon's [`Endpoint`], connects to it
//! ([`connect`]) and speaks the line-delimited JSON-RPC 2.0 protocol ([`RpcClient`]). It has
//! no engine dependency (no capture, library or render crate), so GUI helpers such as the
//! overlay and the CLI/MCP clients link only this. `cm-daemon` depends on it for the server
//! half and re-exports the same names, so `cm_daemon::{Endpoint, connect}` keep working.
//!
//! Default endpoints:
//!
//! * macOS: `~/Library/Application Support/CaptureMann/daemon.sock`
//! * Windows: `\\.\pipe\capturemann`
//! * other Unix (development only): `$XDG_RUNTIME_DIR/capturemann/daemon.sock`, falling back
//!   to `~/.local/share/capturemann/daemon.sock`
//!
//! Unix socket paths are limited (104 bytes on macOS and the BSDs, 108 on Linux, including the
//! terminating NUL). [`Endpoint::default_for_platform`] falls back to a short per-user
//! directory (`/tmp/cm-<uid>/daemon.sock`, mode 0700) when the preferred path is too long; an
//! explicit path that is too long is an `invalid_input` error ([`Endpoint::validate`]).

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod client;
mod endpoint;

pub use client::{RpcClient, RpcError, MAX_LINE_BYTES};
#[cfg(unix)]
pub use endpoint::with_private_umask;
pub use endpoint::{connect, ClientStream, Endpoint, MAX_SOCKET_PATH_BYTES};
