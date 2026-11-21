//! Plugin implementation for extra BRP methods

#[cfg(windows)]
use std::ffi::OsStr;
use std::sync::Mutex;

#[cfg(feature = "diagnostics")]
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::window::PrimaryWindow;
use bevy_remote::RemoteMethodSystemId;
use bevy_remote::RemoteMethods;
use bevy_remote::RemotePlugin;
use bevy_remote::http::DEFAULT_RENDER_PORT;
use bevy_remote::http::HostPort;
use bevy_remote::http::RemoteHttpPlugin;

use super::DEFAULT_REMOTE_PORT;
use super::agent_tools;
use super::agent_tools::RegisteredAgentTools;
use super::constants::BRP_EXTRAS_PORT_ENV_VAR;
use super::constants::BRP_EXTRAS_RENDER_PORT_ENV_VAR;
use super::constants::EXTRAS_COMMAND_PREFIX;
use super::constants::METHOD_AGENT_TOOLS;
use super::constants::METHOD_CLICK_MOUSE;
use super::constants::METHOD_DOUBLE_CLICK_MOUSE;
use super::constants::METHOD_DOUBLE_TAP_GESTURE;
use super::constants::METHOD_DRAG_MOUSE;
#[cfg(feature = "diagnostics")]
use super::constants::METHOD_GET_DIAGNOSTICS;
use super::constants::METHOD_MOVE_MOUSE;
use super::constants::METHOD_PINCH_GESTURE;
use super::constants::METHOD_POINTER_CONTROL;
use super::constants::METHOD_ROTATION_GESTURE;
use super::constants::METHOD_SCREENSHOT;
use super::constants::METHOD_SCROLL_MOUSE;
use super::constants::METHOD_SEND_KEYS;
use super::constants::METHOD_SEND_MOUSE_BUTTON;
use super::constants::METHOD_SET_WINDOW_TITLE;
use super::constants::METHOD_SHUTDOWN;
use super::constants::METHOD_TYPE_TEXT;
#[cfg(feature = "diagnostics")]
use super::diagnostics;
use super::keyboard;
use super::keyboard::KeyboardPlugin;
use super::mouse;
use super::mouse::MousePlugin;
use super::screenshot;
use super::screenshot::ScreenshotPlugin;
use super::shutdown;
use super::window_title;

#[cfg(windows)]
const WINDOW_FOCUSED_ENV_VAR: &str = "BRP_EXTRAS_WINDOW_FOCUSED";

// ---------------------------------------------------------------------------
// Plugin struct and const shorthand
// ---------------------------------------------------------------------------

/// Plugin that adds extra BRP methods to a Bevy app
///
/// Currently provides:
/// - `brp_extras/screenshot`: Capture screenshots
/// - `brp_extras/shutdown`: Gracefully shutdown the app
/// - `brp_extras/send_keys`: Send keyboard input
/// - `brp_extras/set_window_title`: Change the window title
///
/// This also adds `RemoteHttpPlugin` for HTTP transport.
/// 调用 [`BrpExtrasPlugin::without_http_transport`] 时，所有 target 都只注册
/// methods 和相关工作 system，由宿主自行安装 transport。
///
/// # HTTP transport configuration
///
/// HTTP transport can be configured in three ways
/// (mutually exclusive, enforced at compile time):
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_brp_extras::BrpExtrasPlugin;
/// // 1. Default — Main 15702 and Render 15703, with environment overrides
/// App::new().add_plugins((DefaultPlugins, BrpExtrasPlugin::default()));
///
/// // 2. Explicit Main and Render ports
/// App::new().add_plugins((DefaultPlugins, BrpExtrasPlugin::with_ports(9000, 9001)));
/// ```
///
/// ```ignore
/// // 3. Full control — provide your own RemoteHttpPlugin
/// App::new().add_plugins((DefaultPlugins, BrpExtrasPlugin::with_http_plugin(
///     RemoteHttpPlugin::default()
///         .with_port(9000)
///         .with_address([0, 0, 0, 0])
/// )));
/// ```
#[allow(
    non_upper_case_globals,
    reason = "const shares struct name for ergonomic plugin construction"
)]
pub const BrpExtrasPlugin: BrpExtrasPlugin = BrpExtrasPlugin::new();

