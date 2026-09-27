# bevy_brp development workspace

<table>
<tr>
<td><b>bevy_brp_mcp</b></td>
<td>
<a href="https://github.com/slc90/bevy_brp#license"><img src="https://img.shields.io/badge/license-MIT%2FApache-blue.svg" alt="License"></a>
</td>
</tr>
<tr>
<td><b>bevy_brp_extras</b></td>
<td>
<a href="https://github.com/slc90/bevy_brp#license"><img src="https://img.shields.io/badge/license-MIT%2FApache-blue.svg" alt="License"></a>
</td>
</tr>
<tr>
<td><b>bevy_brp_runtime</b></td>
<td>
<a href="https://github.com/slc90/bevy_brp#license"><img src="https://img.shields.io/badge/license-MIT%2FApache-blue.svg" alt="License"></a>
</td>
</tr>
</table>

This is the development workspace for Bevy Remote Protocol (BRP) tools. The crates are distributed from this repository by Git tag and are not published to crates.io.

## Development on Windows

The supported development target is `x86_64-pc-windows-msvc`. The workspace requires Rust 1.98.1 or newer through `rust-version`; install `rustfmt` and `clippy` for your active toolchain. Use PowerShell 7 (`pwsh`) from the repository root. The workspace uses the checked-in `Cargo.lock`.

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
cargo build --workspace --locked
```

For independent feature checks on Windows:

```powershell
cargo check -p bevy_brp_extras --locked --no-default-features
cargo check -p bevy_brp_extras --locked --no-default-features --features diagnostics
cargo check -p bevy_brp_extras --locked --no-default-features --features ui
cargo check -p bevy_brp_mcp --locked --no-default-features
```

The Windows linker and rustdoc linker use `rust-lld.exe` through `.cargo/config.toml`.

## workspace structure
- **`extras/`** - optional Bevy plugin that adds extra BRP methods for enhanced functionality
- **`mcp/`** - Model Context Protocol (mcp) server for AI coding assistants to control Bevy apps
- **`mcp_macros/`** - boilerplate-reducing macros
- **`runtime/`** - wake-aware BRP HTTP transport and progress controller for Bevy apps
- **`test-*/`** - examples and applications for testing and development

## links
- [`bevy_brp_mcp`](mcp/)
- [`bevy_brp_extras`](extras/)
- [`bevy_brp_runtime`](runtime/)
