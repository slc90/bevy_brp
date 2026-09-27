# 当前 workspace 架构

根 `Cargo.toml` 是 virtual workspace，统一管理依赖版本与 lint；根 `Cargo.lock` 由 workspace 共用。生产 package 位于 `crates/`，测试宿主 package 位于 `tests/`。目录名不改变 Cargo package 名或 target 名。

| 路径 | Package 与入口 | 职责 |
| --- | --- | --- |
| `crates/mcp/` | `bevy_brp_mcp`，binary `bevy_brp_mcp` | Agent 侧 stdio MCP server；内部模块完成工具注册、BRP HTTP 调用、App 发现和进程管理。`help_text/` 承载工具说明。 |
| `crates/mcp_macros/` | `bevy_brp_mcp_macros`，proc-macro library | MCP server 使用的工具描述、注册、参数和结果 derive。 |
| `crates/extras/` | `bevy_brp_extras`，library | Bevy App 的 BRP 扩展方法、输入、截图与插件配置。可单独安装 `BrpExtrasPlugin`。 |
| `crates/runtime/` | `bevy_brp_runtime`，library | 将 extras 方法与 wake-aware HTTP transport 组合，提供 `BrpRuntimePlugin`。上游派生说明见 [`UPSTREAM.md`](../crates/runtime/UPSTREAM.md)。 |

生产 crate 的内部依赖只有 `bevy_brp_mcp → bevy_brp_mcp_macros` 与 `bevy_brp_runtime → bevy_brp_extras`。MCP server 与 Bevy App 之间通过 BRP 协议通信，没有互相引用的 Cargo dependency。

对外 Rust 入口由 `crates/extras/src/lib.rs` 的 re-export 和 `crates/runtime/src/lib.rs` 的 `BrpRuntimePlugin` 提供。MCP crate 是 binary，其工具名称、schema、help text 与 BRP 调用构成协议入口；binary 内模块不构成可依赖的 Rust library API。各 crate 的 feature 和具体依赖以其 `Cargo.toml` 为准。

| 路径 | Package 与 Cargo target | 验证用途 |
| --- | --- | --- |
| `tests/test-app/` | `bevy_brp_test_apps`；bin `test_app`，examples `extras_plugin`、`no_extras_plugin`、`event_test`、`mouse_test`、`test_app` | 真实 Bevy App、runtime、输入、事件、截图和 stock BRP 宿主。 |
| `tests/test-duplicate-a/` | `test-app-a`；examples `extras_plugin_duplicate`、`test_app` | 与另一个 package 的同名 example、与 `test-app` 的同名 target，用于发现、路径和搜索顺序验证。 |
| `tests/test-duplicate-b/` | `test-app-b`；example `extras_plugin_duplicate` | 同名 example 的第二个宿主。 |
| `crates/extras/tests/` | `bevy_brp_extras` 的 integration test target | 从 crate 外部验证 agent tool 注册 API。 |

`tests/` 只是多个 workspace member 的容器，不是根 package 的 Cargo test harness。各宿主的 bin/example 需要通过对应 package 的 `cargo run -p …` 或 MCP 启动；`cargo test --workspace` 本身不会运行完整的 MCP→BRP 交互。

从仓库根目录使用 MCP 的 `path` 参数时，目录应使用当前实际路径，例如 `tests/test-app` 或 `crates/extras`。`brp_list_bevy` 返回的 `relative_path` 也反映这些物理目录；跨 package 的同名 target 仍可用 `package_name` 消歧。直接使用本仓库旧 manifest 路径的本地 `path` dependency 需要改为对应的 `crates/<name>` 路径；按 package 名消费的 Git dependency 和 `bevy_brp_mcp` binary 名称不变。
