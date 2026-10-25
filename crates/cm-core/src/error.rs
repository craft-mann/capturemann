//! Shared error codes (specs/README.md "Shared error codes").

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

/// OS permission named in `permission_missing` errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Screen,
    Camera,
    Microphone,
    Accessibility,
    InputMonitoring,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::Screen => "screen",
            Permission::Camera => "camera",
            Permission::Microphone => "microphone",
            Permission::Accessibility => "accessibility",
            Permission::InputMonitoring => "input_monitoring",
        }
    }
}

/// Machine-readable error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    NotFound,
    UserDeclined,
    ConfirmationTimeout,
    PermissionMissing,
    DeviceBusy,
    DiskFull,
    Network,
    PlatformRejected,
    AuthExpired,
    JobFailed,
    Busy,
    Internal,
    DaemonUnavailable,
    NotImplemented,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 15] = [
        ErrorCode::InvalidInput,
        ErrorCode::NotFound,
        ErrorCode::UserDeclined,
        ErrorCode::ConfirmationTimeout,
        ErrorCode::PermissionMissing,
        ErrorCode::DeviceBusy,
        ErrorCode::DiskFull,
        ErrorCode::Network,
        ErrorCode::PlatformRejected,
        ErrorCode::AuthExpired,
        ErrorCode::JobFailed,
        ErrorCode::Busy,
        ErrorCode::Internal,
        ErrorCode::DaemonUnavailable,
        ErrorCode::NotImplemented,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidInput => "invalid_input",
            ErrorCode::NotFound => "not_found",
            ErrorCode::UserDeclined => "user_declined",
            ErrorCode::ConfirmationTimeout => "confirmation_timeout",
            ErrorCode::PermissionMissing => "permission_missing",
            ErrorCode::DeviceBusy => "device_busy",
            ErrorCode::DiskFull => "disk_full",
            ErrorCode::Network => "network",
            ErrorCode::PlatformRejected => "platform_rejected",
            ErrorCode::AuthExpired => "auth_expired",
            ErrorCode::JobFailed => "job_failed",
            ErrorCode::Busy => "busy",
            ErrorCode::Internal => "internal",
            ErrorCode::DaemonUnavailable => "daemon_unavailable",
            ErrorCode::NotImplemented => "not_implemented",
        }
    }

    /// CLI exit code. `not_implemented` exits 1, deliberately shared with `internal`.
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorCode::Internal | ErrorCode::NotImplemented => 1,
            ErrorCode::InvalidInput | ErrorCode::NotFound => 2,
            ErrorCode::UserDeclined | ErrorCode::ConfirmationTimeout => 3,
            ErrorCode::PermissionMissing => 4,
            ErrorCode::DeviceBusy
            | ErrorCode::DiskFull
            | ErrorCode::Network
            | ErrorCode::PlatformRejected
            | ErrorCode::AuthExpired
            | ErrorCode::DaemonUnavailable => 5,
            ErrorCode::JobFailed | ErrorCode::Busy => 6,
        }
    }
}

/// The shared error type. `Display` yields the user-facing message.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CmError {
    #[error("{message}")]
    InvalidInput { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    UserDeclined { message: String },
    #[error("{message}")]
    ConfirmationTimeout { message: String },
    #[error("{message}")]
    PermissionMissing {
        permission: Permission,
        message: String,
    },
    #[error("{message}")]
    DeviceBusy { message: String },
    #[error("{message}")]
    DiskFull { message: String },
    #[error("{message}")]
    Network { message: String },
    #[error("{message}")]
    PlatformRejected {
        message: String,
        platform_code: Option<String>,
    },
    #[error("{message}")]
    AuthExpired { message: String },
    #[error("{message}")]
    JobFailed {
        job_id: String,
        reason: String,
        message: String,
    },
    #[error("{message}")]
    Busy { message: String },
    #[error("{message}")]
    Internal { message: String },
    #[error("{message}")]
    DaemonUnavailable { message: String },
    #[error("{message}")]
    NotImplemented { message: String },
}

/// Serializable `{code, message, data}` form for MCP (`error.data`) and JSON-RPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub message: String,
    pub data: Option<Value>,
}

