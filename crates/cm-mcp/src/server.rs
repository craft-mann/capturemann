//! The MCP `ServerHandler`: every tool call becomes one daemon action over IPC.

use std::sync::Arc;

use cm_core::ErrorCode;
#[allow(deprecated)]
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, GetPromptRequestParams,
    GetPromptResponse, Implementation, ListPromptsResult, ListResourceTemplatesResult,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, ResourceContents, ServerCapabilities, ServerConfig,
    SetLevelRequestParams,
};
use rmcp::service::{NotificationContext, RequestContext};
use rmcp::{ErrorData as McpError, Peer, RoleServer, ServerHandler};
use serde_json::{json, Map, Value};
use tokio::sync::Mutex;

use crate::conn::{ConnError, Connector, DaemonConn, RpcError};
use crate::events::{self, LevelFilter};
use crate::prompts::{self, PromptError};
use crate::resources::{self, UriError};
use crate::tools::{self, ToolSpec};

/// Name used when the MCP client sent no usable `clientInfo.name`.
pub const FALLBACK_CLIENT: &str = "mcp-client";
/// The daemon accepts at most this many bytes in a client name.
const MAX_CLIENT_NAME: usize = 64;

/// Turns an MCP `clientInfo.name` into a daemon client name (printable, at most 64 bytes).
pub fn sanitize_client_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > MAX_CLIENT_NAME {
            break;
        }
        out.push(c);
    }
    if out.is_empty() {
        FALLBACK_CLIENT.to_owned()
    } else {
        out
    }
}

/// An MCP server bound to one daemon connection (opened lazily by the first tool call, so
/// the MCP client's name is known for `session.hello`, and reopened after a daemon restart).
pub struct McpServer {
    connector: Connector,
    conn: Mutex<Option<Arc<DaemonConn>>>,
    tools: Arc<Vec<ToolSpec>>,
    level: LevelFilter,
}

impl McpServer {
    pub fn new(connector: Connector) -> Self {
        McpServer {
            connector,
            conn: Mutex::new(None),
            tools: Arc::new(tools::all()),
            level: LevelFilter::default(),
        }
    }

    /// The tool definitions as listed to clients.
    pub fn tool_definitions(&self) -> Vec<rmcp::model::Tool> {
        self.tools.iter().map(ToolSpec::to_tool).collect()
    }

    async fn connection(
        &self,
        client: &str,
        peer: &Peer<RoleServer>,
    ) -> Result<Arc<DaemonConn>, ToolFailure> {
        let mut guard = self.conn.lock().await;
        if let Some(c) = guard.as_ref() {
            if !c.is_closed() {
                return Ok(c.clone());
            }
        }
        *guard = None;
        let (r, w) = (self.connector)().await.map_err(|e| {
            ToolFailure::unavailable(format!("cannot connect to the CaptureMann daemon: {e}"))
        })?;
        let conn = DaemonConn::new(r, w);
        // AGT-2: the name is the `agent:<client>` actor and the key of the client policy.
        match conn
            .call(
                "session.hello",
                json!({ "client": client, "version": env!("CARGO_PKG_VERSION") }),
            )
            .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(ToolFailure::Rpc(e)),
            Err(e) => return Err(ToolFailure::unavailable(e.to_string())),
        }
        // Events for this client; a policy refusal or a failure only means no notifications.
        if let Ok(Ok(rx)) = conn.subscribe_events(events::TOPICS).await {
            events::spawn_forwarder(peer.clone(), rx, self.level.clone());
        }
        *guard = Some(conn.clone());
        Ok(conn)
    }

    async fn run(
        &self,
        action: &str,
        args: Value,
        client: &str,
        peer: &Peer<RoleServer>,
    ) -> Result<Value, ToolFailure> {
        let conn = self.connection(client, peer).await?;
        match conn.call(action, args).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(ToolFailure::Rpc(e)),
            Err(e @ (ConnError::Closed | ConnError::Io(_) | ConnError::Protocol(_))) => {
                Err(ToolFailure::unavailable(e.to_string()))
            }
        }
    }
}

fn client_name(context: &RequestContext<RoleServer>) -> String {
    context.client_info().map_or_else(
        || FALLBACK_CLIENT.to_owned(),
        |i| sanitize_client_name(&i.name),
    )
}

/// Why a tool call failed after it was routed.
enum ToolFailure {
    /// The daemon answered with a JSON-RPC error (`error.data.code` is kept).
    Rpc(RpcError),
    /// The daemon could not be reached or died mid-call.
    Unavailable(String),
    /// The MCP client cancelled the request.
    Cancelled,
}

