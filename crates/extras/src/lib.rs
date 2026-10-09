//! Extra BRP methods for Bevy applications
//!
//! This crate provides additional Bevy Remote Protocol (BRP) methods that can be added
//! to your Bevy application for enhanced remote control capabilities.
//! This checkout targets Bevy 0.20.0; its Plugin and ECS types cannot be mixed with Bevy 0.19
//! types. Use the same revision for Extras, runtime (when used), and MCP.
//! Hosts must provide Bevy's input and window message channels. `DefaultPlugins` supplies them;
//! a `MinimalPlugins` host can add [`bevy::input::InputPlugin`] and [`bevy::window::WindowPlugin`].
//! For a windowless runner, configure `WindowPlugin` with no primary window and
//! [`bevy::window::ExitCondition::DontExit`]. Picking input additionally requires the host's
//! Picking plugins and a suitable backend.
//!
//! # BRP methods and agent tools
//!
//! Registering a remote method inserts its system into
//! [`RemoteMethods`](bevy_remote::RemoteMethods), which makes the method callable and visible in
//! exhaustive `rpc.discover` transport discovery. Calling
//! [`AppAgentToolExt::register_agent_tool`] is a separate action: it publishes an agent-facing
//! description and optional raw JSON schemas for one existing instant method. It does not register
//! the backing BRP handler or create a native MCP tool.
//!
//! Every published [`AgentTool`] names a BRP method, while most BRP methods need not be published
//! as agent tools. See the complete
//! [agent tool registration example](https://github.com/slc90/bevy_brp/blob/main/crates/extras/examples/agent_tool_registration.rs)
//! for the required plugin ordering, method insertion, and mutable-resource borrow scope.
//!
//! After running the example, agents list the curated entries and pass a selected entry's exact
//! method and matching raw parameters to `brp_execute`:
//!
//! ```text
//! cargo run -p bevy_brp_extras --example agent_tool_registration
//! brp_list_agent_tools(port: 15702)
//! brp_execute(
//!     port: 15702,
//!     method: "example/multiply",
//!     params: { "value": 6, "factor": 7 }
//! )
//! ```
//!
//! An omitted parameter schema documents a parameterless method whose JSON-RPC request omits
//! `params`. An omitted result schema leaves the raw BRP JSON-RPC `result` undocumented. Schema
//! types supplied through the generic builder methods generate documentation during application
//! construction only; they do not decode requests or encode results.
//!
//! [`struct@BrpExtrasPlugin`] installs the instant `brp_extras/agent_tools` endpoint that publishes
//! the current metadata. Each request validates every backing method against the live
//! [`RemoteMethods`](bevy_remote::RemoteMethods) resource. If any entry's method is missing or is a
//! watching method, the request returns no partial catalog. Its BRP error data identifies the
//! rejected entry with stable `name`, `method`, and `reason` fields.
//!
//! # Plugin Composability
//!
//! `BrpExtrasPlugin` is designed to compose with existing BRP setups. If
//! [`RemotePlugin`](bevy_remote::RemotePlugin) or
//! [`RemoteHttpPlugin`](bevy_remote::http::RemoteHttpPlugin) are already added
//! to the app, `BrpExtrasPlugin` will skip adding them and register its methods
//! into the existing [`RemoteMethods`](bevy_remote::RemoteMethods) resource.
//!
//! **Important**: If `RemoteHttpPlugin` is already present, any port
//! configuration on `BrpExtrasPlugin` (`with_port()` or the `BRP_EXTRAS_PORT`
//! environment variable) will be ignored — the existing HTTP transport is used
//! as-is. A warning is logged when this occurs.
//!
//! # HTTP Transport Configuration
//!
//! On native targets, HTTP transport can be configured in three mutually
//! exclusive ways (enforced at compile time):
//!
//! 1. **Default** — uses `BRP_EXTRAS_PORT` env var or port 15702
//! 2. **Explicit port** — `BrpExtrasPlugin::with_port(9000)`
//! 3. **Full control** — `BrpExtrasPlugin::with_http_plugin(plugin)` accepts a pre-configured
//!    [`RemoteHttpPlugin`](bevy_remote::http::RemoteHttpPlugin)
//!
//! # Available BRP Methods
//!
//! ## App Lifecycle
//!
//! ### `brp_extras/screenshot`
//! Captures the primary window, a camera viewport, or a bounds-backed entity and publishes a
//! complete PNG.
//! The watching request returns only after publication.
//! Success means the PNG is fully encoded and atomically published; it does not assert that scene
//! content is nonuniform. A minimized, hidden, or fully occluded primary-window surface may
//! legitimately produce a black image on platforms that stop presenting it. Entity captures
//! reflect the selected camera target; retained image or other offscreen targets avoid
//! primary-window presentation dependence when the application is designed to use them.
//! - `path` (string, required): destination file path
//! - `entity` (u64, optional): exact Bevy entity ID whose bounds select the crop
//! - `camera` (u64, optional): active camera viewport, or the camera used for entity capture
//! - `padding` (u32, optional): physical pixels added around entity bounds; requires `entity` and
//!   defaults to zero
//!
//! AABB capture projects the selected entity's [`Aabb`](bevy::camera::primitives::Aabb) through
//! the selected camera. With the default `ui` feature, complete Bevy UI computed components take
//! precedence over an incidental AABB and supply transformed, clipped physical bounds plus their
//! computed target camera. Partial UI computed state is rejected.
//! UI bounds intersect the transformed node with every inherited clip before padding and pixel
//! rounding. Rotated clips still return a rectangular crop of the composited target: its corners
//! may contain background or other entities. The PNG has no entity-specific alpha mask.
//! Disabling default features retains AABB capture without compiling this crate's UI bounds
//! resolver, imports, or capability.
//! This does not guarantee removal of UI crates from the dependency graph because upstream Bevy
//! 0.20 `bevy_remote` brings that family transitively through `bevy_dev_tools`. Both modes crop the
//! complete composited target, so overlapping content and post-processing remain visible. With
//! neither `camera` nor `entity`, the method captures the primary window. With only `camera`, it
//! captures that camera's physical viewport. The method never resolves names or descendants. If
//! `camera` is omitted for AABB capture, exactly one active, initialized camera with a
//! screenshot-capable target must exist.
//!
//! Requires Bevy's `png` feature. Calls fail before enqueueing when PNG support is unavailable.
//!
//! ### `brp_extras/shutdown`
//! Schedules a graceful application shutdown. No parameters.
//!
//! ### `brp_extras/set_window_title`
//! Changes the title of the primary window.
//! - `title` (string, required): new window title
//!
//! ### `brp_extras/get_diagnostics`
//! Returns FPS and frame time diagnostics from Bevy's `DiagnosticsStore`.
//! No parameters. Requires the `diagnostics` cargo feature (enabled by default).
//!
//! Returns current, average, and smoothed values for FPS and frame time,
//! plus total frame count and history buffer metadata.
//!
//! ## App-owned pointer source
//!
//! Move, button, click, double-click, drag and scroll use one stable Custom pointer per App.
//! The host must install enabled
//! PickingPlugin, InteractionPlugin and an appropriate picking backend. Extras does not
//! install picking plugins or a UI backend. Coordinates are logical window pixels and movement
//! does not write raw mouse events or the native cursor. A new physical Mouse move, press or
//! scroll cancels the custom generation and hands control back to the physical pointer.
//! Left/Right/Middle map to Primary/Secondary/Middle; other mouse buttons are rejected.
//! Timed holds use real elapsed time and allow same-window move/scroll before release.
//! Automatic gestures share a FIFO with Picking cycle barriers and real-time waits. They wait
//! for existing timed holds and keep later input queued until completion. Acceptance does not
//! prove a UI hit or successful widget action. Cancellation emits Cancel rather than Release.
//! Raw MouseMotion/MouseButtonInput/MouseWheel and native cursor consumers are not driven by
//! these methods. Official Mouse-only Hovered/DirectlyHovered consumers need host adaptation.
//! Pointer input does not require OS foreground focus and does not change Window.focused;
//! application logic and host Picking configuration can still constrain results.
//!
//! ### `brp_extras/pointer_control`
//!
//! An instant App method, discoverable through `rpc.discover`. Use the existing MCP
//! `brp_execute` tool; this method adds neither a static MCP tool nor a default agent tool.
//! Requests are exactly `{"action":"status"}` or `{"action":"release"}`; missing, unknown or
//! additional fields return INVALID_PARAMS without changing state. Status requires no Picking.
//! Both actions return phase (inactive/active/draining), busy, pointer_id (stable UUID or null),
//! window (entity bits or null), generation, queued_actions, pressed_buttons (Left/Right/Middle)
//! and last_error (null or generation/method/window/code/message for a queued failure).
//! An idle active pointer has busy=false. Status does not create work or clear last_error.
//! Release cancels outside the FIFO, invalidates the generation and returns draining while
//! cleanup is pending. It is idempotent during cleanup and when inactive. Ordinary input is
//! rejected during draining; poll status until inactive before handing control back.
//! A successful new activation clears last_error. Always release in automation cleanup.
//!
//! MCP examples:
//! `{"method":"brp_extras/pointer_control","params":{"action":"release"},"port":15712}`
//! then `{"method":"brp_extras/pointer_control","params":{"action":"status"},"port":15712}`.
//!
//! ## Keyboard
//!
//! Both methods accept optional `window` entity bits (u64). Omission captures
//! the primary window, or a placeholder in headless apps. Explicit targets
//! must have a live `Window` component or the request returns `INVALID_PARAMS`
//! without input side effects. All press/release events retain the captured
//! window despite focus or primary-window changes. When it disappears, pending
//! input is cancelled and held keys are released to that original target to
//! clear global input state. No input is redirected to another window.
//!
//! ### `brp_extras/send_keys`
//! Simulates keyboard input with a press-hold-release cycle. All keys are
//! pressed simultaneously and held for the specified duration.
//! - `keys` (array of strings, required): key codes (e.g., `["KeyA", "Space", "ShiftLeft"]`)
//! - `duration_ms` (u32, optional, default: 100, max: 60000): hold duration in milliseconds
//!
//! ### `brp_extras/type_text`
//! Types text sequentially, alternating press and release frames, with proper shift handling
//! for uppercase and symbols.
//! - `text` (string, required): text to type (letters, numbers, symbols, newlines, tabs)
//!
//! ## Mouse
//!
//! All mouse methods accept an optional `window` parameter (entity ID) to target
//! a specific window. Defaults to the last BRP window, then the primary window.
//!
//! Button values: `"Left"`, `"Right"`, `"Middle"`.
//!
//! ### `brp_extras/click_mouse`
//! Queues a click at the BRP position after establishing a Picking hit. Holds for 100ms of
//! real elapsed time; Press and Release are consumed in separate Picking cycles.
//! - `button` (string, required)
//! - `window` (u64, optional)
//!
//! ### `brp_extras/double_click_mouse`
//! Queues two complete 100ms clicks, with a delay after the first release. Bevy's
//! `multi_click_interval` decides the count; a long delay can produce two single clicks.
//! - `button` (string, required)
//! - `delay_ms` (u32, optional, default: 250): delay between clicks
//! - `window` (u64, optional)
//!
//! ### `brp_extras/send_mouse_button`
//! Presses and holds a mouse button for a specified duration.
//! - `button` (string, required)
//! - `duration_ms` (u32, optional, default: 100, max: 60000)
//! - `window` (u64, optional)
//!
//! ### `brp_extras/move_mouse`
//! Queues Custom Pointer movement by delta or absolute logical window position; provide exactly
//! one. Each window has independent BRP history, starting at the origin. Finite positions outside
//! the window are valid; overflow is rejected. Window defaults to the last BRP target, then primary.
//! Explicit invalid or destroyed targets fail. Switching windows while holding a button fails.
//! - `delta` ([f32; 2], optional): relative movement
//! - `position` ([f32; 2], optional): absolute position
//! - `window` (u64, optional)
//!
//! ### `brp_extras/drag_mouse`
//! Queues an ordinary widget drag: establish the start hit, press, interpolate, then release
//! after the endpoint is consumed. Native Window move/resize regions are unsupported.
//! - `button` (string, required)
//! - `start` ([f32; 2], required): starting position
//! - `end` ([f32; 2], required): ending position
//! - `frames` (u32, required, minimum: 1): interpolation steps; setup and release add cycles
//! - `window` (u64, optional)
//!
//! ### `brp_extras/scroll_mouse`
//! Queues Custom Pointer scroll after establishing the target location. Units are preserved.
//! - `x` (f32, required): horizontal scroll amount
//! - `y` (f32, required): vertical scroll amount
//! - `unit` (string, required): `"Line"` or `"Pixel"`
//! - `window` (u64, optional)
//!
//! ## Trackpad Gestures (macOS)
//!
//! ### `brp_extras/double_tap_gesture`
//! Sends a double-tap gesture event. No parameters.
//!
//! ### `brp_extras/pinch_gesture`
//! Sends a pinch gesture for zoom operations.
//! - `delta` (f32, required): positive = zoom in, negative = zoom out
//!
//!
//! ### `brp_extras/rotation_gesture`
//! Sends a rotation gesture.
//! - `delta` (f32, required): rotation in radians
//!
//! ## Agent Tools
//!
//! ### `brp_extras/agent_tools`
//! Returns the catalog of agent tools published through
//! [`AppAgentToolExt::register_agent_tool`]. No parameters.
//! See [BRP methods and agent tools](#brp-methods-and-agent-tools) for the per-request validation
//! rules and the BRP error data returned for a rejected entry.

mod activity;
mod agent_tools;
mod constants;
#[cfg(feature = "diagnostics")]
mod diagnostics;
mod keyboard;
mod mouse;
mod plugin;
mod screenshot;
mod shutdown;
mod window_event;
mod window_title;

pub use activity::BrpExtrasActivity;
pub use activity::BrpExtrasActivityState;
pub use agent_tools::AgentTool;
pub use agent_tools::AppAgentToolExt;
pub use constants::DEFAULT_REMOTE_PORT;
pub use plugin::BrpExtrasPlugin;
pub use plugin::ExternalTransport;
pub use plugin::HasEffectivePort;
pub use plugin::HttpPluginConfigured;
pub use plugin::PortConfigured;
pub use plugin::PortDisplay;
pub use plugin::Unconfigured;
