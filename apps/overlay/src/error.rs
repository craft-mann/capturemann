//! Overlay errors: shared error codes (specs/README.md) and their exit codes.

use cm_core::ErrorCode;
use serde_json::{json, Value};

/// A failure to report: JSON on stderr and the process exit code.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayError {
    /// Shared code (`permission_missing`, ...).
    pub code: String,
    pub message: String,
    /// Extra detail, e.g. `{"permission": "screen"}`.
    pub data: Value,
    pub exit: i32,
}

/// Exit code for a code string; 1 for codes outside the shared table.
pub fn exit_code_for(code: &str) -> i32 {
    ErrorCode::ALL
        .iter()
        .find(|c| c.as_str() == code)
        .map_or(1, |c| c.exit_code())
}

impl OverlayError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        OverlayError {
            code: code.as_str().to_owned(),
            message: message.into(),
            data: Value::Null,
            exit: code.exit_code(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    /// The user closed the picker: exit 3 (`user_declined`), nothing was saved (CAP-1.AC6).
    pub fn cancelled() -> Self {
        Self::new(ErrorCode::UserDeclined, "cancelled: nothing was captured")
    }

    /// From a JSON-RPC `error` object; `error.data.code` decides the exit code.
    pub fn from_rpc(error: &Value) -> Self {
        let payload = &error["data"];
        let code = payload["code"].as_str().unwrap_or("internal").to_owned();
        let message = payload["message"]
            .as_str()
            .or_else(|| error["message"].as_str())
            .unwrap_or("the daemon returned an error without a message")
            .to_owned();
        OverlayError {
            exit: exit_code_for(&code),
            code,
            message,
            data: payload["data"].clone(),
        }
    }

    /// `{"error": {...}}` for stderr.
    pub fn render_json(&self) -> String {
        json!({ "error": { "code": self.code, "message": self.message, "data": self.data,
                           "exit_code": self.exit } })
        .to_string()
    }
}

impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for OverlayError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// README table: permission_missing is exit 4, cancel is 3, daemon_unavailable is 5.
    #[test]
    fn exit_codes_follow_the_shared_table() {
        assert_eq!(OverlayError::new(ErrorCode::PermissionMissing, "x").exit, 4);
        assert_eq!(OverlayError::cancelled().exit, 3);
        assert_eq!(OverlayError::new(ErrorCode::DaemonUnavailable, "x").exit, 5);
        assert_eq!(OverlayError::invalid_input("x").exit, 2);
        assert_eq!(exit_code_for("not_implemented"), 1);
    }

    #[test]
    fn rpc_permission_error_keeps_its_data() {
        let e = OverlayError::from_rpc(&json!({
            "code": -32004, "message": "m",
            "data": {"code": "permission_missing", "message": "Screen Recording is off",
                     "data": {"permission": "screen"}},
        }));
        assert_eq!(e.exit, 4);
        let v: Value = serde_json::from_str(&e.render_json()).unwrap();
        assert_eq!(v["error"]["data"]["permission"], "screen");
        assert_eq!(v["error"]["exit_code"], 4);
    }
}