/// Plugin type for adding extra BRP methods.
///
/// The `HttpConfig` type parameter controls how HTTP transport is configured.
/// See the [module-level documentation](struct@BrpExtrasPlugin) for usage examples.
///
/// On Windows, install this plugin after `WindowPlugin` to let
/// `BRP_EXTRAS_WINDOW_FOCUSED=true` or `false` set the primary window's initial focus
/// request before Winit creates the native window. An unset or invalid value preserves
/// the host's window settings. This applies to all HTTP configuration modes.
/// Winit separately requests activation when creating a fullscreen window.
pub struct BrpExtrasPlugin<HttpConfig = Unconfigured> {
    http_config: HttpConfig,
    port_display: Option<PortDisplay>,
}

impl BrpExtrasPlugin<Unconfigured> {
    /// Create a new plugin instance with default HTTP configuration.
    ///
    /// Main defaults to `15702` and Render defaults to `15703` when a render sub-app exists.
    /// Valid `BRP_EXTRAS_PORT` and `BRP_EXTRAS_RENDER_PORT` overrides apply independently.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            http_config: Unconfigured,
            port_display: None,
        }
    }

    /// Create a plugin with a custom Main port and the default Render port (`15703`).
    ///
    /// Valid Main and Render environment overrides still take precedence.
    ///
    /// This is mutually exclusive with [`with_http_plugin`](Self::with_http_plugin)
    /// — the compiler enforces that only one can be used.
    #[must_use]
    pub const fn with_port(port: u16) -> BrpExtrasPlugin<PortConfigured> {
        Self::with_ports(port, DEFAULT_RENDER_PORT)
    }

    /// Create a plugin with custom Main and Render BRP ports.
    ///
    /// Valid `BRP_EXTRAS_PORT` and `BRP_EXTRAS_RENDER_PORT` overrides apply independently.
    /// Add this plugin after Bevy's rendering plugins to configure the render sub-app.
    #[must_use]
    pub const fn with_ports(main_port: u16, render_port: u16) -> BrpExtrasPlugin<PortConfigured> {
        BrpExtrasPlugin {
            http_config: PortConfigured {
                main_port,
                render_port,
            },
            port_display: None,
        }
    }

    /// Provide a fully configured `RemoteHttpPlugin` (native only).
    ///
    /// When using this method, `BrpExtrasPlugin` adds the provided plugin as-is.
    /// Port environment overrides apply only to the managed HTTP configuration.
    ///
    /// This is mutually exclusive with [`with_port`](Self::with_port)
    /// — the compiler enforces that only one can be used.
    #[must_use]
    pub const fn with_http_plugin(
        plugin: RemoteHttpPlugin,
    ) -> BrpExtrasPlugin<HttpPluginConfigured> {
        BrpExtrasPlugin {
            http_config: HttpPluginConfigured(Mutex::new(Some(plugin))),
            port_display: None,
        }
    }

    /// 创建只注册 Extras methods、由宿主提供 transport 的 plugin。
    ///
    /// 该入口仍会安装或复用 [`RemotePlugin`]，但不会添加 [`RemoteHttpPlugin`]、读取端口配置
    /// 或修改 Winit update mode。
    #[must_use]
    pub const fn without_http_transport() -> BrpExtrasPlugin<ExternalTransport> {
        BrpExtrasPlugin {
            http_config: ExternalTransport,
            port_display: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Port resolution
// ---------------------------------------------------------------------------

impl<H: HasEffectivePort> BrpExtrasPlugin<H> {
    /// Get the effective Main port for the managed HTTP transport.
    ///
    /// Priority order:
    /// 1. `BRP_EXTRAS_PORT` environment variable (highest priority)
    /// 2. Configured Main port or default (15702)
    ///
    /// Returns `(port, source_description)` for logging.
    #[must_use]
    pub fn get_effective_port(&self) -> (u16, String) {
        self.resolve_main_port(false)
    }

    fn resolve_main_port(&self, warn_on_invalid: bool) -> (u16, String) {
        let fallback = self.http_config.fallback_port();

        let env_port = read_port_override(BRP_EXTRAS_PORT_ENV_VAR, fallback, warn_on_invalid);

        let effective_port = env_port.unwrap_or(fallback);

        let explicit = self.http_config.is_explicit();

        let source_description = match (env_port, explicit) {
            (Some(_), false) => {
                format!("environment override from default {DEFAULT_REMOTE_PORT}")
            }
            (Some(_), true) => {
                format!("environment override from with_port {fallback}")
            }
            (None, false) => "default".to_string(),
            (None, true) => "with_port".to_string(),
        };

        (effective_port, source_description)
    }

    /// Append `(port: XXXXX)` to the primary window's title at startup.
    ///
    /// - [`PortDisplay::Always`] — always appends the port
    /// - [`PortDisplay::NonDefault`] — only appends when the effective port differs from the
    ///   default (15702)
    ///
    /// If not called, the window title is left unchanged.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use bevy::prelude::*;
    /// # use bevy_brp_extras::BrpExtrasPlugin;
    /// # use bevy_brp_extras::PortDisplay;
    /// App::new()
    ///     .add_plugins(DefaultPlugins)
    ///     .add_plugins(BrpExtrasPlugin::with_port(9000).port_in_title(PortDisplay::NonDefault))
    ///     .run();
    /// ```
    #[must_use]
    pub const fn port_in_title(mut self, display: PortDisplay) -> Self {
        self.port_display = Some(display);
        self
    }

    fn build_managed(&self, app: &mut App, render_fallback: u16) {
        let owns_transport = !app.is_plugin_added::<RemoteHttpPlugin>();
        let (main_port, source) = self.resolve_main_port(owns_transport);
        if owns_transport {
            add_managed_http_transport(app, main_port, render_fallback, source);
        } else {
            warn!("`RemoteHttpPlugin` is already installed and provides the BRP transport.");
        }
        maybe_add_port_title_system(app, main_port, self.port_display);
        build_shared(app);
    }
}

// ---------------------------------------------------------------------------
// Trait implementations
// ---------------------------------------------------------------------------

impl Default for BrpExtrasPlugin<Unconfigured> {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for BrpExtrasPlugin<Unconfigured> {
    fn build(&self, app: &mut App) {
        self.build_managed(app, DEFAULT_RENDER_PORT);
    }
}

impl Plugin for BrpExtrasPlugin<ExternalTransport> {
    fn build(&self, app: &mut App) {
        build_shared(app);
    }
}

impl Plugin for BrpExtrasPlugin<PortConfigured> {
    fn build(&self, app: &mut App) {
        self.build_managed(app, self.http_config.render_port);
    }
}

impl Plugin for BrpExtrasPlugin<HttpPluginConfigured> {
    fn build(&self, app: &mut App) {
        let Some(plugin) = self
            .http_config
            .0
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
        else {
            error!("failed to retrieve `RemoteHttpPlugin` configuration");
            build_shared(app);
            return;
        };

        if app.is_plugin_added::<RemoteHttpPlugin>() {
            warn!(
                "`RemoteHttpPlugin` is already added — the `RemoteHttpPlugin` provided to \
                 `BrpExtrasPlugin::with_http_plugin()` will be ignored. The existing HTTP \
                 transport will be used as-is."
            );
        } else {
            app.add_plugins(plugin);
        }

        build_shared(app);
    }
}

// ---------------------------------------------------------------------------
// Port display configuration
// ---------------------------------------------------------------------------

/// Controls whether the BRP port is appended to the window title.
///
/// Used with [`BrpExtrasPlugin::port_in_title`] to display the port in the
/// primary window's title bar.
#[derive(Clone, Copy, Debug)]
pub enum PortDisplay {
    /// Always append `(port: XXXXX)` to the window title.
    Always,
    /// Only append when not using the default port (15702).
    NonDefault,
}

// ---------------------------------------------------------------------------
// HTTP configuration state types
// ---------------------------------------------------------------------------

/// No HTTP configuration specified — uses `BRP_EXTRAS_PORT` env var or default port.
pub struct Unconfigured;

/// HTTP transport 由宿主安装，Extras 只负责 methods 与工作 system。
pub struct ExternalTransport;

/// HTTP transport configured with an explicit port.
pub struct PortConfigured {
    main_port: u16,
    render_port: u16,
}

/// HTTP transport configured with a user-provided `RemoteHttpPlugin`.
pub struct HttpPluginConfigured(Mutex<Option<RemoteHttpPlugin>>);

// ---------------------------------------------------------------------------
// Port resolution trait
// ---------------------------------------------------------------------------

/// Trait for HTTP configuration states that can resolve an effective port.
///
/// Implemented for [`Unconfigured`] and [`PortConfigured`] — both states where
/// `BrpExtrasPlugin` manages the HTTP transport and the port is knowable.
///
/// Not implemented for [`HttpPluginConfigured`] because the user provides their
/// own `RemoteHttpPlugin` and already knows the port they configured.
pub trait HasEffectivePort {
    /// The fallback port when `BRP_EXTRAS_PORT` env var is not set.
    fn fallback_port(&self) -> u16;

    /// Whether the port was explicitly configured via `with_port()`.
    fn is_explicit(&self) -> bool;
}

impl HasEffectivePort for Unconfigured {
    fn fallback_port(&self) -> u16 {
        DEFAULT_REMOTE_PORT
    }
    fn is_explicit(&self) -> bool {
        false
    }
}

impl HasEffectivePort for PortConfigured {
    fn fallback_port(&self) -> u16 {
        self.main_port
    }
    fn is_explicit(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Shared build logic
// ---------------------------------------------------------------------------

/// Common plugin setup shared across all HTTP configuration states.
fn build_shared(app: &mut App) {
    #[cfg(windows)]
    apply_window_startup_focus(app, std::env::var_os(WINDOW_FOCUSED_ENV_VAR).as_deref());

    app.init_resource::<RegisteredAgentTools>();
    app.init_resource::<crate::activity::BrpExtrasActivity>();

    // Add `RemotePlugin` if not already present
    if !app.is_plugin_added::<RemotePlugin>() {
        app.add_plugins(RemotePlugin::default());
    }

    // Register extras methods into the existing `RemoteMethods` resource
    register_extras_methods(app.world_mut());

    // Defensively add `FrameTimeDiagnosticsPlugin` if not already installed
    #[cfg(feature = "diagnostics")]
    if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());
    }

    app.add_plugins(KeyboardPlugin);
    app.add_plugins(MousePlugin);
    app.add_plugins(ScreenshotPlugin);

    // Add the system to handle deferred shutdown
    app.add_systems(Update, shutdown::deferred_shutdown_system);
}

#[cfg(windows)]
fn apply_window_startup_focus(app: &mut App, env_value: Option<&OsStr>) {
    let Some(value) = env_value else {
        return;
    };
    let Some(focused) = value.to_str().and_then(|value| value.parse::<bool>().ok()) else {
        warn!(
            env_var = WINDOW_FOCUSED_ENV_VAR,
            "Ignoring invalid startup window focus setting. Expected true or false."
        );
        return;
    };

    // Winit reads this field when it creates the native window.
    let world = app.world_mut();
    if let Ok(mut window) = world
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .single_mut(world)
    {
        window.focused = focused;
    }
}

fn read_port_override(env_var: &str, fallback_port: u16, warn_on_invalid: bool) -> Option<u16> {
    let value = std::env::var_os(env_var)?;
    match value.to_string_lossy().parse() {
        Ok(port) => Some(port),
        Err(error) => {
            if warn_on_invalid {
                warn!(env_var, fallback_port, %error, "Invalid BRP port override. Using the configured port.");
            }
            None
        }
    }
}

fn add_managed_http_transport(app: &mut App, main_port: u16, render_fallback: u16, source: String) {
    app.add_plugins(RemoteHttpPlugin::default().with_port(main_port));
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        let render_port = read_port_override(BRP_EXTRAS_RENDER_PORT_ENV_VAR, render_fallback, true)
            .unwrap_or(render_fallback);
        render_app.insert_resource(HostPort(render_port));
    }
    app.add_systems(Startup, move |_: &mut World| {
        log_initialization(main_port, &source);
    });
}

/// Register all extras BRP methods into the world's `RemoteMethods` resource.
fn register_extras_methods(world: &mut World) {
    let methods = vec![
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_AGENT_TOOLS}"),
            RemoteMethodSystemId::Instant(world.register_system(agent_tools::catalog_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_CLICK_MOUSE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::click_mouse_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_DOUBLE_CLICK_MOUSE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::double_click_mouse_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_DOUBLE_TAP_GESTURE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::double_tap_gesture_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_DRAG_MOUSE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::drag_mouse_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_MOVE_MOUSE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::move_mouse_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_POINTER_CONTROL}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::pointer_control_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_PINCH_GESTURE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::pinch_gesture_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_ROTATION_GESTURE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::rotation_gesture_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_SCREENSHOT}"),
            RemoteMethodSystemId::Watching(world.register_system(screenshot::handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_SCROLL_MOUSE}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::scroll_mouse_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_SEND_KEYS}"),
            RemoteMethodSystemId::Instant(world.register_system(keyboard::send_keys_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_SEND_MOUSE_BUTTON}"),
            RemoteMethodSystemId::Instant(world.register_system(mouse::send_mouse_button_handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_SET_WINDOW_TITLE}"),
            RemoteMethodSystemId::Instant(world.register_system(window_title::handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_SHUTDOWN}"),
            RemoteMethodSystemId::Instant(world.register_system(shutdown::handler)),
        ),
        (
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_TYPE_TEXT}"),
            RemoteMethodSystemId::Instant(world.register_system(keyboard::type_text_handler)),
        ),
    ];

    #[cfg(feature = "diagnostics")]
    let methods = {
        let mut methods = methods;
        methods.push((
            format!("{EXTRAS_COMMAND_PREFIX}{METHOD_GET_DIAGNOSTICS}"),
            RemoteMethodSystemId::Instant(world.register_system(diagnostics::handler)),
        ));
        methods
    };

    let mut remote_methods = world.resource_mut::<RemoteMethods>();
    for (name, system_id) in methods {
        remote_methods.insert(name, system_id);
    }
}

fn log_initialization(port: u16, source_description: &str) {
    info!("BRP extras enabled on http://localhost:{port} ({source_description})");
}

/// Conditionally adds a `Startup` system that appends the port to the primary
/// window's title, based on the [`PortDisplay`] policy.
fn maybe_add_port_title_system(
    app: &mut App,
    effective_port: u16,
    port_display: Option<PortDisplay>,
) {
    let Some(display) = port_display else {
        return;
    };

    let should_display = match display {
        PortDisplay::Always => true,
        PortDisplay::NonDefault => effective_port != DEFAULT_REMOTE_PORT,
    };

    if should_display {
        app.add_systems(
            Startup,
            move |mut query: Query<&mut Window, With<PrimaryWindow>>| {
                if let Ok(mut window) = query.single_mut() {
                    window.title = format!("{} (port: {effective_port})", window.title);
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::SubApp;
    use bevy::render::RenderScheduleOrder;

    use super::*;

    const UNEXPECTED_ENTITY_CAPTURE_METHOD: &str = "brp_extras/screenshot_entity";

    fn app_with_render_sub_app() -> App {
        let mut render_app = SubApp::new();
        render_app.init_resource::<RenderScheduleOrder>();
        let mut app = App::new();
        app.insert_sub_app(RenderApp, render_app);
        app
    }

    #[test]
    fn dual_port_configuration_sets_both_managed_world_ports() {
        let mut app = app_with_render_sub_app();
        app.add_plugins(BrpExtrasPlugin::with_ports(9000, 9010));

        let plugin = BrpExtrasPlugin::with_ports(9000, 9010);
        let (main, _) = plugin.get_effective_port();
        assert_eq!(app.world().resource::<HostPort>().0, main);
        let render = std::env::var("BRP_EXTRAS_RENDER_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(9010);
        assert_eq!(
            app.sub_app(RenderApp).world().resource::<HostPort>().0,
            render
        );
    }

    #[test]
    fn dual_port_configuration_does_not_change_unmanaged_transport_resources() {
        let mut app = app_with_render_sub_app();
        app.add_plugins(BrpExtrasPlugin::without_http_transport());
        assert!(!app.world().contains_resource::<HostPort>());
        assert!(
            !app.sub_app(RenderApp)
                .world()
                .contains_resource::<HostPort>()
        );

        let mut app = app_with_render_sub_app();
        app.add_plugins(BrpExtrasPlugin::with_http_plugin(
            RemoteHttpPlugin::default().with_port(9000),
        ));
        assert_eq!(app.world().resource::<HostPort>().0, 9000);
        assert_eq!(
            app.sub_app(RenderApp).world().resource::<HostPort>().0,
            DEFAULT_RENDER_PORT
        );
    }

    #[cfg(windows)]
    #[test]
    fn window_startup_focus_applies_to_the_primary_window_during_build() {
        use std::ffi::OsStr;

        for (configured, initial, expected) in [
            (Some("false"), true, false),
            (Some("true"), false, true),
            (Some("invalid"), true, true),
            (None, false, false),
        ] {
            let mut app = App::new();
            let world = app.world_mut();
            let primary = world
                .spawn((
                    Window {
                        focused: initial,
                        ..default()
                    },
                    PrimaryWindow,
                ))
                .id();
            let secondary = world.spawn(Window::default()).id();

            apply_window_startup_focus(&mut app, configured.map(OsStr::new));

            assert_eq!(
                app.world().get::<Window>(primary).unwrap().focused,
                expected
            );
            assert!(app.world().get::<Window>(secondary).unwrap().focused);
        }

        apply_window_startup_focus(&mut App::new(), Some(OsStr::new("false")));
    }

    fn assert_all_extras_methods_registered(app: &App) {
        let methods = app.world().resource::<RemoteMethods>();
        let method_names = [
            METHOD_AGENT_TOOLS,
            METHOD_CLICK_MOUSE,
            METHOD_DOUBLE_CLICK_MOUSE,
            METHOD_DOUBLE_TAP_GESTURE,
            METHOD_DRAG_MOUSE,
            METHOD_MOVE_MOUSE,
            METHOD_PINCH_GESTURE,
            METHOD_POINTER_CONTROL,
            METHOD_ROTATION_GESTURE,
            METHOD_SCREENSHOT,
            METHOD_SCROLL_MOUSE,
            METHOD_SEND_KEYS,
            METHOD_SEND_MOUSE_BUTTON,
            METHOD_SET_WINDOW_TITLE,
            METHOD_SHUTDOWN,
            METHOD_TYPE_TEXT,
        ];

        for method_name in method_names {
            let method = format!("{EXTRAS_COMMAND_PREFIX}{method_name}");
            assert!(methods.get(&method).is_some(), "missing method: {method}");
        }

        #[cfg(feature = "diagnostics")]
        {
            let diagnostics_method = format!("{EXTRAS_COMMAND_PREFIX}{METHOD_GET_DIAGNOSTICS}");
            assert!(methods.get(&diagnostics_method).is_some());
        }
    }

    #[test]
    fn entity_capture_uses_only_the_existing_screenshot_method() {
        let mut app = App::new();
        app.add_plugins(RemotePlugin::default());
        register_extras_methods(app.world_mut());
        let methods = app.world().resource::<RemoteMethods>();
        let screenshot_method = format!("{EXTRAS_COMMAND_PREFIX}{METHOD_SCREENSHOT}");

        assert!(methods.get(&screenshot_method).is_some());
        assert!(methods.get(UNEXPECTED_ENTITY_CAPTURE_METHOD).is_none());
    }

    /// 验证 methods-only 入口保留完整 Extras 能力与 RemotePlugin，同时不安装 HTTP transport。
    #[test]
    fn external_transport_registers_methods_without_http_plugin() {
        let mut app = App::new();
        app.add_plugins(BrpExtrasPlugin::without_http_transport());

        assert!(app.is_plugin_added::<RemotePlugin>());
        assert!(!app.is_plugin_added::<RemoteHttpPlugin>());
        assert_all_extras_methods_registered(&app);
    }

    /// 验证默认入口继续安装上游管理的 HTTP transport。
    #[test]
    fn default_configuration_keeps_http_plugin() {
        let mut app = App::new();
        app.add_plugins(BrpExtrasPlugin);

        assert!(app.is_plugin_added::<RemoteHttpPlugin>());
        assert_all_extras_methods_registered(&app);
    }

    /// 验证复用宿主已有 `RemotePlugin` 时不会移除或覆盖其他 namespace 的 method。
    #[test]
    fn external_transport_preserves_unrelated_methods() {
        const UNRELATED_METHOD: &str = "example/unrelated";

        let mut app = App::new();
        app.add_plugins(RemotePlugin::default());
        let handler = app.world_mut().register_system(shutdown::handler);
        app.world_mut().resource_mut::<RemoteMethods>().insert(
            UNRELATED_METHOD.to_string(),
            RemoteMethodSystemId::Instant(handler),
        );

        app.add_plugins(BrpExtrasPlugin::without_http_transport());

        let methods = app.world().resource::<RemoteMethods>();
        assert!(methods.get(UNRELATED_METHOD).is_some());
        assert_all_extras_methods_registered(&app);
    }
}
