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

The checked-in source is the current development entry. Existing Git tag `v0.1.0` points to an earlier revision; consumers pinned to that tag do not receive these changes until their dependency reference is updated separately. Crates are not published to crates.io.

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
