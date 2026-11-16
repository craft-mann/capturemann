//! Errors returned by the daemon and their JSON-RPC mapping.
//!
//! # JSON-RPC error codes
//!
//! Every error response is `{"code": <number>, "message": <text>, "data": <payload>}` where
//! `data` is the shared payload `{ "code": "<shared code>", "message": ..., "data": ... }` from
//! `cm_core::CmError::to_payload` (so MCP and JSON-RPC clients read `error.data.code`, as the
//! specs require).
//!
//! | JSON-RPC `code` | Meaning |
//! | --- | --- |
//! | -32700 | Parse error (line is not JSON) |
//! | -32600 | Invalid request (not a JSON-RPC 2.0 object, line too long) |
//! | -32601 | Method not found (unknown name, or a Never-tier operation) |
//! | -32602 | Invalid params; also `invalid_input` |
//! | -32603 | Internal error (a worker panicked, a lock was poisoned) |
//! | -32001 ..= -32013 | Application errors, see [`rpc_code_for`] |
//! | -32012 | `internal` (unexpected I/O, database or other internal failure; exit 1) |
//! | -32013 | `daemon_unavailable` (normally produced client-side; mapped for completeness) |
//! | -32099 | `not_implemented` (a shared code, exit 1): the action exists but no engine is wired to it yet |
//!
//! `user_declined` caused by a per-client policy carries `data.data.reason` of
//! `client_read_only` or `client_blocked` (AGT-2.AC4).

use cm_core::{CmError, ErrorCode};
use serde_json::{json, Value};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const NOT_IMPLEMENTED: i64 = -32099;

/// JSON-RPC code of a shared error code. `invalid_input` is the standard -32602; the rest are
/// application errors in the reserved -32000..-32099 range, numbered in the order of the table
/// in `specs/README.md`.
pub fn rpc_code_for(code: ErrorCode) -> i64 {
    match code {
        ErrorCode::InvalidInput => INVALID_PARAMS,
        ErrorCode::NotFound => -32001,
        ErrorCode::UserDeclined => -32002,
        ErrorCode::ConfirmationTimeout => -32003,
        ErrorCode::PermissionMissing => -32004,
        ErrorCode::DeviceBusy => -32005,
        ErrorCode::DiskFull => -32006,
        ErrorCode::Network => -32007,
        ErrorCode::PlatformRejected => -32008,
        ErrorCode::AuthExpired => -32009,
        ErrorCode::JobFailed => -32010,
        ErrorCode::Busy => -32011,
        ErrorCode::Internal => -32012,
        ErrorCode::DaemonUnavailable => -32013,
        ErrorCode::NotImplemented => NOT_IMPLEMENTED,
    }
}

/// Why a call did not produce a result.
#[derive(Debug, Clone, PartialEq)]
pub enum DaemonError {
    /// A shared-code error from an engine or from validation.
    Cm(CmError),
    /// `user_declined` with a machine-readable reason (`client_read_only`, `client_blocked`).
    Declined {
        reason: &'static str,
        message: String,
    },
    /// A shared-code error that carries code-specific `data` (for example `invalid_input` with
    /// `data.suggestion` when 60 fps is not validated, REC-3). The payload is
    /// `{ code, message, data }` with this `data`.
    WithData { error: CmError, data: Value },
    /// The action is known but no engine implements it in this build. Carries the shared
    /// `not_implemented` code (same as `DaemonError::Cm(CmError::NotImplemented { .. })`).
    NotImplemented(String),
    /// No such method (or a Never-tier operation, which is not exposed at all).
    MethodNotFound(String),
    /// Malformed JSON-RPC envelope.
    InvalidRequest(String),
    /// Unexpected failure inside the daemon (panic in a worker, poisoned lock).
    Internal(String),
}

