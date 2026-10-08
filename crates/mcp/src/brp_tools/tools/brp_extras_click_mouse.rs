//! `brp_extras/click_mouse` tool - Click mouse button

use bevy_brp_mcp_macros::ParamStruct;
use bevy_brp_mcp_macros::ResultStruct;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::brp_tools::Port;
use crate::brp_tools::mouse::MouseButtonWrapper;

/// Parameters for the `brp_extras/click_mouse` tool
#[derive(Clone, Deserialize, Serialize, JsonSchema, ParamStruct)]
pub struct ClickMouseParams {
    /// Mouse button to click (Left, Right, Middle)
    pub button: MouseButtonWrapper,

    /// Optional window entity ID to target (defaults to last BRP target, then primary window)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<u64>,

    /// The BRP port (default: 15702)
    #[serde(default)]
    pub port: Port,
}

/// Result for the `brp_extras/click_mouse` tool
#[derive(Serialize, ResultStruct)]
#[brp_result]
pub struct ClickMouseResult {
    /// The raw BRP response
    #[serde(skip_serializing_if = "Option::is_none")]
    #[to_result(skip_if_none)]
    pub result: Option<Value>,

    /// Message template for formatting responses
    #[to_message(message_template = "Mouse click accepted")]
    pub message_template: String,
}
