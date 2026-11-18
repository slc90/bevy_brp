use std::time::Duration;

use bevy::remote::schemas::open_rpc::OpenRpcDocument;
use rmcp::ErrorData as McpError;
use rmcp::model::CacheScope;
use rmcp::model::GetPromptRequestParams;
use rmcp::model::GetPromptResult;
use rmcp::model::ListPromptsResult;
use rmcp::model::ListResourceTemplatesResult;
use rmcp::model::ListResourcesResult;
use rmcp::model::PaginatedRequestParams;
use rmcp::model::Prompt;
use rmcp::model::PromptArgument;
use rmcp::model::PromptMessage;
use rmcp::model::ReadResourceResult;
use rmcp::model::Resource;
use rmcp::model::ResourceContents;
use rmcp::model::ResourceTemplate;
use rmcp::model::Role;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use crate::brp_tools::BoundedBrpError;
use crate::brp_tools::BrpClient;
use crate::brp_tools::Port;
use crate::tool::BrpMethod;

const GUIDE_URI: &str = "bevy-brp://guide/inspection";
const GUIDE_TEXT: &str = include_str!("inspection_guide.md");
const GUIDE_MIME: &str = "text/markdown";
const METHODS_URI_PREFIX: &str = "bevy-brp://app/";
const METHODS_URI_SUFFIX: &str = "/methods";
const METHODS_MIME: &str = "application/json";
const INSPECTION_PROMPT: &str = "inspect_bevy_app";
const PROMPT_DESCRIPTION: &str = "Prepare a read-only inspection of a Bevy App on a BRP port.";
const PORT_DESCRIPTION: &str = "The BRP port as a decimal string (1024–65534).";
const STATIC_TTL_MS: u64 = 60_000;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DISCOVERY_BYTES: usize = 1_048_576;

pub(super) fn list_resources(
    request: Option<PaginatedRequestParams>,
) -> Result<ListResourcesResult, McpError> {
    require_first_page(request)?;
    let guide = Resource::new(GUIDE_URI, "bevy_brp_inspection_guide")
        .with_title("Bevy BRP inspection guide")
        .with_description("How to inspect a running Bevy App and verify what it does.")
        .with_mime_type(GUIDE_MIME);
    Ok(ListResourcesResult::with_all_items(vec![guide])
        .with_ttl_ms(STATIC_TTL_MS)
        .with_cache_scope(CacheScope::Private))
}

pub(super) fn list_resource_templates(
    request: Option<PaginatedRequestParams>,
) -> Result<ListResourceTemplatesResult, McpError> {
    require_first_page(request)?;
    let methods = ResourceTemplate::new(methods_uri("{port}"), "bevy_brp_methods")
        .with_title("Live BRP method directory")
        .with_description(format!(
            "Read rpc.discover on a running Bevy App. {PORT_DESCRIPTION}"
        ))
        .with_mime_type(METHODS_MIME);
    Ok(ListResourceTemplatesResult::with_all_items(vec![methods])
        .with_ttl_ms(STATIC_TTL_MS)
        .with_cache_scope(CacheScope::Private))
}

pub(super) async fn read_resource(uri: String) -> Result<ReadResourceResult, McpError> {
    let (text, mime, ttl_ms) = if uri == GUIDE_URI {
        (GUIDE_TEXT.to_owned(), GUIDE_MIME, STATIC_TTL_MS)
    } else {
        let port = uri
            .strip_prefix(METHODS_URI_PREFIX)
            .and_then(|path| path.strip_suffix(METHODS_URI_SUFFIX))
            .ok_or_else(|| McpError::resource_not_found("Unknown Bevy BRP resource URI", None))?;
        let document = discover_methods(parse_port(port)?).await?;
        (document.to_string(), METHODS_MIME, 0)
    };

    Ok(
        ReadResourceResult::new(vec![ResourceContents::text(text, uri).with_mime_type(mime)])
            .with_ttl_ms(ttl_ms)
            .with_cache_scope(CacheScope::Private),
    )
}

pub(super) fn list_prompts(
    request: Option<PaginatedRequestParams>,
) -> Result<ListPromptsResult, McpError> {
    require_first_page(request)?;
    let port = PromptArgument::new("port")
        .with_description(PORT_DESCRIPTION)
        .with_required(true);
    let prompt = Prompt::new(
        INSPECTION_PROMPT,
        Some(PROMPT_DESCRIPTION),
        Some(vec![port]),
    )
    .with_title("Inspect a Bevy App");
    Ok(ListPromptsResult::with_all_items(vec![prompt])
        .with_ttl_ms(STATIC_TTL_MS)
        .with_cache_scope(CacheScope::Private))
}