impl From<CmError> for DaemonError {
    fn from(e: CmError) -> Self {
        DaemonError::Cm(e)
    }
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonError::Cm(e) | DaemonError::WithData { error: e, .. } => write!(f, "{e}"),
            DaemonError::Declined { message, .. } => write!(f, "{message}"),
            DaemonError::NotImplemented(m)
            | DaemonError::MethodNotFound(m)
            | DaemonError::InvalidRequest(m)
            | DaemonError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for DaemonError {}

impl DaemonError {
    /// The shared error code, when there is one.
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            DaemonError::Cm(e) | DaemonError::WithData { error: e, .. } => Some(e.code()),
            DaemonError::Declined { .. } => Some(ErrorCode::UserDeclined),
            DaemonError::NotImplemented(_) => Some(ErrorCode::NotImplemented),
            _ => None,
        }
    }

    /// Numeric JSON-RPC code.
    pub fn rpc_code(&self) -> i64 {
        match self {
            DaemonError::Cm(e) | DaemonError::WithData { error: e, .. } => rpc_code_for(e.code()),
            DaemonError::Declined { .. } => rpc_code_for(ErrorCode::UserDeclined),
            DaemonError::NotImplemented(_) => rpc_code_for(ErrorCode::NotImplemented),
            DaemonError::MethodNotFound(_) => METHOD_NOT_FOUND,
            DaemonError::InvalidRequest(_) => INVALID_REQUEST,
            DaemonError::Internal(_) => INTERNAL_ERROR,
        }
    }

    /// The payload placed in the JSON-RPC `error.data`.
    pub fn payload(&self) -> Value {
        match self {
            DaemonError::Cm(e) => serde_json::to_value(e.to_payload()).unwrap_or(Value::Null),
            DaemonError::WithData { error, data } => json!({
                "code": error.code().as_str(),
                "message": error.to_string(),
                "data": data,
            }),
            DaemonError::Declined { reason, message } => json!({
                "code": ErrorCode::UserDeclined.as_str(),
                "message": message,
                "data": { "reason": reason },
            }),
            DaemonError::NotImplemented(m) => {
                serde_json::to_value(CmError::not_implemented(m.clone()).to_payload())
                    .unwrap_or(Value::Null)
            }
            DaemonError::MethodNotFound(m) => {
                json!({ "code": "method_not_found", "message": m, "data": null })
            }
            DaemonError::InvalidRequest(m) => {
                json!({ "code": "invalid_request", "message": m, "data": null })
            }
            DaemonError::Internal(m) => {
                json!({ "code": "internal", "message": m, "data": null })
            }
        }
    }

    /// The whole JSON-RPC `error` object.
    pub fn to_rpc_error(&self) -> Value {
        json!({ "code": self.rpc_code(), "message": self.to_string(), "data": self.payload() })
    }

    /// CLI exit code (specs/README.md); 1 for errors outside the shared table.
    pub fn exit_code(&self) -> i32 {
        self.code().map_or(1, ErrorCode::exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_codes_are_unique_and_in_range() {
        let mut seen = std::collections::HashSet::new();
        for c in ErrorCode::ALL {
            let n = rpc_code_for(c);
            assert!(seen.insert(n), "duplicate {n}");
            if c != ErrorCode::InvalidInput {
                assert!((-32099..=-32000).contains(&n));
            }
        }
        assert_eq!(rpc_code_for(ErrorCode::InvalidInput), -32602);
    }

    #[test]
    fn payload_carries_shared_code() {
        let e: DaemonError = CmError::permission_missing(cm_core::Permission::Screen, "x").into();
        let v = e.to_rpc_error();
        assert_eq!(v["code"], -32004);
        assert_eq!(v["data"]["code"], "permission_missing");
        assert_eq!(v["data"]["data"]["permission"], "screen");
        assert_eq!(e.exit_code(), 4);
    }

    #[test]
    fn new_codes_map_to_rpc_and_exit_codes() {
        let e: DaemonError = CmError::internal("db locked").into();
        let v = e.to_rpc_error();
        assert_eq!((v["code"].clone(), e.exit_code()), (json!(-32012), 1));
        assert_eq!(v["data"]["code"], "internal");
        let e: DaemonError = CmError::daemon_unavailable("gone").into();
        assert_eq!(e.rpc_code(), -32013);
        assert_eq!(e.payload()["code"], "daemon_unavailable");
        assert_eq!(e.exit_code(), 5);
    }

    #[test]
    fn not_implemented_is_a_shared_code_with_the_same_number() {
        assert_eq!(rpc_code_for(ErrorCode::NotImplemented), -32099);
        let a = DaemonError::NotImplemented("later".into());
        let b: DaemonError = CmError::not_implemented("later").into();
        for e in [a, b] {
            assert_eq!(e.rpc_code(), -32099);
            assert_eq!(e.exit_code(), 1);
            assert_eq!(e.code(), Some(ErrorCode::NotImplemented));
            assert_eq!(
                e.payload(),
                json!({"code":"not_implemented","message":"later","data":null})
            );
        }
    }

    #[test]
    fn with_data_keeps_the_shared_code_and_carries_data() {
        let e = DaemonError::WithData {
            error: CmError::invalid_input("60 fps is not validated"),
            data: json!({"suggestion": {"fps": 30}}),
        };
        assert_eq!(e.rpc_code(), -32602);
        assert_eq!(e.exit_code(), 2);
        let p = e.payload();
        assert_eq!(p["code"], "invalid_input");
        assert_eq!(p["data"]["suggestion"]["fps"], 30);
        assert_eq!(e.to_string(), "60 fps is not validated");
    }

    #[test]
    fn declined_has_reason() {
        let e = DaemonError::Declined {
            reason: "client_read_only",
            message: "m".into(),
        };
        assert_eq!(e.payload()["data"]["reason"], "client_read_only");
        assert_eq!(e.exit_code(), 3);
    }
}
