use std::collections::HashMap;

use itertools::Itertools;
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
use rmcp::model::Tool;
use rmcp::service::RequestContext;

use super::tool;
use super::tool::ToolDef;
use crate::constants::TOOL_LIST_CACHE_TTL_MS;

/// MCP service implementation for Bevy Remote Protocol integration.
///
/// This service provides tools for interacting with Bevy applications through BRP,
/// including entity manipulation, component management, and resource access.
pub(crate) struct McpService {
    /// Tool definitions `HashMap` for O(1) lookup by name
    tool_defs: HashMap<String, ToolDef>,
    /// Pre-converted MCP tools for list operations
    tools: Vec<Tool>,
}

impl McpService {
    pub(crate) fn new() -> Self {
        let all_defs = tool::get_all_tool_definitions();

        // Build the `ToolDef` lookup table.
        let tool_defs = all_defs
            .iter()
            .map(|tool_def| (tool_def.name().to_string(), tool_def.clone()))
            .collect();

        // Store a sorted `Vec<Tool>` for `McpService::list_mcp_tools`.
        let tools: Vec<_> = all_defs
            .iter()
            .map(ToolDef::to_tool)
            .sorted_by_key(|tool| {
                tool.annotations
                    .as_ref()
                    .and_then(|ann| ann.title.as_ref())
                    .map_or_else(|| tool.name.as_ref(), String::as_str)
                    .to_string()
            })
            .collect();

        Self { tool_defs, tools }
    }

    /// Get tool definition by name with O(1) lookup
    fn get_tool_def(&self, name: &str) -> Option<&ToolDef> {
        self.tool_defs.get(name)
    }

    /// List all MCP tools using pre-converted and sorted tools
    ///
    /// `ListToolsResult::with_all_items` sets `result_type` to `ResultType::COMPLETE` but leaves
    /// `ttl_ms` and `cache_scope` as `None`, so rmcp drops both from the wire response. Protocol
    /// version `2026-07-28` makes them required on every `CacheableResult`, and clients that
    /// validate against that schema reject the response and load no tools at all. Set them here.
    fn list_mcp_tools(&self) -> ListToolsResult {
        ListToolsResult::with_all_items(self.tools.clone())
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
        assert_eq!(names.len(), service.tool_defs.len());
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
    }
}
