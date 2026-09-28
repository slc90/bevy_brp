use rmcp::ErrorData as McpError;
use rmcp::RoleServer;
use rmcp::ServerHandler;
use rmcp::model::CacheScope;
use rmcp::model::CallToolRequestParams;
use rmcp::model::CallToolResponse;
use rmcp::model::ListToolsResult;
use rmcp::model::PaginatedRequestParams;
use rmcp::model::ServerCapabilities;
use rmcp::model::ServerInfo;
use rmcp::service::RequestContext;

use super::tool::ToolDef;
use super::tool::ToolRegistry;
use crate::constants::TOOL_LIST_CACHE_TTL_MS;

/// MCP service implementation for Bevy Remote Protocol integration.
///
/// This service provides tools for interacting with Bevy applications through BRP,
/// including entity manipulation, component management, and resource access.
pub(crate) struct McpService {
    registry: ToolRegistry,
}

impl McpService {
    pub(crate) fn new() -> Self {
        Self {
            registry: ToolRegistry::new(),
        }
    }

    /// Get tool definition by name with O(1) lookup
    fn get_tool_def(&self, name: &str) -> Option<&ToolDef> {
        self.registry.get(name)
    }

    /// List all MCP tools using pre-converted and sorted tools
    ///
    /// `ListToolsResult::with_all_items` sets `result_type` to `ResultType::COMPLETE` but leaves
    /// `ttl_ms` and `cache_scope` as `None`, so rmcp drops both from the wire response. Protocol
    /// version `2026-07-28` makes them required on every `CacheableResult`, and clients that
    /// validate against that schema reject the response and load no tools at all. Set them here.
    fn list_mcp_tools(&self) -> ListToolsResult {
        ListToolsResult::with_all_items(self.registry.tools().to_vec())
            .with_ttl_ms(TOOL_LIST_CACHE_TTL_MS)
            .with_cache_scope(CacheScope::Private)
    }
}

impl ServerHandler for McpService {
    fn get_info(&self) -> ServerInfo {
        let mut info = rmcp::model::ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info
    }

    fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> {
        std::future::ready(Ok(self.list_mcp_tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool_def = self.get_tool_def(&request.name).ok_or_else(|| {
            McpError::invalid_params(format!("unknown tool: {}", request.name), None)
        })?;

        // Every BRP tool completes in one round trip, so the MRTR response is always `Complete`.
        tool_def.call_tool(request).await.map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::Value;

    use super::McpService;

    fn has_schema_annotation(value: &Value) -> bool {
        match value {
            Value::Object(object) => {
                object.contains_key("title")
                    || object.contains_key("description")
                    || object.values().any(has_schema_annotation)
            }
            Value::Array(items) => items.iter().any(has_schema_annotation),
            _ => false,
        }
    }

    #[test]
    fn output_schemas_keep_constraints_without_repeating_annotations() {
        let service = McpService::new();
        let listing = serde_json::to_value(service.list_mcp_tools()).expect("serialize tool list");
        let tools = listing["tools"].as_array().expect("tool array");
        let first_schema = &tools[0]["outputSchema"];

        assert_eq!(first_schema["type"], "object");
        assert_eq!(
            first_schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(
            first_schema["required"],
            serde_json::json!(["status", "message", "call_info"])
        );
        assert_eq!(
            first_schema["properties"]["status"]["enum"],
            serde_json::json!(["success", "error"])
        );
        assert!(first_schema["properties"]["call_info"]["anyOf"].is_array());
        for tool in tools {
            assert_eq!(&tool["outputSchema"], first_schema, "{}", tool["name"]);
            assert!(
                !has_schema_annotation(&tool["outputSchema"]),
                "{}",
                tool["name"]
            );
        }
    }

    #[test]
    fn listed_tools_are_routable_and_publish_cache_contract() {
        let service = McpService::new();
        let listing = serde_json::to_value(service.list_mcp_tools()).expect("serialize tool list");
        let tools = listing["tools"].as_array().expect("tool array");
        let names: HashSet<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect();

        assert_eq!(listing["ttlMs"], Value::from(0));
        assert_eq!(listing["cacheScope"], Value::from("private"));
        assert_eq!(names.len(), tools.len());
        assert_eq!(names.len(), service.registry.len());
        assert!(
            names
                .iter()
                .all(|name| service.get_tool_def(name).is_some())
        );
        for expected in [
            "brp_launch",
            "brp_status",
            "rpc_discover",
            "world_get_components",
            "world_mutate_components",
            "world_get_components_watch",
            "brp_stop_watch",
            "brp_extras_type_text",
            "brp_extras_screenshot",
            "brp_read_log",
            "brp_shutdown",
        ] {
            assert!(names.contains(expected), "missing {expected}");
        }
        assert!(service.get_tool_def("unknown_tool").is_none());
        #[cfg(not(feature = "mcp-debug"))]
        {
            assert_eq!(names.len(), 47);
            for diagnostic in ["brp_get_trace_log_path", "brp_set_tracing_level"] {
                assert!(!names.contains(diagnostic));
                assert!(service.get_tool_def(diagnostic).is_none());
            }
        }
        #[cfg(feature = "mcp-debug")]
        {
            assert_eq!(names.len(), 49);
            for diagnostic in ["brp_get_trace_log_path", "brp_set_tracing_level"] {
                assert!(names.contains(diagnostic));
                assert!(service.get_tool_def(diagnostic).is_some());
            }
        }
    }
}
