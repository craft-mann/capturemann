//! CLI errors and the mapping from shared error codes to exit codes (specs/README.md).

use cm_core::ErrorCode;
use serde_json::{json, Value};

/// Exit code for a failure outside the shared table (`method_not_found`, `invalid_request`, ...).
/// `not_implemented` is in the table and exits 1 as well (shared with `internal`).
pub const EXIT_FAILURE: i32 = 1;

/// Exit code for a shared error code string (`error.data.code`); [`EXIT_FAILURE`] when the
/// code is not in the shared table (`method_not_found`, ...).
pub fn exit_code_for(code: &str) -> i32 {
    ErrorCode::ALL
        .iter()
        .find(|c| c.as_str() == code)
        .map_or(EXIT_FAILURE, |c| c.exit_code())
}

/// A failed command: what is printed to stderr and the process exit code.
#[derive(Debug, Clone, PartialEq)]
pub struct CliError {
    /// Shared code (`invalid_input`, ...) or a CLI-local one.
    pub code: String,
    pub message: String,
    /// `error.data.data` of the daemon's error (e.g. `{"permission": "screen"}`).
    pub data: Value,
    pub exit: i32,
}

impl CliError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        CliError {
            code: code.as_str().to_owned(),
            message: message.into(),
            data: Value::Null,
            exit: code.exit_code(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// Local failure that has no shared code (exit 1).
    pub fn local(code: &str, message: impl Into<String>) -> Self {
        CliError {
            code: code.to_owned(),
            message: message.into(),
            data: Value::Null,
            exit: EXIT_FAILURE,
        }
    }

    /// Builds the error from a JSON-RPC `error` object. `error.data.code` decides the exit
    /// code; without it the numeric code is not trusted and the exit code is 1.
    pub fn from_rpc(error: &Value) -> Self {
        let payload = &error["data"];
        let code = payload["code"].as_str().unwrap_or("internal").to_owned();
        let message = payload["message"]
            .as_str()
            .or_else(|| error["message"].as_str())
            .unwrap_or("the daemon returned an error without a message")
            .to_owned();
        CliError {
            exit: exit_code_for(&code),
            code,
            message,
            data: payload["data"].clone(),
        }
    }

    /// Human-readable stderr text.
    pub fn render_human(&self) -> String {
        let mut s = format!("error[{}]: {}", self.code, self.message);
        if let Some(p) = self.data["permission"].as_str() {
            s.push_str(&format!(
                "\n  grant the {p} permission to CaptureMann in your system settings and retry"
            ));
        }
        if let Some(r) = self.data["reason"].as_str() {
            s.push_str(&format!("\n  reason: {r}"));
        }
        // REC-3: `invalid_input` for an unvalidated 60 fps carries `data.suggestion`.
        let sg = &self.data["suggestion"];
        if let Some(fps) = sg["fps"].as_u64() {
            s.push_str(&format!("\n  suggestion: record at {fps} fps"));
            if let Some(size) = sg["size"].as_str() {
                s.push_str(&format!(" or at {size}"));
            }
            if let Some(cli) = sg["run_selftest"]["cli"].as_str() {
                s.push_str(&format!(", or run `{cli}`"));
            }
        }
        s
    }

    /// Machine-readable stderr text.
    pub fn render_json(&self) -> String {
        json!({ "error": { "code": self.code, "message": self.message, "data": self.data,
                           "exit_code": self.exit } })
        .to_string()
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// README exit-code table (AGT-1: CLI exit codes per specs/README.md).
    #[test]
    fn every_shared_code_maps_per_the_readme_table() {
        let table = [
            ("invalid_input", 2),
            ("not_found", 2),
            ("user_declined", 3),
            ("confirmation_timeout", 3),
            ("permission_missing", 4),
            ("device_busy", 5),
            ("disk_full", 5),
            ("network", 5),
            ("platform_rejected", 5),
            ("auth_expired", 5),
            ("job_failed", 6),
            ("busy", 6),
            ("internal", 1),
            ("daemon_unavailable", 5),
            ("not_implemented", 1),
        ];
        assert_eq!(table.len(), ErrorCode::ALL.len());
        for (code, exit) in table {
            assert_eq!(exit_code_for(code), exit, "{code}");
        }
    }

    #[test]
    fn unknown_codes_exit_one() {
        for c in ["method_not_found", "invalid_request", "whatever"] {
            assert_eq!(exit_code_for(c), 1);
        }
    }

    #[test]
    fn rpc_error_uses_data_code_not_the_numeric_code() {
        let e = CliError::from_rpc(&json!({
            "code": -32004, "message": "m",
            "data": {"code": "permission_missing", "message": "Screen recording is off",
                     "data": {"permission": "screen"}},
        }));
        assert_eq!(e.exit, 4);
        assert_eq!(e.code, "permission_missing");
        assert!(e.render_human().contains("screen permission"));
        let v: Value = serde_json::from_str(&e.render_json()).unwrap();
        assert_eq!(v["error"]["exit_code"], 4);
    }

    #[test]
    fn suggestions_are_shown_for_an_unvalidated_60_fps() {
        let e = CliError::from_rpc(&json!({
            "code": -32602, "message": "m",
            "data": {"code": "invalid_input", "message": "60 fps isn't stable",
                     "data": {"suggestion": {"fps": 30, "size": "1440p",
                              "run_selftest": {"cli": "capturemann record selftest"}}}},
        }));
        assert_eq!(e.exit, 2);
        let h = e.render_human();
        assert!(
            h.contains(
                "suggestion: record at 30 fps or at 1440p, or run `capturemann record selftest`"
            ),
            "{h}"
        );
    }

    #[test]
    fn not_implemented_exits_one_like_internal() {
        let e = CliError::from_rpc(&json!({
            "code": -32099, "message": "m",
            "data": {"code": "not_implemented", "message": "later", "data": null},
        }));
        assert_eq!((e.exit, e.code.as_str()), (1, "not_implemented"));
        assert_eq!(
            exit_code_for("not_implemented"),
            exit_code_for("internal"),
            "documented: exit 1 is shared"
        );
    }

    #[test]
    fn rpc_error_without_payload_is_exit_one() {
        let e = CliError::from_rpc(&json!({"code": -32601, "message": "nope"}));
        assert_eq!((e.exit, e.message.as_str()), (1, "nope"));
    }
}
