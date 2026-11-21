//! Wake-aware BRP runtime transport for Bevy applications.
//!
//! Add [`BrpRuntimePlugin`] after Bevy's windowing plugins to install BRP Extras methods together
//! with an HTTP transport that wakes the main event loop whenever remote work arrives.
//!
//! This checkout targets Bevy 0.20.0. Its Plugin and ECS types cannot be mixed with Bevy 0.19
//! types. Consume runtime, directly imported Extras, and MCP from the same revision.

mod http;
mod progress;

use std::time::Duration;

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_remote::http::{DEFAULT_RENDER_PORT, RemoteHttpPlugin};

use self::http::BrpRemoteHttpPlugin;

/// Installs BRP Extras methods and the wake-aware HTTP transport.
///
/// Add this plugin after Bevy's `WinitPlugin`. Do not add the upstream
/// [`RemoteHttpPlugin`] to the same `App`: this plugin owns the BRP HTTP
/// transport. Use `BrpExtrasPlugin` when the App already owns an upstream
/// transport.
///
/// On Windows, Runtime shares [`struct@BrpExtrasPlugin`]'s `BRP_EXTRAS_WINDOW_FOCUSED`
/// override for the primary window's initial focus request.
///
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_brp_runtime::BrpRuntimePlugin;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(BrpRuntimePlugin::default())
///     .run();
/// ```
pub struct BrpRuntimePlugin {
    /// Maximum time to wait for the final result of a regular HTTP request.
    request_deadline: Duration,
    /// Main World BRP listener port when the environment does not override it.
    main_port: u16,
    /// Render World BRP listener port when the environment does not override it.
    render_port: u16,
}

impl Default for BrpRuntimePlugin {
    fn default() -> Self {
        Self {
            request_deadline: Duration::from_secs(30),
            main_port: bevy_brp_extras::DEFAULT_REMOTE_PORT,
            render_port: DEFAULT_RENDER_PORT,
        }
    }
}

impl BrpRuntimePlugin {
    /// Create the runtime with a custom Main World BRP port.
    ///
    /// A valid `BRP_EXTRAS_PORT` environment variable takes precedence. The Render World
    /// port defaults to `15703` and can be overridden by `BRP_EXTRAS_RENDER_PORT`.
    #[must_use]
    pub fn with_port(port: u16) -> Self {
        Self::with_ports(port, DEFAULT_RENDER_PORT)
    }

    /// Create the runtime with custom Main and Render World BRP ports.
    ///
    /// Valid `BRP_EXTRAS_PORT` and `BRP_EXTRAS_RENDER_PORT` environment variables
    /// override the corresponding configured port independently. [`Default`] uses
    /// `15702` for Main and `15703` for Render. Render requires Bevy's render sub-app.
    #[must_use]
    pub fn with_ports(main_port: u16, render_port: u16) -> Self {
        Self {
            main_port,
            render_port,
            ..Self::default()
        }
    }
}

impl Plugin for BrpRuntimePlugin {
    fn build(&self, app: &mut App) {
        reject_upstream_http_transport(app);

        app.add_plugins((
            BrpExtrasPlugin::without_http_transport(),
            BrpRemoteHttpPlugin::new(self.request_deadline, self.main_port, self.render_port),
        ));
    }

    fn finish(&self, app: &mut App) {
        reject_upstream_http_transport(app);
    }
}

#[allow(
    clippy::panic,
    reason = "a second BRP HTTP transport violates the runtime plugin ownership contract"
)]
fn reject_upstream_http_transport(app: &App) {
    if app.is_plugin_added::<RemoteHttpPlugin>() {
        error!("BrpRuntimePlugin 与 RemoteHttpPlugin 不能同时拥有 BRP HTTP transport");
        panic!("BrpRuntimePlugin cannot share an App with RemoteHttpPlugin");
    }
}

#[cfg(test)]
mod tests {
    use bevy_remote::http::RemoteHttpPlugin;

    use super::*;

    #[test]
    fn code_configured_main_port_is_retained() {
        let plugin = BrpRuntimePlugin::with_port(9000);
        assert_eq!(plugin.main_port, 9000);
        assert_eq!(plugin.render_port, bevy_remote::http::DEFAULT_RENDER_PORT);
    }

    #[test]
    fn dual_port_configuration_retains_both_code_configured_ports() {
        let plugin = BrpRuntimePlugin::with_ports(9000, 9010);
        assert_eq!((plugin.main_port, plugin.render_port), (9000, 9010));
    }

    #[test]
    #[should_panic(expected = "BrpRuntimePlugin cannot share an App with RemoteHttpPlugin")]
    fn rejects_existing_stock_http_transport() {
        let mut app = App::new();
        app.add_plugins(RemoteHttpPlugin::default());
        app.add_plugins(BrpRuntimePlugin::default());
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[should_panic(expected = "BrpRuntimePlugin cannot share an App with RemoteHttpPlugin")]
    fn rejects_stock_http_transport_added_after_runtime() {
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.add_plugins(bevy::winit::WinitPlugin {
            run_on_any_thread: true,
        });
        app.add_plugins(BrpRuntimePlugin::default());
        app.add_plugins(RemoteHttpPlugin::default());
        app.finish();
    }
}