impl ToolFailure {
    /// The JSON-RPC error of a failed resource read: `not_found` is `resource_not_found`,
    /// everything else keeps the daemon's `{ code, message, data }` as the error data
    /// (a policy refusal is `client_blocked`, `user_declined` and so on).
    fn into_resource_error(self, uri: &str) -> McpError {
        match self {
            ToolFailure::Rpc(e) => {
                let data = if e.data.get("code").is_some() {
                    e.data.clone()
                } else {
                    json!({ "code": ErrorCode::Internal.as_str(), "message": e.message, "data": null })
                };
                if data["code"] == ErrorCode::NotFound.as_str() {
                    McpError::resource_not_found(
                        format!("no such resource: {uri}"),
                        Some(json!({ "uri": uri, "cause": data })),
                    )
                } else {
                    McpError::invalid_request(
                        format!(
                            "{}: {}",
                            data["code"].as_str().unwrap_or("error"),
                            e.message
                        ),
                        Some(data),
                    )
                }
            }
            ToolFailure::Unavailable(msg) => McpError::internal_error(
                msg.clone(),
                Some(json!({ "code": ErrorCode::DaemonUnavailable.as_str(), "message": msg })),
            ),
            ToolFailure::Cancelled => McpError::internal_error("the request was cancelled", None),
        }
    }

    fn unavailable(msg: String) -> Self {
        ToolFailure::Unavailable(msg)
    }

    /// The MCP tool error: text for the model, `structuredContent` = the daemon's
    /// `error.data` (`{ code, message, data }`).
    fn into_result(self) -> CallToolResult {
        let (payload, text) = match self {
            ToolFailure::Rpc(e) => {
                let payload = if e.data.get("code").is_some() {
                    e.data.clone()
                } else {
                    json!({ "code": ErrorCode::Internal.as_str(), "message": e.message, "data": null })
                };
                let kind = payload["code"]
                    .as_str()
                    .unwrap_or(ErrorCode::Internal.as_str())
                    .to_owned();
                let mut text = format!("{kind}: {}", e.message);
                if let Some(d) = payload.get("data").filter(|d| !d.is_null()) {
                    text.push_str(&format!(" {d}"));
                }
                (payload, text)
            }
            ToolFailure::Cancelled => (
                json!({ "code": ErrorCode::UserDeclined.as_str(),
                        "message": "the request was cancelled", "data": { "reason": "cancelled" } }),
                "cancelled: the request was cancelled".to_owned(),
            ),
            ToolFailure::Unavailable(msg) => (
                json!({ "code": ErrorCode::DaemonUnavailable.as_str(), "message": msg, "data": null }),
                format!("{}: {msg}", ErrorCode::DaemonUnavailable.as_str()),
            ),
        };
        let mut r = CallToolResult::error(vec![ContentBlock::text(text)]);
        r.structured_content = Some(payload);
        r
    }
}

/// Builds the MCP result of a successful action: structured content without the base64
/// image, a text block with the same JSON (for clients that ignore structured content), and
/// the image as an MCP image content block (`image.content_index` points at it).
pub fn success_result(mut value: Value) -> CallToolResult {
    let mut content = Vec::new();
    let mut image_block = None;
    if let Some(obj) = value.as_object_mut() {
        if let Some(img) = obj.get("image").filter(|i| i.is_object()).cloned() {
            let data = img.get("data").and_then(Value::as_str);
            let mime = img.get("mime_type").and_then(Value::as_str);
            if let (Some(data), Some(mime)) = (data, mime) {
                image_block = Some(ContentBlock::image(data, mime));
                let mut m = Map::new();
                m.insert("mime_type".into(), json!(mime));
                m.insert("content_index".into(), json!(1));
                obj.insert("image".into(), Value::Object(m));
            }
        }
    }
    content.push(ContentBlock::text(value.to_string()));
    content.extend(image_block);
    let mut r = CallToolResult::success(content);
    r.structured_content = Some(value);
    r
}