impl CmError {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        CmError::InvalidInput {
            message: message.into(),
        }
    }

    /// `invalid_input` whose message names the offending field.
    pub fn invalid_field(field: &str, message: impl std::fmt::Display) -> Self {
        CmError::InvalidInput {
            message: format!("{field}: {message}"),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        CmError::NotFound {
            message: message.into(),
        }
    }

    pub fn user_declined(message: impl Into<String>) -> Self {
        CmError::UserDeclined {
            message: message.into(),
        }
    }

    pub fn confirmation_timeout(message: impl Into<String>) -> Self {
        CmError::ConfirmationTimeout {
            message: message.into(),
        }
    }

    pub fn permission_missing(permission: Permission, message: impl Into<String>) -> Self {
        CmError::PermissionMissing {
            permission,
            message: message.into(),
        }
    }

    pub fn device_busy(message: impl Into<String>) -> Self {
        CmError::DeviceBusy {
            message: message.into(),
        }
    }

    pub fn disk_full(message: impl Into<String>) -> Self {
        CmError::DiskFull {
            message: message.into(),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        CmError::Network {
            message: message.into(),
        }
    }

    pub fn platform_rejected(message: impl Into<String>, platform_code: Option<String>) -> Self {
        CmError::PlatformRejected {
            message: message.into(),
            platform_code,
        }
    }

    pub fn auth_expired(message: impl Into<String>) -> Self {
        CmError::AuthExpired {
            message: message.into(),
        }
    }

    pub fn job_failed(job_id: impl Into<String>, reason: impl Into<String>) -> Self {
        let (job_id, reason) = (job_id.into(), reason.into());
        CmError::JobFailed {
            message: format!("job {job_id} failed: {reason}"),
            job_id,
            reason,
        }
    }

    pub fn busy(message: impl Into<String>) -> Self {
        CmError::Busy {
            message: message.into(),
        }
    }

    /// Unexpected internal, I/O or database failure not covered by another code.
    pub fn internal(message: impl Into<String>) -> Self {
        CmError::Internal {
            message: message.into(),
        }
    }

    /// The client cannot reach the daemon.
    pub fn daemon_unavailable(message: impl Into<String>) -> Self {
        CmError::DaemonUnavailable {
            message: message.into(),
        }
    }

    /// A feature or platform backend that is not built yet (exit 1, JSON-RPC -32099).
    pub fn not_implemented(feature_message: impl Into<String>) -> Self {
        CmError::NotImplemented {
            message: feature_message.into(),
        }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            CmError::InvalidInput { .. } => ErrorCode::InvalidInput,
            CmError::NotFound { .. } => ErrorCode::NotFound,
            CmError::UserDeclined { .. } => ErrorCode::UserDeclined,
            CmError::ConfirmationTimeout { .. } => ErrorCode::ConfirmationTimeout,
            CmError::PermissionMissing { .. } => ErrorCode::PermissionMissing,
            CmError::DeviceBusy { .. } => ErrorCode::DeviceBusy,
            CmError::DiskFull { .. } => ErrorCode::DiskFull,
            CmError::Network { .. } => ErrorCode::Network,
            CmError::PlatformRejected { .. } => ErrorCode::PlatformRejected,
            CmError::AuthExpired { .. } => ErrorCode::AuthExpired,
            CmError::JobFailed { .. } => ErrorCode::JobFailed,
            CmError::Busy { .. } => ErrorCode::Busy,
            CmError::Internal { .. } => ErrorCode::Internal,
            CmError::DaemonUnavailable { .. } => ErrorCode::DaemonUnavailable,
            CmError::NotImplemented { .. } => ErrorCode::NotImplemented,
        }
    }

    pub fn exit_code(&self) -> i32 {
        self.code().exit_code()
    }

    /// The stable id and parameters of this error's message, when it is one a client can
    /// translate (see [`crate::message`]). The English `message` is unchanged.
    pub fn message_parts(&self) -> Option<crate::message::MessageParts> {
        crate::message::describe(self.code(), &self.to_string())
    }

    /// Whether a retry may succeed (only transient `network` failures).
    pub fn is_retryable(&self) -> bool {
        matches!(self, CmError::Network { .. })
    }

    /// Code-specific `data` (`permission`, `platform_code`, `job_id` + `reason`).
    pub fn data(&self) -> Option<Value> {
        match self {
            CmError::PermissionMissing { permission, .. } => {
                Some(json!({ "permission": permission.as_str() }))
            }
            CmError::PlatformRejected {
                platform_code: Some(c),
                ..
            } => Some(json!({ "platform_code": c })),
            CmError::JobFailed { job_id, reason, .. } => {
                Some(json!({ "job_id": job_id, "reason": reason }))
            }
            _ => None,
        }
    }

    pub fn to_payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code(),
            message: self.to_string(),
            data: self.data(),
        }
    }

    /// Rebuilds an error from a received payload (client side).
    /// Errors with `invalid_input` when required `data` fields are missing.
    pub fn from_payload(p: &ErrorPayload) -> Result<Self, CmError> {
        let msg = p.message.clone();
        let field = |name: &str| -> Option<&Value> { p.data.as_ref().and_then(|d| d.get(name)) };
        Ok(match p.code {
            ErrorCode::InvalidInput => CmError::InvalidInput { message: msg },
            ErrorCode::NotFound => CmError::NotFound { message: msg },
            ErrorCode::UserDeclined => CmError::UserDeclined { message: msg },
            ErrorCode::ConfirmationTimeout => CmError::ConfirmationTimeout { message: msg },
            ErrorCode::PermissionMissing => {
                let permission = field("permission")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .ok_or_else(|| {
                        CmError::invalid_field("data.permission", "missing or unknown")
                    })?;
                CmError::PermissionMissing {
                    permission,
                    message: msg,
                }
            }
            ErrorCode::DeviceBusy => CmError::DeviceBusy { message: msg },
            ErrorCode::DiskFull => CmError::DiskFull { message: msg },
            ErrorCode::Network => CmError::Network { message: msg },
            ErrorCode::PlatformRejected => CmError::PlatformRejected {
                message: msg,
                platform_code: field("platform_code")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            },
            ErrorCode::AuthExpired => CmError::AuthExpired { message: msg },
            ErrorCode::JobFailed => {
                let get = |n: &str| field(n).and_then(Value::as_str).map(str::to_owned);
                match (get("job_id"), get("reason")) {
                    (Some(job_id), Some(reason)) => CmError::JobFailed {
                        job_id,
                        reason,
                        message: msg,
                    },
                    _ => {
                        return Err(CmError::invalid_field(
                            "data",
                            "job_failed needs job_id and reason",
                        ))
                    }
                }
            }
            ErrorCode::Busy => CmError::Busy { message: msg },
            ErrorCode::Internal => CmError::Internal { message: msg },
            ErrorCode::DaemonUnavailable => CmError::DaemonUnavailable { message: msg },
            ErrorCode::NotImplemented => CmError::NotImplemented { message: msg },
        })
    }
}

