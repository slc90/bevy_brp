use std::collections::HashMap;

use itertools::Itertools;
use rmcp::model::Tool;
use strum::IntoEnumIterator;

use super::ToolDef;
use super::ToolName;

/// Static tool catalog used for MCP listing and dispatch.
pub(crate) struct ToolRegistry {
    definitions: HashMap<String, ToolDef>,
    tools: Vec<Tool>,
}

impl ToolRegistry {
    pub(crate) fn new() -> Self {
        let definitions = get_all_tool_definitions();
        let tools = definitions
            .iter()
            .map(ToolDef::to_tool)
            .sorted_by_key(|tool| {
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.title.as_ref())
                    .map_or_else(|| tool.name.as_ref(), String::as_str)
                    .to_string()
            })
            .collect();
        let definitions = definitions
            .into_iter()
            .map(|definition| (definition.name().to_string(), definition))
            .collect();

        Self { definitions, tools }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&ToolDef> {
        self.definitions.get(name)
    }

    pub(crate) fn tools(&self) -> &[Tool] {
        &self.tools
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.definitions.len()
    }
}

pub(crate) fn get_all_tool_definitions() -> Vec<ToolDef> {
    ToolName::iter()
        .map(|tool_name| ToolDef {
            tool_name,
            annotations: tool_name.get_annotations(),
            handler: tool_name.create_handler(),
            parameters: tool_name.get_parameters(),
        })
        .collect()
}
