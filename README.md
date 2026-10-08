# Bevy BRP

This repository connects an MCP client to Bevy applications through the Bevy Remote Protocol (BRP). It provides a stdio MCP server, extra BRP methods for application input and screenshots, and a wake-aware HTTP transport for applications whose event loop may sleep. The repository also contains test applications and protocol regression scripts.

## Use it in a Bevy application

Add the App-side libraries from this checkout:

```toml
[dependencies]
bevy_brp_runtime = { path = "../bevy_brp/crates/runtime" }
```

Install the runtime after Bevy's windowing plugins:

```rust
use bevy::prelude::*;
use bevy_brp_runtime::BrpRuntimePlugin;

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(BrpRuntimePlugin::default())
    .run();
```

`BrpRuntimePlugin` installs the extras methods and owns the wake-aware HTTP transport. If an application already owns a `RemoteHttpPlugin`, use `bevy_brp_extras::BrpExtrasPlugin` instead; its [rustdoc](crates/extras/src/lib.rs) describes port configuration and method behavior. The default BRP port is 15702. [App method metadata](crates/extras/examples/agent_tool_registration.rs) is optional: it exposes selected registered BRP methods through `brp_list_agent_tools`, and clients call them with `brp_execute`. It does not create native MCP tools.

To set the runtime's Main BRP port in code, use `BrpRuntimePlugin::with_port(9000)` in place of `BrpRuntimePlugin::default()`; see the [custom port example](crates/runtime/examples/runtime_custom_port.rs). A valid `BRP_EXTRAS_PORT` environment variable overrides that value. The Render BRP port remains Bevy's default.

## Connect an MCP client

Build the server from the repository root with Rust 1.98.1 or newer:

```powershell
cargo build -p bevy_brp_mcp --locked
```

Configure the MCP client to launch `target/debug/bevy_brp_mcp.exe` over stdio, using an absolute path to the built executable. The normal build provides App discovery, BRP, watch, input, screenshot, and App log tools. For server trace controls, explicitly build with `cargo build -p bevy_brp_mcp --locked --features mcp-debug` and launch that executable. The diagnostic build adds two trace tools; application log tools remain available in the normal build. See [MCP usage and result contracts](docs/mcp.md).

This checkout's production packages are version `0.3.0`, with a compatibility change to ordinary mouse input. Use the same full handoff commit for the runtime, Extras and MCP. The earlier `v0.2.2` tag does not contain Custom Pointer input. This change creates no release tag and does not publish crates to crates.io; a remote Git installation requires the handoff commit to be available on that remote.

Replace the placeholder below with the full 40-character commit from the completed handoff. Use the same `rev` for Extras if your App imports it directly:

```toml
[dependencies]
bevy_brp_runtime = { git = "https://github.com/slc90/bevy_brp.git", rev = "<FULL_POINTER_COMMIT>" }
bevy_brp_extras = { git = "https://github.com/slc90/bevy_brp.git", rev = "<FULL_POINTER_COMMIT>" }
```

Build or install MCP from that exact checkout. The following local installation avoids requiring publication; the client should launch the executable under the specified installation root:

```powershell
$pointerCommit = '<FULL_POINTER_COMMIT>'
git checkout --detach $pointerCommit
git rev-parse HEAD
cargo install --path crates/mcp --locked --root target/pointer-mcp-install
```

After the same commit is available remotely, the equivalent Git installation is `cargo install --git https://github.com/slc90/bevy_brp.git --rev $pointerCommit --locked bevy_brp_mcp`. Add `--features mcp-debug` only when selecting the diagnostic server. Never pair the new App libraries with the old MCP installation. Consumers such as Widgetry must update their runtime/Extras Git revision, MCP installation command, tool-version metadata and Cargo.lock together; this repository does not modify that consumer.

## Ordinary mouse compatibility in 0.3.0

Move, button, click, double-click, drag and scroll now follow `BRP → PointerInput → Bevy Picking → Pointer<T>` through one stable App-owned Custom Pointer. They do not inject raw mouse/cursor events or move the OS cursor. Raw `MouseMotion`, `MouseButtonInput`, `MouseWheel`, `WindowEvent` mouse variants and native cursor consumers no longer receive these ordinary BRP inputs. Hosts must provide enabled Picking and interaction plugins plus a suitable backend. Official Mouse-only `Hovered`/`DirectlyHovered` consumers require host adaptation; BRP does not change their writer or add Widgetry dependencies.

Only Left/Right/Middle are supported; Back/Forward are rejected. Coordinates are logical window pixels with independent per-window BRP history starting at the origin. An omitted window uses the last BRP target, then PrimaryWindow; explicit invalid or destroyed windows fail. Automatic gestures use a FIFO, real-time waits and Picking cycle barriers. A successful tool response means acceptance, `status.busy=false` means the input work has finished, and successful UI behavior must be checked separately. Always release the Pointer in automation cleanup and poll until inactive; see the [control requests and parameter boundaries](docs/mcp.md#ordinary-mouse-input-and-control) and [layered validation evidence](docs/testing.md).

Native Window title-bar move and edge resize are unsupported and must be avoided. Existing native observers can still receive Custom Pointer Press and request native operations; unchanged code is not a guarantee that these regions reject virtual input. Keyboard, IME, screenshots and trackpad gestures retain their separate input paths. The BRP desktop fixture has passed cursor isolation and ordinary UI checks; Widgetry hover/Tooltip/cancellation adaptation and its own UI acceptance remain consumer work.

## Develop and verify

The workspace has four production packages under `crates/` and three test-host packages under `tests/`. See [architecture](docs/architecture.md) for their roles and [agent rules](AGENTS.md) for engineering constraints. Run the workspace checks from the repository root:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
cargo build --workspace --locked
```

On Windows, use PowerShell 7. The real MCP-to-Bevy regression needs an interactive desktop and an available graphics device; its build, launch, cleanup, and feature-check instructions are in [testing](docs/testing.md). This checkout does not include a hosted CI workflow, so those commands are the repeatable local validation entry.

The repository and its derived runtime code retain their license and attribution files. See [runtime upstream notes](crates/runtime/UPSTREAM.md) for the transport's source and local differences.