pub(super) fn get_prompt(request: GetPromptRequestParams) -> Result<GetPromptResult, McpError> {
    if request.name != INSPECTION_PROMPT {
        return Err(invalid_field("name", "Unknown Bevy BRP prompt"));
    }
    let arguments = request.arguments.unwrap_or_default();
    if arguments.keys().any(|key| key != "port") {
        return Err(invalid_field(
            "arguments",
            "The prompt accepts only the port argument",
        ));
    }
    let port = arguments
        .get("port")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_field("port", PORT_DESCRIPTION))?;
    let port = parse_port(port)?;
    let uri = methods_uri(port);
    let text = format!(
        "Inspect the Bevy App on BRP port {port}. If this client supports MCP resources, read \
         {GUIDE_URI} and {uri}. If the live directory is unavailable through resources, use \
         rpc_discover with {{\"port\":{port}}}. Use the observed method directory to choose \
         read-only checks. For example, pass {{\"port\":{port}}} along with the required inputs \
         to world_get_components, or call a discovered read-only method with brp_execute on \
         port {port}. Report what you actually observed, including any failed checks or \
         unverified UI behavior."
    );
    Ok(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
            .with_description(PROMPT_DESCRIPTION),
    )
}

fn methods_uri(port: impl std::fmt::Display) -> String {
    format!("{METHODS_URI_PREFIX}{port}{METHODS_URI_SUFFIX}")
}

fn parse_port(port: &str) -> Result<Port, McpError> {
    if port.len() <= 5
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && let Ok(number) = port.parse::<u16>()
        && let Ok(port) = Port::try_from(number)
    {
        return Ok(port);
    }
    Err(invalid_field("port", PORT_DESCRIPTION))
}

fn require_first_page(request: Option<PaginatedRequestParams>) -> Result<(), McpError> {
    if request.is_some_and(|params| params.cursor.is_some()) {
        return Err(invalid_field(
            "cursor",
            "This listing does not issue a cursor",
        ));
    }
    Ok(())
}

fn invalid_field(field: &str, message: &'static str) -> McpError {
    McpError::invalid_params(message, Some(json!({"field": field})))
}

async fn discover_methods(port: Port) -> Result<Value, McpError> {
    let client = BrpClient::new(BrpMethod::RpcDiscover, port, None);
    let value = client
        .execute_local_bounded(DISCOVERY_TIMEOUT, MAX_DISCOVERY_BYTES)
        .await
        .map_err(|error| discovery_error(port, error))?;
    // Validate with the same OpenRPC model as the tools while preserving extension fields.
    let document = OpenRpcDocument::deserialize(&value).map_err(|_| {
        discovery_error(
            port,
            BoundedBrpError::InvalidResponse("Expected an OpenRPC method directory"),
        )
    })?;
    if document.methods.iter().any(|method| method.name.is_empty()) {
        return Err(discovery_error(
            port,
            BoundedBrpError::InvalidResponse("OpenRPC method name is empty"),
        ));
    }
    Ok(value)
}

fn discovery_error(port: Port, error: BoundedBrpError) -> McpError {
    let (kind, reason, details) = match error {
        BoundedBrpError::Transport(source) if source.is_timeout() => (
            "timeout",
            "BRP request timed out",
            json!({"timeout_ms": DISCOVERY_TIMEOUT.as_millis()}),
        ),
        BoundedBrpError::Transport(source) if source.is_connect() => {
            ("connection", "Could not connect to the BRP port", json!({}))
        }
        BoundedBrpError::Transport(_) => ("transport", "BRP transport failed", json!({})),
        BoundedBrpError::HttpStatus(status) => (
            "http_status",
            "BRP returned an unsuccessful HTTP status",
            json!({"status": status}),
        ),
        BoundedBrpError::ResponseTooLarge => (
            "response_too_large",
            "BRP response exceeded the byte limit",
            json!({"max_bytes": MAX_DISCOVERY_BYTES}),
        ),
        BoundedBrpError::InvalidResponse(reason) => ("invalid_response", reason, json!({})),
        BoundedBrpError::Remote(error) => (
            "brp_error",
            "BRP returned a JSON-RPC error",
            json!({"brp_error": error}),
        ),
    };
    let mut data = json!({"method": BrpMethod::RpcDiscover.as_str(), "port": port, "kind": kind});
    if let (Some(data), Value::Object(details)) = (data.as_object_mut(), details) {
        data.extend(details);
    }
    McpError::internal_error(format!("rpc.discover on port {port}: {reason}"), Some(data))
}