#[allow(deprecated)] // logging/setLevel carries the event level filter (see `events`)
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .enable_logging()
                .build(),
        )
        .with_server_info(Implementation::new(
            "capturemann",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "CaptureMann: screenshots, annotations and screen recordings through the local \
                 CaptureMann daemon. Screenshots: list_targets, capture_screenshot, annotate (one \
                 call is one undo step), suggest_redactions (read-only: proposes blur \
                 rectangles, apply the ones you accept with annotate), get_capture. Recording: list_devices, start_recording \
                 (the user approves on screen and sees a countdown), get_recording_status, \
                 pause_recording / resume_recording, set_mic_muted (mute the mic without stopping it), stop_recording (returns the capture id); \
                 60 fps needs a passing run_fps_selftest first. \
                 Read-only resources (resources/list and resources/templates/list): the library, \
                 jobs, a capture, its transcript and the platform rules. Prompts (prompts/list): \
                 record-tutorial, publish-everywhere, bug-report-screenshot, lesson-to-drive, \
                 caption-cleanup, make-short-clip. \
                 Daemon events (capture.*, job.*, export.*, recording.*) arrive as \
                 notifications/message from logger capturemann.events; the data is the event. \
                 Tools marked Confirm wait for the user to approve on screen. \
                 Errors carry a shared code in structuredContent.code (invalid_input, not_found, \
                 user_declined, confirmation_timeout, permission_missing, not_implemented, ...).",
        )
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        // Open the daemon connection now so events flow before the first tool call. A daemon
        // that is down is not an error here: the first tool call retries.
        let client = context.peer.peer_info().map_or_else(
            || FALLBACK_CLIENT.to_owned(),
            |i| sanitize_client_name(&i.client_info.name),
        );
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.connection(&client, &context.peer),
        )
        .await;
    }

    async fn set_level(
        &self,
        request: SetLevelRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.level.set(&request);
        Ok(())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tool_definitions()))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(resources::list()))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(
            resources::templates(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri;
        let target = resources::parse(&uri).map_err(|e| match e {
            UriError::Unknown => McpError::resource_not_found(
                format!("no such resource: {uri}"),
                Some(json!({ "uri": uri })),
            ),
            UriError::BadSegment(what) => McpError::invalid_params(
                format!("invalid {what} in {uri}"),
                Some(json!({ "uri": uri })),
            ),
        })?;
        let (action, params) = target.action();
        let client = client_name(&context);
        let outcome = tokio::select! {
            r = self.run(action, params, &client, &context.peer) => r,
            () = context.ct.cancelled() => Err(ToolFailure::Cancelled),
        };
        let answer = outcome.map_err(|f| f.into_resource_error(&uri))?;
        let body = target.shape(answer).ok_or_else(|| {
            McpError::resource_not_found(
                format!("no such resource: {uri}"),
                Some(json!({ "uri": uri })),
            )
        })?;
        let text = resources::render(&body).map_err(|size| {
            McpError::invalid_request(
                format!(
                    "too_large: {uri} is {size} bytes, over the {} byte limit",
                    resources::MAX_RESOURCE_BYTES
                ),
                Some(json!({ "code": "too_large", "uri": uri, "size_bytes": size,
                             "limit_bytes": resources::MAX_RESOURCE_BYTES })),
            )
        })?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri).with_mime_type(resources::mime())
        ])
        .into())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(prompts::list()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let args = request.arguments.unwrap_or_default();
        match prompts::get(&request.name, &args) {
            Ok(r) => Ok(r.into()),
            Err(PromptError::Unknown(n)) => Err(McpError::invalid_params(
                format!("unknown prompt `{n}`"),
                None,
            )),
            Err(PromptError::MissingArgument(a)) => Err(McpError::invalid_params(
                format!("missing argument `{a}`"),
                None,
            )),
        }
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let Some(spec) = self.tools.iter().find(|t| t.name == request.name) else {
            return Err(McpError::invalid_params(
                format!("unknown tool `{}`", request.name),
                None,
            ));
        };
        let args = request
            .arguments
            .map_or_else(|| Value::Object(Map::new()), Value::Object);
        let client = context.client_info().map_or_else(
            || FALLBACK_CLIENT.to_owned(),
            |i| sanitize_client_name(&i.name),
        );
        // R2-14: when the MCP client cancels this request, dropping `run` cancels it at the
        // daemon too (see `conn`), so a pending confirmation dialog closes.
        let outcome = tokio::select! {
            r = self.run(spec.name, args, &client, &context.peer) => r,
            () = context.ct.cancelled() => Err(ToolFailure::Cancelled),
        };
        let result = match outcome {
            Ok(v) => success_result(v),
            Err(f) => f.into_result(),
        };
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_names_are_sanitized() {
        assert_eq!(sanitize_client_name("claude-ai"), "claude-ai");
        assert_eq!(sanitize_client_name("  "), FALLBACK_CLIENT);
        assert_eq!(sanitize_client_name("a\nb"), "ab");
        assert_eq!(sanitize_client_name(&"é".repeat(100)).len(), 64);
    }

    #[test]
    fn image_becomes_content_block() {
        let r = success_result(
            json!({ "capture_id": "x", "image": { "mime_type": "image/png", "data": "AAAA" } }),
        );
        assert_eq!(r.content.len(), 2);
        assert!(r.content[1].as_image().is_some());
        let s = r.structured_content.unwrap_or_default();
        assert_eq!(s["image"]["content_index"], 1);
        assert!(s["image"].get("data").is_none());
    }
}
