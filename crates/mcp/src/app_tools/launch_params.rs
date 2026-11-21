use std::collections::HashMap;

use bevy_brp_mcp_macros::ParamStruct;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use super::instance_count::InstanceCount;
use super::launch::LaunchParams;
use crate::brp_tools::Port;

/// Search order for target resolution: "app" searches apps first (default), "example" searches
/// examples first
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchOrder {
    /// Search apps first, then examples
    #[default]
    App,
    /// Search examples first, then apps
    Example,
}

/// Shared parameters for launching Bevy binaries (apps or examples)
#[derive(Clone, Deserialize, Serialize, JsonSchema, ParamStruct)]
pub struct LaunchBevyBinaryParams {
    /// Name of the Bevy target to launch (app or example)
    #[serde(rename = "target_name")]
    pub target: String,
    /// Build profile to use (debug or release)
    #[to_metadata(skip_if_none)]
    pub profile: Option<String>,
    /// Optional OS-level path to use as the search root. Overrides the default MCP workspace
    /// roots.
    #[serde(default)]
    #[to_metadata(skip_if_none)]
    pub path: Option<String>,
    /// Package name to filter when multiple targets with the same name exist
    #[serde(default)]
    #[serde(rename = "package_name")]
    #[to_metadata(skip_if_none)]
    pub package: Option<String>,
    /// Main BRP port (default connection target: 15702)
    #[serde(default)]
    #[to_metadata(skip_if_none)]
    pub port: Option<Port>,
    /// Render BRP port (defaults to 15703, or Main plus one for a Main-only override)
    #[serde(default)]
    #[to_metadata(skip_if_none)]
    pub render_port: Option<Port>,
    /// Single instance uses the supplied App-configured pair without environment overrides
    #[serde(default)]
    pub ports_from_config: bool,
    /// Number of instances to launch (default: 1)
    #[serde(default)]
    pub instance_count: InstanceCount,
    /// Request initial activation of the primary window on Windows (default: false)
    #[serde(default)]
    pub activate_window: bool,
    /// Optional environment variables to set on the launched process
    #[serde(default)]
    #[to_metadata(skip_if_none)]
    pub env: Option<HashMap<String, String>>,
    /// Search order: "app" searches apps first (default), "example" searches examples first
    #[serde(default)]
    pub search_order: SearchOrder,
    /// Optional command-line arguments to pass to the launched process
    #[serde(default)]
    #[to_metadata(skip_if_none)]
    pub args: Option<Vec<String>>,
}

impl LaunchBevyBinaryParams {
    /// Convert to `LaunchParams` with the given default profile
    pub fn to_launch_params(&self, default_profile: &str) -> LaunchParams {
        LaunchParams {
            target: self.target.clone(),
            profile: self
                .profile
                .clone()
                .unwrap_or_else(|| default_profile.to_string()),
            path: self.path.clone(),
            package: self.package.clone(),
            port: self.port,
            render_port: self.render_port,
            ports_from_config: self.ports_from_config,
            instance_count: self.instance_count,
            activate_window: self.activate_window,
            env: self.env.clone(),
            search_order: self.search_order.clone(),
            args: self.args.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_startup_focus_is_optional_and_must_be_boolean() {
        let default: LaunchBevyBinaryParams =
            serde_json::from_value(serde_json::json!({"target_name": "example"})).unwrap();
        assert!(!default.to_launch_params("debug").activate_window);

        for activate_window in [false, true] {
            let params: LaunchBevyBinaryParams = serde_json::from_value(serde_json::json!({
                "target_name": "example",
                "activate_window": activate_window,
            }))
            .unwrap();
            assert_eq!(
                params.to_launch_params("debug").activate_window,
                activate_window
            );
        }

        assert!(
            serde_json::from_value::<LaunchBevyBinaryParams>(serde_json::json!({
                "target_name": "example",
                "activate_window": "true",
            }))
            .is_err()
        );
    }
}