impl From<&CmError> for ErrorPayload {
    fn from(e: &CmError) -> Self {
        e.to_payload()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<CmError> {
        vec![
            CmError::invalid_field("fps", "must be positive"),
            CmError::not_found("capture rec_1"),
            CmError::user_declined("declined"),
            CmError::confirmation_timeout("no answer in 5 min"),
            CmError::permission_missing(Permission::InputMonitoring, "grant input monitoring"),
            CmError::device_busy("mic in use"),
            CmError::disk_full("below floor"),
            CmError::network("timeout"),
            CmError::platform_rejected("quota", Some("quotaExceeded".into())),
            CmError::auth_expired("reconnect"),
            CmError::job_failed("job_1", "encoder crashed"),
            CmError::busy("recording running"),
            CmError::internal("sqlite: database is locked"),
            CmError::daemon_unavailable("no daemon at daemon.sock"),
            CmError::not_implemented("the Windows encoder is not built yet"),
        ]
    }

    #[test]
    fn codes_and_exit_codes_match_readme_table() {
        let expected = [
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
        for (err, (code, exit)) in samples().iter().zip(expected) {
            assert_eq!(err.code().as_str(), code);
            assert_eq!(err.exit_code(), exit);
            assert_eq!(serde_json::to_value(err.code()).unwrap(), code);
        }
        assert_eq!(ErrorCode::ALL.len(), 15);
    }

    #[test]
    fn payload_shape_and_data() {
        let p = CmError::permission_missing(Permission::Screen, "need screen").to_payload();
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            json!({"code":"permission_missing","message":"need screen","data":{"permission":"screen"}})
        );
        let p = CmError::job_failed("j", "boom").to_payload();
        assert_eq!(p.data, Some(json!({"job_id":"j","reason":"boom"})));
        let p = CmError::not_implemented("later").to_payload();
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            json!({"code":"not_implemented","message":"later","data":null})
        );
        let p = CmError::busy("x").to_payload();
        assert_eq!(serde_json::to_value(&p).unwrap()["data"], Value::Null);
    }

    #[test]
    fn permission_names() {
        let all = [
            (Permission::Screen, "screen"),
            (Permission::Camera, "camera"),
            (Permission::Microphone, "microphone"),
            (Permission::Accessibility, "accessibility"),
            (Permission::InputMonitoring, "input_monitoring"),
        ];
        for (p, s) in all {
            assert_eq!(p.as_str(), s);
            assert_eq!(serde_json::to_value(p).unwrap(), s);
        }
    }

    #[test]
    fn payload_round_trips() {
        for e in samples() {
            let json = serde_json::to_string(&e.to_payload()).unwrap();
            let back: ErrorPayload = serde_json::from_str(&json).unwrap();
            assert_eq!(CmError::from_payload(&back).unwrap(), e);
        }
        let bad = ErrorPayload {
            code: ErrorCode::PermissionMissing,
            message: "m".into(),
            data: None,
        };
        assert_eq!(
            CmError::from_payload(&bad).unwrap_err().code(),
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn only_network_is_retryable() {
        let retryable: Vec<_> = samples()
            .iter()
            .filter(|e| e.is_retryable())
            .map(CmError::code)
            .collect();
        assert_eq!(retryable, vec![ErrorCode::Network]);
    }
}
